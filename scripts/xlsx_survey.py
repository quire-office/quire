#!/usr/bin/env python3
"""corpus-xlsx 80 檔 cell 級普查（X1a，report-only）。

逐檔解析 xl/workbook.xml、xl/sharedStrings.xml 與各 worksheet XML，產出
機器可讀的 survey.json，供 X1b（cell 解析）與 X2（樣式）挑選考卷。本腳本
只讀 corpus-xlsx，不修改任何引擎代碼或語料。

用法：
    python3 scripts/xlsx_survey.py
    python3 scripts/xlsx_survey.py --corpus corpus-xlsx --out docs/xlsx-survey/survey.json

僅使用 python3 標準函式庫（zipfile、xml.etree.ElementTree、json、zlib）。
輸出為冪等：相同語料與相同 manifest 順序會得到逐位元組相同的 JSON。
"""
from __future__ import annotations

import argparse
import json
import pathlib
import re
import struct
import sys
import zipfile
import zlib
import xml.etree.ElementTree as ET
from collections import Counter

SCHEMA = "quire-xlsx-cell-survey/1"
KNOWN_TYPES = ("n", "s", "inlineStr", "b", "e", "str")
CELL_REF_RE = re.compile(r"^\$?([A-Za-z]+)\$?([0-9]+)$")
ROW_RANGE_RE = re.compile(r"^\$?([0-9]+):\$?([0-9]+)$")
COL_RANGE_RE = re.compile(r"^\$?([A-Za-z]+):\$?([A-Za-z]+)$")
CELL_RANGE_RE = re.compile(
    r"^\$?([A-Za-z]+)\$?([0-9]+):\$?([A-Za-z]+)\$?([0-9]+)$")
REL_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
EXAMPLE_LIMIT = 20


def local_name(tag: object) -> str:
    """去掉 ElementTree 的 {namespace} 前綴，只留下 local name。"""
    if not isinstance(tag, str):
        return ""
    return tag.rsplit("}", 1)[-1]


def attr_local(elem: ET.Element, name: str) -> str | None:
    """依 local name 取屬性，兼容有無 namespace 前綴。"""
    if name in elem.attrib:
        return elem.attrib[name]
    for key, value in elem.attrib.items():
        if key.rsplit("}", 1)[-1] == name:
            return value
    return None


def children(elem: ET.Element, name: str) -> list[ET.Element]:
    return [child for child in list(elem) if local_name(child.tag) == name]


def col_to_index(letters: str) -> int:
    """A -> 1、Z -> 26、AA -> 27。"""
    value = 0
    for ch in letters.upper():
        value = value * 26 + (ord(ch) - ord("A") + 1)
    return value


def index_to_col(value: int) -> str:
    """1 -> A、26 -> Z、27 -> AA。"""
    out = ""
    while value > 0:
        value, rem = divmod(value - 1, 26)
        out = chr(ord("A") + rem) + out
    return out


def parse_cell_ref(ref: str | None) -> tuple[int, int] | None:
    """回傳 (row, col)；非法或缺漏回傳 None。"""
    if not ref:
        return None
    match = CELL_REF_RE.match(ref.strip())
    if not match:
        return None
    return int(match.group(2)), col_to_index(match.group(1))


def parse_dimension_ref(ref: str | None) -> dict:
    """解析 dimension ref，支援 cell、cell range、row range、column range。

    row range（例如 1:102）與 column range（例如 A:C）是合法的 ST_Ref
    變體，單獨記錄 kind，交叉核對時只比對有宣告的那一軸。
    """
    result: dict = {
        "ref": ref,
        "kind": "invalid",
        "min_col": None,
        "max_col": None,
        "min_row": None,
        "max_row": None,
    }
    if not ref:
        return result
    text = ref.strip()
    match = CELL_RANGE_RE.match(text)
    if match:
        result.update({
            "kind": "cell_range",
            "min_col": col_to_index(match.group(1)),
            "min_row": int(match.group(2)),
            "max_col": col_to_index(match.group(3)),
            "max_row": int(match.group(4)),
        })
        return result
    match = CELL_REF_RE.match(text)
    if match:
        row, col = int(match.group(2)), col_to_index(match.group(1))
        result.update({
            "kind": "cell",
            "min_col": col,
            "max_col": col,
            "min_row": row,
            "max_row": row,
        })
        return result
    match = ROW_RANGE_RE.match(text)
    if match:
        result.update({
            "kind": "row_range",
            "min_row": int(match.group(1)),
            "max_row": int(match.group(2)),
        })
        return result
    match = COL_RANGE_RE.match(text)
    if match:
        result.update({
            "kind": "col_range",
            "min_col": col_to_index(match.group(1)),
            "max_col": col_to_index(match.group(2)),
        })
        return result
    return result


