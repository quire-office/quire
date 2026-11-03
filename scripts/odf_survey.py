#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""ODF-C1：corpus-odt 50 檔容器級普查（report-only，零引擎代碼）。

讀 corpus-odt/manifest.json，逐檔產出一筆 record，寫 docs/odf-survey/survey.json。
輸出為冪等：相同語料與相同 manifest 順序會得到逐位元組相同的 JSON。

普查面（卡面必蓋）：
  1. 容器基本：path、size、zip entry 清單與大小；mimetype 是否為首 entry 且不壓縮
     （ODF 1.3 §3.3），違例者標記；有無 styles.xml / meta.xml / settings.xml /
     META-INF/manifest.xml。
  2. content.xml 統計：text:p、table:table、draw:frame、text:outline（目錄）、
     text:list、soft page break（fo:break-before="page"）、text:notes（註解/腳註錨）。
  3. styles.xml 統計：default-style 與 named style 計數、style:page-layout 有無。
  4. 病態標記：mimetype 違例、缺 manifest.xml、缺 styles.xml、external data 引用
     （xlink:href 指向檔外）、加密 entry（manifest:encryption-data）。
  5. 匯總＋ODF-2 必蓋 case 清單（每項附檔名證據）。

先紅自證（卡面紅燈 3）：`--self-test` 把三個合成違例 fixture（fixtures/）當成語料掃，
斷言 mimetype 規則確實報錯；未先紅即回傳非零。

用法：python3 scripts/odf_survey.py [--corpus DIR] [--out FILE] [--self-test]
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import sys
import zipfile
from collections import Counter

SCHEMA = "quire-odf-survey/1"

# --- ODF 命名空間（統計一律以「local-name＋命名空間前綴」雙軌核對，避免前綴改名誤判）
RE_TEXT_P = re.compile(r"<text:p[\s>/]")
RE_TEXT_H = re.compile(r"<text:h[\s>/]")
RE_TABLE_TABLE = re.compile(r"<table:table[\s>/]")
RE_TABLE_ROW = re.compile(r"<table:table-row[\s>/]")
RE_TABLE_CELL = re.compile(r"<table:table-cell[\s>/]")
RE_COVERED_CELL_SINGULAR = re.compile(r"<table:covered-table-cell[\s>/]")
RE_COVERED_CELL = re.compile(r"<table:covered-table-cells[\s>/]")
RE_COLS_SPANNED = re.compile(r'table:number-columns-spanned="(?!1")([1-9]\d*)"')
RE_ROWS_SPANNED = re.compile(r'table:number-rows-spanned="(?!1")([1-9]\d*)"')
RE_DRAW_FRAME = re.compile(r"<draw:frame[\s>/]")
RE_DRAW_TEXT_BOX = re.compile(r"<draw:text-box[\s>/]")
RE_TEXT_OUTLINE = re.compile(r"<text:outline[\s>/]")  # ODF 無此元素（見報告病態考證），恆 0
RE_TOC = re.compile(r"<text:table-of-content[\s>/]")
RE_TOC_SOURCE = re.compile(r"<text:table-of-content-source[\s>/]")
RE_TOC_ENTRY_TEMPLATE = re.compile(r"<text:table-of-content-entry-template[\s>/]")
RE_TEXT_LIST = re.compile(r"<text:list[\s>/]")
RE_TEXT_NOTE = re.compile(r"<text:note[\s>/]")
RE_TEXT_NOTE_BODY = re.compile(r"<text:note-body[\s>/]")
RE_BREAK_BEFORE_PAGE = re.compile(r'fo:break-before="page"')
RE_BREAK_AFTER_PAGE = re.compile(r'fo:break-after="page"')
RE_TEXT_A = re.compile(r"<text:a[\s>/]")
RE_DRAW_IMAGE = re.compile(r"<draw:image[\s>/]")
RE_TEXT_SPAN = re.compile(r"<text:span[\s>/]")
RE_STYLE_DEFAULT = re.compile(r"<style:default-style[\s>/]")
RE_STYLE_STYLE = re.compile(r"<style:style[\s>/]")
RE_STYLE_LIST_STYLE = re.compile(r"<style:list-style[\s>/]")
RE_STYLE_PAGE_LAYOUT = re.compile(r"<style:page-layout[\s>/]")
RE_STYLE_MASTER_PAGE = re.compile(r"<style:master-page[\s>/]")
RE_STYLE_FONT_FACE = re.compile(r"<style:font-face[\s>/]")
RE_STYLE_FONT_DECL = re.compile(r"<style:font-face-decl[\s>/]")
RE_STYLE_HEADER = re.compile(r"<style:header[\s>/]")
RE_STYLE_FOOTER = re.compile(r"<style:footer[\s>/]")
RE_STYLE_PAGE_BREAK = re.compile(r"<style:page-break[\s>/]")
RE_MANIFEST_FILE_ENTRY = re.compile(r"<manifest:file-entry[\s>/]")
RE_MANIFEST_ENCRYPTION = re.compile(r"<manifest:encryption-data[\s>/]")
RE_XLINK_HREF = re.compile(r'xlink:href="([^"]*)"')
# 同一起始標籤內的（標籤名, href）：用於區分「網頁超鏈」（text:a，正常）與
# 「外部資料引用」（draw:image / draw:frame / draw:object / office:binary-data，病態）。
RE_HREF_IN_TAG = re.compile(r'<([\w:-]+)\b[^>]*?xlink:href="([^"]*)"', re.S)
RESOURCE_TAGS = {"draw:image", "draw:frame", "draw:object", "draw:plugin",
                 "draw:embedded", "draw:sound", "draw:video", "draw:audio",
                 "office:binary-data", "svg:image", "text:list-box",
                 "form:image", "form:file", "office:annotation"}
