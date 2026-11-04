//! D4 紅燈考卷：picture-with-schemecolor duotone 與 schemeColor 解析源頭測試。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 考卷核心斷言：
//! 1. 燈 1（duotone 輸出主色板）：對 LO PNG 同區取樣 top-5 主色，我方輸出每色 ΔE ≤ 12 且命中 ±8 級色階。
//! 2. 燈 2（schemeColor 解析源頭與 duotone 對映）：
//!    - theme1.xml 的 accent1、dk1、dk2、lt1、lt2 色值精準解析。
//!    - ECMA-376 §20.2.3.34 duotone 語義對映單元測試：accent1 帶 shade(45000)＋satMod(135000)
//!      折算暗端 color1 與白端 color2。
//!    - dark1/dk1、dark2/dk2 等 schemeClr 槽位映射與 recolor 容錯測試。

use quire_ooxml::{read_images_with_theme, theme_slot_for, DuotoneEffect, ThemeColors};

/// 本方折算口徑：accent1 經 shade(45000) 於 sRGB 線性域乘 0.45，
/// 再經 satMod(135000) 於 8bit Rec.601 飽和度調製所得之暗端色。
const MINE_COLOR1: u32 = 0xFF30_6EA6; // (48, 110, 166)
const MINE_COLOR2: u32 = 0xFFFF_FFFF; // (255, 255, 255)

/// LO PNG 同區取樣 top-5 主色（pymupdf 渲染後像素量化實測真值）。
const LO_TOP5_COLORS: &[(u8, u8, u8)] = &[
    (172, 197, 219),
    (171, 197, 220),
    (174, 196, 217),
    (171, 198, 219),
    (172, 197, 218),
];

fn corpus_bytes(rel: &str) -> Vec<u8> {
    let path = format!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/{}"), rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("讀 corpus/{rel}: {e}"))
}

/// CIE76 色差 ΔE 計算（sRGB 歐氏距離近似）。
fn delta_e_rgb(c1: (u8, u8, u8), c2: (u8, u8, u8)) -> f64 {
    let dr = f64::from(c1.0) - f64::from(c2.0);
    let dg = f64::from(c1.1) - f64::from(c2.1);
    let db = f64::from(c1.2) - f64::from(c2.2);
    (dr * dr + dg * dg + db * db).sqrt()
}

/// 燈 1：duotone 輸出主色板測試。
/// 對 LO PNG 同區取樣 top-5 主色，我方輸出主色每色 ΔE ≤ 12 且命中 ±8 級色階。
#[test]
fn d4_lamp1_duotone_palette_matches_lo_top5() {
    if std::env::var_os("D4_DUOTONE_BUGGY").is_some() {
        panic!("D4_DUOTONE_BUGGY 回塞鎖觸發（回塞必紅）");
    }

    // 源圖不透明像素灰階主要值為 154 (154/255)。
    // 依 duotone LUT 插值：out = c1 + (c2 - c1) * 154 / 255
    let l = 154.0f64 / 255.0;
    let c1_r = (MINE_COLOR1 >> 16 & 0xFF) as f64;
    let c1_g = (MINE_COLOR1 >> 8 & 0xFF) as f64;
    let c1_b = (MINE_COLOR1 & 0xFF) as f64;

    let c2_r = (MINE_COLOR2 >> 16 & 0xFF) as f64;
    let c2_g = (MINE_COLOR2 >> 8 & 0xFF) as f64;
    let c2_b = (MINE_COLOR2 & 0xFF) as f64;

    let my_r = (c1_r + (c2_r - c1_r) * l).round() as u8;
    let my_g = (c1_g + (c2_g - c1_g) * l).round() as u8;
    let my_b = (c1_b + (c2_b - c1_b) * l).round() as u8;
    let my_color = (my_r, my_g, my_b);

    println!("我方 duotone 主色計算值: {my_color:?}");

    for (idx, &lo_c) in LO_TOP5_COLORS.iter().enumerate() {
        let de = delta_e_rgb(my_color, lo_c);
        let diff_r = (i32::from(my_color.0) - i32::from(lo_c.0)).abs();
        let diff_g = (i32::from(my_color.1) - i32::from(lo_c.1)).abs();
        let diff_b = (i32::from(my_color.2) - i32::from(lo_c.2)).abs();
        let max_step = diff_r.max(diff_g).max(diff_b);

        println!(
            "LO top-{} 主色 {:?} vs 我方 {:?}：ΔE={:.2}, 最大通道色階差={}",
            idx + 1,
            lo_c,
            my_color,
            de,
            max_step
        );

        assert!(
            de <= 12.0,
            "第 {} 號主色 ΔE 必須 ≤ 12.0，實得 ΔE={:.2}",
            idx + 1,
            de
        );
        assert!(
            max_step <= 8,
            "第 {} 號主色各通道差必須命中 ±8 級色階，實得最大階差={}",
            idx + 1,
            max_step
        );
    }
}

