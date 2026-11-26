//! U4m（t_f073cd6c）紅燈考卷：`caret_at` 行框歸屬改**行框墨跡真值制**——行框頂
//! 以上點擊不得錯判上一段（用戶「無法任意按 Enter 創建新段落」的直接成因）。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 【PM 最終裁示 11-26 04:45：前提反轉，本檔依反轉後前提書寫】
//! 發射 y／`caret_rect.y`／顯示列表 `Text.y` **不是基線，是行框頂**：三支出貨渲染器
//! （`shells/web/demo/main.js`、編輯器外殼 vendor 渲染器、
//! `shells/native/src/render_svg.rs`）皆把**字形基線**畫在 `y ＋ size_pt`。
//! 原卡 body（PM 拆卡時寫錯的前提）把 y 讀作基線；run 85/88 據此落庫的
//! 「行距中線制」（切分點＝兩行行框頂之中點）依反轉後前提**作廢**——中線橫穿字形
//! （12pt／pitch 16.8：中線 `y＋8.4`，墨跡 `[y−1.92, y＋15.46]`），把每行字形下半
//! 中位 5.61pt、最深 14.01pt 判給下一段，PM 逐行復算 `docs/word-ui-u4m-click-evidence.json`
//! 得墨跡破口 38/38。原 body 的用戶症狀（行框頂以上點擊錯判上一段）與跨度制邊界
//! 缺陷仍屬實（跨度制殘餘破口 1/38），本卡以墨跡真值制同時堵死兩條。
//!
//! # 用戶實測缺口（CDP 實錄，127.0.0.1:8791 示範檔，12pt／行距 1.4／pitch 16.8pt）
//! para5 首行行框頂 `y_pt=408`（display list `Text.y` 同值；上行 391.2、下行 424.8；
//! 該行字形基線在 408＋12＝420）：
//!   dy = -14 .. -2  → para 4 / cp 290（**上一段段尾**）
//!   dy =   0 .. +8  → para 5 / cp 24 （正確）
//! 亦即該行行框頂以上那一帶（視覺上確實是本行的字）點擊全部錯判成前一行。點擊錯判
//! → caret 錯段 → Enter 分錯段（矩陣實測：點 para5 段中 `grew=1`，但新建段落落在
//! para4/5 之間）。
//!
//! # 根因（core/layout/src/lib.rs `caret_at` 的 U4f 跨度判制）
//! 兩處疊加：
//!   1. **先 floor 再判帶**：`(y−margin_top)/line_h` 立刻 `.floor()`，把「行框頂以上
//!      8pt」與「行框頂以下 8pt」壓成同一個整數帶——粗帶尺（每行 1 帶，即用戶實錄
//!      的 12pt／行距 1.4 常態）下任何垂直判制都不可能發火。
//!   2. **跨度自行框頂起算、一路吞到下一行行框頂**：發射口的行 y ＝
//!      `margin_top ＋ band × line_h` 是**行框頂**，反向腿的行跨度卻是
//!      `[起帶, 起帶＋bands_of(para))`＝[本行行框頂, 下一行行框頂)——相鄰兩行的
//!      跨度邊界正好切在下一行的行框頂上 ⇒ 本行字形下半的全部墨跡被判給下一行，
//!      且下一行行框頂以上的墨跡被上一行吃掉。`caret_rect` 正向腿給的 y 恰在邊界
//!      上，U4i 的 `band_eps=1e-3` 只救「恰邊界的浮點噪聲」，救不了真實點擊位置。
//!
//! # 修法（PM 裁示；不造第二把尺）
//! 反向腿改用**小數帶商**比對**行框墨跡真值**切分的中介帶。一行（行框頂帶 b、
//! 字級 size）的墨跡上下緣（帶單位；`asc_em／desc_em`＝內嵌子集 hhea 度量，
//! `shaping::ink_em_ratios`，與斷行／發射／caret 同一支字型）：
//!     ink_top ＝ b ＋ (size − asc_em·size)／line_h
//!     ink_bot ＝ b ＋ (size ＋ desc_em·size)／line_h
//! 相鄰兩行（同欄，k 在上、i 在下）的切分點＝`ink_bot_k` 與 `ink_top_i` 的中點，
//! 夾到「不越下行行框頂」⇒ 行間空白歸**最近墨跡行**、每行墨跡區間內任一點反查
//! 必歸該行。上下兩行用同一運算式求同一條邊 ⇒ 首尾相接（零重疊、零死區）。
//!   · 欄內首行上界＝本行墨跡上緣再上 half-pitch（段前距／行距空白歸首行）。
//!   · 欄內末行下界＝`band ＋ span`（沿用修前口徑＝命中面只增不減，U4f 死區教訓）。
//!   · 表格切片／流內圖佔用帶區間一律夾住 lo/hi ⇒ 既有「表格帶必 None」零擾動。
//! 尺源仍是 `page.line_bands` ＋ `ParaGrid::bands_of` ＋ `ParaGrid::size_of`（正向腿
//! 同一來源）；整數帶 `band` 仍留給修前路徑（`U4F_CARET_AT_BUGGY` 回塞態與舊快照
//! 陣標退回）。
//!
//! # Tripwire（PM 裁示三：只收緊不弱化）
//! 燈 1 鎖「每一行**自身墨跡區間內任一點**（含行框頂以上與基線上下兩側）反查歸屬
//! ＝該行」；燈 2 除全欄逐點掃 y 與行框模型對帳外，另以**直接幾何**（不走模型）鎖
//! 「相鄰行之間的空白帶歸最近墨跡行」。兩條都比修前跨度制的契約更強（跨度制殘餘
//! 墨跡破口 1/38 一併堵死）。
//!
//! # 既有考卷的歸屬口徑變更（唯一一處，PM 承擔改卷責任）
//! `u4f_band_ruler` 燈 3(b) 舊斷言「行距帶（`b0+1 .. b0+span`）全部落回**上一行**」
//! ——那正是本卡修掉的缺陷（細帶尺 span=23 帶＝整拍行距，其中靠下的帶是下一行
//! 字形的字面位置，舊口徑把下一行的字搶給上一段）。本卡把該臂的歸屬期望改為
//! 「以**兩行墨跡相鄰邊的中點**為界」（仍保留 U4f 的本意＝非死區斷言）。燈 3(c)
//! 正逆互逆臂、燈 4 粗尺零擾動臂、t4aa 全部**零改動**（取樣皆在行框頂＋小於半拍
//! 內，判制換軸不動它）。
//!
//! # 鑑別力（BUGGY 回塞鎖口徑，比照 `U4F_CARET_AT_BUGGY`）
//! - `U4M_LINEBOX_BUGGY=1`：反查整支退回 U4f 修後的「小數部先丟、跨度起自行框頂」
//!   判制（修本卡前的逐位元組舊態）⇒ **燈 1／燈 2／燈 3／燈 6 必紅**（行框頂以上
//!   那一帶成批改判上一行／上一段；四燈各自內建「BUGGY 態未紅＝靶失效」自證）。
//! - 燈 4（命中面不擴大）與燈 5（正向腿 y 護欄）在 BUGGY 態**恆綠**：兩燈護的是
//!   「不得吞表格帶／無主空帶」與「y 腿與發射同源」，與行框歸屬正交（記帳於各燈）。
//! - 四地形：「用戶實錄態」（12pt／1.4＝粗帶尺，1 帶＝1 行）、「寬行距態」
//!   （12pt／2.0＝行間真有空白，鎖『空白歸最近墨跡行』）、「細帶尺態」（比照 u4f：
//!   0.69285715pt／帶、23 帶／行）、「混級態」（12pt／26pt 交錯，逐段 `size_pt` ＋
//!   逐段帶數，鎖墨跡尺隨段級聯字級走）。用戶抱怨發生在粗尺態，只測細尺態會假綠。

