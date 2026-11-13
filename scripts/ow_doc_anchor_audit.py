#!/usr/bin/env python3
"""設計書代碼引用稽核器 v3（支援 struct 欄位／呼叫點／模組名判定與 --selftest）。

v1（ow-doc-anchor-audit.py）只認 markdown 連結 `](path#L..)`；v2 擴充行內代碼錨點
`path.rs:N`／`path.rs:N-M`；本 v3 在 v2 基礎上補齊三類既有盲區並維持舊 ok 零回歸：

1. FIELD 級（`[FIELD]`）：解析 `struct X { ... }` 區塊內的 `pub name:` / `name:` 欄位宣告；
   錨點區間含該欄位實際宣告行才算 ok（否則依既有型別宣告檢查或報 STALE）。
2. CALLSITE 級（`[CALLSITE]`）：識別明確「呼叫點／callsite」語境或帶 `(...)` 的呼叫式
   錨點，只驗區間未越界且含該 ident 完整詞彙 token（`\\bident\\b`），不要求宣告在區間內。
3. FILE 級（`[FILE]`）：無可推符號、前鄰片段為檔案路徑／CLI 指令（避免誤抓路徑片段
   `engine`／`corpus`／`zip_struct` 等為符號）、跨檔 `對齊` 引用，或全小寫模組名 ident
   等於檔名去副檔名者，自動降為檔級判定（只驗檔案存在與區間未越界）。

用法：
    python3 scripts/ow_doc_anchor_audit.py <ref> <doc-path> [--repo PATH]
    python3 scripts/ow_doc_anchor_audit.py --selftest

綠燈口徑：STALE=0 且 MISSFILE=0 且 PHANTOM=0 且 RANGE=0（exit code 0）。
"""
import argparse
import re
import subprocess
from collections import defaultdict
from pathlib import Path

RE_MD_LINK = re.compile(
    r'\[`([^`]*)`\]\((?:file:///[^)\s]*?)?'
    r'((?:\.\./)*(?:core|shells|scripts|docs|tools)/[^)#\s]+)(#L(\d+)(?:-L(\d+))?)?\)')
# 行內代碼錨點：`core/x.rs:12`、`core/x.rs:12-30`、含 ../ 相對路徑與反引號外緣
RE_INLINE = re.compile(
    r'`((?:\.\./)*(?:core|shells|scripts|docs|tools)/[^\s`:]+\.rs):(\d+)(?:-(\d+))?`')
RE_DECL_TMPL = r'\b(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|fn|trait|type|const|static|mod)\s+([A-Za-z_][A-Za-z0-9_]{2,})\b'
RE_STRUCT_START = re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+([A-Za-z_][A-Za-z0-9_]*)\b')
RE_FIELD_LINE = re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?([A-Za-z_][A-Za-z0-9_]{1,})\s*:')
RE_CODE_SPANS = re.compile(r'`([^`]*)`')
RE_QUAL_FIELD = re.compile(
    r'(?:([A-Za-z_][A-Za-z0-9_]*)::|\.)([A-Za-z_][A-Za-z0-9_]{1,})(?:\s*:[^=]*)?$')
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
    """回傳 (text_or_ident_hint, path, l1, l2, source, anchor_pos)。"""
    out = []
    for text, path, _a, l1, l2 in RE_MD_LINK.findall(line):
        out.append((text, norm_path(path, doc), l1, l2, 'md', None))
    for m in RE_INLINE.finditer(line):
        path, l1, l2 = m.group(1), m.group(2), m.group(3)
        # inline 錨點是倉庫樹路徑（非文件相對）：先原樣，查無才試消解 ../
        if path.startswith(('core/', 'shells/', 'scripts/', 'docs/', 'tools/')):
            resolved = path
        else:
            resolved = norm_path(path, doc)
        out.append((None, resolved, l1, l2, 'inline', m.start()))
    return out


