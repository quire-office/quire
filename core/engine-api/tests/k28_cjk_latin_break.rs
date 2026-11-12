//! K28 紅燈：幽靈網格欄把格寬稀釋到拉丁 atom 比行寬還窄 ⇒ KP 緊急切分逐字鋸開
//! （用戶 11/01 手機截圖實錘：`Phase 0 (單機驗證)`→`Pha/se 0`、`Cloudflare
//! Workers Paid ($5)`→`Clo/udfl/are/Wor/ker/s`、`$10.00`→`$10./00`、CJK 欄
//! 逐字直排、LO 7 頁 vs 我方 11 頁）。
//!
//! # 實錘根因（卡面假說 (a)「斷點表把拉丁字內誤標 allow」被否證）
//! `break_atoms`（UAX14 層）其實是對的：`'結束。'→['結','束。']`、
//! `'$100'→整塊`（本模組既有單元測試＋本檔燈 3 原子層自證）。真因在**欄寬
//! 層**：用戶檔 §七預算表宣告 `w:tblGrid` **12×1560 dxa**（自然 18720），但每
//! 列只有 6 格（`gridSpan` 全 1 ⇒ 列實際佔用**前 6 欄＝9360 dxa＝468pt**），
//! `w:tblW=9360 dxa` 恰等**已用欄**總寬。`resolve_columns` 取全部 12 欄再把
//! natural＝12×1560 等比歸一到宣告寬 9360 ⇒ 每欄 **39pt**（LO：raw 78.0pt）
//! ⇒ 內寬 39−18＝**21pt**（LO：78−18＝60pt）⇒ `'Worker'`≈36pt、`'$10.00'`
//! ≈33pt 等 atom 比行寬寬 ⇒ `kp_break_lines_at` 的**緊急切分**（uax14.rs 文件
//! 所述「寧可溢出不可丟字」防線）按 cluster 邊界把詞從字中鋸開；CJK 欄同時
//! 被壓成逐字直排、行數灌水撐爆頁（11 vs 7）。行尾「（」／行首「）」禁則違反
//! 同理＝緊急切分繞過 atom 禁則的下游症狀，非禁則規則未實作。
//!
//! # LO 25.8 ground truth（render_lab 單檔轉 PDF，格線實測）
//! - k28 真檔 §七表（解析器把 `w:tblGridChange` 歷史欄併入 ⇒ 12×1560、用 6、
//!   tblW 9360）：LO PDF p4 七條豎線 72.4/150.4/228.3/306.4/384.4/462.3/540.4
//!   （節距 **78.0＝raw 1560 dxa**，非歸一 39pt）、總寬 468pt、全文 7 頁
//!   （worker 2026-11-02 獨立實轉覆核）。舊態把 12 欄全寬歸一到 9360 ⇒ 39pt
//!   ＝用戶截圖爆字態（PM 存證渲染實錘）⇒ 本表為幽靈態「截斷＋跳過歸一」的
//!   **直接判別檔**（歸一 39pt vs raw 78pt 二選一，LO 取 raw）。
//! - `xopt/TestFiles_RA001-Tracked-Revisions-01/02`：同型 `w:tblGridChange`
//!   幽靈表（3 直屬欄＋3 歷史欄併入 ⇒ 6 欄、列用 3、tblW 8970＝直屬和≠併入
//!   和 17940）——舊態歸一（k＝0.5）後列寬 [79.1,52.8,92.4]pt＝raw 50% ⇒ 表格
//!   壓窄、行數灌水 7 頁；幽靈態截斷＋跳過歸一回 raw [158.1,105.6,184.8]pt
//!   ＝LO。worker 實轉 LO 兩檔 `/Count: 6`、render_lab LO 6/我方 6（頁數 ✓）。
//!   此二檔是全庫幽靈態下**唯一**歸一分支翻轉＋頁數變動者（400 檔 A/B）。
//! - `poi/bug65738`／`ooxmlexport/tdf104061`（無 `tblGridChange`）：實測非幽靈
//!   翻轉態——bug65738 兩表皆 2 欄用 2、tdf104061 tbl2 為 7 欄用 6 但 tblW＝
//!   全欄和 ⇒ 舊路徑歸一恆等，幽靈態截斷前後列寬逐位相同（display-list A/B
//!   diff=0）。列此二檔為**零擾動旁證**，非歸一口徑判別檔。
//!
//! 處方：`resolve_columns` 取 `grid[:N]`（N＝各列 ΣgridSpan 最大值）並跳過
//! 對宣告寬的等比歸一（幽靈表專屬；`w:tblW` 總超版心時仍走既有 K7 壓回）。
//! 一般表 N＝網格欄數 ⇒ 逐位元組零擾動（corpus 400 檔實掃，解析器口徑含
//! `w:tblGridChange` 內欄併入：幽靈表 24 張／11 檔，幽靈態下歸一分支狀態
//! 改變者僅 RA001 兩檔＝全庫頁數 A/B 唯一變動檔，見 completion metadata）。
//!
//! # 卡面燈 3 方向修正（worker 實測覆核點）
//! 卡面「行尾不得 `、。`、行首不得 `（「`」與 UAX#14 LB13/LB14 **方向相反**：
//! `、。`（CL/NS）合法可行尾、禁行首；`（「`（OP）合法可行首、禁行尾——LO 真值
//! 中 `。` 行尾隨處可見，照卡面文字鎖會把正確排版判成紅燈。本檔按 UAX/LO
//! 正確方向實作（行首禁閉標點、行尾禁開標點），用戶截圖的「檢r（」行尾正是
//! 「行尾開標點」態。
//!
//! # 燈號
//! - 燈 1（合成檔 `k28_ghost_grid.docx`，`gen_k28_fixture.py` 產生；幾何軸
//!   逐部複製用戶檔 §七表、文字中性化——用戶策略資產不入公開倉庫）：
//!   - 1a 格線七條 x∈{72,150,228,306,384,462,540}±0.7（LO raw 節距）＋反向
//!     封鎖 buggy 39pt 節距（x≈111/189/…）。
//!   - 1b 字界鎖（卡面燈 1 原口徑）：把每欄發射切片依序**貼回**該欄源字串序列
//!     （貼不回＝丟字），切片邊界若既非空白、又非 CJK/連字字界＝拉丁字中斷。
//!   - 1c `Worker`/`Alpha`/`$10.00`/`$45.00`/`$0.004` 必完整出現於單一切片；
//!     PM 實測爆字形態（`'Pha'`/`'Clo'`/`'$10.'`/`'00'`…）逐條封殺。
//!   - 1d 頁數收斂 ≤2（buggy 態逐字直排灌水，真檔同軸 11 vs 7）。
//! - 燈 2（卡面燈 2）：CJK 逐字可斷仍成立——中文欄正常多行（存在 ≥2 字行；
//!   buggy 21pt 行寬把每行壓成 1 字＝截圖「逐字直排」必紅）且零行溢出欄。
//! - 燈 3（禁則鎖）：行首不得閉標點、行尾不得開標點（buggy 緊急切分產出
//!   「（」尾／「）」首必紅）＋atom 生成器自身禁則自證。
//! - 燈 4（真檔回鎖，`K28_EVIDENCE_DOCX` 指向 PM 存證 k28-repro.docx 啟用；
//!   CI 無檔自動跳過）：頁數必＝7（PM 裁決）、§七表 78pt 格線、`Phase`/
//!   `Cloudflare`/`Sentry`/`$10.00`/`CRITICAL`/`Stripe` 完整切片、PM 實測爆字
//!   切片封殺、禁則零違反。
//! - 燈 5（零擾動鎖，K23 教訓）：fdo68607（正常網格）頁數 8、第二欄起點
//!   161.8pt 逐位元組不動。
//! - 燈 6（回塞鎖）：`K28_GHOST_GRID_BUGGY=1` ⇒ `resolve_columns` 退回「全欄
//!   歸一」舊路徑，合成檔必回 39pt 節距＋`'Alp'` 爆字＋頁數灌水（判別窗自我
//!   證明，防假綠）。
//!
//! 測試進程內不設 env（避免平行污染），回塞鎖／真檔鎖由外部執行：
//!
//! ```sh
//! K28_GHOST_GRID_BUGGY=1 cargo test -p quire-engine --test k28_cjk_latin_break
//! K28_EVIDENCE_DOCX=~/.hermes/cache/scratch/k28-evidence/k28-repro.docx \
//!   cargo test -p quire-engine --test k28_cjk_latin_break -- --nocapture
//! ```

