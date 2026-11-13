//! E 卡紅燈考卷：表格邊框——Mutation 寫入欄位＋顯示列表 Rule 發射。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 卡面缺口（U4 t_e1b68330 收案點名、PM 實查坐實）
//! `TablePropsPatch`／`CellPropsPatch` 沒有任何 border 欄位，serde 對未知欄位
//! 靜默吞掉回 ok=true——外殼送邊框欄會得到「成功但零效果」。本考卷先紅拍這個
//! 形態（燈 1／燈 2 在寫入層落地前必紅），修後轉綠。
//!
//! # 斷言層口徑
//! - 寫入面：存檔件 `word/document.xml` 明文逐字斷言（`w:tblBorders`／
//!   `w:tcBorders` 與 `@w:val/@w:sz/@w:space/@w:color`）＋重開件讀回。
//! - 發射面：`page_display_list` 的 `DrawItem::Rule`（寬/色/虛線/朝向＋座標）。
//! - 無損紅線：未觸碰 entry 必 byte-identical（`roundtrip_check`），空補丁
//!   （`borders: None`）整檔 byte_identical。
//!
//! # LO 真值口徑（Linux LO 25.8 探針 2026-11-13，docs/e-tblborder-redlight.md §2）
//! - 邊框 sz→pt 換算：`width_pt = sz/8`（LO 25.8 實測 sz=16→2.0pt、sz=12→1.5pt、
//!   sz=8→1.0pt、sz=24→3.0pt，逐條吻合）；下限 0.25pt 為引擎既有口徑。
//! - `val=none／nil／hidden`：LO 不畫，我方 `solve_pen` 回 None 不發射（燈 3 釘）。
//! - inside 方位只畫內部邊界：LO 只在列／欄交界處畫 insideH／insideV（燈 3 釘）。
//! - 虛線：LO dashes `[8 2.5]`（dashed）；我方 `dash="3 2"`（SVG stroke-dasharray
//!   口徑，非同一字串但同為虛線語義）——本卡只鎖「虛線與實線可區分」，
//!   不鎖 dash 字串逐字（既有 d6 考卷同款取捨，記 docs）。
//! - LO 表格起點 y 與我方差（LO 25.8 探針實測，本卡不修既有行高度量差）：
//!   表 0 頂邊 1.6pt（LO 87.20／我方 88.80）、表 0 insideH 2.45pt（LO 103.15／
//!   我方 105.60）、表 1 頂邊 5.4pt（LO 133.80／我方 139.20，跨表累積）——皆屬
//!   無宣告列高時各自字體度量推得的既有行高差，非邊框缺陷；燈 3 對 insideH
//!   採 ±2.5pt 容差，容差值與逐條實測差記 docs §2，不虛構「完全吻合」。
//!
//! # 鑑別力證明
//! `E_TBLBORDER_BUGGY=1` 把燈 1／燈 2 的期望常數換成錯誤值 ⇒ 該燈必紅，
//! 證明斷言真在比對寫入結果而非恆真（實跑兩態記 docs §4）。

use quire_engine::{
    BorderSpec, CellPropsPatch, DrawItem, Engine, Mutation, ReversibilityLevel, TableBordersPatch,
    TablePath, TablePropsPatch,
};

const PROBE: &str = "fixtures/e_tblborder_probe.docx";

fn settle(engine: &mut Engine) {
    while !engine.is_settled() {
        engine.reflow_step_json();
    }
}

fn open(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀取 {full} 失敗: {e}"));
    Engine::from_docx_bytes(&bytes).unwrap_or_else(|e| panic!("開啟 {rel} 失敗: {e}"))
}

fn probe_engine() -> Engine {
    let mut e = open(PROBE);
    settle(&mut e);
    e
}

fn doc_xml(engine: &Engine) -> String {
    String::from_utf8(
        engine
            .part("word/document.xml")
            .expect("必須有 word/document.xml")
            .to_vec(),
    )
    .expect("document.xml 非 UTF-8")
}

