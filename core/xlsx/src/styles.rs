//! XLSX 樣式鏈唯讀投影模型（X2 卡）——styles.xml 解析＋EffectiveStyle 級聯＋數字格式與日期序列。
//!
//! 遵循設計書 `docs/xlsx-styles-design.md` 規範與 quire 核心「無損紀律」、「病態容錯」原則：
//! - 純讀取投影，不提供寫回或修改介面；
//! - 不 panic、不猜測、不虛構；
//! - 純 std ＋ quick-xml，無跨平台相依，支援 wasm32 編譯。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::model::attr_val;

/// styles part 路徑（ECMA-376 預設命名慣例）。
pub const STYLES_PART: &str = "xl/styles.xml";

// ---------------------------------------------------------------- 色彩模型

/// 色彩表示模型（設計書 §2.2；支援 RGB、主題色彩、索引色）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorModel {
    /// 0xAARRGGBB 十六進位色彩值。
    Rgb(u32),
    /// 0-based 主題調色盤索引。
    Theme(u32),
    /// 試算表傳統 64 色調色盤索引。
    Indexed(u32),
    /// 自動顏色。
    Auto,
}

// ---------------------------------------------------------------- 原子樣式模型

/// 字體樣式模型（設計書 §2.2；ECMA-376 Part 1 §18.8.22）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FontModel {
    pub name: Option<String>,
    pub size_pt: Option<f64>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: Option<ColorModel>,
}

/// 填滿樣式模型（設計書 §2.2；ECMA-376 Part 1 §18.8.20）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FillModel {
    /// 圖案類型，如 "none", "gray125", "solid" 等。
    pub pattern_type: String,
    pub fg_color: Option<ColorModel>,
    pub bg_color: Option<ColorModel>,
}

/// 邊框線型與色彩模型（設計書 §2.2；ECMA-376 Part 1 §18.8.4）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BorderLine {
    /// 線條樣式，如 "thin", "medium", "dashed", "double" 等。
    pub style: Option<String>,
    pub color: Option<ColorModel>,
}

/// 單元邊框模型（設計書 §2.2；ECMA-376 Part 1 §18.8.5）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BorderModel {
    pub left: BorderLine,
    pub right: BorderLine,
    pub top: BorderLine,
    pub bottom: BorderLine,
    pub diagonal: BorderLine,
}

/// 對齊方式模型（設計書 §2.2；ECMA-376 Part 1 §18.8.1）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlignmentModel {
    /// 水平對齊，如 "left", "center", "right", "fill" 等。
    pub horizontal: Option<String>,
    /// 垂直對齊，如 "top", "center", "bottom" 等。
    pub vertical: Option<String>,
    /// 自動換行開關。
    pub wrap_text: bool,
}

// ---------------------------------------------------------------- 格式記錄與具名樣式

/// 具名樣式宣告（設計書 §2.2；ECMA-376 Part 1 §18.8.10）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellStyleModel {
    pub name: String,
    pub xf_id: u32,
    pub builtin_id: Option<u32>,
}

/// 格式記錄項（設計書 §2.2；ECMA-376 Part 1 §18.8.45）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct XfRecord {
    pub num_fmt_id: u32,
    pub font_id: u32,
    pub fill_id: u32,
    pub border_id: u32,
    pub xf_id: Option<u32>,
    pub apply_number_format: Option<bool>,
    pub apply_font: Option<bool>,
    pub apply_fill: Option<bool>,
    pub apply_border: Option<bool>,
    pub apply_alignment: Option<bool>,
    pub apply_protection: Option<bool>,
    pub alignment: Option<AlignmentModel>,
}

// ---------------------------------------------------------------- 樣式表總模型

/// 完整的 xl/styles.xml 唯讀投影模型（設計書 §2.2）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StyleSheetModel {
    /// 自訂與覆寫數字格式代碼字典：numFmtId -> formatCode。
    pub num_fmts: BTreeMap<u32, String>,
    /// 字體清單（0-based 索引對齊 fontId）。
    pub fonts: Vec<FontModel>,
    /// 填滿清單（0-based 索引對齊 fillId）。
    pub fills: Vec<FillModel>,
    /// 邊框清單（0-based 索引對齊 borderId）。
    pub borders: Vec<BorderModel>,
    /// 基準格式記錄清單（0-based 索引對齊 xfId）。
    pub cell_style_xfs: Vec<XfRecord>,
    /// 儲存格套用格式記錄清單（0-based 索引對齊儲存格 c/@s）。
    pub cell_xfs: Vec<XfRecord>,
    /// 具名樣式清單。
    pub cell_styles: Vec<CellStyleModel>,
}