use quire_engine::Engine;
use quire_layout::{break_atoms, TextMeasurer};
use quire_render_trait::DrawItem;

fn load_corpus(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("開啟 corpus docx 失敗")
}

fn load_fixture(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("開啟 fixture docx 失敗")
}

const GHOST: &str = "k28_ghost_grid.docx";

// ---- 合成檔幾何常數（與 gen_k28_fixture.py 同步）----
/// raw gridCol 1560 dxa＝78.0pt（LO k28-lo.pdf 實測格線節距）。
const COL_W: f32 = 78.0;
/// 七條豎線 x（LO 實測 72.4…540.4，容差 ±0.9）。
const GRID_X: [f32; 7] = [72.0, 150.0, 228.0, 306.0, 384.0, 462.0, 540.0];
/// 格內水平內距 tcMar 180 dxa×2＝18pt ⇒ 內寬 78−18＝60pt（燈 2 判別軸）。
const INNER_W: f32 = 60.0;
/// 合成檔字級 22 半磅＝11pt。
const SIZE_PT: f32 = 11.0;

/// 第 c 欄文字起點（表格 x_off 0＋版心 72＋c·78＋左內距 9；buggy 態同式取
/// 39pt 節距——1b/1c/2 的判別軸是切片合法性/完整性，欄 x 由 1a 獨立鎖死）。
fn col_x(c: usize) -> f32 {
    72.0 + c as f32 * COL_W + 9.0
}

