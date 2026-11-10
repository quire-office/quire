//! 宣告族 → LO 實際渲染 face 的「全形標點 advance」覆寫表（E1-C 卡 C）。
//!
//! # 為什麼要這張表（實測，非推測）
//!
//! 內嵌子集源自 Noto Sans CJK TC，‘ ’ “ ” … · • 等標點在**原始字型**裡就是
//! 1.0 em 方塊（本 run `probe_src_advance.py`：原始 otf 與我方子集逐字全等
//! ⇒ 不是子集產生器的缺陷，是 CJK 字型的正當設計）。我方把整套 hmtx 照抄進來，
//! 於是**拉丁上下文**裡的這些標點也跟着吃 1.0 em：
//! `iBall’s` 的 ’ 我方量 10.000pt@10pt，而定錨 LO 實渲 3.672pt（trebuc hmtx）
//! ⇒ 單字元虛增 6.328pt，跨檔污染（本 run 實掃：拉丁上下文含任一者 **50 檔**）。
//!
//! # 為什麼不能改字體二進位、也不能寫死一個數
//!
//! 1. **漢語上下文我方現態是對的**：LO 對漢語段的 “ ” 渲染 1.0 em（本 run
//!    `probe_lo_rawdict.py` 實測 5DayTour「插入”选项卡」Δ=11.0pt@11pt＝恰 1.0 em，
//!    渲 WenQuanYiZenHei）。改 otf 會把漢語標點一起改窄 ⇒ 回歸。
//!    ⇒ 覆寫**只對非 CJK script run 生效**（比照本庫既有「CJK cluster 不乘尺」鐵律）。
//! 2. **沒有單一 LO 真值**：同一個 ’ 隨宣告族而變（本 run `probe_lo_face_punct.py`
//!    逐 face rawdict 實測：Calibri 0.249／TNR 0.331／Trebuchet 0.3672／NotoSerif
//!    0.250 em）。卡面寫的 0.175／0.791 是 LO **自帶 NotoSans** 的 hmtx＝E1-C 卡 B
//!    已明文否決的頂替態口徑，不可當前題。
//!    ⇒ 必須按 face 查表，與 `face_advance_scale`（engine-api）**同臂、同守門、同尺向**。
//!
//! # 值源與守門紀律
//!
//! 每個數都由**真實 face 二進位的 hmtx** 直讀（本 run `probe_punct_truth.py`／
//! `probe_cambria2.py`），不推估、不虛構：
//! - Calibri→真 `calibri.ttf`、Cambria→真 `k6cal/cambria.ttc`（Cambria 面）、
//!   TNR→真 `times.ttf`、Arial→真 `arial.ttf`、Trebuchet→真 `trebuc.ttf`
//!   （本庫基準環境＝真 MS face 裝機態，見 `docs/e1cb-lo-truth-canon.md`）
//! - NotoSerif→LO 25.8 自帶 `NotoSerif-Regular.ttf`（theme 斷鏈臂）。
//!   覆寫集**只含我方子集實測為 1.0 em 的碼點**（實測非 1.0 em 的 – — ‹ › ' „ 一律不動）；
//!   真 face 無該 glyph（n/a）⇒ 不覆寫（**無真值即不猜**，比照 K7 未命中＝1.0 守門）。
//!
//! 交叉驗證（rawdict 物理渲染 vs hmtx，本 run）：TNR ’ 0.331 vs 0.3330、Calibri ’
//! 0.249 vs 0.2495、NotoSerif ’ 0.250 vs 0.2500、TNR … 1.000 vs 1.0000（PDF 0.05pt
//! 量化內）⇒ 表值與 LO 實際渲染一致，非只讀 hmtx 的紙上值。

/// 覆寫臂 id。`0`＝不覆寫（未宣告族／查表未命中／回塞鎖），既有行為逐位元組不變。
pub const PUNCT_ARM_NONE: u8 = 0;

/// 一個覆寫點：碼點 + 真 face 的 em 值。
const fn em(cp: u32, v: f32) -> (u32, f32) {
    (cp, v)
}