// ---------------------------------------------------------------- 數字格式分類與有效樣式

/// 數字格式語義類別（設計書 §2.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumFmtCategory {
    General,
    Number,
    Currency,
    Accounting,
    Date,
    Time,
    DateTime,
    Percentage,
    Fraction,
    Scientific,
    Text,
    Unknown,
}

/// 有效數字格式描述（設計書 §2.4）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveNumFmt {
    pub id: u32,
    pub format_code: String,
    pub category: NumFmtCategory,
}

/// 樣式解析狀態與異常誠實標記（設計書 §2.3 與 §4）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleResolutionStatus {
    /// 儲存格 s 屬性超出 cellXfs 範圍。
    pub cell_xf_out_of_bounds: bool,
    /// cellXf 的 xfId 基準記錄超出 cellStyleXfs 範圍。
    pub base_style_out_of_bounds: bool,
    /// 數字格式 ID 未在自訂表或內建表定義。
    pub unknown_num_fmt: bool,
    /// 異常描述（供診斷輸出，不 panic）。
    pub warnings: Vec<String>,
}

/// 儲存格計算後的最終有效樣式（設計書 §2.3）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectiveStyle {
    /// 原始儲存格採用的 cellXfs 索引。
    pub s_index: u32,
    /// 有效數字格式。
    pub num_fmt: EffectiveNumFmt,
    /// 有效字體（若解析失敗或為空則為 None）。
    pub font: Option<FontModel>,
    /// 有效填滿。
    pub fill: Option<FillModel>,
    /// 有效邊框。
    pub border: Option<BorderModel>,
    /// 有效對齊設定。
    pub alignment: Option<AlignmentModel>,
    /// 解析狀態診斷（誠實標記任何異常與退回行為）。
    pub status: StyleResolutionStatus,
}

// ---------------------------------------------------------------- 日期序列模型

/// 日期序列值轉換錯誤列舉（設計書 §3.4）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DateSerialError {
    /// 負數序列值（Excel 顯示為 ######，不支援負數日期）。
    NegativeSerial(f64),
    /// 1900 系統專屬之 Lotus 1-2-3 幻影閏日 (1900-02-29)。
    PhantomLeapDay1900,
    /// 序號過大超出公曆上限。
    Overflow(f64),
}

/// 序列值解析結果（設計書 §3.4）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParsedDateTime {
    /// 完整公曆日期與時間。
    DateTime {
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
        microsecond: u32,
    },
    /// 純時間（序號介於 0.0 至 1.0 之間，且整數日為 0）。
    TimeOnly {
        hour: u32,
        minute: u32,
        second: u32,
        microsecond: u32,
    },
    /// 幻影閏日（1900-02-29 帶時刻）。
    Phantom1900Leap {
        hour: u32,
        minute: u32,
        second: u32,
        microsecond: u32,
    },
}

// ---------------------------------------------------------------- 內建格式表與分類引擎

/// ECMA-376 內建數字格式靜態對照表（設計書 §2.5；0..163）。
pub fn lookup_builtin_num_fmt(id: u32) -> Option<&'static str> {
    match id {
        0 => Some("General"),
        1 => Some("0"),
        2 => Some("0.00"),
        3 => Some("#,##0"),
        4 => Some("#,##0.00"),
        5 => Some("$#,##0;($#,##0)"),
        6 => Some("$#,##0;[Red]($#,##0)"),
        7 => Some("$#,##0.00;($#,##0.00)"),
        8 => Some("$#,##0.00;[Red]($#,##0.00)"),
        9 => Some("0%"),
        10 => Some("0.00%"),
        11 => Some("0.00E+00"),
        12 => Some("# ?/?"),
        13 => Some("# ??/??"),
        14 => Some("mm-dd-yy"),
        15 => Some("d-mmm-yy"),
        16 => Some("d-mmm"),
        17 => Some("mmm-yy"),
        18 => Some("h:mm AM/PM"),
        19 => Some("h:mm:ss AM/PM"),
        20 => Some("h:mm"),
        21 => Some("h:mm:ss"),
        22 => Some("m/d/yy h:mm"),
        37 => Some("#,##0 ;(#,##0)"),
        38 => Some("#,##0 ;[Red](#,##0)"),
        39 => Some("#,##0.00;(#,##0.00)"),
        40 => Some("#,##0.00;[Red](#,##0.00)"),
        45 => Some("mm:ss"),
        46 => Some("[h]:mm:ss"),
        47 => Some("mmss.0"),
        48 => Some("##0.0E+0"),
        49 => Some("@"),
        _ => None,
    }
}

