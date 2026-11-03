#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""ODF-C1：建立 corpus-odt/（50 檔）＋授權登記 manifest.json＋quarantine.json。

兩路來源（逐檔登記，授權查證不過進 quarantine）：
  1. LO 轉出 40 檔：從既有 corpus/ 的 DOCX 挑多樣（表格/圖片/欄位/頁首/目錄/修訂/嵌入字型…）
     經 LibreOffice headless --convert-to odt 產出，改名 lo__<來源目錄>__<檔名>.odt。
  2. 外部真實 ODT 10 檔：LibreOffice/core 倉庫 sw|chart2 QA 資料（MPL-2.0/GPL-3.0/LGPL-3.0
     三重授權，與既有 corpus/ 的 LibreOffice 授權口徑一致）＋OASIS ODF 1.3 正式規範文件
     （OASIS 標準軌版權政策允許未修改再散布）。

鐵律對齊：
  * LO 進程自拉自殺＝scripts/render_lab.py 的 kill_stale_lo + kill_own_lo 口徑（本檔自備
    一份不 import render_lab，因它 top-level 就要 pymupdf；邏輯逐行鏡像）。
  * `soffice.exe --version` 在 WSL interop 下會**掛死**（本卡實測 180s 逾時兩次），
    故轉換器版本改由 version.ini ＋ Windows 檔案版本屬性取得，不呼叫 --version。
  * 產出前逐檔 PII 掃描＋消毒（鏡像 scripts/scrub_corpus.py：docProps creator/company ＋
    email 遮罩），並把本機路徑（C:\\Users\\<user>、/home/<user>、\\wsl.localhost）一併遮罩。
  * 冪等：同一來源（DOCX sha256＋LO 版本）重跑產出位元組穩定；已存在且 sha256 相符者跳過。

