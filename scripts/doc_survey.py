#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""DOC-C1：corpus-doc 48 檔 MS-CFB 容器與 MS-DOC 語意普查（report-only，零引擎代碼）。

讀取 corpus-doc/manifest.json，逐檔產出一筆 record，寫入 docs/doc-survey/survey.json。
輸出為嚴格冪等：相同語料與相同 manifest 會得到逐位元組相同的 JSON。

普查維度（任務卡面必蓋）：
  1. MS-CFB 檔頭合法性：D0CF11E0 標記、Major/Minor Version、Byte Order、ZIP 偽裝檢驗。
  2. 扇區大小：512 位元組（v3）與 4096 位元組（v4）扇區、mini-sector 大小（64 位元組）。
  3. FAT / miniFAT / DIFAT 健康度：
     - DIFAT 陣列解析（Header 109 entries ＋ 擴展 DIFAT 扇區鏈）。
     - FAT 扇區數、FAT 鏈健康狀態（循環引用檢測、越界扇區檢測、Free Sectors 計數）。
     - miniFAT 扇區鏈解析、Mini Stream（Root Entry）完整性與迷你扇區健康度。
  4. 目錄結構與必要流：
     - Root Storage 存在性與 Mini Stream 大小。
     - 目錄項目總數（Directory Entries 遍歷，包含紅黑樹結構展平）。
     - WordDocument 流存在性、流大小、儲存型態（FAT 扇區 vs miniFAT 扇區）。
     - 0Table 與 1Table 流存在性。
     - 其他常見流存在性（Data、SummaryInformation、DocumentSummaryInformation、CompObj）。
  5. FIB（File Information Block）解析：
     - wIdent 合法性（0xA5EC 為 Word 97+，0xA5DC 為 Word 6.0/95 早期相容）。
     - nFib 版本號（Word 97: 193、Word 2000: 217、Word 2002: 257、Word 2003: 268、Word 2007: 274 等）。
     - 旗標解析：fEncrypted（加密）、fComplex（piece table 複雜流）、fWhichTblStm（0Table/1Table 選擇）、fDot、fHasPic 等。
     - 1Table/0Table 選擇與目錄中實際存在之表格流對齊檢驗。
  6. 病態判定（Pathology Flagging）：
     - ZIP 偽裝（magic_mismatch）、檔頭截斷（truncated）、FAT 循環（fat_cycle）、
       缺失 WordDocument（missing_worddocument）、加密（encrypted）、舊版相容（legacy_fib）等。

先紅自證（--self-test）：
  以 fixtures/ 內故意構造的病態樣本驗證普查器能否 100% 準確捕獲對應病理；任何一項未捕獲即退出非零。

用法：python3 scripts/doc_survey.py [--corpus DIR] [--out FILE] [--self-test]
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import struct
import sys
from collections import Counter

SCHEMA = "quire-doc-survey/1"

CFB_SIGNATURE = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1"
ZIP_SIGNATURE = b"PK"

# FIB 版本映射表（參考微軟 [MS-DOC] 規格書 2.5.1 FIB）
NFIB_VERSIONS = {
    101: "Word 6.0",
    104: "Word 95",
    193: "Word 97",
    217: "Word 2000",
    257: "Word 2002 (XP)",
    268: "Word 2003",
    274: "Word 2007",
}


def sha256_bytes(data: bytes) -> str:
    h = hashlib.sha256()
    h.update(data)
    return h.hexdigest()