def bbox_from_cells(refs: list[str]) -> dict | None:
    coords = [parse_cell_ref(ref) for ref in refs]
    coords = [coord for coord in coords if coord is not None]
    if not coords:
        return None
    rows = [coord[0] for coord in coords]
    cols = [coord[1] for coord in coords]
    return {
        "min_row": min(rows),
        "max_row": max(rows),
        "min_col": min(cols),
        "max_col": max(cols),
        "ref": f"{index_to_col(min(cols))}{min(rows)}:{index_to_col(max(cols))}{max(rows)}",
    }


def dimension_consistency(declared: dict, actual: dict | None) -> tuple[bool, str, str]:
    """比對宣告 dimension 與實際 cell 外框。

    回傳 (是否不一致, 說明, 不一致類型)。類型區分為 actual_outside_declared
    （實際 cell 超出宣告範圍，才是真正紅燈）、declared_larger_than_actual
    （宣告比實際大，常見於結尾空白格式化列）、shifted（兩側各有出入）、
    declared_but_no_cells（宣告 A1 但沒有任何 cell）與 invalid_ref。
    """
    if declared["kind"] == "missing":
        return True, "沒有 dimension 宣告", "missing_dimension"
    if declared["kind"] == "invalid":
        return True, "dimension ref 無法解析或非標準形式", "invalid_ref"
    if actual is None:
        return True, "宣告了 dimension 但實際掃不到任何 cell 座標", "declared_but_no_cells"
    if declared["kind"] in ("cell", "cell_range"):
        actual_out = (actual["min_row"] < declared["min_row"]
                      or actual["max_row"] > declared["max_row"]
                      or actual["min_col"] < declared["min_col"]
                      or actual["max_col"] > declared["max_col"])
        declared_out = (declared["min_row"] < actual["min_row"]
                        or declared["max_row"] > actual["max_row"]
                        or declared["min_col"] < actual["min_col"]
                        or declared["max_col"] > actual["max_col"])
    elif declared["kind"] == "row_range":
        actual_out = (actual["min_row"] < declared["min_row"]
                      or actual["max_row"] > declared["max_row"])
        declared_out = (declared["min_row"] < actual["min_row"]
                        or declared["max_row"] > actual["max_row"])
    elif declared["kind"] == "col_range":
        actual_out = (actual["min_col"] < declared["min_col"]
                      or actual["max_col"] > declared["max_col"])
        declared_out = (declared["min_col"] < actual["min_col"]
                        or declared["max_col"] > actual["max_col"])
    else:
        return True, "dimension ref 類型無法核對", "invalid_ref"
    if actual_out and declared_out:
        return True, "宣告與實際外框互有超出（錯位）", "shifted"
    if actual_out:
        return True, "實際 cell 超出宣告 dimension 範圍", "actual_outside_declared"
    if declared_out:
        return True, "宣告 dimension 大於實際 cell 外框", "declared_larger_than_actual"
    return False, "", ""


def unpack_local_header(raw: bytes, offset: int) -> tuple[int, int] | None:
    """回傳 (檔名長度, extra 長度)；偏移或簽章不對回傳 None。"""
    if offset < 0 or offset + 30 > len(raw):
        return None
    header = raw[offset:offset + 30]
    if len(header) < 30 or header[:4] != b"PK\x03\x04":
        return None
    name_len, extra_len = struct.unpack_from("<HH", header, 26)
    return name_len, extra_len


def read_member_raw(zf: zipfile.ZipFile, name: str, archive: pathlib.Path) -> bytes:
    """繞過 CRC 檢查，直接解壓單一 zip member。

    僅在 zipfile 因 CRC 不符拋錯時作為補救；corpus 內有一檔 fuzz testcase
    的 sheet8.xml CRC 損壞，但 DEFLATE 資料本身可解，仍可完整普查。
    """
    info = zf.getinfo(name)
    with archive.open("rb") as handle:
        handle.seek(0, 2)
        size = handle.tell()
        handle.seek(0)
        raw = handle.read(size)
    lengths = unpack_local_header(raw, info.header_offset)
    if lengths is None:
        raise zipfile.BadZipFile(f"local header 無法解析: {name}")
    name_len, extra_len = lengths
    start = info.header_offset + 30 + name_len + extra_len
    payload = raw[start:start + info.compress_size]
    if info.compress_type == zipfile.ZIP_STORED:
        return payload
    if info.compress_type == zipfile.ZIP_DEFLATED:
        return zlib.decompress(payload, -15)
    raise zipfile.BadZipFile(f"不支援的壓縮方式 {info.compress_type}: {name}")


