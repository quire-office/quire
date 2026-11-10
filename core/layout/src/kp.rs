//! Knuth–Plass 最佳化斷行：在 UAX#14 atom 邊界上選「整段代價總和最小」的
//! 斷行方案（TeX 斷行骨架的務實簡化：單一 fitness class、無斷字詞典）。
//!
//! 與舊貪心（前瞻斷：塞不下就換行）的差別：KP 會把短詞提前挪去下一行、
//! 讓**各行**鬆緊度的總和最低——長文檔版面品質的根，也是 P0-6 增量重排
//! 要包住的對象（段落級 DP：O(段內斷點數 × 每行平均單元數)，段長有界）。
//!
//! 代價模型（KP 論文 badness 的簡化版；行寬 w、目標寬 W）：
//! - 普通行（段內可繼續）：deficit t=(W−w)/W；t≤0.85 時代價 (100t)³×0.1，
//!   超過視為不可用（寧可整單元推去下一行）；溢出以更陡斜率計價。
//! - 硬斷行後／段尾末行：ragged（行尾不拉伸），deficit 不計價，溢出仍計。
//! - 連字號斷行：+HY_PENALTY（偏好其他斷點，非不可越）。
//! - 兜底（緊急終斷）：當前 atom 本身比行寬還寬、所有在飛 state 全數爆寬時，
//!   把該 atom 單獨成行並加巨罰——斷行必須收斂：寧可溢出不可丟字（缺字策略
//!   同 P0-4）。緊急行恆為單 atom：uax14 構造保證可斷空白只在 atom 尾端
//!   trim 區，單 atom 行去掉尾端後必不含可斷空白（property P3 的根基）。
//!
//! ## 複雜度鐵律（run40/42/44 三次 OOM 的教訓，改動前必讀）
//! 經典 KP 的 active list 之所以有界，靠兩個不變量：
//! 1. **每個行起點 `from` 至多一個 state**：同 from 的 state 之後吸收的 atom
//!    序列完全相同、寬度軌跡相同，cost 小者嚴格支配大者——只准留最小者
//!    （支配合併）。一旦允許同 from 多 state（「收行」與「續行」各留一份
//!    且都無限繁殖），active 變成組合爆炸：n 個斷點、每行 k 個 atom ⇒
//!    active=O(k^n) 級膨脹，n≈8000 的 CJK 段直接吃掉 15GB RSS 被核心記憶體
//!    管理機制移除（12:30/12:56/12:58/12:59/15:57 五次，見 t_6c3a72ae 評論）。
//! 2. **爆寬 state 立即棄置**：續行只會更寬，爆寬 state 永不產生合法收行；
//!    唯一例外是緊急終斷路徑（見上），且緊急後從當前 atom 的下一個重開 DP，
//!    絕不讓爆寬 state 帶著巨罰長期存活。
//!
//! 由此 active ≤ 「一行寬度內的 atom 數」（451pt/12pt CJK ≈ 38），arena 每
//! 迭代至多新增 1 個 state ⇒ 時間/空間 O(atom 數)。動這條迴路前先用
//! `kp_probe`（務必 ulimit -v 包起來）確認 RSS 曲線持平。
//!
//! 緊急切分（emergency expansion）：atom 連行尾不計寬部分都超過 W 時，按其
//! shaping cluster 邊界貪心切成 ≤W 的 chunk、chunk 間設強制邊界。cluster
//! 邊界永不拆組合字元/連字（shaping.rs 的覆蓋校驗保證）——這是「任意文字
//! 不斷字」性質在緊急路徑下的最終防線。

use crate::shaping::{shape_advances_with_punct, ClusterAdvance, TextMeasurer};
use crate::uax14::{break_atoms, is_ignorable, Atom};
use crate::{Line, EOL_CONTROLS};

