#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""ODF-E2d 獨立第二雙眼基準生成器：styles.xml `office:master-styles` 頁首／頁尾
宣告面逐檔逐槽逐段（文字＋段級字級）→ `fixtures/e2d_hf_truth.json`。

實作依 ODF 1.3 Part 1 §15.1（master-page）、§17.2（header/footer 槽）、§19.501
（段落宣告主頁 ⇒ 換頁）與本卡定案的發射口徑（`E2D-DESIGN.md`），與 Rust
`core/odf/src/header_footer.rs` **各自獨立**寫成——兩邊同規則才算閉合。

規則（與卡面／設計檔同）：
  * 只掃 `styles.xml` 的 `office:master-styles`（全語料 `content.xml` 宣告
    `office:master-styles` 實測 0 檔）。
  * 六槽：header / header-first / header-left / footer / footer-first / footer-left，
    依**文件序**入列（不是固定 sort 序）。
  * 段＝槽內所有 `text:p`／`text:h`，**含巢裝表格／文字框／自訂圖形內段落**
    （LO 實測渲染），依**文件序**（container-before-inner）。
  * 段文字＝該段自身 inline 文字（**不得**把巢裝段落的文字算進容器段——
    tdf102466 實測 2× 差的根因），加 `text:s`（`@text:c` 展開）／`text:tab`→\\t／
    `text:line-break`→\\n，加**欄位節點快取字面值**（`text:page-number` 等節點內
    文字照收；LO 重算屬 README §11.1 合成軸）。
  * `svg:desc`／`svg:title`（圖形替代文字）整棵子樹**零發射**（LO 不渲染；
    D3 同律——fdo78420 頁首 alt-text 實測零命中）。
  * 段級字級級聯：styles.xml `office:automatic-styles`（HF 樣式 MP1/MP3 在此層）
    → `office:styles` 具名 parent 鏈 → default-style(paragraph) →
    default-style(text) → 12.0pt。