use quire_layout::shaping::ink_em_ratios;
use quire_layout::table::{CellBlock, CellInput, RowInput, TableInput};
use quire_layout::{CaretPos, LayoutEngine, LayoutParams, ParaGrid, TableSlot, TextMeasurer};

fn buggy() -> bool {
    std::env::var_os("U4M_LINEBOX_BUGGY").is_some()
}

/// 混級態的大字級（比照 u4i 燈 3(b) 的混級段口徑）。
const BIG_FS: f32 = 26.0;
/// 混級態每行帶數（**全段統一** 3 帶＝50.4pt 拍節）：26pt 字級的墨跡全高
/// `asc＋desc＝1.448 em＝37.65pt`，相鄰兩行（12pt／26pt 任意搭配）的行距必須
/// ≥ 37.65pt 才放得下彼此——故小字級段也要佔 3 帶，否則大小相鄰行的墨跡真的
/// 重疊，『每行墨跡自屬』契約在幾何上不可能成立（燈 0 的地形自證會直接紅）。
const MIX_BANDS: u32 = 3;

/// 版面地形（四態同考）。
#[derive(Clone, Copy)]
struct Terrain {
    name: &'static str,
    fs: f32,
    ls: f32,
    /// 逐段每行帶數基準（`ParaGrid::bands`；1＝粗帶尺＝用戶實錄態）。
    bands: u32,
    /// true＝奇數段走 `BIG_FS`（段級聯字級混排；`ParaGrid::size_pt` 逐段宣告）。
    mixed: bool,
}

/// 用戶實錄態：12pt／行距 1.4 ⇒ 行距 16.8pt、帶尺 16.8pt（每行恰 1 帶）。
const USER: Terrain = Terrain {
    name: "用戶實錄態(12pt/1.4/粗帶尺)",
    fs: 12.0,
    ls: 1.4,
    bands: 1,
    mixed: false,
};
/// 寬行距態：帶尺＝行距＝24pt > 12pt 字級的墨跡全高 17.38pt ⇒ 相鄰行墨跡之間
/// **真有 6.6pt 空白**，本態是「行間空白歸最近墨跡行」這條 Tripwire 的唯一可判地形。
const WIDE: Terrain = Terrain {
    name: "寬行距態(12pt/2.0/行間有空白)",
    fs: 12.0,
    ls: 2.0,
    bands: 1,
    mixed: false,
};
/// 細帶尺態（比照 u4f：0.69285715pt／帶、每行 23 帶＝用戶檔實測 para_band；
/// 23 帶＝15.936pt 恰放得下 11pt 字級的 15.93pt 墨跡）。
const FINE: Terrain = Terrain {
    name: "細帶尺態(0.693pt/帶/23帶每行)",
    fs: 11.0,
    ls: 0.062987015,
    bands: 23,
    mixed: false,
};
/// 混級態：12pt／26pt 逐段交錯，墨跡上下緣必須逐段取 `ParaGrid::size_of`（正向腿
/// 字級同源）；全段統一 3 帶行距（見 `MIX_BANDS`）。
const MIX: Terrain = Terrain {
    name: "混級態(12pt/26pt 逐段交錯/3帶行距)",
    fs: 12.0,
    ls: 1.4,
    bands: MIX_BANDS,
    mixed: true,
};
const TERRAINS: [Terrain; 4] = [USER, WIDE, FINE, MIX];

impl Terrain {
    /// 段級聯字級（混級態奇數段＝26pt）。
    fn size(&self, para: usize) -> f32 {
        if self.mixed && para % 2 == 1 {
            BIG_FS
        } else {
            self.fs
        }
    }
    /// 段級聯每行帶數。
    fn bands_of(&self, _para: usize) -> u32 {
        self.bands
    }
}

fn params_of(t: Terrain) -> LayoutParams {
    LayoutParams {
        page_w_pt: 612.0,
        page_h_pt: 792.0,
        margins_pt: [72.0; 4],
        font_size_pt: t.fs,
        line_spacing: t.ls,
        footnote_reserve_pt: 0.0,
        footnote_reserves_pt: Vec::new(),
        endnote_reserves_pt: Vec::new(),
        header_pushes_pt: Vec::new(),
    }
}