/// f32 累加容差：max_w 是硬約束，浮點連加會飄移。
pub(crate) const EPS: f32 = 1e-3;
/// deficit 立方代價尺度（KP badness=100t³ 的縮放）。
const BAD_SCALE: f64 = 0.1;
/// 鬆散閾值：deficit 比例超過即視為不可用。
const LOOSE_T: f32 = 0.85;
const LOOSE_COST: f64 = 1e7;
/// 連字號斷行罰分（≈deficit 44% 的代價）。
const HY_PENALTY: f64 = 3_000.0;
/// 兜底（被迫接受不可行方案）罰分：遠大於一切正常代價。
const OVERFULL_PENALTY: f64 = 1e10;

/// cluster 起點前綴和：`prefix(p)`（O(log n)）＝源文字 `text[..p]` 的
/// shaped 總寬。
pub(crate) struct PrefixWidths {
    starts: Vec<u32>,
    cum: Vec<f32>,
}

impl PrefixWidths {
    fn new(clusters: &[ClusterAdvance]) -> Self {
        let starts: Vec<u32> = clusters.iter().map(|c| c.start).collect();
        let mut cum = Vec::with_capacity(clusters.len() + 1);
        cum.push(0.0f32);
        let mut acc = 0.0f32;
        for c in clusters {
            acc += c.w;
            cum.push(acc);
        }
        Self { starts, cum }
    }

    pub(crate) fn prefix(&self, p: usize) -> f32 {
        // cluster 起點嚴格遞增（shaping 覆蓋校驗保證）；首個 start≥p 的下標
        // j ⇒ 前 j 個 cluster 完整落在 text[..p] 內。
        let j = self.starts.partition_point(|&s| (s as usize) < p);
        self.cum[j]
    }
}

#[derive(Clone, Copy)]
struct AtomW {
    /// 完整寬度（含行尾不計寬部分）。
    full: f32,
    /// 行尾不計寬部分的寬度（trim_from..end）。
    tail: f32,
}

/// 單行代價。`ragged`＝行尾不拉伸（硬斷點後/段尾末行）：deficit 免計。
pub(crate) fn line_cost(w: f32, max_w: f32, ragged: bool, hyphen: bool) -> f64 {
    let t = (max_w - w) / max_w; // ≥0 欠寬；<0 溢出
    let mut c = if t >= 0.0 {
        if ragged {
            0.0
        } else if t > LOOSE_T {
            LOOSE_COST
        } else {
            (t as f64 * 100.0).powi(3) * BAD_SCALE
        }
    } else {
        ((-t) as f64 * 100.0).powi(3) * BAD_SCALE * 2.0
    };
    if hyphen && !ragged {
        c += HY_PENALTY;
    }
    c
}

