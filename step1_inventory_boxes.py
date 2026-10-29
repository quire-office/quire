#!/usr/bin/env python3
"""
step1_inventory_boxes.py
深度盤點 fdo68607.docx 所有 xml part 中的 wps:txbx / mc:AlternateContent / DrawingML / VML 錨點
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

def inspect_all_parts():
    with zipfile.ZipFile(DOCX_PATH, 'r') as z:
        for name in z.namelist():
            if name.endswith('.xml'):
                data = z.read(name)
                # 檢查是否有 IGCSE 或 Drama 或 Glossary
                matches = []
                for kw in [b'IGCSE', b'Drama', b'Glossary', b'txbx', b'AlternateContent', b'v:shape']:
                    cnt = data.count(kw)
                    if cnt > 0:
                        matches.append(f"{kw.decode('ascii')}={cnt}")
                if matches:
                    print(f"[{name}] {', '.join(matches)}")

def inspect_document_details():
    with zipfile.ZipFile(DOCX_PATH, 'r') as z:
        doc_xml = z.read('word/document.xml')
    
    root = ET.fromstring(doc_xml)
    
    print("\n--- Searching for 'IGCSE Drama Glossary' in word/document.xml ---")
    for elem in root.iter():
        if elem.text and 'IGCSE' in elem.text:
            print(f"Found text in tag <{elem.tag}>: '{elem.text}'")
            # 向上追溯父節點鏈
            # ElementTree doesn't have parent pointers by default, build map:
            pass

    parent_map = {c: p for p in root.iter() for c in p}
    
    for elem in root.iter():
        if elem.text and 'IGCSE' in elem.text:
            chain = []
            curr = elem
            while curr in parent_map:
                curr = parent_map[curr]
                tag_name = curr.tag.split('}')[-1]
                chain.append(tag_name)
            print("  Parent chain:", " -> ".join(reversed(chain)))

    print("\n--- Inspecting all AlternateContent / drawing / pict / wps elements in document.xml ---")
    alt_contents = root.findall('.//mc:AlternateContent', NAMESPACES)
    print(f"Total mc:AlternateContent: {len(alt_contents)}")
    for idx, alt in enumerate(alt_contents):
        choice = alt.find('mc:Choice', NAMESPACES)
        fallback = alt.find('mc:Fallback', NAMESPACES)
        choice_requires = choice.attrib.get('Requires') if choice is not None else None
        
        c_texts = [t.text for t in choice.findall('.//w:t', NAMESPACES) if t.text] if choice is not None else []
        f_texts = [t.text for t in fallback.findall('.//w:t', NAMESPACES) if t.text] if fallback is not None else []
        
        print(f"AltContent #{idx}:")
        print(f"  Choice (Requires={choice_requires}) texts: {' '.join(c_texts)}")
        print(f"  Fallback texts: {' '.join(f_texts)}")
        
        # Parent of alt
        p = alt
        p_chain = []
        while p in parent_map:
            p = parent_map[p]
            p_chain.append(p.tag.split('}')[-1])
        print(f"  Parent chain: {' -> '.join(reversed(p_chain))}")

if __name__ == '__main__':
    inspect_all_parts()
    inspect_document_details()