// ---- 通用取樣 ----

/// 全文件文字項 `(page, x, y, size_pt, text)`，依 (page, y, x) 排序。
fn all_texts(eng: &Engine) -> Vec<(usize, f32, f32, String)> {
    raw_texts(eng)
        .into_iter()
        .map(|(p, x, y, _s, t)| (p, x, y, t))
        .collect()
}

/// 含 size_pt 的原始文字項（行重建要推估片段寬）。
fn raw_texts(eng: &Engine) -> Vec<(usize, f32, f32, f32, String)> {
    let mut v = Vec::new();
    for p in 0..eng.page_count() {
        let dl = eng
            .page_display_list(p)
            .unwrap_or_else(|| panic!("第 {p} 頁應存在"));
        for it in &dl.items {
            if let DrawItem::Text {
                x,
                y,
                text,
                size_pt,
                ..
            } = it
            {
                if !text.trim().is_empty() {
                    v.push((p, *x, *y, *size_pt, text.clone()));
                }
            }
        }
    }
    v.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.2.partial_cmp(&b.2).unwrap())
            .then(a.1.partial_cmp(&b.1).unwrap())
    });
    v
}

/// 某欄（x 容差 ±1.5）的切片序列（跨頁依 (page,y) 串接，去行尾空白）。
fn col_slices(all: &[(usize, f32, f32, String)], x: f32) -> Vec<String> {
    all.iter()
        .filter(|(_, tx, _, _)| (tx - x).abs() < 1.5)
        .map(|(_, _, _, t)| t.trim_end().to_string())
        .collect()
}

/// 某頁豎線（Rule x1≈x2）x 去重集合。
fn vertical_rule_xs(eng: &Engine, page: usize) -> Vec<f32> {
    let dl = eng.page_display_list(page).unwrap();
    let mut xs: Vec<f32> = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Rule { x1, y1, x2, y2, .. }
                if (x1 - x2).abs() < 0.01 && (y1 - y2).abs() > 1.0 =>
            {
                Some(*x1)
            }
            _ => None,
        })
        .collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    xs
}

// ---- CJK / 禁則小工具 ----

/// CJK／全形（UAX14 ID/OP/CP/QU 主要區段＋全形區）。
fn is_cjk(c: char) -> bool {
    matches!(
        c as u32,
        0x2E80..=0x2FFF
            | 0x3000..=0x303F
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0xFE30..=0xFE4F
            | 0xFF00..=0xFFEF
            | 0x20000..=0x2FA1F
    )
}

/// 行首禁則（LB13 CL/NS：閉標點／詞末標點**不得開行**）。
const NO_LINE_START: &str = "、。；，：）」』〉】…！？·";
/// 行尾禁則（LB14 OP：開標點**不得收尾**）。
const NO_LINE_END: &str = "「『《【（〔〈";

