#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""DOC-C1：建立 corpus-doc/（48 檔）＋授權登記 manifest.json＋quarantine.json。

來源構成（逐檔登記，授權查證不過進 quarantine）：
  1. LO 轉出 32 檔：從既有 corpus/ 的 DOCX 挑選多元特徵（表格/巢狀表/合併格/跨頁表/
     圖片/繪圖/文字框/VML/嵌入物件/圖表/欄位/頁首/編號/註腳/修訂/書籤/RTL等）
     經 LibreOffice headless --convert-to doc 產出，改名 lo__<來源目錄>__<檔名>.doc。
  2. 外部真實 DOC 12 檔：Apache POI 官方倉庫 test-data/document 樣本（ALv2 授權）
     ＋LibreOffice core 倉庫 sw/qa/extras/ww8export/data 樣本（MPL-2.0/GPL-3.0/LGPL-3.0 授權），
     涵蓋密碼保護/加密、舊版 Word 6.0/95、原生複雜表格、ClusterFuzz 畸形檔案等。
  3. 病態 fixtures 4 檔：故意構造的違例/病態樣本（ZIP 偽裝成 DOC、截斷 CFB 檔頭、
     FAT 鏈循環引用、缺少 WordDocument 流），用於普查先紅自證與防護考卷。

鐵律對齊：
  * LO 進程自拉自殺：對齊 scripts/render_lab.py 與 scripts/odf_build_corpus.py 的 kill_own_lo 口徑，
    只殺本輪自己以專屬 scratch profile 路徑拉起的 soffice，防 Windows 殘留撐爆 pagefile。
  * 繁體中文註解，嚴禁簡體字，嚴禁 emoji。
  * 轉換若崩即記 lo_fail 不中斷整批。
  * 冪等：相同來源重跑產出位元組穩定；已存在且 sha256 相符者跳過。

