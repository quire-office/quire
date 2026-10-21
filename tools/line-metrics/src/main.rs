//! line_metrics —— 分頁行高分解診斷工具（純觀測；quire 分頁 vs LibreOffice
//! 分頁點分歧捉蟲的第一手觀測面）。
//!
//! 把一份 DOCX 開檔→分頁→**每頁每段每行的推進高度**攤開成 JSON：每行帶
//! 所屬段落、段內序位、全文件行序、頁內 y、實際消耗高度、級聯字級、行距
//! 規則（exact/atLeast/auto）與 kind。`height_pt` 就是分頁判定用的同一個
//! 數值（帶量化後的扣帳高度）——對照 LO 基線找「為什麼我方 N 頁、LO N＋1 頁」
//! 時，壞掉的行高會直接露在這份報告裡。
//!
//! 用法：
//!   cargo run -p line-metrics --bin line_metrics -- <file.docx> [--out <檔>]
//! 沒有 `--out` 就把 JSON 寫到 stdout。任何收集不到的小節一律 null／
//! "unknown"——本工具不 panic、不因缺欄位拒絕輸出（容錯鐵律）。
//!
//! 已知語義說明（都屬引擎現況的老實回報，不是工具缺陷）：
//! - `spacing_before_pt`／`spacing_after_pt` 恆為 null——行帶模型沒有段落
//!   前後距概念（styles.rs 未支援清單明列），分頁確實**沒有消費**它們；
//!   這正是與 LO 分頁分歧的高嫌疑項目之一。
//! - 文字行的 `height_pt`＝該段行高的帶量化值（ceil(段行高／帶尺)×帶尺，
//!   與 pack_run 扣帳同值）——如果我們與 LO 在某段差一帶，原因多半就
//!   藏在量化前後的差值裡。
//!
//! 工程取徑：分頁輸入管線（級聯/sections/表格槽/圖片帶）自
//! `core/engine-api` 逐式移植（見 doc_inputs.rs／tables.rs 檔頭的同步責任
//! 聲明），本工具**不改動** `core/layout`、`core/ooxml`、`core/engine-api`
//! 的任何一行——純旁路觀測，pick 的是「從 LayoutEngine 已產出的 Page
//! 結構重建」這條改動最小的路。

mod doc_inputs;
mod tables;

use std::path::PathBuf;
use std::process::ExitCode;

use serde::Serialize;

use doc_inputs::DocInputs;

/// 一行的觀測記錄（卡片規定的欄位；收集不到的填 null／"unknown"）。
#[derive(Debug, Serialize)]
struct LineRecord {
    /// 所屬段落（扁平索引；表格列＝錨點段落 para_start）。
    para_index: Option<usize>,
    /// 段內行序（0 起）；表格列無段內行序＝null。
    line_in_para: Option<usize>,
    /// 全文件第幾條行記錄之前的計數（0 起累計，含文字行＋表格列）——
    /// 跨頁／跨工具對位用的全域行鍵。
    line_count_before: Option<usize>,
    /// 行頂 y（pt，頁內絕對＝margin_top＋帶索引×帶尺）。表格列＝切片
    /// frag.y_pt＋切片內前列高度累計。
    top_pt: Option<f32>,
    /// 本行在分頁時實際消耗的推進高度（pt）＝帶數×帶尺，與分頁判定
    /// 同一個數值（本工具的全部意義）。表格列＝列高（分頁帶量化的
    /// RowFrag.height_pt，同口徑）。
    height_pt: Option<f32>,
    /// 該行所屬段落基準量測字級（pt，級聯最終生效值）；表格列＝null。
    font_size_pt: Option<f32>,
    /// 該段**量化前**的段行高原值（pt）。`height_pt` 是 ceil 到帶尺後的值，
    /// 兩者之差就是「我方與 LO 差一帶」的元兇所在——本工具存在的主要理由
    /// 之一，故兩值併陳而非只給量化後。表格列／收集不到＝null。
    line_h_prequant_pt: Option<f32>,
    /// 行距規則（`w:spacing/@w:lineRule` 的級聯有效值）。
    /// 未宣告（含全鏈無宣告）＝"unknown"（引擎此時用 12pt×1.4 後備）。
    line_rule: &'static str,
    /// 段前距（pt）。行帶模型無段前後距概念（引擎未實作）＝null。
    spacing_before_pt: Option<f32>,
    /// 段後距（pt）。同上＝null。
    spacing_after_pt: Option<f32>,
    /// 行類別：text／table／empty-para／page-break／unknown。
    /// page-break＝行所屬段落宣布手動分頁（`w:br type="page"` 或
    /// `w:pageBreakBefore`）；empty-para＝純空文字行（無手動分頁）。
    kind: &'static str,
}