用法：python3 scripts/odf_build_corpus.py [--only-missing] [--force] [--dry-run]
"""
from __future__ import annotations

import argparse
import atexit
import hashlib
import json
import pathlib
import re
import shutil
import subprocess
import sys
import time
import urllib.parse
import urllib.request
import zipfile

REPO = pathlib.Path(__file__).resolve().parent.parent
CORPUS = REPO / "corpus"
OUT = REPO / "corpus-odt"
SRC_MANIFEST = json.loads((CORPUS / "manifest.json").read_text(encoding="utf-8"))

SOFFICE_CANDS = (
    "/mnt/c/Program Files/LibreOffice/program/soffice.exe",
    "/mnt/c/Program Files (x86)/LibreOffice/program/soffice.exe",
)
FILTER = "odt:writer8"
LO_TIMEOUT = 180

# ---------------------------------------------------------------- 語料選定（冪等錨點）
# 40 檔 LO 轉出：由 corpus/manifest.json 的 tag 全自動盤點後人工審定，覆蓋
# 表格/巢狀表/合併格/跨頁表/圖片/繪圖/文字框/VML/嵌入物件/圖表/欄位/頁首/編號/
# 註腳/尾註/修訂/書籤/超鏈/東亞/RTL/節/上下標/螢光/刪除線/段落框線/內容控制項/
# 自訂 XML/詞彙/公式/符號/嵌入字型/altchunk/首字下沉/SmartArt/範本。
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
    "xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Mercury.docx",
    "xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Neptune.docx",
    "xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Pluto.docx",
    "xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Saturn.docx",
    "xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_SolarOverview.docx",
    "xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Sun.docx",
    "xopt/OpenXmlPowerToolsExamples_HtmlConverter01_Test-03.docx",
    "xopt/TestFiles_HC031-Complicated-Document.docx",
    # 原選 xopt/TestFiles_RP_RP051-Arabic.docx：LO 對其**掛死逾時**（實測 300s 無產出，
    # 見 docs/odf-survey/README.md 第 6 節病態記錄），改由 ooxmlimport/tdf87533_bidi.docx
    # 承擔 RTL/bidi 覆蓋（同一批 tag 掃描選出，轉換實測正常）。
    "ooxmlimport/tdf87533_bidi.docx",
]

# 外部真實 ODT：授權一律可查證（LibreOffice 三重授權＝倉庫 LICENSE.MPL3/LGPL3＋
# sw/qa 為 tdf bugzilla 匿名化真實用戶文件；OASIS 標準軌＝OASIS 版權政策允許未修改再散布）。
EXT_PICKS: list[dict] = [
    {
        "url": "https://raw.githubusercontent.com/LibreOffice/core/master/"
               "sw/qa/extras/odfexport/data/sha1_correct.odt",
        "orig_path": "sw/qa/extras/odfexport/data/sha1_correct.odt",
        "file": "ext/lo_core_sw_qa_odfexport_sha1_correct.odt",
        "source": "LibreOffice/core",
        "source_url": "https://github.com/LibreOffice/core/tree/master/sw/qa/extras/odfexport/data",
        "license": "MPL-2.0/GPL-3.0/LGPL-3.0（LibreOffice 三重授權；sw/qa 為 tdf bugzilla 匿名化真實用戶文件）",
        "why": "真加密 ODT 鏡像：META-INF/manifest.xml 帶 manifest:encryption-data"
               "（Blowfish CFB＋PBKDF2／SHA1/1K 摘要），是「加密 entry 不得當成可解析 XML」的考卷",
        "tags": ["encrypted", "real-world"],
    },
    {
        "url": "https://raw.githubusercontent.com/LibreOffice/core/master/"
               "sw/qa/extras/odfexport/data/sha1_wrong.odt",
        "orig_path": "sw/qa/extras/odfexport/data/sha1_wrong.odt",
        "file": "ext/lo_core_sw_qa_odfexport_sha1_wrong.odt",
        "source": "LibreOffice/core",
        "source_url": "https://github.com/LibreOffice/core/tree/master/sw/qa/extras/odfexport/data",
        "license": "MPL-2.0/GPL-3.0/LGPL-3.0（LibreOffice 三重授權；sw/qa 為 tdf bugzilla 匿名化真實用戶文件）",
        "why": "加密＋摘要故意錯誤（檔名即 sha1_wrong）：解密後摘要不符的病態，測容器是否老實報錯",
        "tags": ["encrypted", "checksum_mismatch", "real-world"],
    },
    {
        "url": "https://raw.githubusercontent.com/LibreOffice/core/master/"
               "sw/qa/extras/odfexport/data/tdf140437.odt",
        "orig_path": "sw/qa/extras/odfexport/data/tdf140437.odt",
        "file": "ext/lo_core_sw_qa_odfexport_tdf140437.odt",
        "source": "LibreOffice/core",
        "source_url": "https://github.com/LibreOffice/core/tree/master/sw/qa/extras/odfexport/data",
        "license": "MPL-2.0/GPL-3.0/LGPL-3.0（LibreOffice 三重授權；sw/qa 為 tdf bugzilla 匿名化真實用戶文件）",
        "why": "mimetype 非首 entry（首個是 Configurations2/accelerator/）——ODF 1.3 §3.3 違例實據",
        "tags": ["mimetype_violation", "real-world"],
    },
    {
        "url": "https://raw.githubusercontent.com/LibreOffice/core/master/"
               "sw/qa/extras/odfexport/data/tdf77961.odt",
        "orig_path": "sw/qa/extras/odfexport/data/tdf77961.odt",
        "file": "ext/lo_core_sw_qa_odfexport_tdf77961.odt",
        "source": "LibreOffice/core",
        "source_url": "https://github.com/LibreOffice/core/tree/master/sw/qa/extras/odfexport/data",
        "license": "MPL-2.0/GPL-3.0/LGPL-3.0（LibreOffice 三重授權；sw/qa 為 tdf bugzilla 匿名化真實用戶文件）",
        "why": "mimetype 既非首 entry 又被壓縮（首個是 styles.xml）——§3.3 雙重違例",
        "tags": ["mimetype_violation", "mimetype_compressed", "real-world"],
    },
    {
        "url": "https://raw.githubusercontent.com/LibreOffice/core/master/"
               "sw/qa/extras/odfexport/data/tdf152710.odt",
        "orig_path": "sw/qa/extras/odfexport/data/tdf152710.odt",
        "file": "ext/lo_core_sw_qa_odfexport_tdf152710.odt",
        "source": "LibreOffice/core",
        "source_url": "https://github.com/LibreOffice/core/tree/master/sw/qa/extras/odfexport/data",
        "license": "MPL-2.0/GPL-3.0/LGPL-3.0（LibreOffice 三重授權；sw/qa 為 tdf bugzilla 匿名化真實用戶文件）",
        "why": "mimetype 非首 entry＋content.xml 帶多個外部 http: xlink:href（external data 引用）",
        "tags": ["mimetype_violation", "external_href", "real-world"],
    },
    {
        "url": "https://raw.githubusercontent.com/LibreOffice/core/master/"
               "sw/qa/extras/odfimport/data/ooo32780-1.odt",
        "orig_path": "sw/qa/extras/odfimport/data/ooo32780-1.odt",
        "file": "ext/lo_core_sw_qa_odfimport_ooo32780-1.odt",
        "source": "LibreOffice/core",
        "source_url": "https://github.com/LibreOffice/core/tree/master/sw/qa/extras/odfimport/data",
        "license": "MPL-2.0/GPL-3.0/LGPL-3.0（LibreOffice 三重授權；sw/qa 為 tdf bugzilla 匿名化真實用戶文件）",
        "why": "xlink:href 指向檔外相對路徑 ../sw/qa/core/binfilter/...（同目錄內不存在的 .sdw）"
               "——缺檔的外部引用考卷",
        "tags": ["external_href", "broken_link", "real-world"],
    },
    {
        "url": "https://raw.githubusercontent.com/LibreOffice/core/master/"
               "sw/qa/extras/odfimport/data/feature_table_merged-cells.odt",
        "orig_path": "sw/qa/extras/odfimport/data/feature_table_merged-cells.odt",
        "file": "ext/lo_core_sw_qa_odfimport_feature_table_merged-cells.odt",
        "source": "LibreOffice/core",
        "source_url": "https://github.com/LibreOffice/core/tree/master/sw/qa/extras/odfimport/data",
        "license": "MPL-2.0/GPL-3.0/LGPL-3.0（LibreOffice 三重授權；sw/qa 為 tdf bugzilla 匿名化真實用戶文件）",
        "why": "原生 ODT 合併儲存格表格（table:covered-table-cells），非 LO 由 DOCX 轉出者",
        "tags": ["table", "merged_cells", "real-world"],
    },
    {
        "url": "https://raw.githubusercontent.com/LibreOffice/core/master/"
               "sw/qa/extras/odfexport/data/embedded-pdf.odt",
        "orig_path": "sw/qa/extras/odfexport/data/embedded-pdf.odt",
        "file": "ext/lo_core_sw_qa_odfexport_embedded-pdf.odt",
        "source": "LibreOffice/core",
        "source_url": "https://github.com/LibreOffice/core/tree/master/sw/qa/extras/odfexport/data",
        "license": "MPL-2.0/GPL-3.0/LGPL-3.0（LibreOffice 三重授權；sw/qa 為 tdf bugzilla 匿名化真實用戶文件）",
        "why": "draw:frame 內嵌 PDF（Object/… 子容器＋application/pdf）——嵌套 OLE 物件",
        "tags": ["embedded_object", "draw_frame", "real-world"],
    },
    {
        "url": "https://raw.githubusercontent.com/LibreOffice/core/master/"
               "sw/qa/extras/odfexport/data/WordTest_edit.odt",
        "orig_path": "sw/qa/extras/odfexport/data/WordTest_edit.odt",
        "file": "ext/lo_core_sw_qa_odfexport_WordTest_edit.odt",
        "source": "LibreOffice/core",
        "source_url": "https://github.com/LibreOffice/core/tree/master/sw/qa/extras/odfexport/data",
        "license": "MPL-2.0/GPL-3.0/LGPL-3.0（LibreOffice 三重授權；sw/qa 為 tdf bugzilla 匿名化真實用戶文件）",
        "why": "Microsoft Word 2013 直接存出的 ODT（meta:generator=MicrosoftOffice/15.0）"
               "：無 Configurations2/、無 Thumbnails/，最小 entry 集",
        "tags": ["word_produced", "minimal_container", "real-world"],
    },
    {
        "url": "https://docs.oasis-open.org/office/OpenDocument/v1.3/os/"
               "part1-introduction/OpenDocument-v1.3-os-part1-introduction.odt",
        "orig_path": "part1-introduction/OpenDocument-v1.3-os-part1-introduction.odt",
        "file": "ext/oasis_opendocument_v1p3_os_part1_introduction.odt",
        "source": "OASIS OpenDocument v1.3 (OASIS Standard)",
        "source_url": "https://docs.oasis-open.org/office/OpenDocument/v1.3/os/part1-introduction/",
        "license": "OASIS 標準軌文件授權（實查自 docs.oasis-open.org 官方文件版權聲明原文："
                   "\"may be copied and furnished to others, and derivative works that comment on "
                   "or otherwise explain it or assist in its implementation may be prepared, copied, "
                   "published, and distributed, in whole or in part, without restriction of any kind, "
                   "provided that the above copyright notice and this section are included\"；"
                   "本檔為未修改的官方 OASIS Standard 原檔，授權查證通過）",
        "why": "規範本體文件：真實世界由 ODF 1.3 全功能 writer 產出、目錄＋交叉引用＋長文件，"
               "且是「解析器要解的規格自己」，可當自對照樣本",
        "tags": ["specification", "toc", "real-world"],
    },
]

# 查證不過／無法取得的真實 ODT：老實登記進 quarantine.json，不入正庫（實測紀錄，非湊數）。
QUARANTINE_TRIED: list[dict] = [
    {
        "url": "https://odfalliance.org/wp-content/uploads/2021/06/ODF-Alliance-Test-Corpus.zip",
        "title": "ODF-Alliance Test Corpus zip",
        "reason": "下載回 200 但內容是 114 bytes 的停放網域跳頁（text/html，非 zip），"
                  "且站方未附任何授權條款可查證——授權與完整性雙查證不過，不入正庫",
        "http": "200 text/html 114B（偽裝成功）",
    },
    {
        "url": "https://joinup.ec.europa.eu/collection/odf-open-document-format-for-productivity-"
              "applications/solution/odf-test-suite-corpus",
        "title": "EU Joinup ODF test suite corpus",
        "reason": "HTTPS 連線直接失敗（curl rc=6，無回應），無法取得檔案本體與授權聲明",
        "http": "連線失敗（rc=6）",
    },
    {
        "url": "https://commons.wikimedia.org/w/api.php（filetype:office ext:odt 檢索）",
        "title": "Wikimedia Commons ODT 檔案（多筆 CC/PD）",
        "reason": "API 於本環境回 403 Forbidden（無 UA 白名單），無法完成「逐檔授權欄位實查」；"
                  "未實查不得登記授權，故整批暫不入庫（改天補 UA／鏡像站再議）",
        "http": "403 Forbidden",
    },
]

# 故意造的違例 fixture（先紅證明用，不計入 50 檔正庫；見 scripts/odf_survey.py --self-test）
FIXTURES_DIR = "fixtures"
FIXTURE_BASE = "ext/lo_core_sw_qa_odfimport_feature_table_merged-cells.odt"


def sha256(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def local_name(rel: str) -> str:
    return "lo__" + rel.replace("/", "__")[:-len(".docx")] + ".odt"


# --------------------------------------------------------------- LO 自拉自殺（鏡像 render_lab）
def find_soffice() -> pathlib.Path:
    for name in ("soffice", "libreoffice"):
        w = shutil.which(name)
        if w:
            return pathlib.Path(w)
    for c in SOFFICE_CANDS:
        if pathlib.Path(c).exists():
            return pathlib.Path(c)
    sys.exit("找不到 LibreOffice（soffice）；請安裝 Windows 版 LibreOffice 或 linux libreoffice-writer")


def lo_version(soffice: pathlib.Path) -> str:
    """轉換器版本。**不可**用 `soffice --version`：Windows exe 經 WSL interop 會掛死
    （本卡實測 180s 逾時）。改讀 version.ini 的 buildid ＋ Windows 檔案版本屬性。"""
    prog = soffice.parent
    ini = prog / "version.ini"
    buildid = None
    if ini.exists():
        for line in ini.read_text(encoding="utf-8", errors="ignore").splitlines():
            if line.strip().startswith("buildid="):
                buildid = line.strip()[len("buildid="):]
    ver = None
    if shutil.which("powershell.exe") and str(prog).startswith("/mnt/"):
        # 必經 wslpath：/mnt/c/... 直接反斜線化會得到「不是 Windows 路徑」，
        # Get-Item 查不到 → FileVersion 空字串（本卡首輪實測踩到）。
        ps = ("$ErrorActionPreference='SilentlyContinue';"
              "(Get-Item -LiteralPath '%s').VersionInfo.FileVersion"
              % (w2w(prog / "soffice.bin").replace("/", "\\")))
        r = subprocess.run(["powershell.exe", "-NoProfile", "-Command", ps],
                           capture_output=True, text=True, timeout=120, errors="ignore")
        ver = (r.stdout or "").strip() or None
    parts = [v for v in (ver, ("buildid=" + buildid) if buildid else None) if v]
    return " | ".join(parts) or "unknown"


def win_scratch_dir() -> pathlib.Path:
    """Windows 本機暫存（.exe 版 LO 走 \\\\wsl.localhost 讀寫易掛，改走 C: 本機盤）。"""
    cands: list[pathlib.Path] = []
    if shutil.which("cmd.exe"):
        try:
            r = subprocess.run(["cmd.exe", "/c", "echo %TEMP%"], capture_output=True, timeout=30)
            w = r.stdout.decode("utf-8", errors="ignore").strip().strip('"')
            if w:
                rr = subprocess.run(["wslpath", "-u", w], capture_output=True, text=True)
                if rr.returncode == 0:
                    cands.append(pathlib.Path(rr.stdout.strip()))
        except (OSError, subprocess.TimeoutExpired):
            pass
    cands += sorted(pathlib.Path("/mnt/c/Users").glob("*/AppData/Local/Temp"))
    for base in cands:
        d = base / "quire-odf-c1"
        try:
            d.mkdir(parents=True, exist_ok=True)
            probe = d / ".write-probe"
            probe.write_text("x")
            probe.unlink()
            return d
        except OSError:
            continue
    sys.exit(f"Windows 暫存目錄不可寫（試過 {[str(c) for c in cands]}）")


def w2w(linux_path: pathlib.Path) -> str:
    r = subprocess.run(["wslpath", "-w", str(linux_path)], capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"wslpath 轉換失敗（{linux_path}）：{r.stderr.strip()}")
    return r.stdout.strip().replace("\\", "/")


def kill_stale_lo() -> None:
    """清上一輪殘留 headless soffice（只用溫和 taskkill，不碰使用者自己開的視窗）。"""
    if shutil.which("taskkill.exe"):
        subprocess.run(["taskkill.exe", "/IM", "soffice.bin.exe"], capture_output=True)
        subprocess.run(["taskkill.exe", "/IM", "soffice.exe"], capture_output=True)
        time.sleep(1.5)


def kill_own_lo(needle: str) -> int:
    """只殺本輪自己拉起的 soffice（命令列含本 scratch profile 路徑者）。
    背景同 render_lab：崩潰殘留的模態視窗溫和砍不動，兩日可累積上百個進程撐爆 pagefile。"""
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
        print(f"（收場：清掉本輪殘留 LibreOffice 進程 {killed} 個）", flush=True)
    return killed


def lo_scratch(soffice: pathlib.Path) -> tuple[pathlib.Path, str, str]:
    if str(soffice).endswith(".exe"):
        d = win_scratch_dir()
        prof = d / "odf-c1-profile"
        prof.mkdir(exist_ok=True)
        win = w2w(prof)
        return d, "file:///" + win.lstrip("/"), win
    import tempfile
    d = pathlib.Path(tempfile.mkdtemp(prefix="quire-odf-"))
    return d, d.absolute().as_uri(), str(d)


def lo_to_odt(soffice: pathlib.Path, src: pathlib.Path, scratch: pathlib.Path,
              profile_uri: str, out_name_stem: str, timeout: int = LO_TIMEOUT) -> bool:
    """單檔 LO 轉 ODT；成功回 True（產物已搬到 scratch 內以 out_name_stem 命名）。"""
    work = scratch / "conv"
    work.mkdir(exist_ok=True)
    # 來源複製到 C: 本機，檔名帶父目錄避免同名互覆
    staged = work / f"{src.parent.name}__{src.name}"
    shutil.copyfile(src, staged)
    stale = work / (staged.stem + ".odt")
    if stale.exists():
        stale.unlink()  # 上一輪殘留會被誤判成功
    cmd = [str(soffice), f"-env:UserInstallation={profile_uri}", "--headless", "--norestore",
           "--convert-to", FILTER, "--outdir", w2w(work), w2w(staged)]
    try:
        subprocess.run(cmd, capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return False
    if not stale.exists():
        return False
    dest = scratch / f"{out_name_stem}.odt"
    shutil.move(str(stale), dest)
    return True


# --------------------------------------------------------------------- PII 掃描／消毒
EMAIL_RE = re.compile(rb"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")
XML_PART_RE = re.compile(r"\.(xml|rdf|rel|rels)$", re.I)
REDACTED = b"redacted@example.invalid"
PII_RES = {
    "win_users": re.compile(rb"([Cc]:[\\/]{1,2}[Uu]sers[\\/]{1,2})[A-Za-z0-9._% -]{1,40}"),
    "home": re.compile(rb"(/home/)[a-z0-9._-]{1,32}"),
    "users_dir": re.compile(rb"(/Users/)[A-Za-z0-9._ -]{1,32}"),
    "unc": re.compile(rb"[\\\\]{2}wsl\.localhost[\\\\]{1,2}[A-Za-z0-9._-]{1,32}"),
}
TAG_RE = re.compile(
    rb"<([a-zA-Z-]+:)?(creator|lastModifiedBy|company|Manager)"
    rb"((?:\s[^<>]*)?)(?:\s*/>|>(?:(?!</)[\s\S])*?</\1?\2>)", re.I)
REPL = {"creator": "Quire Corpus", "lastmodifiedby": "Quire Corpus",
        "company": "Quire", "manager": ""}


def scrub_xml(data: bytes) -> bytes:
    def sub(m):
        ns, tag, attrs = (m.group(1) or b"").decode(), m.group(2).decode(), m.group(3)
        key = {"creator": "creator", "lastmodifiedby": "lastModifiedBy",
               "company": "company", "manager": "Manager"}.get(tag.lower())
        want = REPL.get(key) if key else None
        if want is None or attrs:
            return m.group(0)
        return f"<{ns}{tag}>".encode() + want.encode() + f"</{ns}{tag}>".encode()
    return TAG_RE.sub(sub, data)


def redact_pii(data: bytes) -> tuple[bytes, dict]:
    """回傳（消毒後位元組, 命中統計）。本機路徑與 email 一律遮罩；二進位 part 不碰。"""
    hits: dict[str, int] = {}
    out = data
    for label, rx in PII_RES.items():
        found = rx.findall(out)
        if found:
            hits[label] = len(found)
            out = rx.sub(lambda m: m.group(1) + b"redacted", out)
    mails = EMAIL_RE.findall(out)
    if mails:
        hits["email"] = len(mails)
        out = EMAIL_RE.sub(REDACTED, out)
    out = scrub_xml(out)
    return out, hits


def scrub_odt(path: pathlib.Path) -> dict:
    """重寫 ODT 內的 XML part（未觸碰 entry 原樣保留）；回傳命中統計。"""
    with zipfile.ZipFile(path) as zf:
        items = [(i, zf.read(i.filename)) for i in zf.infolist()]
    total: dict[str, int] = {}
    rebuilt = []
    dirty = False
    for info, data in items:
        if XML_PART_RE.search(info.filename):
            new, hits = redact_pii(data)
            if new != data:
                dirty = True
            for k, v in hits.items():
                total[k] = total.get(k, 0) + v
            rebuilt.append((info, new))
        else:
            rebuilt.append((info, data))
    if dirty:
        tmp = path.with_suffix(".odt.tmp")
        with zipfile.ZipFile(tmp, "w", zipfile.ZIP_DEFLATED) as zo:
            for info, data in rebuilt:
                zi = zipfile.ZipInfo(info.filename, date_time=info.date_time)
                zi.compress_type = info.compress_type
                zi.external_attr = info.external_attr
                zo.writestr(zi, data)
        tmp.replace(path)
    return total


def find_pii(path: pathlib.Path) -> dict:
    """只掃不改（驗收用）：回傳 label → 命中樣本清單。"""
    out: dict[str, list] = {}
    with zipfile.ZipFile(path) as zf:
        for name in zf.namelist():
            if not XML_PART_RE.search(name):
                continue
            data = zf.read(name)
            for label, rx in PII_RES.items():
                for m in rx.findall(data):
                    s = m.decode("utf-8", "ignore")[:40]
                    out.setdefault(label, [])
                    if s not in out[label]:
                        out[label].append(s)
            for m in EMAIL_RE.findall(data):
                s = m.decode("utf-8", "ignore")[:60]
                out.setdefault("email", [])
                if s not in out["email"]:
                    out["email"].append(s)
    return out


# ------------------------------------------------------------------------- 外部 ODT 下載
def fetch(url: str, timeout: int = 120) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "quire-corpus/1.0 (ODF-C1 survey)"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read()


def looks_like_odt(data: bytes) -> tuple[bool, str]:
    if not data.startswith(b"PK\x03\x04"):
        return False, f"非 zip（頭 {data[:8]!r}，content 可能是錯誤頁）"
    try:
        zf = zipfile.ZipFile(__import__("io").BytesIO(data))
    except Exception as exc:  # noqa: BLE001
        return False, f"zip 不可解析：{exc}"
    names = zf.namelist()
    if "mimetype" not in names:
        return False, "缺 mimetype entry"
    if "META-INF/manifest.xml" not in names:
        return False, "缺 META-INF/manifest.xml"
    mt = zf.read("META-INF/manifest.xml").decode("utf-8", "ignore")
    for media in ("application/vnd.oasis.opendocument.text",
                  "application/vnd.oasis.opendocument.text-template"):
        if media in mt:
            return True, media
    return False, "manifest 未聲明 ODT media-type"


# ---------------------------------------------------------------------------------- 主流程
def build(dry: bool, force: bool, only_missing: bool) -> int:
    src_by_file = {e["file"]: e for e in SRC_MANIFEST["files"]}
    missing_src = [f for f in LO_PICKS if f not in src_by_file]
    if missing_src:
        print(f"FAIL: corpus/manifest.json 查無這些來源：{missing_src}", file=sys.stderr)
        return 1
    missing_disk = [f for f in LO_PICKS if not (CORPUS / f).exists()]
    if missing_disk:
        print(f"FAIL: 來源實體檔不存在：{missing_disk}", file=sys.stderr)
        return 1
    if len(LO_PICKS) != 40:
        print(f"FAIL: LO_PICKS 必須正好 40 檔（目前 {len(LO_PICKS)}）", file=sys.stderr)
        return 1
    if len(EXT_PICKS) != 10:
        print(f"FAIL: EXT_PICKS 必須正好 10 檔（目前 {len(EXT_PICKS)}）", file=sys.stderr)
        return 1

    out_lo = OUT / "lo_out"
    out_ext = OUT / "ext"
    out_fix = OUT / FIXTURES_DIR
    if not dry:
        for d in (out_lo, out_ext, out_fix):
            d.mkdir(parents=True, exist_ok=True)

    soffice = find_soffice()
    print(f"soffice: {soffice}")
    version = lo_version(soffice)
    print(f"轉換器版本：{version}")
    scratch, profile_uri, needle = lo_scratch(soffice)
    kill_stale_lo()
    atexit.register(kill_own_lo, needle)

    entries: list[dict] = []
    fails: list[dict] = []

    print(f"--- LO 轉出 {len(LO_PICKS)} 檔 ---")
    for i, rel in enumerate(LO_PICKS, 1):
        src = CORPUS / rel
        target = out_lo / local_name(rel)
        src_sha = sha256(src)
        entry = {
            "file": f"lo_out/{target.name}",
            "source": f"LibreOffice {version} --convert-to {FILTER}",
            "source_url": src_by_file[rel]["source_url"],
            "orig_path": src_by_file[rel]["orig_path"],
            "kind": "lo-converted",
            "license": src_by_file[rel]["license"],
            "acquisition": f"本機 LibreOffice headless 轉換（filter={FILTER}，"
                           f"version={version}，來源 DOCX sha256={src_sha}）",
            "batch": "odf-c1-lo",
            "doc_type": "odt",
            "converted_from": {"file": f"corpus/{rel}", "sha256": src_sha,
                               "bytes": src_by_file[rel]["bytes"],
                               "filter": FILTER, "lo_version": version},
            "tags": sorted(set(src_by_file[rel].get("tags", []))),
        }
        if target.exists() and not force:
            # 冪等：既有檔若來源 sha256 與 LO 版本都相符則沿用
            prev = read_prev_entry(target.name)
            if prev and prev.get("converted_from", {}).get("sha256") == src_sha \
                    and prev.get("converted_from", {}).get("lo_version") == version:
                entry["sha256"] = sha256(target)
                entry["bytes"] = target.stat().st_size
                entry["scrubbed"] = prev.get("scrubbed", True)
                entries.append(entry)
                print(f"[{i:2}/40] 沿用 {rel}（來源與轉換器未變）")
                continue
        if dry:
            print(f"[{i:2}/40] (dry) {rel}")
            entries.append(entry)
            continue
        ok = lo_to_odt(soffice, src, scratch, profile_uri, target.stem)
        if not ok:
            fails.append({"file": rel, "reason": "LO 轉換失敗或逾時"})
            print(f"[{i:2}/40] 失敗 {rel}")
            continue
        produced = scratch / f"{target.stem}.odt"
        shutil.move(str(produced), target)
        hits = scrub_odt(target)
        entry["sha256"] = sha256(target)
        entry["bytes"] = target.stat().st_size
        entry["scrubbed"] = True
        if hits:
            entry["pii_redacted"] = hits
        entries.append(entry)
        print(f"[{i:2}/40] {rel} -> {target.name} {entry['bytes']} bytes pii={hits or '無'}")

    print(f"--- 外部真實 ODT {len(EXT_PICKS)} 檔（逐檔授權查證） ---")
    quarantined: dict[str, str] = {}
    quarantine_extra: list[dict] = []
    for i, pick in enumerate(EXT_PICKS, 1):
        target = REPO / "corpus-odt" / pick["file"]
        entry = {
            "file": pick["file"],
            "source": pick["source"],
            "source_url": pick["source_url"],
            "orig_path": pick["orig_path"],
            "kind": "real-world",
            "license": pick["license"],
            "acquisition": f"直接下載上游原檔（{pick['url']}），下載後以 sha256 固定；"
                           f"未經任何本機改寫",
            "batch": "odf-c1-ext",
            "doc_type": "odt",
            "source_sha": pick["url"],
            "why": pick["why"],
            "tags": pick["tags"],
        }
        if dry:
            print(f"[{i:2}/10] (dry) {pick['file']}")
            entries.append(entry)
            continue
        try:
            data = fetch(pick["url"])
        except Exception as exc:  # noqa: BLE001
            quarantined[pick["file"]] = f"下載失敗：{exc}；授權查證連同檔案本體都無法完成"
            print(f"[{i:2}/10] 隔離 {pick['file']}：下載失敗 {exc}")
            continue
        good, info = looks_like_odt(data)
        if not good:
            quarantined[pick["file"]] = f"不是可用 ODT（{info}）；來源亦無從查證授權"
            print(f"[{i:2}/10] 隔離 {pick['file']}：{info}")
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        entry["sha256"] = sha256(target)
        entry["bytes"] = target.stat().st_size
        entry["media_type_declared"] = info
        entries.append(entry)
        print(f"[{i:2}/10] {pick['file']} {entry['bytes']} bytes sha256={entry['sha256'][:12]}")

    for q in QUARANTINE_TRIED:
        quarantine_extra.append(q)

    # 先紅用的違例 fixture（合成，不計入 50）
    fix_entries = build_fixtures(out_fix, dry)

    entries.sort(key=lambda e: e["file"])
    manifest = {
        "schema": "quire-corpus-odt/1",
        "description": "真實世界 ODT corpus：LibreOffice headless 自既有 corpus DOCX 轉出 40 檔"
                       "＋上游公開授權可查證的真實 ODT 10 檔。逐檔記來源、授權、取得方式與"
                       "sha256；授權查證不過者進 quarantine.json 不入正庫。",
        "count": len(entries),
        "converter": {"soffice": str(soffice), "version": version, "filter": FILTER,
                      "note": "soffice --version 在 WSL interop 下會掛死，版本由 version.ini "
                              "buildid＋Windows 檔案版本屬性取得"},
        "files": entries,
        "fixtures": fix_entries,
        "fixtures_note": "fixtures/ 為普查腳本「先紅」用的合成違例檔，不計入 count，"
                         "也不得當成真實語料使用",
    }
    quarantine = {
        "schema": "quire-corpus-quarantine/1",
        "description": "授權／完整性查證不過，或轉換與下載失敗的 ODT 清單：清單內檔案不入正庫，"
                       "必須逐檔列出原因，修復後移除。",
        "files": quarantined,
        "tried_and_rejected": quarantine_extra,
    }
    if dry:
        print(f"(dry-run) 將寫 {OUT/'manifest.json'}：正庫 {len(entries)} 檔、"
              f"隔離 {len(quarantined)} 檔")
        return 0
    (OUT / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    (OUT / "quarantine.json").write_text(
        json.dumps(quarantine, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(f"寫出 corpus-odt/manifest.json（{len(entries)} 檔）與 quarantine.json"
          f"（隔離 {len(quarantined)}＋查證失敗 {len(quarantine_extra)}）")
    if fails:
        print(f"FAIL: LO 轉換失敗 {len(fails)} 檔：{[f['file'] for f in fails]}", file=sys.stderr)
        return 1
    if len(entries) != 50:
        print(f"FAIL: 正庫應為 50 檔，實得 {len(entries)}", file=sys.stderr)
        return 1
    return 0


def read_prev_entry(name: str) -> dict | None:
    """讀既有 manifest 中對應條目（冪等沿用判斷）。"""
    mp = OUT / "manifest.json"
    if not mp.exists():
        return None
    try:
        m = json.loads(mp.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return None
    for e in m.get("files", []):
        if e.get("file", "").endswith("/" + name) or e.get("file") == name:
            return e
    return None


def build_fixtures(out_fix: pathlib.Path, dry: bool) -> list[dict]:
    """造三個違例 fixture（先紅證明用）：mimetype 非首 entry、mimetype 被壓縮、缺 manifest.xml。
    基礎素材取自已下載的真實 ODT（MPL 三重授權），因此 fixture 授權口徑可追。"""
    base = REPO / "corpus-odt" / FIXTURE_BASE
    specs = [
        ("mimetype_not_first.odt", "mimetype 非首 entry（放在 content.xml 之後）",
         "mimetype_not_first"),
        ("mimetype_compressed.odt", "首 entry 是 mimetype 但採 DEFLATE 壓縮（§3.3 要求不壓縮）",
         "mimetype_compressed"),
        ("missing_manifest_xml.odt", "缺 META-INF/manifest.xml（容器必要 part 缺失）",
         "missing_manifest_xml"),
    ]
    out = []
    for name, desc, flag in specs:
        rec = {
            "file": f"{FIXTURES_DIR}/{name}",
            "kind": "synthetic",
            "source": "本卡合成（基礎素材 ext/lo_core_sw_qa_odfimport_feature_table_merged-cells.odt）",
            "license": "MPL-2.0/GPL-3.0/LGPL-3.0（LibreOffice 三重授權；衍生自上游真實 ODT，"
                       "僅重排 zip entry／改壓縮位元，未加入第三方內容）",
            "acquisition": "由 scripts/odf_build_corpus.py 以 zipfile 重排產生，用途僅限普查腳本"
                           "的「先紅」自證（驗證 mimetype 規則真的會報錯）",
            "violation": desc,
            "expected_flags": [flag],
        }
        if dry or not base.exists():
            out.append(rec)
            continue
        with zipfile.ZipFile(base) as zf:
            items = [(i, zf.read(i.filename)) for i in zf.infolist()]
        out_fix.mkdir(parents=True, exist_ok=True)
        dest = out_fix / name
        with zipfile.ZipFile(dest, "w", zipfile.ZIP_DEFLATED) as zo:
            if flag == "mimetype_not_first":
                order = [it for it in items if it[0].filename != "mimetype"]
                mt = [it for it in items if it[0].filename == "mimetype"]
                for info, data in order + mt:
                    zi = zipfile.ZipInfo(info.filename, date_time=info.date_time)
                    zi.compress_type = info.compress_type
                    zo.writestr(zi, data)
            elif flag == "mimetype_compressed":
                for info, data in items:
                    zi = zipfile.ZipInfo(info.filename, date_time=info.date_time)
                    zi.compress_type = (zipfile.ZIP_DEFLATED if info.filename == "mimetype"
                                        else info.compress_type)
                    zo.writestr(zi, data)
            else:  # missing_manifest_xml
                for info, data in items:
                    if info.filename == "META-INF/manifest.xml":
                        continue
                    zi = zipfile.ZipInfo(info.filename, date_time=info.date_time)
                    zi.compress_type = info.compress_type
                    zo.writestr(zi, data)
        rec["sha256"] = sha256(dest)
        rec["bytes"] = dest.stat().st_size
        out.append(rec)
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description="建立 corpus-odt（50 檔）＋manifest＋quarantine")
    ap.add_argument("--dry-run", action="store_true", help="只回報計畫，不落檔")
    ap.add_argument("--force", action="store_true", help="忽略冪等沿用，全部重轉")
    ap.add_argument("--only-missing", action="store_true",
                    help="保留既有產出（預設行為即沿用，此旗標供口徑對齊）")
    a = ap.parse_args()
    return build(dry=a.dry_run, force=a.force, only_missing=a.only_missing)


if __name__ == "__main__":
    sys.exit(main())