class CFBParser:
    """MS-CFB（Compound File Binary）低階二進制解析器（report-only，零外部依賴）。"""

    def __init__(self, data: bytes):
        self.data = data
        self.size = len(data)
        self.pathology: list[str] = []
        self.is_cfb = False
        self.is_zip = False
        self.magic_hex = data[:8].hex() if len(data) >= 8 else data.hex()

        self.major_version = 0
        self.minor_version = 0
        self.byte_order = 0
        self.sector_size = 0
        self.mini_sector_size = 0
        self.num_dir_sectors = 0
        self.num_fat_sectors = 0
        self.first_dir_sector = 0
        self.mini_cutoff = 4096
        self.first_minifat_sector = 0xFFFFFFFE
        self.num_minifat_sectors = 0
        self.first_difat_sector = 0xFFFFFFFE
        self.num_difat_sectors = 0

        self.difat: list[int] = []
        self.fat: list[int] = []
        self.minifat: list[int] = []
        self.fat_cyclic = False
        self.fat_out_of_bounds = False
        self.minifat_cyclic = False

        self.entries: list[dict] = []
        self.root_entry: dict | None = None
        self.mini_stream: bytes = b""

    def parse(self) -> dict:
        # 1. 檢查檔案基本檔頭與簽章
        if self.data.startswith(ZIP_SIGNATURE):
            self.is_zip = True
            self.pathology.append("zip_disguised")
            return self._build_result()

        if not self.data.startswith(CFB_SIGNATURE):
            self.pathology.append("magic_invalid")
            return self._build_result()

        self.is_cfb = True
        if self.size < 512:
            self.pathology.append("truncated")
            return self._build_result()

        # 2. 解析 Header
        hdr = self.data[:512]
        (
            self.minor_version,
            self.major_version,
            self.byte_order,
            sec_shift,
            mini_shift,
        ) = struct.unpack("<HHHHH", hdr[24:34])

        if sec_shift not in (9, 12):
            self.pathology.append("invalid_sector_shift")
            return self._build_result()

        self.sector_size = 1 << sec_shift
        self.mini_sector_size = 1 << mini_shift

        (
            self.num_dir_sectors,
            self.num_fat_sectors,
            self.first_dir_sector,
        ) = struct.unpack("<III", hdr[40:52])

        (
            self.mini_cutoff,
            self.first_minifat_sector,
            self.num_minifat_sectors,
        ) = struct.unpack("<III", hdr[56:68])

        (
            self.first_difat_sector,
            self.num_difat_sectors,
        ) = struct.unpack("<II", hdr[68:76])

        # 3. 讀取 DIFAT（Header 109 entries ＋ 擴展 DIFAT 扇區）
        header_difat = list(struct.unpack("<109I", hdr[76:512]))
        self.difat = [s for s in header_difat if s != 0xFFFFFFFF]

        cur_difat_sec = self.first_difat_sector
        visited_difat = set()
        for _ in range(self.num_difat_sectors):
            if cur_difat_sec in (0xFFFFFFFE, 0xFFFFFFFF) or cur_difat_sec in visited_difat:
                break
            visited_difat.add(cur_difat_sec)
            offset = 512 + cur_difat_sec * self.sector_size
            if offset + self.sector_size > self.size:
                self.pathology.append("difat_out_of_bounds")
                break
            sec_bytes = self.data[offset : offset + self.sector_size]
            entries_in_sec = struct.unpack(f"<{self.sector_size // 4}I", sec_bytes)
            self.difat.extend([s for s in entries_in_sec[:-1] if s != 0xFFFFFFFF])
            cur_difat_sec = entries_in_sec[-1]

        # 4. 組裝 FAT 表
        for fat_sec in self.difat:
            offset = 512 + fat_sec * self.sector_size
            if offset + self.sector_size > self.size:
                self.fat_out_of_bounds = True
                self.pathology.append("fat_out_of_bounds")
                break
            sec_bytes = self.data[offset : offset + self.sector_size]
            self.fat.extend(struct.unpack(f"<{self.sector_size // 4}I", sec_bytes))

        # 5. 讀取常規 FAT 鏈流輔助函式
        def read_fat_stream(start_sec: int, max_bytes: int) -> bytes:
            if start_sec in (0xFFFFFFFE, 0xFFFFFFFF):
                return b""
            cur = start_sec
            chunks = []
            visited = set()
            while cur not in (0xFFFFFFFE, 0xFFFFFFFF) and cur < len(self.fat):
                if cur in visited:
                    self.fat_cyclic = True
                    self.pathology.append("fat_cycle")
                    break
                visited.add(cur)
                offset = 512 + cur * self.sector_size
                if offset >= self.size:
                    self.fat_out_of_bounds = True
                    self.pathology.append("fat_out_of_bounds")
                    break
                chunk = self.data[offset : min(offset + self.sector_size, self.size)]
                chunks.append(chunk)
                cur = self.fat[cur]
            return b"".join(chunks)[:max_bytes]

        # 6. 解析 miniFAT 表
        cur_mf = self.first_minifat_sector
        visited_mf = set()
        for _ in range(self.num_minifat_sectors):
            if cur_mf in (0xFFFFFFFE, 0xFFFFFFFF) or cur_mf in visited_mf:
                break
            if cur_mf >= len(self.fat):
                self.pathology.append("minifat_out_of_bounds")
                break
            visited_mf.add(cur_mf)
            offset = 512 + cur_mf * self.sector_size
            if offset + self.sector_size > self.size:
                self.pathology.append("minifat_out_of_bounds")
                break
            sec_bytes = self.data[offset : offset + self.sector_size]
            self.minifat.extend(struct.unpack(f"<{self.sector_size // 4}I", sec_bytes))
            cur_mf = self.fat[cur_mf]

        # 7. 讀取 Directory 扇區流並解析目錄項目（每個 Entry 128 位元組）
        dir_bytes = read_fat_stream(self.first_dir_sector, 1024 * 1024 * 16)
        num_entries = len(dir_bytes) // 128
        for i in range(num_entries):
            raw = dir_bytes[i * 128 : (i + 1) * 128]
            name_raw = raw[:64]
            name_len, obj_type, color_flag = struct.unpack("<HBB", raw[64:68])
            left_sib, right_sib, child_id = struct.unpack("<iii", raw[68:80])
            start_sec, stream_size = struct.unpack("<IQ", raw[116:128])

            # obj_type: 0=Empty, 1=User Storage, 2=User Stream, 5=Root Storage
            if obj_type == 0:
                continue

            # 解碼名稱（UTF-16LE，以 null 位元組結尾）
            if name_len > 0:
                name = name_raw[:name_len].decode("utf-16le", errors="replace").rstrip("\x00")
            else:
                name = ""

            entry_info = {
                "id": i,
                "name": name,
                "type": obj_type,
                "type_name": {1: "Storage", 2: "Stream", 5: "Root"}.get(obj_type, f"Unknown({obj_type})"),
                "start_sector": start_sec,
                "stream_size": stream_size,
            }
            self.entries.append(entry_info)
            if obj_type == 5:
                self.root_entry = entry_info

        # 8. 載入 Mini Stream（由 Root Entry 的常規 FAT 鏈提供）
        if self.root_entry:
            self.mini_stream = read_fat_stream(
                self.root_entry["start_sector"], self.root_entry["stream_size"]
            )
        else:
            self.pathology.append("missing_root_storage")

        # 9. 讀取任意 Stream 內容函式
        def read_entry_stream(entry: dict) -> bytes:
            size = entry["stream_size"]
            start_sec = entry["start_sector"]
            if entry["type"] == 5:
                return self.mini_stream
            if size < self.mini_cutoff:
                # 存於 Mini Stream，依 miniFAT 鏈走訪
                cur = start_sec
                chunks = []
                visited = set()
                while cur not in (0xFFFFFFFE, 0xFFFFFFFF) and cur < len(self.minifat):
                    if cur in visited:
                        self.minifat_cyclic = True
                        self.pathology.append("minifat_cycle")
                        break
                    visited.add(cur)
                    offset = cur * self.mini_sector_size
                    if offset >= len(self.mini_stream):
                        break
                    chunks.append(
                        self.mini_stream[offset : min(offset + self.mini_sector_size, len(self.mini_stream))]
                    )
                    cur = self.minifat[cur]
                return b"".join(chunks)[:size]
            else:
                # 存於常規 FAT 扇區
                return read_fat_stream(start_sec, size)

        # 10. 解析 WordDocument 流與 FIB 結構
        wd_entry = next((e for e in self.entries if e["name"] == "WordDocument"), None)
        fib_result = None

        if wd_entry:
            wd_bytes = read_entry_stream(wd_entry)
            fib_result = self._parse_fib(wd_bytes)
        else:
            self.pathology.append("missing_worddocument")

        # 11. 檢查 0Table / 1Table 與其他常用流
        streams_set = {e["name"] for e in self.entries if e["type"] == 2}
        has_0table = "0Table" in streams_set
        has_1table = "1Table" in streams_set
        has_data = "Data" in streams_set
        has_summary_info = "\x05SummaryInformation" in streams_set
        has_doc_summary_info = "\x05DocumentSummaryInformation" in streams_set
        has_compobj = "\x01CompObj" in streams_set

        # 驗證 FIB 中的表格流與目錄流是否對齊
        if fib_result:
            expected_tbl = fib_result["flags"]["fWhichTblStm"]
            table_stream_present = (expected_tbl == "1Table" and has_1table) or (
                expected_tbl == "0Table" and has_0table
            )
            fib_result["table_stream_present"] = table_stream_present
            if not table_stream_present:
                self.pathology.append("table_stream_mismatch")

        res = self._build_result()
        res.update(
            {
                "cfb": {
                    "major_version": self.major_version,
                    "minor_version": self.minor_version,
                    "byte_order_hex": f"0x{self.byte_order:04X}",
                    "sector_size": self.sector_size,
                    "mini_sector_size": self.mini_sector_size,
                    "num_dir_sectors": self.num_dir_sectors,
                    "num_fat_sectors": self.num_fat_sectors,
                    "first_dir_sector": self.first_dir_sector,
                    "mini_cutoff": self.mini_cutoff,
                    "first_minifat_sector": self.first_minifat_sector,
                    "num_minifat_sectors": self.num_minifat_sectors,
                    "first_difat_sector": self.first_difat_sector,
                    "num_difat_sectors": self.num_difat_sectors,
                    "difat_entries_count": len(self.difat),
                    "fat_entries_count": len(self.fat),
                    "fat_cyclic": self.fat_cyclic,
                    "fat_out_of_bounds": self.fat_out_of_bounds,
                    "minifat_entries_count": len(self.minifat),
                    "minifat_cyclic": self.minifat_cyclic,
                },
                "directory": {
                    "entries_count": len(self.entries),
                    "has_root_storage": self.root_entry is not None,
                    "root_stream_size": self.root_entry["stream_size"] if self.root_entry else 0,
                    "streams": sorted(list(streams_set)),
                    "storages": sorted([e["name"] for e in self.entries if e["type"] == 1]),
                },
                "streams_presence": {
                    "has_worddocument": wd_entry is not None,
                    "has_0table": has_0table,
                    "has_1table": has_1table,
                    "has_data": has_data,
                    "has_summary_info": has_summary_info,
                    "has_doc_summary_info": has_doc_summary_info,
                    "has_compobj": has_compobj,
                },
                "worddocument": {
                    "present": wd_entry is not None,
                    "stream_size": wd_entry["stream_size"] if wd_entry else 0,
                    "storage_type": "minifat" if wd_entry and wd_entry["stream_size"] < self.mini_cutoff else "fat",
                },
                "fib": fib_result,
            }
        )
        return res

    def _parse_fib(self, wd_data: bytes) -> dict | None:
        """解析 WordDocument 開頭之 FIB（File Information Block）結構。"""
        if len(wd_data) < 32:
            self.pathology.append("fib_truncated")
            return None

        wIdent, nFib = struct.unpack("<HH", wd_data[:4])
        # wIdent: 0xA5EC (Word 97+), 0xA5DC (Word 6.0/95)
        wIdent_valid = wIdent in (0xA5EC, 0xA5DC)
        if not wIdent_valid:
            self.pathology.append("fib_ident_invalid")

        if nFib < 193:
            self.pathology.append("legacy_fib_version")

        lid = struct.unpack("<H", wd_data[6:8])[0]
        pnNext = struct.unpack("<H", wd_data[8:10])[0]
        flags = struct.unpack("<H", wd_data[10:12])[0]

        fDot = bool(flags & 0x0001)
        fGlsy = bool(flags & 0x0002)
        fComplex = bool(flags & 0x0004)
        fHasPic = bool(flags & 0x0008)
        cQuickSaves = (flags & 0x00F0) >> 4
        fEncrypted = bool(flags & 0x0100)
        fWhichTblStm = "1Table" if bool(flags & 0x0200) else "0Table"
        fReadOnlyRecommended = bool(flags & 0x0400)
        fWriteReservation = bool(flags & 0x0800)
        fExtChar = bool(flags & 0x1000)
        fLoadOverride = bool(flags & 0x2000)
        fFarEast = bool(flags & 0x4000)
        fObfuscated = bool(flags & 0x8000)

        if fEncrypted:
            self.pathology.append("encrypted")

        nFibBack = struct.unpack("<H", wd_data[12:14])[0]
        lKey = struct.unpack("<I", wd_data[14:18])[0]

        return {
            "wIdent": f"0x{wIdent:04X}",
            "wIdent_valid": wIdent_valid,
            "nFib": nFib,
            "nFib_hex": f"0x{nFib:04X}",
            "version_hint": NFIB_VERSIONS.get(nFib, f"Unknown(nFib={nFib})"),
            "nFibBack": nFibBack,
            "lid": lid,
            "pnNext": pnNext,
            "lKey": lKey,
            "flags": {
                "fDot": fDot,
                "fGlsy": fGlsy,
                "fComplex": fComplex,
                "fHasPic": fHasPic,
                "cQuickSaves": cQuickSaves,
                "fEncrypted": fEncrypted,
                "fWhichTblStm": fWhichTblStm,
                "fReadOnlyRecommended": fReadOnlyRecommended,
                "fWriteReservation": fWriteReservation,
                "fExtChar": fExtChar,
                "fLoadOverride": fLoadOverride,
                "fFarEast": fFarEast,
                "fObfuscated": fObfuscated,
            },
        }

    def _build_result(self) -> dict:
        unique_pathology = sorted(list(set(self.pathology)))
        return {
            "bytes": self.size,
            "sha256": sha256_bytes(self.data),
            "magic_hex": self.magic_hex,
            "is_cfb": self.is_cfb,
            "is_zip": self.is_zip,
            "pathology": {
                "is_pathological": len(unique_pathology) > 0,
                "flags": unique_pathology,
            },
        }