fn kinsoku_violations(lines: &[(usize, f32, f32, String)]) -> Vec<String> {
    let mut v = Vec::new();
    for (p, x, y, t) in lines {
        let t = t.trim();
        if t.is_empty() {
            continue;
        }
        let head = t.chars().next().unwrap();
        let tail = t.chars().next_back().unwrap();
        if NO_LINE_START.contains(head) {
            v.push(format!("p{p} y={y:.1} x={x:.1} 行首禁則 {head:?} ← {t:?}"));
        }
        if NO_LINE_END.contains(tail) {
            v.push(format!("p{p} y={y:.1} x={x:.1} 行尾禁則 {tail:?} ← {t:?}"));
        }
    }
    v
}

/// 把同一 (page, y±0.6) 的文字項按 x 序併成「重建行」。多樣式段落發射同 y 的
/// 逐 run 片段（如粗體「     ● Developer Pro（主力訂閱）」＋同 y「：$15 美元
/// / 月…」），逐項判禁則會把**行中段**樣式片段誤判行首（實測 p2/p3 十條
/// '：'／'，'／'。' 全屬此類偽陽）；併行後行首＝最左片段首字、行尾＝最右
/// 片段末字，才是視覺行首尾。表格欄另由 col_slices 逐欄判（欄內行首不等於
/// 頁內最左片段）。
fn rebuild_rows(items: &[(usize, f32, f32, String)]) -> Vec<(usize, f32, f32, String)> {
    let mut out: Vec<(usize, f32, f32, String)> = Vec::new();
    let mut i = 0usize;
    while i < items.len() {
        let (p, x0, y0, _) = &items[i];
        let mut text = String::new();
        let mut j = i;
        while j < items.len() && items[j].0 == *p && (items[j].2 - y0).abs() < 0.6 {
            text.push_str(&items[j].3);
            j += 1;
        }
        out.push((*p, *x0, *y0, text));
        i = j;
    }
    out
}

/// 燈 1b 判別器：把一欄的切片依序貼回**該欄源字串序列**（列序）。回傳違規：
/// - 貼不回／覆蓋不完 = 丟字、錯序；
/// - 欄內邊界非法 = 該處源字串無空白、兩側皆非 CJK、非連字 ⇒ 拉丁字中斷。
///
/// 欄與欄交界只要求精確貼合（不查空白合法性——新欄從新源開始）。
fn re_stitch(slices: &[String], sources: &[&str]) -> Vec<String> {
    let mut viol = Vec::new();
    let mut si = 0usize; // 當前源字串
    let mut pos = 0usize; // 源內位元組游標
    for (n, s) in slices.iter().enumerate() {
        if si >= sources.len() {
            viol.push(format!("切片 #{n} {s:?} 超出欄源字串序列（源已貼盡）"));
            return viol;
        }
        let src = sources[si];
        // 邊界吞空白（行尾空白發射端已 trim；源空白合法不計寬）。
        while pos < src.len() && src[pos..].starts_with(' ') {
            pos += 1;
        }
        if s.is_empty() {
            continue;
        }
        if !src[pos..].starts_with(s.as_str()) {
            viol.push(format!(
                "切片 #{n} {s:?} 貼不回（源 {src:?} 游標 {pos} 起為 {:?}）",
                &src[pos..src.len().min(pos + 24)]
            ));
            return viol;
        }
        let after = pos + s.len();
        // 內部邊界合法性（欄邊界 after==src.len() 免查）。
        if after < src.len() {
            let prev_c = src[..after].chars().next_back().unwrap();
            let mut j = after;
            while j < src.len() && src[j..].starts_with(' ') {
                j += 1;
            }
            let next_c = src[j..].chars().next();
            let legal = src[after..].starts_with(' ')
                || is_cjk(prev_c)
                || next_c.is_none_or(is_cjk)
                || prev_c == '-'
                || next_c == Some('-');
            if !legal {
                viol.push(format!(
                    "字中斷點：{s:?} | {:?}（{src:?} 位元組 {after} 處無空白/CJK 字界）",
                    slices.get(n + 1).map(String::as_str).unwrap_or("")
                ));
            }
        }
        pos = after;
        if pos == src.len() {
            si += 1;
            pos = 0;
        }
    }
    if si < sources.len() {
        viol.push(format!(
            "欄源字串未貼盡：{}/{} 欄起 {:?} 游標 {pos} 未貼完",
            si + 1,
            sources.len(),
            sources[si]
        ));
    }
    viol
}

