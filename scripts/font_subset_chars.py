"""掃出「引擎必須精確度量」的字元清單，供 build_font_subset.sh 使用。

涵蓋：corpus/**/*.docx 全部 XML（含屬性值）＋ fixtures/**/*.docx ＋
repo 內所有 .rs 的「字串字面值＋註解」——註解也掃是因為 golden test 的
快照字串可能出現在註解，而任何將被斷行的字元都必須在子集內。
"""
import glob
import re
import sys


def docx_text(path):
    import zipfile

    out = []
    try:
        with zipfile.ZipFile(path) as z:
            for n in z.namelist():
                if n.endswith(".xml"):
                    try:
                        out.append(z.read(n).decode("utf-8", "ignore"))
                    except Exception:
                        pass
    except Exception:
        pass
    return "".join(out)


def main():
    chars = set()
    for p in glob.glob("corpus/**/*.docx", recursive=True):
        chars.update(docx_text(p))
    for p in glob.glob("fixtures/**/*.docx", recursive=True):
        chars.update(docx_text(p))
    for pat in ("core/**/*.rs", "shells/**/*.rs", "scripts/**/*.py"):
        for p in glob.glob(pat, recursive=True):
            src = open(p, encoding="utf-8").read()
            # 連同註解一起掃（// 與 # 之後、字串字面值），避免漏掉快照字串。
            for m in re.findall(r'"(?:[^"\\\n]|\\.)*"|//[^\n]*|#[^\n]*', src):
                chars.update(m)

    chars = sorted(c for c in chars if ord(c) > 0x20 and not (0x7F <= ord(c) <= 0x9F))
    with open(sys.argv[1], "w", encoding="utf-8") as f:
        f.write("".join(chars))
    print(f"掃描字元數：{len(chars)}", file=sys.stderr)


if __name__ == "__main__":
    main()
