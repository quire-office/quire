//! E 卡（t_f0cbc25d）紅燈：`w:tblPr` 直接底紋的發射極性鎖。
//!
//! ## 卡面前提的實測更正（極性反轉，依「卡面前提不可信」紀律）
//!
//! 卡面契約 2 要求「tblPr 直接底紋發 Rect，範圍＝整表外框」。LO 25.8 探針
//! 實測（`scripts/e_probe_lo_tblpr_shd_v1..v5.py`，全記錄見
//! `docs/e-tblpr-shd-redlight.md`）推翻此前提：**LO 不渲染表格層直接底紋**
//! ——七變體（val=clear／solid、schema 正確順序／bug57031 實際順序、尾部、
//! autofit、無 tblBorders、無 tblW、cellSpacing 0/5pt）高飽和色 `9900CC`
//! 全部零渲染（`get_drawings` fill 筆數＝0 ＋ 72dpi 像素掃描零命中），而同檔
//! 同色 cell 層 `tcPr/w:shd` 正向對照照常渲染（1363 像素精確 9900CC）。
//! 樣式路徑（styles.xml 表格樣式 tblPr/w:shd）亦零渲染（v5 Q1/Q2）。
//! 我方現行「零發射」＝與 LO 真值一致，**不是缺陷**。
//!
//! 因此本考卷極性反轉：發射層**必須保持零發射**（發了反而偏離 LO），並用
//! 正向對照燈證明零發射不是「底紋機制整個壞掉」的假綠。
//!
//! ## 四盞燈
//!
//! 1. **單邊失效鎖（卡面證據的考卷化）**：`SetTableProps.shading_fill` 對
//!    `fixtures/table_mixed.docx` 真寫入 XML（`w:shd` 進 `tblPr` 可讀回），
//!    但顯示列表逐項簽名逐位不動（零發射）——「XML 寫入成功、畫面看不見」
//!    如實釘死；存檔重開該 `w:shd` 仍在（無損）。
//! 2. **cell 層正向對照**：同表 `CellPropsPatch.shading_fill` 立即多恰一筆
//!    填色 Rect——證明零發射不是底紋渲染鏈全滅（排除假綠）。
//! 3. **真檔奇偶校驗（bug57031）**：該檔 `tblPr` 直接底紋 8 表（FAFBFE）＋
//!    cell 層 `tcPr/w:shd` 12 格（同色）。顯示列表 FAFBFE Rect 總數必須
//!    ＝XML 掃描出的 cell 層有效數（12）、**不得**含表層貢獻（發射態為 20）
//!    ——零發射由 XML 宣告計數在考卷內獨立推導，非寫死常數。
//! 4. **表層投影零消費鎖**：`TableProps.shading`（ooxml 已投影）對顯示列表
//!    零貢獻＋roundtrip byte-identical——防「順手修發射」誤觸 LO 真值。
//!
//! ## 鑑別力（BUGGY 回塞）
//!
//! `E_TBLPR_SHD_BUGGY=1`：燈 1/3 的期望換成「表層發射假設值」（Rect 簽名
//! 變動視為正常、FAFBFE 期望 12→20）——兩燈必紅，證明斷言真在比對發射
//! 數量而非恆真。測試進程內不設 env（避免平行污染），由外部執行
//! `E_TBLPR_SHD_BUGGY=1 cargo test -p quire-engine --test e_tblpr_shd_parity -- --exact` 驗證
//! （逐燈 `--exact` 名單見 docs）。

use quire_engine::{
    CellPropsPatch, DrawItem, Engine, Mutation, ReversibilityLevel, TablePath, TablePropsPatch,
};

/// 探針封頂色：LO 實測不渲染的表層底紋用色（v4 P1–P5）與 cell 正向對照色。
const PROBE_FILL: &str = "E06699";
/// bug57031 的表層／格層同色底紋（XML 實測 `FAFBFE`）。
const BUGGY57031_FILL: u32 = 0xFFFA_FBFE;

fn load(rel: &str) -> Vec<u8> {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../").to_string() + rel;
    std::fs::read(&full).unwrap_or_else(|e| panic!("讀取 {full} 失敗: {e}"))
}