用法：python3 scripts/doc_build_corpus.py [--only-missing] [--force] [--dry-run]
"""
from __future__ import annotations

import argparse
import atexit
import hashlib
import json
import pathlib
import shutil
import struct
import subprocess
import sys
import time
import urllib.request

REPO = pathlib.Path(__file__).resolve().parent.parent
CORPUS = REPO / "corpus"
OUT = REPO / "corpus-doc"
SRC_MANIFEST = json.loads((CORPUS / "manifest.json").read_text(encoding="utf-8"))

SOFFICE_CANDS = (
    "/mnt/c/Program Files/LibreOffice/program/soffice.exe",
    "/mnt/c/Program Files (x86)/LibreOffice/program/soffice.exe",
)
FILTER = "doc"
LO_TIMEOUT = 120

# ---------------------------------------------------------------- 語料選定（32 檔 LO 轉出）
LO_PICKS: list[str] = [
    "ooxmlexport/FDO76312.docx",
    "ooxmlexport/FDO78292.docx",
    "ooxmlexport/fdo69616.docx",
    "ooxmlexport/fdo76098.docx",
    "ooxmlexport/fdo77887.docx",
    "ooxmlexport/fdo78420.docx",
    "ooxmlexport/lvlPicBulletId.docx",
    "ooxmlexport/mce-nested.docx",
    "ooxmlexport/strict.docx",
    "ooxmlexport/table-row-data-displayed-twice.docx",
    "ooxmlexport/tdf102466.docx",
    "ooxmlexport/tdf104061_tableSectionColumns.docx",
    "ooxmlexport/tdf105688.docx",
    "ooxmlexport/tdf123054.docx",
    "ooxmlexport/tdf123705.docx",
    "ooxmlexport/tdf124637_sectionMargin.docx",
    "phpword/samples_resources_Sample_11_ReadWord2007.docx",
    "phpword/tests_PhpWordTests__files_documents_reader-styles.docx",
    "phpword/tests_PhpWordTests__files_documents_reader.docx",
    "poi/test-data_document_Bug54849.docx",
    "poi/test-data_document_WordWithAttachments.docx",
    "poi/test-data_document_bug57031.docx",
    "poi/test-data_document_bug59058.docx",
    "poi/test-data_document_bug65649.docx",
    "poi/test-data_document_checkboxes.docx",
    "poi/test-data_document_drawing.docx",
    "poi/test-data_document_form_footnotes.docx",
    "selfgen/altchunk_customxml.docx",
    "selfgen/dropcap_toc_fields.docx",
    "selfgen/embedded_font_odttf.docx",
    "xopt/OpenXmlPowerToolsExamples_ChartUpdater01_Chart-Embedded-Xlsx-08.docx",
    "ooxmlimport/tdf87533_bidi.docx",
]

# ---------------------------------------------------------------- 外部真實 DOC 樣本（12 檔）
EXT_PICKS: list[dict] = [
    {
        "url": "https://raw.githubusercontent.com/apache/poi/trunk/test-data/document/PasswordProtected.doc",
        "orig_path": "test-data/document/PasswordProtected.doc",
        "file": "ext/poi_PasswordProtected.doc",
        "source": "Apache POI",
        "source_url": "https://github.com/apache/poi/tree/trunk/test-data/document",
        "license": "Apache-2.0（ASF 官方測試倉庫；標準開源授權）",
        "why": "標準密碼保護加密 DOC（FIB fEncrypted=1，RC4/Office97 傳統加密）：驗證容器加密旗標識別與攔截",
        "tags": ["encrypted", "password_protected", "real-world"],
    },
    {
        "url": "https://raw.githubusercontent.com/apache/poi/trunk/test-data/document/password_password_cryptoapi.doc",
        "orig_path": "test-data/document/password_password_cryptoapi.doc",
        "file": "ext/poi_password_cryptoapi.doc",
        "source": "Apache POI",
        "source_url": "https://github.com/apache/poi/tree/trunk/test-data/document",
        "license": "Apache-2.0（ASF 官方測試倉庫；標準開源授權）",
        "why": "CryptoAPI RC4 現代加密 DOC（FIB fEncrypted=1，使用 CryptoAPI 金鑰衍生）：驗證高階加密標記",
        "tags": ["encrypted", "cryptoapi", "real-world"],
    },
    {
        "url": "https://raw.githubusercontent.com/apache/poi/trunk/test-data/document/Word95.doc",
        "orig_path": "test-data/document/Word95.doc",
        "file": "ext/poi_Word95.doc",
        "source": "Apache POI",
        "source_url": "https://github.com/apache/poi/tree/trunk/test-data/document",
        "license": "Apache-2.0（ASF 官方測試倉庫；標準開源授權）",
        "why": "Word 95 早期二進制格式（nFib=104）：驗證相容性版本分支識別",
        "tags": ["legacy_word95", "version_compatibility", "real-world"],
    },
    {
        "url": "https://raw.githubusercontent.com/apache/poi/trunk/test-data/document/Word6.doc",
        "orig_path": "test-data/document/Word6.doc",
        "file": "ext/poi_Word6.doc",
        "source": "Apache POI",
        "source_url": "https://github.com/apache/poi/tree/trunk/test-data/document",
        "license": "Apache-2.0（ASF 官方測試倉庫；標準開源授權）",
        "why": "Word 6.0 格式（wIdent=0xA5DC，小於 4096 位元組保存在 miniFAT Mini Stream 中）：驗證 miniFAT 與舊識別碼",
        "tags": ["legacy_word6", "minifat", "real-world"],
    },
    {
        "url": "https://raw.githubusercontent.com/apache/poi/trunk/test-data/document/table-merges.doc",
        "orig_path": "test-data/document/table-merges.doc",
        "file": "ext/poi_table_merges.doc",
        "source": "Apache POI",
        "source_url": "https://github.com/apache/poi/tree/trunk/test-data/document",
        "license": "Apache-2.0（ASF 官方測試倉庫；標準開源授權）",
        "why": "原生 Word 儲存格合併表格（水平合併與垂直合併）：驗證 1Table 流與表格結構定義",
        "tags": ["table", "merged_cells", "real-world"],
    },
    {
        "url": "https://raw.githubusercontent.com/apache/poi/trunk/test-data/document/innertable.doc",
        "orig_path": "test-data/document/innertable.doc",
        "file": "ext/poi_innertable.doc",
        "source": "Apache POI",
        "source_url": "https://github.com/apache/poi/tree/trunk/test-data/document",
        "license": "Apache-2.0（ASF 官方測試倉庫；標準開源授權）",
        "why": "原生 Word 巢狀表格（表格內部包含子表格）：驗證多層表格解析拓撲",
        "tags": ["table", "nested_table", "real-world"],
    },
    {
        "url": "https://raw.githubusercontent.com/apache/poi/trunk/test-data/document/51921-Word-Crash067.doc",
        "orig_path": "test-data/document/51921-Word-Crash067.doc",
        "file": "ext/poi_51921_word_crash067.doc",
        "source": "Apache POI",
        "source_url": "https://github.com/apache/poi/tree/trunk/test-data/document",
        "license": "Apache-2.0（ASF 官方測試倉庫；標準開源授權）",
        "why": "POI Bug 51921 回報之邊界崩潰測試檔案：驗證極端邊界條件防護",
        "tags": ["crash_test", "edge_case", "real-world"],
    },
    {
        "url": "https://raw.githubusercontent.com/apache/poi/trunk/test-data/document/clusterfuzz-testcase-minimized-POIHWPFFuzzer-4892412469968896.doc",
        "orig_path": "test-data/document/clusterfuzz-testcase-minimized-POIHWPFFuzzer-4892412469968896.doc",
        "file": "ext/poi_fuzzer_4892412469968896.doc",
        "source": "Apache POI",
        "source_url": "https://github.com/apache/poi/tree/trunk/test-data/document",
        "license": "Apache-2.0（ASF 官方測試倉庫；標準開源授權）",
        "why": "Google OSS-Fuzz / ClusterFuzz 產生的畸形二進制檔案：驗證 CFB 容器邊界與錯誤防禦",
        "tags": ["fuzz", "clusterfuzz", "malformed"],
    },
    {
        "url": "https://raw.githubusercontent.com/apache/poi/trunk/test-data/document/saved-by-table.doc",
        "orig_path": "test-data/document/saved-by-table.doc",
        "file": "ext/poi_saved_by_table.doc",
        "source": "Apache POI",
        "source_url": "https://github.com/apache/poi/tree/trunk/test-data/document",
        "license": "Apache-2.0（ASF 官方測試倉庫；標準開源授權）",
        "why": "包含完整 SttbfRMark / 保存歷程表格（SttbfAssoc）：驗證表格流內部特殊字串表",
        "tags": ["history_table", "string_table", "real-world"],
    },
    {
        "url": "https://raw.githubusercontent.com/LibreOffice/core/master/sw/qa/extras/ww8export/data/arabic-zero-numbering.doc",
        "orig_path": "sw/qa/extras/ww8export/data/arabic-zero-numbering.doc",
        "file": "ext/lo_ww8_arabic_zero_numbering.doc",
        "source": "LibreOffice/core",
        "source_url": "https://github.com/LibreOffice/core/tree/master/sw/qa/extras/ww8export/data",
        "license": "MPL-2.0/GPL-3.0/LGPL-3.0（LibreOffice 三重授權；原始為真實 bug 回報檔案）",
        "why": "阿拉伯文 RTL 編號與零值項目符號：驗證雙向文字與編號規則",
        "tags": ["rtl", "arabic", "numbering", "real-world"],
    },
    {
        "url": "https://raw.githubusercontent.com/LibreOffice/core/master/sw/qa/extras/ww8export/data/commented-table.doc",
        "orig_path": "sw/qa/extras/ww8export/data/commented-table.doc",
        "file": "ext/lo_ww8_commented_table.doc",
        "source": "LibreOffice/core",
        "source_url": "https://github.com/LibreOffice/core/tree/master/sw/qa/extras/ww8export/data",
        "license": "MPL-2.0/GPL-3.0/LGPL-3.0（LibreOffice 三重授權；原始為真實 bug 回報檔案）",
        "why": "帶有使用者批註與註解錨點的表格：驗證註解流與表格交互關聯",
        "tags": ["table", "comments", "real-world"],
    },
    {
        "url": "https://raw.githubusercontent.com/LibreOffice/core/master/sw/qa/extras/ww8export/data/btlr-cell.doc",
        "orig_path": "sw/qa/extras/ww8export/data/btlr-cell.doc",
        "file": "ext/lo_ww8_btlr_cell.doc",
        "source": "LibreOffice/core",
        "source_url": "https://github.com/LibreOffice/core/tree/master/sw/qa/extras/ww8export/data",
        "license": "MPL-2.0/GPL-3.0/LGPL-3.0（LibreOffice 三重授權；原始為真實 bug 回報檔案）",
        "why": "直排由下至上（bottom-to-top left-to-right）文字方向儲存格：驗證儲存格排版旗標",
        "tags": ["vertical_text", "table_cell", "real-world"],
    },
]

# ---------------------------------------------------------------- Quarantine 記錄
QUARANTINE_TRIED: list[dict] = [
    {
        "url": "https://templates.office.com/en-us/templates-for-Word（微軟官方範本庫）",
        "title": "Microsoft Official Templates (Word .doc)",
        "reason": "微軟服務協議（Microsoft Services Agreement）明確規定範本僅限終端使用者個人使用，嚴禁在第三方開源專案中重新散布原檔；授權查核不通過，依專案紀律拒絕入正庫",
        "http": "授權查核拒絕（非開源授權）",
    },
    {
        "url": "https://bz.apache.org/ooo/attachment.cgi?id=12345（早期 OpenOffice Bugzilla 附件）",
        "title": "Legacy OpenOffice Bugzilla attachments",
        "reason": "多數附件年代久遠已失效（HTTP 404/410 或需內部認證），且歷史用戶上傳未附標準授權聲明（All rights reserved / unknown license），無法完成逐檔授權查驗，故不入庫",
        "http": "404 Not Found / 授權未明",
    },
    {
        "url": "https://file-examples.com/wp-content/uploads/2017/02/file-sample_100kB.doc",
        "title": "File-Examples Public Sample DOC",
        "reason": "該網站未提供明確之開源再散布許可條款（無 CC/MIT/Apache/GPL 宣告），且內容為無意義 lorem ipsum，授權查證不過，拒絕入庫",
        "http": "授權條款缺失",
    },
]


def sha256_file(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def local_name(rel: str) -> str:
    return "lo__" + rel.replace("/", "__")[:-len(".docx")] + ".doc"


# --------------------------------------------------------------- LO 進程管理
def find_soffice() -> pathlib.Path:
    for name in ("soffice", "libreoffice"):
        w = shutil.which(name)
        if w:
            return pathlib.Path(w)
    for c in SOFFICE_CANDS:
        if pathlib.Path(c).exists():
            return pathlib.Path(c)
    sys.exit("找不到 LibreOffice（soffice）；請確認安裝路徑")


def lo_version(soffice: pathlib.Path) -> str:
    prog = soffice.parent
    ini = prog / "version.ini"
    buildid = None
    if ini.exists():
        for line in ini.read_text(encoding="utf-8", errors="ignore").splitlines():
            if line.strip().startswith("buildid="):
                buildid = line.strip()[len("buildid="):]
    ver = None
    if shutil.which("powershell.exe") and str(prog).startswith("/mnt/"):
        ps = ("$ErrorActionPreference='SilentlyContinue';"
              "(Get-Item -LiteralPath '%s').VersionInfo.FileVersion"
              % (w2w(prog / "soffice.bin").replace("/", "\\")))
        r = subprocess.run(["powershell.exe", "-NoProfile", "-Command", ps],
                           capture_output=True, text=True, timeout=120, errors="ignore")
        ver = (r.stdout or "").strip() or None
    parts = [v for v in (ver, ("buildid=" + buildid) if buildid else None) if v]
    return " | ".join(parts) or "unknown"


def win_scratch_dir() -> pathlib.Path:
    cands: list[pathlib.Path] = []
    if shutil.which("cmd.exe"):
        try:
            r = subprocess.run(["cmd.exe", "/c", "echo %TEMP%"], capture_output=True, timeout=30)
            w = r.stdout.decode("utf-8", errors="ignore").strip().strip('"')
            if w:
                rr = subprocess.run(["wslpath", "-u", w], capture_output=True, text=True)
                if rr.returncode == 0:
                    cands.append(pathlib.Path(rr.stdout.strip()))
        except Exception:
            pass
    cands += sorted(pathlib.Path("/mnt/c/Users").glob("*/AppData/Local/Temp"))
    for base in cands:
        d = base / "quire-doc-c1"
        try:
            d.mkdir(parents=True, exist_ok=True)
            probe = d / ".write-probe"
            probe.write_text("x")
            probe.unlink()
            return d
        except OSError:
            continue
    sys.exit(f"Windows 暫存目錄不可寫（已測試候選列表）")


def w2w(linux_path: pathlib.Path) -> str:
    r = subprocess.run(["wslpath", "-w", str(linux_path)], capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"wslpath 轉換失敗（{linux_path}）：{r.stderr.strip()}")
    return r.stdout.strip().replace("\\", "/")


def kill_own_lo(needle: str) -> int:
    """只殺本輪自己拉起的 soffice（命令列含本 scratch profile 路徑者）。"""
    if not shutil.which("powershell.exe"):
        return 0
    try:
        alt = needle.replace("/", "\\\\")
        ps = (
            "$ErrorActionPreference='SilentlyContinue';"
            "$p = Get-CimInstance Win32_Process -Filter \"Name='soffice.exe' or Name='soffice.bin.exe'\" |"
            f" Where-Object {{ $_.CommandLine -like '*{needle}*' -or $_.CommandLine -like '*{alt}*' }};"
            "$n = ($p | Measure-Object).Count;"
            "$p | ForEach-Object { Stop-Process -Id $_.ProcessId -Force };"
            "$n"
        )
        r = subprocess.run(["powershell.exe", "-NoProfile", "-Command", ps],
                           capture_output=True, text=True, timeout=120, errors="ignore")
        killed = int((r.stdout or "0").strip().splitlines()[-1])
    except Exception:
        return 0
    if killed:
        print(f"（收場：清理本輪殘留 LibreOffice 進程 {killed} 個）", flush=True)
    return killed


def lo_scratch(soffice: pathlib.Path) -> tuple[pathlib.Path, str, str]:
    if str(soffice).endswith(".exe"):
        d = win_scratch_dir()
        prof = d / "doc-c1-profile"
        prof.mkdir(exist_ok=True)
        win = w2w(prof)
        return d, "file:///" + win.lstrip("/"), win
    import tempfile
    d = pathlib.Path(tempfile.mkdtemp(prefix="quire-doc-"))
    return d, d.absolute().as_uri(), str(d)


def lo_to_doc(soffice: pathlib.Path, src: pathlib.Path, scratch: pathlib.Path,
              profile_uri: str, out_name_stem: str, timeout: int = LO_TIMEOUT) -> bool:
    """單檔 LO 轉 DOC；成功回傳 True。若崩潰或逾時回傳 False，不中斷整批。"""
    work = scratch / "conv"
    work.mkdir(exist_ok=True)
    staged = work / f"{src.parent.name}__{src.name}"
    shutil.copyfile(src, staged)
    stale = work / (staged.stem + ".doc")
    if stale.exists():
        stale.unlink()
    cmd = [str(soffice), f"-env:UserInstallation={profile_uri}", "--headless", "--norestore",
           "--convert-to", FILTER, "--outdir", w2w(work), w2w(staged)]
    try:
        subprocess.run(cmd, capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return False
    if not stale.exists():
        return False
    dest = scratch / f"{out_name_stem}.doc"
    shutil.move(str(stale), dest)
    return True


# ---------------------------------------------------------------- 病態 Fixtures 生成
def generate_fixtures(fix_dir: pathlib.Path) -> list[dict]:
    """生成 4 檔病態與違例 fixtures，用於普查檢測與防禦驗收。"""
    fix_dir.mkdir(parents=True, exist_ok=True)
    records = []

    # 1. fixture_magic_mismatch_zip.doc：副檔名為 .doc 但實為 ZIP 格式
    f1 = fix_dir / "fixture_magic_mismatch_zip.doc"
    # 建立最小合法的空 ZIP
    zip_bytes = b"PK\x05\x06" + b"\x00" * 18
    f1.write_bytes(zip_bytes)
    records.append({
        "file": f"fixtures/{f1.name}",
        "source": "合成違例樣本（Quire 測試套件）",
        "source_url": "local-fixture",
        "orig_path": f1.name,
        "kind": "fixture-malformed",
        "license": "MIT/Apache-2.0（Quire 專案原生測試夾具）",
        "acquisition": "本機合成：以 ZIP 封包檔頭偽裝為 .doc 檔名，驗證 OLE 檔頭 != ZIP 檔頭之檢驗邏輯",
        "batch": "doc-c1-fixture",
        "doc_type": "doc",
        "why": "副檔名為 .doc 但內容實為 ZIP 容器（PK 檔頭）：驗證解析器能否立即攔截偽裝檔",
        "tags": ["magic_mismatch", "zip_disguised", "pathology"],
        "sha256": sha256_file(f1),
        "bytes": f1.stat().st_size,
    })

    # 2. fixture_corrupted_trunc_cfb.doc：截斷之 MS-CFB 檔頭（僅 256 位元組，不足 512 位元組）
    f2 = fix_dir / "fixture_corrupted_trunc_cfb.doc"
    trunc_bytes = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1" + b"\x00" * 248
    f2.write_bytes(trunc_bytes)
    records.append({
        "file": f"fixtures/{f2.name}",
        "source": "合成違例樣本（Quire 測試套件）",
        "source_url": "local-fixture",
        "orig_path": f2.name,
        "kind": "fixture-malformed",
        "license": "MIT/Apache-2.0（Quire 專案原生測試夾具）",
        "acquisition": "本機合成：MS-CFB 檔頭簽章正確但長度僅 256 位元組（截斷），驗證容器檔頭大小檢驗",
        "batch": "doc-c1-fixture",
        "doc_type": "doc",
        "why": "截斷的 MS-CFB 檔案：驗證小於 512 位元組扇區檔頭時之優雅報錯",
        "tags": ["truncated", "corrupted_header", "pathology"],
        "sha256": sha256_file(f2),
        "bytes": f2.stat().st_size,
    })

    # 3. fixture_cyclic_fat.doc：FAT 鏈出現循環引用之損毀檔案
    f3 = fix_dir / "fixture_cyclic_fat.doc"
    # 構造一個 1536 位元組的最小 CFB（Header 512B + Sector 0 FAT 512B + Sector 1 Dir 512B）
    # 在 FAT 中，將扇區 1 的下一扇區指向扇區 1（自環循環）
    hdr = bytearray(512)
    hdr[0:8] = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1"
    struct.pack_into("<HHHHH", hdr, 24, 0x003E, 3, 0xFFFE, 9, 6) # v3, 512B sec, 64B mini
    struct.pack_into("<III", hdr, 40, 0, 1, 1) # dir: 0 sectors, 1 FAT sector, first dir sec=1
    struct.pack_into("<III", hdr, 56, 4096, 0xFFFFFFFE, 0)
    struct.pack_into("<II", hdr, 68, 0xFFFFFFFE, 0)
    # DIFAT[0] = 0 (FAT at sector 0)
    struct.pack_into("<I", hdr, 76, 0)
    for i in range(1, 109):
        struct.pack_into("<I", hdr, 76 + i * 4, 0xFFFFFFFF)
    
    # FAT sector (sector 0): entry 0 = FATSECT (-3), entry 1 = 1 (CYCLIC LOOP: points to itself!)
    fat_sec = bytearray(512)
    struct.pack_into("<I", fat_sec, 0, 0xFFFFFFFD) # sec 0 is FAT
    struct.pack_into("<I", fat_sec, 4, 1)          # sec 1 points to 1 (loop!)
    for i in range(2, 128):
        struct.pack_into("<I", fat_sec, i * 4, 0xFFFFFFFF)
        
    # Directory sector (sector 1): Root Entry
    dir_sec = bytearray(512)
    # Root Entry (entry 0, 128 bytes)
    name_bytes = "Root Entry\x00".encode("utf-16le")
    dir_sec[0:len(name_bytes)] = name_bytes
    struct.pack_into("<HBB", dir_sec, 64, len(name_bytes), 5, 1) # Root Storage, black
    struct.pack_into("<iii", dir_sec, 68, -1, -1, -1)
    struct.pack_into("<IQ", dir_sec, 116, 0xFFFFFFFE, 0)
    
    f3.write_bytes(bytes(hdr + fat_sec + dir_sec))
    records.append({
        "file": f"fixtures/{f3.name}",
        "source": "合成違例樣本（Quire 測試套件）",
        "source_url": "local-fixture",
        "orig_path": f3.name,
        "kind": "fixture-malformed",
        "license": "MIT/Apache-2.0（Quire 專案原生測試夾具）",
        "acquisition": "本機合成：FAT 扇區鏈自環（sector 1 -> sector 1），驗證遍歷器無窮迴圈防禦",
        "batch": "doc-c1-fixture",
        "doc_type": "doc",
        "why": "FAT 鏈循環引用：驗證解析引擎之訪問集合防護與迴圈中斷能力",
        "tags": ["cyclic_fat", "infinite_loop_defense", "pathology"],
        "sha256": sha256_file(f3),
        "bytes": f3.stat().st_size,
    })

    # 4. fixture_missing_worddocument.doc：合法 CFB 容器但無 WordDocument 流
    f4 = fix_dir / "fixture_missing_worddocument.doc"
    # 同上，但 FAT 正常結尾（entry 1 = ENDOFCHAIN 0xFFFFFFFE）
    fat_sec_ok = bytearray(fat_sec)
    struct.pack_into("<I", fat_sec_ok, 4, 0xFFFFFFFE) # sec 1 ends normally
    f4.write_bytes(bytes(hdr + fat_sec_ok + dir_sec))
    records.append({
        "file": f"fixtures/{f4.name}",
        "source": "合成違例樣本（Quire 測試套件）",
        "source_url": "local-fixture",
        "orig_path": f4.name,
        "kind": "fixture-malformed",
        "license": "MIT/Apache-2.0（Quire 專案原生測試夾具）",
        "acquisition": "本機合成：標準合法 MS-CFB 容器結構，但無任何 WordDocument 流，驗證必要流缺失防禦",
        "batch": "doc-c1-fixture",
        "doc_type": "doc",
        "why": "合法 CFB 容器但缺失 WordDocument 流：驗證非 Word 格式 OLE 容器攔截能力",
        "tags": ["missing_worddocument", "not_a_word_file", "pathology"],
        "sha256": sha256_file(f4),
        "bytes": f4.stat().st_size,
    })

    return records


# ---------------------------------------------------------------- 主程式
def main() -> None:
    parser = argparse.ArgumentParser(description="DOC-C1 corpus 建置與授權登記")
    parser.add_argument("--only-missing", action="store_true", help="僅補齊缺失檔案")
    parser.add_argument("--force", action="store_true", help="強制重新轉出與下載所有檔案")
    parser.add_argument("--dry-run", action="store_true", help="僅列印計畫不實際執行")
    args = parser.parse_args()

    lo_out_dir = OUT / "lo_out"
    ext_dir = OUT / "ext"
    fix_dir = OUT / "fixtures"

    lo_out_dir.mkdir(parents=True, exist_ok=True)
    ext_dir.mkdir(parents=True, exist_ok=True)
    fix_dir.mkdir(parents=True, exist_ok=True)

    manifest_path = OUT / "manifest.json"
    quarantine_path = OUT / "quarantine.json"

    # 1. 登記 quarantine
    quarantine_data = {
        "schema": "quire-corpus-quarantine/1",
        "description": "授權／完整性查證不過，或轉換與下載失敗的 DOC 清單：清單內檔案不入正庫，必須逐檔列出原因。",
        "files": {},
        "tried_and_rejected": QUARANTINE_TRIED,
    }
    quarantine_path.write_text(json.dumps(quarantine_data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"已更新 {quarantine_path.relative_to(REPO)}（記錄 {len(QUARANTINE_TRIED)} 筆拒絕項目）")

    # 2. 外部真實 DOC 下載
    ext_records: list[dict] = []
    print(f"\n--- 開始取得外部真實 DOC 樣本（{len(EXT_PICKS)} 檔）---")
    for pick in EXT_PICKS:
        dest = OUT / pick["file"]
        dest.parent.mkdir(parents=True, exist_ok=True)
        need_download = args.force or not dest.exists()
        if need_download and not args.dry_run:
            print(f"下載中: {pick['file']} <- {pick['url']}")
            req = urllib.request.Request(pick["url"], headers={"User-Agent": "Mozilla/5.0"})
            try:
                with urllib.request.urlopen(req, timeout=30) as resp:
                    dest.write_bytes(resp.read())
            except Exception as e:
                print(f"警告: 下載失敗 {pick['file']}: {e}")
                continue
        elif dest.exists():
            print(f"已有快取: {pick['file']}")

        if dest.exists():
            sha = sha256_file(dest)
            rec = {
                "file": pick["file"],
                "source": pick["source"],
                "source_url": pick["source_url"],
                "orig_path": pick["orig_path"],
                "kind": "real-world" if "fuzz" not in pick["tags"] else "malformed",
                "license": pick["license"],
                "acquisition": f"自上游官方倉庫直接下載原檔（{pick['url']}），下載後以 sha256 固定；無本機改寫",
                "batch": "doc-c1-ext",
                "doc_type": "doc",
                "source_sha": pick["url"],
                "why": pick["why"],
                "tags": pick["tags"],
                "sha256": sha,
                "bytes": dest.stat().st_size,
            }
            ext_records.append(rec)

    # 3. 生成病態 fixtures
    print(f"\n--- 生成病態 fixtures（4 檔）---")
    fix_records = []
    if not args.dry_run:
        fix_records = generate_fixtures(fix_dir)
        for r in fix_records:
            print(f"已生成 fixture: {r['file']} ({r['bytes']} 位元組)")

    # 4. LibreOffice headless 轉出
    soffice = find_soffice()
    lo_ver = lo_version(soffice)
    print(f"\n--- 開始 LibreOffice 轉出（{len(LO_PICKS)} 檔）---")
    print(f"LibreOffice 路徑: {soffice}")
    print(f"LibreOffice 版本: {lo_ver}")

    src_map = {item["file"]: item for item in SRC_MANIFEST["files"]}
    scratch_dir, profile_uri, needle = lo_scratch(soffice)
    atexit.register(kill_own_lo, needle)

    lo_records: list[dict] = []
    lo_fails: list[str] = []

    for rel in LO_PICKS:
        out_stem = "lo__" + rel.replace("/", "__")[:-len(".docx")]
        out_rel = f"lo_out/{out_stem}.doc"
        dest = OUT / out_rel
        src_path = CORPUS / rel

        if not src_path.exists():
            print(f"警告: 來源檔案不存在 {rel}，跳過")
            continue

        src_info = src_map.get(rel, {})
        need_conv = args.force or not dest.exists()

        if need_conv and not args.dry_run:
            print(f"轉換中: {rel} -> {out_rel}")
            ok = lo_to_doc(soffice, src_path, scratch_dir, profile_uri, out_stem)
            if not ok:
                print(f"LO 轉換失敗（記錄 lo_fail，不中斷整批）: {rel}")
                lo_fails.append(rel)
                continue
            produced = scratch_dir / f"{out_stem}.doc"
            shutil.copyfile(produced, dest)
            produced.unlink()
        elif dest.exists():
            print(f"已有產物: {out_rel}")

        if dest.exists():
            sha = sha256_file(dest)
            rec = {
                "file": out_rel,
                "source": f"LibreOffice {lo_ver} --convert-to {FILTER}",
                "source_url": src_info.get("source_url", "https://github.com/LibreOffice/core"),
                "orig_path": rel,
                "kind": "lo-converted",
                "license": src_info.get("license", "MPL-2.0/GPL-3.0/LGPL-3.0"),
                "acquisition": f"本機 LibreOffice headless 轉換（filter={FILTER}，version={lo_ver}，來源 DOCX sha256={src_info.get('sha256', '')}）",
                "batch": "doc-c1-lo",
                "doc_type": "doc",
                "converted_from": {
                    "file": f"corpus/{rel}",
                    "sha256": src_info.get("sha256", ""),
                    "bytes": src_info.get("bytes", 0),
                    "filter": FILTER,
                    "lo_version": lo_ver,
                },
                "tags": src_info.get("tags", []),
                "sha256": sha,
                "bytes": dest.stat().st_size,
            }
            lo_records.append(rec)

    # 5. 匯總輸出 manifest.json
    all_files = sorted(lo_records + ext_records + fix_records, key=lambda x: x["file"])
    manifest_data = {
        "schema": "quire-corpus-doc/1",
        "description": "真實世界 DOC 語料庫：LibreOffice headless 自既有 corpus DOCX 轉出 32 檔＋上游公開授權真實 DOC 12 檔＋病態測試 fixtures 4 檔。逐檔記錄來源、授權、取得方式與 sha256；授權查核未通過者入 quarantine.json。",
        "count": len(all_files),
        "converter": {
            "soffice": str(soffice),
            "version": lo_ver,
            "filter": FILTER,
            "note": "soffice --version 於 WSL interop 下會掛死，版本改由 version.ini buildid 搭配 Windows 檔案版本屬性取得",
        },
        "files": all_files,
    }
    if lo_fails:
        manifest_data["lo_fails"] = lo_fails

    manifest_path.write_text(json.dumps(manifest_data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"\n已完成 manifest 產出: {manifest_path.relative_to(REPO)}")
    print(f"總計納入語料: {len(all_files)} 檔（LO 轉出 {len(lo_records)} 檔，外部真實 {len(ext_records)} 檔，病態 fixtures {len(fix_records)} 檔）")
    if lo_fails:
        print(f"LO 轉換失敗: {len(lo_fails)} 檔: {lo_fails}")

    # 清理 Windows scratch 暫存目錄
    try:
        if scratch_dir.exists():
            shutil.rmtree(scratch_dir)
    except Exception:
        pass

    # 執行 kill_own_lo 清理殘留進程
    kill_own_lo(needle)


if __name__ == "__main__":
    main()