def strip_comments_and_strings(line, in_block):
    """移除 //、/* ... */ 註解與 "..." 字串常值，供大括號層級追蹤。"""
    out = []
    i = 0
    n = len(line)
    while i < n:
        if in_block:
            end = line.find('*/', i)
            if end < 0:
                return '', True
            i = end + 2
            in_block = False
            continue
        ch = line[i]
        if ch == '/' and i + 1 < n:
            if line[i + 1] == '/':
                break
            if line[i + 1] == '*':
                in_block = True
                i += 2
                continue
        if ch == '"':
            i += 1
            while i < n:
                if line[i] == '\\':
                    i += 2
                elif line[i] == '"':
                    i += 1
                    break
                else:
                    i += 1
            out.append(' ')
            continue
        out.append(ch)
        i += 1
    return ''.join(out), in_block


def build_indices(blob):
    """建置頂層宣告索引與 struct 欄位索引。"""
    decl = defaultdict(list)
    field_decl = defaultdict(list)
    field_at = defaultdict(set)
    struct_field_at = defaultdict(set)

    for p, body in blob.items():
        in_block = False
        pending = None
        cur_struct = None
        depth = 0
        for idx, raw in enumerate(body.splitlines(), 1):
            for m in re.finditer(RE_DECL_TMPL, raw):
                decl[m.group(1)].append((p, idx))

            clean, in_block = strip_comments_and_strings(raw, in_block)
            if cur_struct is None:
                if pending is None:
                    sm = RE_STRUCT_START.match(clean)
                    if sm:
                        pending = sm.group(1)
                if pending is not None:
                    semi = clean.find(';')
                    brace = clean.find('{')
                    if semi >= 0 and (brace < 0 or semi < brace):
                        pending = None
                    elif brace >= 0:
                        cur_struct = pending
                        pending = None
                        depth = clean[brace:].count('{') - clean[brace:].count('}')
                        if depth <= 0:
                            cur_struct = None
                            depth = 0
                        continue
            else:
                if depth == 1:
                    fm = RE_FIELD_LINE.match(clean)
                    if fm:
                        fname = fm.group(1)
                        field_decl[fname].append((cur_struct, p, idx))
                        field_at[(p, fname)].add(idx)
                        struct_field_at[(p, cur_struct, fname)].add(idx)
                depth += clean.count('{') - clean.count('}')
                if depth <= 0:
                    cur_struct = None
                    depth = 0

    return decl, field_decl, field_at, struct_field_at


def is_path_or_cli_span(span):
    """辨識檔案路徑、同列相鄰錨點或 CLI 指令，避免把路徑詞段誤抓為 Rust 識別符。"""
    if re.match(r'^[^\s(]+/', span):
        return True
    if re.search(r'\.(?:rs|docx|xlsx|odt|xml|json|md|js|ts|py|sh)(?::\d+(?:-\d+)?)?$', span):
        return True
    if ' --' in span:
        return True
    return False


def has_word(seg, ident):
    return bool(re.search(rf'\b{re.escape(ident)}\b', seg))