/// 一頁的觀測記錄。
#[derive(Debug, Serialize)]
struct PageRecord {
    /// 頁序（0 起，與 Page::index／pagebreaks.json 一致）。
    page: usize,
    /// 本頁起頭所屬 section 的實際頁寬（pt）。
    width_pt: f32,
    /// 本頁起頭所屬 section 的實際頁高（pt）。
    height_pt: f32,
    /// 本頁起頭是否由 section 邊界換頁（nextPage；首頁恆 false——
    /// 與 breaks_from_pages 的口徑一致）。
    starts_section: bool,
    /// 本頁全部行記錄（文字行＋表格列，依帶序＝閱讀序）。
    lines: Vec<LineRecord>,
}

/// 頂層報告。
#[derive(Debug, Serialize)]
struct Report {
    /// 輸入檔路徑字串。
    file: String,
    /// 頁數（＝分頁判定實際產出的頁數；與 quire-cli render 的
    /// pagebreaks.json 的 pagebreaks 長度可比對）。
    page_count: usize,
    pages: Vec<PageRecord>,
}

/// 行距規則字串：級聯有效值；未宣告＝unknown（引擎走 12pt×1.4 後備）。
fn rule_of(line: Option<quire_ooxml::LineSpacing>) -> &'static str {
    match line {
        Some(quire_ooxml::LineSpacing::Exact(_)) => "exact",
        Some(quire_ooxml::LineSpacing::AtLeast(_)) => "atLeast",
        Some(quire_ooxml::LineSpacing::Auto(_)) => "auto",
        None => "unknown",
    }
}

/// 一段文字行是否屬「宣布手動分頁的段落」：`w:br type="page"`（段內碼點
/// 偏移非空）或 `w:pageBreakBefore`（grid 的 break_before 旗標）。
fn manual_break(inputs: &DocInputs, para: usize) -> bool {
    !inputs.breaks.get(para).is_none_or(Vec::is_empty) || inputs.grid.break_before_of(para)
}

