#!/usr/bin/env python3
"""設計書代碼引用稽核器 v2（PM 覆核工具，擴充行內代碼錨點格式）。

v1（ow-doc-anchor-audit.py）只認 markdown 連結 `](path#L..)`；本 v2 同時認
行內代碼錨點 `path.rs:N`／`path.rs:N-M`（edit-api-design.md 的 414 條錨點全用
此格式，v1 全部隐形 → `ok=0 stale=0` 是「零條被看到」而非全綠）。

用法：
    python3 ~/.hermes/scripts/ow-doc-anchor-audit-v2.py <ref> <doc-path> [--repo PATH]

判定（與 v1 同口徑，另加 RANGE）：
  STALE     區間找不到同行點名的任一識別符，但它在整棵 *.rs 有真實宣告 → 行號漂移
  MISSFILE  檔案不存在 → 路徑虛構或已搬遷
  PHANTOM   識別符整棵 *.rs 無宣告，且該行未標「提議／尚未」→ 虛構既有實作
  RANGE     錨點行號越過檔尾，或 N>M → 明顯壞錨點
  PROPOSED  同上 PHANTOM 但該行明文標提議語彙 → 誠實標記，不算違規

inline-code 錨點的 ident 取自「同一行所有代碼片段點名的候選 ident」（偏寬鬆，
只有全部候選都不落在區間內才判 STALE，避免 ident 推錯造成誤報）；無候選可推
時只驗檔存在＋RANGE，計 ok（不宣稱 ident 級判定）。
綠燈口徑：STALE=0 且 MISSFILE=0 且 PHANTOM=0 且 RANGE=0（exit code 0）。
"""
import argparse
import re
import subprocess
from collections import defaultdict

RE_MD_LINK = re.compile(
    r'\[`([^`]*)`\]\((?:file:///[^)\s]*?)?'
    r'((?:\.\./)*(?:core|shells|scripts|docs|tools)/[^)#\s]+)(#L(\d+)(?:-L(\d+))?)?\)')
# 行內代碼錨點：`core/x.rs:12`、`core/x.rs:12-30`、含 ../ 相對路徑與反引號外緣
RE_INLINE = re.compile(
    r'`((?:\.\./)*(?:core|shells|scripts|docs|tools)/[^\s`:]+\.rs):(\d+)(?:-(\d+))?`')
RE_DECL_TMPL = r'\b(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|fn|trait|type|const|static|mod)\s+([A-Za-z_][A-Za-z0-9_]{2,})\b'
RE_CODE_SPANS = re.compile(r'`([^`]*)`')
HONEST = ('提議', '尚未', '待 ODF', '待ODF', '新增於', '將於 ODF', '建議新增', 'ODF-2 建立')


def norm_path(path, doc):
    parts = [s for s in doc.split('/')[:-1] if s]
    for seg in path.split('/'):
        if seg == '..':
            if parts:
                parts.pop()
        else:
            parts.append(seg)
    return '/'.join(parts)


def sh(repo, *args):
    return subprocess.run(['git', '-C', repo, *args], capture_output=True, text=True).stdout