def audit_lines(doc_lines, doc_path, fset, blob, emit=print):
    """對已載入的檔案集合與內容執行錨點稽核，回傳 (exit_code, stats)。"""
    decl, field_decl, field_at, struct_field_at = build_indices(blob)
    counts = defaultdict(int)

    for lineno, line in enumerate(doc_lines, 1):
        anchors = collect_anchors(line, doc_path)
        if not anchors:
            continue
        for text, path, l1, l2, src, anchor_pos in anchors:
            if path not in fset:
                emit(f'  MISSFILE  L{lineno}  {path}  ({text or "inline"})')
                counts['miss'] += 1
                continue

            if src == 'md':
                key = re.split(r'[:\s(]', text.split('::')[-1])[0].strip(' `')
                if not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]{2,}', key):
                    emit(f'  OK        L{lineno}  {path}  [FILE]')
                    counts['ok_file'] += 1
                    continue
                if key not in decl:
                    if any(h in line for h in HONEST):
                        counts['proposed'] += 1
                    else:
                        emit(f'  PHANTOM   L{lineno}  {key}  宣稱於 {path}'
                             f'{("#L" + l1) if l1 else ""}  —— 整棵 *.rs 無此宣告且未標提議')
                        counts['phantom'] += 1
                    continue
                if not l1:
                    emit(f'  OK        L{lineno}  {path}  [FILE]')
                    counts['ok_file'] += 1
                    continue
                body = blob[path].splitlines()
                n = len(body)
                x = int(l1)
                y = int(l2) if l2 else x
                if x < 1 or y < x or x > n or y > n:
                    emit(f'  RANGE     L{lineno}  {path}:{x}-{y}  檔長 {n} 行（越界或反向）')
                    counts['range'] += 1
                    continue
                seg = '\n'.join(body[x - 1:y])
                if key in seg:
                    counts['ok_decl'] += 1
                else:
                    where = ','.join(f'{p}:{i}' for p, i in decl[key][:3])
                    emit(f'  STALE     L{lineno}  {path}:{x}-{y}  ident={key}  實際宣告 {where}')
                    counts['stale'] += 1
                continue

            # inline 錨點
            if not l1:
                emit(f'  OK        L{lineno}  {path}  [FILE]')
                counts['ok_file'] += 1
                continue
            body = blob[path].splitlines()
            n = len(body)
            x = int(l1)
            y = int(l2) if l2 else x
            if x < 1 or y < x or x > n or y > n:
                emit(f'  RANGE     L{lineno}  {path}:{x}-{y}  檔長 {n} 行（越界或反向）')
                counts['range'] += 1
                continue

            seg = '\n'.join(body[x - 1:y])
            stem = Path(path).stem

            span = None
            between = ''
            pos = anchor_pos
            if pos is not None and pos >= 0:
                for m in RE_CODE_SPANS.finditer(line):
                    if m.end() <= pos and (pos - m.end()) <= 6:
                        span = m.group(1)
                        between = line[m.end():pos]
                    if m.start() > pos:
                        break
            is_callsite_ctx = ('呼叫點' in between or 'callsite' in between.lower()
                               or 'call site' in between.lower())
            if span and is_path_or_cli_span(span):
                span = None

            # 明確「呼叫點」字樣緊鄰另一錨點之後：回溯同句最近的非路徑符號片段
            if is_callsite_ctx and span is None:
                last_sym_span = None
                for m in RE_CODE_SPANS.finditer(line):
                    if m.end() <= pos and not is_path_or_cli_span(m.group(1)):
                        last_sym_span = m.group(1)
                    if m.start() > pos:
                        break
                if last_sym_span:
                    cands = [t for t in re.findall(r'[A-Za-z_][A-Za-z0-9_]{2,}', last_sym_span)
                             if t in decl]
                    if cands:
                        ident = cands[-1]
                        if has_word(seg, ident):
                            emit(f'  OK        L{lineno}  {path}:{x}-{y}  ident={ident}  [CALLSITE]')
                            counts['ok_callsite'] += 1
                        else:
                            where = ','.join(f'{p}:{i}' for p, i in decl[ident][:3])
                            emit(f'  STALE     L{lineno}  {path}:{x}-{y}  ident={ident}  '
                                 f'[CALLSITE] 區間無呼叫 token  實際宣告 {where}')
                            counts['stale'] += 1
                        continue
                emit(f'  OK        L{lineno}  {path}:{x}-{y}  [FILE]')
                counts['ok_file'] += 1
                continue

            if not span:
                emit(f'  OK        L{lineno}  {path}:{x}-{y}  [FILE]')
                counts['ok_file'] += 1
                continue

            toks = re.findall(r'[A-Za-z_][A-Za-z0-9_]{2,}', span)
            decl_cands = [t for t in toks if t in decl]

            # FIELD 級：辨識 Type::field、obj.field 或獨立欄位名
            m_qual = RE_QUAL_FIELD.search(span.strip())
            field_pair = None
            if m_qual and m_qual.group(2) in field_decl:
                field_pair = (m_qual.group(1), m_qual.group(2))
            elif toks and toks[-1] in field_decl and toks[-1] not in decl:
                field_pair = (None, toks[-1])

            if field_pair:
                parent_tok, fname = field_pair
                if parent_tok and (path, parent_tok, fname) in struct_field_at:
                    flines = struct_field_at[(path, parent_tok, fname)]
                else:
                    flines = field_at.get((path, fname), set())
                if any(x <= fl <= y for fl in flines):
                    emit(f'  OK        L{lineno}  {path}:{x}-{y}  ident={fname}  [FIELD]')
                    counts['ok_field'] += 1
                    continue
                # 欄位錨點行不命中：若為 Type::field 且 Type 實為同區間結構宣告（保留 v2 舊 ok 零回歸），走宣告判定；
                # 否則若該區間連 Type 宣告也不含，報 STALE 並標示實際欄位行號。
                if decl_cands and decl_cands[-1] in seg:
                    counts['ok_decl'] += 1
                    continue
                if decl_cands:
                    ordered = sorted(field_decl[fname], key=lambda item: (item[1] != path, item[2]))
                    where_f = ','.join(f'{p}:{i}' for _, p, i in ordered[:3])
                    emit(f'  STALE     L{lineno}  {path}:{x}-{y}  ident={fname}  '
                         f'[FIELD] 實際欄位 {where_f}')
                    counts['stale'] += 1
                    continue

            if not decl_cands:
                emit(f'  OK        L{lineno}  {path}:{x}-{y}  [FILE]')
                counts['ok_file'] += 1
                continue

            if '對齊' in between and not any(p == path for k in decl_cands for p, _ in decl[k]):
                emit(f'  OK        L{lineno}  {path}:{x}-{y}  [FILE]')
                counts['ok_file'] += 1
                continue

            key = decl_cands[-1]
            # 模組名 ident（全小寫且等於檔名去副檔名）：降為檔級判定
            if key.islower() and key == stem and key not in seg:
                emit(f'  OK        L{lineno}  {path}:{x}-{y}  ident={key}  [FILE]')
                counts['ok_file'] += 1
                continue

            is_decl_hit = any(p == path and x <= ln <= y for p, ln in decl[key])
            if is_callsite_ctx:
                if has_word(seg, key):
                    emit(f'  OK        L{lineno}  {path}:{x}-{y}  ident={key}  [CALLSITE]')
                    counts['ok_callsite'] += 1
                else:
                    where = ','.join(f'{p}:{i}' for p, i in decl[key][:3])
                    emit(f'  STALE     L{lineno}  {path}:{x}-{y}  ident={key}  '
                         f'[CALLSITE] 區間無呼叫 token  實際宣告 {where}')
                    counts['stale'] += 1
            elif not is_decl_hit and '(' in span and has_word(seg, key):
                emit(f'  OK        L{lineno}  {path}:{x}-{y}  ident={key}  [CALLSITE]')
                counts['ok_callsite'] += 1
            elif key in seg:
                counts['ok_decl'] += 1
            else:
                where = ','.join(f'{p}:{i}' for p, i in decl[key][:3])
                emit(f'  STALE     L{lineno}  {path}:{x}-{y}  ident={key}  實際宣告 {where}')
                counts['stale'] += 1

    ok = counts['ok_decl'] + counts['ok_field'] + counts['ok_callsite'] + counts['ok_file']
    stale = counts['stale']
    miss = counts['miss']
    phantom = counts['phantom']
    proposed = counts['proposed']
    rng = counts['range']
    total = ok + stale + miss + phantom + proposed + rng
    emit(f'AUDIT_SUMMARY ok={ok} stale={stale} missfile={miss} phantom={phantom} '
         f'proposed={proposed} range={rng} total={total} '
         f'ok_decl={counts["ok_decl"]} ok_field={counts["ok_field"]} '
         f'ok_callsite={counts["ok_callsite"]} ok_file={counts["ok_file"]}')
    rc = 1 if (stale or miss or phantom or rng) else 0
    return rc, dict(counts, ok=ok, total=total)