fn grid_of(t: Terrain, n: usize) -> ParaGrid {
    ParaGrid {
        size_pt: if t.mixed {
            (0..n).map(|i| t.size(i)).collect()
        } else {
            Vec::new()
        },
        bands: (0..n).map(|i| t.bands_of(i)).collect(),
        break_before: Vec::new(),
        before_bands: Vec::new(),
        after_bands: Vec::new(),
        advance_scale: Vec::new(),
        punct_arm: Vec::new(),
    }
}

fn engine_of(t: Terrain, texts: &[String]) -> LayoutEngine {
    let n = texts.len();
    LayoutEngine::with_breaks_grid(
        texts,
        params_of(t),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        grid_of(t, n),
    )
}

/// 正文行框幾何（**全部取自引擎自身印表**：`line_bands`／`Line::col`／`ParaGrid`）。
#[derive(Clone, Copy)]
struct Box0 {
    page: usize,
    /// 該頁 `lines` 的陣標。
    li: usize,
    para: usize,
    col: u8,
    /// 起帶＝**行框頂帶**（發射 y ＝ margin_top ＋ band × line_h）。
    band: u32,
    /// 該段每行帶數。
    span: u32,
    /// 該段級聯字級（`ParaGrid::size_of`；墨跡尺同源）。
    size: f32,
}

/// 掃出全文件正文行框（每頁每行一條，順序＝引擎印表序）。
fn body_boxes(eng: &LayoutEngine, t: Terrain) -> Vec<Box0> {
    let mut out = Vec::new();
    for pg in eng.pages() {
        for (li, l) in pg.lines.iter().enumerate() {
            out.push(Box0 {
                page: pg.index,
                li,
                para: l.para_index,
                col: l.col,
                band: pg.line_bands.get(li).copied().unwrap_or(li as u32),
                span: t.bands_of(l.para_index),
                size: t.size(l.para_index),
            });
        }
    }
    out
}

/// 帶單位下某行的**墨跡上下緣**（考卷端獨立實作；公式＝卡面修法書面語義，
/// 吃 `shaping::ink_em_ratios`（同一支內嵌字型度量）與引擎印出的 `line_bands`，
/// 不吃被測反查碼）。`b` ＝行框頂帶、`size` ＝段級聯字級、`lh` ＝帶尺（pt／帶）。
fn ink_edges(b: f32, size: f32, lh: f32) -> (f32, f32) {
    let (asc_em, desc_em) = ink_em_ratios();
    if lh <= 0.0 {
        return (b, b + 1.0);
    }
    (
        b + (size - asc_em * size) / lh,
        b + (size + desc_em * size) / lh,
    )
}

/// 同欄相鄰兩行（`k` 在上、`i` 在下）的切分點（考卷端獨立實作；上下兩行用同一
/// 運算式 ⇒ 首尾相接）。中點夾到「不越下行行框頂」。
fn ink_split(k: &Box0, i: &Box0, lh: f32) -> f32 {
    let (_, bot_k) = ink_edges(k.band as f32, k.size, lh);
    let (top_i, _) = ink_edges(i.band as f32, i.size, lh);
    let mid = (bot_k + top_i) / 2.0;
    let hi = i.band as f32;
    if mid >= hi {
        hi
    } else if mid <= k.band as f32 {
        k.band as f32
    } else {
        mid
    }
}

/// 某行的命中中介帶 `[lo, hi)`（考卷端獨立實作；欄內首行上界退 half-pitch、
/// 末行下界沿用 `band＋span`＝修前覆蓋）。
fn hit_interval(boxes: &[Box0], idx: &[usize], j: usize, lh: f32) -> (f32, f32) {
    let i = idx[j];
    let b = boxes[i].band as f32;
    let span = boxes[i].span as f32;
    let prev = (0..j)
        .rev()
        .find(|&k| boxes[idx[k]].col == boxes[i].col)
        .map(|k| idx[k]);
    let next = (j + 1..idx.len())
        .find(|&k| boxes[idx[k]].col == boxes[i].col)
        .map(|k| idx[k]);
    let (my_top, _) = ink_edges(b, boxes[i].size, lh);
    let lo = match prev {
        Some(k) => ink_split(&boxes[k], &boxes[i], lh),
        None => {
            let pitch = next.map_or(span, |jx| {
                (ink_edges(boxes[jx].band as f32, boxes[jx].size, lh).0 - b).max(1.0)
            });
            (my_top - pitch / 2.0).min(b)
        }
    };
    let hi = match next {
        Some(jx) => ink_split(&boxes[i], &boxes[jx], lh),
        None => b + span,
    };
    (lo, hi)
}

/// 行框歸屬的**考卷端獨立實作**（卡面修法書面語義；吃引擎印出的 `line_bands`／
/// `Line::col`，不吃被測反查碼）——兩側不同調 ⇒ 本檔紅。
///
/// `bd` ＝ 點擊的**小數**帶商（`(y − margin_top)／帶尺`，未 floor）。
fn slot_owner(boxes: &[Box0], page: usize, bd: f32, lh: f32) -> Option<usize> {
    let idx: Vec<usize> = (0..boxes.len())
        .filter(|&i| boxes[i].page == page)
        .collect();
    for j in 0..idx.len() {
        let (lo, hi) = hit_interval(boxes, &idx, j, lh);
        if bd >= lo && bd < hi {
            return Some(boxes[idx[j]].para);
        }
    }
    None
}

/// 命中邊界的**不可判窄帶**（帶）：0.002 帶 ≈ 0.03～0.05pt，低於渲染精度。點擊恰
/// 落在兩行切分點上時「兩行都對」＝沒有唯一正解，考卷不判這種點（引擎與考卷各自
/// 的 f32 往返也可能把它推到切分點另一側，判了會假紅）。
const TIE_BAND: f32 = 0.002;

