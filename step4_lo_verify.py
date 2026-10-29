#!/usr/bin/env python3
"""
step4_lo_verify.py
抽樣 10-15 檔使用 scripts/render_lab.py 的 LO 渲染，驗證 LO 是否發射該 wps:txbx 文字。
跑完呼叫 kill_own_lo 收場。
"""

import json
import pymupdf
from pathlib import Path
import scripts.render_lab as rl

REPO_ROOT = Path(__file__).resolve().parent
RESULTS_FILE = REPO_ROOT / "step3_scan_results.json"

def main():
    with open(RESULTS_FILE, "r", encoding="utf-8") as f:
        missing_files = json.load(f)

    print(f"Total missing candidates: {len(missing_files)}")
    
    soffice = rl.find_soffice()
    print("Found soffice:", soffice)
    scratch, profile_uri, profile_dir = rl.lo_scratch(soffice)
    
    # 抽樣 12 檔
    sample = missing_files[:13]
    
    lo_verifications = []

    try:
        for item in sample:
            rel_file = item["file"]
            docx_path = REPO_ROOT / rel_file
            print(f"\n--- Testing {rel_file} ---")
            
            pdf_path = rl.lo_to_pdf(soffice, docx_path, scratch, profile_uri)
            if not pdf_path or not pdf_path.exists():
                print(f"  LO conversion failed for {rel_file}")
                lo_verifications.append({
                    "file": rel_file,
                    "lo_success": False,
                    "lo_emitted": None,
                })
                continue
            
            doc = pymupdf.open(pdf_path)
            full_pdf_text = []
            for p in doc:
                full_pdf_text.append(p.get_text())
            pdf_blob = " ".join(full_pdf_text)
            
            # 檢查缺漏的項目在 LO PDF 中是否存在
            emitted_in_lo = []
            for m in item["missing_details"]:
                kw = m["keyword"]
                found = kw.lower() in pdf_blob.lower()
                emitted_in_lo.append({
                    "keyword": kw,
                    "text": m["text"],
                    "found_in_lo": found,
                    "in_cell": m["in_cell"],
                    "is_anchor": m["is_anchor"],
                })
                print(f"  Keyword '{kw}' in LO? {found} (in_cell={m['in_cell']}, is_anchor={m['is_anchor']})")

            lo_verifications.append({
                "file": rel_file,
                "lo_success": True,
                "items": emitted_in_lo,
            })
    finally:
        rl.kill_own_lo("quire-render-lab")
        print("\nCleaned up LO processes.")

    out_json = REPO_ROOT / "step4_lo_results.json"
    with open(out_json, "w", encoding="utf-8") as f:
        json.dump(lo_verifications, f, ensure_ascii=False, indent=2)
    print(f"Saved LO verification results to {out_json.name}")

if __name__ == '__main__':
    main()