def run_selftest():
    """回歸自測：含正測與兩條謊言反證紅燈斷言。"""
    rs_body = '\n'.join([
        'pub struct Widget {',            # L1
        '    pub shape_boxes: Vec<u8>,',   # L2 (FIELD)
        '    pub other_field: u32,',       # L3
        '}',                               # L4
        'pub fn helper_call(v: u32) {}',   # L5 (DECL)
        'pub fn caller() {',               # L6
        '    helper_call(42);',            # L7 (CALLSITE)
        '    let unrelated = 1;',          # L8 (NO TOKEN)
        '}',                               # L9
    ])
    blob = {'core/demo/src/demo_mod.rs': rs_body}
    fset = set(blob.keys())

    cases = [
        ('正測_DECL', ['- `helper_call`（`core/demo/src/demo_mod.rs:5`）'], 0, 'ok_decl', 1),
        ('正測_FIELD', ['- `Widget::shape_boxes`（`core/demo/src/demo_mod.rs:2`）'], 0, 'ok_field', 1),
        ('正測_CALLSITE', ['- 呼叫 `helper_call()`（呼叫點 `core/demo/src/demo_mod.rs:7`）'], 0, 'ok_callsite', 1),
        ('正測_FILE模組名', ['- `demo_mod`（`core/demo/src/demo_mod.rs:1-4`）'], 0, 'ok_file', 1),
        ('謊言反證_FIELD錯行紅燈', ['- `Widget::shape_boxes`（`core/demo/src/demo_mod.rs:3`）'], 1, 'stale', 1),
        ('謊言反證_CALLSITE無token紅燈', ['- 呼叫 `helper_call()`（呼叫點 `core/demo/src/demo_mod.rs:8`）'], 1, 'stale', 1),
        ('反證_FILE越界紅燈', ['- `demo_mod`（`core/demo/src/demo_mod.rs:99`）'], 1, 'range', 1),
    ]

    failed = 0
    for name, lines, expect_rc, key, expect_val in cases:
        logs = []
        rc, stats = audit_lines(lines, 'docs/selftest.md', fset, blob, emit=logs.append)
        actual_val = stats.get(key, 0)
        passed = (rc == expect_rc and actual_val == expect_val)
        tag = 'PASS' if passed else 'FAIL'
        print(f'[{tag}] {name}: rc={rc} (expect {expect_rc}), {key}={actual_val} (expect {expect_val})')
        for l in logs:
            print(f'    {l}')
        if not passed:
            failed += 1

    print(f'SELFTEST_SUMMARY total={len(cases)} passed={len(cases) - failed} failed={failed}')
    return 1 if failed else 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('ref', nargs='?')
    ap.add_argument('doc', nargs='?')
    ap.add_argument('--repo', default='.',
                    help='要被稽核的 git repo 路徑（預設目前目錄；評 worktree 交付時帶 --repo <worktree>）')
    ap.add_argument('--selftest', action='store_true',
                    help='執行內建回歸自測與謊言反證紅燈檢查')
    a = ap.parse_args()

    if a.selftest:
        return run_selftest()
    if not a.ref or not a.doc:
        ap.error('需要 <ref> 與 <doc> 參數（或使用 --selftest）')

    doc = sh(a.repo, 'show', f'{a.ref}:{a.doc}')
    lines = doc.splitlines()
    fset = set(sh(a.repo, 'ls-tree', '-r', '--name-only', a.ref).split())
    rs = [p for p in fset if p.endswith('.rs')]
    blob = {p: sh(a.repo, 'show', f'{a.ref}:{p}') for p in sorted(rs)}
    rc, _ = audit_lines(lines, a.doc, fset, blob)
    return rc


if __name__ == '__main__':
    raise SystemExit(main())