HYPERLINK_TAGS = {"text:a"}
RE_OFFICE_VERSION = re.compile(r'office:version="([^"]*)"')
RE_MANIFEST_VERSION = re.compile(r'manifest:version="([^"]*)"')
RE_MEDIA_TYPE = re.compile(r'manifest:media-type="([^"]*)"')
RE_ENCRYPT_ALGO = re.compile(r'manifest:algorithm-name="([^"]*)"')
RE_KEY_DERIV = re.compile(r'manifest:key-derivation-name="([^"]*)"')
RE_CHECKSUM_TYPE = re.compile(r'manifest:checksum-type="([^"]*)"')
RE_GEN = re.compile(r"<meta:generator>([^<]*)</meta:generator>")
RE_CREATOR = re.compile(r"<dc:creator>([^<]*)</dc:creator>")
RE_INITIAL_CREATOR = re.compile(r"<meta:initial-creator>([^<]*)</meta:initial-creator>")
RE_TITLE = re.compile(r"<dc:title>([^<]*)</dc:title>")
RE_LANGUAGE = re.compile(r'dc:language="([^"]*)"')
PII_RES = {
    "win_users": re.compile(rb"([Cc]:[\\/]{1,2}[Uu]sers[\\/]{1,2})[A-Za-z0-9._% -]{1,40}"),
    "home": re.compile(rb"(/home/)[a-z0-9._-]{1,32}"),
    "users_dir": re.compile(rb"(/Users/)[A-Za-z0-9._ -]{1,32}"),
    "unc": re.compile(rb"[\\\\]{2}wsl\.localhost[\\\\]{1,2}[A-Za-z0-9._-]{1,32}"),
}
EMAIL_RE = re.compile(rb"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")
# 消毒哨兵（scripts/odf_build_corpus.py 寫入的固定值）不該被當成殘留個資計數。
SCRUB_SENTINEL = b"redacted@example.invalid"
PII_SAFE_VALUES = {b"redacted"}

# 內部引用（合法）：檔內相對路徑、錨點
INTERNAL_PREFIXES = ("Pictures/", "Object", "Thumbnails/", "#", "./")