// 覆寫集＝本 run `probe_fullwidth_cov.py` 實測「我方子集 advance ≥0.999 em」的全部碼點。
// 逐臂值＝該真 face hmtx（@1000 upm 歸一）。n/a（真 face 缺该 glyph）一律缺席＝不覆寫。

/// Calibri → 真 `calibri.ttf`（本基準環境 LO 渲真 MS face）。
static CALIBRI: &[(u32, f32)] = &[
    em(0x2018, 0.2495),
    em(0x2019, 0.2495),
    em(0x201C, 0.4185),
    em(0x201D, 0.4185),
    em(0x2026, 0.6904),
    em(0x00B7, 0.2524),
    em(0x2022, 0.4980),
    em(0x2015, 0.9053),
    em(0x2010, 0.3062),
];

/// Cambria → 真 `k6cal/cambria.ttc`（Cambria 面，非 Math 面）。
static CAMBRIA: &[(u32, f32)] = &[
    em(0x2018, 0.2212),
    em(0x2019, 0.2212),
    em(0x201C, 0.3755),
    em(0x201D, 0.3755),
    em(0x2026, 0.7520),
    em(0x00B7, 0.2822),
    em(0x2022, 0.4434),
    em(0x2015, 0.7500),
    em(0x2010, 0.3320),
];

/// Times New Roman／Liberation Serif → 真 `times.ttf`。
/// U+2026 真值恰 1.0000 em（LO rawdict 實渲亦 1.000）⇒ 表中不列（覆寫等於現態）。
static TIMES: &[(u32, f32)] = &[
    em(0x2018, 0.3330),
    em(0x2019, 0.3330),
    em(0x201C, 0.4438),
    em(0x201D, 0.4438),
    em(0x00B7, 0.3330),
    em(0x2022, 0.3501),
    em(0x2015, 1.0000),
    em(0x2010, 0.3330),
];

/// Arial／Liberation Sans／Helvetica → 真 `arial.ttf`。
/// U+2026／U+2015 真值恰 1.0000 em ⇒ 不列；U+2010 真 face 缺字 ⇒ 不覆寫。
static ARIAL: &[(u32, f32)] = &[
    em(0x2018, 0.2222),
    em(0x2019, 0.2222),
    em(0x201C, 0.3330),
    em(0x201D, 0.3330),
    em(0x00B7, 0.3330),
    em(0x2022, 0.3501),
];

/// Noto Serif（theme 斷鏈臂）→ LO 25.8 自帶 `NotoSerif-Regular.ttf`。
static NOTO_SERIF: &[(u32, f32)] = &[
    em(0x2018, 0.2500),
    em(0x2019, 0.2500),
    em(0x201C, 0.4500),
    em(0x201D, 0.4500),
    em(0x2026, 0.8580),
    em(0x00B7, 0.2500),
    em(0x2022, 0.3620),
    em(0x2027, 0.2500),
    em(0x2025, 0.5360),
    em(0x2015, 1.0000),
    em(0x2010, 0.3100),
];

/// Trebuchet MS → 真 `trebuc.ttf`（本基準環境裝機）。
static TREBUCHET: &[(u32, f32)] = &[
    em(0x2018, 0.3672),
    em(0x2019, 0.3672),
    em(0x201C, 0.5244),
    em(0x201D, 0.5244),
    em(0x2026, 0.7344),
    em(0x00B7, 0.3672),
    em(0x2022, 0.5244),
    em(0x2015, 0.7344),
    em(0x2010, 0.3672),
];

const ARMS: &[&[(u32, f32)]] = &[CALIBRI, CAMBRIA, TIMES, ARIAL, NOTO_SERIF, TREBUCHET];