"""
import json
import zipfile
import xml.etree.ElementTree as ET
from pathlib import Path

STYLES = "urn:oasis:names:tc:opendocument:xmlns:style:1.0"
TEXT = "urn:oasis:names:tc:opendocument:xmlns:text:1.0"
SVG = "urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0"
FO = "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0"
OFFICE = "urn:oasis:names:tc:opendocument:xmlns:office:1.0"

SLOT_TAGS = {
    f"{{{STYLES}}}header": "header",
    f"{{{STYLES}}}header-first": "header-first",
    f"{{{STYLES}}}header-left": "header-left",
    f"{{{STYLES}}}footer": "footer",
    f"{{{STYLES}}}footer-first": "footer-first",
    f"{{{STYLES}}}footer-left": "footer-left",
}
PARA_TAGS = {f"{{{TEXT}}}p", f"{{{TEXT}}}h"}
ALT_TAGS = {f"{{{SVG}}}desc", f"{{{SVG}}}title"}
# 欄位節點：取節點內快取字面值（不外推）。
FIELD_TAGS = {
    f"{{{TEXT}}}{t}"
    for t in (
        "page-number", "page-count", "page", "pages", "paragraph-number",
        "paragraph-count", "sentence-number", "chapter", "time", "date",
        "variable", "expression", "user-field-declare", "user-field-get",
        "initial-content", "conditional-text", "script", "sender-field",
        "table-row-number", "table-column-number", "frame-number",
        "line-number", "note-number", "note-citation", "annotation-count",
    )
}
DEFAULT_SIZE = 12.0


def local(tag):
    return tag.rsplit("}", 1)[-1] if "}" in tag else tag


def q(ns, name):
    return f"{{{ns}}}{name}"


def attr(elem, ns, name):
    return elem.get(q(ns, name))


def size_to_pt(raw):
    """`fo:font-size` 值 → pt（宣告了但讀不出＝None 交由上層落 default）。"""
    if raw is None:
        return None
    s = raw.strip()
    if not s:
        return None
    low = s.lower()
    try:
        if low.endswith("pt"):
            return float(low[:-2])
        if low.endswith("cm"):
            return float(low[:-2]) * 72 / 2.54
        if low.endswith("mm"):
            return float(low[:-2]) * 72 / 25.4
        if low.endswith("in"):
            return float(low[:-2]) * 72
        if low.endswith("pc"):
            return float(low[:-2]) * 12
        if low.endswith("%"):
            return None  # 相對值：非本軸，視為未宣告
        return float(low)
    except ValueError:
        return None


class StyleRec:
    def __init__(self):
        self.family = None
        self.parent = None
        self.size_pt = None
        self.bold = None


class Book:
    """字級級聯簿：automatic（styles.xml 的 automatic-styles）→ named
    （office:styles 具名 parent 鏈）→ default-style(paragraph) →
    default-style(text) → 12pt。"""

    def __init__(self):
        self.automatic = {}
        self.named = {}
        self.default_paragraph = None
        self.default_text = None

    def resolve(self, name):
        size, bold = self._chain(name)
        if size is None:
            d = self.default_paragraph or self.default_text
            if d is not None and d.size_pt is not None:
                size = d.size_pt
        if bold is None:
            d = self.default_paragraph or self.default_text
            if d is not None and d.bold is not None:
                bold = d.bold
        return (size if size is not None else DEFAULT_SIZE, bool(bold))

    def _chain(self, name):
        size = bold = None
        cur = name or None
        seen = set()
        while cur and cur not in seen:
            seen.add(cur)
            rec = self.automatic.get(cur) or self.named.get(cur)
            if rec is None:
                break
            if size is None and rec.size_pt is not None:
                size = rec.size_pt
            if bold is None and rec.bold is not None:
                bold = rec.bold
            cur = rec.parent
        return size, bold


def build_book(styles_root):
    book = Book()
    office_styles = styles_root.find(q(OFFICE, "styles"))
    automatic = styles_root.find(q(OFFICE, "automatic-styles"))
    masters = styles_root.find(q(OFFICE, "master-styles"))

    containers = [c for c in (office_styles, automatic, masters) if c is not None]
    for cont in containers:
        scope_is_automatic = cont is automatic or cont is masters
        for st in cont.iter(q(STYLES, "style")):
            name = attr(st, STYLES, "name")
            if not name:
                continue
            rec = StyleRec()
            rec.family = attr(st, STYLES, "family")
            rec.parent = attr(st, STYLES, "parent-style-name")
            for tp in st.iter(q(STYLES, "text-properties")):
                raw = tp.get(q(FO, "font-size")) or tp.get("fo:font-size")
                pt = size_to_pt(raw)
                if pt is not None:
                    rec.size_pt = pt
                w = tp.get(q(FO, "font-weight")) or tp.get("fo:font-weight")
                if w is not None:
                    rec.bold = (w.strip().lower() == "bold") or (
                        w.strip().isdigit() and int(w.strip()) >= 700
                    )
            (book.automatic if scope_is_automatic else book.named)[name] = rec

    for cont in containers:
        for ds in cont.iter(q(STYLES, "default-style")):
            fam = attr(ds, STYLES, "family")
            rec = StyleRec()
            rec.family = fam
            for tp in ds.iter(q(STYLES, "text-properties")):
                raw = tp.get(q(FO, "font-size")) or tp.get("fo:font-size")
                pt = size_to_pt(raw)
                if pt is not None:
                    rec.size_pt = pt
                w = tp.get(q(FO, "font-weight")) or tp.get("fo:font-weight")
                if w is not None:
                    rec.bold = (w.strip().lower() == "bold") or (
                        w.strip().isdigit() and int(w.strip()) >= 700
                    )
            if fam == "paragraph" and book.default_paragraph is None:
                book.default_paragraph = rec
            elif fam == "text" and book.default_text is None:
                book.default_text = rec
    return book


def paragraph_text(elem):
    """該段**自身** inline 文字：巢裝段落與 alt-text 子樹整棵略過（另筆／零發射），
    `text:s`／`text:tab`／`text:line-break` 展開，欄位節點取節點內快取字面值。"""
    out = []

    def emit_text(s):
        if s:
            out.append(s)

    def walk(node):
        emit_text(node.text)
        for child in node:
            ctag = child.tag
            if ctag in PARA_TAGS:
                # 巢裝段落：另筆入列（由槽層枚舉），此处整棵略過。
                emit_text(child.tail)
                continue
            if ctag in ALT_TAGS:
                emit_text(child.tail)
                continue
            if ctag == q(TEXT, "s"):
                n = attr(child, TEXT, "c")
                try:
                    out.append(" " * (int(n) if n else 1))
                except ValueError:
                    out.append(" ")
            elif ctag == q(TEXT, "tab"):
                out.append("\t")
            elif ctag == q(TEXT, "line-break"):
                out.append("\n")
            elif ctag in FIELD_TAGS:
                # 欄位：節點內快取字面值照收（含巢裝 text:s／text:span）。
                walk_field(child)
            else:
                walk(child)
            emit_text(child.tail)

    def walk_field(node):
        emit_text(node.text)
        for child in node:
            if child.tag in ALT_TAGS or child.tag in PARA_TAGS:
                emit_text(child.tail)
                continue
            if child.tag == q(TEXT, "s"):
                n = attr(child, TEXT, "c")
                try:
                    out.append(" " * (int(n) if n else 1))
                except ValueError:
                    out.append(" ")
            elif child.tag == q(TEXT, "tab"):
                out.append("\t")
            elif child.tag == q(TEXT, "line-break"):
                out.append("\n")
            else:
                walk_field(child)
            emit_text(child.tail)

    walk(elem)
    return "".join(out)


def collect(styles_bytes):
    book = build_book(ET.fromstring(styles_bytes))
    root = ET.fromstring(styles_bytes)
    masters_cont = root.find(q(OFFICE, "master-styles"))
    out = []
    if masters_cont is None:
        return out
    for mp in masters_cont.findall(q(STYLES, "master-page")):
        slots = []
        for child in mp:
            kind = SLOT_TAGS.get(child.tag)
            if kind is None:
                continue
            paras = []
            for node in child.iter():
                if node.tag not in PARA_TAGS:
                    continue
                style = attr(node, TEXT, "style-name")
                size, _bold = book.resolve(style)
                paras.append({"text": paragraph_text(node), "size_pt": round(size, 4)})
            slots.append({"kind": kind, "paragraphs": paras})
        out.append({"name": attr(mp, STYLES, "name") or "", "slots": slots})
    return out


def main():
    # core/odf/tests/odf_e2d_hf_truth.py → repo root（parents[3]）。
    repo = Path(__file__).resolve().parents[3]
    manifest = json.loads((repo / "corpus-odt/manifest.json").read_text(encoding="utf-8"))
    census = {}
    n_master = n_slot = n_para = 0
    for rec in manifest["files"]:
        rel = rec["file"]
        p = repo / "corpus-odt" / rel
        if not p.exists():
            continue
        with zipfile.ZipFile(p) as zf:
            names = set(zf.namelist())
            if "styles.xml" not in names:
                census[rel] = {"masters": []}
                continue
            try:
                masters = collect(zf.read("styles.xml"))
            except ET.ParseError as e:
                # 加密／非 XML 的 styles.xml：ODF-E1 閘門口徑＝宣告面不可讀，
                # 基準記零主頁（Rust 端 project() 對該檔直接報錯，不入考卷迴圈）。
                print(f"  styles.xml 非合法 XML（加密檔常態）: {rel}: {e}")
                census[rel] = {"masters": []}
                continue
        census[rel] = {"masters": masters}
        for m in masters:
            n_master += 1
            n_slot += len(m["slots"])
            n_para += sum(len(s["paragraphs"]) for s in m["slots"])
    key = "corpus-odt/lo_out/lo__ooxmlexport__strict.odt"
    for m in census.get(key, {}).get("masters", []):
        print("strict master:", m["name"], [s["kind"] for s in m["slots"]])
        print(json.dumps(census[key], ensure_ascii=False)[:320])
    out = repo / "core/odf/tests/fixtures/e2d_hf_truth.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(
        json.dumps(census, ensure_ascii=False, indent=1, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(f"\nwrote {out}  files={len(census)} masters={n_master} slots={n_slot} paras={n_para}")


if __name__ == "__main__":
    main()