/// 全部 Rule（含虛線）逐條摘要：(朝向 h/v, 起點 x, 起點 y, 終點 x, 終點 y, 寬, 色, 虛線?)。
#[allow(clippy::type_complexity)]
fn rules(engine: &Engine, page: usize) -> Vec<(bool, f32, f32, f32, f32, f32, u32, bool)> {
    let dl = engine
        .page_display_list(page)
        .expect("第 0 頁顯示列表必須存在");
    let mut out = Vec::new();
    for it in &dl.items {
        if let DrawItem::Rule {
            x1,
            y1,
            x2,
            y2,
            width_pt,
            color_argb,
            dash,
        } = it
        {
            out.push((
                (y1 - y2).abs() < 0.01,
                *x1,
                *y1,
                *x2,
                *y2,
                *width_pt,
                *color_argb,
                dash.is_some(),
            ));
        }
    }
    out.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.5.total_cmp(&b.5))
            .then(a.1.total_cmp(&b.1))
            .then(a.2.total_cmp(&b.2))
    });
    out
}

/// 燈 1：表格級邊框寫入閉環——六方位補丁進 `w:tblBorders`（sz=pt×8、space 取整、
/// color 原字串），存檔件明文可撿、重開件發射層出現對應 Rule。
/// 現行（無寫入層）必紅：serde 吞 `borders` 欄 ⇒ 存檔件查無 `w:tblBorders` 新值。
#[test]
fn e_lamp1_table_borders_write_to_xml_and_emit_rules() {
    let mut e = probe_engine();
    // 前提：表 1（root_slot=1）原本全 none ⇒ 該表區域除單格覆寫外零實線。
    let before = rules(&e, 0);
    let before_table1 = before
        .iter()
        .filter(|r| r.2 > 130.0) // 表 1 在本頁 y>130（表 0 於 y<130）
        .count();
    assert_eq!(
        before_table1, 1,
        "表 1 前提：僅一格 tcBorders 覆寫（綠 3pt top）"
    );

    e.apply_mutation(Mutation::SetTableProps {
        table: TablePath::body(1),
        patch: TablePropsPatch {
            borders: Some(TableBordersPatch {
                top: Some(BorderSpec {
                    style: Some("single".into()),
                    width_pt: Some(3.0),
                    space_pt: Some(1.0),
                    color: Some("FF0000".into()),
                }),
                left: Some(BorderSpec {
                    style: Some("single".into()),
                    width_pt: Some(1.0),
                    space_pt: Some(0.0),
                    color: Some("auto".into()),
                }),
                bottom: Some(BorderSpec {
                    style: Some("single".into()),
                    width_pt: Some(2.5),
                    space_pt: Some(0.0),
                    color: Some("00B050".into()),
                }),
                right: Some(BorderSpec {
                    style: Some("none".into()),
                    width_pt: Some(0.0),
                    space_pt: Some(0.0),
                    color: Some("auto".into()),
                }),
                inside_h: Some(BorderSpec {
                    style: Some("single".into()),
                    width_pt: Some(0.5),
                    space_pt: Some(0.0),
                    color: Some("auto".into()),
                }),
                inside_v: Some(BorderSpec {
                    style: Some("dashed".into()),
                    width_pt: Some(1.0),
                    space_pt: Some(0.0),
                    color: Some("auto".into()),
                }),
            }),
            ..Default::default()
        },
    })
    .expect("M4.6 SetTableProps（邊框）失敗");
    settle(&mut e);

    // (a) 存檔件明文：六方位逐字斷言（sz = pt×8；space 取整；color 透傳）。
    let xml = doc_xml(&e);
    let buggy = std::env::var_os("E_TBLBORDER_BUGGY").is_some();
    let want_top = if buggy {
        r#"<w:top w:val="single" w:sz="8" w:space="1" w:color="FF0000"/>"#.to_string()
    } else {
        r#"<w:top w:val="single" w:sz="24" w:space="1" w:color="FF0000"/>"#.to_string()
    };
    assert!(
        xml.contains(&want_top),
        "表 1 tblBorders/@top 必須寫成 {want_top}；實際 xml={}",
        xml_snippet(&xml, "w:tblBorders")
    );
    assert!(
        xml.contains(r#"<w:left w:val="single" w:sz="8" w:space="0" w:color="auto"/>"#),
        "tblBorders/@left 缺位或錯值；實際 xml={}",
        xml_snippet(&xml, "w:tblBorders")
    );
    assert!(
        xml.contains(r#"<w:bottom w:val="single" w:sz="20" w:space="0" w:color="00B050"/>"#),
        "tblBorders/@bottom 缺位或錯值；實際 xml={}",
        xml_snippet(&xml, "w:tblBorders")
    );
    assert!(
        xml.contains(r#"<w:right w:val="none" w:sz="0" w:space="0" w:color="auto"/>"#),
        "tblBorders/@right（none）缺位；實際 xml={}",
        xml_snippet(&xml, "w:tblBorders")
    );
    assert!(
        xml.contains(r#"<w:insideH w:val="single" w:sz="4" w:space="0" w:color="auto"/>"#),
        "tblBorders/@insideH 缺位或錯值；實際 xml={}",
        xml_snippet(&xml, "w:tblBorders")
    );
    assert!(
        xml.contains(r#"<w:insideV w:val="dashed" w:sz="8" w:space="0" w:color="auto"/>"#),
        "tblBorders/@insideV 缺位或錯值；實際 xml={}",
        xml_snippet(&xml, "w:tblBorders")
    );
    // (a′) 表 1 容器（第二顆）逐方位嚴格斷言：新值全在、原 six-none 被逐位
    // 覆寫（none 零殘留）、未觸碰方位零扰动——防「表 0 同名字串假綠」。
    {
        let first = xml
            .find("<w:tblBorders>")
            .expect("第一顆 tblBorders 必須存在");
        let second = xml[first + 1..]
            .find("<w:tblBorders>")
            .map(|i| i + first + 1)
            .expect("第二顆 tblBorders 必須存在");
        let second_seg = xml
            [second..second + xml[second..].find("</w:tblBorders>").expect("閉合必須存在")]
            .to_string();
        for want in [
            r#"<w:top w:val="single" w:sz="24" w:space="1" w:color="FF0000"/>"#,
            r#"<w:left w:val="single" w:sz="8" w:space="0" w:color="auto"/>"#,
            r#"<w:bottom w:val="single" w:sz="20" w:space="0" w:color="00B050"/>"#,
            r#"<w:right w:val="none" w:sz="0" w:space="0" w:color="auto"/>"#,
            r#"<w:insideH w:val="single" w:sz="4" w:space="0" w:color="auto"/>"#,
            r#"<w:insideV w:val="dashed" w:sz="8" w:space="0" w:color="auto"/>"#,
        ] {
            assert!(
                second_seg.contains(want),
                "表 1 容器缺 {want}；實際 seg={second_seg}"
            );
        }
        assert!(
            second_seg.matches(r#"w:val="none""#).count() == 1,
            "表 1 容器原 six-none 必須全數被覆寫（僅 right 一條補丁 none 殘留）；seg={second_seg}"
        );
    }

    // (b) 重開件：發射層出現對應 Rule（寬＝sz/8、色、虛線可區分）。
    let saved = e.save_docx().expect("存檔失敗");
    let mut r = Engine::from_docx_bytes(&saved).expect("重開失敗");
    settle(&mut r);
    let after = rules(&r, 0);
    let t1 = after
        .iter()
        .filter(|it| it.2 > 130.0 - 2.5)
        .cloned()
        .collect::<Vec<_>>();
    let red3 = t1
        .iter()
        .filter(|it| it.0 && (it.5 - 3.0).abs() < 1e-3 && it.6 == 0xFFFF_0000)
        .count();
    assert_eq!(
        red3, 1,
        "表 1 頂邊 red 3pt 實線恰 1 條（cell(0,0) 走 tcBorders 綠）：{t1:?}"
    );
    let green25 = t1
        .iter()
        .filter(|it| it.0 && (it.5 - 2.5).abs() < 1e-3 && it.6 == 0xFF00_B050)
        .count();
    assert_eq!(
        green25, 1,
        "表 1 底邊 green 2.5pt 恰 1 條（cell(0,0) tcBorders=none 蓋掉）：{t1:?}"
    );
    let dashed_v = t1
        .iter()
        .filter(|it| !it.0 && it.7 && (it.5 - 1.0).abs() < 1e-3)
        .count();
    assert_eq!(
        dashed_v, 1,
        "insideV 虛線只在欄交界 x=180 一條（cell(0,0) 右邊被 tcBorders right=hidden 壓掉、\
         不與左格共用邊重疊）：{t1:?}"
    );
    let black1 = t1
        .iter()
        .filter(|it| !it.0 && (it.5 - 1.0).abs() < 1e-3 && !it.7 && it.6 == 0xFF00_0000)
        .count();
    assert_eq!(
        black1, 0,
        "tcBorders left=nil 必須贏過表格級 left 宣告（零實線）：{t1:?}"
    );
}

/// 燈 2：儲存格級邊框寫入閉環——`w:tcBorders` 四方位；insideH／insideV 在
/// tcBorders 非法（ECMA-376 §17.4.10），傳了＝未宣告、零擾動。
#[test]
fn e_lamp2_cell_borders_write_to_xml_and_emit_rules() {
    let mut e = probe_engine();
    e.apply_mutation(Mutation::SetCellProps {
        table: TablePath::body(0),
        row: 1,
        cell: 1,
        patch: CellPropsPatch {
            borders: Some(TableBordersPatch {
                top: Some(BorderSpec {
                    style: Some("thick".into()),
                    width_pt: Some(4.0),
                    space_pt: Some(0.0),
                    color: Some("0000FF".into()),
                }),
                left: Some(BorderSpec {
                    style: Some("dotted".into()),
                    width_pt: Some(1.5),
                    space_pt: Some(2.0),
                    color: Some("auto".into()),
                }),
                bottom: Some(BorderSpec {
                    style: Some("nil".into()),
                    width_pt: None,
                    space_pt: None,
                    color: None,
                }),
                right: None,
                // 非法方位：儲存格級必須無視（零擾動）。
                inside_h: Some(BorderSpec {
                    style: Some("single".into()),
                    width_pt: Some(9.0),
                    space_pt: None,
                    color: None,
                }),
                inside_v: Some(BorderSpec {
                    style: Some("single".into()),
                    width_pt: Some(9.0),
                    space_pt: None,
                    color: None,
                }),
            }),
            ..Default::default()
        },
    })
    .expect("M4.6 SetCellProps（邊框）失敗");
    settle(&mut e);

    let xml = doc_xml(&e);
    let buggy = std::env::var_os("E_TBLBORDER_BUGGY").is_some();
    let want = if buggy {
        r#"<w:top w:val="thick" w:sz="16" w:space="0" w:color="0000FF"/>"#.to_string()
    } else {
        r#"<w:top w:val="thick" w:sz="32" w:space="0" w:color="0000FF"/>"#.to_string()
    };
    assert!(
        xml.contains(&want),
        "cell(1,1) tcBorders/@top 必須寫成 {want}；實際 xml={}",
        xml_snippet(&xml, "w:tcBorders")
    );
    assert!(
        xml.contains(r#"<w:left w:val="dotted" w:sz="12" w:space="2" w:color="auto"/>"#),
        "tcBorders/@left 缺位或錯值；實際 xml={}",
        xml_snippet(&xml, "w:tcBorders")
    );
    assert!(
        xml.contains(r#"<w:bottom w:val="nil"/>"#),
        "tcBorders/@bottom（nil，欄位全缺＝空元素）缺位；實際 xml={}",
        xml_snippet(&xml, "w:tcBorders")
    );
    assert!(
        !xml.contains(r#"w:sz="72""#),
        "儲存格級非法方位 insideH／insideV 不得被寫入（sz=72 滲漏）"
    );
    assert_eq!(
        xml.matches("<w:tcBorders>").count(),
        2,
        "表 0 原格 1 顆＋本格新 1 顆＝2 顆 tcBorders；實際 {}",
        xml.matches("<w:tcBorders>").count()
    );

    // 發射面：該格頂邊必須出現 blue 4pt 粗實線（thick→dash=None）。
    let after = rules(&e, 0);
    let thick = after
        .iter()
        .filter(|it| it.0 && (it.5 - 4.0).abs() < 1e-3 && it.6 == 0xFF00_00FF)
        .count();
    assert_eq!(thick, 1, "cell(1,1) 頂邊 blue 4pt 實線恰 1 條：{after:?}");
    let dotted = after
        .iter()
        .filter(|it| !it.0 && it.7 && (it.5 - 1.5).abs() < 1e-3)
        .count();
    assert_eq!(dotted, 1, "cell(1,1) 左邊 dotted 1.5pt 恰 1 條：{after:?}");

    // 無損紅線：只准觸碰 word/document.xml。
    let diff = e.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(diff.pass, "roundtrip 必須通過: {:?}", diff.violations);
    assert_eq!(
        diff.whitelisted_changed,
        vec!["word/document.xml".to_string()],
        "儲存格邊框只准觸碰 word/document.xml"
    );
}

/// 燈 3：讀取面 LO 真值鎖（合成探針件，未經 Mutation）——sz→pt 換算逐條、
/// none／nil／hidden 不發射、inside 只畫內部。本盞在寫入層之前就該綠，
/// 作用是釘住既有發射面幾何，防下游卡把換算尺改錯。
#[test]
fn e_lamp3_read_side_matches_lo_truth() {
    let e = probe_engine();
    let all = rules(&e, 0);

    // 表 0（tblBorders：top single sz16 紅／left single sz8 黑／bottom none／
    // right none／insideH dashed sz12 藍／insideV none）＋表 1（全 none＋
    // cell(0,0) tcBorders top single sz24 綠）⇒ 共 9 條。
    assert_eq!(all.len(), 9, "探針件 Rule 總數基準：{all:?}");
    let w = |v: f32| {
        (v - 0.25).abs() < 1e-3
            || (v - 1.0).abs() < 1e-3
            || (v - 1.5).abs() < 1e-3
            || (v - 2.0).abs() < 1e-3
            || (v - 3.0).abs() < 1e-3
    };
    assert!(
        all.iter().all(|it| w(it.5)),
        "寬度必落在 sz/8 換算離散值：{all:?}"
    );
    // sz→pt 逐條（LO 25.8 實測同值：見 docs §2 對照表）。
    let at = |wd: f32, argb: u32| {
        all.iter()
            .filter(|it| (it.5 - wd).abs() < 1e-3 && it.6 == argb)
            .count()
    };
    assert_eq!(
        at(2.0, 0xFFFF_0000),
        2,
        "sz=16→2.0pt 紅頂邊 2 條（列 0 兩格）"
    );
    assert_eq!(
        at(1.0, 0xFF00_0000),
        2,
        "sz=8→1.0pt 黑左邊 2 條（第 0 欄兩列）"
    );
    assert_eq!(
        at(1.5, 0xFF00_00FF),
        4,
        "sz=12→1.5pt 藍虛內橫 4 條（列交界兩格×兩側）"
    );
    assert_eq!(
        at(3.0, 0xFF00_B050),
        1,
        "sz=24→3.0pt 綠（tcBorders 覆寫）1 條"
    );
    assert!(
        all.iter()
            .filter(|it| (it.5 - 1.5).abs() < 1e-3)
            .all(|it| it.7),
        "insideH（dashed）必須全帶虛線旗標"
    );
    // none／nil／hidden 零發射：表 0 無 right／insideV；表 1 無 top/bottom/left/right
    // （除 cell(0,0) 覆寫 top）——以「x=288（表右界）垂直線」與「y>130 的水平
    // 黑線」兩個探針位置證零發射。
    let right_edge_v = all
        .iter()
        .filter(|it| !it.0 && (it.1 - 288.0).abs() < 0.6)
        .count();
    assert_eq!(
        right_edge_v, 0,
        "right=none 不得發射（表右界 x=288 垂直線 0 條）"
    );
    let t1_black_h = all
        .iter()
        .filter(|it| it.0 && it.2 > 130.0 - 2.5 && it.6 == 0xFF00_0000)
        .count();
    assert_eq!(
        t1_black_h, 0,
        "表 1 全 none＋nil／hidden：y>130 不得有黑色水平線"
    );
    // inside 只畫內部：表 0 內橫線只出现在列交界（y≈105.6），不在表外框外。
    let inside_h = all
        .iter()
        .filter(|it| it.0 && (it.5 - 1.5).abs() < 1e-3)
        .map(|it| it.2)
        .collect::<Vec<_>>();
    assert!(
        inside_h.iter().all(|y| (y - 105.6).abs() < 2.5),
        "insideH 只在列交界 y≈105.6（LO 真值 103.15，容差見 docs §2）：{inside_h:?}"
    );
}

/// 燈 4：無損紅線——`borders: None`／全缺補丁＝整檔 byte-identical；
/// 有值補丁＝僅 `word/document.xml` 變更、其餘 entry 逐位元組原樣。
#[test]
fn e_lamp4_untouched_entries_stay_byte_identical() {
    // (a) 空補丁（三欄全 None）：整檔逐位元組不動。
    let mut e = probe_engine();
    e.apply_mutation(Mutation::SetTableProps {
        table: TablePath::body(0),
        patch: TablePropsPatch::default(),
    })
    .expect("空補丁必須成功（零擾動）");
    let diff = e.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(
        diff.byte_identical,
        "空邊框補丁必須 byte_identical；violations={:?}",
        diff.violations
    );

    // (b) 只設一個方位：既有未列方位必須原樣保留（容器重建但零遺失）。
    let mut e2 = probe_engine();
    e2.apply_mutation(Mutation::SetTableProps {
        table: TablePath::body(0),
        patch: TablePropsPatch {
            borders: Some(TableBordersPatch {
                top: Some(BorderSpec {
                    style: Some("double".into()),
                    width_pt: Some(1.5),
                    space_pt: Some(0.0),
                    color: Some("808080".into()),
                }),
                ..Default::default()
            }),
            ..Default::default()
        },
    })
    .expect("單方位 SetTableProps 失敗");
    let xml = doc_xml(&e2);
    // 逐方位斷言一律**限縮在表 0 容器段**內比對：表 1 另有 six-none 容器，
    // 整檔 `contains` 會讓「表 0 沒保留、表 1 同名字串頂過」變成假綠（燈 1(a′)
    // 同款取捨，這裡把口徑補齊到燈 4）。
    let seg0 = container_segment(&xml, "w:tblBorders", 0);
    assert!(
        seg0.contains(r#"<w:top w:val="double" w:sz="12" w:space="0" w:color="808080"/>"#),
        "top 覆寫未落地；實際 seg0={seg0}"
    );
    for keep in [
        // 以下四條逐字取自 fixtures/e_tblborder_probe.docx 表 0 原宣告
        // （scripts/gen_e_tblborder_fixture.py TBL0）——未列方位零擾動。
        r#"<w:left w:val="single" w:sz="8" w:space="0" w:color="auto"/>"#,
        r#"<w:bottom w:val="none" w:sz="0" w:space="0" w:color="auto"/>"#,
        r#"<w:insideH w:val="dashed" w:sz="12" w:space="0" w:color="0000FF"/>"#,
        r#"<w:insideV w:val="none" w:sz="0" w:space="0" w:color="auto"/>"#,
    ] {
        assert!(
            seg0.contains(keep),
            "未列方位必須原樣保留（容器重建零遺失）：缺 {keep}；實際 seg0={seg0}"
        );
    }
    // 被覆寫方位不得雙發（remove-then-append 的 remove 真生效），且原 top 宣告
    // （紅 sz=16）在表 0 容器內零殘留。
    assert_eq!(
        seg0.matches("<w:top ").count(),
        1,
        "top 覆寫必為「取代」而非「附加」（容器內 top 恰 1 條）；seg0={seg0}"
    );
    assert!(
        !seg0.contains(r#"<w:top w:val="single" w:sz="16""#),
        "原 top 宣告（single sz=16）必須被取代、零殘留；seg0={seg0}"
    );
    // 表 1（未被本補丁觸碰）容器段必須逐字元原樣——防「全檔重建容器」擴散。
    let seg1 = container_segment(&xml, "w:tblBorders", 1);
    for keep in [
        r#"<w:top w:val="none" w:sz="0" w:space="0" w:color="auto"/>"#,
        r#"<w:insideH w:val="none" w:sz="0" w:space="0" w:color="auto"/>"#,
        r#"<w:insideV w:val="none" w:sz="0" w:space="0" w:color="auto"/>"#,
    ] {
        assert!(
            seg1.contains(keep),
            "未觸碰的表 1 容器必須原樣；缺 {keep}；實際 seg1={seg1}"
        );
    }
    let diff = e2.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(diff.pass, "roundtrip 必須通過: {:?}", diff.violations);
    assert_eq!(
        diff.whitelisted_changed,
        vec!["word/document.xml".to_string()],
        "只准觸碰 word/document.xml"
    );
}

/// 燈 5：Undo 全量還原——邊框寫入可逆，Undo 後整檔 byte-identical。
#[test]
fn e_lamp5_undo_restores_byte_identical() {
    let mut e = probe_engine();
    e.apply_mutation(Mutation::SetCellProps {
        table: TablePath::body(1),
        row: 0,
        cell: 1,
        patch: CellPropsPatch {
            borders: Some(TableBordersPatch {
                top: Some(BorderSpec {
                    style: Some("single".into()),
                    width_pt: Some(0.75),
                    space_pt: Some(0.0),
                    color: Some("123456".into()),
                }),
                ..Default::default()
            }),
            ..Default::default()
        },
    })
    .expect("SetCellProps（邊框）失敗");
    let xml_after = doc_xml(&e);
    assert!(
        xml_after.contains(r#"<w:top w:val="single" w:sz="6" w:space="0" w:color="123456"/>"#),
        "寫入未落地（0.75pt→sz=6 四捨五入）；實際 xml={}",
        xml_snippet(&xml_after, "w:tcBorders")
    );
    let undone = e.undo().expect("undo 失敗").expect("undo 棧非空");
    assert!(
        matches!(
            undone.reversibility,
            ReversibilityLevel::StrictByteReversible
        ),
        "邊框寫入必須宣告為嚴格可逆：{:?}",
        undone.reversibility
    );
    let diff = e.roundtrip_check().expect("roundtrip_check 失敗");
    assert!(
        diff.byte_identical,
        "Undo 後整檔必須 byte_identical；violations={:?}",
        diff.violations
    );
}

/// 燈 6：臆造欄位誠實鎖（U4 紅線的反向鎖）——既有真欄位在，臆造欄仍被吞，
/// 但真欄位必須產生 Rule。若哪天 `borders` 欄又被拿掉，本盞前半仍綠、
/// 後半必紅 ⇒ 強制重估「邊框可用」的宣稱。
#[test]
fn e_lamp6_real_field_drives_rules_not_invented_ones() {
    let mut e = probe_engine();
    // 臆造欄（零散拼名，不撞本卡真欄位 `borders`）：serde 吞掉 ⇒ 零效果。
    let before = rules(&e, 0).len();
    let json = serde_json::json!({
        "SetTableProps": {
            "table": {"target": "Body", "root_slot": 1, "nested_steps": []},
            "patch": {"border_pt": 3, "border_style": "single", "border_color": "FF0000"}
        }
    });
    let op: Mutation =
        serde_json::from_value(json).expect("臆造欄 Mutation 必須可解析（serde 吞未知欄）");
    e.apply_mutation(op)
        .expect("臆造欄必須回 ok（serde 靜默吞）");
    settle(&mut e);
    assert_eq!(
        rules(&e, 0).len(),
        before,
        "臆造欄位必須零發射（U4 探針前提）"
    );

    // 真欄位：同一引擎實例送真補丁 ⇒ Rule 數必增（表 1 從 1 條變多條）。
    e.apply_mutation(Mutation::SetTableProps {
        table: TablePath::body(1),
        patch: TablePropsPatch {
            borders: Some(TableBordersPatch {
                bottom: Some(BorderSpec {
                    style: Some("single".into()),
                    width_pt: Some(1.0),
                    space_pt: Some(0.0),
                    color: Some("auto".into()),
                }),
                ..Default::default()
            }),
            ..Default::default()
        },
    })
    .expect("真欄位 SetTableProps 失敗");
    settle(&mut e);
    assert!(
        rules(&e, 0).len() > before,
        "真欄位必須產生 Rule（邊框可用宣稱）"
    );
}

/// 從 xml 抓第一個指定標籤附近片段（診斷用，避免把整份 XML 噴進斷言訊息）。
fn xml_snippet(xml: &str, tag: &str) -> String {
    match xml.find(&format!("<{tag}")) {
        Some(i) => {
            let end = (i + 700).min(xml.len());
            xml[i..end].to_string()
        }
        None => format!("<查無 {tag} 標籤>"),
    }
}

/// 抓第 `nth`（0-based）顆 `<tag>…</tag>` 容器段的**逐字元原文**（含標籤本體）。
/// 限縮比對範圍用：整檔 `contains` 會被另一顆同名容器頂過＝假綠。
fn container_segment(xml: &str, tag: &str, nth: usize) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    // 每次命中後游標必須越過整枚開標籤（`+ open.len()`）：只前移 1 字元會讓
    // 下一輪在同一位置再命中同一顆 ⇒ 第 2 顆實得第 1 顆（本卡實跑踩過）。
    let mut from = 0usize;
    let mut start = 0usize;
    for _ in 0..=nth {
        let rel = xml[from..].find(&open).unwrap_or_else(|| {
            panic!(
                "第 {} 顆 {tag} 容器必須存在；xml_len={}",
                nth + 1,
                xml.len()
            )
        });
        start = from + rel;
        from = start + open.len();
    }
    let rel = xml[start..]
        .find(close.as_str())
        .unwrap_or_else(|| panic!("{tag} 容器必須閉合；start={start}"));
    xml[start..start + rel + close.len()].to_string()
}
