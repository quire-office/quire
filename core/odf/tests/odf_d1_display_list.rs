//! ODF-D1 紅燈考卷：ODT 顯示列表發射層（`OdfProjection` → `PageDisplayList`）。
//!
//! 口徑來源：`docs/odf-d1-anatomy.md`（契約解剖）＋`docs/odf-d1-redlight.md`（先紅實據）。
//! 紅燈 commit 先拍紅，四盞燈與契約基準檔 `fixtures/display_list_contract.json`
//! 一起進 commit，事後不得改。
//!
//! 四盞燈（卡面 t_fee8234e → t_fee82b4e）：
//! 1. 語料 `lo_core_sw_qa_odfexport_WordTest_edit.odt` 第 0 頁發射 ≥1 個文字項，
//!    且 JSON 欄位名與 DOCX 線契約**逐欄同名**（基準取自 DOCX 線真實 `dump` 產物，
//!    非本卡手寫——防兩線各烤一套欄位）。
//! 2. 發射文字項座標全落在頁矩形內（頁寬高來自 params，±0.5pt）。
//! 3. 分頁歸屬正確：(a) 目標檔每頁發射段落 index 區間＝`page_break_positions`
//!    給的邊界；(b) 全庫每頁發射項與該頁分頁行一一對應（同序、同行文字、同行
//!    網格 y），故段落集合＝`breaks_from_pages` 的 (first_para, last_para)。
//! 4. 零回退：發射層為**純讀取投影**——不改 `OdfProjection` 任何欄位，既有
//!    ODF-C3 計數基準（語料段落總數 19935）與頁尺寸口徑不動。
//!
//! 發射面覆蓋（解剖報告 §4）：本切片只發 **文字項**。圖片／表格／頁首尾／
//! 註腳／段落框線／分頁框實體項一律不發，記帳 D1b。缺件欄位發中性值：字色
//! 0xFF000000、family=None（serde 跳欄）、flags 只帶粗體位。
//!
//! 契約面鐵條：段落 index **不在**顯示列表欄位裡（DOCX 線也沒有）——分頁歸屬
//! 因此只能用幾何反查驗證（行網格 y ＋行文字逐字對照），不准發射器自報欄。

use quire_layout::{MARGIN_LEFT, MARGIN_TOP};
use quire_odf::display::{
    display_list_json, emit_display_lists, DrawItem, OdtView, PageDisplayList,
};
use quire_odf::layout_glue::{page_break_positions, project, LayoutParams};
use quire_odf::OdfBundle;
use serde_json::Value;
use std::path::{Path, PathBuf};

const TARGET: &str = "ext/lo_core_sw_qa_odfexport_WordTest_edit.odt";
/// ODF-C3 語料段落總數基準（odt_layout_projection 燈 a：text:p＋text:h）。
const CORPUS_PARAGRAPH_BASELINE: usize = 19935;

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-odt")
}

/// 跨線契約基準檔（由 `scripts/odf_d1_contract_probe.py` 從 DOCX 線真實 `dump`
/// 產物抽取；DOCX 側另有一盞鎖在 `shells/native/tests/odf_d1_dump_contract.rs`
/// 反查本檔未漂移——兩側互鎖，任一條線改欄位名都會咬紅）。
fn contract_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/display_list_contract.json")
}

fn contract() -> Value {
    let p = contract_path();
    let s = std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("讀取契約基準檔失敗 {}: {e}", p.display()));
    serde_json::from_str(&s).expect("契約基準檔應為合法 JSON")
}

fn read_file(rel: &str) -> Vec<u8> {
    let path = corpus_root().join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("讀取檔案失敗 {rel}: {e}"))
}

fn manifest_files() -> Vec<String> {
    let p = corpus_root().join("manifest.json");
    let content = std::fs::read_to_string(&p).expect("讀取 manifest.json 失敗");
    let json: Value = serde_json::from_str(&content).expect("解析 manifest.json 失敗");
    json["files"]
        .as_array()
        .expect("files 欄位應為陣列")
        .iter()
        .map(|f| f["file"].as_str().expect("file 欄位應為字串").to_string())
        .collect()
}