def sha256(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def decode(data: bytes) -> str:
    return data.decode("utf-8", errors="replace")


def classify_href(href: str) -> str | None:
    """回傳外部引用類型（無則 None）。ODF 的 xlink:href 若是檔內相對路徑屬正常。"""
    h = href.strip()
    if not h or h.startswith("#"):
        return None
    low = h.lower()
    if low.startswith(("http:", "https:", "ftp:", "mailto:", "file:", "jar:", "data:")):
        return "absolute_uri"
    if re.match(r"^[A-Za-z]:[\\/]", h) or h.startswith("\\\\"):
        return "windows_path"
    if h.startswith("/"):
        return "absolute_path"
    if h.startswith("../"):
        return "parent_escape"
    if h.startswith(INTERNAL_PREFIXES):
        return None
    # 其餘是檔內相對路徑：若 entry 不存在於 zip，也算外部/缺檔（由呼叫端再判）
    return None


def part_exists(names: set[str], target: str) -> bool:
    """ODF 內部引用常指向目錄（如 Pictures/）或帶 %xx 跳脫的檔名，兩者都要能核對。"""
    if target in names:
        return True
    if target.endswith("/") and any(n.startswith(target) for n in names):
        return True
    try:
        import urllib.parse as up
        un = up.unquote(target)
    except Exception:
        return False
    return un in names or (un.endswith("/") and any(n.startswith(un) for n in names))


def zip_entries(path: pathlib.Path) -> tuple[list[dict], list[str], dict]:
    """回傳（entry 清單, 檔名集合, 容器層病態）。首 entry 的 mimetype 規則是本卡第一塊考證。"""
    infos = []
    with zipfile.ZipFile(path) as zf:
        for i, info in enumerate(zf.infolist()):
            infos.append({
                "index": i,
                "name": info.filename,
                "compress_type": info.compress_type,
                "compressed": info.compress_type != zipfile.ZIP_STORED,
                "compress_size": info.compress_size,
                "file_size": info.file_size,
                "encrypted_flag": bool(info.flag_bits & 0x1),
            })
    names = [e["name"] for e in infos]
    patho: dict = {}
    first = infos[0] if infos else None
    mt = next((e for e in infos if e["name"] == "mimetype"), None)
    patho["mimetype_present"] = mt is not None
    patho["mimetype_first_entry"] = bool(first and first["name"] == "mimetype")
    patho["mimetype_stored"] = bool(mt and not mt["compressed"])
    patho["mimetype_ok"] = bool(mt and first and first["name"] == "mimetype" and not mt["compressed"])
    if mt:
        with zipfile.ZipFile(path) as zf:
            try:
                patho["mimetype_value"] = zf.read("mimetype").decode("utf-8", "ignore").strip()
            except Exception:
                patho["mimetype_value"] = None
    patho["mimetype_first_index"] = mt["index"] if mt else None
    patho["encrypted_entries"] = [e["name"] for e in infos if e["encrypted_flag"]]
    return infos, names, patho


def survey_content(zf: zipfile.ZipFile, names: set[str]) -> dict:
    out: dict = {"present": False}
    if "content.xml" not in names:
        out["flags"] = ["missing_content_xml"]
        return out
    data = zf.read("content.xml")
    text = decode(data)
    out.update({
        "present": True,
        "bytes": len(data),
        "text_p": len(RE_TEXT_P.findall(text)),
        "text_h": len(RE_TEXT_H.findall(text)),
        "text_span": len(RE_TEXT_SPAN.findall(text)),
        "table_table": len(RE_TABLE_TABLE.findall(text)),
        "table_row": len(RE_TABLE_ROW.findall(text)),
        "table_cell": len(RE_TABLE_CELL.findall(text)),
        "table_covered_cells": (len(RE_COVERED_CELL_SINGULAR.findall(text))
                                + len(RE_COVERED_CELL.findall(text))),
        # 合併格的另一半證據：LO 用 covered-table-cell 佔位，真正宣告跨距的是
        # table:number-columns-spanned / table:number-rows-spanned（>1 才算合併）。
        "table_cols_spanned_gt1": len(RE_COLS_SPANNED.findall(text)),
        "table_rows_spanned_gt1": len(RE_ROWS_SPANNED.findall(text)),
        "draw_frame": len(RE_DRAW_FRAME.findall(text)),
        "draw_text_box": len(RE_DRAW_TEXT_BOX.findall(text)),
        "draw_image": len(RE_DRAW_IMAGE.findall(text)),
        # ODF 無 text:outline 元素（DOCX/Word 的 TOC 在 LO 導出成 text:table-of-content；
        # text:outline 是 ODF 之外語境的詞，保留計數只為證明它在本批永遠是 0）。
        "text_outline_present": bool(RE_TEXT_OUTLINE.search(text)),
        "toc_present": bool(RE_TOC.search(text)),
        "toc_count": len(RE_TOC.findall(text)),
        "toc_source": len(RE_TOC_SOURCE.findall(text)),
        "toc_entry_templates": len(RE_TOC_ENTRY_TEMPLATE.findall(text)),
        "draw_text_box_present": bool(RE_DRAW_TEXT_BOX.search(text)),
        "text_list": len(RE_TEXT_LIST.findall(text)),
        "text_note": len(RE_TEXT_NOTE.findall(text)),
        "text_note_body": len(RE_TEXT_NOTE_BODY.findall(text)),
        "soft_page_break_fo": len(RE_BREAK_BEFORE_PAGE.findall(text)),
        "break_after_page_fo": len(RE_BREAK_AFTER_PAGE.findall(text)),
        "text_a": len(RE_TEXT_A.findall(text)),
        "office_version": (RE_OFFICE_VERSION.search(text) or [None, None])[1],
    })
    hrefs = RE_XLINK_HREF.findall(text)
    # 區分「網頁超鏈」（text:a 指向 http 屬正常語意）與「外部資料引用」（圖片/物件/
    # 二進位資料的 href 指向檔外＝容器病態：解析器拿不到內容）。
    resource_kinds: Counter = Counter()
    hyperlink_kinds: Counter = Counter()
    missing: list[str] = []
    for tag, h in RE_HREF_IN_TAG.findall(text):
        k = classify_href(h)
        if k is None and part_exists(names, h):
            continue                       # 檔內引用且 entry 存在：正常
        if k is None:
            k = "dangling_internal"        # 檔內路徑但 entry 不存在
        if tag in HYPERLINK_TAGS:
            hyperlink_kinds[k] += 1
        else:
            resource_kinds[k] += 1
            missing.append(h)
    # 沒有起始標籤可歸屬的 href（styles.xml 等其他 part 用同一函數時）
    tagged = {id_ for id_ in RE_HREF_IN_TAG.finditer(text)}
    bare = len(hrefs) - len(tagged)
    out["xlink_href_total"] = len(hrefs)
    out["xlink_href_unattributed"] = bare
    out["external_data_ref_kinds"] = dict(sorted(resource_kinds.items()))
    out["external_data_ref_count"] = sum(resource_kinds.values())
    out["external_data_ref_samples"] = sorted(set(missing))[:8]
    out["hyperlink_ref_kinds"] = dict(sorted(hyperlink_kinds.items()))
    out["hyperlink_ref_count"] = sum(hyperlink_kinds.values())
    # 相容舊欄位名（report 只讀 external_data_ref_*）
    out["external_href_kinds"] = out["external_data_ref_kinds"]
    out["external_href_count"] = out["external_data_ref_count"]
    out["external_href_samples"] = out["external_data_ref_samples"]
    return out


def survey_part(zf: zipfile.ZipFile, names: set[str], part: str) -> dict:
    out: dict = {"present": part in names}
    if part not in names:
        return out
    text = decode(zf.read(part))
    out["bytes"] = len(zf.read(part))
    out["default_styles"] = len(RE_STYLE_DEFAULT.findall(text))
    out["named_styles"] = len(RE_STYLE_STYLE.findall(text))
    out["list_styles"] = len(RE_STYLE_LIST_STYLE.findall(text))
    out["page_layout_present"] = bool(RE_STYLE_PAGE_LAYOUT.search(text))
    out["master_pages"] = len(RE_STYLE_MASTER_PAGE.findall(text))
    out["font_faces"] = len(RE_STYLE_FONT_FACE.findall(text))
    out["font_face_decls"] = len(RE_STYLE_FONT_DECL.findall(text))
    out["headers"] = len(RE_STYLE_HEADER.findall(text))
    out["footers"] = len(RE_STYLE_FOOTER.findall(text))
    out["page_breaks"] = len(RE_STYLE_PAGE_BREAK.findall(text))
    out["soft_page_break_fo"] = len(RE_BREAK_BEFORE_PAGE.findall(text))
    return out


def survey_manifest(zf: zipfile.ZipFile, names: set[str]) -> dict:
    out: dict = {"present": "META-INF/manifest.xml" in names}
    if not out["present"]:
        return out
    text = decode(zf.read("META-INF/manifest.xml"))
    out["file_entries"] = len(RE_MANIFEST_FILE_ENTRY.findall(text))
    out["manifest_version"] = (RE_MANIFEST_VERSION.search(text) or [None, None])[1]
    enc = len(RE_MANIFEST_ENCRYPTION.findall(text))
    out["encryption_entries"] = enc
    out["encrypted"] = enc > 0
    out["algorithms"] = sorted(set(RE_ENCRYPT_ALGO.findall(text)))
    out["key_derivations"] = sorted(set(RE_KEY_DERIV.findall(text)))
    out["checksum_types"] = sorted(set(RE_CHECKSUM_TYPE.findall(text)))
    out["media_types"] = sorted(set(RE_MEDIA_TYPE.findall(text)))
    # manifest 宣告但 zip 沒有的 entry（反向缺檔）。根路徑 "/" 是容器自述，
    # 按 ODF 規定不是 zip entry，必須排除，否則每檔都會被誤報缺一。
    declared = [d for d in re.findall(r'manifest:full-path="([^"]*)"', text) if d != "/"]
    missing = [d for d in declared
               if d not in names and not (d.endswith("/") and
                                          any(n.startswith(d) for n in names))]
    out["declared_paths"] = len(declared)
    out["declared_but_absent"] = sorted(missing)[:8]
    out["declared_but_absent_count"] = len(missing)
    # 在 zip 但 manifest 未宣告（未引用串流）
    un = []
    for n in names:
        if n in ("mimetype", "META-INF/manifest.xml") or n.endswith("/"):
            continue
        base = n.split("/")[0] + "/" if "/" in n else n
        if n not in declared and base not in declared:
            un.append(n)
    out["present_but_undeclared"] = sorted(un)[:8]
    out["present_but_undeclared_count"] = len(un)
    return out


def survey_meta(zf: zipfile.ZipFile, names: set[str]) -> dict:
    out: dict = {"present": "meta.xml" in names}
    if not out["present"]:
        return out
    text = decode(zf.read("meta.xml"))
    out["generator"] = (RE_GEN.search(text) or [None, None])[1]
    out["creator"] = (RE_CREATOR.search(text) or [None, None])[1]
    out["initial_creator"] = (RE_INITIAL_CREATOR.search(text) or [None, None])[1]
    out["title"] = (RE_TITLE.search(text) or [None, None])[1]
    out["language"] = (RE_LANGUAGE.search(text) or [None, None])[1]
    out["editing_cycles"] = None
    m = re.search(r"<meta:editing-cycles>(\d+)<", text)
    if m:
        out["editing_cycles"] = int(m.group(1))
    return out


def scan_pii(path: pathlib.Path) -> dict:
    """殘留個資掃描：先剔除消毒哨兵再計數，避免把 redacted@example.invalid／
    <dc:creator>Quire Corpus</…> 這類「已消毒的固定值」誤報成病態。"""
    hits: dict[str, int] = {}
    try:
        with zipfile.ZipFile(path) as zf:
            for name in zf.namelist():
                if not name.endswith((".xml", ".rdf", ".rels")):
                    continue
                data = zf.read(name).replace(SCRUB_SENTINEL, b"")
                for label, rx in PII_RES.items():
                    # finditer 才能看到整段匹配（findall 只回 groups[1]，會漏判哨兵）
                    n = len([m for m in rx.finditer(data) if m.group(0) not in PII_SAFE_VALUES])
                    if n:
                        hits[label] = hits.get(label, 0) + n
                n = len(EMAIL_RE.findall(data))
                if n:
                    hits["email"] = hits.get("email", 0) + n
    except Exception:
        hits["unreadable"] = 1
    return hits


def survey_file(archive: pathlib.Path, entry: dict) -> dict:
    rec: dict = {
        "file": entry["file"],
        "absent": False,
        "source": entry.get("source"),
        "kind": entry.get("kind"),
        "license": entry.get("license"),
        "batch": entry.get("batch"),
        "tags_manifest": entry.get("tags", []),
    }
    if not archive.exists():
        rec.update({"absent": True, "size": None, "manifest_bytes": entry.get("bytes"),
                    "entries": [], "pathology": {"read_errors": ["實體檔不存在"]},
                    "flags": ["missing_file", "read_error"]})
        return rec

    raw = archive.read_bytes()
    rec["size"] = len(raw)
    rec["sha256"] = hashlib.sha256(raw).hexdigest()
    rec["sha256_matches_manifest"] = (entry.get("sha256") == rec["sha256"]
                                      if entry.get("sha256") else None)
    rec["bytes_match_manifest"] = (entry.get("bytes") == rec["size"]
                                   if entry.get("bytes") is not None else None)

    flags: list[str] = []
    try:
        infos, names, patho = zip_entries(archive)
    except zipfile.BadZipFile as exc:
        rec.update({"entries": [], "pathology": {"read_errors": [f"BadZipFile: {exc}"]},
                    "flags": ["bad_zip", "read_error"]})
        return rec

    rec["entries"] = infos
    rec["n_entries"] = len(infos)
    rec["uncompressed_total"] = sum(e["file_size"] for e in infos)
    rec["container"] = {
        "has_content_xml": "content.xml" in names,
        "has_styles_xml": "styles.xml" in names,
        "has_meta_xml": "meta.xml" in names,
        "has_settings_xml": "settings.xml" in names,
        "has_manifest_xml": "META-INF/manifest.xml" in names,
        "has_manifest_rdf": "manifest.rdf" in names,
        "has_thumbnails": any(n.startswith("Thumbnails/") for n in names),
        "has_configurations2": any(n.startswith("Configurations2/") for n in names),
        "has_pictures": any(n.startswith("Pictures/") for n in names),
        "has_objects": any(n.startswith("Object") for n in names),
    }
    patho["missing_manifest_xml"] = not rec["container"]["has_manifest_xml"]
    patho["missing_styles_xml"] = not rec["container"]["has_styles_xml"]
    patho["missing_content_xml"] = not rec["container"]["has_content_xml"]
    patho["missing_meta_xml"] = not rec["container"]["has_meta_xml"]

    with zipfile.ZipFile(archive) as zf:
        rec["content"] = survey_content(zf, set(names))
        rec["styles"] = survey_part(zf, set(names), "styles.xml")
        rec["styles_default_part"] = survey_part(zf, set(names), "styles.xml")
        rec["manifest"] = survey_manifest(zf, set(names))
        rec["meta"] = survey_meta(zf, set(names))
    rec["styles"]["present"] = "styles.xml" in names
    rec["pii_hits"] = scan_pii(archive)
    rec["pathology"] = patho

    # 病態旗標（先紅後綠的落點都在這裡）
    if not patho["mimetype_ok"]:
        if not patho["mimetype_present"]:
            flags.append("mimetype_missing")
        elif not patho["mimetype_first_entry"]:
            flags.append("mimetype_not_first")
        if patho["mimetype_present"] and not patho["mimetype_stored"]:
            flags.append("mimetype_compressed")
    for key, flag in (("missing_manifest_xml", "missing_manifest_xml"),
                      ("missing_styles_xml", "missing_styles_xml"),
                      ("missing_content_xml", "missing_content_xml"),
                      ("missing_meta_xml", "missing_meta_xml")):
        if patho.get(key):
            flags.append(flag)
    if patho["encrypted_entries"]:
        flags.append("zip_encrypted_entry_flag")
    if rec["manifest"].get("encrypted"):
        flags.append("manifest_encryption_data")
    if rec["content"].get("external_href_count"):
        flags.append("external_data_ref")
    if rec["manifest"].get("declared_but_absent_count"):
        flags.append("manifest_declares_absent_part")
    if rec["manifest"].get("present_but_undeclared_count"):
        flags.append("undeclared_stream")
    if rec["content"].get("soft_page_break_fo"):
        flags.append("soft_page_break")
    if rec["content"].get("text_outline_present"):
        flags.append("toc_outline")
    if rec["content"].get("table_covered_cells"):
        flags.append("merged_cells")
    if rec["content"].get("text_note"):
        flags.append("notes_anchored")
    if rec["content"].get("draw_image"):
        flags.append("raster_or_svg_image")
    if rec["content"].get("draw_text_box"):
        flags.append("text_box")
    if rec["pii_hits"]:
        flags.append("pii_residual")
    if rec["sha256_matches_manifest"] is False:
        flags.append("sha256_mismatch")
    if rec["bytes_match_manifest"] is False:
        flags.append("bytes_mismatch")
    rec["flags"] = sorted(set(flags))
    return rec


def summarize(records: list[dict], manifest_count: int) -> dict:
    def files_with(pred) -> list[str]:
        return [r["file"] for r in records if pred(r)]

    content_keys = ("text_p", "table_table", "draw_frame", "text_list", "text_note",
                    "soft_page_break_fo", "table_cell", "draw_image", "draw_text_box",
                    "text_a", "text_h")
    totals = {k: sum((r.get("content") or {}).get(k, 0) or 0 for r in records) for k in content_keys}
    s = {
        "manifest_count": manifest_count,
        "records": len(records),
        "absent_files": files_with(lambda r: r.get("absent")),
        "bytes_total": sum(r.get("size") or 0 for r in records),
        "entries_total": sum(r.get("n_entries", 0) for r in records),
        "content_totals": totals,
        "files_with_text_outline": files_with(lambda r: (r.get("content") or {}).get("text_outline_present")),
        "files_with_toc": files_with(lambda r: (r.get("content") or {}).get("toc_present")),
        "files_without_page_layout": files_with(
            lambda r: r.get("content") and not (r.get("styles") or {}).get("page_layout_present")),
        "files_with_text_list": files_with(lambda r: ((r.get("content") or {}).get("text_list") or 0) > 0),
        "files_with_notes": files_with(lambda r: ((r.get("content") or {}).get("text_note") or 0) > 0),
        "files_with_soft_page_break": files_with(lambda r: ((r.get("content") or {}).get("soft_page_break_fo") or 0) > 0),
        "files_with_table": files_with(lambda r: ((r.get("content") or {}).get("table_table") or 0) > 0),
        "files_with_merged_cells": files_with(lambda r: ((r.get("content") or {}).get("table_covered_cells") or 0) > 0),
        "files_with_draw_frame": files_with(lambda r: ((r.get("content") or {}).get("draw_frame") or 0) > 0),
        "files_with_text_box": files_with(lambda r: ((r.get("content") or {}).get("draw_text_box") or 0) > 0),
        "files_with_image": files_with(lambda r: ((r.get("content") or {}).get("draw_image") or 0) > 0),
        "files_with_page_layout": files_with(lambda r: (r.get("styles") or {}).get("page_layout_present")),
        "files_with_master_page": files_with(lambda r: ((r.get("styles") or {}).get("master_pages") or 0) > 0),
        "files_with_header_style": files_with(lambda r: ((r.get("styles") or {}).get("headers") or 0) > 0),
        "files_with_footer_style": files_with(lambda r: ((r.get("styles") or {}).get("footers") or 0) > 0),
        "files_without_styles_xml": files_with(lambda r: (r.get("pathology") or {}).get("missing_styles_xml")),
        "files_without_meta_xml": files_with(lambda r: (r.get("pathology") or {}).get("missing_meta_xml")),
        "files_without_settings_xml": files_with(
            lambda r: not (r.get("container") or {}).get("has_settings_xml")),
        "files_without_manifest_xml": files_with(
            lambda r: (r.get("pathology") or {}).get("missing_manifest_xml")),
        "mimetype_violation_files": files_with(
            lambda r: not (r.get("pathology") or {}).get("mimetype_ok", False)),
        "encrypted_files": files_with(lambda r: (r.get("manifest") or {}).get("encrypted")),
        "external_href_files": files_with(
            lambda r: ((r.get("content") or {}).get("external_href_count") or 0) > 0),
        "manifest_declares_absent_files": files_with(
            lambda r: (r.get("manifest") or {}).get("declared_but_absent_count")),
        "undeclared_stream_files": files_with(
            lambda r: (r.get("manifest") or {}).get("present_but_undeclared_count")),
        "pii_files": files_with(lambda r: bool(r.get("pii_hits"))),
        "sha_mismatch_files": files_with(lambda r: r.get("sha256_matches_manifest") is False),
        "manifest_versions": dict(Counter(
            (r.get("manifest") or {}).get("manifest_version") or "?" for r in records)),
        "office_versions": dict(Counter(
            (r.get("content") or {}).get("office_version") or "?" for r in records)),
        "media_types": dict(Counter(
            mt for r in records for mt in (r.get("manifest") or {}).get("media_types", []))),
        "generators": dict(Counter(
            (r.get("meta") or {}).get("generator") or "?" for r in records)),
        "encryption_algorithms": dict(Counter(
            a for r in records for a in (r.get("manifest") or {}).get("algorithms", []))),
    }
    s["pathology_files"] = [
        {"file": r["file"], "flags": [f for f in r.get("flags", [])
                                      if f not in ("soft_page_break", "toc_outline",
                                                   "merged_cells", "notes_anchored",
                                                   "raster_or_svg_image", "text_box")]}
        for r in records
        if any(f not in ("soft_page_break", "toc_outline", "merged_cells",
                         "notes_anchored", "raster_or_svg_image", "text_box")
               for f in r.get("flags", []))
    ]
    return s


# --------------------------------------------------------------- 自證（卡面紅燈 1／2／3）
REQUIRED_ENTRY_KEYS = ("file", "source", "source_url", "orig_path", "license", "acquisition",
                       "sha256", "bytes", "kind")


def manifest_completeness(corpus: pathlib.Path) -> list[str]:
    """紅燈 2：50 檔全有來源＋授權＋取得方式，缺任一即回報（由報告落紅燈）。"""
    problems: list[str] = []
    mp = corpus / "manifest.json"
    if not mp.exists():
        return [f"找不到 {mp}"]
    m = json.loads(mp.read_text(encoding="utf-8"))
    files = m.get("files", [])
    if m.get("count") != len(files):
        problems.append(f"manifest count={m.get('count')} 與 files 長度 {len(files)} 不符")
    if len(files) != 50:
        problems.append(f"正庫應為 50 檔，實得 {len(files)}")
    seen: set[str] = set()
    for e in files:
        name = e.get("file", "(無檔名)")
        if name in seen:
            problems.append(f"{name}: 條目重複")
        seen.add(name)
        for key in REQUIRED_ENTRY_KEYS:
            v = e.get(key)
            if v is None or (isinstance(v, str) and not v.strip()):
                problems.append(f"{name}: 缺 {key}")
        lic = (e.get("license") or "").strip()
        if lic.lower() in {"", "unknown", "mit", "apache-2.0", "bsd", "gpl", "lgpl", "n/a", "tbd"}:
            problems.append(f"{name}: 授權欄為裸字或空（{lic!r}）")
        path = corpus / name
        if not path.exists():
            problems.append(f"{name}: 實體檔不存在")
        elif e.get("sha256") and sha256(path) != e["sha256"]:
            problems.append(f"{name}: sha256 與實體不符")
        elif e.get("bytes") is not None and path.exists() and path.stat().st_size != e["bytes"]:
            problems.append(f"{name}: bytes 與實體不符")
    # quarantine 不得與正庫重疊
    qp = corpus / "quarantine.json"
    if qp.exists():
        q = json.loads(qp.read_text(encoding="utf-8")).get("files", {})
        for name in q:
            if name in seen:
                problems.append(f"{name}: 同時在正庫與 quarantine")
    return problems


def required_cases(records: list[dict], s: dict) -> list[dict]:
    """X1a 口徑的「ODF-2 必蓋 case 清單」：每項附 corpus-odt 實檔證據。

    這些是 core/odf crate（無損容器＋投影模型）第一期必須自證的考題，
    全部從本批 50 檔的實測結果反推，不是憑空列清單。
    """
    def with_font(rec_list: list[dict]) -> list[str]:
        out = []
        for r in rec_list:
            # media_type 只存在於 META-INF/manifest.xml 的宣告面（entries[] 是 zip 層，
            # 沒有 MIME 欄位）；字型判定必須查 manifest 宣告的 media_types。
            if any("font" in mt for mt in (r.get("manifest") or {}).get("media_types", [])):
                out.append(r["file"])
        return out

    cases = [
        {
            "id": "ODF2-01",
            "title": "mimetype 必須是首 entry 且 stored（ODF 1.3 §3.3）",
            "why": "3 檔 LO QA 真實檔違例（見 mimetype_violation_files）：讀取端若先假設首 entry，"
                   "對這三檔會直接誤判；寫出端若首 entry 被壓縮則違反規格。",
            "evidence": s["mimetype_violation_files"],
        },
        {
            "id": "ODF2-02",
            "title": "加密容器（manifest:encryption-data）只登記不解析",
            "why": "2 檔 Blowfish CFB＋PBKDF2 真加密（LO QA 鏡像 fdo76591 待遇）：容器層必須"
                   "能辨識、跳過、報明，不得當損壞檔，也不得在本期實作解密。",
            "evidence": s["encrypted_files"],
        },
        {
            "id": "ODF2-03",
            "title": "manifest 宣告 entry 但 zip 未帶（self-declared absent）",
            "why": "tdf152710.odt 宣告 content.xml 卻缺：無損容器必須保留「宣告≠實際」差異，"
                   "round-trip 後要原樣還原而非憑猜補齊。",
            "evidence": s["manifest_declares_absent_files"],
        },
        {
            "id": "ODF2-04",
            "title": "檔外資源引用（資源性 xlink:href 指向容器外）",
            "why": "12 檔有 draw:image / draw:frame 層級的外部引用：無損往返必須原封保留 href，"
                   "投影面則要能區分「引用不存在的外部資源」與純超鏈（text:a 另計）。",
            "evidence": s["external_href_files"],
        },
        {
            "id": "ODF2-05",
            "title": "合併格：covered-table-cell 單複數兩種寫法都要吃",
            "why": "17 檔有合併格。本批實測：LO 導出走單數 <table:covered-table-cell/>，"
                   "複數 <table:covered-table-cells/>（range 寫法）在本批 0 現但規格允許——"
                   "投影模型兩者都必須映射，否則覆蓋格會渲染成實體格。",
            "evidence": s["files_with_merged_cells"],
        },
        {
            "id": "ODF2-06",
            "title": "目錄是 text:table-of-content，不是 text:outline",
            "why": "本批 50 檔 text:outline 出現 0 次、text:table-of-content 5 檔：卡面原本的"
                   "text:outline 假設是錯的（ODF 無此元素），考題必須以真實標籤為準，"
                   "含 toc 的 text:table-of-content-source 與 entry-template 結構。",
            "evidence": s["files_with_toc"],
        },
        {
            "id": "ODF2-07",
            "title": "軟分頁 fo:break-before=\"page\"（本批未見 text:soft-page-break）",
            "why": "12 檔靠 fo:break-before 斷頁：LO 導出不用 text:soft-page-break 標籤，"
                   "投影模型的斷頁訊號來源必須以 styles 屬性為準。",
            "evidence": s["files_with_soft_page_break"],
        },
        {
            "id": "ODF2-08",
            "title": "腳註/尾註錨（text:note + text:note-body）",
            "why": "12 檔有註解錨：content.xml 的註解正文與 styles.xml 的 footnote/endnote "
                   "configuration 要成對還原，缺 sidecar 就丟註解。",
            "evidence": s["files_with_notes"],
        },
        {
            "id": "ODF2-09",
            "title": "頁首/頁尾（style:header / style:footer）",
            "why": "21 檔有頁首、19 檔有頁尾：頁首頁尾住在 styles.xml 的 master-page→page-layout "
                   "鏈上，投影器不能只讀 content.xml。",
            "evidence": s["files_with_header_style"],
        },
        {
            "id": "ODF2-10",
            "title": "嵌入物件 draw:frame 家族（chart / formula / OLE / PDF）",
            "why": "27 檔有 draw:frame，media_type 混雜 opendocument.chart×4、formula×2、"
                   "sun.star.oleobject×4、application/pdf×1：無損面要原樣保留 secondary manifest "
                   "宣告，投影面要決定降採策略。",
            "evidence": s["files_with_draw_frame"],
        },
        {
            "id": "ODF2-11",
            "title": "嵌入字型（application/x-font-ttf）",
            "why": "本批僅 1 檔帶 TTF（lo__selfgen__embedded_font_odttf.odt），但字型是版面還原的關鍵輸入：容器層必須"
                   "把字型 entry 當一等公民保留（含 manifest 宣告順序），不能歸入「其他媒體」。",
            "evidence": with_font(records),
        },
        {
            "id": "ODF2-12",
            "title": "文字框 draw:text-box（含 k18 撐列模型輸入）",
            "why": "4 檔有 draw:text-box：LO 文字框撐列（K18-cal 校準過的 posV+eff_h+clash+ε）"
                   "要能從 ODT 側座標重現，容器解析必須保留框的位置/尺寸屬性。",
            "evidence": s["files_with_text_box"],
        },
        {
            "id": "ODF2-13",
            "title": "消毒哨兵不得被個資掃描誤報",
            "why": "2 檔報 PII：FDO78292 的 svg:desc 殘留 C:\\Users\\redacted\\…（消毒後的哨兵路徑），"
                   "OASIS 規範檔帶 12 個公開 mailto（文件作者聯絡連結，非個資）。"
                   "掃描器要把「消毒結果＋公開出版聯絡」與真實個資分開，否則紅燈永遠關不掉。",
            "evidence": s["pii_files"],
        },
    ]
    return cases


def self_test(corpus: pathlib.Path) -> tuple[bool, list[str]]:
    """紅燈 3：普查的 mimetype 規則必須抓到合成違例 fixture（先紅證明）。"""
    lines: list[str] = []
    fixtures = corpus / "fixtures"
    cases = [
        ("mimetype_not_first.odt", "mimetype_not_first"),
        ("mimetype_compressed.odt", "mimetype_compressed"),
        ("missing_manifest_xml.odt", "missing_manifest_xml"),
    ]
    ok = True
    for fname, want_flag in cases:
        path = fixtures / fname
        if not path.exists():
            lines.append(f"FAIL fixture 不存在：{fname}（請先跑 scripts/odf_build_corpus.py）")
            ok = False
            continue
        rec = survey_file(path, {"file": f"fixtures/{fname}", "kind": "synthetic"})
        got = rec.get("flags", [])
        if want_flag in got:
            lines.append(f"RED-OK {fname} 抓到 {want_flag}（flags={got}）")
        else:
            lines.append(f"FAIL {fname} 未抓到 {want_flag}（flags={got}）")
            ok = False
    # 對照組：正常檔必須是綠的（否則「先紅」是假的——規則永遠報紅）
    normal = corpus / "lo_out"
    sample = sorted(normal.glob("*.odt"))[:1]
    if sample:
        rec = survey_file(sample[0], {"file": sample[0].name, "kind": "lo-converted"})
        bad = [f for f in rec.get("flags", []) if f.startswith("mimetype")
               or f == "missing_manifest_xml"]
        if bad:
            lines.append(f"FAIL 對照組 {sample[0].name} 不該報 {bad}（規則過寬）")
            ok = False
        else:
            lines.append(f"GREEN-OK 對照組 {sample[0].name} mimetype/manifest 均綠")
    return ok, lines


def main() -> int:
    repo = pathlib.Path(__file__).resolve().parent.parent
    ap = argparse.ArgumentParser(description="corpus-odt 50 檔容器級普查")
    ap.add_argument("--corpus", default=str(repo / "corpus-odt"))
    ap.add_argument("--out", default=str(repo / "docs/odf-survey/survey.json"))
    ap.add_argument("--self-test", action="store_true",
                    help="只跑先紅自證＋manifest 完備性自證，不寫 survey.json")
    a = ap.parse_args()
    corpus = pathlib.Path(a.corpus)
    out_path = pathlib.Path(a.out)

    if a.self_test:
        ok, lines = self_test(corpus)
        problems = manifest_completeness(corpus)
        for ln in lines:
            print(ln)
        for p in problems:
            print("MANIFEST-FAIL", p)
        print("self-test:", "PASS" if (ok and not problems) else "FAIL")
        return 0 if (ok and not problems) else 1

    mp = corpus / "manifest.json"
    if not mp.exists():
        print(f"FAIL: 找不到 {mp}", file=sys.stderr)
        return 1
    manifest = json.loads(mp.read_text(encoding="utf-8"))
    entries = manifest.get("files", [])
    records = [survey_file(corpus / e["file"], e) for e in entries]
    problems = manifest_completeness(corpus)
    ok, red_lines = self_test(corpus)
    s = summarize(records, manifest.get("count", len(entries)))

    document = {
        "schema": SCHEMA,
        "description": "corpus-odt 50 檔容器級普查：zip entry 與 mimetype 規則（ODF 1.3 §3.3）、"
                       "content.xml/styles.xml 結構統計、外部引用與加密 entry 病態標記。",
        "source_manifest": "corpus-odt/manifest.json",
        "generated_by": "scripts/odf_survey.py",
        "gate": {
            "manifest_complete": not problems,
            "manifest_problems": problems,
            "red_first_proof_pass": ok,
            "red_first_lines": red_lines,
            "record_count_ok": len(records) == manifest.get("count", len(entries)),
        },
        "files": records,
        "summary": s,
        "required_cases": required_cases(records, s),
    }
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n",
                        encoding="utf-8")
    s = document["summary"]
    print(f"寫出 {out_path}：{len(records)} 檔，entry 合計 {s['entries_total']}，"
          f"mimetype 違例 {len(s['mimetype_violation_files'])} 檔，"
          f"加密 {len(s['encrypted_files'])} 檔，外部引用 {len(s['external_href_files'])} 檔。")
    print(f"閘門：manifest 完備={not problems}，先紅自證={ok}，"
          f"record 數相符={document['gate']['record_count_ok']}，"
          f"ODF-2 必蓋 case {len(document['required_cases'])} 項。")
    if problems:
        for p in problems:
            print("  MANIFEST-FAIL", p)
    return 0 if (not problems and ok) else 1


if __name__ == "__main__":
    sys.exit(main())