def read_member(zf: zipfile.ZipFile, name: str, archive: pathlib.Path) -> tuple[bytes | None, str | None]:
    """讀取 zip member；CRC 損壞時改用 raw deflate 補救並回報原因。"""
    try:
        return zf.read(name), None
    except zipfile.BadZipFile as exc:
        try:
            return read_member_raw(zf, name, archive), f"CRC 損壞，已用 raw deflate 補讀: {exc}"
        except Exception as fallback_exc:
            return None, f"讀取失敗: {exc}；補讀也失敗: {fallback_exc}"
    except Exception as exc:
        return None, f"讀取失敗: {exc}"


def zip_norm(path: str) -> str:
    return path.replace("\\", "/")


def resolve_target(base_dir: str, target: str) -> str:
    target = zip_norm(target)
    if target.startswith("/"):
        return target.lstrip("/")
    parts = [part for part in (base_dir + "/" + target).split("/") if part]
    normalized: list[str] = []
    for part in parts:
        if part == ".":
            continue
        if part == "..":
            if normalized:
                normalized.pop()
            continue
        normalized.append(part)
    return "/".join(normalized)


def read_workbook_sheets(zf: zipfile.ZipFile, archive: pathlib.Path,
                         names: list[str], read_errors: list[dict]) -> tuple[list[dict], int | None]:
    """從 workbook.xml 與其 rels 解析 sheet 清單（含 chartsheet）。"""
    workbook_parts = [name for name in names
                      if name.endswith("/workbook.xml") or name == "workbook.xml"]
    if not workbook_parts:
        return [], None
    workbook_part = workbook_parts[0]
    data, error = read_member(zf, workbook_part, archive)
    if error:
        read_errors.append({"part": workbook_part, "error": error})
    if data is None:
        return [], None
    try:
        root = ET.fromstring(data)
    except ET.ParseError as exc:
        read_errors.append({"part": workbook_part, "error": f"XML 解析失敗: {exc}"})
        return [], None

    rels_part = f"{workbook_part.rsplit('/', 1)[0]}/_rels/{workbook_part.rsplit('/', 1)[-1]}.rels"
    rels: dict[str, dict] = {}
    if rels_part in names:
        rel_data, rel_error = read_member(zf, rels_part, archive)
        if rel_error:
            read_errors.append({"part": rels_part, "error": rel_error})
        if rel_data is not None:
            try:
                rel_root = ET.fromstring(rel_data)
                for rel in rel_root:
                    if local_name(rel.tag) != "Relationship":
                        continue
                    rel_id = rel.get("Id")
                    if rel_id:
                        rels[rel_id] = {
                            "type": rel.get("Type") or "",
                            "target": rel.get("Target") or "",
                        }
            except ET.ParseError as exc:
                read_errors.append({"part": rels_part, "error": f"XML 解析失敗: {exc}"})

    base_dir = workbook_part.rsplit("/", 1)[0] if "/" in workbook_part else ""
    sheets: list[dict] = []
    for elem in root.iter():
        if local_name(elem.tag) != "sheet":
            continue
        rel_id = attr_local(elem, "id")
        rel = rels.get(rel_id or "", {})
        target = rel.get("target", "")
        part = resolve_target(base_dir, target) if target else None
        rel_type = rel.get("type", "")
        kind = "worksheet"
        if rel_type.endswith("/chartsheet") or (part and part.startswith("xl/chartsheets/")):
            kind = "chartsheet"
        elif rel_type.endswith("/dialogsheet") or (part and part.startswith("xl/dialogsheets/")):
            kind = "dialogsheet"
        sheets.append({
            "name": elem.get("name"),
            "part": part,
            "kind": kind,
            "relationship_id": rel_id,
            "relationship_type": rel_type or None,
        })
    return sheets, len(sheets)


