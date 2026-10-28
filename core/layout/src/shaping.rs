//! 真字型度量：swash shaping + 內建 Noto Sans TC 子集（OFL 1.1）。
//!
//! 鐵律：寬度只准來自內嵌子集的 **shaped advances**（連字、標點壓縮、組合字
//! 都如實反映），禁止系統字型——同一份文件在 native/wasm 兩殼必須斷在同一個
//! 字元（分頁一致性的根，研究報告 §風險2）。子集重建：
//! `bash scripts/build_font_subset.sh`。
//!
//! script run 切分：Common/Inherited（標點、空白、數字）隨相鄰文字 script，
//! 漢語標點因此走 Han context、拉丁詞走 Latin context（fi 連字、halt 壓縮
//! 如實生效）。
//!
//! 缺字處理：子集未涵蓋的字元（.notdef 或無 cluster）以「平均字寬」估值
//! 佔位，讓斷行仍收斂；真實缺字由 golden test 與子集覆蓋率兜底，而不是
//! 讓排版在缺字時直接炸。
//!
//! ★parley 評估（Phase0-4）：已逐 API 審閱 parley 0.12 / parley_engine。
//! parley 的增值在斷行層（Layout/break_all_lines/UAX#14），屬 Phase0-5 自研
//! Knuth-Plass 的範圍；P0-4 只取 shaped advances，swash 單棧即足，且避免
//! fontique 的系統字型探索混進雙殼一致性。Phase0-5 若要接 parley 的
//! analysis 層，在本模組換 shape_advances 實作即可，斷行外殼不動。

use std::cell::RefCell;

use swash::shape::ShapeContext;
use swash::text::{Codepoint, Script};
use swash::FontRef;

/// 內建字型子集（OFL；來源與重產方式見 assets/OFL.txt、scripts/build_font_subset.sh）。
pub const EMBEDDED_FONT: &[u8] = include_bytes!("../assets/quire-notosanstc-subset.otf");

/// 內嵌子集的家族名（OFL 保留名稱合規：不使用 Noto 的 Reserved Font Name）。
pub const EMBEDDED_FONT_FAMILY: &str = "Quire Noto Sans TC";

/// 一個 text cluster 的 shaped 結果：`start..end` 為源文字元（UTF-8 位元組
/// 區間），`w` 為該 cluster 的水平 advance（pt，@ size_pt）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClusterAdvance {
    pub start: u32,
    pub end: u32,
    pub w: f32,
}

/// 內嵌子集的常駐字形參考（`FontRef` 是 `Copy`；由 from_index 固定 key 命中
/// swash 內部快取）。
fn embedded_font() -> FontRef<'static> {
    FontRef::from_index(EMBEDDED_FONT, 0)
        .expect("內建字型子集損毀（assets/quire-notosanstc-subset.otf，重產：scripts/build_font_subset.sh）")
}

thread_local! {
    /// swash 官方建議每執行緒一個 ShapeContext（內部自帶字形快取）。
    static SHAPE_CTX: RefCell<ShapeContext> = RefCell::new(ShapeContext::new());
}

/// 是否為「無所謂 script」的通用字元（標點/空白/數字/組合音標隨上下文）。
fn is_common(c: char) -> bool {
    matches!(c.properties().script(), Script::Common | Script::Inherited)
}

/// script run 切分：通用字元歸屬「當前有效 script」（初值 Latin，遇首個
/// 非通用字元切換；換 script 時，掛在舊 script 下的通用字元自當前 char 起
/// 歸新 run）。回傳 (位元組起點, 長度, shaping script)。
pub(crate) fn script_runs(text: &str) -> Vec<(usize, usize, Script)> {
    let mut runs: Vec<(usize, usize, Script)> = Vec::new();
    let mut effective = Script::Latin;
    let mut start = 0usize;
    let mut start_script = Script::Latin;
    for (i, c) in text.char_indices() {
        let eff = if is_common(c) {
            effective
        } else {
            c.properties().script()
        };
        if eff != start_script {
            if i > start {
                runs.push((start, i - start, start_script));
            }
            start = i;
            start_script = eff;
        }
        if !is_common(c) {
            effective = c.properties().script();
        }
    }
    if start < text.len() {
        runs.push((start, text.len() - start, start_script));
    }
    runs
}

/// 把一段文字 shape 成逐 cluster advances（pt@size_pt）。空串回傳空向量。
/// 輸出依 `start` 嚴格遞增且完整覆蓋每個字元的起點。
pub fn shape_advances(text: &str, size_pt: f32) -> Vec<ClusterAdvance> {
    shape_advances_scaled(text, size_pt, 1.0)
}

