//! ODF 表格流排版考卷（ODTTBL）。
//!
//! 依據 `docs/odf-table-model-design.md` 規格書驗證 ODF 正文流二維表格模型：
//! - 燈 1（表格邊界攔截）：表格格內段落不再平鋪展開為獨立正文段，表格作為整體物件發射。
//! - 燈 2（二維幾何正確）：同列儲存格 y 相同、各格 x 等於欄軸累加、跨欄格寬度＝被覆蓋欄寬總和。
//! - 燈 3（真值收斂）：LO 25.8 真值比對（表頭 table:table-header-rows 跨頁重發、總頁數收斂）。
//! - 燈 4（零回歸）：語料庫真實表格模型解析與既有口徑零破壞。

use quire_odf::display::{layout_odf_with_tables, DrawItem};
use quire_odf::layout_glue::{project, LayoutParams};
use quire_odf::table::{extract_tables, OdfCellMerge};
use quire_odf::OdfBundle;
use std::path::{Path, PathBuf};

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn corpus_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus-odt")
        .join(rel)
}

fn repo_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(rel)
}

// ---------------------------------------------------------------------------
// 燈 1：表格邊界攔截（格內段不平鋪為獨立正文段，整體物件進發射清單）
// ---------------------------------------------------------------------------

#[test]
fn lamp1_table_boundary_intercept_and_slot_count() {
    let path = fixture_path("table_flow_3x5.odt");
    let bytes = std::fs::read(&path).expect("讀取 table_flow_3x5.odt");
    let bundle = OdfBundle::open(&bytes).expect("開啟封裝");
    let proj = project(&bundle).expect("投影");

    // 1. 段落槽位真相：全檔共 16 段（表前 1 段＋格內 14 段＋表後 1 段），平鋪槽位不變量保持。
    assert_eq!(
        proj.paragraph_count(),
        16,
        "全檔平鋪段落總數應為 16（保持槽位不變量）"
    );

    // 2. 抽取表格模型：必須識別出 1 張頂層表，3 欄×5 列。
    let tables = extract_tables(&bundle).expect("抽取表格");
    assert_eq!(tables.len(), 1, "必須識別出 1 張表格");
    let tbl = &tables[0];
    assert_eq!(tbl.para_start, 1, "表格首個格內段落槽位起點應為 1");
    assert_eq!(tbl.table.grid.len(), 3, "表格應為 3 欄");
    assert_eq!(tbl.table.rows.len(), 5, "表格應為 5 列");

    // 3. 發射清單攔截斷言：格內段落不再作為獨立文字段垂直堆疊，而是以表格物件發射。
    let params = LayoutParams::default();
    let pages = layout_odf_with_tables(&proj, &tables, &params).expect("排版");
    assert_eq!(pages.len(), 1, "3x5 表格排版總頁數應為 1 頁");
    let p0 = &pages[0];

    // 發射清單中必須包含表格的圖元（底紋 Rect、邊框 Rule、儲存格 Text）
    let has_rect = p0
        .items
        .iter()
        .any(|it| matches!(it, DrawItem::Rect { .. }));
    let has_rule = p0
        .items
        .iter()
        .any(|it| matches!(it, DrawItem::Rule { .. }));
    assert!(has_rect, "發射清單必須包含儲存格底紋 Rect");
    assert!(has_rule, "發射清單必須包含儲存格邊框 Rule");

    // 驗證表前正文文字與表後正文文字皆在發射清單中
    let texts: Vec<&str> = p0
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(texts.contains(&"前置正文段落"), "表前段落必須被發射");
    assert!(texts.contains(&"後置正文段落"), "表後段落必須被發射");
    assert!(texts.contains(&"首列跨兩欄格"), "格內段落文字必須被發射");
}

// ---------------------------------------------------------------------------
// 燈 2：二維幾何正確（同列 y 相同、各格 x 等於欄軸累加、跨欄寬度＝覆蓋欄寬和）
// ---------------------------------------------------------------------------