/// 數字格式類別判定規則（設計書 §2.4；按優先序逐條核對 formatCode）。
pub fn classify_num_fmt(id: u32, format_code: &str) -> NumFmtCategory {
    // 設計書 §2.4 步驟 1：未定義／保留段
    if format_code.is_empty() {
        return NumFmtCategory::Unknown;
    }

    // 設計書 §2.4 步驟 2：常規格式
    if format_code.eq_ignore_ascii_case("General") {
        return NumFmtCategory::General;
    }

    // 設計書 §2.4 步驟 3：純文字直通
    if format_code == "@" {
        return NumFmtCategory::Text;
    }

    // 預處理：剔除引號字串、轉義字元與非時間中括號指示（設計書 §2.4 步驟 4）
    let mut stripped = String::with_capacity(format_code.len());
    let chars: Vec<char> = format_code.chars().collect();
    let mut i = 0;
    let mut has_duration_bracket = false;

    while i < chars.len() {
        match chars[i] {
            '"' => {
                // 剔除引號內的常數字串（如 "yyyy年"、"$" 等）
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    i += 1;
                }
                if i < chars.len() {
                    i += 1; // 跳過閉合引號
                }
            }
            '\\' => {
                // 跳過轉義字元及其後綴字元
                i += 2;
            }
            '[' => {
                // 檢查中括號內容（色彩指示如 [Red]、條件如 [>100]、或累計時間如 [h]）
                let start = i + 1;
                let mut end = start;
                while end < chars.len() && chars[end] != ']' {
                    end += 1;
                }
                if end < chars.len() {
                    let content: String = chars[start..end].iter().collect();
                    let trimmed = content.trim().to_ascii_lowercase();
                    // [h], [hh], [m], [mm], [s], [ss] 是累計時間標記，不能剔除
                    if trimmed == "h"
                        || trimmed == "hh"
                        || trimmed == "m"
                        || trimmed == "mm"
                        || trimmed == "s"
                        || trimmed == "ss"
                    {
                        has_duration_bracket = true;
                        stripped.push_str(&trimmed);
                    }
                    i = end + 1;
                } else {
                    i += 1;
                }
            }
            c => {
                stripped.push(c);
                i += 1;
            }
        }
    }

    let lower = stripped.to_ascii_lowercase();

    // 檢查日期標記與時間標記（設計書 §2.4 步驟 4）
    let has_date_token = lower.contains('y') || lower.contains('d');
    let has_time_token = lower.contains('h')
        || lower.contains('s')
        || has_duration_bracket
        || lower.contains("am/pm")
        || lower.contains("a/p");

    let has_m = lower.contains('m');

    let (is_date, is_time) = if has_date_token && has_time_token {
        (true, true)
    } else if has_date_token {
        (true, false)
    } else if has_time_token {
        (false, true)
    } else if has_m {
        // 僅有 m 時，試算表預設視為日期月份標記
        (true, false)
    } else {
        (false, false)
    };

    if is_date && is_time {
        return NumFmtCategory::DateTime;
    }
    if is_date {
        return NumFmtCategory::Date;
    }
    if is_time {
        return NumFmtCategory::Time;
    }

    // 設計書 §2.4 步驟 5：百分比
    if lower.contains('%') {
        return NumFmtCategory::Percentage;
    }

    // 設計書 §2.4 步驟 6：科學記號
    if lower.contains("e+") || lower.contains("e-") {
        return NumFmtCategory::Scientific;
    }

    // 設計書 §2.4 步驟 7：分數
    if lower.contains("?/?") || lower.contains("#/#") || lower.contains("0/") {
        return NumFmtCategory::Fraction;
    }

    // 內建貨幣與會計 ID 判斷
    if (5..=8).contains(&id)
        || lower.contains('$')
        || lower.contains('¥')
        || lower.contains('€')
        || lower.contains('£')
    {
        return NumFmtCategory::Currency;
    }

    // 設計書 §2.4 步驟 8：一般數值
    if lower.contains('0') || lower.contains('#') {
        return NumFmtCategory::Number;
    }

    NumFmtCategory::General
}

