//! 顯示列表 → SVG（render-lab 跨引擎比對用；骨架級，逐 DrawItem 直出）。
//!
//! 注意：這是「幾何直譯」而非排版——座標全來自 layout，SVG 只負責把它畫出來。
//! 目的：headless 環境下不需要任何字型 shaping 就能拿到位圖/向量輸出，供 LO 基線對照。

use quire_engine::{
    DrawItem, PageDisplayList, FLAG_BOLD, FLAG_ITALIC, FLAG_STRIKE, FLAG_UNDERLINE,
};
use std::fmt::Write as _;

fn esc(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
}

/// 單頁顯示列表 → 完整 SVG 字串（座標單位即 pt，viewBox 與頁面同尺寸）。
pub fn page_to_svg(dl: &PageDisplayList) -> String {
    let mut s = String::new();
    let _ = write!(
        s,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.0}pt\" height=\"{h:.0}pt\" \
viewBox=\"0 0 {w:.0} {h:.0}\">\n<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>\n",
        w = dl.width_pt,
        h = dl.height_pt
    );
    for item in &dl.items {
        match item {
            DrawItem::Text {
                x,
                y,
                size_pt,
                flags,
                color_argb,
                family,
                text,
            } => {
                // baseline = y + 字級：讓 y 語義接近「行頂」，與 display list 座標系一致。
                let a = (*color_argb >> 24) & 0xFF;
                let r = (*color_argb >> 16) & 0xFF;
                let g = (*color_argb >> 8) & 0xFF;
                let b = *color_argb & 0xFF;
                let _ = write!(
                    s,
                    "<text x=\"{x:.1}\" y=\"{:.1}\" font-size=\"{size_pt:.1}\" fill=\"#{r:02x}{g:02x}{b:02x}\"",
                    y + size_pt
                );
                if a < 255 {
                    let _ = write!(s, " fill-opacity=\"{:.3}\"", a as f64 / 255.0);
                }
                // 文件宣告了字型族才寫屬性；`None` 時輸出與舊版逐位元組相同。
                if let Some(family) = family {
                    s.push_str(" font-family=\"");
                    esc(family, &mut s);
                    s.push('"');
                }
                if flags & FLAG_BOLD != 0 {
                    s.push_str(" font-weight=\"bold\"");
                }
                if flags & FLAG_ITALIC != 0 {
                    s.push_str(" font-style=\"italic\"");
                }
                match (flags & FLAG_UNDERLINE != 0, flags & FLAG_STRIKE != 0) {
                    (true, true) => s.push_str(" text-decoration=\"underline line-through\""),
                    (true, false) => s.push_str(" text-decoration=\"underline\""),
                    (false, true) => s.push_str(" text-decoration=\"line-through\""),
                    (false, false) => {}
                }
                s.push_str(" xml:space=\"preserve\">");
                esc(text, &mut s);
                s.push_str("</text>\n");
            }
            DrawItem::Rect {
                x,
                y,
                w,
                h,
                fill_argb,
            } => {
                // R34：alpha-0 的矩形在 SVG 仍 hit-testable（透明≠不存在），
                // 會無形中吃掉上層事件的 pointer 命中 ⇒ 填色全透明的 Rect
                // 一律不輸出（發射層 R34 起對表內無填充框根已改「不發 Rect」，
                // 此處是兜底閂：任何其他來源的 alpha-0 矩形同樣不落 SVG）。
                if (*fill_argb >> 24) == 0 {
                    continue;
                }
                let a = ((*fill_argb >> 24) & 0xFF) as f64 / 255.0;
                let r = (fill_argb >> 16) & 0xFF;
                let g = (fill_argb >> 8) & 0xFF;
                let b = fill_argb & 0xFF;
                let _ = writeln!(
                    s,
                    "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" \
fill=\"#{r:02x}{g:02x}{b:02x}\" fill-opacity=\"{a:.3}\"/>"
                );
            }
            DrawItem::Image {
                x,
                y,
                w,
                h,
                mime,
                data_b64,
                alt,
                duotone: _, // K19：data_b64 已是變換後像素（宣告欄僅診斷透傳）。
            } => {
                // K4c-image：真像素直出（取代同位灰底佔位框）。
                // preserveAspectRatio="none"＝像素拉伸到 extent 框（LO 同款
                // 近似：wp:extent 就是渲染框，不管源圖像素_aspect_）。
                s.push_str("<image ");
                let _ = write!(
                    s,
                    "x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" "
                );
                s.push_str("preserveAspectRatio=\"none\" href=\"data:");
                s.push_str(mime);
                s.push_str(";base64,");
                s.push_str(data_b64);
                s.push('"');
                if let Some(a) = alt {
                    s.push_str(" alt=\"");
                    esc(a, &mut s);
                    s.push('"');
                }
                s.push_str("/>\n");
            }
            DrawItem::Rule {
                x1,
                y1,
                x2,
                y2,
                width_pt,
                color_argb,
                dash,
            } => {
                // argb → #rrggbb + fill-opacity（與 Rect 分支同一口徑）。
                let a = ((*color_argb >> 24) & 0xFF) as f64 / 255.0;
                let r = (color_argb >> 16) & 0xFF;
                let g = (color_argb >> 8) & 0xFF;
                let b = color_argb & 0xFF;
                let _ = write!(
                    s,
                    "<line x1=\"{x1:.1}\" y1=\"{y1:.1}\" x2=\"{x2:.1}\" y2=\"{y2:.1}\" \
stroke=\"#{r:02x}{g:02x}{b:02x}\" stroke-opacity=\"{a:.3}\" stroke-width=\"{width_pt:.2}\""
                );
                if let Some(d) = dash {
                    let _ = write!(s, " stroke-dasharray=\"{d}\"");
                }
                // 線段座標已由 layout 端做半線內收，這裡只 crisp 繪出。
                s.push_str(" shape-rendering=\"crispEdges\"/>\n");
            }
        }
    }
    s.push_str("</svg>\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use quire_engine::Engine;

    const SAMPLE: &[u8] = include_bytes!("../../../fixtures/sample.docx");
    const INLINE_IMG: &[u8] = include_bytes!("../../../fixtures/inline_image.docx");

    /// K4c-image：inline 圖真像素落地 SVG——`<image href="data:image/png;
    /// base64,…">`（PNG magic 前綴 iVBORw0KGgo）、尺寸＝extent（200×100pt）、
    /// preserveAspectRatio="none"（extent 框拉伸口徑）。灰底佔位框退位。
    #[test]
    fn svg_emits_embedded_image_pixels() {
        let engine = Engine::from_docx_bytes(INLINE_IMG).unwrap();
        let svg = page_to_svg(&engine.page_display_list(0).unwrap());
        assert!(svg.contains("<image "), "SVG 應含 <image> 真像素項：{svg}");
        assert!(
            svg.contains("href=\"data:image/png;base64,iVBORw0KGgo"),
            "data URI 帶 PNG magic 像素：{svg}"
        );
        assert!(
            svg.contains("width=\"200.0\" height=\"100.0\""),
            "image 尺寸＝extent 200×100：{svg}"
        );
        assert!(
            svg.contains("preserveAspectRatio=\"none\""),
            "extent 框拉伸口徑：{svg}"
        );
        assert!(
            !svg.contains("fill=\"#f0f0f0\""),
            "真像素處不再發灰底佔位框：{svg}"
        );
    }

    /// t_a1397522 負向臂（K4c 改掛 EMF 檔）：不可渲染格式（EMF 浮水印）仍
    /// 走灰底 Rect（#f0f0f0）＋四邊淺灰 Rule（#cccccc）佔位——誠實近似。
    #[test]
    fn svg_emits_image_placeholder_rect_and_rules() {
        const EMF: &[u8] = include_bytes!("../../../corpus/ooxmlexport/n779834.docx");
        let engine = Engine::from_docx_bytes(EMF).unwrap();
        let mut saw_rect = false;
        let mut saw_rules = false;
        for page in 0..engine.page_count() {
            let svg = page_to_svg(&engine.page_display_list(page).unwrap());
            saw_rect |= svg.contains("fill=\"#f0f0f0\"");
            saw_rules |= svg.split("stroke=\"#cccccc\"").count() - 1 == 4;
            assert!(
                !svg.contains("<image "),
                "EMF 不可渲染 ⇒ 第 {page} 頁不得發 <image>：{svg}"
            );
        }
        assert!(saw_rect, "EMF 浮水印必須維持灰底佔位 Rect");
        assert!(saw_rules, "佔位框四邊各一條 #cccccc Rule");
    }

    #[test]
    fn svg_outputs_red_fill_bold_and_text_decorations_from_fixture() {
        let mut base = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx");
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r>
        <w:rPr>
          <w:b/>
          <w:color w:val="FF0000"/>
        </w:rPr>
        <w:t>紅字粗體標題</w:t>
      </w:r>
      <w:r>
        <w:rPr>
          <w:u w:val="single"/>
        </w:rPr>
        <w:t>底線段</w:t>
      </w:r>
      <w:r>
        <w:rPr>
          <w:strike/>
        </w:rPr>
        <w:t>刪除線段</w:t>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#
            .as_bytes();
        base.set_part("word/document.xml", xml.to_vec());
        let docx = base.save_docx().unwrap();
        let engine = Engine::from_docx_bytes(&docx).unwrap();
        let dl = engine.page_display_list(0).unwrap();
        let svg = page_to_svg(&dl);
        assert!(
            svg.contains("fill=\"#ff0000\""),
            "SVG 應含紅色 fill：{}",
            svg
        );
        assert!(
            svg.contains("font-weight=\"bold\""),
            "SVG 粗體段應含 font-weight=\"bold\"：{}",
            svg
        );
        assert!(
            svg.contains("text-decoration=\"underline\""),
            "SVG 底線段應含 text-decoration=\"underline\"：{}",
            svg
        );
        assert!(
            svg.contains("text-decoration=\"line-through\""),
            "SVG 刪除線段應含 text-decoration=\"line-through\"：{}",
            svg
        );
    }

    /// (e) XML 轉義涵蓋 `& < > " '`（font-family 屬性值同用此函式）。
    #[test]
    fn svg_escapes_xml_special_characters() {
        let mut out = String::new();
        esc("A&B<C>D\"E'F", &mut out);
        assert_eq!(out, "A&amp;B&lt;C&gt;D&quot;E&apos;F");
    }

    /// (c) 無字型宣告的文件：SVG 完全不寫 font-family。
    #[test]
    fn svg_omits_font_family_without_declaration() {
        let engine = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx");
        let svg = page_to_svg(&engine.page_display_list(0).unwrap());
        assert!(
            !svg.contains("font-family"),
            "無 w:rFonts 宣告時不得輸出 font-family：{svg}"
        );
    }

    /// 字型名含 XML 特殊字元時，font-family 屬性必須完整跳脫且不破壞標籤。
    #[test]
    fn svg_escapes_font_family_attribute() {
        let mut base = Engine::from_docx_bytes(SAMPLE).expect("開啟 sample.docx");
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r>
        <w:rPr>
          <w:rFonts w:ascii="A&amp;B&lt;C&gt;D&quot;E&apos;F"/>
        </w:rPr>
        <w:t>特殊字元字型名</w:t>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
        base.set_part("word/document.xml", xml.as_bytes().to_vec());
        let docx = base.save_docx().expect("存回 docx 失敗");
        let engine = Engine::from_docx_bytes(&docx).expect("開啟測試 docx 失敗");
        let svg = page_to_svg(&engine.page_display_list(0).unwrap());
        assert!(
            svg.contains("font-family=\"A&amp;B&lt;C&gt;D&quot;E&apos;F\""),
            "font-family 屬性值必須跳脫 &<>\"'：{svg}"
        );
    }

    /// 有字型宣告的文件：`<text>` 寫出 font-family 屬性。
    #[test]
    fn svg_emits_font_family_from_declaration() {
        let engine = Engine::from_docx_bytes(INLINE_IMG).expect("開啟 inline_image.docx");
        let svg = page_to_svg(&engine.page_display_list(0).unwrap());
        assert!(
            svg.contains("font-family=\"Calibri\""),
            "docDefaults 宣告 w:ascii=Calibri → SVG 應寫 font-family：{svg}"
        );
    }

    /// R34 兜底閂：alpha-0（全透明）Rect 一律不落 SVG——SVG 的透明 fill 仍
    /// hit-testable（透明≠不存在），會無形中吃掉上層 pointer 命中。本測直接
    /// 構造顯示列表餵 `page_to_svg`（不依賴特定 corpus 檔的發射態）。
    #[test]
    fn svg_skips_fully_transparent_rects() {
        let dl = PageDisplayList {
            page: 0,
            width_pt: 400.0,
            height_pt: 300.0,
            items: vec![
                DrawItem::Rect {
                    x: 10.0,
                    y: 10.0,
                    w: 100.0,
                    h: 50.0,
                    fill_argb: 0x0000_0000, // alpha=0：幽靈矩形
                },
                DrawItem::Rect {
                    x: 20.0,
                    y: 20.0,
                    w: 60.0,
                    h: 30.0,
                    fill_argb: 0x0111_2233, // alpha=1：仍算可見，照常輸出
                },
                DrawItem::Rect {
                    x: 30.0,
                    y: 30.0,
                    w: 40.0,
                    h: 20.0,
                    fill_argb: 0xFF44_72C4, // 不透明：照常輸出
                },
            ],
        };
        let svg = page_to_svg(&dl);
        let n = svg.matches("<rect ").count();
        // 頁背景白底 1 個＋可見 Rect 2 個＝3；alpha-0 幽靈被跳過。
        assert_eq!(
            n, 3,
            "alpha-0 Rect 不得輸出（SVG 透明 rect 仍 hit-testable）：{svg}"
        );
        assert!(
            !svg.contains("fill-opacity=\"0.000\""),
            "不得殘留 fill-opacity=0.000 的矩形：{svg}"
        );
        assert!(svg.contains("#4472c4"), "不透明 Rect 照常輸出：{svg}");
        assert!(
            svg.contains("#112233"),
            "alpha=1 的可見 Rect 照常輸出：{svg}"
        );
    }
}