// ================================================================ 燈 1a
/// 燈 1a（格線幾何鎖）：幽靈網格表（12 欄宣告、列用 6 欄、tblW=已用欄和）
/// 格線必落 raw 78pt 節距（LO 實測）。buggy 態 39pt 節距必紅。
#[test]
fn k28_lamp1a_ghost_grid_column_geometry() {
    let eng = load_fixture(GHOST);
    let xs = vertical_rule_xs(&eng, 0);
    assert!(
        xs.len() >= 7,
        "p0 應有 ≥7 條豎線（6 欄表），實得 {}：{xs:?}",
        xs.len()
    );
    for (i, &want) in GRID_X.iter().enumerate() {
        assert!(
            xs.iter().any(|x| (x - want).abs() < 0.9),
            "格線第 {i} 條應在 x≈{want}（raw 1560 dxa 節距＝LO 實測），實得 {xs:?}"
        );
    }
    // 反向封鎖：buggy 態特有的 39pt 節距不得存在。
    for bad in [111.0f32, 189.0, 267.0, 345.0, 423.0] {
        assert!(
            !xs.iter().any(|x| (x - bad).abs() < 0.9),
            "格線落在 buggy 節距 x≈{bad}（39pt 欄）：{xs:?}"
        );
    }
}

// ================================================================ 燈 1b
/// 燈 1b（卡面燈 1 原口徑：字界鎖）：混排欄切片貼回源字串＋邊界合法性。
/// 欄源字串序＝`gen_k28_fixture.py` ROWS 的該欄列序（9 列，含空字串跳過）。
#[test]
fn k28_lamp1b_slice_boundaries_land_on_legal_breaks() {
    let eng = load_fixture(GHOST);
    let all = all_texts(&eng);
    // 欄 1（資源規格與用途）：9 列源字串。
    let col1 = vec![
        "資源規格與用途",
        "Edge Worker Paid ($5) + KV / DO 計量",
        "Host Node (64GB RAM, NVMe)",
        "無頭瀏覽器沙盒運算（動態抓取）",
        "Log Tracker 追蹤 + 基礎監控",
        "登記、申報與諮詢費用",
        "商業網域名稱 (.com)",
        "進階方案：$15 美元 / 月，包含 3,000 次查詢（超出按量計費：$0.004 / 次）。",
        "",
    ];
    // 欄 2–5：金額＋表頭（欄 x 由燈 1a 保證節距；此處判切片合法性）。
    let col2 = vec![
        "Alpha 0 (單機驗證)",
        "$0.00",
        "$45.00",
        "$0.00",
        "$0.00",
        "$0.00",
        "$1.20",
        "$0.00",
        "$46.20",
    ];
    let col3 = vec![
        "Alpha 1 (集中營運)",
        "$5.00",
        "$45.00",
        "$10.00",
        "$0.00",
        "$20.00",
        "$1.20",
        "$15.00",
        "$84.20",
    ];
    let col5 = vec![
        "Alpha 3 (分片啟動)",
        "$15.00",
        "$90.00（雙機備援）",
        "$40.00",
        "$20.00",
        "$20.00",
        "$1.20",
        "$15.00",
        "$211.20",
    ];
    let mut violations: Vec<String> = Vec::new();
    for (c, srcs) in [
        (1usize, &col1[..]),
        (2, &col2[..]),
        (3, &col3[..]),
        (5, &col5[..]),
    ] {
        let slices = col_slices(&all, col_x(c));
        let nonempty: Vec<&str> = srcs.iter().copied().filter(|s| !s.is_empty()).collect();
        let v = re_stitch(&slices, &nonempty);
        for e in v {
            violations.push(format!("欄 {c} x≈{}：{e}", col_x(c)));
        }
    }
    assert!(
        violations.is_empty(),
        "混排欄斷行違規 {} 條：\n{}",
        violations.len(),
        violations.join("\n")
    );
}

