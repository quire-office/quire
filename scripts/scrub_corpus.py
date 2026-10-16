#!/usr/bin/env python3
"""開源公開發布前的 corpus 個資消毒：重寫 docProps 的 creator/lastModifiedBy/company。

設計（PM 決定，dogfooding）：不走 zip 字節級打補丁，而是把每個 .docx 走
quire-ooxml 的 set_part() 無損替換 docProps/core.xml 與 docProps/app.xml
（未觸碰 entry 位元級原樣保留＝引擎自身 round-trip 閘門為本腳本背書），
再把 manifest 的 sha256/bytes 重算。

用法：python3 scripts/scrub_corpus.py            # 就地消毒
      python3 scripts/scrub_corpus.py --check     # 只掃描回報，不改檔
"""
import json
import re
import subprocess
import sys
import zipfile
import pathlib
import hashlib
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
CORPUS = ROOT / "corpus"
MANIFEST = CORPUS / "manifest.json"
REPL = {
    "creator": "Quire Corpus",
    "lastModifiedBy": "Quire Corpus",
    "company": "Quire",
    "Manager": "",
    "title": None,  # None = 保留原值
}
# 成對元素（開→閉）或 self-closing 一次吃掉，避免半截替換產生標籤錯置
# 文件本體殘留的聯絡 email（上游公開附件裡的真實業務/個人信箱）一律遮罩。
# 只動 XML 文字 part；二進位 part（內嵌字型 odttf 的廠商授權信箱）不碰——
# 改字節會破壞字型，且那是軟體供應商聯絡方式而非個資。
EMAIL_RE = re.compile(rb"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")
XML_PART_RE = re.compile(r"\.(xml|rels)$", re.I)
REDACTED = b"redacted@example.invalid"


def redact_emails(data: bytes) -> bytes:
    return EMAIL_RE.sub(lambda m: REDACTED, data)


TAG_RE = re.compile(
    rb"<([a-zA-Z-]+:)?(creator|lastModifiedBy|company|Manager)"
    rb"((?:\s[^<>]*)?)(?:\s*/>|>(?:(?!</)[\s\S])*?</\1?\2>)",
    re.I,
)


def scrub_xml(data: bytes) -> bytes:
    def sub(m):
        ns, tag, attrs = (m.group(1) or b"").decode(), m.group(2).decode(), m.group(3)
        if attrs:
            return m.group(0)  # 帶屬性的元素不在 docProps 生成慣例內：保守跳過
        key = {"creator": "creator", "lastmodifiedby": "lastModifiedBy",
               "company": "company", "manager": "Manager"}.get(tag.lower())
        want = REPL.get(key) if key else None
        if want is None:
            return m.group(0)
        return f"<{ns}{tag}>".encode() + want.encode() + f"</{ns}{tag}>".encode()
    return TAG_RE.sub(sub, data)


def main() -> int:
    check = "--check" in sys.argv
    binp = ROOT / "target" / "debug" / "scrub"
    manifest = json.loads(MANIFEST.read_text())
    changed = []
    for entry in manifest["files"]:
        p = CORPUS / entry["file"]
        with zipfile.ZipFile(p) as z:
            names = z.namelist()
            core = z.read("docProps/core.xml") if "docProps/core.xml" in names else None
            app = z.read("docProps/app.xml") if "docProps/app.xml" in names else None
            xml_redacted = {}
            for n in names:
                if not XML_PART_RE.search(n):
                    continue
                raw = z.read(n)
                new = redact_emails(raw)
                if new != raw:  # 已是遮罩值者不重複計入，腳本可冪等重跑
                    xml_redacted[n] = new
        dirty = (
            bool(core and TAG_RE.search(core))
            or bool(app and TAG_RE.search(app))
            or bool(xml_redacted)
        )
        if not dirty:
            continue
        if check:
            print(f"待消毒: {entry['file']}")
            changed.append(entry["file"])
            continue
        # 無損替換走 Rust 引擎（zip_struct 保留未觸碰 entry 位元級原樣）
        with tempfile.TemporaryDirectory() as td:
            out = pathlib.Path(td) / "out.docx"
            # 以 stdin JSON 餵入（路徑, 新 core, 新 app）
            payload = {
                "in": str(p),
                "out": str(out),
                "parts": {},
            }
            if core is not None:
                payload["parts"]["docProps/core.xml"] = scrub_xml(core).hex()
            if app is not None:
                payload["parts"]["docProps/app.xml"] = scrub_xml(app).hex()
            for n, new in xml_redacted.items():
                payload["parts"][n] = new.hex()
            subprocess.run([str(binp)], input=json.dumps(payload).encode(), check=True)
            data = out.read_bytes()
        p.write_bytes(data)
        entry["sha256"] = hashlib.sha256(data).hexdigest()
        entry["bytes"] = len(data)
        entry["scrubbed"] = True
        changed.append(entry["file"])
    if not check:
        MANIFEST.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    mode = "掃描" if check else "消毒"
    print(f"{mode}完成：{len(changed)} 檔處理（總 {len(manifest['files'])}）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