/// K7-cal：帶水平 advance 尺的 shaping。`scale` 只乘 **Latin/通用** cluster
/// （宣告族→LO 渲染 face 的寬比校準）；Han/Hangul 等 CJK cluster 走 em
/// 方塊、LO 替代 face 同為方塊，**不乘**（乘了會把方塊字縮出裂縫）。
/// `scale==1.0` 時與 [`shape_advances`] 逐位元組全等（舊行為守門）。
pub fn shape_advances_scaled(text: &str, size_pt: f32, scale: f32) -> Vec<ClusterAdvance> {
    let mut out = Vec::new();
    if text.is_empty() {
        return out;
    }
    let s = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let size = size_pt.max(0.001);
    let font = embedded_font();
    let m = font.metrics(&[]).scale(size);
    let avg = if m.average_width > 0.0 {
        m.average_width
    } else {
        size
    };

    for (rs, rl, script) in script_runs(text) {
        // K7-cal：CJK（Han／Hangul／Kana）cluster 不乘尺（em 方塊，LO 替代
        // face 同為方塊）；Latin/通用乘。
        let run_scale = if matches!(
            script,
            Script::Han | Script::Hangul | Script::Hiragana | Script::Katakana
        ) {
            1.0
        } else {
            s
        };
        let run_start = out.len();
        SHAPE_CTX.with(|ctx| {
            let mut ctx = ctx.borrow_mut();
            let mut shaper = ctx.builder(font).script(script).size(size).build();
            shaper.add_str(&text[rs..rs + rl]);
            shaper.shape_with(|cluster| {
                let g = cluster.glyphs;
                let w = if g.is_empty() {
                    // 控制字元（\n 等）：0 寬、不參與斷行寬度。
                    0.0
                } else if g.iter().all(|x| x.id == 0) {
                    // .notdef＝子集缺字：平均字寬估值佔位（斷行仍收斂）。
                    avg
                } else {
                    g.iter().map(|x| x.advance).sum()
                };
                out.push(ClusterAdvance {
                    start: rs as u32 + cluster.source.start,
                    end: rs as u32 + cluster.source.end,
                    w,
                });
            });
            if run_scale != 1.0 {
                // 只乘本 run 剛推入的 clusters（run_start 之後）。
                for c in out[run_start..].iter_mut() {
                    c.w *= run_scale;
                }
            }
        });
    }

    // 防禦性覆蓋校驗：cluster 必須完整鋪滿每個 char（swash 理論上保證）。
    // 缺口 char 以平均寬補位，保證「每個 char 有且僅有一個所屬 cluster」。
    let mut cov: Vec<ClusterAdvance> = Vec::with_capacity(out.len());
    let chars: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    let mut ci = 0usize; // out 游標
    let mut k = 0usize; // chars 游標
    while k < chars.len() {
        let i = chars[k];
        while ci < out.len() && (out[ci].end as usize) <= i {
            ci += 1;
        }
        if ci < out.len() && (out[ci].start as usize) == i {
            let end = out[ci].end as usize;
            cov.push(out[ci]);
            while k < chars.len() && chars[k] < end {
                k += 1;
            }
            ci += 1;
        } else if ci < out.len() && (out[ci].start as usize) < i && i < (out[ci].end as usize) {
            // 連字/組合字的後續 char：寬度已併入所屬 cluster。
            k += 1;
        } else {
            cov.push(ClusterAdvance {
                start: i as u32,
                end: chars.get(k + 1).copied().unwrap_or(text.len()) as u32,
                w: avg,
            });
            k += 1;
        }
    }
    cov
}

/// 文字測量器：字級＋水平 advance 尺（K7-cal），餵給 `break_lines`。
/// `advance_scale`＝宣告族→LO 渲染 face 的字形寬比（見 engine-api
/// `face_advance_scale`；1.0＝舊行為）。水平尺只影響 `text_width`，
/// 不動 `size_pt`（KP 取 clusters 一律經本器，caret/發射路徑各自另製）。
#[derive(Debug, Clone, Copy)]
pub struct TextMeasurer {
    size_pt: f32,
    advance_scale: f32,
}

impl TextMeasurer {
    pub fn new(size_pt: f32) -> Self {
        Self {
            size_pt,
            advance_scale: 1.0,
        }
    }

    /// K7-cal：帶水平 advance 尺的測量器（scale≡1.0 時與 [`Self::new`] 全等）。
    pub fn with_advance_scale(size_pt: f32, advance_scale: f32) -> Self {
        let s = if advance_scale.is_finite() && advance_scale > 0.0 {
            advance_scale
        } else {
            1.0
        };
        Self {
            size_pt,
            advance_scale: s,
        }
    }

    pub fn size_pt(&self) -> f32 {
        self.size_pt
    }

    /// K7-cal：水平 advance 尺（1.0＝未校準＝舊行為逐位元組不變）。
    pub fn advance_scale(&self) -> f32 {
        self.advance_scale
    }

