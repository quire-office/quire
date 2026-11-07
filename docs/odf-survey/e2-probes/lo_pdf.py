#!/usr/bin/env python3
"""ODF-E2：六檔 LO --convert-to pdf（render_lab 同口徑：自備 scratch profile＋收場只殺自己）。"""
import pathlib, subprocess, sys, shutil, time, tempfile

SOFFICE = pathlib.Path("/mnt/c/Program Files/LibreOffice/program/soffice.exe")
REPO = pathlib.Path("/home/jerry/projects/quire/.worktrees/t_72dc8bc8")
OUT = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path("lo_pdf")
OUT.mkdir(parents=True, exist_ok=True)

FILES = [
    "corpus-odt/ext/lo_core_sw_qa_odfimport_ooo32780-1.odt",
    "corpus-odt/lo_out/lo__poi__test-data_document_form_footnotes.odt",
    "corpus-odt/lo_out/lo__ooxmlexport__mce-nested.odt",
    "corpus-odt/lo_out/lo__ooxmlexport__strict.odt",
    "corpus-odt/lo_out/lo__ooxmlexport__tdf124637_sectionMargin.odt",
    "corpus-odt/lo_out/lo__ooxmlimport__tdf87533_bidi.odt",
]

def win(path):
    r = subprocess.run(["wslpath", "-w", str(path)], capture_output=True, text=True)
    return r.stdout.strip().replace("\\", "/")

def kill_own(needle):
    if not shutil.which("powershell.exe"):
        return 0
    alt = needle.replace("/", "\\\\")
    ps = ("$ErrorActionPreference='SilentlyContinue';"
          "$p = Get-CimInstance Win32_Process -Filter \"Name='soffice.exe' or Name='soffice.bin.exe'\" |"
          f" Where-Object {{ $_.CommandLine -like '*{needle}*' -or $_.CommandLine -like '*{alt}*' }};"
          "$n = ($p | Measure-Object).Count;"
          "$p | ForEach-Object { Stop-Process -Id $_.ProcessId -Force };"
          "$n")
    try:
        r = subprocess.run(["powershell.exe", "-NoProfile", "-Command", ps],
                           capture_output=True, text=True, timeout=120)
        return int((r.stdout or "0").strip().splitlines()[-1])
    except Exception:
        return 0

if shutil.which("taskkill.exe"):
    subprocess.run(["taskkill.exe", "/IM", "soffice.bin.exe"], capture_output=True)
    subprocess.run(["taskkill.exe", "/IM", "soffice.exe"], capture_output=True)
    time.sleep(1.5)

# render_lab 同口徑：exe 版 LO 讀不到 /mnt 或 \\wsl.localhost → 來源與輸出都走 C: 本機暫存
r = subprocess.run(["cmd.exe", "/c", "echo %TEMP%"], capture_output=True, timeout=15)
wintemp = r.stdout.decode("utf-8", errors="ignore").strip().strip('"')
scratch = pathlib.Path(subprocess.run(["wslpath", "-u", wintemp], capture_output=True,
                                      text=True).stdout.strip()) / "quire-e2lo"
scratch.mkdir(parents=True, exist_ok=True)
prof = scratch / "profile"
prof.mkdir(exist_ok=True)
winprof = win(prof)
uri = "file:///" + winprof.lstrip("/")

for rel in FILES:
    src = REPO / rel
    name = pathlib.Path(rel).name
    d = OUT / name
    d.mkdir(exist_ok=True)
    # 同名檔可能互覆：暫存名帶父目錄（同 render_lab）
    staged = scratch / f"{src.parent.name}__{name}"
    shutil.copyfile(src, staged)
    outwin = win(scratch)
    srcwin = win(staged)
    stale = scratch / (staged.stem + ".pdf")
    if stale.exists():
        stale.unlink()
    t0 = time.time()
    r = subprocess.run([str(SOFFICE), f"-env:UserInstallation={uri}",
                        "--headless", "--norestore",
                        "--convert-to", "pdf", "--outdir", outwin, srcwin],
                       capture_output=True, text=True, timeout=300)
    if stale.exists():
        shutil.move(str(stale), str(d / stale.name))
    pdfs = sorted(d.glob("*.pdf"))
    print(f"{name}: rc={r.returncode} pdf={[p.name for p in pdfs]} {time.time()-t0:.1f}s", flush=True)
    if r.returncode != 0 or not pdfs:
        print("  stdout:", (r.stdout or "")[-400:], flush=True)
        print("  stderr:", (r.stderr or "")[-400:], flush=True)
    kill_own(winprof)
kill_own(winprof)