/// 語料全檔走查（口徑與 ODF-C3 燈 a／ODF-E1 完全對齊，**禁 catch-continue 假綠**）：
/// - `OdfBundle::open` 失敗＝直接 panic（加密檔是完好封裝，容器層必須成功）。
/// - manifest 宣告 content.xml／styles.xml 加密者＝必須落 `Err(Encrypted)`，
///   記入 `encrypted` 不算入可投影集。
/// - 其餘檔必須 `Ok`（mimetype「非首條目」違例三檔仍可投影，照 C3 口徑入列）。
///
/// 回傳（可投影檔相對路徑, bundle, 投影, 加密檔清單, mimetype 違例數）。
fn corpus_walk() -> (
    Vec<(String, OdfBundle, quire_odf::OdfProjection)>,
    Vec<String>,
    usize,
) {
    let mut out = Vec::new();
    let mut encrypted = Vec::new();
    let mut violations = 0usize;
    for rel in manifest_files() {
        let bytes = read_file(&rel);
        let bundle =
            OdfBundle::open(&bytes).unwrap_or_else(|e| panic!("{rel} 容器層必須開檔成功: {e}"));
        if bundle.mimetype_status().is_not_first() {
            violations += 1;
        }
        if bundle.manifest().is_encrypted("content.xml")
            || bundle.manifest().is_encrypted("styles.xml")
        {
            let err = project(&bundle)
                .err()
                .unwrap_or_else(|| panic!("{rel}：加密條目投影必須回 Err（ODF-E1）"));
            assert!(
                matches!(err, quire_odf::OdfError::Encrypted { .. }),
                "{rel}：加密必須落 OdfError::Encrypted，實際 {err:?}"
            );
            encrypted.push(rel);
            continue;
        }
        let proj = project(&bundle).unwrap_or_else(|e| panic!("{rel} 投影失敗: {e}"));
        out.push((rel, bundle, proj));
    }
    (out, encrypted, violations)
}

/// 可投影語料檔（走查快取；各燈共用同一次開檔，省 IO）。
fn projectable_corpus() -> Vec<(String, OdfBundle, quire_odf::OdfProjection)> {
    corpus_walk().0
}

fn page_items(dl: &Value) -> Vec<Value> {
    dl["items"].as_array().expect("items 欄位應為陣列").clone()
}

