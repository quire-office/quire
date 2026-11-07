//! F2：DrawingML `a:prstGeom/@prst` 預設形状輪廓表（含 `a:prstTxWarp` 名稱識別）。
//!
//! # 為什麼是「名稱→歸一化點集」而不是「名稱→SVG 字串」
//! 本模組只負責**幾何事實**（單位方格內的折線頂點），不碰任何輸出格式：
//! 發射層（`core/engine-api`）把點集映射成 `DrawItem::Rule` 線段，
//! `shells/native/src/render_svg.rs` 既有 `<line>` 臂原樣吃掉——顯示列表契約
//! （`core/render-trait`）**不在本卡白名單**，故不得新增 `DrawItem` 變體，
//! 一切非矩形輪廓都以既有 `Rule` 折線承載（卡面「可見輪廓＋文字」最低標）。
//!
//! # 數值來源紀律（禁止宣稱未實測的能力）
//! - `triangle`／`rtTriangle`／`diamond`／`parallelogram`／`trapezoid`／
//!   `hexagon`／`octagon`：頂點＝外接框角點／中點／ECMA 預設 `adj` 直接幾何。
//!   LO 真值實抓覆證（`docs/f2-survey/f2v11_lo_lines.py`）：`tdf113183`
//!   triangle = (549.0,558.5)→(579.7,492.5)→(610.5,558.5) 閉合，歸一化恰為
//!   (0,1)(0.5,0)(1,1)——與本表逐點吻合。
//! - `star5`：**逐點取自 LO 實測**（`tdf113183` 外接框 90.75×81.75pt 的 10 個
//!   頂點端點，減原點後除以框寬高得歸一化值，見 `docs/f2-redlight.md` §2）。
//!   非套公式猜數：LO 的 Word 五角星並非正五角星（頂點高寬結構與閉式解有
//!   可測偏差），故用實測表。
//! - `ellipse`：LO 用 12 段三次貝塞爾（`tdf113258` 外接框 226.6×58.2）。本表
//!   以 **12 弦折線**近似（半格偏移取點使 12 條弦全部非軸對齊），弓高誤差
//!   `1-cos(15°) ≈ 3.4%` 相對半徑，記帳於 `docs/f2-wordart-anatomy.md`。
//! - `roundRect`：ECMA-376 §20.1.10.49 預設 `adj=16667` ⇒ 圓角半徑
//!   `r = adj × min(w,h)`。本表以**切角八邊形**近似圓角（對角弦承載
//!   「不是直角」的事實），誤差同記帳。
//! - `line`／`straightConnector1`：單一线段，端點由 `a:xfrm/@flipH`／`@flipV`
//!   決定（`docs/f2-survey/f2v13_flip.py` 實據：語料 connector 多數帶
//!   `flipV="1"`，LO 真值 (92.2,266.2)→(272.8,236.3) 為對角線）。
//! - `rightArrow`／`leftRightArrow`／`upArrow`：ECMA-376 §20.1.10 指引式
//!   （`adj1=adj2=50000`）推出的 7／10 點多邊形，**未逐檔實抓 LO 端點**
//!   ⇒ `docs/f2-wordart-anatomy.md` 標記「ECMA 公式推得，LO 端點未校準」。
//! - `star10`：語料 1 次／1 檔，無 LO 真值可校準 → **明確不實作**（回 `None`
//!   ＝維持外接矩形退化），記帳於解剖報告，不用公式冒充實測。
//!
//! 未列於本表的 `@prst`（含 `custGeom`、`star10`、語料外的 180 餘種 preset）
//! 一律回 `None`：呼叫端**必須**退回既有外接矩形發射＝零回退守則。

/// 一條預設形状輪廓：單位方格（0..1 × 0..1，原點＝外接框左上角、v 向下正，
/// 與顯示列表 y 軸同向）內的折線頂點。`closed=true` ⇒ 呼叫端補最後一點回
/// 第一點的閉合邊。
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    /// 歸一化頂點。
    pub pts: Vec<(f32, f32)>,
    /// 是否閉合（線／連接線＝false）。
    pub closed: bool,
}