#[test]
fn lamp2_table_2d_geometry_and_spans() {
    let path = fixture_path("table_flow_3x5.odt");
    let bytes = std::fs::read(&path).expect("讀取 table_flow_3x5.odt");
    let bundle = OdfBundle::open(&bytes).expect("開啟封裝");
    let proj = project(&bundle).expect("投影");
    let tables = extract_tables(&bundle).expect("抽取表格");
    let params = LayoutParams::default();
    let pages = layout_odf_with_tables(&proj, &tables, &params).expect("排版");
    let p0 = &pages[0];

    // 檢視第一列（Row 1：次列第一格、次列第二格、次列第三格）三個文字項
    let find_text_item = |target: &str| -> (f32, f32) {
        for it in &p0.items {
            if let DrawItem::Text { x, y, text, .. } = it {
                if text == target {
                    return (*x, *y);
                }
            }
        }
        panic!("找不到文字項: {target}");
    };

    let (r1c0_x, r1c0_y) = find_text_item("次列第一格");
    let (r1c1_x, r1c1_y) = find_text_item("次列第二格");
    let (r1c2_x, r1c2_y) = find_text_item("次列第三格");

    // 斷言 1：同列儲存格 y 座標相同（容差 0.1pt）
    assert!(
        (r1c0_y - r1c1_y).abs() < 0.1 && (r1c1_y - r1c2_y).abs() < 0.1,
        "同列儲存格文字 y 必須相等: y0={r1c0_y}, y1={r1c1_y}, y2={r1c2_y}"
    );

    // 斷言 2：各格 x 座標按欄寬遞增累加
    // 欄寬分別為 3cm (~85.04pt)、4cm (~113.39pt)、5cm (~141.73pt)
    let dx01 = r1c1_x - r1c0_x;
    let dx12 = r1c2_x - r1c1_x;
    assert!(
        (dx01 - 85.04).abs() < 1.0,
        "欄 0 寬度約為 85.04pt (3cm)，實際位移 dx01={dx01}"
    );
    assert!(
        (dx12 - 113.39).abs() < 1.0,
        "欄 1 寬度約為 113.39pt (4cm)，實際位移 dx12={dx12}"
    );

    // 斷言 3：跨欄寬度＝被覆蓋欄寬總和
    // 首列首格為 span=2 底紋 Rect，其寬度應為 3cm + 4cm = 7cm (~198.43pt)
    let shaded_rect = p0
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Rect { w, fill_argb, .. } if *fill_argb == 0xffd9_e1f2 => Some(*w),
            _ => None,
        })
        .expect("必須發射底紋 Rect");
    assert!(
        (shaded_rect - 198.43).abs() < 1.0,
        "跨 2 欄儲存格底紋寬度應約為 198.43pt (7cm)，實際: {shaded_rect}"
    );
}

// ---------------------------------------------------------------------------
// 燈 3：真值收斂（以 LO 25.8 轉 PDF 比對跨頁切片與表頭重發）
// ---------------------------------------------------------------------------

#[test]
fn lamp3_libreoffice_truth_and_header_repeat() {
    let path = fixture_path("table_flow_header_split.odt");
    let bytes = std::fs::read(&path).expect("讀取 table_flow_header_split.odt");
    let bundle = OdfBundle::open(&bytes).expect("開啟封裝");
    let proj = project(&bundle).expect("投影");
    let tables = extract_tables(&bundle).expect("抽取表格");

    // 樣式版面參數：頁高 14.0cm (~396.85pt)，邊距各 2.0cm (~56.69pt)
    let params = LayoutParams {
        page_w_pt: 21.0 * (72.0 / 2.54),
        page_h_pt: 14.0 * (72.0 / 2.54),
        margins_pt: [
            2.0 * (72.0 / 2.54),
            2.0 * (72.0 / 2.54),
            2.0 * (72.0 / 2.54),
            2.0 * (72.0 / 2.54),
        ],
        font_size_pt: 12.0,
        line_spacing: 1.0,
        ..Default::default()
    };

    let pages = layout_odf_with_tables(&proj, &tables, &params).expect("排版");

    // 1. 頁數收斂至 LO 25.8 真值（2 頁）
    assert_eq!(
        pages.len(),
        2,
        "table_flow_header_split 總頁數必須收斂至 LO 25.8 真值（2 頁）"
    );

    // 2. 表頭重複驗證：第 1 頁與第 2 頁頂端皆必須發射表頭文字
    let p0_has_hdr = pages[0].items.iter().any(|it| match it {
        DrawItem::Text { text, .. } => text == "表頭甲",
        _ => false,
    });
    let p1_has_hdr = pages[1].items.iter().any(|it| match it {
        DrawItem::Text { text, .. } => text == "表頭甲",
        _ => false,
    });
    assert!(p0_has_hdr, "第 1 頁必須包含表頭列");
    assert!(p1_has_hdr, "第 2 頁必須重複發射 table-header-rows 表頭列");
}

// ---------------------------------------------------------------------------
// 燈 4：零回歸與真實語料表格模型解析驗證（設計書 5 大基準測試）
// ---------------------------------------------------------------------------