/// `Text` 變體項的 JSON 鍵集合（排序去重）。
fn text_item_keys(items: &[Value]) -> Vec<String> {
    let mut keys: Vec<String> = items
        .iter()
        .filter_map(|it| it.get("Text"))
        .flat_map(|t| t.as_object().expect("Text 應為物件").keys().cloned())
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

/// JSON 陣列欄位 → 排序後的字串清單（比對用）。
fn str_list(v: &Value) -> Vec<String> {
    let mut out: Vec<String> = v
        .as_array()
        .expect("欄位應為陣列")
        .iter()
        .map(|s| s.as_str().expect("元素應為字串").to_string())
        .collect();
    out.sort();
    out
}

/// 同頁文字項依 y 分群（一行可含多個同質 run 段）→（行 y, 行首 x, 行文字）。
/// 顯示列表契約不帶段落欄，故本函數是「發射面 ↔ 分頁面」對照的唯一讀取口。
fn group_lines_by_y(dl: &PageDisplayList) -> Vec<(f32, f32, String)> {
    let mut out: Vec<(f32, f32, String)> = Vec::new();
    for it in &dl.items {
        let DrawItem::Text { x, y, text, .. } = it else {
            panic!("D1 切片只准發 Text 變體，實際 {it:?}");
        };
        match out.last_mut() {
            Some((py, _, t)) if (*py - y).abs() <= 1e-3 => t.push_str(text),
            _ => out.push((*y, *x, text.clone())),
        }
    }
    out
}

/// 發射器的分頁面：該檔逐頁（首段, 末段）區間（＝發射器實際消費的分頁器輸出，
/// 經公開出口 `breaks_from_pages` 換算；非發射器自報欄）。
fn page_para_bounds(proj: &quire_odf::OdfProjection, params: &LayoutParams) -> Vec<(usize, usize)> {
    let pages = quire_layout::paginate_per_section(
        &quire_odf::layout_projection(proj, params),
        params,
        &[],
    );
    quire_layout::breaks_from_pages(&pages, params)
        .into_iter()
        .map(|b| (b.first_para, b.last_para))
        .collect()
}

/// `page_break_positions`（每頁起排段落 index，首頁除外）→ 逐頁起排段落清單。
fn page_starts(breaks: &[usize], n_para: usize) -> Vec<usize> {
    let mut starts = vec![0usize];
    for &b in breaks {
        if *starts.last().unwrap() != b {
            starts.push(b);
        }
    }
    assert!(
        starts.last().copied().unwrap_or(0) < n_para.max(1),
        "斷點表尾值 {starts:?} 越過段落數 {n_para}"
    );
    starts
}

// ---------------------------------------------------------------------------
// 燈 1：目標檔第 0 頁發射 ≥1 文字項＋欄位名與 DOCX 線契約逐欄同名
// ---------------------------------------------------------------------------

#[test]
fn lamp1_target_file_emits_text_items_with_docx_field_names() {
    let bytes = read_file(TARGET);
    let bundle = OdfBundle::open(&bytes).expect("開檔失敗");
    let proj = project(&bundle).expect("投影失敗");
    let params = LayoutParams::default();
    let lists = emit_display_lists(&proj, &params);
    assert!(!lists.is_empty(), "目標檔至少要有 1 頁顯示列表");

    let json = display_list_json(&lists, 0).expect("第 0 頁顯示列表 JSON 應存在");
    let dl: Value = serde_json::from_str(&json).expect("輸出必須是合法 JSON（不准字串拼接）");
    let c = contract();

    // 頁層欄位名逐欄同名。
    let odt_top: Vec<String> = {
        let mut k: Vec<String> = dl
            .as_object()
            .expect("頁應為物件")
            .keys()
            .cloned()
            .collect();
        k.sort();
        k
    };
    assert_eq!(
        odt_top,
        str_list(&c["top_level_keys"]),
        "頁層欄位名必須與 DOCX 線契約逐欄同名（基準檔 {}",
        contract_path().display()
    );

    // 文字項存在性。
    let items = page_items(&dl);
    let texts: Vec<&Value> = items.iter().filter(|it| it.get("Text").is_some()).collect();
    assert!(
        !texts.is_empty(),
        "目標檔第 0 頁必須發射 ≥1 個文字項，實際 items={}",
        items.len()
    );

    // 欄位名：必要欄位齊備＋不發契約沒有的欄位（兩側皆逐欄同名）。
    let emitted = text_item_keys(&items);
    let required = str_list(&c["variant_required_keys"]["Text"]);
    let docx_text_keys = str_list(&c["variant_keys"]["Text"]);
    for k in &required {
        assert!(
            emitted.iter().any(|e| e == k),
            "文字項必要欄位 {k} 未發射（ODT 線漏欄位）；實發 {emitted:?}"
        );
    }
    for k in &emitted {
        assert!(
            docx_text_keys.iter().any(|d| d == k),
            "文字項發了 DOCX 線契約沒有的欄位 {k}：ODT 線自烤欄位名，實發 {emitted:?}，DOCX 側 {docx_text_keys:?}"
        );
    }

    // 變體名鍵：本切片只該有 Text。
    let variants: Vec<&str> = items
        .iter()
        .flat_map(|it| {
            it.as_object()
                .expect("項應為物件")
                .keys()
                .map(|k| k.as_str())
        })
        .collect();
    assert!(
        variants.iter().all(|v| *v == "Text"),
        "D1 切片只准發 Text 變體，實際變體集 {variants:?}"
    );
}

// ---------------------------------------------------------------------------
// 燈 2：座標全落在頁矩形內（頁寬高來自 params，±0.5pt）
// ---------------------------------------------------------------------------

#[test]
fn lamp2_text_item_origins_inside_page_rect() {
    let params = LayoutParams::default();
    let mut checked = 0usize;
    for (rel, _, proj) in projectable_corpus() {
        let lists = emit_display_lists(&proj, &params);
        for (page, dl) in lists.iter().enumerate() {
            let raw = display_list_json(&lists, page).expect("頁 JSON 應存在");
            let json: Value = serde_json::from_str(&raw).expect("頁 JSON 合法");
            let w = json["width_pt"].as_f64().expect("width_pt") as f32;
            let h = json["height_pt"].as_f64().expect("height_pt") as f32;
            assert!(
                w > 0.0 && h > 0.0,
                "{rel} p{page}：頁尺寸必須為正，實際 {w}x{h}"
            );
            for item in &dl.items {
                let DrawItem::Text { x, y, text, .. } = item else {
                    panic!("D1 切片只准發 Text 變體，實際 {item:?}");
                };
                assert!(
                    *x >= -0.5 && *x <= w + 0.5 && *y >= -0.5 && *y <= h + 0.5,
                    "{rel} p{page}：文字項原點 ({x}, {y}) 落在頁矩形 [0, {w}]x[0, {h}] ±0.5pt 之外，文字={text:?}"
                );
                // 第二道（params 邊界，防「在頁內但出版心」的假綠）。
                assert!(
                    *x >= params.margins_pt[MARGIN_LEFT] - 0.5,
                    "{rel} p{page}：文字項 x={x} 左於版心左緣 {}",
                    params.margins_pt[MARGIN_LEFT]
                );
                assert!(
                    *y >= params.margins_pt[MARGIN_TOP] - 0.5,
                    "{rel} p{page}：文字項 y={y} 上於上邊界 {}",
                    params.margins_pt[MARGIN_TOP]
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked > 1000,
        "燈 2 實測文字項數過少（{checked}），斷言可能空轉"
    );
}

// ---------------------------------------------------------------------------
// 燈 3：分頁歸屬正確
// ---------------------------------------------------------------------------

#[test]
fn lamp3_page_membership_matches_pagination_boundaries() {
    let params = LayoutParams::default();

    // (a) 目標檔：每頁發射段落 index 區間嚴格等於 `page_break_positions` 邊界。
    //     首段邊界逐頁等值；末段邊界只容「跨頁續排＝與下頁首段同值」一種歧義。
    let bytes = read_file(TARGET);
    let bundle = OdfBundle::open(&bytes).expect("開檔失敗");
    let proj = project(&bundle).expect("投影失敗");
    let (texts, _) = proj.flatten();
    let breaks = page_break_positions(&texts, &params);
    let lists = emit_display_lists(&proj, &params);
    let bounds = page_para_bounds(&proj, &params);
    assert_eq!(lists.len(), bounds.len(), "發射頁數必須等於邊界表頁數");
    let starts: Vec<usize> = bounds.iter().map(|b| b.0).collect();
    assert_eq!(
        starts,
        page_starts(&breaks, texts.len()),
        "目標檔：每頁發射起排段落必須等於 page_break_positions 邊界（breaks={breaks:?}）"
    );
    for (i, (lo, hi)) in bounds.iter().enumerate() {
        assert!(lo <= hi, "目標檔 p{i}：段落區間反轉 {lo}..{hi}");
        let expect = bounds
            .get(i + 1)
            .map(|n| n.0)
            .unwrap_or(texts.len().saturating_sub(1));
        assert!(
            *hi == expect || *hi + 1 == expect,
            "目標檔 p{i}：末段 {hi} 與下頁首段 {expect} 不符（既非跨頁續排亦非段邊界）"
        );
    }
    assert_eq!(lists.len(), breaks.len() + 1, "頁數必須等於斷點數＋1");

    // (b) 全庫：每頁發射項與該頁分頁行一一對應（同序、同行文字、同行網格 y），
    //     且該頁行段落區間＝`breaks_from_pages` 的 (first_para, last_para)。
    //     顯示列表契約不帶段落欄 ⇒ 「發射序列＝分頁行序列」就是段落歸屬的幾何證明。
    let line_h = params.line_h_pt();
    let mut total_lines = 0usize;
    for (rel, _, proj) in projectable_corpus() {
        let pls = quire_odf::layout_projection(&proj, &params);
        let pages = quire_layout::paginate_per_section(&pls, &params, &[]);
        let lists = emit_display_lists(&proj, &params);
        assert_eq!(lists.len(), pages.len(), "{rel}：頁數與分頁器不一致");
        let bounds = quire_layout::breaks_from_pages(&pages, &params);
        assert_eq!(bounds.len(), lists.len(), "{rel}：邊界表頁數不一致");
        for (page, pg) in pages.iter().enumerate() {
            let groups = group_lines_by_y(&lists[page]);
            assert_eq!(
                groups.len(),
                pg.lines.len(),
                "{rel} p{page}：發射行群數必須等於該頁行數（一行一群）"
            );
            for (i, (line, (y, first_x, text))) in pg.lines.iter().zip(groups.iter()).enumerate() {
                assert_eq!(
                    text, &line.text,
                    "{rel} p{page} 行 {i}：發射文字與分頁行不符"
                );
                let want_y = pg.margin_top_pt + pg.line_bands[i] as f32 * line_h;
                assert!(
                    (y - want_y).abs() <= 0.01,
                    "{rel} p{page} 行 {i}：y={y} 偏離行網格 {want_y}"
                );
                assert!(
                    first_x - pg.margin_left_pt >= -0.01,
                    "{rel} p{page} 行 {i}：行首 x={first_x} 越過版心左緣 {}（負縮排未挾）",
                    pg.margin_left_pt
                );
                assert!(
                    first_x + 0.01 <= pg.width_pt,
                    "{rel} p{page} 行 {i}：行首 x={first_x} 越過紙張寬度 {}（防呆：縮排不得把文字推出頁外）",
                    pg.width_pt
                );
            }
            let lo = pg.lines.iter().map(|l| l.para_index).min().unwrap();
            let hi = pg.lines.iter().map(|l| l.para_index).max().unwrap();
            assert_eq!(
                (lo, hi),
                (bounds[page].first_para, bounds[page].last_para),
                "{rel} p{page}：發射段落區間與 breaks_from_pages 邊界不符"
            );
            let paras: Vec<usize> = pg.lines.iter().map(|l| l.para_index).collect();
            for w in paras.windows(2) {
                assert!(
                    w[0] <= w[1],
                    "{rel} p{page}：頁內段落 index 回跳 {} > {}",
                    w[0],
                    w[1]
                );
            }
            total_lines += pg.lines.len();
        }
    }
    assert!(
        total_lines > 1000,
        "燈 3 實測行數過少（{total_lines}），斷言可能空轉"
    );
}

// ---------------------------------------------------------------------------
// 燈 4：零回退——發射層純讀取，不動投影計數基準
// ---------------------------------------------------------------------------

#[test]
fn lamp4_emitter_is_pure_read_and_keeps_projection_totals() {
    let params = LayoutParams::default();
    let (files, encrypted, violations) = corpus_walk();
    // 語料歸類帳（與 C3 燈 a／E1 同口徑）：50 檔＝48 可投影＋2 加密；
    // mimetype「非首條目」違例 3 檔仍可投影（C3 事實），故不在加密集內。
    assert_eq!(
        files.len(),
        48,
        "可投影語料檔數必須為 48（50 檔減 2 加密檔）"
    );
    assert_eq!(encrypted.len(), 2, "加密語料檔數必須為 2（ODF-E1 帳）");
    assert_eq!(violations, 3, "mimetype 違例檔必須仍為 3（C3 帳）");

    let mut total_paragraphs = 0usize;
    for (rel, bundle, proj) in &files {
        // 純讀取證明：再開一次、再投影一次，逐欄位等值（發射層不得改動投影）。
        let snapshot = (
            proj.paragraph_count(),
            proj.total_text_len,
            proj.text_p_count,
            proj.text_h_count,
            proj.sizes_pt(),
        );
        let lists = emit_display_lists(proj, &params);
        let after = project(bundle).unwrap_or_else(|e| panic!("{rel} 再投影失敗: {e}"));
        let now = (
            after.paragraph_count(),
            after.total_text_len,
            after.text_p_count,
            after.text_h_count,
            after.sizes_pt(),
        );
        assert_eq!(snapshot, now, "{rel}：發射層改動了投影結果（必須純讀取）");
        total_paragraphs += after.paragraph_count();
        // 頁碼連續（0..n）且頁尺寸取自 params（分頁框 width_pt/height_pt 來源）。
        for (i, dl) in lists.iter().enumerate() {
            assert_eq!(dl.page, i, "{rel}：頁碼必須連續");
            assert_eq!(dl.width_pt, params.page_w_pt, "{rel}：頁寬須來自 params");
            assert_eq!(dl.height_pt, params.page_h_pt, "{rel}：頁高須來自 params");
        }
    }
    assert_eq!(
        total_paragraphs, CORPUS_PARAGRAPH_BASELINE,
        "語料段落總數必須維持 ODF-C3 基準 {CORPUS_PARAGRAPH_BASELINE}（發射層不得改動計數口徑）"
    );
}

// ---------------------------------------------------------------------------
// 附加鎖：OdtView 快取面＝直發面；JSON 可 serde round-trip 回型別（derive 路線）
// ---------------------------------------------------------------------------

#[test]
fn view_and_direct_emitter_agree_and_json_round_trips() {
    let params = LayoutParams::default();
    let bytes = read_file(TARGET);
    let view = OdtView::from_odt_bytes(&bytes, &params).expect("OdtView 開檔失敗");
    assert!(view.page_count() >= 1);
    assert_eq!(
        view.paragraph_count(),
        13,
        "目標檔段落數基準 13（ODF-C3 帳）"
    );
    assert_eq!(
        view.total_text_len(),
        402,
        "目標檔文字總碼點基準 402（odt-render 帳）"
    );
    let bundle = OdfBundle::open(&bytes).expect("開檔失敗");
    let proj = project(&bundle).expect("投影失敗");
    let direct = emit_display_lists(&proj, &params);
    assert_eq!(
        direct.len(),
        view.page_count(),
        "OdtView 與直發面頁數必須一致"
    );
    for i in 0..view.page_count() {
        let a = view.page_display_list_json(i).expect("OdtView 頁 JSON");
        let b = display_list_json(&direct, i).expect("直發面頁 JSON");
        assert_eq!(a, b, "OdtView 快取面與直發面第 {i} 頁必須逐字相同");
        // serde round-trip：輸出 JSON 必須能還原成同一個 PageDisplayList 型別
        // （證明欄位契約由 serde derive 決定，非字串拼接）。
        let back: PageDisplayList =
            serde_json::from_str(&a).expect("頁 JSON 必須可還原為 PageDisplayList");
        assert_eq!(&back, &direct[i], "第 {i} 頁 round-trip 必須等值");
    }
    assert!(
        display_list_json(&direct, 999).is_none(),
        "不存在頁碼必須回 None"
    );
    assert!(
        view.page_display_list_json(999).is_none(),
        "OdtView 不存在頁碼必須回 None"
    );
    // 加密檔必須走明確變體老實報錯（ODF-E1 口徑不得被新入口繞過）。
    let enc = read_file("ext/lo_core_sw_qa_odfexport_sha1_correct.odt");
    let err = OdtView::from_odt_bytes(&enc, &params).expect_err("加密檔必須報錯");
    assert!(
        matches!(err, quire_odf::OdfError::Encrypted { .. }),
        "加密檔必須落 OdfError::Encrypted，實際 {err:?}"
    );
}