/// KP 主幹：回傳每行的 atom 區間 `(start, end)`（閉區間）。
///
/// 每步（atom 邊界 i）對每個在飛 state 三分支：
/// - **爆寬**（行寬 > max_w+EPS）：棄置，只留快照給段尾兜底（續行只會更寬，
///   合法收行機會已永久消失）。
/// - **收行**：全部收行選項同為 `from=i+1` 的新行 → 支配下只留 cost 最小者
///   一個 state（全域支配合併，見模組頭複雜度鐵律 1）。
/// - **續行**：普通斷點保留既有 state 本身（同 from、同 w，零複製）。
///
/// 不變量：active∪pending 中每個 `from` 至多一個 state ⇒ active ≤ 一行寬度
/// 內 atom 數、arena 每步至多 +1。
fn optimal_ranges(atoms: &[Atom], aw: &[AtomW], max_w: f32) -> Vec<(usize, usize)> {
    let n = atoms.len();
    debug_assert_eq!(atoms.len(), aw.len());
    if n == 0 {
        return Vec::new();
    }

    #[derive(Clone, Copy)]
    struct St {
        from: usize, // 本行首 atom
        w: f32,      // 本行已累计寬（含目前 atom）
        cost: f64,   // 本行之前各行總代價
        prev: usize, // 上一行的 state（usize::MAX＝文件開頭）
    }
    let mut arena: Vec<St> = vec![St {
        from: 0,
        w: 0.0,
        cost: 0.0,
        prev: usize::MAX,
    }];
    let mut active: Vec<usize> = vec![0];
    // 上一步的收行候選（from＝目前 i，支配合併後唯一）；爆寬全棄時給緊急路徑收尾用。
    let mut pending: Option<usize> = None;

    for i in 0..n {
        let last = i + 1 == n;
        let forced = atoms[i].forced;
        let (full, tail) = (aw[i].full, aw[i].tail);
        // 所有在飛 state 吞下本 atom 的完整寬。
        for s in active.iter_mut() {
            arena[*s].w += full;
        }

        // 掃描：記錄（不立刻寫入）最小收行選項，最後只落地一個 state。
        let mut best: Option<(f64, usize)> = None; // (cost, 收行來源 state)
        let mut alive: Vec<usize> = Vec::new();
        for &s in &active {
            let st = arena[s];
            let w = st.w - tail; // 行寬（不計行尾空白/控制）
            if w > max_w + EPS {
                continue; // 爆寬：棄置（續行只會更寬，合法收行機會已永久消失）
            }
            if !forced && !last {
                alive.push(s); // 普通斷點保留「不斷」選項（同 state，零複製）
            }
            let cost = st.cost + line_cost(w, max_w, forced || last, atoms[i].hyphen);
            if best.is_none_or(|(c, _)| cost < c) {
                best = Some((cost, s));
            }
        }
        // 收行選項落地：至多一個新 state（from=i+1，同 from 全域最小 cost）。
        let ended = best.map(|(cost, s)| {
            arena.push(St {
                from: i + 1,
                w: 0.0,
                cost,
                prev: s,
            });
            arena.len() - 1
        });

        // 全域爆寬（best=None）：atom i 本身比行寬寬、所有在飛 state 全數爆寬
        // ⇒ 緊急終斷。回溯鏈必須是 …→(從 i 起的收行 state)→(from=i+1 的緊急
        // 行)：pending 本身就是「從 i 起」的收行候選；pending=None 表示上一步
        // 就是緊急終斷（active 唯一起點 state 自 i 起），直接以它收行。
        let cur = match ended {
            Some(e) => e,
            None => {
                let s = *active
                    .iter()
                    .min_by(|a, b| arena[**a].cost.total_cmp(&arena[**b].cost))
                    .expect("active 非空 ⇒ state 存在");
                let (close_cost, close_idx) = match pending {
                    Some(p) => (arena[p].cost, p),
                    None if arena[s].from == i => (arena[s].cost, s),
                    None => unreachable!(
                        "pending=None 只出現在緊急終斷下一步：active 唯一 state 必自 i 起"
                    ),
                };
                // 緊急行＝atom i 單獨成行（uax14 構造保證單 atom 行不含可斷
                // 空白）；寬度必然超出，巨罰＋溢出立方項。
                let w_i = full - tail;
                let over = (((w_i - max_w) / max_w * 100.0) as f64).powi(3);
                arena.push(St {
                    from: i + 1,
                    w: 0.0,
                    cost: close_cost + OVERFULL_PENALTY + over,
                    prev: close_idx,
                });
                arena.len() - 1
            }
        };

        if last {
            // 回溯：state 鏈由後而前；每行起點取自 state.from，終點＝下行起點−1。
            let mut starts: Vec<usize> = Vec::new();
            let mut cur = cur;
            while cur != usize::MAX {
                starts.push(arena[cur].from);
                cur = arena[cur].prev;
            }
            starts.reverse();
            // 末項是 winner 的 `from`＝n（「下一行從 n 起」的哨兵），不是行首。
            starts.pop();
            return starts
                .iter()
                .enumerate()
                .map(|(k, &from)| {
                    let end = if k + 1 < starts.len() {
                        starts[k + 1] - 1
                    } else {
                        n - 1
                    };
                    (from, end)
                })
                .collect();
        }

        if ended.is_some() {
            active = alive;
            active.extend(ended);
            pending = ended;
        } else {
            // 緊急終斷後：新行自 i+1 重開，不留任何爆寬殘黨。
            active = vec![cur];
            pending = None;
        }
    }
    unreachable!("last 分支必 return")
}