#[test]
fn lamp4_design_doc_benchmarks_and_zero_regression() {
    // 基準 1：網格欄寬展開與重複欄解析（sample2_table_frame.odt）
    let s2_bytes =
        std::fs::read(repo_path("docs/odf-design/sample2_table_frame.odt")).expect("讀取 sample2");
    let s2_bundle = OdfBundle::open(&s2_bytes).expect("開啟 sample2");
    let s2_tables = extract_tables(&s2_bundle).expect("抽取表格");
    assert_eq!(s2_tables.len(), 1, "sample2 必須識別出 1 張表格");
    let s2_tbl = &s2_tables[0];
    assert_eq!(
        s2_tbl.table.grid.len(),
        3,
        "number-columns-repeated='3' 必須展開為 3 欄"
    );
    for col in &s2_tbl.table.grid {
        let w = col.w_pt.expect("欄寬必須解析成功");
        assert!(
            (w - 160.64).abs() < 1.0,
            "欄寬 5.667cm 應約為 160.64pt，實際: {w}"
        );
    }
    assert_eq!(s2_tbl.table.rows.len(), 3, "表格總列數應為 3");

    // 基準 2：水平跨欄與覆蓋格略過狀態機
    let row0 = &s2_tbl.table.rows[0];
    assert_eq!(
        row0.cells.len(),
        2,
        "水平跨 2 欄之列，有效儲存格數應為 2（covered 格略過）"
    );
    assert_eq!(row0.cells[0].props.grid_span, 2, "首格 grid_span 應為 2");
    assert_eq!(row0.cells[1].props.grid_span, 1, "第 2 格 grid_span 應為 1");

    // 基準 3：垂直跨列合併狀態機（row 1 cell 0 為 Restart，row 2 延續 Continue）
    let row1 = &s2_tbl.table.rows[1];
    assert_eq!(
        row1.cells[0].props.v_merge,
        Some(OdfCellMerge::Restart),
        "垂直跨列首格應為 Restart"
    );
    let row2 = &s2_tbl.table.rows[2];
    assert_eq!(
        row2.cells[0].props.v_merge,
        Some(OdfCellMerge::Continue),
        "垂直跨列延續格應為 Continue"
    );

    // 基準 4：真實開源語料表頭列統計（bug57031.odt 包含 25 個表頭列）
    let b57_path = corpus_path("lo_out/lo__poi__test-data_document_bug57031.odt");
    let b57_bytes = std::fs::read(&b57_path).expect("讀取 bug57031");
    let b57_bundle = OdfBundle::open(&b57_bytes).expect("開啟 bug57031");
    let b57_tables = extract_tables(&b57_bundle).expect("抽取 bug57031 表格");
    let total_headers: usize = b57_tables
        .iter()
        .flat_map(|t| &t.table.rows)
        .filter(|r| r.props.is_header)
        .count();
    assert_eq!(
        total_headers, 25,
        "bug57031 中包裹在 table-header-rows 的表頭列總數應為 25"
    );

    // 基準 5：跨頁大表格排版收斂至 LO 2 頁（fdo77887.odt）
    let fdo_path = corpus_path("lo_out/lo__ooxmlexport__fdo77887.odt");
    let fdo_bytes = std::fs::read(&fdo_path).expect("讀取 fdo77887");
    let fdo_bundle = OdfBundle::open(&fdo_bytes).expect("開啟 fdo77887");
    let fdo_proj = project(&fdo_bundle).expect("投影 fdo77887");
    let fdo_tables = extract_tables(&fdo_bundle).expect("抽取 fdo77887 表格");
    assert_eq!(fdo_tables.len(), 2, "fdo77887 應包含 2 張表格");
    assert_eq!(
        fdo_tables.iter().map(|t| t.table.rows.len()).sum::<usize>(),
        77,
        "總列數應為 77 列（設計書筆誤將 153 格記為 153 列）"
    );
    assert_eq!(
        fdo_tables
            .iter()
            .flat_map(|t| &t.table.rows)
            .map(|r| r.cells.len())
            .sum::<usize>(),
        153,
        "總儲存格數應為 153 格"
    );
    let geo = quire_odf::page_geometry(&fdo_bundle).expect("頁幾何");
    let params = LayoutParams {
        page_w_pt: geo.page_w_pt(),
        page_h_pt: geo.page_h_pt(),
        margins_pt: geo.margins_pt(),
        font_size_pt: 9.0,
        line_spacing: 1.0,
        ..Default::default()
    };
    let fdo_pages = layout_odf_with_tables(&fdo_proj, &fdo_tables, &params).expect("排版 fdo77887");
    assert_eq!(
        fdo_pages.len(),
        2,
        "fdo77887 表格排版總頁數必須收斂至 LO 真值 2 頁"
    );
}