// ---------------------------------------------------------------- 日期序列演算法

/// 判定公曆閏年（設計書 §3.2）。
fn is_leap_year(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

/// 取得指定月份之公曆日數（設計書 §3.2）。
fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(y) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// 自指定年月日推進指定公曆天數（設計書 §3.2 與 §3.3）。
fn add_days_to_civil(mut y: i32, mut m: u32, mut d: u32, mut days: i64) -> (i32, u32, u32) {
    // 先按年為單位加速推進
    while days >= 366 {
        let diy = if is_leap_year(y) { 366 } else { 365 };
        if days >= diy {
            days -= diy;
            y += 1;
        } else {
            break;
        }
    }
    // 再按月為單位精確推進
    while days > 0 {
        let dim = days_in_month(y, m) as i64;
        let left_in_month = dim - (d as i64) + 1;
        if days >= left_in_month {
            days -= left_in_month;
            d = 1;
            m += 1;
            if m > 12 {
                m = 1;
                y += 1;
            }
        } else {
            d += days as u32;
            days = 0;
        }
    }
    (y, m, d)
}

/// 序列值解析為日期時間結構（設計書 §3.1 - §3.4）。
pub fn parse_date_serial(serial: f64, date1904: bool) -> Result<ParsedDateTime, DateSerialError> {
    // 設計書 §3.4 邊界值：負數序列值
    if serial < 0.0 {
        return Err(DateSerialError::NegativeSerial(serial));
    }

    // 序列值過大防禦
    if serial > 2_958_465.0 {
        return Err(DateSerialError::Overflow(serial));
    }

    let mut days = serial.floor() as i64;
    let time_fraction = serial - serial.floor();

    // 微秒精度時刻換算（設計書 §3.1）
    let mut total_microseconds = (time_fraction * 86_400.0 * 1_000_000.0).round() as u64;
    if total_microseconds >= 86_400_000_000 {
        days += 1;
        total_microseconds -= 86_400_000_000;
    }

    let total_seconds = total_microseconds / 1_000_000;
    let microsecond = (total_microseconds % 1_000_000) as u32;
    let second = (total_seconds % 60) as u32;
    let minute = ((total_seconds / 60) % 60) as u32;
    let hour = (total_seconds / 3600) as u32;

    if date1904 {
        // 1904 日期系統（設計書 §3.3）：序號 0 對應 1904-01-01
        let (year, month, day) = add_days_to_civil(1904, 1, 1, days);
        Ok(ParsedDateTime::DateTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
            microsecond,
        })
    } else {
        // 1900 日期系統（設計書 §3.2）
        if days == 0 {
            // 設計書 §3.2 邊界 0：純時間值
            Ok(ParsedDateTime::TimeOnly {
                hour,
                minute,
                second,
                microsecond,
            })
        } else if days < 60 {
            // 序號 1 對應 1900-01-01（推進 days - 1 天）
            let (year, month, day) = add_days_to_civil(1900, 1, 1, days - 1);
            Ok(ParsedDateTime::DateTime {
                year,
                month,
                day,
                hour,
                minute,
                second,
                microsecond,
            })
        } else if days == 60 {
            // 設計書 §3.2 與 §3.5：Lotus 1-2-3 幻影閏日 (1900-02-29)，誠實回報
            Err(DateSerialError::PhantomLeapDay1900)
        } else {
            // days >= 61：扣除虛構閏日（推進 days - 2 天）
            let (year, month, day) = add_days_to_civil(1900, 1, 1, days - 2);
            Ok(ParsedDateTime::DateTime {
                year,
                month,
                day,
                hour,
                minute,
                second,
                microsecond,
            })
        }
    }
}

// ---------------------------------------------------------------- 級聯與有效樣式解析

impl StyleSheetModel {
    /// 依數字格式 ID 解析有效數字格式條目（設計書 §1.2 與 §4.3）。
    pub fn resolve_effective_num_fmt(&self, num_fmt_id: u32) -> EffectiveNumFmt {
        // 自訂與覆寫優先（設計書 §1.2 檔案宣告勝出）
        if let Some(code) = self.num_fmts.get(&num_fmt_id) {
            let category = classify_num_fmt(num_fmt_id, code);
            return EffectiveNumFmt {
                id: num_fmt_id,
                format_code: code.clone(),
                category,
            };
        }

        // 退回查詢內建固定表格（設計書 §2.5）
        if let Some(builtin) = lookup_builtin_num_fmt(num_fmt_id) {
            let category = classify_num_fmt(num_fmt_id, builtin);
            return EffectiveNumFmt {
                id: num_fmt_id,
                format_code: builtin.to_string(),
                category,
            };
        }

        // 兩者皆無：誠實標記為 Unknown（設計書 §4.3 情境 A 與 B）
        EffectiveNumFmt {
            id: num_fmt_id,
            format_code: String::new(),
            category: NumFmtCategory::Unknown,
        }
    }

