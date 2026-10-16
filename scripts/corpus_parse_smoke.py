"""corpus 煙霧測試：全部 corpus/*.docx 必須能被 quire-ooxml parse()。

演進點：Phase 0 之後解析器逐步覆蓋表格/欄位/圖形，此測試保持紅燈清單可见。
（語料由 scripts/corpus/collect.sh 採集；來源與授權見 corpus/manifest.json。）
"""
import json
import pathlib
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parent.parent


def main() -> int:
    corpus = REPO / "corpus"
    manifest = json.loads((corpus / "manifest.json").read_text(encoding="utf-8"))
    with tempfile.TemporaryDirectory() as td:
        out = pathlib.Path(td) / "info.json"
        r = subprocess.run(
            ["cargo", "run", "-q", "-p", "quire-cli", "--", "info", str(corpus / manifest["files"][0]["file"])],
            capture_output=True, text=True, cwd=REPO)
        if r.returncode != 0:
            print("quire-cli 建置失敗:", r.stderr[:2000])
            return 1
    # 逐檔走 CLI info（含 parse）；記錄紅燈
    bad = []
    for e in manifest["files"]:
        p = corpus / e["file"]
        r = subprocess.run(
            ["cargo", "run", "-q", "-p", "quire-cli", "--", "info", str(p)],
            capture_output=True, text=True, cwd=REPO)
        if r.returncode != 0:
            bad.append((e["file"], (r.stderr or r.stdout).strip().splitlines()[-1:] or ["?"]))
    total = len(manifest["files"])
    print(f"quire-ooxml parse 綠燈: {total - len(bad)}/{total}")
    for f, err in bad:
        print("  RED", f, "->", err[0][:160])
    return 0 if not bad else 1


if __name__ == "__main__":
    sys.exit(main())