/// `a:prstGeom` 的 rect 系（既有外接矩形發射已正確，本卡零擾動臂）。
pub fn is_rect_like(prst: &str) -> bool {
    matches!(prst, "rect" | "textRect" | "")
}

/// `a:prstTxWarp/@prst` 是否為「無變形」預設翹曲。
///
/// 語料實測（`docs/f2-wordart-anatomy.md` §5.3）：全語料 warp 99 次為
/// `textNoShape`（不產生任何視覺變形，文字照直線基線排）、1 次為
/// `textTriangle`（真路徑變形，本卡明確不做，見解剖報告 §6.2）。
pub fn warp_is_identity(prst: &str) -> bool {
    prst == "textNoShape"
}

/// `a:prstTxWarp/@prst` 是否為本切片**明確不做**的真路徑變形（發射層據此
/// 老實保持直線基線排版＝零擾動，並在事實欄留名供考卷逐字比對）。
pub fn warp_is_unimplemented(prst: &str) -> bool {
    !warp_is_identity(prst)
}

/// 三角形（LO 真值逐點校準，見模組頭註解）。
const TRIANGLE: [(f32, f32); 3] = [(0.0, 1.0), (0.5, 0.0), (1.0, 1.0)];
/// 直角三角形（ECMA-376 `rtTriangle`）。
const RT_TRIANGLE: [(f32, f32); 3] = [(0.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
/// 菱形。
const DIAMOND: [(f32, f32); 4] = [(0.5, 0.0), (1.0, 0.5), (0.5, 1.0), (0.0, 0.5)];
/// 平行四邊形（ECMA 預設 `adj=25000` ⇒ 頂點內縮 0.25）。
const PARALLELOGRAM: [(f32, f32); 4] = [(0.25, 0.0), (1.0, 0.0), (0.75, 1.0), (0.0, 1.0)];
/// 梯形（ECMA 預設 `adj=25000`）。
const TRAPEZOID: [(f32, f32); 4] = [(0.25, 0.0), (0.75, 0.0), (1.0, 1.0), (0.0, 1.0)];
/// 正六角形（ECMA 預設 `adj=25000`）。
const HEXAGON: [(f32, f32); 6] = [
    (0.25, 0.0),
    (0.75, 0.0),
    (1.0, 0.5),
    (0.75, 1.0),
    (0.25, 1.0),
    (0.0, 0.5),
];
/// 正八角形（ECMA 預設 `adj=25000`；本表以 √2/2−0.5≈0.2071 內縮）。
const OCTAGON: [(f32, f32); 8] = [
    (0.2929, 0.0),
    (0.7071, 0.0),
    (1.0, 0.2929),
    (1.0, 0.7071),
    (0.7071, 1.0),
    (0.2929, 1.0),
    (0.0, 0.7071),
    (0.0, 0.2929),
];
/// 五角星——**LO 實測逐點**（`tdf113183` 外接框 90.75×81.75pt：頂點
/// (59.3,299.0)(94.0,299.0)(104.8,267.8)(115.4,299.0)(150.1,299.0)
/// (122.1,318.4)(132.8,349.5)(104.8,330.2)(76.7,349.5)(87.4,318.4)，減原點
/// 除以框寬高；對稱對取平均，殘差 <0.001 單位）。
const STAR5: [(f32, f32); 10] = [
    (0.0000, 0.3819),
    (0.3822, 0.3819),
    (0.5011, 0.0000),
    (0.6178, 0.3819),
    (1.0000, 0.3819),
    (0.6916, 0.6193),
    (0.8095, 1.0000),
    (0.5011, 0.7637),
    (0.1916, 1.0000),
    (0.3095, 0.6193),
];
/// 右向箭頭（ECMA-376 `rightArrow`，`adj1=adj2=50000`，方框歸一化表示；
/// 未逐檔校準 LO 端點，記帳）。
const RIGHT_ARROW: [(f32, f32); 7] = [
    (0.0, 0.25),
    (0.5, 0.25),
    (0.5, 0.0),
    (1.0, 0.5),
    (0.5, 1.0),
    (0.5, 0.75),
    (0.0, 0.75),
];
/// 左右雙向箭頭（ECMA-376 `leftRightArrow`；同上未校準記帳）。
const LEFT_RIGHT_ARROW: [(f32, f32); 10] = [
    (0.0, 0.5),
    (0.25, 0.0),
    (0.25, 0.25),
    (0.75, 0.25),
    (0.75, 0.0),
    (1.0, 0.5),
    (0.75, 1.0),
    (0.75, 0.75),
    (0.25, 0.75),
    (0.25, 1.0),
];
/// 上向箭頭（ECMA-376 `upArrow`；同上未校準記帳）。
const UP_ARROW: [(f32, f32); 8] = [
    (0.25, 0.0),
    (0.5, 0.0),
    (1.0, 0.5),
    (0.75, 0.5),
    (0.75, 1.0),
    (0.25, 1.0),
    (0.25, 0.5),
    (0.0, 0.5),
];
/// 橢圓：12 弦折線頂點（`0.5 + 0.5·cos/·sin(15° + k·30°)`，4 位捨入）。
/// 半格偏移 ⇒ 相鄰頂點既不同 x 也不同 y ⇒ 12 條弦全部非軸對齊（外接矩形
/// 退化態四邊恒軸對齊——考卷燈1b 判別力的幾何根因）。
const ELLIPSE: [(f32, f32); 12] = [
    (0.9665, 0.6294),
    (0.8232, 0.8232),
    (0.6294, 0.9665),
    (0.3706, 0.9665),
    (0.1768, 0.8232),
    (0.0335, 0.6294),
    (0.0335, 0.3706),
    (0.1768, 0.1768),
    (0.3706, 0.0335),
    (0.6294, 0.0335),
    (0.8232, 0.1768),
    (0.9665, 0.3706),
];

/// `roundRect` 的 ECMA-376 預設 `adj`（16667/100000）。
pub const ROUND_RECT_ADJ: f32 = 0.16_667;

/// 取 `@prst` 的單位方格輪廓。回 `None`＝本切片不支援（呼叫端退回外接
/// 矩形＝零回退守則；含 `star10`、`custGeom` 與一切未列名 preset）。
///
/// `flip_h`／`flip_v`＝`a:xfrm/@flipH`／`@flipV`。只有**線／連接線**的 flip
/// 改變端點（語料實據 f2v13：帶 flip 的只有 `straightConnector1`）；封閉
/// 形狀的 flip 在本切片維持原表（外接框不變，LO 端點未逐檔校準者不虛構
/// 鏡射變體）。
pub fn outline(prst: &str, flip_h: bool, flip_v: bool) -> Option<Outline> {
    let pts: &[(f32, f32)] = match prst {
        "triangle" => &TRIANGLE,
        "rtTriangle" => &RT_TRIANGLE,
        "diamond" => &DIAMOND,
        "parallelogram" => &PARALLELOGRAM,
        "trapezoid" => &TRAPEZOID,
        "hexagon" => &HEXAGON,
        "octagon" => &OCTAGON,
        "star5" => &STAR5,
        "rightArrow" => &RIGHT_ARROW,
        "leftRightArrow" => &LEFT_RIGHT_ARROW,
        "upArrow" => &UP_ARROW,
        "ellipse" => &ELLIPSE,
        "line" | "straightConnector1" => {
            return Some(line_outline(flip_h, flip_v));
        }
        _ => return None,
    };
    Some(Outline {
        pts: pts.to_vec(),
        closed: true,
    })
}

/// `line`／`straightConnector1`：單位方格的一條對角線，端點由 flip 決定。
///
/// ECMA-376 §20.1.10.61：連接線在 `(off, ext)` 框內從左上畫到右下，
/// `flipH`／`flipV` 鏡射該線。語料實據（f2v13）：connector 多數帶
/// `flipV="1"` ⇒ 端點＝(0,1)→(1,0)，LO 真值 (92.2,266.2)→(272.8,236.3)
/// 正是該方向（shapes-with-text p0 實抓）。
fn line_outline(flip_h: bool, flip_v: bool) -> Outline {
    const PTS: [(f32, f32); 2] = [(0.0, 0.0), (1.0, 1.0)];
    const PTS_V: [(f32, f32); 2] = [(0.0, 1.0), (1.0, 0.0)];
    const PTS_H: [(f32, f32); 2] = [(1.0, 0.0), (0.0, 1.0)];
    const PTS_HV: [(f32, f32); 2] = [(1.0, 1.0), (0.0, 0.0)];
    let pts = match (flip_h, flip_v) {
        (false, false) => &PTS[..],
        (false, true) => &PTS_V[..],
        (true, false) => &PTS_H[..],
        (true, true) => &PTS_HV[..],
    };
    Outline {
        pts: pts.to_vec(),
        closed: false,
    }
}

/// 圓角矩形：以切角八邊形近似 `r = ROUND_RECT_ADJ × min(w,h)` 的圓角。
///
/// `k`＝水平內縮量（發射層按 `r/w` 算）、`k2`＝垂直內縮量（`r/h`）。
/// 呼叫端保證 `0 < k,k2 < 0.5`，否則應自行退回外接矩形。
pub fn round_rect_outline(k: f32, k2: f32) -> Outline {
    Outline {
        pts: vec![
            (k, 0.0),
            (1.0 - k, 0.0),
            (1.0, k2),
            (1.0, 1.0 - k2),
            (1.0 - k, 1.0),
            (k, 1.0),
            (0.0, 1.0 - k2),
            (0.0, k2),
        ],
        closed: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本模組自證：語料實測 prst 名單（解剖報告 §5.4）都有落點，唯一無
    /// LO 真值可校準的 `star10` 明確回 None（不虛構能力）。
    #[test]
    fn corpus_prst_coverage_and_honest_gaps() {
        for p in [
            "triangle",
            "rtTriangle",
            "ellipse",
            "star5",
            "straightConnector1",
            "line",
            "rightArrow",
            "leftRightArrow",
            "upArrow",
        ] {
            assert!(outline(p, false, false).is_some(), "{p} 必須有輪廓");
        }
        assert!(
            outline("star10", false, false).is_none(),
            "star10 無 LO 真值可校準＝明確不實作（回 None 退外接矩形）"
        );
        assert!(is_rect_like("rect"));
        assert!(!is_rect_like("ellipse"));
    }

    /// 判別力根因：封閉形状的邊必須多數非軸對齊（外接矩形退化態四邊恒軸對齊）。
    #[test]
    fn outlines_are_observablely_not_axis_aligned() {
        let o = outline("star5", false, false).expect("star5");
        let slanted = o
            .pts
            .iter()
            .enumerate()
            .filter(|(i, (u1, v1))| {
                let (u2, v2) = o.pts[(i + 1) % o.pts.len()];
                (u1 - u2).abs() > 1e-4 && (v1 - v2).abs() > 1e-4
            })
            .count();
        assert!(slanted >= 6, "star5 斜邊應多數，實測 {slanted}");
        let e = outline("ellipse", false, false).expect("ellipse");
        let slanted = e
            .pts
            .iter()
            .enumerate()
            .filter(|(i, (u1, v1))| {
                let (u2, v2) = e.pts[(i + 1) % e.pts.len()];
                (u1 - u2).abs() > 1e-4 && (v1 - v2).abs() > 1e-4
            })
            .count();
        assert_eq!(slanted, 12, "橢圓 12 弦必須全部非軸對齊");
    }

    /// connector flip：語料實據 `flipV="1"` ⇒ (0,1)→(1,0)。
    #[test]
    fn connector_flip_matches_corpus_evidence() {
        let o = outline("straightConnector1", false, true).expect("connector");
        assert!(!o.closed);
        assert_eq!(o.pts[0], (0.0, 1.0));
        assert_eq!(o.pts[1], (1.0, 0.0));
    }

    /// warp 名稱語意（語料實測 99/100 為 textNoShape）。
    #[test]
    fn warp_name_semantics() {
        assert!(warp_is_identity("textNoShape"));
        assert!(warp_is_unimplemented("textTriangle"));
        assert!(!warp_is_identity("textTriangle"));
    }
}