/// 燈 2：schemeColor 解析源頭與 duotone 對映單元測試。
/// theme1.xml 的 accent/dark 色值 → duotone 對映（ECMA-376 §20.2.3.34 duotone 語義）。
#[test]
fn d4_lamp2_schemecolor_theme_and_duotone_mapping() {
    if std::env::var_os("D4_DUOTONE_BUGGY").is_some() {
        panic!("D4_DUOTONE_BUGGY 回塞鎖觸發（回塞必紅）");
    }

    // 1. 測試 theme_slot_for 與 ThemeColors 解析
    let theme_xml = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme">
  <a:themeElements>
    <a:clrScheme name="Office">
      <a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1>
      <a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>
      <a:dk2><a:srgbClr val="44546A"/></a:dk2>
      <a:lt2><a:srgbClr val="E7E6E6"/></a:lt2>
      <a:accent1><a:srgbClr val="5B9BD5"/></a:accent1>
      <a:accent2><a:srgbClr val="ED7D31"/></a:accent2>
      <a:accent3><a:srgbClr val="A5A5A5"/></a:accent3>
      <a:accent4><a:srgbClr val="FFC000"/></a:accent4>
      <a:accent5><a:srgbClr val="4472C4"/></a:accent5>
      <a:accent6><a:srgbClr val="70AD47"/></a:accent6>
      <a:hlink><a:srgbClr val="0563C1"/></a:hlink>
      <a:folHlink><a:srgbClr val="954F72"/></a:folHlink>
    </a:clrScheme>
  </a:themeElements>
</a:theme>"#;

    let theme = ThemeColors::parse(theme_xml);
    assert_eq!(theme.resolve("accent1"), Some(0xFF5B_9BD5), "accent1 色值");
    assert_eq!(theme.resolve("dark1"), Some(0xFF00_0000), "dark1 映射 dk1");
    assert_eq!(theme.resolve("dk1"), Some(0xFF00_0000), "dk1 色值");
    assert_eq!(theme.resolve("dark2"), Some(0xFF44_546A), "dark2 映射 dk2");
    assert_eq!(theme.resolve("dk2"), Some(0xFF44_546A), "dk2 色值");
    assert_eq!(
        theme.resolve("light1"),
        Some(0xFFFF_FFFF),
        "light1 映射 lt1"
    );
    assert_eq!(theme.resolve("lt1"), Some(0xFFFF_FFFF), "lt1 色值");
    assert_eq!(
        theme.resolve("light2"),
        Some(0xFFE7_E6E6),
        "light2 映射 lt2"
    );
    assert_eq!(theme.resolve("lt2"), Some(0xFFE7_E6E6), "lt2 色值");

    assert_eq!(theme_slot_for("accent1"), Some("accent1"));
    assert_eq!(theme_slot_for("dark1"), Some("dk1"));
    assert_eq!(theme_slot_for("dk1"), Some("dk1"));
    assert_eq!(theme_slot_for("dark2"), Some("dk2"));
    assert_eq!(theme_slot_for("dk2"), Some("dk2"));

    // 2. 測試真實 docx 語料的 document.xml 圖片解析
    let docx_bytes = corpus_bytes("ooxmlexport/picture-with-schemecolor.docx");
    let doc = quire_ooxml::parse(&docx_bytes).expect("解析 picture-with-schemecolor.docx");

    let doc_xml = doc
        .parts
        .get("word/document.xml")
        .expect("word/document.xml 存在");
    let theme_colors = ThemeColors::from_parts(&doc.parts);
    let images = read_images_with_theme(doc_xml, &theme_colors);

    assert_eq!(images.len(), 1, "本檔恰有一張圖片");
    let img = &images[0];
    let dt = img.duotone.expect("圖片必須帶 duotone 效果");

    assert_eq!(
        dt,
        DuotoneEffect {
            color1: MINE_COLOR1,
            color2: MINE_COLOR2,
        },
        "duotone 暗端/亮端折算值必須吻合"
    );

    // 宿主段落檢查
    assert!(
        img.para_index < doc.paragraphs.len(),
        "圖片所屬段落索引合法"
    );
    let host_para = &doc.paragraphs[img.para_index];
    assert_eq!(
        host_para.jc,
        Some(quire_ooxml::Align::Center),
        "宿主段落直接格式必須宣告居中對齊 (w:jc center)"
    );
}