def survey_sheet(zf: zipfile.ZipFile, archive: pathlib.Path, sheet: dict,
                 read_errors: list[dict]) -> dict:
    """普查單一 worksheet XML；chartsheet 不掃 cell。"""
    result: dict = {
        "name": sheet.get("name"),
        "part": sheet.get("part"),
        "kind": sheet.get("kind"),
        "readable": False,
        "dimension": None,
        "actual_bbox": None,
        "dimension_mismatch": False,
        "dimension_mismatch_reason": "",
        "dimension_mismatch_kind": "",
        "actual_outside_declared": False,
        "rows": 0,
        "row_gap_count": 0,
        "missing_row_numbers": 0,
        "rows_missing_r": 0,
        "cells": 0,
        "cells_missing_r": 0,
        "t_missing": 0,
        "cell_type_counts": {},
        "other_cell_types": {},
        "empty_cells": 0,
        "spans_row": 0,
        "spans_cell": 0,
        "is_count": 0,
        "is_rich_count": 0,
        "is_rich_run_count": 0,
    }
    if sheet.get("kind") != "worksheet" or not sheet.get("part"):
        result["readable"] = None
        return result
    part = sheet["part"]
    if part not in zf.namelist():
        result["read_error"] = "workbook 宣告的 worksheet part 不在 zip 內"
        read_errors.append({"part": part, "error": result["read_error"]})
        return result
    data, error = read_member(zf, part, archive)
    if error:
        read_errors.append({"part": part, "error": error})
    if data is None:
        result["read_error"] = "worksheet XML 讀取失敗"
        return result
    try:
        root = ET.fromstring(data)
    except ET.ParseError as exc:
        result["read_error"] = f"XML 解析失敗: {exc}"
        return result
    result["readable"] = True

    dimensions = [elem for elem in root.iter() if local_name(elem.tag) == "dimension"]
    dimension_elem = dimensions[0] if dimensions else None
    if dimension_elem is None:
        dimension_info = {
            "present": False,
            "ref": None,
            "kind": "missing",
            "min_col": None,
            "max_col": None,
            "min_row": None,
            "max_row": None,
        }
    else:
        parsed = parse_dimension_ref(dimension_elem.get("ref"))
        dimension_info = {"present": True, **parsed}
    result["dimension"] = dimension_info

    type_counts: Counter = Counter()
    other_counts: Counter = Counter()
    row_numbers: list[int] = []
    rows_missing_r = 0
    cell_refs: list[str] = []
    cells_missing_r = 0

    sheet_data = next((elem for elem in root.iter()
                       if local_name(elem.tag) == "sheetData"), None)
    row_elems = children(sheet_data, "row") if sheet_data is not None else []

    for row in row_elems:
        row_ref = row.get("r")
        if row_ref is None:
            rows_missing_r += 1
        else:
            try:
                row_numbers.append(int(row_ref))
            except ValueError:
                rows_missing_r += 1
        if row.get("spans") is not None:
            result["spans_row"] += 1
        for cell in children(row, "c"):
            result["cells"] += 1
            cell_ref = cell.get("r")
            if cell_ref is None:
                cells_missing_r += 1
            else:
                cell_refs.append(cell_ref)
            if cell.get("spans") is not None:
                result["spans_cell"] += 1
            if len(list(cell)) == 0:
                result["empty_cells"] += 1
            raw_type = cell.get("t")
            if raw_type is None:
                result["t_missing"] += 1
            cell_type = "n" if raw_type in (None, "", "n") else raw_type
            if cell_type in KNOWN_TYPES:
                type_counts[cell_type] += 1
            else:
                other_counts[raw_type] += 1
            for child in cell:
                child_name = local_name(child.tag)
                if child_name == "is":
                    result["is_count"] += 1
                    runs = [run for run in child.iter() if local_name(run.tag) == "r"]
                    if runs:
                        result["is_rich_count"] += 1
                        result["is_rich_run_count"] += len(runs)

    result["rows"] = len(row_elems)
    result["rows_missing_r"] = rows_missing_r
    result["cells_missing_r"] = cells_missing_r
    result["cell_type_counts"] = {key: type_counts.get(key, 0) for key in KNOWN_TYPES}
    result["other_cell_types"] = dict(sorted(other_counts.items()))
    sorted_rows = sorted(set(row_numbers))
    if sorted_rows:
        gap_count = 0
        missing_numbers = 0
        for prev, cur in zip(sorted_rows, sorted_rows[1:]):
            if cur - prev > 1:
                gap_count += 1
                missing_numbers += cur - prev - 1
        result["row_gap_count"] = gap_count
        result["missing_row_numbers"] = missing_numbers

    actual = bbox_from_cells(cell_refs)
    result["actual_bbox"] = actual
    mismatch, reason, mismatch_kind = dimension_consistency(dimension_info, actual)
    result["dimension_mismatch"] = mismatch
    result["dimension_mismatch_reason"] = reason
    result["dimension_mismatch_kind"] = mismatch_kind
    result["actual_outside_declared"] = mismatch_kind in ("actual_outside_declared", "shifted")
    return result


