//! 顯示列表 → SVG（render-lab 跨引擎比對用；骨架級，逐 DrawItem 直出）。
//!
//! 注意：這是「幾何直譯」而非排版——座標全來自 layout，SVG 只負責把它畫出來。
//! 目的：headless 環境下不需要任何字型 shaping 就能拿到位圖/向量輸出，供 LO 基線對照。

use quire_engine::{DrawItem, PageDisplayList, FLAG_BOLD, FLAG_ITALIC};
use std::fmt::Write as _;

fn esc(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
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
                text,
            } => {
                // baseline = y + 字級：讓 y 語義接近「行頂」，與 display list 座標系一致。
                let _ = write!(
                    s,
                    "<text x=\"{x:.1}\" y=\"{:.1}\" font-size=\"{size_pt:.1}\"",
                    y + size_pt
                );
                if flags & FLAG_BOLD != 0 {
                    s.push_str(" font-weight=\"bold\"");
                }
                if flags & FLAG_ITALIC != 0 {
                    s.push_str(" font-style=\"italic\"");
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