/// `bd` 是否恰落在某條中介帶邊界（＝切分點）的不可判窄帶內。
fn at_boundary(boxes: &[Box0], page: usize, bd: f32, lh: f32) -> bool {
    let idx: Vec<usize> = (0..boxes.len())
        .filter(|&i| boxes[i].page == page)
        .collect();
    for j in 0..idx.len() {
        let (lo, hi) = hit_interval(boxes, &idx, j, lh);
        if (bd - lo).abs() < TIE_BAND || (bd - hi).abs() < TIE_BAND {
            return true;
        }
    }
    false
}

/// 用戶形態 fixture：n 段、每段多行（頁內必然出現「上一行屬別段」的靶邊界）。
fn multi_para_texts(n: usize, rep: usize) -> Vec<String> {
    (0..n)
        .map(|i| format!("第{i}段：{}內容", "中文文字內容文字".repeat(rep)))
        .collect()
}

/// 帶尺（pt／帶）。
fn line_h(t: Terrain) -> f32 {
    t.fs * t.ls
}

/// 靶：某頁某行的**行框頂** y（pt）。
fn top_y(margin_top: f32, band: u32, lh: f32) -> f32 {
    margin_top + band as f32 * lh
}

// ---------------------------------------------------------------------------
// 燈 0（前提自證）：四地形都必須真的多行、且**本行墨跡上緣越過自己的行框頂**並
// 落在上一行的舊跨度內（＝跨度制缺陷發生的地形）；另自證中線制橫穿墨跡（＝run 85
// /88 判制作廢的幾何事實）。地形失效直接紅，不給假綠。
// ---------------------------------------------------------------------------