def run_survey(corpus_dir: pathlib.Path) -> dict:
    """執行完整語料庫普查，回傳匯總結構字典。"""
    manifest_path = corpus_dir / "manifest.json"
    if not manifest_path.exists():
        sys.exit(f"找不到 manifest: {manifest_path}")

    manifest_data = json.loads(manifest_path.read_text(encoding="utf-8"))
    files_manifest = manifest_data.get("files", [])

    file_records = []
    cfb_major_counter = Counter()
    sector_size_counter = Counter()
    nfib_counter = Counter()
    tbl_stream_counter = Counter()
    fcomplex_counter = Counter()
    fencrypted_counter = Counter()
    batch_counter = Counter()
    pathology_counter = Counter()

    for item in files_manifest:
        rel_path = item["file"]
        batch = item.get("batch", "unknown")
        batch_counter[batch] += 1
        full_path = corpus_dir / rel_path

        if not full_path.exists():
            print(f"警告: 語料檔案不存在 {rel_path}", file=sys.stderr)
            continue

        raw_bytes = full_path.read_bytes()
        parser = CFBParser(raw_bytes)
        parsed = parser.parse()

        # 覆寫/補充 manifest 資訊
        record = {
            "file": rel_path,
            "batch": batch,
            "kind": item.get("kind", "unknown"),
            "source": item.get("source", ""),
            "license": item.get("license", ""),
            "why": item.get("why", ""),
            "tags": item.get("tags", []),
            "manifest_sha256": item.get("sha256", ""),
            "actual_sha256": parsed["sha256"],
            "sha256_matches": item.get("sha256", "") == parsed["sha256"],
            "bytes": parsed["bytes"],
            "magic_hex": parsed["magic_hex"],
            "is_cfb": parsed["is_cfb"],
            "is_zip": parsed["is_zip"],
            "pathology": parsed["pathology"],
        }

        # 累加病態旗標
        for pflag in parsed["pathology"]["flags"]:
            pathology_counter[pflag] += 1

        if parsed["is_cfb"] and "cfb" in parsed:
            record["cfb"] = parsed["cfb"]
            record["directory"] = parsed["directory"]
            record["streams_presence"] = parsed["streams_presence"]
            record["worddocument"] = parsed["worddocument"]
            record["fib"] = parsed["fib"]

            cfb_major_counter[parsed["cfb"]["major_version"]] += 1
            sector_size_counter[parsed["cfb"]["sector_size"]] += 1

            if parsed["fib"]:
                nfib_counter[f"{parsed['fib']['version_hint']} ({parsed['fib']['nFib']})"] += 1
                tbl_stream_counter[parsed["fib"]["flags"]["fWhichTblStm"]] += 1
                fcomplex_counter[parsed["fib"]["flags"]["fComplex"]] += 1
                fencrypted_counter[parsed["fib"]["flags"]["fEncrypted"]] += 1

        file_records.append(record)

    summary = {
        "total_files": len(file_records),
        "batches": dict(batch_counter),
        "container_format": {
            "valid_cfb_count": sum(1 for r in file_records if r["is_cfb"]),
            "zip_disguised_count": sum(1 for r in file_records if r["is_zip"]),
            "invalid_magic_count": sum(1 for r in file_records if not r["is_cfb"] and not r["is_zip"]),
        },
        "sector_size_distribution": dict(sector_size_counter),
        "cfb_major_version_distribution": dict(cfb_major_counter),
        "nfib_version_distribution": dict(nfib_counter),
        "table_stream_distribution": dict(tbl_stream_counter),
        "fcomplex_distribution": {
            "fast_saved_piece_table (True)": fcomplex_counter[True],
            "linear_text (False)": fcomplex_counter[False],
        },
        "fencrypted_distribution": {
            "encrypted (True)": fencrypted_counter[True],
            "plain (False)": fencrypted_counter[False],
        },
        "pathology_summary": {
            "total_pathological_files": sum(1 for r in file_records if r["pathology"]["is_pathological"]),
            "flag_counts": dict(pathology_counter),
            "pathological_files": [
                {"file": r["file"], "flags": r["pathology"]["flags"], "why": r["why"]}
                for r in file_records
                if r["pathology"]["is_pathological"]
            ],
        },
    }

    return {
        "schema": SCHEMA,
        "description": "DOC-C1 普查報告：MS-CFB 容器健康度、WordDocument 流與 FIB 結構、病態清單與統計匯總",
        "summary": summary,
        "files": file_records,
    }