    /// 便利方法：直接依數字格式 ID 查詢有效數字格式（測試考卷使用）。
    pub fn resolve_effective_style_for_num_fmt(&self, num_fmt_id: u32) -> EffectiveNumFmt {
        self.resolve_effective_num_fmt(num_fmt_id)
    }

    /// 依儲存格 s 屬性解析計算最終有效樣式（設計書 §2.3 級聯演算法）。
    pub fn resolve_effective_style(&self, s_opt: Option<u32>) -> EffectiveStyle {
        let s = s_opt.unwrap_or(0);
        let mut status = StyleResolutionStatus::default();

        // 步驟 1：取出 cellXf（設計書 §2.3 與 §4.1）
        let cell_xf = match self.cell_xfs.get(s as usize) {
            Some(xf) => xf,
            None => {
                status.cell_xf_out_of_bounds = true;
                status.warnings.push(format!(
                    "c/@s={} 超出 cellXfs 範圍（長度 {}）",
                    s,
                    self.cell_xfs.len()
                ));
                self.cell_xfs.first().unwrap_or(&DEFAULT_XF)
            }
        };

        // 步驟 2：取出基準 base_xf (cellStyleXf)（設計書 §2.3 與 §4.2）
        let base_xf = match cell_xf.xf_id {
            Some(id) => match self.cell_style_xfs.get(id as usize) {
                Some(b) => Some(b),
                None => {
                    status.base_style_out_of_bounds = true;
                    status
                        .warnings
                        .push(format!("xfId={} 超出 cellStyleXfs 範圍", id));
                    None
                }
            },
            // 設計書 §4.2 情境 A：缺失 xfId 自動退回第 0 筆（若存在），不發警告
            None => self.cell_style_xfs.first(),
        };

        // 步驟 3：逐維度依據 apply 開關決定有效 ID（設計書 §1.5 與 §4.4）
        let eff_num_fmt_id = resolve_dim(
            cell_xf.num_fmt_id,
            base_xf.map(|b| b.num_fmt_id),
            cell_xf.apply_number_format,
        );
        let eff_font_id = resolve_dim(
            cell_xf.font_id,
            base_xf.map(|b| b.font_id),
            cell_xf.apply_font,
        );
        let eff_fill_id = resolve_dim(
            cell_xf.fill_id,
            base_xf.map(|b| b.fill_id),
            cell_xf.apply_fill,
        );
        let eff_border_id = resolve_dim(
            cell_xf.border_id,
            base_xf.map(|b| b.border_id),
            cell_xf.apply_border,
        );

        let eff_alignment = match cell_xf.apply_alignment {
            Some(true) => cell_xf.alignment.clone(),
            Some(false) => base_xf
                .and_then(|b| b.alignment.clone())
                .or_else(|| cell_xf.alignment.clone()),
            None => cell_xf
                .alignment
                .clone()
                .or_else(|| base_xf.and_then(|b| b.alignment.clone())),
        };

        // 步驟 4：解析實體原子模型
        let font = self.fonts.get(eff_font_id as usize).cloned();
        let fill = self.fills.get(eff_fill_id as usize).cloned();
        let border = self.borders.get(eff_border_id as usize).cloned();

        let num_fmt = self.resolve_effective_num_fmt(eff_num_fmt_id);
        if num_fmt.category == NumFmtCategory::Unknown && eff_num_fmt_id != 0 {
            status.unknown_num_fmt = true;
            status
                .warnings
                .push(format!("未定義數字格式 ID: {}", eff_num_fmt_id));
        }

        EffectiveStyle {
            s_index: s,
            num_fmt,
            font,
            fill,
            border,
            alignment: eff_alignment,
            status,
        }
    }
}

static DEFAULT_XF: XfRecord = XfRecord {
    num_fmt_id: 0,
    font_id: 0,
    fill_id: 0,
    border_id: 0,
    xf_id: None,
    apply_number_format: None,
    apply_font: None,
    apply_fill: None,
    apply_border: None,
    apply_alignment: None,
    apply_protection: None,
    alignment: None,
};