/// 宣告族 → 覆寫臂 id。鍵口徑與 engine-api `face_advance_scale` **逐臂對齊**
/// （同名同分組，未命中＝`PUNCT_ARM_NONE`）；兩表若走樣，`e1c_subset_punct`
/// 的對齊鎖會先紅。
pub fn punct_arm_for_face(face: &str) -> u8 {
    match face.trim().to_lowercase().as_str() {
        "calibri" | "carlito" => 1,
        "cambria" | "caladea" => 2,
        "liberation serif" | "liberationserif" | "times new roman" | "times" => 3,
        "arial" | "liberation sans" | "liberationsans" | "helvetica" => 4,
        "noto serif" | "notoserif" => 5,
        "trebuchet ms" | "trebuchetms" => 6,
        _ => PUNCT_ARM_NONE,
    }
}

/// 臂 id → 覆寫表。
fn arm_table(arm: u8) -> Option<&'static [(u32, f32)]> {
    ARMS.get(arm.checked_sub(1)? as usize).copied()
}

/// 該標點在此臂的真實 em（`None`＝不覆寫：無臂、非覆寫碼點、或真 face 缺字）。
pub fn punct_em(arm: u8, cp: u32) -> Option<f32> {
    arm_table(arm)?
        .iter()
        .find(|(c, _)| *c == cp)
        .map(|(_, v)| *v)
}

/// 該碼點是否屬「我方子集全形（1.0 em）標點家族」——供考卷與診斷使用。
pub fn is_fullwidth_in_subset(cp: u32) -> bool {
    matches!(
        cp,
        0x2018
            | 0x2019
            | 0x201C
            | 0x201D
            | 0x2025
            | 0x2026
            | 0x2027
            | 0x00B7
            | 0x2022
            | 0x2015
            | 0x2010
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arm_means_no_override() {
        assert_eq!(punct_arm_for_face("Calibri"), 1);
        assert_eq!(punct_arm_for_face("  TREBUCHET MS "), 6);
        assert_eq!(punct_arm_for_face("宋体"), PUNCT_ARM_NONE);
        assert_eq!(punct_arm_for_face(""), PUNCT_ARM_NONE);
        assert!(punct_em(PUNCT_ARM_NONE, 0x2019).is_none());
        // 臂 id 越界（表外）＝不覆寫，不得 panic。
        assert!(punct_em(200, 0x2019).is_none());
    }

    #[test]
    fn only_subset_fullwidth_codepoints_are_overridden() {
        // 我方子集實測非 1.0 em 的碼點一律不動（– — ‹ › ' „）。
        for cp in [0x2013u32, 0x2014, 0x2039, 0x203A, 0x0027, 0x201E, 0x0020] {
            for arm in 1..=6 {
                assert!(punct_em(arm, cp).is_none(), "U+{cp:04X} 不應被覆寫");
            }
        }
    }

    #[test]
    fn override_never_exceeds_subset_square_and_is_positive() {
        for arm in 1..=6u8 {
            let t = arm_table(arm).unwrap();
            for (cp, em) in t {
                assert!(*em > 0.0, "臂 {arm} U+{cp:04X} em 必須 >0");
                assert!(
                    *em < 1.0 || (*em - 1.0).abs() < 1e-6,
                    "臂 {arm} U+{cp:04X} em={em} 只允許 ≤1.0（覆寫是收窄不是放大）"
                );
                assert!(is_fullwidth_in_subset(*cp), "臂 {arm} U+{cp:04X} 非覆寫集");
            }
        }
    }

    #[test]
    fn trebuchet_apostrophe_is_the_measured_value() {
        // 本卡標的：’ 由 1.0 em → 真 trebuc hmtx 0.3672 em。
        assert!((punct_em(6, 0x2019).unwrap() - 0.3672).abs() < 1e-9);
        assert!((punct_em(6, 0x2026).unwrap() - 0.7344).abs() < 1e-9);
        // 卡面的 0.175／0.791 是頂替態 NotoSans，本表**不得**出現該口徑。
        for arm in 1..=6u8 {
            assert_ne!(punct_em(arm, 0x2019), Some(0.175));
            assert_ne!(punct_em(arm, 0x2026), Some(0.791));
        }
    }
}