// ================================================================ 燈 1c
/// 燈 1c（完整 token 鎖）：長詞與金額必以完整形態出現於**單一**切片；
/// PM 實測爆字形態（合成檔同軸）逐條封殺。buggy 態發射 `'Wor'/'ker'`、
/// `'$10.'/'00'` ⇒ 必紅。
#[test]
fn k28_lamp1c_tokens_intact() {
    let eng = load_fixture(GHOST);
    let all: Vec<String> = all_texts(&eng)
        .into_iter()
        .map(|(_, _, _, t)| t.trim().to_string())
        .collect();
    for want in [
        "Worker", "Alpha", "Tracker", "$0.00", "$5.00", "$10.00", "$45.00", "$0.004", "$46.20",
        "$211.20", "$90.00",
    ] {
        assert!(
            all.iter().any(|t| t.contains(want)),
            "token {want:?} 未完整出現在任何切片（被鋸開＝用戶截圖實錘缺陷）；切片樣本：{:?}",
            all.iter().take(10).cloned().collect::<Vec<_>>()
        );
    }
    // PM 實測爆字形態逐條封殺（真檔 'Pha'/'Clo' 族在合成檔的同型）。
    for bad in ["Alp", "ha", "Wor", "ker", "$10.", "$0.0", "$45.", "00"] {
        let hit = all.iter().any(|t| t == bad);
        assert!(!hit, "爆字切片 {bad:?} 仍出現（燈 1 回退）");
    }
}

// ================================================================ 燈 1d
/// 燈 1d（頁數收斂鎖）：爆字逐字直排把行數灌水（用戶真檔 LO 7 頁 vs 我方 11
/// 頁）——修復後合成檔（2 段＋9 列表）必 ≤2 頁；buggy 態（燈 6 鎖 4 頁）必紅。
#[test]
fn k28_lamp1d_page_count_converges() {
    let eng = load_fixture(GHOST);
    assert!(
        eng.page_count() <= 2,
        "幽靈網格表頁數必收斂 ≤2（LO 同表 1 頁＋兩段正文；buggy 逐字直排灌水 \\\n\
         到 4 頁、真檔灌到 11 頁），實得 {}",
        eng.page_count()
    );
}

// ================================================================ 燈 2
/// 燈 2（卡面燈 2：CJK 逐字可斷仍成立）：修復給足內寬（60pt）後，中文欄
/// 「正常多行」＝存在 ≥2 字的行（buggy 21pt 行寬把每行壓成 1 字＝用戶截圖
/// 「中文欄逐字堆疊」，必紅）；且任何切片寬 ≤ 內寬（零溢出欄）。
#[test]
fn k28_lamp2_cjk_wraps_multichar_lines_without_overflow() {
    let eng = load_fixture(GHOST);
    let all = all_texts(&eng);
    let m = TextMeasurer::with_advance_scale(SIZE_PT, 1.0);

    // (a) 表格欄切片零溢出：x 必落「列文字 x」（raw 節距 72+78k+9 或 buggy
    // 節距 72+39k+9，k∈0..6——兩態都要抓到），正文（x=72）不計。
    let pitch: Vec<f32> = (0..6)
        .map(|k| 72.0 + k as f32 * COL_W + 9.0)
        .chain((0..6).map(|k| 72.0 + k as f32 * 39.0 + 9.0))
        .collect();
    let mut over = Vec::new();
    for (p, x, y, t) in &all {
        if *y > 110.0
            && pitch.iter().any(|px| (x - px).abs() < 1.0)
            && m.text_width(t.trim_end()) > INNER_W + 0.01
        {
            over.push(format!(
                "p{p} y={y:.1} x={x:.1} {:?} 寬={:.1}",
                t,
                m.text_width(t)
            ));
        }
    }
    assert!(
        over.is_empty(),
        "表格切片溢出內寬 {INNER_W}pt：\n{}",
        over.join("\n")
    );

    // (b)「資源規格與用途」欄（欄 1 第 0 列）：7 CJK 字×11pt＝77pt＞60pt ⇒
    // 必折 ≥2 行、且存在 ≥2 字行（逐字直排態 7 行×1 字必紅）。
    let c1 = col_slices(&all, col_x(1));
    let spec: Vec<String> = c1
        .iter()
        .take_while(|t| t.chars().all(|c| "資源規格與用途".contains(c)))
        .cloned()
        .collect();
    assert!(
        spec.len() >= 2,
        "「資源規格與用途」必折 ≥2 行（77pt＞60pt 內寬），實得 {:?}",
        c1.iter().take(8).collect::<Vec<_>>()
    );
    assert!(
        spec.iter().any(|t| t.chars().count() >= 2),
        "CJK 欄必「正常多行」＝存在 ≥2 字行；全部切片 1 字＝逐字直排未修：{spec:?}"
    );

    // (c) 表後純 CJK 長句多行且每行 ≥2 字（正文寬 468pt，一行 ~42 字）：
    // 從含首句標記的行起、向後同 x 帶的行都算本段（跨頁亦收）。
    let body: Vec<(usize, f32, String)> = all
        .iter()
        .filter(|(_, x, _, _)| (x - 72.0).abs() < 1.0)
        .map(|(p, _, y, t)| (*p, *y, t.trim_end().to_string()))
        .collect();
    let start = body
        .iter()
        .position(|(_, _, t)| t.contains("收錄邊緣層"))
        .expect("CJK 長句首行應存在（x=72 帶）");
    let para: Vec<String> = body[start..].iter().map(|(_, _, t)| t.clone()).collect();
    assert!(para.len() >= 3, "CJK 長句應多行，實得 {:?}", para);
    assert!(
        para.iter().all(|t| t.chars().count() >= 2),
        "正文 CJK 行必 ≥2 字（逐字直排回退）：{:?}",
        para
    );
}