fn open(rel: &str) -> Engine {
    Engine::from_docx_bytes(&load(rel)).unwrap_or_else(|e| panic!("開啟 {rel} 失敗: {e}"))
}

fn settle(engine: &mut Engine) {
    while !engine.is_settled() {
        engine.reflow_step_json();
    }
}

/// 全部頁顯示列表的逐項簽名（Debug 逐字元形——幾何＋顏色＋文字全欄位）。
fn signatures(engine: &Engine) -> Vec<String> {
    (0..engine.page_count())
        .flat_map(|p| {
            engine
                .page_display_list(p)
                .map(|dl| {
                    dl.items
                        .iter()
                        .map(|it| format!("{it:?}"))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .collect()
}

/// 全部頁指定填色的 Rect 數。
fn rect_count_with_fill(engine: &Engine, argb: u32) -> usize {
    (0..engine.page_count())
        .map(|p| {
            engine
                .page_display_list(p)
                .map(|dl| {
                    dl.items
                        .iter()
                        .filter(|it| {
                            matches!(it, DrawItem::Rect { fill_argb, .. } if *fill_argb == argb)
                        })
                        .count()
                })
                .unwrap_or(0)
        })
        .sum()
}

fn hex6_to_argb(hex: &str) -> u32 {
    0xFF00_0000 | u32::from_str_radix(hex, 16).expect("合法 RRGGBB")
}

/// 掃描 document.xml：`w:tcPr` 區塊內帶 6 位 hex fill 的 `w:shd` 數（cell 層
/// 有效底紋宣告，考卷內獨立推導——燈 3 的期望值來源，非寫死常數）。
fn cell_layer_shd_count(xml: &str) -> usize {
    let mut n = 0;
    let mut at = 0usize;
    while let Some(rel) = xml[at..].find("<w:tcPr") {
        let s = at + rel;
        // 開標籤本體（`<w:tcPr ...>`／自閉 `<w:tcPr/>`）。
        let Some(gt) = xml[s..].find('>') else {
            break;
        };
        let open_tag = &xml[s..=s + gt];
        if open_tag.ends_with("/>") {
            at = s + gt + 1; // 自閉合＝無內容，續找下一塊
            continue;
        }
        let Some(rel_e) = xml[s..].find("</w:tcPr>") else {
            break;
        };
        let block = &xml[s..s + rel_e];
        let mut b = 0usize;
        while let Some(rel2) = block[b..].find("<w:shd") {
            let t = b + rel2;
            let end = block[t..]
                .find('>')
                .map(|k| t + k + 1)
                .unwrap_or(block.len());
            let tag = &block[t..end];
            // fill="RRGGBB"（auto／空值＝明確無底色，不算有效宣告）。
            if let Some(fi) = tag.find("w:fill=\"") {
                let v = &tag[fi + 8..];
                if let Some(ve) = v.find('"') {
                    let f = &v[..ve];
                    if f.len() == 6 && f.bytes().all(|c| c.is_ascii_hexdigit()) {
                        n += 1;
                    }
                }
            }
            b = end;
        }
        at = s + rel_e + "</w:tcPr>".len();
    }
    n
}

/// 掃描 document.xml：`w:tblPr` 區塊內帶 6 位 hex fill 的 `w:shd`／`w:tblShd` 數
/// （表層底紋宣告；斷言「我方發射總數 **不含**這一層」的差額證據）。
fn tblpr_layer_shd_count(xml: &str) -> usize {
    let mut n = 0;
    let mut at = 0usize;
    while let Some(rel) = xml[at..].find("<w:tblPr") {
        let s = at + rel;
        let Some(rel_e) = xml[s..].find("</w:tblPr>") else {
            break;
        };
        let block = &xml[s..s + rel_e];
        // tblPr 內嵌套的 tcPr 不可能出現（schema 平鋪），整區塊內計數即可；
        // 但 tblStylePr 條件帶屬另一層（k8_shd 已鎖），此處tblPr 區塊不涵蓋。
        if block.contains("<w:shd") || block.contains("<w:tblShd") {
            n += 1;
        }
        at = s + rel_e + "</w:tblPr>".len();
    }
    n
}

/// 燈 1：單邊失效鎖——表層底紋 XML 寫入成功、顯示列表逐位不動、存檔保留。
#[test]
fn e_tblpr_lamp1_xml_written_display_list_frozen() {
    let mut e = open("fixtures/table_mixed.docx");
    settle(&mut e);
    let before = signatures(&e);

    let out = e
        .apply_mutation(Mutation::SetTableProps {
            table: TablePath::body(0),
            patch: TablePropsPatch {
                width_pt: None,
                alignment: None,
                shading_fill: Some(PROBE_FILL.to_string()),
                // 合併漂移補正：main 的 E 邊框卡新增 `borders` 欄位（本卡分支基線較舊）。
                // None＝零擾動，不影響本卡極性鎖語義。
                borders: None,
            },
        })
        .expect("SetTableProps 應被受理");
    assert!(
        matches!(out.reversibility, ReversibilityLevel::StrictByteReversible),
        "表層底紋應位元組級可逆（undo 堆）"
    );
    settle(&mut e);

    // ① XML 面：w:shd 真的寫進 tblPr（part 明文可讀回）。
    let xml = String::from_utf8(
        e.part("word/document.xml")
            .expect("document.xml 存在")
            .to_vec(),
    )
    .expect("XML 應為 UTF-8");
    let shd_tag = format!(r#"<w:shd w:val="clear" w:color="auto" w:fill="{PROBE_FILL}"/>"#);
    assert!(
        xml.contains(&shd_tag),
        "SetTableProps.shading_fill 應把 {shd_tag} 寫入 document.xml"
    );
    // 且落在 tblPr 區塊內（非誤掛別處）。
    let in_tblpr = xml.match_indices("</w:tblPr>").any(|(end, _)| {
        xml[..end]
            .rsplit("<w:tblPr")
            .next()
            .is_some_and(|b| b.contains(&shd_tag))
    });
    assert!(in_tblpr, "{shd_tag} 應位於 w:tblPr 區塊內");

    // ② 發射面：顯示列表逐項簽名逐位不動（零發射）。BUGGY 態反向：預期有變動。
    let buggy = std::env::var_os("E_TBLPR_SHD_BUGGY").is_some();
    let after = signatures(&e);
    if buggy {
        assert_ne!(
            before, after,
            "BUGGY 態：燈 1 期望表層底紋進入顯示列表——未變即鑑別失效"
        );
    } else {
        assert_eq!(
            before, after,
            "LO 25.8 實測不渲染 tblPr 直接底紋（探針 v1–v5），發射層必須零發射；\
             顯示列表出現新圖元＝偏離 LO 真值（改此斷言前先重跑 LO 探針）"
        );
        // ③ 填色專項：探針色在顯示列表零出現。
        assert_eq!(
            rect_count_with_fill(&e, hex6_to_argb(PROBE_FILL)),
            0,
            "表層 {PROBE_FILL} 不得發 Rect（LO 真值零渲染）"
        );
    }

    // ④ 無損：存檔重開後 w:shd 仍在、顯示列表仍零發射。
    let saved = e.save_docx().expect("存檔");
    let e2 = Engine::from_docx_bytes(&saved).expect("重開");
    let xml2 =
        String::from_utf8(e2.part("word/document.xml").expect("part").to_vec()).expect("utf8");
    assert!(
        xml2.contains(&shd_tag),
        "存檔重開後 tblPr 底紋宣告必須保留（無損紅線）"
    );
    assert_eq!(
        rect_count_with_fill(&e2, hex6_to_argb(PROBE_FILL)),
        0,
        "重開後仍零發射"
    );
}

/// 燈 2：cell 層正向對照——同表儲存格底紋立即發恰一筆 Rect。
/// 本燈無 BUGGY 態：它證明燈 1 的「零發射」不是底紋渲染鏈全滅的假綠。
#[test]
fn e_tblpr_lamp2_cell_shading_still_emits() {
    let mut e = open("fixtures/table_mixed.docx");
    settle(&mut e);
    let before = rect_count_with_fill(&e, hex6_to_argb(PROBE_FILL));
    assert_eq!(before, 0, "基線：表內不得已有探針色");

    e.apply_mutation(Mutation::SetCellProps {
        table: TablePath::body(0),
        row: 0,
        cell: 0,
        patch: CellPropsPatch {
            shading_fill: Some(PROBE_FILL.to_string()),
            ..Default::default()
        },
    })
    .expect("SetCellProps 應被受理");
    settle(&mut e);

    let after = rect_count_with_fill(&e, hex6_to_argb(PROBE_FILL));
    assert_eq!(
        after - before,
        1,
        "cell 層底紋必須多恰一筆 Rect（卡面證據機制：cell 層有發射）"
    );
}

/// 燈 3：真檔奇偶校驗（bug57031）——表層 8 表 FAFBFE 零發射、
/// 顯示列表總數＝XML cell 層有效宣告數（表層不得混入）。
#[test]
fn e_tblpr_lamp3_bug57031_table_layer_excluded_from_emission() {
    let bytes = load("corpus/poi/test-data_document_bug57031.docx");
    let e = open_at(&bytes);
    let xml = String::from_utf8(e.part("word/document.xml").expect("part").to_vec()).expect("utf8");

    let cell_layer = cell_layer_shd_count(&xml);
    let tbl_layer = tblpr_layer_shd_count(&xml);
    assert!(tbl_layer > 0, "bug57031 應有 tblPr 直接底紋（選檔前提）");

    let emitted = rect_count_with_fill(&e, BUGGY57031_FILL);
    let buggy = std::env::var_os("E_TBLPR_SHD_BUGGY").is_some();
    if buggy {
        // 卡面假設態：表層也發 → 期望＝cell＋tbl 兩層總和。
        assert_eq!(
            emitted,
            cell_layer + tbl_layer,
            "BUGGY 態：期望表層發射（cell {cell_layer}＋tbl {tbl_layer}）——實得 {emitted} 即燈紅"
        );
    } else {
        assert_eq!(
            emitted, cell_layer,
            "bug57031 顯示列表 {BUGGY57031_FILL:#x} Rect 應＝cell 層有效宣告 {cell_layer}\
             （XML 掃描）；表層 {tbl_layer} 表零貢獻——多出即發射層偷跑、變少即 cell 層回歸"
        );
    }
}

fn open_at(bytes: &[u8]) -> Engine {
    Engine::from_docx_bytes(bytes).expect("開啟失敗")
}

/// 燈 4：表層投影零消費＋roundtrip——ooxml `TableProps.shading` 有投影
/// （XML 無損）、發射層零消費、開檔存檔 byte-identical。
#[test]
fn e_tblpr_lamp4_projection_present_emission_zero_roundtrip_clean() {
    let bytes = load("corpus/poi/test-data_document_bug57031.docx");
    // ooxml 投影面：parse 成功且 part 明文含 tblPr 底紋（讀取投影無損）。
    let doc = quire_ooxml::parse(&bytes).expect("ooxml parse");
    let xml = String::from_utf8(doc.parts["word/document.xml"].clone()).expect("utf8");
    assert!(
        xml.contains("<w:shd") && tblpr_layer_shd_count(&xml) > 0,
        "tblPr 直接底紋必須在 part 明文（唯讀來源隔離：不吞不丟）"
    );

    // roundtrip 紅線：不動存檔 byte-identical（roundtrip --check 同口徑）。
    let e = open_at(&bytes);
    let saved = e.save_docx().expect("存檔");
    assert_eq!(
        saved.as_slice(),
        bytes.as_slice(),
        "bug57031 開檔→存檔必須 byte-identical（零觸碰）"
    );

    // 發射面零消費：探針色（表層色）Rect＝cell 層數（燈 3 的獨立覆核）。
    let cell_layer = cell_layer_shd_count(&xml);
    assert_eq!(
        rect_count_with_fill(&e, BUGGY57031_FILL),
        cell_layer,
        "發射層對 tblPr 直接底紋零消費（＝LO 真值）"
    );
}