def collect_anchors(line, doc):
    """回傳 (text_or_ident_hint, path, l1, l2, source)。"""
    out = []
    for text, path, _a, l1, l2 in RE_MD_LINK.findall(line):
        out.append((text, norm_path(path, doc), l1, l2, 'md'))
    for path, l1, l2 in RE_INLINE.findall(line):
        # inline 錨點是倉庫樹路徑（非文件相對）：先原樣，查無才試消解 ../
        if path.startswith(('core/', 'shells/', 'scripts/', 'docs/', 'tools/')):
            resolved = path
        else:
            resolved = norm_path(path, doc)
        out.append((None, resolved, l1, l2, 'inline'))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('ref')
    ap.add_argument('doc')
    ap.add_argument('--repo', default='.',
                    help='要被稽核的 git repo 路徑（預設目前目錄；評 worktree 交付時帶 --repo <worktree>）')
    a = ap.parse_args()

    doc = sh(a.repo, 'show', f'{a.ref}:{a.doc}')
    lines = doc.splitlines()
    fset = set(sh(a.repo, 'ls-tree', '-r', '--name-only', a.ref).split())
    rs = [p for p in fset if p.endswith('.rs')]
    blob = {p: sh(a.repo, 'show', f'{a.ref}:{p}') for p in sorted(rs)}

    decl = defaultdict(list)
    for p, body in blob.items():
        for i, ln in enumerate(body.splitlines()):
            for m in re.finditer(RE_DECL_TMPL, ln):
                decl[m.group(1)].append((p, i + 1))

    stale = miss = phantom = proposed = ok = rng = 0
    for lineno, line in enumerate(lines, 1):
        anchors = collect_anchors(line, a.doc)
        if not anchors:
            continue
        # 同行代碼片段點名的候選 ident（給 inline 錨點用）
        candidates = set()
        for span in RE_CODE_SPANS.findall(line):
            for m in re.finditer(r'\b([A-Za-z_][A-Za-z0-9_]{2,})\b', span):
                tok = m.group(1)
                if tok in decl:
                    candidates.add(tok)
        for text, path, l1, l2, src in anchors:
            if path not in fset:
                print(f'  MISSFILE  L{lineno}  {path}  ({text or "inline"})')
                miss += 1
                continue
            if src == 'md':
                key = re.split(r'[:\s(]', text.split('::')[-1])[0].strip(' `')
                if not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]{2,}', key):
                    ok += 1
                    continue
                keys = [key] if key in decl else []
                if not keys:
                    if any(h in line for h in HONEST):
                        proposed += 1
                    else:
                        print(f'  PHANTOM   L{lineno}  {key}  宣稱於 {path}'
                              f'{("#L" + l1) if l1 else ""}  —— 整棵 *.rs 無此宣告且未標提議')
                        phantom += 1
                    continue
            else:
                # inline 錨點的 ident 只認「同一行、緊接在錨點之前的代碼片段」點名的符號
                # （表格列格式：| 符號 | `path.rs:N-M` |）。實測證明取「整行所有代碼 ident」
                # 會誤報（L21 講 `parse` 呼叫、行號卻被拿去對 `Engine` 宣告），因此只取
                # 錨點前最近的一個代碼片段，且要求它點名到真實宣告，否則降為結構判定。
                keys = []
                pos = line.find(f'`{path}:{l1}', 0)
                if pos >= 0:
                    span = None
                    for m in RE_CODE_SPANS.finditer(line):
                        if m.end() <= pos and (pos - m.end()) <= 6:
                            span = m.group(1)
                        if m.start() > pos:
                            break
                    if span:
                        cands = [t for t in re.findall(r'[A-Za-z_][A-Za-z0-9_]{2,}', span)
                                 if t in decl]
                        if cands:
                            keys = [cands[-1]]
            if not l1:
                ok += 1
                continue
            body = blob[path].splitlines()
            n = len(body)
            x = int(l1)
            y = int(l2) if l2 else x
            if x < 1 or y < x or x > n or y > n:
                print(f'  RANGE     L{lineno}  {path}:{x}-{y}  檔長 {n} 行（越界或反向）')
                rng += 1
                continue
            if not keys:
                ok += 1
                continue
            seg = '\n'.join(body[x - 1:y])
            if any(k in seg for k in keys):
                ok += 1
            else:
                where = ','.join(f'{p}:{i}' for p, i in decl[keys[0]][:3])
                print(f'  STALE     L{lineno}  {path}:{x}-{y}  ident={keys[0]}'
                      f'{"(同行候選:" + ",".join(keys) + ")" if len(keys) > 1 else ""}'
                      f'  實際宣告 {where}')
                stale += 1

    total = ok + stale + miss + phantom + proposed + rng
    print(f'AUDIT_SUMMARY ok={ok} stale={stale} missfile={miss} phantom={phantom} '
          f'proposed={proposed} range={rng} total={total}')
    return 1 if (stale or miss or phantom or rng) else 0


if __name__ == '__main__':
    sys_exit = main()
    raise SystemExit(sys_exit)