// ================================================================ 燈 3
/// 燈 3（禁則鎖）：表格區行首不得閉標點、行尾不得開標點（buggy 緊急切分繞過
/// atom 產出「（dynamic…）」的「（」尾／「）」首＝真檔 p4/p5 實錘 4 條，必紅）；
/// 加 atom 生成器（UAX14 層）自身禁則自證——它從未壞，壞的是緊急切分繞過它。
#[test]
fn k28_lamp3_kinsoku_locked() {
    let eng = load_fixture(GHOST);
    let table_region: Vec<(usize, f32, f32, String)> = all_texts(&eng)
        .into_iter()
        .filter(|(_, _, y, _)| *y >= 110.0)
        .collect();
    let v = kinsoku_violations(&table_region);
    assert!(
        v.is_empty(),
        "禁則違規 {} 條（緊急切分繞過 atom＝爆字下游）：\n{}",
        v.len(),
        v.join("\n")
    );

    // 原子層自證：LB13/LB14 生成器永不產出行首閉標點／行尾開標點的 atom 主體。
    for t in [
        "結束。",
        "引用「開頭」結尾",
        "運算（動態抓取）",
        "（USD）。Edge Worker 與 Host Node 的組合",
        "進階方案：$15 美元 / 月，包含 3,000 次查詢（超出按量計費：$0.004 / 次）。",
    ] {
        for a in break_atoms(t) {
            let body = &t[a.start..a.trim_from];
            if body.is_empty() {
                continue;
            }
            let head = body.chars().next().unwrap();
            let tail = body.chars().next_back().unwrap();
            assert!(
                !NO_LINE_START.contains(head),
                "atom 生成器洩漏行首禁則字元 {head:?}：{t:?} → atom {body:?}"
            );
            assert!(
                !NO_LINE_END.contains(tail),
                "atom 生成器洩漏行尾禁則字元 {tail:?}：{t:?} → atom {body:?}"
            );
        }
    }
}