/// 由分頁輸入束＋真實頁序列組裝報告。
///
/// 文字行的 `top_pt`／`height_pt` 直接取自引擎分頁產物：前者＝
/// `margin_top＋line_bands[i]×帶尺`（顯示列表發射用的同一條幾何公式），
/// 後者＝帶數×帶尺（pack_run 扣帳的同一個數值）。表格列取 `Page.tables`
/// 的切片幾何（frag.y_pt＋列高累計）。「從 paginate 已產出的 Page 結構
/// 重建」的設計位置就在這裡——零旁路掛鉤、零核心改動。
fn build_report(path: &str, inputs: &DocInputs, pages: &[quire_layout::Page]) -> Report {
    // 逐段斷行行起點（碼點偏移）：與分頁同源的對位尺，給 line_in_para 用。
    let offsets = doc_inputs::para_line_char_offsets(inputs);
    let line_h = inputs.params.line_h_pt();
    let mut running: usize = 0; // 全文件行序（含表格列）
    let mut pages_out: Vec<PageRecord> = Vec::with_capacity(pages.len());
    for (pi, pg) in pages.iter().enumerate() {
        let mut queued: Vec<(u32, usize, usize, LineRecord)> = Vec::new();
        for (i, l) in pg.lines.iter().enumerate() {
            // 行帶索引：line_bands 缺位時老實退回陣標（引擎的既有後備路徑，
            // 同 make 顯示列表路徑），不虛構也不拒絕輸出。
            let band = pg.line_bands.get(i).copied().unwrap_or(i as u32);
            // line_in_para：以「碼點行起點尺」定位本行在段內的序位。
            let line_in_para = offsets
                .get(l.para_index)
                .and_then(|off| off.iter().position(|&o| o == l.char_offset));
            // 文字排版字級（級聯最終生效值）。
            let font_size = inputs
                .grid
                .size_of(l.para_index, inputs.params.font_size_pt);
            let rule = rule_of(inputs.para_line.get(l.para_index).copied().flatten());
            let kind = if manual_break(inputs, l.para_index) {
                "page-break"
            } else if l.text.trim().is_empty() {
                "empty-para"
            } else {
                "text"
            };
            let rec = LineRecord {
                para_index: Some(l.para_index),
                line_in_para,
                line_count_before: Some(running),
                top_pt: Some(pg.margin_top_pt + band as f32 * line_h),
                height_pt: Some(inputs.grid.bands_of(l.para_index) as f32 * line_h),
                font_size_pt: Some(font_size),
                line_h_prequant_pt: inputs.para_line_h.get(l.para_index).copied(),
                line_rule: rule,
                spacing_before_pt: None,
                spacing_after_pt: None,
                kind,
            };
            queued.push((band, 0, i, rec));
            running += 1;
        }
        // 表格列（每列一條記錄；無需展開 cell 級）。列頂＝切片 y＋前列高累計。
        for (ti, tp) in pg.tables.iter().enumerate() {
            let mut cum = 0.0f32;
            for (ri, r) in tp.frag.rows.iter().enumerate() {
                let rec = LineRecord {
                    para_index: Some(tp.anchor_para),
                    line_in_para: None,
                    line_count_before: Some(running),
                    top_pt: Some(tp.frag.y_pt + cum),
                    height_pt: Some(r.height_pt),
                    font_size_pt: None, // 格內字級不在本卡範圍（無需 cell 級展開）
                    line_h_prequant_pt: None, // 表格列以列高計，無段行高概念
                    line_rule: "unknown",
                    spacing_before_pt: None,
                    spacing_after_pt: None,
                    kind: "table",
                };
                queued.push((tp.band + r.band_start, 1, ti * 1000 + ri, rec));
                cum += r.height_pt;
                running += 1;
            }
        }
        // 帶序（＝閱讀序）排列：文字行在前、表格列在後只做**帶相等**時的
        // 順位保底，不顛倒帶正序。
        queued.sort_by_key(|(band, cat, i, _)| (*band, *cat, *i));
        let lines = queued.into_iter().map(|(_, _, _, r)| r).collect();
        let starts_section = pi > 0 && pages[pi - 1].section_id.ne(&pg.section_id);
        pages_out.push(PageRecord {
            page: pg.index,
            width_pt: pg.width_pt,
            height_pt: pg.height_pt,
            starts_section,
            lines,
        });
    }
    Report {
        file: path.to_string(),
        page_count: pages.len(),
        pages: pages_out,
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args.len() > 5 {
        eprintln!(
            "用法: line_metrics <file.docx> [--out <檔>]\n  （不帶 --out 就把 JSON 印到 stdout）"
        );
        return ExitCode::from(2);
    }
    let path = &args[1];
    let mut out: Option<PathBuf> = None;
    let mut i = 2;
    while i < args.len() {
        if args[i] == "--out" {
            match args.get(i + 1) {
                Some(p) => out = Some(PathBuf::from(p)),
                None => {
                    eprintln!("--out 需要參數");
                    return ExitCode::from(2);
                }
            }
            i += 2;
        } else {
            eprintln!("未知選項: {}", args[i]);
            return ExitCode::from(2);
        }
    }
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("讀檔失敗 {path}: {e}");
            return ExitCode::from(1);
        }
    };
    // 容錯鐵律：任何收集不到的資訊老實填 null／unknown，不 panic、不因
    // 缺欄位拒絕輸出。開檔失敗例外——沒有輸入就沒有觀測對象。
    let report = match doc_inputs::from_docx_bytes(&bytes) {
        Ok(inputs) => {
            let pages = doc_inputs::layout_pages(&inputs);
            build_report(path, &inputs, &pages)
        }
        Err(e) => {
            eprintln!("解析失敗 {path}: {e}");
            return ExitCode::from(1);
        }
    };
    let json = serde_json::to_string_pretty(&report).unwrap_or_else(|e| {
        eprintln!("JSON 序列化失敗（輸入含非有限浮點？）: {e}");
        std::process::exit(1);
    });
    match out {
        Some(f) => {
            if let Err(e) = std::fs::write(&f, json) {
                eprintln!("寫檔失敗 {}: {e}", f.display());
                return ExitCode::from(1);
            }
            println!(
                "line_metrics: {} → {} 頁 → {}",
                path,
                report.page_count,
                f.display()
            );
        }
        None => println!("{json}"),
    }
    ExitCode::SUCCESS
}