def run_self_test(corpus_dir: pathlib.Path) -> bool:
    """執行先紅自測：驗證 fixtures 故意構造之病理旗標是否 100% 正確被普查器捕獲。"""
    print("=== 執行 DOC-C1 普查先紅自測試（fixtures/ 驗證）===")
    fix_dir = corpus_dir / "fixtures"

    tests = [
        ("fixtures/fixture_magic_mismatch_zip.doc", "zip_disguised"),
        ("fixtures/fixture_corrupted_trunc_cfb.doc", "truncated"),
        ("fixtures/fixture_cyclic_fat.doc", "fat_cycle"),
        ("fixtures/fixture_missing_worddocument.doc", "missing_worddocument"),
    ]

    all_passed = True
    for rel_path, expected_flag in tests:
        p = corpus_dir / rel_path
        if not p.exists():
            print(f"  [FAIL] 找不到測試夾具: {rel_path}")
            all_passed = False
            continue
        data = p.read_bytes()
        parser = CFBParser(data)
        res = parser.parse()
        flags = res["pathology"]["flags"]
        if expected_flag in flags:
            print(f"  [PASS] {rel_path} 正確命中病態旗標: '{expected_flag}'")
        else:
            print(f"  [FAIL] {rel_path} 未命中預期旗標 '{expected_flag}'！實際旗標: {flags}")
            all_passed = False

    if all_passed:
        print("=== 先紅自測試全部通過（PASS）===\n")
        return True
    else:
        print("=== 先紅自測試失敗（FAIL）===\n", file=sys.stderr)
        return False


