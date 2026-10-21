//! quire-render-trait —— 平台無關的顯示列表（display list）契約。
//!
//! 原則（研究報告 §6）：文書區必須自繪。core 只產出「幾何+文字」的顯示列表，
//! 像素怎麼來由 shells 決定：native 走 wgpu（vello 為候選），web 走 Canvas2D/WebGPU。

use serde::{Deserialize, Serialize};

pub const FLAG_BOLD: u8 = 1;
pub const FLAG_ITALIC: u8 = 2;
pub const FLAG_UNDERLINE: u8 = 4;
pub const FLAG_STRIKE: u8 = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DrawItem {
    Text {
        x: f32,
        y: f32,
        size_pt: f32,
        flags: u8,
        /// 文字顏色（0xAARRGGBB）；預設不透明黑（0xFF000000），保持舊顯示列表 JSON 向後相容。
        #[serde(default = "default_opaque_black")]
        color_argb: u32,
        /// 字型族名（`w:rFonts` 級聯選取結果；ECMA-376 §17.3.2.26）。
        /// `None`＝文件未宣告（渲染端用自身預設），舊顯示列表 JSON 照解析。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        family: Option<String>,
        text: String,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill_argb: u32,
    },
    /// 筆畫（表格框線等）：寬度/顏色/虛線樣式齊備（Phase3-1b）。
    /// 新增欄位全部帶 serde default——舊顯示列表 JSON 仍可解析（向後兼容）。
    Rule {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        /// 線寬（pt）；0 → 渲染端自行選預設。
        #[serde(default)]
        width_pt: f32,
        /// 筆畫顏色（0xAARRGGBB）。
        #[serde(default = "default_opaque_black")]
        color_argb: u32,
        /// 虛線樣式（SVG stroke-dasharray 語義，如 "2 2"；None＝實線）。
        #[serde(default)]
        dash: Option<String>,
    },
}

fn default_opaque_black() -> u32 {
    0xFF00_0000
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageDisplayList {
    pub page: usize,
    pub width_pt: f32,
    pub height_pt: f32,
    pub items: Vec<DrawItem>,
}

pub trait Renderer {
    type Error;
    /// 渲染單頁顯示列表。實作者必須支援增量（只重繪失效區域）以達成 120fps 目標。
    fn render_page(&mut self, dl: &PageDisplayList) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_color_defaults_to_opaque_black_and_flags_match_contract() {
        assert_eq!(FLAG_BOLD, 1);
        assert_eq!(FLAG_ITALIC, 2);
        assert_eq!(FLAG_UNDERLINE, 4);
        assert_eq!(FLAG_STRIKE, 8);
        assert_eq!(default_opaque_black(), 0xFF00_0000);
    }

    /// (d) 舊顯示列表 JSON（無 family 欄）仍可解析，family 落回 None。
    #[test]
    fn legacy_text_json_without_family_deserializes_to_none() {
        let json = r#"{"Text":{"x":1.0,"y":2.0,"size_pt":12.0,"flags":0,"text":"舊格式"}}"#;
        let item: DrawItem = serde_json::from_str(json).expect("舊顯示列表 JSON 必須可解析");
        match item {
            DrawItem::Text { family, text, .. } => {
                assert_eq!(family, None, "缺 family 欄必須落回 None");
                assert_eq!(text, "舊格式");
            }
            other => panic!("應解析為 Text，實際 {other:?}"),
        }
    }

    /// family=None 時序列化必須跳過欄位（與舊版 JSON 逐位元組相同）。
    #[test]
    fn text_without_family_serializes_without_field() {
        let item = DrawItem::Text {
            x: 1.0,
            y: 2.0,
            size_pt: 12.0,
            flags: 0,
            color_argb: default_opaque_black(),
            family: None,
            text: "舊格式".to_string(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(
            !json.contains("family"),
            "family=None 不得寫入 JSON：{json}"
        );
    }
}