def survey_file(archive: pathlib.Path, manifest_entry: dict) -> dict:
    read_errors: list[dict] = []
    record: dict = {
        "file": manifest_entry["file"],
        "source": manifest_entry.get("source"),
        "doc_type": manifest_entry.get("doc_type"),
        "size": archive.stat().st_size,
        "manifest_bytes": manifest_entry.get("bytes"),
        "sheet_count": None,
        "chartsheet_count": 0,
        "worksheet_count": 0,
        "shared_strings": {
            "present": False,
            "path": None,
            "count": 0,
            "rich_text_count": 0,
            "rich_run_count": 0,
        },
        "cells": {
            "total": 0,
            "by_type": {key: 0 for key in KNOWN_TYPES},
            "other_types": {},
            "t_missing": 0,
            "empty": 0,
            "missing_r": 0,
            "spans": {"row": 0, "cell": 0, "present": False},
            "inline_rich": {
                "is_count": 0,
                "is_rich_count": 0,
                "is_rich_run_count": 0,
            },
        },
        "structure": {
            "rows": 0,
            "row_gap_sheets": [],
            "missing_row_numbers": 0,
            "sheets_without_dimension": [],
            "sheets_with_row_gaps": [],
        },
        "dimension": {
            "mismatch": False,
            "mismatch_kind_counts": {},
            "actual_outside_declared": False,
            "actual_outside_declared_sheets": [],
            "mismatch_sheets": [],
            "missing_dimension_sheets": [],
        },
        "pathology": {
            "shared_strings_missing": False,
            "dangling_shared_string_refs": 0,
            "dangling_examples": [],
            "malformed_shared_string_cells": 0,
            "unused_shared_strings": 0,
            "unused_examples": [],
            "shared_strings_fully_unused": False,
            "sheets_without_dimension": [],
            "read_errors": [],
        },
        "sheets": [],
        "flags": [],
    }

    try:
        zf = zipfile.ZipFile(archive)
    except Exception as exc:
        record["pathology"]["read_errors"] = [{"part": None, "error": f"zip 無法開啟: {exc}"}]
        record["flags"].append("read_error")
        return record

    try:
        names = zf.namelist()
        sheets, workbook_sheet_count = read_workbook_sheets(zf, archive, names, read_errors)
        record["sheet_count"] = workbook_sheet_count

        worksheet_parts = [name for name in names
                           if name.startswith("xl/worksheets/") and name.endswith(".xml")]
        referenced_parts = {sheet["part"] for sheet in sheets if sheet.get("part")}
        if not sheets:
            sheets = [{
                "name": None,
                "part": part,
                "kind": "worksheet",
                "relationship_id": None,
                "relationship_type": None,
            } for part in sorted(worksheet_parts)]
        else:
            for part in sorted(set(worksheet_parts) - referenced_parts):
                sheets.append({
                    "name": None,
                    "part": part,
                    "kind": "worksheet",
                    "relationship_id": None,
                    "relationship_type": None,
                })

        record["chartsheet_count"] = sum(1 for sheet in sheets if sheet.get("kind") == "chartsheet")
        record["worksheet_count"] = sum(1 for sheet in sheets if sheet.get("kind") == "worksheet")

        shared_path = "xl/sharedStrings.xml"
        if shared_path not in names:
            shared_path = next((name for name in names if name.endswith("/sharedStrings.xml")), None)
        shared_count = 0
        shared_rich_count = 0
        shared_rich_runs = 0
        shared_loaded = False
        if shared_path:
            shared_data, shared_error = read_member(zf, shared_path, archive)
            if shared_error:
                read_errors.append({"part": shared_path, "error": shared_error})
            if shared_data is not None:
                try:
                    shared_root = ET.fromstring(shared_data)
                    si_elems = [elem for elem in shared_root
                                if local_name(elem.tag) == "si"]
                    shared_count = len(si_elems)
                    for si in si_elems:
                        runs = [run for run in si.iter() if local_name(run.tag) == "r"]
                        if runs:
                            shared_rich_count += 1
                            shared_rich_runs += len(runs)
                    shared_loaded = True
                    record["shared_strings"].update({
                        "present": True,
                        "path": shared_path,
                        "count": shared_count,
                        "rich_text_count": shared_rich_count,
                        "rich_run_count": shared_rich_runs,
                    })
                except ET.ParseError as exc:
                    read_errors.append({"part": shared_path, "error": f"XML 解析失敗: {exc}"})

        total_type_counts: Counter = Counter()
        total_other_counts: Counter = Counter()
        t_missing = 0
        serialized_sheets: list[dict] = []
        s_cells: list[dict] = []

        for sheet in sheets:
            sheet_result = survey_sheet(zf, archive, sheet, read_errors)
            serialized_sheets.append(sheet_result)
            if not sheet_result.get("readable"):
                continue
            record["cells"]["total"] += sheet_result["cells"]
            record["cells"]["empty"] += sheet_result["empty_cells"]
            record["cells"]["missing_r"] += sheet_result["cells_missing_r"]
            record["cells"]["t_missing"] += sheet_result["t_missing"]
            record["cells"]["spans"]["row"] += sheet_result["spans_row"]
            record["cells"]["spans"]["cell"] += sheet_result["spans_cell"]
            record["cells"]["inline_rich"]["is_count"] += sheet_result["is_count"]
            record["cells"]["inline_rich"]["is_rich_count"] += sheet_result["is_rich_count"]
            record["cells"]["inline_rich"]["is_rich_run_count"] += sheet_result["is_rich_run_count"]
            for key, value in sheet_result["cell_type_counts"].items():
                total_type_counts[key] += value
            for key, value in sheet_result["other_cell_types"].items():
                total_other_counts[key] += value
            record["structure"]["rows"] += sheet_result["rows"]
            record["structure"]["missing_row_numbers"] += sheet_result["missing_row_numbers"]
            if sheet_result["missing_row_numbers"] > 0:
                record["structure"]["sheets_with_row_gaps"].append(sheet_result["part"])
            if not sheet_result["dimension"]["present"]:
                record["structure"]["sheets_without_dimension"].append(sheet_result["part"])
                record["dimension"]["missing_dimension_sheets"].append(sheet_result["part"])
            if sheet_result["dimension_mismatch"]:
                record["dimension"]["mismatch_sheets"].append({
                    "part": sheet_result["part"],
                    "name": sheet_result["name"],
                    "declared": sheet_result["dimension"],
                    "actual": sheet_result["actual_bbox"],
                    "reason": sheet_result["dimension_mismatch_reason"],
                    "kind": sheet_result["dimension_mismatch_kind"],
                    "actual_outside_declared": sheet_result["actual_outside_declared"],
                })
                kind = sheet_result["dimension_mismatch_kind"]
                record["dimension"]["mismatch_kind_counts"][kind] = (
                    record["dimension"]["mismatch_kind_counts"].get(kind, 0) + 1)
                if sheet_result["actual_outside_declared"]:
                    record["dimension"]["actual_outside_declared_sheets"].append(sheet_result["part"])

            for cell in children_by_part(zf, archive, sheet_result["part"], read_errors):
                if cell.get("t") != "s":
                    continue
                value_elem = next((child for child in cell
                                   if local_name(child.tag) == "v"), None)
                raw_value = value_elem.text if value_elem is not None else None
                s_cells.append({
                    "part": sheet_result["part"],
                    "ref": cell.get("r"),
                    "raw": raw_value,
                })

        record["cells"]["by_type"] = {key: total_type_counts.get(key, 0) for key in KNOWN_TYPES}
        record["cells"]["other_types"] = dict(sorted(total_other_counts.items()))
        record["cells"]["spans"]["present"] = bool(
            record["cells"]["spans"]["row"] or record["cells"]["spans"]["cell"])
        record["structure"]["row_gap_sheets"] = list(record["structure"]["sheets_with_row_gaps"])
        record["dimension"]["mismatch"] = bool(record["dimension"]["mismatch_sheets"])
        record["dimension"]["actual_outside_declared"] = bool(
            record["dimension"]["actual_outside_declared_sheets"])
        record["structure"]["sheets_without_dimension"] = list(record["dimension"]["missing_dimension_sheets"])

        dangling: list[dict] = []
        malformed = 0
        referenced_indices: set[int] = set()
        for cell in s_cells:
            raw = cell["raw"]
            try:
                index = int(raw.strip()) if raw is not None else None
            except (TypeError, ValueError):
                index = None
            if index is None:
                malformed += 1
                if len(dangling) < EXAMPLE_LIMIT:
                    dangling.append({
                        "part": cell["part"],
                        "cell": cell["ref"],
                        "value": raw,
                        "reason": "t=s 但 v 缺失或非整數",
                    })
                continue
            referenced_indices.add(index)
            if not shared_loaded:
                reason = "t=s 但 sharedStrings 缺失或無法解析"
            elif index < 0 or index >= shared_count:
                reason = "sharedStrings 索引越界"
            else:
                reason = None
            if reason and len(dangling) < EXAMPLE_LIMIT:
                dangling.append({
                    "part": cell["part"],
                    "cell": cell["ref"],
                    "index": index,
                    "shared_strings_count": shared_count if shared_loaded else None,
                    "reason": reason,
                })
            if reason:
                record["pathology"]["dangling_shared_string_refs"] += 1
        record["pathology"]["malformed_shared_string_cells"] = malformed

        if shared_loaded and shared_count > 0:
            in_range = {index for index in referenced_indices if 0 <= index < shared_count}
            unused_indices = [index for index in range(shared_count) if index not in in_range]
            record["pathology"]["unused_shared_strings"] = len(unused_indices)
            record["pathology"]["unused_examples"] = unused_indices[:EXAMPLE_LIMIT]
            record["pathology"]["shared_strings_fully_unused"] = not in_range

        record["pathology"]["shared_strings_missing"] = not shared_loaded
        record["pathology"]["dangling_examples"] = dangling
        record["pathology"]["sheets_without_dimension"] = list(record["dimension"]["missing_dimension_sheets"])
        deduped_errors: list[dict] = []
        seen_errors: set[tuple[str | None, str]] = set()
        for error in read_errors:
            key = (error.get("part"), error.get("error", ""))
            if key in seen_errors:
                continue
            seen_errors.add(key)
            deduped_errors.append(error)
        read_errors[:] = deduped_errors
        record["pathology"]["read_errors"] = read_errors
        record["sheets"] = serialized_sheets

        flags: list[str] = []
        if record["dimension"]["mismatch"]:
            flags.append("dimension_mismatch")
        if record["dimension"]["missing_dimension_sheets"]:
            flags.append("dimension_missing")
        if record["pathology"]["dangling_shared_string_refs"]:
            flags.append("dangling_shared_strings")
        if record["pathology"]["unused_shared_strings"]:
            flags.append("unused_shared_strings")
        if record["pathology"]["shared_strings_fully_unused"]:
            flags.append("shared_strings_fully_unused")
        if record["pathology"]["shared_strings_missing"]:
            flags.append("shared_strings_missing")
        if record["pathology"]["malformed_shared_string_cells"]:
            flags.append("malformed_shared_string_cells")
        if read_errors:
            flags.append("read_error")
        if record["structure"]["row_gap_sheets"]:
            flags.append("row_gap")
        if record["cells"]["spans"]["present"]:
            flags.append("spans")
        if record["cells"]["inline_rich"]["is_rich_count"]:
            flags.append("inline_rich")
        record["flags"] = flags
    finally:
        zf.close()
    return record