#[test]
fn u4m_lamp0_fixture_terrain_is_the_defect_ground() {
    for t in TERRAINS {
        let lh = line_h(t);
        let texts = multi_para_texts(6, 12);
        let eng = engine_of(t, &texts);
        let boxes = body_boxes(&eng, t);
        let pg0: Vec<&Box0> = boxes.iter().filter(|b| b.page == 0).collect();
        assert!(
            pg0.len() > texts.len(),
            "{}：每段必須折多行（頁0 僅 {} 行／{} 段）",
            t.name,
            pg0.len(),
            texts.len()
        );
        assert!(
            pg0.windows(2).any(|w| w[0].para != w[1].para),
            "{}：靶缺失——頁0 無跨段行邊界",
            t.name
        );
        assert!(
            pg0.windows(2).any(|w| w[0].para == w[1].para),
            "{}：段內必須有續行（正逆互逆與同段掃描的地形）",
            t.name
        );
        // 跨段邊界行：本行墨跡上緣必須越過自己的行框頂（＝字形畫在行框頂之上，
        // 前提反轉的幾何自證），且該上緣仍落在上一行的舊跨度 [prev.band,
        // prev.band＋span) 內 ⇒ 修前跨度制把本行的字判給上一段。
        let k = pg0
            .windows(2)
            .position(|w| w[0].para != w[1].para)
            .expect("前置斷言已保證")
            + 1;
        let (prev, cur) = (pg0[k - 1], pg0[k]);
        let (ink_top, _) = ink_edges(cur.band as f32, cur.size, lh);
        assert!(
            ink_top < cur.band as f32,
            "{}：本行墨跡上緣（帶 {:.3}）未越過行框頂（帶 {:.1}）——前提反轉不成立，本卡靶失效",
            t.name,
            ink_top,
            cur.band
        );
        assert!(
            ink_top >= prev.band as f32 && ink_top < prev.band as f32 + prev.span as f32,
            "{}：本行墨跡上緣（帶 {:.3}）未落在上一行舊跨度 [{}, {})——地形非缺陷態",
            t.name,
            ink_top,
            prev.band,
            prev.band + prev.span
        );
        // 行距中線制的作廢佐證：中線（相鄰兩行框頂之中點）落在**上排那行**的墨跡
        // 之內 ⇒ 中線制必然把上排字形下半判給下一行（PM 逐行復算 38/38 的幾何成因；
        // 中線制把「上排墨跡下緣～中線」那一塊讓給下排）。頁0 至少一對相鄰行要
        // 真的踩到這個幾何，否則本卡對中線制的作廢沒有本地證據。
        let mut midline_hits = 0usize;
        for w in pg0.windows(2) {
            let (up, dn) = (w[0], w[1]);
            let (u_top, u_bot) = ink_edges(up.band as f32, up.size, lh);
            let mid = (up.band as f32 + dn.band as f32) / 2.0;
            if mid > u_top && mid < u_bot {
                midline_hits += 1;
            }
        }
        assert!(
            midline_hits >= 1,
            "{}：無任何相鄰行的行距中線橫穿墨跡——中線制作廢理由缺乏本地證據",
            t.name
        );
        // 本行墨跡下緣不得越入下一行行框頂之上（四地形的行距都 ≥ em 盒高－容差；
        // 若此斷言紅＝地形行距太緊、墨跡真的疊字，契約不可判，須換地形）。
        if let Some(nx) = pg0.get(k + 1) {
            let (_, my_bot) = ink_edges(cur.band as f32, cur.size, lh);
            assert!(
                my_bot <= nx.band as f32,
                "{}：本行墨跡下緣（帶 {:.3}）越入下一行行框頂（帶 {:.1}）——行距過緊，靶失效",
                t.name,
                my_bot,
                nx.band
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 燈 1（Tripwire 主燈，鑑別燈）：**每一行自身墨跡區間內任一點**（含行框頂以上、
// 基線上下兩側）反查歸屬＝該行。樣本取墨跡上緣／下緣＋行框頂＋基線＋四等分內點，
// 每一行都掃。BUGGY 態全批紅＝用戶「無法任意按 Enter 創建新段落」。
// ---------------------------------------------------------------------------

#[test]
fn u4m_lamp1_own_ink_always_resolves_to_own_paragraph() {
    for t in TERRAINS {
        let lh = line_h(t);
        let texts = multi_para_texts(24, 8);
        let eng = engine_of(t, &texts);
        let boxes = body_boxes(&eng, t);

        let mut total = 0usize;
        let mut bad = Vec::new();
        for b in &boxes {
            let pg = eng.pages().get(b.page).expect("頁必存在");
            let x = pg.margin_left_pt + 100.0;
            let (it, ib) = ink_edges(b.band as f32, b.size, lh);
            // 取樣：墨跡上下緣＋0.05 帶內縮、行框頂、基線（行框頂＋字級）、
            // 墨跡內五等分（涵蓋行框頂以上與基線以下兩側）。
            let mut dys: Vec<f32> = vec![
                (it - b.band as f32) * lh + 0.05 * lh,
                (ib - b.band as f32) * lh - 0.05 * lh,
                0.0,
                b.size,
            ];
            for q in 1..5 {
                dys.push((it - b.band as f32) * lh + (ib - it) * lh * (q as f32 / 5.0));
            }
            for dy in dys {
                let y = top_y(pg.margin_top_pt, b.band, lh) + dy;
                if y < pg.margin_top_pt {
                    continue;
                }
                total += 1;
                match eng.caret_at(b.page, x, y) {
                    Some(c) if c.para == b.para => {}
                    other => bad.push(format!(
                        "{} 頁{} 段{}(帶{} {}pt) dy={:+.2} → {:?}",
                        t.name,
                        b.page,
                        b.para,
                        b.band,
                        b.size,
                        dy,
                        other.map(|c| (c.para, c.cp))
                    )),
                }
            }
        }
        assert!(total >= 150, "{}：墨跡取樣不足（實得 {}）", t.name, total);
        if buggy() {
            assert!(
                !bad.is_empty(),
                "燈 1 在 U4M_LINEBOX_BUGGY 態未紅＝靶地形失效（回塞鎖失去鑑別力）"
            );
        }
        assert!(
            bad.is_empty(),
            "燈 1 點到自己墨跡卻判別段 {}／{} 例（BUGGY 態點名：錯判上一段段尾）：\n{}",
            bad.len(),
            total,
            bad.iter().take(8).cloned().collect::<Vec<_>>().join("\n")
        );
    }
}

// ---------------------------------------------------------------------------
// 燈 2（卡面考題 1 ＋ Tripwire『空白歸最近墨跡行』，鑑別燈）：
//  (a) 全欄從首行墨跡上緣到末行行框頂＋跨度，逐 0.25 帶掃 y ⇒ 反查必與考卷端行框
//      模型全等（零死區、零錯判）。
//  (b) 相鄰兩行之間的**空白帶**（上排墨跡下緣～下排墨跡上緣）取樣，反查段必屬
//      「離哪邊墨跡近就歸哪邊」——直接幾何判定，不走模型（非循環論證）。
// ---------------------------------------------------------------------------

#[test]
fn u4m_lamp2_vertical_scan_restores_ink_truth() {
    for t in TERRAINS {
        let lh = line_h(t);
        let texts = multi_para_texts(24, 8);
        let eng = engine_of(t, &texts);
        let boxes = body_boxes(&eng, t);
        assert!(!boxes.is_empty(), "{}：fixture 必須有正文行", t.name);

        // (a) 全欄逐點掃 y：與考卷端模型逐點對帳。
        let mut total = 0usize;
        let mut bad = Vec::new();
        for pg in eng.pages() {
            let idx: Vec<usize> = (0..boxes.len())
                .filter(|&i| boxes[i].page == pg.index)
                .collect();
            let lo_first = hit_interval(&boxes, &idx, 0, lh).0;
            let hi_last = hit_interval(&boxes, &idx, idx.len() - 1, lh).1;
            let mut bd = lo_first + 0.05;
            while bd < hi_last - 0.05 {
                // 恰在切分點窄帶內的取樣無唯一正解（見 `at_boundary`），跳過不判。
                if !at_boundary(&boxes, pg.index, bd, lh) {
                    let y = pg.margin_top_pt + bd * lh;
                    total += 1;
                    let want = slot_owner(&boxes, pg.index, bd, lh);
                    let got = eng
                        .caret_at(pg.index, pg.margin_left_pt + 100.0, y)
                        .map(|c| c.para);
                    if got != want {
                        bad.push(format!(
                            "{} 頁{} 帶{:.2}：反查 {:?} ≠ 行框墨跡模型 {:?}",
                            t.name, pg.index, bd, got, want
                        ));
                    }
                }
                bd += 0.25;
            }
        }
        assert!(total >= 150, "{}：掃 y 樣本不足（實得 {}）", t.name, total);
        assert!(
            bad.is_empty(),
            "燈 2(a) 逐點掃 y 破口 {}／{} 例：\n{}",
            bad.len(),
            total,
            bad.iter().take(10).cloned().collect::<Vec<_>>().join("\n")
        );

        // (b) 行間空白歸最近墨跡行（直接幾何）。
        let mut gap_total = 0usize;
        let mut gap_bad = Vec::new();
        for pg in eng.pages() {
            let idx: Vec<usize> = (0..boxes.len())
                .filter(|&i| boxes[i].page == pg.index)
                .collect();
            for j in 0..idx.len().saturating_sub(1) {
                let (up, dn) = (boxes[idx[j]], boxes[idx[j + 1]]);
                if up.col != dn.col || dn.para == up.para {
                    continue; // 只判跨段邊界的行間空白（用戶症狀形態）
                }
                let (_, ub) = ink_edges(up.band as f32, up.size, lh);
                let (dt, _) = ink_edges(dn.band as f32, dn.size, lh);
                if dt <= ub {
                    continue; // 墨跡相接／重疊＝無空白可判（寬行距態才有）
                }
                for q in 1..6 {
                    let bd = ub + (dt - ub) * (q as f32 / 6.0);
                    // 恰在兩行切分點（＝空白正中，兩行都算「最近」）的樣本無唯一
                    // 正解，不判（f32 往返也會把它推到任一側）。
                    if (bd - (ub + dt) / 2.0).abs() < TIE_BAND {
                        continue;
                    }
                    let y = pg.margin_top_pt + bd * lh;
                    gap_total += 1;
                    let got = eng
                        .caret_at(pg.index, pg.margin_left_pt + 100.0, y)
                        .map(|c| c.para);
                    // 離上排墨跡下緣近 ⇒ 上排段；反之 ⇒ 下排段。
                    let want = if (bd - ub) < (dt - bd) {
                        up.para
                    } else {
                        dn.para
                    };
                    if got != Some(want) {
                        gap_bad.push(format!(
                            "{} 頁{} 帶{:.2}（空白 [{:.2},{:.2}]）：反查 {:?} ≠ 最近墨跡 {:?}",
                            t.name, pg.index, bd, ub, dt, got, want
                        ));
                    }
                }
            }
        }
        if gap_total > 0 {
            assert!(
                gap_bad.is_empty(),
                "燈 2(b) 行間空白未歸最近墨跡行 {}／{} 例：\n{}",
                gap_bad.len(),
                gap_total,
                gap_bad
                    .iter()
                    .take(8)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
        // 寬行距態必須真的提供空白樣本（否則本 Tripwire 沒被測到）。
        if t.name == WIDE.name {
            assert!(
                gap_total >= 20,
                "{}：行間空白樣本不足（實得 {}）——『空白歸最近墨跡行』未被測到",
                t.name,
                gap_total
            );
        }
        if buggy() {
            assert!(
                !bad.is_empty(),
                "燈 2 在 U4M_LINEBOX_BUGGY 態未紅＝靶地形失效（回塞鎖失去鑑別力）"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 燈 3（卡面考題 2，鑑別燈；口徑依 PM 裁示改以 **ink 邊界**表述）：
//  (a) `caret_rect(p,cp)` 的行框頂 y 本身＋y 以下（仍在自身墨跡內）⇒ 必回同段；
//  (b) 越過墨跡切分邊界（切分點再越 0.3 帶）⇒ 必換到模型屬主的段（且不得回 None：
//      邊界外一線仍是行框，不是死區）；
//  (c) 換段只走一格（歸屬段只能是本行段或同欄鄰行段）——與「上一段段尾」這種
//      跨段錯判劃清界線。
//  （原稿「caret_rect 的 y 上下 ±(pitch/2−1) 仍回同段」依反轉後前提作廢：粗帶尺
//   態 y−(pitch/2−1) 落在上一行的**墨跡**上，墨跡真值制下正確答案就是上一行。）
// ---------------------------------------------------------------------------

#[test]
fn u4m_lamp3_inverse_symmetry_around_ink_boundary() {
    for t in TERRAINS {
        let lh = line_h(t);
        let texts = multi_para_texts(24, 8);
        let eng = engine_of(t, &texts);
        let boxes = body_boxes(&eng, t);

        let mut same_side = 0usize;
        let mut cross_side = 0usize;
        let mut bad = Vec::new();
        let mut wild = Vec::new();
        for (pi, tx) in texts.iter().enumerate() {
            let n = tx.chars().count();
            for cp in [0usize, n / 2, n.saturating_sub(1)] {
                let Some(rect) = eng.caret_rect(CaretPos { para: pi, cp }) else {
                    continue;
                };
                let pg = eng.pages().get(rect.page).expect("頁必存在");
                let bd = (rect.y_pt - pg.margin_top_pt) / lh;
                let Some(b) = boxes
                    .iter()
                    .find(|x| x.page == rect.page && (x.band as f32 - bd).abs() < 0.01)
                else {
                    continue;
                };
                let idx: Vec<usize> = (0..boxes.len())
                    .filter(|&i| boxes[i].page == b.page)
                    .collect();
                let j = idx
                    .iter()
                    .position(|&i| boxes[i].li == b.li && boxes[i].page == b.page)
                    .expect("本行必在頁內");
                let (lo, hi) = hit_interval(&boxes, &idx, j, lh);
                let (it, ib) = ink_edges(b.band as f32, b.size, lh);
                let x = rect.x_pt;
                // (a) 自身墨跡內（含行框頂以上）與 y 本身：必回同段。
                for dy in [0.0f32, b.size, (it - b.band as f32) * lh + 0.05 * lh] {
                    let y = rect.y_pt + dy;
                    if y < pg.margin_top_pt {
                        continue;
                    }
                    same_side += 1;
                    match eng.caret_at(rect.page, x, y) {
                        Some(c) if c.para == pi => {}
                        other => bad.push(format!(
                            "{} 墨跡內：段{} cp{} y偏移 {:+.2} → {:?}",
                            t.name,
                            pi,
                            cp,
                            dy,
                            other.map(|c| (c.para, c.cp))
                        )),
                    }
                }
                // (b) 越過墨跡切分邊界（上界 lo 再上 0.3 帶／下界 hi 再下 0.3 帶）：
                //     必換到模型屬主的段，且不得回 None。
                for (yb, tag) in [
                    (pg.margin_top_pt + (lo - 0.3) * lh, "上界外"),
                    (pg.margin_top_pt + (hi + 0.3) * lh, "下界外"),
                ] {
                    if yb < pg.margin_top_pt {
                        continue;
                    }
                    cross_side += 1;
                    let want = slot_owner(&boxes, rect.page, (yb - pg.margin_top_pt) / lh, lh);
                    let got = eng.caret_at(rect.page, x, yb).map(|c| c.para);
                    if got != want {
                        bad.push(format!(
                            "{} {}：段{} cp{} 帶{:.2} → {:?} ≠ 墨跡模型 {:?}",
                            t.name, tag, pi, cp, lo, got, want
                        ));
                    }
                }
                // (c) 歸屬不野跳：行框頂以上一個 half-pitch 內的反查，段號只能是
                //     本行段或同欄鄰行段。
                let near = boxes
                    .iter()
                    .filter(|x| x.page == b.page && x.col == b.col)
                    .map(|x| x.para)
                    .collect::<Vec<_>>();
                for dy in [-(lh * 0.9), lh * 0.9, -(lh * 1.6), lh * 1.6] {
                    let y = rect.y_pt + dy;
                    if y < pg.margin_top_pt {
                        continue;
                    }
                    if let Some(c) = eng.caret_at(rect.page, x, y) {
                        if !near.contains(&c.para) {
                            wild.push(format!(
                                "{} 野跳：段{} cp{} dy={:+.2} → 段{}（同欄行段集 {:?}）",
                                t.name, pi, cp, dy, c.para, near
                            ));
                        }
                    }
                }
                let _ = ib;
            }
        }
        assert!(
            same_side >= 48,
            "{}：墨跡內樣本不足（實得 {}）",
            t.name,
            same_side
        );
        assert!(
            cross_side >= 48,
            "{}：跨界樣本不足（實得 {}）",
            t.name,
            cross_side
        );
        assert!(
            wild.is_empty(),
            "燈 3(c) 歸屬野跳（跨過相鄰行以外的段）{} 例：\n{}",
            wild.len(),
            wild.iter().take(8).cloned().collect::<Vec<_>>().join("\n")
        );
        if buggy() {
            assert!(
                !bad.is_empty(),
                "燈 3 在 U4M_LINEBOX_BUGGY 態未紅＝靶地形失效（回塞鎖失去鑑別力）"
            );
        }
        assert!(
            bad.is_empty(),
            "燈 3 墨跡邊界互逆破口 {} 例：\n{}",
            bad.len(),
            bad.iter().take(10).cloned().collect::<Vec<_>>().join("\n")
        );
    }
}

// ---------------------------------------------------------------------------
// 燈 4（卡面考題 4，命中面不擴大）：表格帶／空帶仍必回 None。
// 【記帳】BUGGY 態恆綠：本燈護「新歸屬制不得吞表格帶／無主空帶」，與修前的
// 跨度判制在此同向（護欄燈，非鑑別燈）。
// ---------------------------------------------------------------------------

#[test]
fn u4m_lamp4_table_and_empty_bands_stay_none() {
    for t in TERRAINS {
        let p = params_of(t);
        let lh = line_h(t);
        // 5 列 × 40pt 列高（列高遠大於格內文字行高）⇒ 表格帶群大於格內行帶群。
        let input = TableInput {
            grid_pt: vec![200.0],
            rows: (0..5)
                .map(|i| RowInput {
                    cells: vec![CellInput {
                        blocks: vec![CellBlock::Text(format!("儲存格{i}"))],
                        ..Default::default()
                    }],
                    height_pt: Some(40.0),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let mut texts = vec!["表格前的正文段。".repeat(4)];
        texts.extend((0..5).map(|i| format!("儲存格{i}")));
        texts.push("表格後的正文段。".to_string());
        let m = TextMeasurer::new(p.font_size_pt);
        let tl = quire_layout::table::layout_table(&input, p.content_w_pt(), lh, &m, 0);
        let slot = TableSlot {
            para_start: 1,
            para_end: 1 + 5,
            layout: tl,
        };
        let n = texts.len();
        let eng = LayoutEngine::with_breaks_grid(
            &texts,
            p.clone(),
            Vec::new(),
            vec![slot.clone()],
            Vec::new(),
            Vec::new(),
            grid_of(t, n),
        );
        let pg = eng.pages().first().expect("fixture 必有頁");
        let tp = pg.tables.first().expect("頁內必有表格切片");
        let rows_h: f32 = tp.frag.rows.iter().map(|r| r.height_pt).sum();
        let rows_bands = (rows_h / lh).ceil() as u32;
        assert!(
            rows_bands >= 3,
            "{}：表格必須真的吃掉一批帶（實得 {}）",
            t.name,
            rows_bands
        );

        // 格內段的正向腿 y（T4AA 已產品化）：落在格內行跨度者屬格內腿，本燈不
        // 觸碰；其餘表格帶是「無正文行」的帶群——正文反查一律不得命中。
        // 判定採「命中段是否屬表格段號區間 [para_start, para_end)」：屬之＝格內
        // 腿的正當命中（T4AA 考卷把關，本燈不碰）；屬正文段號 ⇒ 正文反查吞掉表格
        // 帶（命中面擴大＝搶走格內打字）。此判準與地形帶尺無關，比舊的「離格內 y
        // 小於 6pt 就跳過」穩健（寬行距／混級態的取樣格距會變，舊容差會誤判）。
        let mut probed = 0usize;
        let mut leak = Vec::new();
        for k in 0..rows_bands {
            let y = pg.margin_top_pt + (tp.band + k) as f32 * lh + lh * 0.25;
            probed += 1;
            if let Some(c) = eng.caret_at(pg.index, pg.margin_left_pt + 100.0, y) {
                if !(slot.para_start..slot.para_end).contains(&c.para) {
                    leak.push((tp.band + k, c.para));
                }
            }
        }
        assert!(probed >= 3, "{}：表格帶樣本不足（實得 {}）", t.name, probed);
        assert!(
            leak.is_empty(),
            "{}：燈 4(a) 正文反查吞掉表格帶 {} 例（命中面擴大＝搶走格內打字）：{:?}",
            t.name,
            leak.len(),
            leak.iter().take(8).collect::<Vec<_>>()
        );

        // (b) 空帶：末行下緣以遠的頁底空白仍 None（末行下界沿用修前口徑）。
        let one = vec!["只有這一頁這一段。".repeat(4)];
        let eng1 = engine_of(t, &one);
        let pg1 = eng1.pages().first().expect("首頁必存在");
        let last = *pg1.line_bands.last().expect("首頁必有行");
        let deep = pg1.margin_top_pt + (last as f32 + t.bands_of(0) as f32 + 1.5) * lh;
        assert!(
            eng1.caret_at(pg1.index, pg1.margin_left_pt + 60.0, deep)
                .is_none(),
            "{}：燈 4(b) 末行下緣以遠的頁底空白必 None（深處 {:.1}pt）",
            t.name,
            deep
        );
    }
}

/// 燈 4 判定「格內行命中面」的容差（pt）：已改採段號區間判準（見燈 4(a)），
/// 保留常數供後續需要 y 容差比對時使用。
#[allow(dead_code)]
const FINE_AWAY: f32 = 6.0;

// ---------------------------------------------------------------------------
// 燈 5（正向腿護欄）：`caret_rect` 的 y 仍恰為某條行的**行框頂帶**換算值（發射
// 同尺），且 caret_rect→caret_at(y＋0.35×帶尺) 互逆仍還原同段（u4f 燈 3(c) 同調）。
// 【記帳】BUGGY 態恆綠：本燈護 y 腿與發射同源，與行框歸屬正交。
// ---------------------------------------------------------------------------

#[test]
fn u4m_lamp5_forward_leg_y_untouched() {
    for t in TERRAINS {
        let lh = line_h(t);
        let texts = multi_para_texts(6, 8);
        let eng = engine_of(t, &texts);
        let boxes = body_boxes(&eng, t);
        let mut checked = 0usize;
        for (pi, tx) in texts.iter().enumerate() {
            let n = tx.chars().count();
            for cp in [0usize, n / 2] {
                let Some(rect) = eng.caret_rect(CaretPos { para: pi, cp }) else {
                    continue;
                };
                let mt = eng.pages()[rect.page].margin_top_pt;
                let bd = (rect.y_pt - mt) / lh;
                assert!(
                    boxes
                        .iter()
                        .any(|b| b.page == rect.page && (b.band as f32 - bd).abs() < 0.01),
                    "{}：段{} cp{} caret y {:.3} 非任何行的行框頂帶換算值（y 腿被動壞？）",
                    t.name,
                    pi,
                    cp,
                    rect.y_pt
                );
                checked += 1;
                let back = eng.caret_at(rect.page, rect.x_pt, rect.y_pt + lh * 0.35);
                assert_eq!(
                    back.map(|c| c.para),
                    Some(pi),
                    "{}：段{} cp{} caret_rect→caret_at(y＋0.35 帶) 互逆必還原同段",
                    t.name,
                    pi,
                    cp
                );
            }
        }
        assert!(checked >= 8, "{}：正向樣本不足（實得 {}）", t.name, checked);
    }
}

// ---------------------------------------------------------------------------
// 燈 6（用戶可見結果，鑑別燈）：點擊段 ↔ caret 段全等——用戶「按 Enter 分錯段」
// 的根因就是 caret 段 ≠ 點擊段。取樣＝跨段邊界行的**墨跡全區**（行框頂以上與
// 基線兩側）。另做同源對帳：caret 的 cp 必落在點擊行自身的碼點區間內（正逆兩腿
// 同一份行文字，不是憑空落到別段的段尾）。
// 修前（BUGGY）態：caret 落上一段段尾 ⇒ 段號與 cp 雙雙錯 ⇒ 本燈紅。
// ---------------------------------------------------------------------------

#[test]
fn u4m_lamp6_click_paragraph_equals_caret_paragraph() {
    for t in TERRAINS {
        let lh = line_h(t);
        let texts = multi_para_texts(24, 8);
        let eng = engine_of(t, &texts);
        let boxes = body_boxes(&eng, t);

        let mut total = 0usize;
        let mut bad = Vec::new();
        for (i, b) in boxes.iter().enumerate() {
            let Some(k) = i.checked_sub(1) else { continue };
            if boxes[k].page != b.page || boxes[k].para == b.para || boxes[k].col != b.col {
                continue;
            }
            let pg = eng.pages().get(b.page).expect("頁必存在");
            // 本行墨跡全區（行框頂以上那一帶是用戶實錄的錯判區）。
            let (it, ib) = ink_edges(b.band as f32, b.size, lh);
            let mut dys: Vec<f32> = (1..5)
                .map(|q| (it - b.band as f32) * lh + (ib - it) * lh * (q as f32 / 5.0))
                .collect();
            dys.push(0.0);
            dys.push(b.size);
            dys.push((it - b.band as f32) * lh + 0.05 * lh);
            for dy in dys {
                let y = top_y(pg.margin_top_pt, b.band, lh) + dy;
                if y < pg.margin_top_pt {
                    continue;
                }
                total += 1;
                let Some(c) = eng.caret_at(b.page, pg.margin_left_pt + 100.0, y) else {
                    bad.push(format!(
                        "{} 頁{} 段{} dy={:.2}：墨跡點擊反查 None（死區＝點擊無游標）",
                        t.name, b.page, b.para, dy
                    ));
                    continue;
                };
                if c.para != b.para {
                    bad.push(format!(
                        "{} 頁{} 帶{} dy={:.2}：caret 落段{}（點擊段 {}）⇒ Enter 把新段落切到兩段之間",
                        t.name, b.page, b.band, dy, c.para, b.para
                    ));
                    continue;
                }
                // 同源對帳：cp 必落在點擊行的碼點區間 [行起, 行起＋行長]（段內
                // 偏移由前行累計而來；錯判上一段段尾的舊態在此同樣露餡）。
                let line_start: usize = pg
                    .lines
                    .iter()
                    .take(b.li)
                    .filter(|l| l.para_index == b.para)
                    .map(|l| l.text.chars().count())
                    .sum();
                let line_len = pg.lines[b.li].text.chars().count();
                if c.cp < line_start || c.cp > line_start + line_len {
                    bad.push(format!(
                        "{} 頁{} 段{} dy={:.2}：cp {} 不在點擊行碼點區間 [{}, {}]",
                        t.name,
                        b.page,
                        b.para,
                        dy,
                        c.cp,
                        line_start,
                        line_start + line_len
                    ));
                }
            }
        }
        assert!(total >= 40, "{}：靶樣本不足（實得 {}）", t.name, total);
        if buggy() {
            assert!(
                !bad.is_empty(),
                "燈 6 在 U4M_LINEBOX_BUGGY 態未紅＝靶地形失效（回塞鎖失去鑑別力）"
            );
        }
        assert!(
            bad.is_empty(),
            "燈 6 點擊段 ↔ caret 段全等破口 {}／{} 例（BUGGY 態點名：Enter 分錯段）：\n{}",
            bad.len(),
            total,
            bad.iter().take(8).cloned().collect::<Vec<_>>().join("\n")
        );
    }
}
