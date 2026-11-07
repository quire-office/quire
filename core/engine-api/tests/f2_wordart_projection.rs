//! F2 紅燈：`a:prstGeom/@prst` 從未被消費——非矩形形状全畫成外接矩形。
//!
//! # 卡面前提的實測更正（docs/f2-wordart-anatomy.md §0／§1，本卡接手再逐檔實拆覆證）
//! `corpus/ooxmlexport/artistic_effects.docx` 的 drawing XML 實拆：`prstTxWarp`／
//! `wordArt`／`effectLst`／`bevelT`／`glow`／`gradFill` **全為 0**，`prstGeom` 僅
//! 2 個 `rect`（且在 `pic:spPr` 圖片裁切框）；該檔無任何文字（`<a:t>` 0／`<w:t>` 0）。
//! 卡面「我方整幅空白」不成立：現況 p0 已發射 **Image 2／Rect 1／Rule 4／Text 3**
//! （`quire-cli dump` 實抓，燈1a 鎖之）。圖軸 0.0 的根因是 OLE 圖示 EMF 向量記錄
//! 未光柵化（K27 閘門場域），不在本卡白名單，解剖報告 §3 已記帳上報。
//!
//! # 本檔鎖的真實缺口（白名單內、LO 真值可驗）
//! 全語料本體 part `prstGeom` 590 次、非 rect 195 次／約 25 檔（解剖報告 §5.4 普查），
//! 但 `@prst` 屬性在 `core/ooxml/src/*.rs` 實作路徑**零消費**——ellipse／star5／
//! triangle／connector 一律退化為外接矩形（1 Rect＋4 條軸對齊 Rule）。LO 真值
//! （pymupdf 抽 LO 轉換 PDF 的 `get_drawings()`，逐邊端點存 docs/f2-survey/）：
//! - `tdf113183.docx`：triangle = 3 條線段閉合、star5 = 10 條線段閉合（10 頂點
//!   實測已入 `preset.rs` 校準表）；我方現況兩者皆 Rect＋4 軸對齊 Rule。
//! - `tdf113258.docx`：ellipse = 12 段貝塞爾；我方外接矩形。
//! - `shapes-with-text.docx`：straightConnector1 = 1 條對角線；我方外接矩形四邊。
//!
//! # 紅燈規格（燈集中單一 suite，k25b／F1 紀律：`F2_GEOM_BUGGY` 的設／清只在
//! 單一測試函數內進行，不跨執行緒平行污染——發射層在開檔期讀該 env，拆多 test
//! 平行跑時正常態燈會誤吃回塞態假紅）
//! 1. 燈1a（卡面字面照辦）：artistic_effects p0 發射清單非空——**現況已綠**
//!    （卡面「現況即紅」前提實測推翻，證據與更正存 docs/f2-redlight.md）。
//! 2. 燈1b（真實缺口，現況即紅）：非矩形 prst 檔必須發**非軸對齊**筆畫（星形／
//!    三角形／橢圓的斜邊）；現況非軸對齊 Rule ＝ 0 → 必紅。
//! 3. 燈2（逐字比對源檔）：`shape_geom_facts()` 的 prst／warp 名稱與源檔 XML
//!    實拆雙向一致（源檔每個名稱都要在、我方不得虛構源檔沒有的）。
//! 4. 燈3（鑑別力）：`F2_GEOM_BUGGY=1` 回塞「几何節點整段跳過」舊行為——事實
//!    清空＋斜筆畫歸零；正常態二者必在。同一 suite 內前後兩態對照，紅燈由實作
//!    驅動而非環境偶然。
//! 5. 燈4（零擾動鎖）：artistic_effects（僅 rect prst）正常態與 buggy 態顯示
//!    列表 JSON **逐位元組相同**——新發射只走新分支（SVG byte 級 A/B 另見
//!    docs/f2-survey/svg_ab.sh）。

use quire_engine::DrawItem;
use quire_engine::Engine;

fn corpus(rel: &str) -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../corpus");
    p.push(rel);
    let p = std::path::Path::new(&p).canonicalize().unwrap_or(p);
    assert!(p.exists(), "語料缺失：{}", p.display());
    p
}

fn engine(rel: &str) -> Engine {
    let bytes = std::fs::read(corpus(rel)).expect("讀取語料");
    Engine::from_docx_bytes(&bytes).expect("引擎載入 docx 成功")
}

/// 源檔 `word/document.xml` 原始位元組（燈2 逐字比對的「源」）。
/// 走 `ZipBundle::open(...).entry(name).decoded()`：測試內直接解 zip，不經
/// 引擎發射面（避免「比對自己產物」的循環論證）。
fn source_part(rel: &str, part: &str) -> Vec<u8> {
    let bytes = std::fs::read(corpus(rel)).expect("讀取語料");
    let bundle = quire_ooxml::ZipBundle::open(&bytes).expect("開 zip");
    bundle
        .entry(part)
        .unwrap_or_else(|| panic!("{rel} 缺 part {part}"))
        .decoded()
        .expect("解 part")
}