    /// shaped 總寬（pt；K7-cal 尺按 script 選擇性乘入，與 KP 斷行同徑）。
    pub fn text_width(&self, s: &str) -> f32 {
        shape_advances_scaled(s, self.size_pt, self.advance_scale)
            .iter()
            .map(|c| c.w)
            .sum()
    }
}

/// 字級相關的基線/行高資訊（取自內嵌字型真實 metrics，供分頁/顯示列表）。
#[derive(Debug, Clone, Copy)]
pub struct FontMetrics {
    pub size_pt: f32,
    pub ascent_pt: f32,
    pub descent_pt: f32,
    pub leading_pt: f32,
}

impl FontMetrics {
    pub fn new(size_pt: f32) -> Self {
        let m = embedded_font().metrics(&[]).scale(size_pt.max(0.001));
        Self {
            size_pt,
            ascent_pt: m.ascent,
            descent_pt: m.descent,
            leading_pt: m.leading,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_runs_keep_punctuation_with_context() {
        // 漢語標點歸 Han run；拉丁詞歸 Latin run。
        let runs = script_runs("中文,abc。");
        assert_eq!(runs[0].2, Script::Han);
        assert!(runs.iter().any(|r| r.2 == Script::Latin));
    }

    #[test]
    fn cjk_advance_is_em_square() {
        // Noto Sans TC 漢字 advance = 1.0em → 12pt 字級下 12pt 寬。
        let cs = shape_advances("中", 12.0);
        assert_eq!(cs.len(), 1);
        assert!(
            (cs[0].w - 12.0).abs() < 0.01,
            "han advance 應≈1em，實得 {}",
            cs[0].w
        );
    }

    #[test]
    fn latin_advances_differ_per_glyph() {
        // 舊 SimpleFontMetrics 把所有拉丁字當半形；真度量下 i 明顯窄於 W。
        let wi = shape_advances("i", 12.0)[0].w;
        let ww = shape_advances("W", 12.0)[0].w;
        assert!(wi < ww * 0.6, "i={wi} 應明顯窄於 W={ww}");
        // 半形估值=6pt：多數拉丁字母的真寬應偏離 >5%（證明不再估值）。
        let probe = shape_advances("abcdefghijklmnopqrstuvwxyz", 12.0);
        let off = probe.iter().filter(|c| (c.w - 6.0).abs() > 0.3).count();
        assert!(off >= probe.len() / 2, "多數拉丁字寬應偏離半形估值");
    }

    #[test]
    fn latin_ligature_forms_in_latin_context() {
        // "fi"（Noto 標準 liga）：Latin script run 內應形成連字（一 cluster
        // 覆蓋兩字元）；即使此子集無 liga，也必須是兩 cluster 完整覆蓋。
        let cs = shape_advances("fi", 12.0);
        let total: f32 = cs.iter().map(|c| c.w).sum();
        assert!(total > 0.0, "fi 總寬必須 > 0");
        assert!(
            (cs.len() == 1 && cs[0].end == 2) || (cs.len() == 2 && cs[1].start == 1),
            "fi 必須完整覆蓋兩字元，實得 {cs:?}"
        );
    }

    #[test]
    fn control_chars_are_zero_width() {
        let cs = shape_advances("a\nb", 12.0);
        let nl = cs
            .iter()
            .find(|c| c.start == 1)
            .expect("換行必須有 cluster");
        assert_eq!(nl.w, 0.0);
    }

    #[test]
    fn missing_glyph_falls_back_to_average_width() {
        // 子集外字元（太極圖 U+262F 不在涵蓋區）：平均寬估值，不 panic、寬>0。
        let cs = shape_advances("\u{262F}", 12.0);
        assert_eq!(cs.len(), 1);
        assert!(cs[0].w > 0.0, "缺字應以平均寬佔位");
    }

    #[test]
    fn cluster_starts_tile_all_chars() {
        let text = "Quire 測試：mixed中文mix！😀12";
        let cs = shape_advances(text, 12.0);
        let starts: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
        let covered: Vec<usize> = starts
            .into_iter()
            .filter(|i| {
                cs.iter().any(|c| {
                    (c.start as usize) == *i || (*i > c.start as usize && *i < c.end as usize)
                })
            })
            .collect();
        let total = text.chars().count();
        assert_eq!(covered.len(), total, "每個 char 必須恰屬於一個 cluster");
    }

    #[test]
    fn width_is_linear_in_size() {
        let a = shape_advances("測試test", 12.0)
            .iter()
            .map(|c| c.w)
            .sum::<f32>();
        let b = shape_advances("測試test", 24.0)
            .iter()
            .map(|c| c.w)
            .sum::<f32>();
        assert!(
            (a * 2.0 - b).abs() < a * 0.05,
            "12pt={a} 24pt={b} 應近似線性"
        );
    }
}
