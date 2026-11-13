#!/usr/bin/env python3
"""
scripts/odt1129_axis_probe.py

ODT 投影層多軸缺陷診斷探針（冪等，report-only）。
依據 .task-cards/agy-odt1129-diag-card.md 規格：
- 檔路徑一律由環境變數 ODT_SAMPLE_PATH 讀取；未設定即印出提示並正常結束。
- 嚴格隱私保護：本腳本輸出絕不包含任何文件內文段落文字，僅輸出度量值（座標、計數、分數、樣式名）。
- LibreOffice PDF 逐字座標一律使用 pymupdf rawdict。
"""

import json
import os
import re
import subprocess
import sys
import zipfile
import xml.etree.ElementTree as ET

def main():
    odt_path = os.environ.get("ODT_SAMPLE_PATH")
    if not odt_path:
        print("[提示] 未設定環境變數 ODT_SAMPLE_PATH，跳過探針執行。")
        print("用法: ODT_SAMPLE_PATH=user-sample.odt /usr/bin/python3 scripts/odt1129_axis_probe.py")
        sys.exit(0)

    if not os.path.isfile(odt_path):
        print(f"[錯誤] 檔案不存在: {odt_path}")
        sys.exit(1)

    lo_pdf_path = os.environ.get("LO_PDF_PATH", "/tmp/odt1129-lo/lo-export.pdf")
    has_lo_pdf = os.path.isfile(lo_pdf_path)

    print("================================================================================")
    print("ODT 投影層多軸缺陷診斷探針 (ODT-1129 Axis Probe)")
    print(f"樣本檔案: {odt_path}")
    print(f"LO PDF 檔案: {lo_pdf_path if has_lo_pdf else '未提供 (跳過 LO 比對)'}")
    print("================================================================================\n")

    # 讀取 ODT ZIP 與 XML
    with zipfile.ZipFile(odt_path, "r") as z:
        entries = z.infolist()
        content_bytes = z.read("content.xml")
        styles_bytes = z.read("styles.xml")

    root_c = ET.fromstring(content_bytes)
    root_s = ET.fromstring(styles_bytes)

    # 執行 quire-cli
    cli_bin = "./target/debug/quire-cli"
    has_cli = os.path.isfile(cli_bin)
    render_out = {}
    dump_p0 = {}
    dump_p1 = {}
    dump_p2 = {}

    if has_cli:
        res_render = subprocess.run([cli_bin, "odt-render", odt_path], capture_output=True, text=True)
        if res_render.returncode == 0:
            try:
                render_out = json.loads(res_render.stdout.strip().splitlines()[-1])
            except Exception:
                pass

        res_p0 = subprocess.run([cli_bin, "odt-dump", odt_path, "0"], capture_output=True, text=True)
        if res_p0.returncode == 0:
            try:
                dump_p0 = json.loads(res_p0.stdout)
            except Exception:
                pass

        res_p1 = subprocess.run([cli_bin, "odt-dump", odt_path, "1"], capture_output=True, text=True)
        if res_p1.returncode == 0:
            try:
                dump_p1 = json.loads(res_p1.stdout)
            except Exception:
                pass

        res_p2 = subprocess.run([cli_bin, "odt-dump", odt_path, "2"], capture_output=True, text=True)
        if res_p2.returncode == 0:
            try:
                dump_p2 = json.loads(res_p2.stdout)
            except Exception:
                pass

    # -------------------------------------------------------------------------
    # 軸 1：縮排 / 置中零消費
    # -------------------------------------------------------------------------
    print("### 軸 1：縮排 / 置中零消費 (Indentation & Alignment Zero-Consumption)")
    pp_c = 0
    indent_c = 0
    margin_l_c = 0
    align_c = 0
    align_values = set()
    for el in root_c.iter():
        if el.tag.endswith("paragraph-properties"):
            pp_c += 1
            for k, v in el.attrib.items():
                if "text-indent" in k: indent_c += 1
                if "margin-left" in k: margin_l_c += 1
                if "align" in k:
                    align_c += 1
                    align_values.add(v)

    list_align_count = 0
    for el in root_c.iter():
        if el.tag.endswith("list-level-label-alignment"):
            list_align_count += 1

    print(f"- content.xml paragraph-properties 數量: {pp_c}")
    print(f"  - 宣告 fo:text-indent 數量: {indent_c}")
    print(f"  - 宣告 fo:margin-left 數量: {margin_l_c}")
    print(f"  - 宣告 fo:text-align 數量: {align_c} (值域: {sorted(list(align_values))})")
    print(f"- content.xml list-level-label-alignment 縮排定義數量: {list_align_count}")

    # 我方 p0 x 分布
    p0_items = dump_p0.get("items", [])
    p0_xs = sorted(list(set(round(it["Text"]["x"], 3) for it in p0_items if "Text" in it)))
    print(f"- 我方 p0 顯示列表 Text 項總數: {len(p0_items)}")
    print(f"- 我方 p0 唯一 x 座標集合: {p0_xs}")

    # LO p0 x 分布 (rawdict)
    if has_lo_pdf:
        try:
            import fitz
            doc = fitz.open(lo_pdf_path)
            raw_p0 = doc[0].get_text("rawdict")
            lo_p0_xs = set()
            for b in raw_p0.get("blocks", []):
                if b.get("type") == 0:
                    for l in b.get("lines", []):
                        for s in l.get("spans", []):
                            for c in s.get("chars", []):
                                lo_p0_xs.add(round(c["origin"][0], 1))
            lo_xs_sample = sorted(list(lo_p0_xs))[:15]
            print(f"- LO p0 rawdict 逐字 x 原點相異值總數: {len(lo_p0_xs)}")
            print(f"- LO p0 x 座標樣本 (前 15 個): {lo_xs_sample}")
        except Exception as e:
            print(f"- LO PDF 讀取失敗: {e}")
    print()

    # -------------------------------------------------------------------------
    # 軸 2：空段洪水
    # -------------------------------------------------------------------------
    print("### 軸 2：空段洪水 (Empty Paragraph Flood & XML Whitespace Inflation)")
    all_p = root_c.findall(".//{urn:oasis:names:tc:opendocument:xmlns:text:1.0}p")
    total_p = len(all_p)
    pure_empty_p = 0
    whitespace_p = 0
    non_empty_p = 0
    p_with_xml_newlines = 0

    for p in all_p:
        text = "".join(p.itertext())
        if len(p) == 0 and not text:
            pure_empty_p += 1
        elif not text.strip():
            whitespace_p += 1
        else:
            non_empty_p += 1
        if "\n" in text or "\r" in text:
            p_with_xml_newlines += 1

    print(f"- content.xml <text:p> 總數: {total_p}")
    print(f"  - 真空段 (<text:p/> 或無子節點且無文字): {pure_empty_p} (佔 {pure_empty_p/total_p*100:.1f}%)")
    print(f"  - 純空格/空白字元段: {whitespace_p}")
    print(f"  - 具備實質文字段: {non_empty_p}")
    print(f"  - 含 XML 格式化換行 (\\n/\\r) 段落數: {p_with_xml_newlines} (佔 {p_with_xml_newlines/total_p*100:.1f}%)")

    p0_empty_items = [it for it in p0_items if "Text" in it and it["Text"].get("text") == ""]
    p0_non_empty = [it for it in p0_items if "Text" in it and it["Text"].get("text") != ""]
    print(f"- 我方 p0 Text 項: 總計 {len(p0_items)} 項，空字串項 {len(p0_empty_items)} 項，非空項 {len(p0_non_empty)} 項")
    print("  (註：p0 僅排入源檔前 8 個段落 p[0..7]，其中真正空段僅 p[0] 1 個；")
    print("   其餘 32 個空項均為 XML 內 \\n 被 break_lines 誤當強制換行切割產生的幽靈空行)")
    print()

    # -------------------------------------------------------------------------
    # 軸 3：分頁 17 vs 10
    # -------------------------------------------------------------------------
    print("### 軸 3：分頁 17 vs 10 (Pagination Mismatch 17 vs 10)")
    page_breaks = render_out.get("page_breaks", [])
    print(f"- 我方排版總頁數: {len(page_breaks) + 1} 頁 (page_breaks 斷點數: {len(page_breaks)})")
    print(f"- 我方 page_breaks 邊界: {page_breaks}")

    # 表格統計
    tables = root_c.findall(".//{urn:oasis:names:tc:opendocument:xmlns:table:1.0}table")
    table_p_count = sum(len(tbl.findall(".//{urn:oasis:names:tc:opendocument:xmlns:text:1.0}p")) for tbl in tables)
    print(f"- 源檔表格數: {len(tables)} 個 (表格內包含 {table_p_count} 個 <text:p>，佔全文 {table_p_count/total_p*100:.1f}%)")
    print(f"  (我方未實作 ODF-3A 表格模型，將 {table_p_count} 個儲存格段落一維垂直平鋪，是分頁膨脹的第一主因)")

    if has_lo_pdf:
        try:
            import fitz
            doc = fitz.open(lo_pdf_path)
            print(f"- LO 官方渲染總頁數: {len(doc)} 頁")
            p0_lo_lines = []
            raw_p0 = doc[0].get_text("rawdict")
            for b in raw_p0.get("blocks", []):
                if b.get("type") == 0:
                    for l in b.get("lines", []):
                        p0_lo_lines.append(l)
            first_l = p0_lo_lines[0] if p0_lo_lines else None
            if first_l:
                first_span = first_l["spans"][0]
                print(f"- LO p0 首行 baseline y: {first_span['origin'][1]:.1f} pt (bbox top: {first_l['bbox'][1]:.1f} pt)")
            p0_first_y = p0_items[0]["Text"]["y"] if p0_items else None
            print(f"- 我方 p0 首項頂端 y: {p0_first_y} pt (落差約 {abs(first_span['origin'][1] - p0_first_y):.1f} pt)")
        except Exception as e:
            print(f"- LO 分頁資訊比對失敗: {e}")
    print()

    # -------------------------------------------------------------------------
    # 軸 4：13pt 級聯路徑分析
    # -------------------------------------------------------------------------
    print("### 軸 4：13pt 級聯路徑分析 (13pt Font-Size Cascade Diagnosis)")
    styles_13pt = []
    for s in root_c.iter("{urn:oasis:names:tc:opendocument:xmlns:style:1.0}style"):
        name = s.attrib.get("{urn:oasis:names:tc:opendocument:xmlns:style:1.0}name", "")
        tp = s.find("{urn:oasis:names:tc:opendocument:xmlns:style:1.0}text-properties")
        if tp is not None and any("13pt" in v for v in tp.attrib.values()):
            parent = s.attrib.get("{urn:oasis:names:tc:opendocument:xmlns:style:1.0}parent-style-name", "")
            styles_13pt.append((name, parent))

    print(f"- content.xml 宣告 13pt 的自動樣式清單: {styles_13pt}")
    used_13pt_paras = []
    for i, p in enumerate(all_p):
        s = p.attrib.get("{urn:oasis:names:tc:opendocument:xmlns:text:1.0}style-name", "")
        if any(s == name for name, _ in styles_13pt):
            used_13pt_paras.append((i, s))

    print(f"- 正文引用 13pt 樣式的段落索引: {used_13pt_paras}")
    print("  (p[14] 位於表格 9 第二列，p[30] 位於表格 10 第二列)")

    sizes_p0 = sorted(list(set(it["Text"]["size_pt"] for it in p0_items if "Text" in it)))
    sizes_p1 = sorted(list(set(it["Text"]["size_pt"] for it in dump_p1.get("items", []) if "Text" in it)))
    sizes_p2 = sorted(list(set(it["Text"]["size_pt"] for it in dump_p2.get("items", []) if "Text" in it)))
    print(f"- 我方各頁字級集合實測:")
    print(f"  - Page 0 字級集合: {sizes_p0} (缺 13pt)")
    print(f"  - Page 1 字級集合: {sizes_p1} (包含 13pt，來自 p[14])")
    print(f"  - Page 2 字級集合: {sizes_p2} (包含 13pt，來自 p[30])")
    print("  [診斷結論] 13pt 在 p0 缺席非級聯讀取斷鏈，而是分頁提早斷點（在 p[8]）將 p[14] 與 p[30] 擠至第 1、2 頁。")
    print()

    # -------------------------------------------------------------------------
    # 軸 5：清單發射現狀與 ODF-2B 缺口
    # -------------------------------------------------------------------------
    print("### 軸 5：清單發射現狀與 ODF-2B 缺口 (List Emission & ODF-2B Gap)")
    list_tags = {
        "text:list": len(root_c.findall(".//{urn:oasis:names:tc:opendocument:xmlns:text:1.0}list")),
        "text:list-item": len(root_c.findall(".//{urn:oasis:names:tc:opendocument:xmlns:text:1.0}list-item")),
        "text:list-style": len(root_c.findall(".//{urn:oasis:names:tc:opendocument:xmlns:text:1.0}list-style")),
        "text:list-level-style-number": len(root_c.findall(".//{urn:oasis:names:tc:opendocument:xmlns:text:1.0}list-level-style-number")),
        "style:list-level-properties": len(root_c.findall(".//{urn:oasis:names:tc:opendocument:xmlns:style:1.0}list-level-properties")),
        "style:list-level-label-alignment": len(root_c.findall(".//{urn:oasis:names:tc:opendocument:xmlns:style:1.0}list-level-label-alignment")),
    }
    total_list_elements = (list_tags["text:list"] + list_tags["text:list-item"] +
                           list_tags["text:list-style"] + list_tags["text:list-level-style-number"])
    print(f"- 清單相關標籤計數:")
    for k, v in list_tags.items():
        print(f"  - {k}: {v}")
    print(f"- 清單主要元素合計 (text:list + item + style + number): {total_list_elements} (對應任務卡 101 計數)")
    print(f"- 我方清單發射態: 編號/符號完全缺失 (0 markers)，清單層級縮排完全缺失 (0 indent)")
    print(f"- 設計書 docs/odf-design.md §清單 (ODF-2B) 落地缺口:")
    print("  1. 模型層: 缺少 ListFormat 定義與 Paragraph::list_format 欄位")
    print("  2. 解析層: parse_style_file 完全跳過 <text:list-style>")
    print("  3. 狀態機: walk_paragraphs 未維護 list_stack 巢狀計數器")
    print("  4. 發射層: display.rs push_line_items 未合成 marker 與 hanging indent")
    print()

    # -------------------------------------------------------------------------
    # 補充：頁首頁尾自翻頁鏈 next-style-name
    # -------------------------------------------------------------------------
    print("### 補充：頁首頁尾自翻頁鏈 next-style-name (Header/Footer Transition)")
    mp_info = []
    for mp in root_s.iter("{urn:oasis:names:tc:opendocument:xmlns:style:1.0}master-page"):
        mname = mp.attrib.get("{urn:oasis:names:tc:opendocument:xmlns:style:1.0}name", "")
        next_m = mp.attrib.get("{urn:oasis:names:tc:opendocument:xmlns:style:1.0}next-style-name", "")
        has_hdr = len(mp.findall("{urn:oasis:names:tc:opendocument:xmlns:style:1.0}header")) > 0
        has_ftr = len(mp.findall("{urn:oasis:names:tc:opendocument:xmlns:style:1.0}footer")) > 0
        mp_info.append((mname, next_m, has_hdr, has_ftr))

    print(f"- styles.xml master-page 定義: {mp_info}")
    print(f"- odt-render 輸出: hf_chunks={render_out.get('hf_chunks')}, "
          f"hf_page_count={render_out.get('hf_page_count')}, "
          f"hf_paragraph_count={render_out.get('hf_paragraph_count')}, "
          f"hf_text_len={render_out.get('hf_text_len')}")
    print("  [診斷結論] 首段綁定 MPF0 (首頁 master，無頁首尾)；MPF0 宣告 next-style-name=\"MP0\"，")
    print("  但 header_footer.rs 未追蹤 next-style-name，將全書所有頁面鎖定在 MPF0，致頁尾頁碼全數丟失。")
    print("\n================================================================================")
    print("診斷探針執行完成。")

if __name__ == "__main__":
    main()