/// 依據 apply 開關合併維度 ID（設計書 §1.5 與 §4.4）。
fn resolve_dim(cell_val: u32, base_val: Option<u32>, apply: Option<bool>) -> u32 {
    match apply {
        Some(true) => cell_val,
        Some(false) => base_val.unwrap_or(cell_val),
        None => {
            // 設計書 §4.4 情境 A：若屬性缺失且 cell 自訂非 0，視為套用；否則繼承基準
            if cell_val != 0 {
                cell_val
            } else {
                base_val.unwrap_or(cell_val)
            }
        }
    }
}

// ---------------------------------------------------------------- styles.xml 解析器

/// 解析布林值屬性（設計書 §4.4 情境 B；支援 "true"/"1" 為 true，"false"/"0" 為 false）。
fn parse_bool_opt(v: Option<&str>) -> Option<bool> {
    match v.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("true") | Some("1") => Some(true),
        Some("false") | Some("0") => Some(false),
        _ => None,
    }
}

/// 解析色彩元素（設計書 §2.2 ColorModel）。
fn parse_color(e: &quick_xml::events::BytesStart) -> Option<ColorModel> {
    if let Some(rgb_str) = attr_val(e, b"rgb") {
        let s = rgb_str.trim();
        if let Ok(val) = u32::from_str_radix(s, 16) {
            let argb = if s.len() == 6 { 0xFF000000 | val } else { val };
            return Some(ColorModel::Rgb(argb));
        }
    }
    if let Some(theme_str) = attr_val(e, b"theme") {
        if let Ok(theme) = theme_str.trim().parse::<u32>() {
            return Some(ColorModel::Theme(theme));
        }
    }
    if let Some(indexed_str) = attr_val(e, b"indexed") {
        if let Ok(idx) = indexed_str.trim().parse::<u32>() {
            return Some(ColorModel::Indexed(idx));
        }
    }
    if let Some(auto_str) = attr_val(e, b"auto") {
        if parse_bool_opt(Some(&auto_str)) == Some(true) {
            return Some(ColorModel::Auto);
        }
    }
    None
}

/// 解析邊框線段（設計書 §2.2 BorderLine）。
fn parse_border_line(e: &quick_xml::events::BytesStart) -> BorderLine {
    BorderLine {
        style: attr_val(e, b"style"),
        color: None,
    }
}

/// 解析對齊模型（設計書 §2.2 AlignmentModel）。
fn parse_alignment(e: &quick_xml::events::BytesStart) -> AlignmentModel {
    AlignmentModel {
        horizontal: attr_val(e, b"horizontal"),
        vertical: attr_val(e, b"vertical"),
        wrap_text: parse_bool_opt(attr_val(e, b"wrapText").as_deref()) == Some(true),
    }
}

/// 解析 xf 格式記錄（設計書 §2.2 XfRecord）。
fn parse_xf_attrs(e: &quick_xml::events::BytesStart) -> XfRecord {
    XfRecord {
        num_fmt_id: attr_val(e, b"numFmtId")
            .as_deref()
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(0),
        font_id: attr_val(e, b"fontId")
            .as_deref()
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(0),
        fill_id: attr_val(e, b"fillId")
            .as_deref()
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(0),
        border_id: attr_val(e, b"borderId")
            .as_deref()
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(0),
        xf_id: attr_val(e, b"xfId")
            .as_deref()
            .and_then(|v| v.trim().parse::<u32>().ok()),
        apply_number_format: parse_bool_opt(attr_val(e, b"applyNumberFormat").as_deref()),
        apply_font: parse_bool_opt(attr_val(e, b"applyFont").as_deref()),
        apply_fill: parse_bool_opt(attr_val(e, b"applyFill").as_deref()),
        apply_border: parse_bool_opt(attr_val(e, b"applyBorder").as_deref()),
        apply_alignment: parse_bool_opt(attr_val(e, b"applyAlignment").as_deref()),
        apply_protection: parse_bool_opt(attr_val(e, b"applyProtection").as_deref()),
        alignment: None,
    }
}

