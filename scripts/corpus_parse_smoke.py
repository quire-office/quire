"""corpus 煙霧測試：corpus/*.docx 必須能被 quire-ooxml parse()。

PM 補正（2026-10-16）：
- quarantine.json 內檔案跳過評測、不計紅，於報告單列原因。
- 分批次綠燈率：batch=phase0（既有 130）必須 100%（倒退即 FAIL）；
  batch=expand（新增檔）綠燈率 ≥90%，低於則紅燈清單與原因必須列全。
（語料由 scripts/corpus/collect.sh 採集；來源與授權見 corpus/manifest.json。）
"""
import json
import pathlib
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parent.parent
EXPAND_MIN_GREEN = 0.90


def main() -> int:
    corpus = REPO / "corpus"
    manifest = json.loads((corpus / "manifest.json").read_text(encoding="utf-8"))
    quarantine = {}
    qp = corpus / "quarantine.json"
    if qp.exists():
        quarantine = json.loads(qp.read_text(encoding="utf-8")).get("files", {})
    with tempfile.TemporaryDirectory() as td:
        out = pathlib.Path(td) / "info.json"
        r = subprocess.run(
            ["cargo", "run", "-q", "-p", "quire-cli", "--", "info", str(corpus / manifest["files"][0]["file"])],
            capture_output=True, text=True, cwd=REPO)
        if r.returncode != 0:
            print("quire-cli 建置失敗:", r.stderr[:2000])
            return 1
    bad = []  # (file, batch, err)
    n_by_batch = {"phase0": 0, "expand": 0}
    bad_by_batch = {"phase0": 0, "expand": 0}
    q_skipped = []
    for e in manifest["files"]:
        f = e["file"]
        if f in quarantine:
            q_skipped.append((f, quarantine[f]))
            continue
        batch = e.get("batch", "expand")
        n_by_batch[batch] = n_by_batch.get(batch, 0) + 1
        p = corpus / f
        r = subprocess.run(
            ["cargo", "run", "-q", "-p", "quire-cli", "--", "info", str(p)],
            capture_output=True, text=True, cwd=REPO)
        if r.returncode != 0:
            err = (r.stderr or r.stdout).strip().splitlines()[-1:] or ["?"]
            bad.append((f, batch, err[0][:160]))
            bad_by_batch[batch] = bad_by_batch.get(batch, 0) + 1
    total = sum(n_by_batch.values())
    print(f"quarantine 跳過: {len(q_skipped)} 檔")
    for f, why in q_skipped:
        print(f"  Q {f} — {why}")
    print(f"quire-ooxml parse 綠燈: {total - len(bad)}/{total}")
    fails = []
    for b in ("phase0", "expand"):
        n, k = n_by_batch.get(b, 0), bad_by_batch.get(b, 0)
        rate = (n - k) / n if n else 1.0
        print(f"  批次 {b:7s}: {n - k}/{n} = {100 * rate:.1f}%")
        if b == "phase0" and k:
            fails.append(f"phase0 倒退：{k} 檔紅燈（門檻 100%）")
        if b == "expand" and n and rate < EXPAND_MIN_GREEN:
            fails.append(f"expand 綠燈率 {100 * rate:.1f}% < {100 * EXPAND_MIN_GREEN:.0f}%")
    for f, b, err in bad:
        print("  RED", f, f"[{b}] ->", err)
    for msg in fails:
        print("FAIL:", msg)
    return 0 if not fails else 1


if __name__ == "__main__":
    sys.exit(main())