// ================================================================ 燈 4
/// 燈 4（真檔回鎖）：`K28_EVIDENCE_DOCX` 指向 PM 存證 k28-repro.docx（用戶
/// 線上實測原檔，22.6KB，用戶策略資產**不入庫**）時啟用；CI 無檔自動跳過。
/// 鎖：頁數＝7（PM 裁決＋LO ground truth）、§七表 78pt 格線、爆字 token
/// 完整、PM 實測爆字切片封殺、禁則零違反。
#[test]
fn k28_lamp4_real_file_evidence_lock() {
    let Some(path) = std::env::var_os("K28_EVIDENCE_DOCX") else {
        eprintln!("未設 K28_EVIDENCE_DOCX：跳過真檔回鎖（本地以 PM 存證路徑跑）。");
        return;
    };
    let bytes = std::fs::read(&path).expect("K28_EVIDENCE_DOCX 指向的檔不存在");
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟真檔失敗");
    assert_eq!(
        eng.page_count(),
        7,
        "真檔頁數必收斂到 LO ground truth 7 頁（PM 實測修復前 11 頁＝爆字灌水下游）"
    );
    let all = all_texts(&eng);
    // §七表所在頁：含「邊緣閘道」**且有豎線**（正文 p2 也含該詞但無格線）。
    let p = (0..eng.page_count())
        .filter(|p| {
            all.iter()
                .any(|(pp, _, _, t)| pp == p && t.contains("邊緣閘道"))
        })
        .find(|p| !vertical_rule_xs(&eng, *p).is_empty())
        .expect("§七預算表頁應存在（含「邊緣閘道」格＋格線）");
    let xs = vertical_rule_xs(&eng, p);
    for (i, want) in GRID_X.iter().enumerate() {
        assert!(
            xs.iter().any(|x| (x - want).abs() < 1.2),
            "§七表格線第 {i} 條應在 x≈{want}（LO 實測 72.4…540.4），實得 {xs:?}"
        );
    }
    let flat: Vec<String> = all
        .iter()
        .map(|(_, _, _, t)| t.trim().to_string())
        .collect();
    for want in [
        "Phase",
        "Cloudflare",
        "Sentry",
        "$10.00",
        "CRITICAL",
        "Stripe",
        "Hetzner",
    ] {
        assert!(
            flat.iter().any(|t| t.contains(want)),
            "真檔 token {want:?} 未完整出現在任何切片（PM 實測 19 條爆字清單成員）"
        );
    }
    for bad in [
        "Pha", "Clo", "udfl", "are", "Wor", "ker", "$0.0", "$10.", "$5.0", "$15.", "00",
    ] {
        assert!(
            !flat.iter().any(|t| t == bad),
            "真檔爆字切片 {bad:?} 仍出現（PM 實錘 p4 共 19 條之一）"
        );
    }
    let v = kinsoku_violations(&rebuild_rows(&all));
    assert!(
        v.is_empty(),
        "真檔禁則違規 {} 條（修復前實測 4 條、修復後必 0）：\n{}",
        v.len(),
        v.join("\n")
    );
}

// ================================================================ 燈 5
/// 燈 5（零擾動鎖，K23 教訓：改斷行必附零擾動清單）：正常網格表（欄數＝已用
/// 欄數，N==grid.len.）路徑逐位元組不動——fdo68607 頁數 8、第二欄起點
/// 161.8pt 維持；goldens tables.jsonl 的輸入全為正常網格（無幽靈態）。
#[test]
fn k28_lamp5_normal_grid_tables_zero_perturbation() {
    let eng = load_corpus("ooxmlexport/fdo68607.docx");
    assert_eq!(eng.page_count(), 8, "fdo68607 頁數必須維持 8（零擾動面）");
    let dl = eng.page_display_list(0).unwrap();
    let x0 = dl
        .items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text { x, text, .. } if *x > 160.5 && !text.trim().is_empty() => Some(*x),
            _ => None,
        })
        .fold(f32::MAX, f32::min);
    assert!(
        (x0 - 161.8).abs() < 0.6,
        "fdo68607 第二欄起點必須維持 161.8pt（正常網格零擾動），實得 {x0:.1}"
    );
}

// ================================================================ 燈 6
/// 燈 6（回塞鎖）：`K28_GHOST_GRID_BUGGY=1` ⇒ `resolve_columns` 退回「全欄
/// 歸一宣告寬」舊路徑：幽靈表必回 39pt 節距（x≈111 格線存在）＋`'Alp'` 爆字
/// ＋頁數灌水 4——判別窗自我證明（防燈 1a/1b/1d 假綠）。
#[test]
fn k28_lamp6_buggy_regression_lock() {
    if std::env::var_os("K28_GHOST_GRID_BUGGY").is_none() {
        eprintln!("未設 K28_GHOST_GRID_BUGGY：跳過回塞檢驗（以 env=1 跑本檔驗證必紅）。");
        return;
    }
    let eng = load_fixture(GHOST);
    let xs = vertical_rule_xs(&eng, 0);
    assert!(
        xs.iter().any(|x| (x - 111.0).abs() < 0.9),
        "BUGGY 態格線必回 39pt 節距（x≈111 存在），實得 {xs:?}"
    );
    let flat: Vec<String> = all_texts(&eng)
        .into_iter()
        .map(|(_, _, _, t)| t.trim().to_string())
        .collect();
    assert!(
        flat.iter().any(|t| t == "Alp"),
        "BUGGY 態 'Alpha' 必被鋸成 'Alp'（爆字舊態），切片樣本 {:?}",
        flat.iter().take(12).cloned().collect::<Vec<_>>()
    );
    assert!(
        eng.page_count() > 2,
        "BUGGY 態頁數必灌水（逐字直排舊態；修復態 ≤2），實得 {}",
        eng.page_count()
    );
}