def children_by_part(zf: zipfile.ZipFile, archive: pathlib.Path, part: str,
                     read_errors: list[dict]) -> list[ET.Element]:
    """重新解析單一 worksheet 內所有 c 元素（用於 shared string 引用盤點）。"""
    if not part or part not in zf.namelist():
        return []
    data, error = read_member(zf, part, archive)
    if error:
        read_errors.append({"part": part, "error": error})
    if data is None:
        return []
    try:
        root = ET.fromstring(data)
    except ET.ParseError:
        return []
    return [elem for elem in root.iter() if local_name(elem.tag) == "c"]


def summarize(records: list[dict], manifest_count: int) -> dict:
    """彙整整批普查結果；清單排序以 manifest 順序為準。"""
    cell_totals: Counter = Counter()
    other_totals: Counter = Counter()
    files_with_type: Counter = Counter()
    summary: dict = {
        "manifest_count": manifest_count,
        "file_count": len(records),
        "cell_total": 0,
        "cell_totals_by_type": {key: 0 for key in KNOWN_TYPES},
        "other_cell_type_totals": {},
        "empty_cells_total": 0,
        "cells_missing_r_total": 0,
        "cells_t_missing_total": 0,
        "sheets_total": 0,
        "worksheet_parts_total": 0,
        "chartsheet_total": 0,
        "shared_strings_files": 0,
        "shared_strings_entries_total": 0,
        "rich_text_si_total": 0,
        "rich_text_si_files": [],
        "files_with_cell_type": {},
        "dimension_mismatch_files": [],
        "dimension_actual_outside_declared_files": [],
        "dimension_mismatch_kind_counts": {},
        "dimension_missing_files": [],
        "dangling_shared_string_files": [],
        "unused_shared_string_files": [],
        "fully_unused_shared_string_files": [],
        "shared_strings_missing_files": [],
        "row_gap_files": [],
        "spans_files": [],
        "inline_rich_files": [],
        "read_error_files": [],
        "pathology_files": [],
    }
    for record in records:
        name = record["file"]
        cell_totals.update(record["cells"]["by_type"])
        other_totals.update(record["cells"]["other_types"])
        summary["cell_total"] += record["cells"]["total"]
        summary["empty_cells_total"] += record["cells"]["empty"]
        summary["cells_missing_r_total"] += record["cells"]["missing_r"]
        summary["cells_t_missing_total"] += record["cells"]["t_missing"]
        summary["sheets_total"] += record["sheet_count"] or 0
        summary["worksheet_parts_total"] += record["worksheet_count"]
        summary["chartsheet_total"] += record["chartsheet_count"]
        for key, value in record["cells"]["by_type"].items():
            if value > 0:
                files_with_type[key] += 1
        if record["shared_strings"]["present"]:
            summary["shared_strings_files"] += 1
            summary["shared_strings_entries_total"] += record["shared_strings"]["count"]
            summary["rich_text_si_total"] += record["shared_strings"]["rich_text_count"]
            if record["shared_strings"]["rich_text_count"] > 0:
                summary["rich_text_si_files"].append(name)
        if record["dimension"]["mismatch"]:
            summary["dimension_mismatch_files"].append(name)
        if record["dimension"]["actual_outside_declared"]:
            summary["dimension_actual_outside_declared_files"].append(name)
        for kind, count in record["dimension"]["mismatch_kind_counts"].items():
            summary["dimension_mismatch_kind_counts"][kind] = (
                summary["dimension_mismatch_kind_counts"].get(kind, 0) + count)
        if record["dimension"]["missing_dimension_sheets"]:
            summary["dimension_missing_files"].append(name)
        if record["pathology"]["dangling_shared_string_refs"]:
            summary["dangling_shared_string_files"].append(name)
        if record["pathology"]["unused_shared_strings"]:
            summary["unused_shared_string_files"].append(name)
        if record["pathology"]["shared_strings_fully_unused"]:
            summary["fully_unused_shared_string_files"].append(name)
        if record["pathology"]["shared_strings_missing"]:
            summary["shared_strings_missing_files"].append(name)
        if record["structure"]["row_gap_sheets"]:
            summary["row_gap_files"].append(name)
        if record["cells"]["spans"]["present"]:
            summary["spans_files"].append(name)
        if record["cells"]["inline_rich"]["is_rich_count"]:
            summary["inline_rich_files"].append(name)
        if record["pathology"]["read_errors"]:
            summary["read_error_files"].append(name)
        reasons = [flag for flag in record["flags"]
                   if flag not in ("spans", "inline_rich", "row_gap")]
        if reasons:
            summary["pathology_files"].append({"file": name, "reasons": reasons})

    summary["cell_totals_by_type"] = {key: cell_totals.get(key, 0) for key in KNOWN_TYPES}
    summary["other_cell_type_totals"] = dict(sorted(other_totals.items()))
    summary["files_with_cell_type"] = {key: files_with_type.get(key, 0) for key in KNOWN_TYPES}
    return summary


