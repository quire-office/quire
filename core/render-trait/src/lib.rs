//! quire-render-trait —— 平台無關的顯示列表（display list）契約。
//!
//! 原則（研究報告 §6）：文書區必須自繪。core 只產出「幾何+文字」的顯示列表，
//! 像素怎麼來由 shells 決定：native 走 wgpu（vello 為候選），web 走 Canvas2D/WebGPU。

use serde::{Deserialize, Serialize};

pub const FLAG_BOLD: u8 = 1;
pub const FLAG_ITALIC: u8 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DrawItem {
    Text {
        x: f32,
        y: f32,
        size_pt: f32,
        flags: u8,
        text: String,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill_argb: u32,
    },
    Rule {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },
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
