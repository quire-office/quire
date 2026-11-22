//! quire-render-trait —— 平台無關的顯示列表（display list）契約。
//!
//! 原則（研究報告 §6）：文書區必須自繪。core 只產出「幾何+文字」的顯示列表，
//! 像素怎麼來由 shells 決定：native 走 wgpu（vello 為候選），web 走 Canvas2D/WebGPU。

use serde::{Deserialize, Serialize};

pub const FLAG_BOLD: u8 = 1;
pub const FLAG_ITALIC: u8 = 2;
pub const FLAG_UNDERLINE: u8 = 4;
pub const FLAG_STRIKE: u8 = 8;

/// 底線形態（ECMA-376 §17.3.2.41 `w:u/@w:val` ST_UnderlineValues 子集）。
///
/// 單一真值源（E-2）：解析層級聯（`quire_ooxml::RunStyle::underline`）與
/// 發射層（[`DrawItem::Text::underline`]）共用本型——讀取端與繪製端零口徑
/// 漂移。三態語義向 `bold`/`italic` 靠攏，由外層 `Option` 表達「未宣告」：
/// - `Option::None`＝未宣告（級聯繼承，發射端以 `FLAG_UNDERLINE` 旗標兜底）；
/// - [`Underline::Off`]＝明確關閉（`w:val="none"/"false"/"0"/"off"`），必須
///   能熄滅樣式層宣告（`toggle_attr_on` 語義銜接）；
/// - 其餘變體＝開啟＋形態。
///
/// serde 表示＝`w:val` 小寫字面（`"single"`…），`Raw` 以外部標籤形
/// `{"raw":"wavy"}` 保形——JSON 線契約逐字鎖死於 E-2 考卷。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Underline {
    /// 明確關閉（`none`／`false`／`0`／`off` 四種拼法歸一於解析層）。
    Off,
    /// `single`：單條底線（無 `@w:val` 時的 ECMA 預設）。
    Single,
    /// `double`：雙條底線。
    Double,
    /// `thick`：粗單線。
    Thick,
    /// `dotted`：點狀底線。
    Dotted,
    /// `dashed`：虛線底線（Word 相容別名 `dash` 於解析層歸一，corpus
    /// phpword 四檔實見）。
    Dashed,
    /// 未支援形態的 raw 保形通道（ECMA 全集其餘值如 `wavy`／`words`，
    /// corpus fdo61343 實見 `words`）。發射端**降級單線＋TODO，禁 panic**，
    /// 原字面保留待後續卡補繪製配方。
    Raw(String),
}

impl Underline {
    /// 是否為「有底線」（`Off` 以外皆真）。`FLAG_UNDERLINE` 旗標的判準。
    pub fn on(&self) -> bool {
        !matches!(self, Underline::Off)
    }
}

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
        /// 底線形態（E-2：`w:u/@w:val` 保形態；ECMA-376 §17.3.2.41）。
        /// `None`＝級聯未宣告（渲染端見 `FLAG_UNDERLINE` 時畫單線兜底——修訂
        /// 插入線等繪製層註解走此路），舊顯示列表 JSON 照解析且零位移。
        /// `Some(Off)` 理論上不進發射（旗標由 `Underline::on()` 點亮），渲染端
        /// 遇 `Some(Off)` 一律不畫。未支援 `Raw(_)` 降級單線（禁 panic）。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        underline: Option<Underline>,
        text: String,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill_argb: u32,
    },
    /// 內嵌圖片像素（K4c-image）：docx `word/media/*` 真像素直出，
    /// 取代同位的灰底佔位框（查無像素／不可渲染格式由發射層退佔位框）。
    /// 新欄位全 `#[serde(default)]`——舊顯示列表 JSON 照解析。
    /// z 序約定：與填色／框線同組（`sort_display_items` 的非文字臂）——
    /// 保持推入序、恆在文字項之前，與表格切片同一約定的延伸（PM 2026-10-24：
    /// 圖在填充之後、文字之前，以現行 push 序為準）。
    Image {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        /// MIME 型別（由媒體副檔名判定，如 `image/png`）。
        #[serde(default)]
        mime: String,
        /// 像素的 base64 編碼（SVG 端 `href="data:<mime>;base64,<data_b64>"`）。
        /// 取捨：編碼放顯示列表而非渲染端——顯示列表要能跨進程（wasm 邊界）
        /// 自帶像素，渲染端零 IO。
        #[serde(default)]
        data_b64: String,
        /// 替代文字（`wp:docPr/@descr` 等；本卡不讀述欄＝None，留給後續卡）。
        #[serde(default)]
        alt: Option<String>,
        /// K19：像素級雙色調宣告（`[color1, color2]`，0xAARRGGBB）。發射層的
        /// `data_b64` **已是變換後像素**（`quire_ooxml::duotone_png_bytes`），
        /// 本欄是宣告的診斷透傳（渲染端可據此做等效濾鏡；SVG 端不需再變換）。
        /// serde default＋None 不外露＝無 duotone 檔案的顯示列表 JSON 零位移。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duotone: Option<[u32; 2]>,
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
            // E-2：底線形態欄（None＝未宣告，serde 跳欄；本測鎖 family 欄）。
            underline: None,
            text: "舊格式".to_string(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(
            !json.contains("family"),
            "family=None 不得寫入 JSON：{json}"
        );
    }
}