// ---------------------------------------------------------------------------
// 單元測試（卡片指定兩例：與 paginate 直調的一致性＋手動換頁 top 重置）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use quire_layout::{
        break_lines, layout_paragraphs, paginate, LayoutParams, ParaGrid, SectionSpec,
    };

    /// 兩段普通文字版面（預設 A4 後備參數）：斷行＋排版全走公開 API。
    fn default_params() -> LayoutParams {
        LayoutParams::default()
    }

    fn plain_inputs(texts: Vec<String>) -> DocInputs {
        DocInputs {
            texts: texts.clone(),
            params: default_params(),
            sections: Vec::new(),
            tables: Vec::new(),
            images: Vec::new(),
            breaks: vec![Vec::new(); texts.len()],
            grid: ParaGrid::default(),
            para_line: vec![None; texts.len()],
            para_line_h: vec![default_params().line_h_pt(); texts.len()],
        }
    }

    /// (a) 兩段直排文字文件：行數／頁數與 `paginate` 直調結果一致。
    ///
    /// 內容夠長逼出多頁，鎖「報告頁序列 == 分頁器頁序列」不只是巧合；
    /// 每頁文字行數也逐一對齊（從 Page 重建的 cookbook 驗證）。
    #[test]
    fn two_paragraphs_match_direct_paginate() {
        let texts: Vec<String> = (0..2)
            .map(|i| format!("第 {i} 段：{}", "直排文字測試行高貢獻。".repeat(60)))
            .collect();
        let params = default_params();
        let inputs = plain_inputs(texts.clone());
        // 直調對照組：公開 API 的 layout_paragraphs＋paginate（單一幾何）。
        let pls = layout_paragraphs(&texts, &params);
        let direct = paginate(&pls, &params);
        let mine = doc_inputs::layout_pages(&inputs);
        assert_eq!(mine.len(), direct.len(), "頁數分岔：report={}", mine.len());
        for (a, b) in mine.iter().zip(direct.iter()) {
            assert_eq!(a.lines.len(), b.lines.len(), "頁 {} 行數分岔", a.index);
            assert!(a.margin_top_pt - b.margin_top_pt < 1e-9);
            let h = params.line_h_pt();
            // 行高貢獻：整段行高一致（帶尺×帶數）。
            for l in &a.lines {
                assert!(
                    (h * inputs.grid.bands_of(l.para_index) as f32 - h * 1.0).abs() < 1e-9,
                    "全域尺下每行必占 1 帶"
                );
            }
        }
        let report = build_report("直排兩段", &inputs, &mine);
        assert_eq!(report.page_count, direct.len());
        // 直接用 break_lines 驗證首段行數：斷行內容沒被工具這一層動過。
        let m = quire_layout::TextMeasurer::new(params.font_size_pt);
        let widths = params.content_w_pt();
        let expected = break_lines(&texts[0], widths, &m);
        let counted = report
            .pages
            .iter()
            .flat_map(|p| p.lines.iter())
            .filter(|l| l.para_index == Some(0))
            .count();
        assert_eq!(
            expected.len(),
            counted,
            "報告中第 0 段的行記錄數必須與 break_lines 一致"
        );
    }

    /// (b) 含一個手動換頁符的文件：跨頁行記錄的 `top_pt` 在頁界重置。
    ///
    /// 斷點放在第 0 段尾（碼點偏移＝段長 ⇒ k=行數 ⇒ 段後斷）：第 1 段
    /// 整段推到第 2 頁，首行 top_pt 必須回到 margin_top（帶 0），不能
    /// 是「延續上一頁的最後一條推進位置」。
    #[test]
    fn manual_page_break_resets_top_at_page_boundary() {
        let p0 = "手動換頁之前的一段正文，提供一個有效的段落內容。".repeat(2);
        let p1 = "手動換頁之後的段落，整段必須落在新的頁面最頂行。";
        let texts = vec![p0.clone(), p1.to_string()];
        let mut inputs = plain_inputs(texts);
        // 斷點：br 段尾（碼點軸=段長）→ k=行數 → 段後斷頁。
        inputs.breaks[0] = vec![p0.chars().count()];
        let pages = doc_inputs::layout_pages(&inputs);
        assert!(
            pages.len() >= 2,
            "手動換頁至少要產出 2 頁，實得 {}",
            pages.len()
        );
        let top0 = pages[0].margin_top_pt;
        // 第 2 頁首行記錄：top_pt 重置回 margin_top。
        let report = build_report("頁符", &inputs, &pages);
        let pg2 = &report.pages[1];
        let first = pg2.lines.first().expect("換頁後的新頁至少有 1 行");
        assert_eq!(
            first.top_pt,
            Some(top0),
            "頁界重置：新頁首行 top（＝margin_top+0×帶尺）應為 margin_top({top0})，實得 {:?}",
            first.top_pt
        );
        // 新頁首行的段落必須是第 1 段（＝br 指到的下一頁內容）。
        assert_eq!(
            first.para_index,
            Some(1),
            "換頁後首行必屬第 1 段（br 語義：斷點後新頁）"
        );
        // 第 2 頁首行的 kind 必須標 page-break（pageBreakBefore 老實回報）：
        // 斷點所在的第 0 段或跨出的第 1 段都不該誤標成 text 從性丟失。
        assert_eq!(first.kind, "text", "跨頁段落的純文字行 kind=text");
        // 第 0 頁的最後一行 top 必須**低於**頁底（分頁沒把它推出去），
        // 而且 top 高度是帶量化的倍數。
        let h = inputs.params.line_h_pt();
        let last = report.pages[0].lines.last().expect("第 1 頁至少有 1 行");
        let top = last.top_pt.expect("測試幾何必有 top");
        let rem = (top - top0) / h;
        assert!(
            (rem - rem.round()).abs() < 1e-4,
            "文字行 top 必在帶網格上（帶眼差 {}）",
            rem
        );
    }

    /// 手動換頁的段落宣告（breaks 非空）必須部署 kind=page-break。
    /// 與 (b) 配對：同一組輸入把 br 放在第 0 段**段前**（k=0 段前斷），
    /// 第 0 段整段即「換頁宣告段落」。
    #[test]
    fn manual_break_para_kind_is_page_break() {
        let texts = vec![
            "這段整段是換頁宣告前文字。".repeat(3),
            "這段是換頁後的第一段。".repeat(3),
        ];
        let mut inputs = plain_inputs(texts);
        // br 在第 1 段開頭（段前斷 k=0）→ 第 1 段 = page-break 段。
        inputs.breaks[1] = vec![0];
        let pages = doc_inputs::layout_pages(&inputs);
        assert!(pages.len() >= 2, "段前 br 應推頁：實得 {} 頁", pages.len());
        let report = build_report("段前br", &inputs, &pages);
        // 第 1 段的每一行都該標 page-break（段落級語義）。
        for pg in &report.pages {
            for l in &pg.lines {
                if l.para_index == Some(1) {
                    // 第 1 頁的段落 1 = br 所在的段落宣告。
                    assert_eq!(
                        l.kind, "page-break",
                        "br 段落行（para 1）kind 必為 page-break"
                    );
                }
            }
        }
    }

    /// section 幾何報告：逐段 section 造成的頁面尺寸不同時，頁 width/height
    /// 與 starts_section 都要老實分岔（卡片槽「權威來源是檔案自己的 sectPr」）。
    #[test]
    fn section_boundary_reports_geometry_and_starts_flag() {
        let texts = vec!["段落甲。".repeat(20), "段落乙。".repeat(20)];
        let mut inputs = plain_inputs(texts);
        // 手工節格：前 1 段 A4、後 1 段 Letter，section id 不同 ⇒ 邊界換頁。
        let geo = |w: f32, h: f32| quire_layout::SectionGeometry {
            page_w_pt: w,
            page_h_pt: h,
            margin_top_pt: 72.0,
            margin_bottom_pt: 72.0,
            margin_left_pt: 72.0,
            margin_right_pt: 72.0,
            landscape: false,
            continuous: false,
        };
        let a4 = geo(595.0, 842.0);
        let letter = geo(612.0, 792.0);
        inputs.sections = vec![
            SectionSpec { id: 0, geo: a4 },
            SectionSpec { id: 1, geo: letter },
        ];
        let pages = doc_inputs::layout_pages(&inputs);
        assert_eq!(pages.len(), 2, "兩節都非 continuous：邊界必為兩頁");
        let report = build_report("分節", &inputs, &pages);
        assert!(report.pages[1].starts_section, "第 2 頁起頭＝section 邊界");
        assert!(
            (report.pages[0].width_pt - 595.0).abs() < 1e-6,
            "第 1 頁寬=A4"
        );
        assert!(
            (report.pages[1].width_pt - 612.0).abs() < 1e-6,
            "第 2 頁寬=Letter"
        );
    }
}