def main() -> int:
    repo = pathlib.Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description="corpus-xlsx 80 檔 cell 級普查")
    parser.add_argument("--corpus", default=str(repo / "corpus-xlsx"),
                        help="corpus-xlsx 目錄（預設 <repo>/corpus-xlsx）")
    parser.add_argument("--out", default=str(repo / "docs/xlsx-survey/survey.json"),
                        help="輸出 JSON 路徑（預設 docs/xlsx-survey/survey.json）")
    args = parser.parse_args()

    corpus = pathlib.Path(args.corpus)
    out_path = pathlib.Path(args.out)
    manifest_path = corpus / "manifest.json"
    if not manifest_path.exists():
        print(f"FAIL: 找不到 {manifest_path}", file=sys.stderr)
        return 1
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    entries = manifest.get("files", [])
    records: list[dict] = []
    for entry in entries:
        archive = corpus / entry["file"]
        if not archive.exists():
            records.append({
                "file": entry["file"],
                "source": entry.get("source"),
                "doc_type": entry.get("doc_type"),
                "size": None,
                "manifest_bytes": entry.get("bytes"),
                "sheet_count": None,
                "chartsheet_count": 0,
                "worksheet_count": 0,
                "shared_strings": {"present": False, "path": None, "count": 0,
                                   "rich_text_count": 0, "rich_run_count": 0},
                "cells": {"total": 0, "by_type": {key: 0 for key in KNOWN_TYPES},
                          "other_types": {}, "t_missing": 0, "empty": 0, "missing_r": 0,
                          "spans": {"row": 0, "cell": 0, "present": False},
                          "inline_rich": {"is_count": 0, "is_rich_count": 0,
                                          "is_rich_run_count": 0}},
                "structure": {"rows": 0, "row_gap_sheets": [], "missing_row_numbers": 0,
                              "sheets_without_dimension": [], "sheets_with_row_gaps": []},
                "dimension": {"mismatch": False, "mismatch_sheets": [],
                              "missing_dimension_sheets": []},
                "pathology": {"shared_strings_missing": True, "dangling_shared_string_refs": 0,
                              "dangling_examples": [], "malformed_shared_string_cells": 0,
                              "unused_shared_strings": 0, "unused_examples": [],
                              "shared_strings_fully_unused": False,
                              "sheets_without_dimension": [],
                              "read_errors": [{"part": None, "error": "實體檔不存在"}]},
                "sheets": [],
                "flags": ["missing_file", "read_error"],
            })
            continue
        records.append(survey_file(archive, entry))

    document = {
        "schema": SCHEMA,
        "description": "corpus-xlsx 80 檔 cell 級普查：型別、特殊結構、dimension 交叉核對、sharedStrings 病態。",
        "source_manifest": "corpus-xlsx/manifest.json",
        "generated_by": "scripts/xlsx_survey.py",
        "files": records,
        "summary": summarize(records, manifest.get("count", len(entries))),
    }
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n",
                        encoding="utf-8")
    summary = document["summary"]
    print(f"寫出 {out_path}：{len(records)} 檔，cell 總數 {summary['cell_total']}，"
          f"dimension 不一致 {len(summary['dimension_mismatch_files'])} 檔，"
          f"懸空 sharedString {len(summary['dangling_shared_string_files'])} 檔。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