/// 行資料：顯示行＋其源文字區間（property test 用，不對外）。
#[cfg_attr(not(test), allow(dead_code))] // range/hard_end 只被 cfg(test) 的 property test 讀
pub(crate) struct LineSpan {
    pub line: Line,
    /// 源文字位元組區間 [start, end)：行內容＝此區間去掉行尾空白與控制字元。
    pub range: (usize, usize),
    /// 此區間是否因「緊急切分/硬斷點」而必然終止（property test 豁免用）。
    pub hard_end: bool,
}

/// 文字 → 行（含區間）。性質見 lib.rs 的 property test。
pub(crate) fn kp_break_lines(text: &str, max_w: f32, m: &TextMeasurer) -> Vec<LineSpan> {
    kp_break_lines_at(text, max_w, m, &[])
}

/// 在排序去重的位元組偏移集 `fb`（恆為字元邊界、嚴格介於段首/段尾之間）
/// 處劈開 atom 並把左半標 `forced`（其後必收行）。切點若恰在 atom 邊界，
/// 只給左邻 atom 補 forced 旗標、不產生零寬 atom。切點所在 atom 的
/// `trim_from` 收進切點左側（左半主體完整保留、不計寬區不吞字）。
fn split_atoms_forced(atoms: Vec<Atom>, fb: &[usize]) -> Vec<Atom> {
    let mut out: Vec<Atom> = Vec::with_capacity(atoms.len() + fb.len());
    let mut fi = 0usize;
    'atoms: for a in atoms {
        // 切點落在本 atom 內部（start < b < end）的所有位置依序劈段。
        while fi < fb.len() && fb[fi] <= a.start {
            fi += 1; // 已越過：切點恰在 atom 左界＝前一 atom 已處理
        }
        let mut cur = a;
        while fi < fb.len() && fb[fi] < cur.end {
            let b = fb[fi];
            let left = Atom {
                start: cur.start,
                end: b,
                trim_from: cur.trim_from.min(b),
                forced: true,
                hyphen: false, // 硬斷點非連字號斷行（計價只屬軟斷）
            };
            out.push(left);
            cur = Atom {
                start: b,
                end: cur.end,
                trim_from: cur.trim_from.max(b),
                forced: false,
                hyphen: cur.hyphen,
            };
            fi += 1;
        }
        // 切點恰在 atom 右界：本 atom 標 forced（其後即斷點），零寬續段不产生。
        if fi < fb.len() && fb[fi] == cur.end {
            out.push(Atom {
                forced: true,
                ..cur
            });
            fi += 1;
            while fi < fb.len() && fb[fi] == cur.end {
                fi += 1; // 同位重複（上游已 dedup，防禦性跳過）
            }
            continue 'atoms;
        }
        out.push(cur);
    }
    out
}

