#!/usr/bin/env python3
"""
step2_verify_hypotheses.py
驗證 fdo68607.docx 中所有的 drawing / shape / textbox / alternateContent / anchor
並逐一排查「同檔其他 WPS 文字框（頁首『Box set』等）有發射」這句話所指為何。
"""

import zipfile
import xml.etree.ElementTree as ET
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent
DOCX_PATH = REPO_ROOT / "corpus" / "ooxmlexport" / "fdo68607.docx"

NAMESPACES = {
    'w': 'http://schemas.openxmlformats.org/wordprocessingml/2006/main',
    'wp': 'http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing',
    'a': 'http://schemas.openxmlformats.org/drawingml/2006/main',
    'pic': 'http://schemas.openxmlformats.org/drawingml/2006/picture',
    'wps': 'http://schemas.microsoft.com/office/word/2010/wordprocessingShape',
    'wpg': 'http://schemas.microsoft.com/office/word/2010/wordprocessingGroup',
    'mc': 'http://schemas.openxmlformats.org/markup-compatibility/2006',
    'v': 'urn:schemas-microsoft-com:vml',
    'o': 'urn:schemas-microsoft-com:office:office',
    'w14': 'http://schemas.microsoft.com/office/word/2010/wordml',
}

def check_all_drawings_in_fdo68607():
    with zipfile.ZipFile(DOCX_PATH, 'r') as z:
        doc_xml = z.read('word/document.xml')
    
    root = ET.fromstring(doc_xml)
    
    print("=== 全文 DrawingML / VML / AlternateContent 節點統計 ===")
    drawings = root.findall('.//w:drawing', NAMESPACES)
    picts = root.findall('.//w:pict', NAMESPACES)
    anchors = root.findall('.//wp:anchor', NAMESPACES)
    inlines = root.findall('.//wp:inline', NAMESPACES)
    wps_txbxs = root.findall('.//wps:txbx', NAMESPACES)
    v_shapes = root.findall('.//v:shape', NAMESPACES)
    v_textboxes = root.findall('.//v:textbox', NAMESPACES)
    w_txbxContents = root.findall('.//w:txbxContent', NAMESPACES)
    
    print(f"w:drawing: {len(drawings)}")
    print(f"w:pict: {len(picts)}")
    print(f"wp:anchor: {len(anchors)}")
    print(f"wp:inline: {len(inlines)}")
    print(f"wps:txbx: {len(wps_txbxs)}")
    print(f"v:shape: {len(v_shapes)}")
    print(f"v:textbox: {len(v_textboxes)}")
    print(f"w:txbxContent: {len(w_txbxContents)}")

    # 查 Box set 的所在位置
    print("\n=== 尋找 'Box set' ===")
    for p_idx, p in enumerate(root.findall('.//w:p', NAMESPACES)):
        text = "".join(p.itertext())
        if "Box set" in text or "Box Set" in text:
            print(f"P #{p_idx}: text='{text}'")
            # 檢查這個 p 是否有 drawing
            d = p.findall('.//w:drawing', NAMESPACES)
            pi = p.findall('.//w:pict', NAMESPACES)
            print(f"  drawings={len(d)}, picts={len(pi)}")

if __name__ == '__main__':
    check_all_drawings_in_fdo68607()