/// 解析 xl/styles.xml 內容（設計書 §2.2 樣式表解析型錄）。
pub fn read_styles(xml: &[u8]) -> StyleSheetModel {
    let mut model = StyleSheetModel::default();
    if xml.is_empty() {
        return model;
    }

    let text = String::from_utf8_lossy(xml);
    let mut reader = quick_xml::Reader::from_str(&text);

    let mut stack: Vec<String> = Vec::new();

    let mut cur_font: Option<FontModel> = None;
    let mut cur_fill: Option<FillModel> = None;
    let mut cur_border: Option<BorderModel> = None;
    let mut cur_border_segment: Option<String> = None;
    let mut cur_cell_style_xf: Option<XfRecord> = None;
    let mut cur_cell_xf: Option<XfRecord> = None;

    while let Ok(ev) = reader.read_event() {
        match ev {
            quick_xml::events::Event::Eof => break,
            quick_xml::events::Event::Start(e) => {
                let tag = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                stack.push(tag.clone());

                match tag.as_str() {
                    "font"
                        if stack.get(stack.len().saturating_sub(2)).map(|s| s.as_str())
                            == Some("fonts") =>
                    {
                        cur_font = Some(FontModel::default());
                    }
                    "fill"
                        if stack.get(stack.len().saturating_sub(2)).map(|s| s.as_str())
                            == Some("fills") =>
                    {
                        cur_fill = Some(FillModel::default());
                    }
                    "border"
                        if stack.get(stack.len().saturating_sub(2)).map(|s| s.as_str())
                            == Some("borders") =>
                    {
                        cur_border = Some(BorderModel::default());
                    }
                    "left" | "right" | "top" | "bottom" | "diagonal" if cur_border.is_some() => {
                        cur_border_segment = Some(tag.clone());
                        let line = parse_border_line(&e);
                        let b = cur_border.as_mut().expect("cur_border");
                        match tag.as_str() {
                            "left" => b.left = line,
                            "right" => b.right = line,
                            "top" => b.top = line,
                            "bottom" => b.bottom = line,
                            "diagonal" => b.diagonal = line,
                            _ => {}
                        }
                    }
                    "xf" => {
                        let parent = stack.get(stack.len().saturating_sub(2)).map(|s| s.as_str());
                        if parent == Some("cellStyleXfs") {
                            cur_cell_style_xf = Some(parse_xf_attrs(&e));
                        } else if parent == Some("cellXfs") {
                            cur_cell_xf = Some(parse_xf_attrs(&e));
                        }
                    }
                    "patternFill" if cur_fill.is_some() => {
                        let pt = attr_val(&e, b"patternType").unwrap_or_else(|| "none".to_string());
                        cur_fill.as_mut().expect("cur_fill").pattern_type = pt;
                    }
                    "gradientFill" if cur_fill.is_some() => {
                        cur_fill.as_mut().expect("cur_fill").pattern_type = "gradient".to_string();
                    }
                    _ => {}
                }
            }
            quick_xml::events::Event::Empty(e) => {
                let tag = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                let parent = stack.last().map(|s| s.as_str());

                match tag.as_str() {
                    // 自訂數字格式 <numFmt numFmtId="..." formatCode="..."/>
                    "numFmt" if parent == Some("numFmts") => {
                        if let (Some(id_str), Some(code)) =
                            (attr_val(&e, b"numFmtId"), attr_val(&e, b"formatCode"))
                        {
                            if let Ok(id) = id_str.trim().parse::<u32>() {
                                model.num_fmts.insert(id, code);
                            }
                        }
                    }
                    // 具名樣式 <cellStyle name="..." xfId="..." builtinId="..."/>
                    "cellStyle" if parent == Some("cellStyles") => {
                        if let (Some(name), Some(xf_id_str)) =
                            (attr_val(&e, b"name"), attr_val(&e, b"xfId"))
                        {
                            if let Ok(xf_id) = xf_id_str.trim().parse::<u32>() {
                                let builtin_id = attr_val(&e, b"builtinId")
                                    .as_deref()
                                    .and_then(|v| v.trim().parse::<u32>().ok());
                                model.cell_styles.push(CellStyleModel {
                                    name,
                                    xf_id,
                                    builtin_id,
                                });
                            }
                        }
                    }
                    // 自閉合 <xf/>
                    "xf" => {
                        if parent == Some("cellStyleXfs") {
                            model.cell_style_xfs.push(parse_xf_attrs(&e));
                        } else if parent == Some("cellXfs") {
                            model.cell_xfs.push(parse_xf_attrs(&e));
                        }
                    }
                    // 字體子元素
                    "name" if cur_font.is_some() => {
                        cur_font.as_mut().expect("cur_font").name = attr_val(&e, b"val");
                    }
                    "sz" if cur_font.is_some() => {
                        cur_font.as_mut().expect("cur_font").size_pt = attr_val(&e, b"val")
                            .as_deref()
                            .and_then(|v| v.trim().parse::<f64>().ok());
                    }
                    "b" if cur_font.is_some() => {
                        let val = attr_val(&e, b"val");
                        let is_bold = !matches!(val.as_deref(), Some("false") | Some("0"));
                        cur_font.as_mut().expect("cur_font").bold = is_bold;
                    }
                    "i" if cur_font.is_some() => {
                        let val = attr_val(&e, b"val");
                        let is_italic = !matches!(val.as_deref(), Some("false") | Some("0"));
                        cur_font.as_mut().expect("cur_font").italic = is_italic;
                    }
                    "u" if cur_font.is_some() => {
                        let val = attr_val(&e, b"val");
                        let is_u =
                            !matches!(val.as_deref(), Some("none") | Some("false") | Some("0"));
                        cur_font.as_mut().expect("cur_font").underline = is_u;
                    }
                    "strike" if cur_font.is_some() => {
                        let val = attr_val(&e, b"val");
                        let is_strike = !matches!(val.as_deref(), Some("false") | Some("0"));
                        cur_font.as_mut().expect("cur_font").strike = is_strike;
                    }
                    "color" => {
                        let col = parse_color(&e);
                        if let (Some(seg), Some(b)) =
                            (cur_border_segment.as_deref(), cur_border.as_mut())
                        {
                            match seg {
                                "left" => b.left.color = col,
                                "right" => b.right.color = col,
                                "top" => b.top.color = col,
                                "bottom" => b.bottom.color = col,
                                "diagonal" => b.diagonal.color = col,
                                _ => {}
                            }
                        } else if let Some(font) = cur_font.as_mut() {
                            font.color = col;
                        }
                    }
                    "fgColor" if cur_fill.is_some() => {
                        cur_fill.as_mut().expect("cur_fill").fg_color = parse_color(&e);
                    }
                    "bgColor" if cur_fill.is_some() => {
                        cur_fill.as_mut().expect("cur_fill").bg_color = parse_color(&e);
                    }
                    "patternFill" if cur_fill.is_some() => {
                        let pt = attr_val(&e, b"patternType").unwrap_or_else(|| "none".to_string());
                        cur_fill.as_mut().expect("cur_fill").pattern_type = pt;
                    }
                    "left" | "right" | "top" | "bottom" | "diagonal" if cur_border.is_some() => {
                        let line = parse_border_line(&e);
                        let b = cur_border.as_mut().expect("cur_border");
                        match tag.as_str() {
                            "left" => b.left = line,
                            "right" => b.right = line,
                            "top" => b.top = line,
                            "bottom" => b.bottom = line,
                            "diagonal" => b.diagonal = line,
                            _ => {}
                        }
                    }
                    "alignment" => {
                        let align = parse_alignment(&e);
                        if let Some(xf) = cur_cell_style_xf.as_mut() {
                            xf.alignment = Some(align);
                        } else if let Some(xf) = cur_cell_xf.as_mut() {
                            xf.alignment = Some(align);
                        }
                    }
                    _ => {}
                }
            }
            quick_xml::events::Event::End(e) => {
                let tag = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                match tag.as_str() {
                    "font" if cur_font.is_some() => {
                        model.fonts.push(cur_font.take().expect("cur_font"));
                    }
                    "fill" if cur_fill.is_some() => {
                        model.fills.push(cur_fill.take().expect("cur_fill"));
                    }
                    "border" if cur_border.is_some() => {
                        model.borders.push(cur_border.take().expect("cur_border"));
                    }
                    "left" | "right" | "top" | "bottom" | "diagonal" => {
                        cur_border_segment = None;
                    }
                    "xf" => {
                        if let Some(xf) = cur_cell_style_xf.take() {
                            model.cell_style_xfs.push(xf);
                        }
                        if let Some(xf) = cur_cell_xf.take() {
                            model.cell_xfs.push(xf);
                        }
                    }
                    _ => {}
                }

                if stack.last().map(|s| s.as_str()) == Some(tag.as_str()) {
                    stack.pop();
                } else if let Some(pos) = stack.iter().rposition(|t| t == &tag) {
                    stack.truncate(pos);
                }
            }
            _ => {}
        }
    }

    model
}