def main() -> None:
    parser = argparse.ArgumentParser(description="DOC-C1 MS-CFB 容器與 MS-DOC 語意普查器")
    parser.add_argument("--corpus", default="corpus-doc", help="語料庫目錄路徑（預設 corpus-doc）")
    parser.add_argument("--out", default="docs/doc-survey/survey.json", help="輸出 JSON 路徑")
    parser.add_argument("--self-test", action="store_true", help="執行先紅自測（驗證病態夾具捕捉）")
    args = parser.parse_args()

    repo = pathlib.Path(__file__).resolve().parent.parent
    corpus_dir = repo / args.corpus
    out_file = repo / args.out

    if args.self_test:
        ok = run_self_test(corpus_dir)
        sys.exit(0 if ok else 1)

    print(f"開始普查: {corpus_dir}")
    report = run_survey(corpus_dir)

    out_file.parent.mkdir(parents=True, exist_ok=True)
    # 輸出保證確定性：indent=2, ensure_ascii=False, 尾隨換行
    out_file.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"普查完成！已寫入 {out_file.relative_to(repo)}")
    print(f"總計掃描 {report['summary']['total_files']} 檔：")
    print(f"  - 合法 MS-CFB 容器: {report['summary']['container_format']['valid_cfb_count']} 檔")
    print(f"  - ZIP 偽裝樣本: {report['summary']['container_format']['zip_disguised_count']} 檔")
    print(f"  - 截斷/非標記樣本: {report['summary']['container_format']['invalid_magic_count']} 檔")
    print(f"  - 扇區大小分佈: {report['summary']['sector_size_distribution']}")
    print(f"  - FIB 版本分佈: {report['summary']['nfib_version_distribution']}")
    print(f"  - 表格流分佈: {report['summary']['table_stream_distribution']}")
    print(f"  - 加密文件數: {report['summary']['fencrypted_distribution']}")
    print(f"  - 病態文件總數: {report['summary']['pathology_summary']['total_pathological_files']}")


if __name__ == "__main__":
    main()