/// 從源 XML 抓 `<a:{tag} ... prst="X"` 的出現序（文件序、含重覆）。
/// 口徑同 docs/f2-survey/warp_names.py（prst= 屬性、任意屬性順序）。
fn prst_sequence(xml: &[u8], tag: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut needle = Vec::with_capacity(tag.len() + 3);
    needle.extend_from_slice(b"<a:");
    needle.extend_from_slice(tag);
    let mut i = 0;
    while i + needle.len() <= xml.len() {
        if xml[i..i + needle.len()] == needle[..] {
            // 起標後 400 bytes 窗口內找 prst="（prst 恆為 prstGeom/prstTxWarp 首屬性）
            let win = &xml[i..(i + 400).min(xml.len())];
            if let Some(rel) = win.windows(6).position(|w| w == b"prst=\"") {
                let s = i + rel + 6;
                let mut j = s;
                while j < xml.len() && xml[j] != b'"' {
                    j += 1;
                }
                out.push(String::from_utf8_lossy(&xml[s..j]).into_owned());
            }
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

/// 星形／三角形／橢圓／連接線的邊是**非軸對齊**筆畫；外接矩形退化態的四條邊
/// 恒軸對齊（x1==x2 或 y1==y2）。本函數＝該頁非軸對齊 Rule 數（容差 0.05pt）。
fn slanted_rules(eng: &Engine, page: usize) -> usize {
    let dl = eng.page_display_list(page).expect("頁存在");
    dl.items
        .iter()
        .filter(|it| match it {
            DrawItem::Rule {
                x1, y1, x2, y2, ..
            } => (x1 - x2).abs() > 0.05 && (y1 - y2).abs() > 0.05,
            _ => false,
        })
        .count()
}

/// 全部頁顯示列表 JSON（燈4 逐位元組比對用）。
fn all_pages_json(eng: &Engine) -> String {
    (0..eng.page_count())
        .map(|p| eng.page_display_list_json(p).expect("頁存在"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn f2_projection_suite() {
    // 基準態先強制清空（外部殘留 env 不得污染正常態段）。
    std::env::remove_var("F2_GEOM_BUGGY");

    // ---- 燈1a：卡面字面——artistic_effects p0 發射清單不得為空。 ----
    // 現況實測 11 項（Image 2／Rect 1／Rule 4／Text 3）＝現況已綠；卡面
    // 「現況即紅，因為整幅無發射」的前提經實拆推翻（docs/f2-redlight.md）。
    // 本燈保留作防回退：任何人把該檔發射弄空，本燈必紅。
    {
        let eng = engine("ooxmlexport/artistic_effects.docx");
        let dl = eng.page_display_list(0).expect("p0 存在");
        assert!(
            !dl.items.is_empty(),
            "artistic_effects p0 發射清單不得為空（LO 有圖形、我方也必須有東西）"
        );
        let images = dl
            .items
            .iter()
            .filter(|it| matches!(it, DrawItem::Image { .. }))
            .count();
        assert_eq!(
            images, 2,
            "兩張內嵌 PNG 像素發射必須維持（K4c-image 既有能力零回退）"
        );
    }

    // ---- 燈1b（紅燈主燈，修前實測全紅）：非矩形 prst 必須發非軸對齊筆畫。 ----
    // 拍紅實測（2026-11-07，基線 f6f83bf）：三檔斜筆畫全 0。
    {
        // star5（LO：10 頂點閉合折線，斜邊 ≥ 6）＋ triangle（LO：3 邊，斜邊 2）。
        let eng = engine("ooxmlexport/tdf113183.docx");
        let n = slanted_rules(&eng, 0);
        assert!(
            n >= 6,
            "tdf113183 star5 十邊輪廓（斜邊多數）必須發非軸對齊筆畫；實測 {n}（修前 0＝外接矩形退化）"
        );
        // ellipse（LO：12 段貝塞爾 → 我方折線近似，斜段 ≥ 8）。
        let eng = engine("ooxmlexport/tdf113258.docx");
        let n = slanted_rules(&eng, 0);
        assert!(
            n >= 8,
            "tdf113258 ellipse 折線近似必須發斜筆畫；實測 {n}（修前 0）"
        );
        // straightConnector1（LO 實測：對角線 (92.2,266.2)-(272.8,236.3)）。
        let eng = engine("poi/test-data_document_shapes-with-text.docx");
        let n = slanted_rules(&eng, 0);
        assert!(
            n >= 1,
            "shapes-with-text connector 對角線必須發射；實測 {n}（修前 0）"
        );
    }

    // ---- 燈2：prst／warp 名稱解析與源檔逐字雙向一致。 ----
    {
        let cases = [
            "ooxmlexport/tdf113183.docx",
            "ooxmlexport/tdf113258.docx",
            "ooxmlexport/fdo80897.docx",
            "poi/test-data_document_shapes-with-text.docx",
        ];
        for rel in cases {
            let xml = source_part(rel, "word/document.xml");
            let want_geom = prst_sequence(&xml, b"prstGeom");
            let want_warp = prst_sequence(&xml, b"prstTxWarp");
            let eng = engine(rel);
            let facts = eng.shape_geom_facts();
            let got_geom: Vec<String> = facts
                .iter()
                .filter_map(|(g, _)| g.clone())
                .filter(|g| g != "rect") // rect＝退化態既有發射（源檔也常有 rect，比對時不排除）
                .collect();
            let got_all_geom: Vec<&str> = facts.iter().filter_map(|(g, _)| g.as_deref()).collect();
            let got_warp: Vec<&str> = facts.iter().filter_map(|(_, w)| w.as_deref()).collect();
            // 每個非 rect 的源檔 prst 名稱必須被我方解析出來（缺口正面鎖）。
            for name in want_geom.iter().filter(|n| n.as_str() != "rect") {
                assert!(
                    got_geom.iter().any(|g| g == name),
                    "{rel}：源檔 prstGeom 名稱 {name:?} 必須出現在解析事實；實測非 rect {got_geom:?}（全量 {got_all_geom:?}）"
                );
            }
            for name in &want_warp {
                assert!(
                    got_warp.contains(&name.as_str()),
                    "{rel}：源檔 prstTxWarp 名稱 {name:?} 必須出現在解析事實；實測 {got_warp:?}"
                );
            }
            // 防灌水：我方事實不得虛構源檔沒有的名稱（源檔＝document.xml 實拆；
            // 群組子件／pic 裁切框同檔同口徑，無跨檔來源可辯）。
            for g in &got_all_geom {
                assert!(
                    want_geom.iter().any(|w| w == g),
                    "{rel}：我方虛構 prstGeom 名稱 {g:?}（源檔實拆 {want_geom:?}）"
                );
            }
            for wp in &got_warp {
                assert!(
                    want_warp.iter().any(|w| w == wp),
                    "{rel}：我方虛構 warp 名稱 {wp:?}（源檔實拆 {want_warp:?}）"
                );
            }
        }
        // 錨點具名斷言（防「湊集合」假綠）：fdo80897 的 textTriangle 是全語料
        // 唯一非 textNoShape warp（docs/f2-survey/f2_warp_names.json 普查口徑）。
        let eng = engine("ooxmlexport/fdo80897.docx");
        let facts = eng.shape_geom_facts();
        assert!(
            facts
                .iter()
                .any(|(_, w)| w.as_deref() == Some("textTriangle")),
            "fdo80897 必須解析出 textTriangle（全語料唯一真實翹曲）；實測 {facts:?}"
        );
    }

    // ---- 燈4（正常態側取樣）：artistic_effects 顯示列表 JSON 先存一份。 ----
    let json_normal = all_pages_json(&engine("ooxmlexport/artistic_effects.docx"));

    // ---- 燈3（回塞態）：F2_GEOM_BUGGY=1 整段跳過几何節點＝舊行為。 ----
    std::env::set_var("F2_GEOM_BUGGY", "1");
    {
        let eng = engine("ooxmlexport/tdf113183.docx");
        let facts = eng.shape_geom_facts();
        assert!(
            facts.iter().all(|(g, w)| g.is_none() && w.is_none()),
            "回塞態 prst/warp 事實必須整段清空（舊行為＝prst 從未被讀取）；實測 {facts:?}"
        );
        assert_eq!(
            slanted_rules(&eng, 0),
            0,
            "回塞態必須退回外接矩形退化（斜筆畫 0）——考卷抓得到舊行為即證鑑別力"
        );
        // 燈4（回塞態側）：僅 rect prst 檔案兩態顯示列表逐位元組相同。
        let json_buggy = all_pages_json(&engine("ooxmlexport/artistic_effects.docx"));
        assert_eq!(
            json_normal, json_buggy,
            "僅 rect prst 檔案的顯示列表必須零位元組差異（新發射只走新分支）"
        );
    }
    std::env::remove_var("F2_GEOM_BUGGY");

    eprintln!("F2 考卷：燈1a（發射非空）／燈1b（斜筆畫）／燈2（prst 逐字）／燈3（回塞鑑別）／燈4（零擾動）通過");
}
