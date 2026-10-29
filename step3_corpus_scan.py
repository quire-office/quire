#!/usr/bin/env python3
"""
step3_corpus_scan.py
全量掃描 corpus/ 400 檔：
1. 找出包含 wps:txbx 的檔案
2. 提取 wps:txbx 內的文字
3. 與 quire-cli dump 比對，找出「wps:txbx 文字在我方 dump 缺漏」的檔案
4. 分析這些缺漏檔是否與 table cell / in_cell 相關
"""

import os
import sys
import json
import zipfile
import subprocess
import xml.etree.ElementTree as ET
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent
CORPUS_DIR = REPO_ROOT / "corpus"
QUIRE_CLI = REPO_ROOT / "target" / "release" / "quire-cli"

NAMESPACES = {
    'w': 'http://schemas.openxmlformats.org/wordprocessingml/2006/main',
    'wp': 'http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing',
    'a': 'http://schemas.openxmlformats.org/drawingml/2006/main',
    'pic': 'http://schemas.openxmlformats.org/drawingml/2006/picture',
    'wps': 'http://schemas.microsoft.com/office/word/2010/wordprocessingShape',
    'wpg': 'http://schemas.microsoft.com/office/word/2010/wordprocessingGroup',
    'mc': 'http://schemas.openxmlformats.org/markup-compatibility/2006',
    'v': 'urn:schemas-microsoft-com:vml',
}

def scan_corpus():
    all_docx = sorted(CORPUS_DIR.rglob("*.docx"))
    print(f"Total docx files found: {len(all_docx)}")

    files_with_wps_txbx = []

    for docx_path in all_docx:
        rel_path = docx_path.relative_to(REPO_ROOT)
        try:
            with zipfile.ZipFile(docx_path, 'r') as z:
                # 檢查 document.xml 及 header/footer xml
                for name in z.namelist():
                    if name.startswith('word/') and name.endswith('.xml'):
                        xml_bytes = z.read(name)
                        if b'wps:txbx' in xml_bytes or b'txbxContent' in xml_bytes:
                            files_with_wps_txbx.append((docx_path, name))
                            break
        except Exception as e:
            continue

    print(f"Files with wps:txbx / txbxContent: {len(files_with_wps_txbx)}")

    results = []

    for docx_path, xml_name in files_with_wps_txbx:
        rel_path = str(docx_path.relative_to(REPO_ROOT))
        # 提取 wps:txbx 內的文字
        txbx_texts = []
        in_cell_flags = []
        try:
            with zipfile.ZipFile(docx_path, 'r') as z:
                xml_bytes = z.read(xml_name)
            root = ET.fromstring(xml_bytes)
            parent_map = {c: p for p in root.iter() for c in p}
            
            # 專門找 wps:txbx
            for txbx in root.findall('.//wps:txbx', NAMESPACES):
                texts = [t.text for t in txbx.findall('.//w:t', NAMESPACES) if t.text and t.text.strip()]
                full_t = " ".join(texts).strip()
                if full_t:
                    # 檢查此 txbx 是否在 w:tc 內
                    curr = txbx
                    is_in_cell = False
                    is_anchor = False
                    while curr in parent_map:
                        curr = parent_map[curr]
                        tag = curr.tag.split('}')[-1]
                        if tag == 'tc':
                            is_in_cell = True
                        if tag == 'anchor':
                            is_anchor = True
                    txbx_texts.append(full_t)
                    in_cell_flags.append((is_in_cell, is_anchor))
        except Exception as e:
            continue

        if not txbx_texts:
            continue

        # 跑 quire-cli dump
        try:
            res = subprocess.run([str(QUIRE_CLI), "dump", str(docx_path)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, timeout=15)
            if res.returncode != 0:
                print(f"Error dumping {rel_path}: {res.stderr[:100]}")
                continue
            dump_pages = json.loads(res.stdout)
            dump_all_texts = []
            for p in dump_pages:
                for it in p.get("items", []):
                    if "Text" in it:
                        dump_all_texts.append(it["Text"].get("text", ""))
            dump_blob = " ".join(dump_all_texts)
        except Exception as e:
            print(f"Dump exception on {rel_path}: {e}")
            continue

        missing = []
        for idx, (t, (in_cell, is_anchor)) in enumerate(zip(txbx_texts, in_cell_flags)):
            # 抽關鍵字檢驗
            words = [w for w in t.split() if len(w) >= 3]
            kw = words[0] if words else t
            if kw not in dump_blob:
                missing.append({
                    "text": t[:60],
                    "keyword": kw,
                    "in_cell": in_cell,
                    "is_anchor": is_anchor,
                })

        if missing:
            results.append({
                "file": rel_path,
                "total_wps_txbx": len(txbx_texts),
                "missing_count": len(missing),
                "missing_details": missing,
            })
            print(f"[MISSING] {rel_path}: {len(missing)}/{len(txbx_texts)} missed (in_cell={missing[0]['in_cell']}, is_anchor={missing[0]['is_anchor']})")
        else:
            print(f"[OK] {rel_path}: all {len(txbx_texts)} emitted")

    output_json = REPO_ROOT / "step3_scan_results.json"
    with open(output_json, "w", encoding="utf-8") as f:
        json.dump(results, f, ensure_ascii=False, indent=2)
    print(f"\nScan completed. Found {len(results)} files with missing wps:txbx text.")

if __name__ == '__main__':
    scan_corpus()