/// 帶**強制斷點**的 KP 斷行（`w:br w:type="page"` 的斷行層承載）：
/// `forced_cp` 為碼點偏移，折成位元組後把所在 atom 劈開、左半標 `forced`
/// ——`optimal_ranges` 對 forced atom 必收行（硬邊界），其後文字必然另起一行。
/// `0`／段尾偏移由呼叫端（para_breaks→pack_run 的段落級硬斷）處理，此處只
/// 影響斷行形狀。空清單與 `kp_break_lines` 同路徑（golden 逐位元組不動）。
pub(crate) fn kp_break_lines_at(
    text: &str,
    max_w: f32,
    m: &TextMeasurer,
    forced_cp: &[usize],
) -> Vec<LineSpan> {
    if text.is_empty() {
        return vec![LineSpan {
            line: Line {
                text: String::new(),
                width_pt: 0.0,
                para_index: 0, // 由 layout_paragraphs_per_section 覆寫為真正段落索引
                char_offset: 0,
                col: 0, // L1-columns：欄序由 pack_run 放置時蓋章
            },
            range: (0, 0),
            hard_end: true,
        }];
    }
    // K7-cal：水平 advance 尺經 shaping 內部按 script 選擇性乘入
    // （Latin/通用乘、CJK 方塊不乘）；scale≡1.0＝舊行為逐位元組不變。
    // E1-C 卡 C：標點覆寫臂同徑穿過 KP（測量器帶臂＝斷行與量測同源）。
    let clusters = shape_advances_with_punct(text, m.size_pt(), m.advance_scale(), m.punct_arm());
    let pw = PrefixWidths::new(&clusters);

    // atoms：UAX#14 切分＋超寬 atom 的 cluster 緊急切分。
    let mut atoms: Vec<Atom> = Vec::new();
    for a in break_atoms(text) {
        let body_w = pw.prefix(a.trim_from) - pw.prefix(a.start);
        if body_w <= max_w + EPS {
            atoms.push(a);
            continue;
        }
        // 緊急切分：cluster 貪心裝到 ≤max_w（單 cluster 必自成一 chunk）。
        let mut chunks: Vec<(usize, usize)> = Vec::new();
        let mut chunk_start = a.start;
        let mut base = pw.prefix(a.start);
        let mut ci = clusters.partition_point(|c| (c.start as usize) < a.start);
        while ci < clusters.len() && (clusters[ci].start as usize) < a.end {
            let cs = clusters[ci].start as usize;
            let ce = clusters[ci].end as usize;
            // 逾寬且本 chunk 已有至少一個 cluster ⇒ 於 cs 前劈出新 chunk。
            if pw.prefix(ce) - base > max_w + EPS && cs > chunk_start {
                chunks.push((chunk_start, cs));
                chunk_start = cs;
                base = pw.prefix(cs);
            }
            ci += 1;
        }
        chunks.push((chunk_start, a.end));
        let last = chunks.len() - 1;
        for (j, (cs, ce)) in chunks.iter().enumerate() {
            atoms.push(Atom {
                start: *cs,
                end: *ce,
                trim_from: if j == last { a.trim_from.max(*cs) } else { *ce },
                forced: j != last || a.forced,
                hyphen: j == last && a.hyphen,
            });
        }
    }

    // 強制斷點：碼點 → 位元組，折到字元邊界、排序去重、剔掉段首/段尾
    // （那兩個位置不是「行內」斷點，由段落級硬斷處理），然後劈 atom。
    if !forced_cp.is_empty() {
        let mut fb: Vec<usize> = Vec::with_capacity(forced_cp.len());
        for &cp in forced_cp {
            let total = text.chars().count();
            if cp == 0 || cp >= total {
                continue;
            }
            if let Some((b, _)) = text.char_indices().nth(cp) {
                fb.push(b);
            }
        }
        fb.sort_unstable();
        fb.dedup();
        if !fb.is_empty() {
            atoms = split_atoms_forced(atoms, &fb);
        }
    }

    let aw: Vec<AtomW> = atoms
        .iter()
        .map(|a| AtomW {
            full: pw.prefix(a.end) - pw.prefix(a.start),
            tail: pw.prefix(a.end) - pw.prefix(a.trim_from),
        })
        .collect();
    let ranges = optimal_ranges(&atoms, &aw, max_w);

    ranges
        .into_iter()
        .map(|(s, e)| {
            let byte_start = atoms[s].start;
            let raw = &text[byte_start..atoms[e].end];
            // 顯示文字：去掉行尾空白＋全部控制字元（控制字元是斷行標記）。
            let shown: String = raw
                .trim_end_matches(is_ignorable)
                .chars()
                .filter(|c| !EOL_CONTROLS.contains(c))
                .collect();
            let char_offset = text[..byte_start].chars().count();
            LineSpan {
                line: Line {
                    text: shown,
                    width_pt: pw.prefix(atoms[e].end) - pw.prefix(byte_start) - aw[e].tail,
                    para_index: 0, // 由 layout_paragraphs_per_section 覆寫為真正段落索引
                    char_offset,
                    col: 0, // L1-columns：欄序由 pack_run 放置時蓋章
                },
                range: (byte_start, atoms[e].end),
                hard_end: atoms[e].forced,
            }
        })
        .collect()
}
