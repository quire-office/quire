#!/usr/bin/env python3
"""圖示精靈產生器：讀 icons/icon-map.json，從 icons/src/<fa>.svg 抽 path，
產出 editor sprite 的 <symbol> 區間（icons/sprite-symbols.part），
必要時把 index.html 的 <defs> 精靈整段換裝（--apply）。

單一真相源 = icons/（原始 FA svg 本機副本＋映射表＋LICENSE）。
index.html 的 sprite 是 build 產物，禁止手改——改圖改 icons/ 後重跑本腳本。
"""
import argparse
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
EDITOR = os.path.normpath(os.path.join(HERE, "..", "shells", "web", "editor"))
ICONS = os.path.join(EDITOR, "icons")
SRC = os.path.join(ICONS, "src")
MAP = os.path.join(ICONS, "icon-map.json")
PART = os.path.join(ICONS, "sprite-symbols.part")
INDEX = os.path.join(EDITOR, "index.html")

RE_SVG_OPEN = re.compile(r"<svg[^>]*viewBox=\"([^\"]+)\"[^>]*>")
RE_COMMENT = re.compile(r"<!--.*?-->", re.DOTALL)
RE_PATHS = re.compile(r"<path\b[^>]*/?>(?:(?!</?path).)*?(?:</path>)?", re.DOTALL)


def extract(viewbox: str, svg_text: str, name: str, fa_name: str) -> str:
    body = RE_COMMENT.sub("", svg_text)
    paths = RE_PATHS.findall(body)
    if not paths:
        sys.exit(f"icons/src/{fa_name}.svg 沒有 <path>（name={name}）")
    cleaned = []
    for p in paths:
        p = re.sub(r"\s+fill=\"[^\"]*\"", "", p)
        p = p.rstrip("/>")
        cleaned.append(p.rstrip() + "/>")
    inner = "\n      ".join(cleaned)
    return (
        f'    <symbol id="icon-{name}" viewBox="{viewbox}" fill="currentColor">\n'
        f"      {inner}\n"
        f"    </symbol>"
    )


def build_symbols() -> str:
    mapping = json.load(open(MAP, encoding="utf-8"))
    out = []
    for name in sorted(mapping):
        fa = mapping[name]
        path = os.path.join(SRC, f"{fa}.svg")
        if not os.path.isfile(path):
            sys.exit(f"缺來源檔：icons/src/{fa}.svg（name={name}）")
        text = open(path, encoding="utf-8").read()
        m = RE_SVG_OPEN.search(text)
        if not m:
            sys.exit(f"icons/src/{fa}.svg 沒有 viewBox（name={name}）")
        out.append(extract(m.group(1), text, name, fa))
    return "\n".join(out)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--apply", action="store_true", help="把 index.html 的 <defs> 精靈換裝")
    args = ap.parse_args()

    symbols = build_symbols()
    header = (
        "    <!-- 由 scripts/gen_icon_sprite.py 從 icons/（Font Awesome Free 7.3.1 本機副本，"
        "CC BY 4.0）產生，禁止手改；改圖改 icons/icon-map.json 或 icons/src/ 後重跑腳本 -->"
    )
    open(PART, "w", encoding="utf-8").write(header + "\n" + symbols + "\n")
    n = symbols.count("<symbol")
    print(f"symbols={n} -> {os.path.relpath(PART, EDITOR)}")

    if args.apply:
        html = open(INDEX, encoding="utf-8").read()
        m = re.search(r"  <defs>\n(.*?)  </defs>", html, re.DOTALL)
        if not m:
            sys.exit("index.html 找不到 2-縮排 <defs> 區塊")
        new_html = html[: m.start(1)] + header + "\n" + symbols + "\n" + html[m.end(1):]
        open(INDEX, "w", encoding="utf-8").write(new_html)
        print(f"index.html defs 已換裝（{n} symbols）")


if __name__ == "__main__":
    main()
