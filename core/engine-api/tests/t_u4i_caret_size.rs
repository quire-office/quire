//! U4i（t_b06cccde）紅燈考卷：正文 caret 的字級腿（高度＋行內 x）須隨段落字級。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 用戶實測缺口（PM 轉達 2026-11-24）
//! 「有 cursor 但會上下偏移，且大小不會隨段落字體大小改變。」編輯器線上站
//! 實測檔（多級標題文件，標題段 26pt 級）：點大字號段落時游標明顯偏離文字
//! 行、游標高度恆細短。
//!
//! # 探針實測對卡面前提的修正（先量後修；合成檔＋200 檔 5554 樣本全 corpus 掃）
//! 卡面根因寫「y 取 margin_top＋band×line_h（全域 12pt 尺）⇒ 段級越大越往後
//! 漂移」——**y 腿經實測已與發射同源**：行的帶索引由 `pack_run` 按段級
//! ParaGrid 擴張（`line_bands`），caret 與發射腿（engine-api 正文迴路）吃
//! 同一 `margin_top＋band×line_h`，逐段全等（corpus 5554 樣本僅 2 例偏差
//! 逾 0.5pt，皆浮動框錨點頁，屬正交既有態）。燈 1 因此是**護欄燈**（修前即
//! 綠，防後續回歸把 band 腿動壞），非鑑別燈——如實記帳，不充當修前紅證據。
//!
//! 真正不同源的是**字級腿**（探針實錄，本檔 fixture）：
//! 1. `height_pt` 吃全域預設尺：26pt 標題段 caret 高 12pt ⇒ 用戶「游標高度
//!    恆細短」＋「上下偏移」的垂直成分（caret 頂對齊行頂、只畫半截高）。
//!    corpus H_BAD=2006/5554（36% 樣本 caret 高 ≠ 發射字級）。
//! 2. 行內 x 由 `caret_x_in_line` 按**全域 12pt 尺**量測，發射腿按該段級聯
//!    字級排（26pt 段 caret 每字推進 11.93pt，文字每字 ~26pt）⇒ 用戶「游標
//!    偏離文字行」的橫向成分。正逆兩腿舊態同用全域尺所以互逆自洽，但與發
//!    射尺不同源——本卡補的就是這條與發射／斷行同源的尺（燈 3）。
//!
//! # 修法（PM 定案第 1 條的正文版；已入源碼）
//! 正逆兩腿字級一律取 `grid.size_of(para, 全域)`——與 `break_para` 斷行
//! **同一支尺**（同一 ParaGrid 通道，engine 由樣式級聯算出餵入），嚴禁另造
//! 第二把尺；Line 層零新欄位、serde 零擾動。y 腿與「表格帶必 None」「U4f
//! 行跨度判定制」零擾動；格內腿（t_4aa cell_caret_*）零碰觸。混級段的
//! run 級穿透屬後續卡——段級（斷行尺同源）口徑為本卡契約，燈 4 如實記帳。
//!
//! # 鑑別力（BUGGY 回塞鎖口徑，鎖在源碼熱路徑）
//! - `U4I_CARET_GLOBAL_RULER_BUGGY=1`：正逆兩腿字級整支退回全域預設尺＋
//!   反查帶商容差退 0（修前逐位元組舊態）⇒ **燈 2／燈 3／燈 4 必紅**（實測
//!   點名：燈 2 高度 7 條「12pt 畫進 26pt 段」、燈 3(a) x 步進退 11.7pt/字、
//!   燈 4 混級段帶界成批反查回前一行）；燈 1（y 護欄）與燈 5（介面零擴權）
//!   在該態恆綠——兩燈護的與字級腿正交（記帳於各燈註解）。
//! - 零擾動守門由護欄群承接：t4aa_cell_caret 7/7、u4f_band_ruler 4/4、
//!   goldens／roundtrip／wasm32（卡面驗收清單）。

use quire_engine::Engine;
use serde_json::Value;

/// 合成混級檔（產生器同檔同名 py）：
/// 段0＝26pt 標題｜段1＝12pt 正文（5 行）｜段2＝混級段（26pt 起頭＋12pt
/// 續寫，4 行）｜段3＝12pt 尾段｜段4＝26pt 尾標題（累積漂移偵測點）。
fn engine() -> Engine {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/u4i_mixed_size.docx"
    );
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("讀 {path}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("開啟混級 fixture")
}

/// 一頁顯示列表的文字項（經 JSON 出口取 DL，不 import 私域——run4b 教訓）。
/// 回傳 (y_pt, size_pt, x_pt)。
fn text_items(eng: &Engine, page: usize) -> Vec<(f32, f32, f32)> {
    let raw = eng
        .page_display_list_json(page)
        .expect("頁顯示列表必存在（JSON 出口）");
    let v: Value = serde_json::from_str(&raw).expect("顯示列表 JSON 可解析");
    let mut out = Vec::new();
    if let Some(items) = v["items"].as_array() {
        for it in items {
            let Some(t) = it.get("Text") else { continue };
            if let (Some(y), Some(s), Some(x)) =
                (t["y"].as_f64(), t["size_pt"].as_f64(), t["x"].as_f64())
            {
                out.push((y as f32, s as f32, x as f32));
            }
        }
    }
    out
}

/// caret_rect 經 JSON 出口（不 import 私域 CaretRect）：(page, x, y, height)。
fn caret_rect(eng: &Engine, para: usize, cp: usize) -> Option<(usize, f32, f32, f32)> {
    let raw = eng.caret_rect_json(para, cp)?;
    let v: Value = serde_json::from_str(&raw).expect("caret JSON 可解析");
    Some((
        v["page"].as_u64()? as usize,
        v["x_pt"].as_f64()? as f32,
        v["y_pt"].as_f64()? as f32,
        v["height_pt"].as_f64()? as f32,
    ))
}

fn caret_at(eng: &Engine, page: usize, x: f32, y: f32) -> Option<(usize, usize)> {
    let raw = eng.caret_at_json(page, x, y)?;
    let v: Value = serde_json::from_str(&raw).expect("caret_at JSON 可解析");
    Some((v["para"].as_u64()? as usize, v["cp"].as_u64()? as usize))
}

/// 段落數（texts 通道：engine.paragraph_text 逐段可得）。
fn para_count(eng: &Engine) -> usize {
    (0..)
        .take_while(|i| eng.paragraph_text(*i).is_some())
        .count()
}

/// 段首行在顯示列表的發射字級：caret(段,0) 給 y，取同頁 |Δy| 最小項。
/// （段首行 y 與他段行 y 在本 fixture 互異；同段續行 y 更大，取最小者＝首行。）
fn head_line_size(eng: &Engine, para: usize) -> Option<(usize, f32, f32)> {
    let (page, _x, y, _h) = caret_rect(eng, para, 0)?;
    let items = text_items(eng, page);
    items
        .iter()
        .map(|(iy, s, ix)| ((*iy - y).abs(), *s, *ix))
        .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
        .map(|(_, s, ix)| (page, s, ix))
}

// ---------------------------------------------------------------------------
// 燈 1（護欄：y 腿與發射同源）：逐段逐樣本斷言 caret_rect 的 y 與顯示列表
// 對應 Text item 實際 y_pt 差 ≤0.5pt（經 JSON 出口取 DL，不 import 私域）。
// 【記帳】y 腿修前已同源（見檔頭）——本燈 BUGGY 態恆綠：它護 band×帶尺腿，
// 與本卡字級腿正交；設為護欄防「修字級把 y 也動壞」。
// ---------------------------------------------------------------------------

#[test]
fn u4i_lamp1_caret_y_matches_display_list() {
    let eng = engine();
    let npara = para_count(&eng);
    let mut samples = 0usize;
    let mut bad: Vec<String> = Vec::new();
    for para in 0..npara {
        let text = eng.paragraph_text(para).unwrap();
        let n = text.chars().count();
        if n == 0 {
            continue;
        }
        for cp in (0..=n).step_by((n / 8).max(1)) {
            let Some((page, _x, y, _h)) = caret_rect(&eng, para, cp) else {
                bad.push(format!("段 {para} cp {cp}：caret_rect 必可得"));
                continue;
            };
            let items = text_items(&eng, page);
            let Some(dy) = items
                .iter()
                .map(|(iy, _, _)| (*iy - y).abs())
                .min_by(|a, b| a.partial_cmp(b).unwrap())
            else {
                bad.push(format!("段 {para} cp {cp}：頁 {page} 無文字項"));
                continue;
            };
            samples += 1;
            if dy > 0.5 {
                bad.push(format!(
                    "段 {para} cp {cp}：caret y {y:.3} 與最近發射行 y 差 {dy:.3}pt（>0.5pt）"
                ));
            }
        }
    }
    assert!(samples >= 20, "取樣不足（實得 {samples}）");
    assert!(
        bad.is_empty(),
        "燈 1 y 護欄破口 {} 條：\n{}",
        bad.len(),
        bad.join("\n")
    );
}

// ---------------------------------------------------------------------------
// 燈 2（高度腿）：caret 高度＝該段（首行）發射字級（±0.1pt）。
// 修前必紅（BUGGY 鎖重現）：26pt 標題段 caret 高 12pt（探針實錄 h=12）。
// ---------------------------------------------------------------------------

#[test]
fn u4i_lamp2_caret_height_follows_paragraph_size() {
    let eng = engine();
    let npara = para_count(&eng);
    let mut checked = 0usize;
    let mut big_checked = 0usize;
    let mut bad: Vec<String> = Vec::new();
    for para in 0..npara {
        let text = eng.paragraph_text(para).unwrap();
        let n = text.chars().count();
        if n == 0 {
            continue;
        }
        let Some((_, head_size, _)) = head_line_size(&eng, para) else {
            continue;
        };
        if head_size > 20.0 {
            big_checked += 1;
        }
        // 首行 cp（caret y 與首行 y 同值者）逐個對帳高度。
        for cp in [0usize, n / 4, n / 2] {
            let Some((page, _x, y, h)) = caret_rect(&eng, para, cp) else {
                bad.push(format!("段 {para} cp {cp}：caret_rect 必可得"));
                continue;
            };
            let Some((_, hs, _)) = head_line_size(&eng, para) else {
                continue;
            };
            let head_y = match caret_rect(&eng, para, 0) {
                Some((_p, _x, hy, _h)) => hy,
                None => continue,
            };
            if (y - head_y).abs() > 0.5 {
                continue; // 續行（混級段首行≠續行字級時跳過，契約＝段級）
            }
            checked += 1;
            if (h - hs).abs() > 0.1 {
                bad.push(format!(
                    "段 {para}（第 {page} 頁）cp {cp}：caret 高 {h:.3}pt ≠ 發射字級 {hs:.3}pt（>0.1pt）"
                ));
            }
        }
    }
    assert!(checked >= 8, "首行樣本不足（實得 {checked}）");
    assert!(
        big_checked >= 2,
        "大字號段樣本不足（26pt 級實得 {big_checked}，fixture 應含兩段）"
    );
    assert!(
        bad.is_empty(),
        "燈 2 高度腿破口 {} 條（BUGGY 態點名：全域尺 12pt 畫進 26pt 段）：\n{}",
        bad.len(),
        bad.join("\n")
    );
}

// ---------------------------------------------------------------------------
// 燈 3（x 腿與發射尺同源＋正逆 round-trip）：
// (a) 26pt 段的行內 x 步進必按該段字級推進（與發射尺同源），不得按全域
//     12pt 尺（修前態 step≈11.93pt/字＝BUGGY 必紅點名）。
// (b) 大字號段逐 cp 正向 y→反向 caret_at 必還原同 (para,cp)。
// ---------------------------------------------------------------------------

#[test]
fn u4i_lamp3_x_follows_size_and_round_trip() {
    let eng = engine();
    // (a) 段0＝26pt 標題段：連續 cp 的 x 步進＝該段全形字 advance（26pt 級）。
    //     全域 12pt 尺步進實錄 11.93pt；26pt 級聯尺步進≈25.86pt（全角 1em）。
    //     判準 ≥20pt：全域尺態（11.9）必紅、級聯尺態（25.9）綠。
    let head_para = 0usize;
    let head = eng.paragraph_text(head_para).unwrap();
    let xs: Vec<f32> = (0..head.chars().count())
        .filter_map(|cp| caret_rect(&eng, head_para, cp))
        .map(|(_p, x, _y, _h)| x)
        .collect();
    assert!(xs.len() >= 4, "26pt 段 caret 取樣不足");
    let step = xs[1] - xs[0];
    assert!(
        step > 20.0,
        "燈 3(a) 26pt 段行內 x 步進 {step:.3}pt/字——未按段落字級（全域尺態≈11.9，發射同尺態≈25.9）"
    );

    // (b) 正逆 round-trip：大字號段（段0／段4）行內 cp 的 (x, y) 反查必還原
    //     同 (para,cp)。x 取 caret 自身值（同尺反查；跨半字取樣由燈 3(a) 的
    //     尺源斷言把關，這裡護互逆契約）。
    for para in [0usize, 4usize] {
        let n = eng.paragraph_text(para).unwrap().chars().count();
        for cp in 0..n {
            let Some((page, x, y, _h)) = caret_rect(&eng, para, cp) else {
                panic!("大字號段 {para} cp {cp}：正向腿 None");
            };
            let Some((bp, bcp)) = caret_at(&eng, page, x + 1.0, y) else {
                panic!(
                    "大字號段 {para} cp {cp}：反向腿 None（x={x:.2}, y={y:.2}）——帶尺判定制被動壞？"
                );
            };
            assert_eq!(
                (bp, bcp),
                (para, cp),
                "大字號段 {para} cp {cp} 正逆 round-trip 破口（x={x:.2}, y={y:.2}）"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 燈 4（尺源印證：caret 高度必取 ParaGrid 段級尺，與斷行同源）：混級段
// （段2，26pt 起頭＋12pt 續寫）斷行按段內最大字級（26pt）成形——行內 cp
// 反查還原同 (para,cp) 要求正逆兩腿吃**同一**段級尺；若反向腿退回全域尺
// 而正向腿吃段級（或反之），混級段大 cp 樣本必錯位。BUGGY 態兩腿同步退
// 全域尺 → 反查仍自洽，但燈 2／燈 3 已點名；本燈在 BUGGY 態**亦紅**：
// 混級段的斷行尺（26pt）與 caret 尺（全域 12pt）不同源時，按 caret 尺的
// x 反查會命中錯位 cp（斷行每字 25.9pt、反查每字 11.9pt，行界錯判）。
// ---------------------------------------------------------------------------

#[test]
fn u4i_lamp4_mixed_para_uses_break_ruler() {
    let eng = engine();
    // 段2＝混級段；斷行尺＝段內最大 26pt（每行 17 字＝468pt<版心 468pt 邊界，
    // 探針實錄每行 17 字）。取末行行尾附近 cp 做正逆對帳：全域尺態的 x 反查
    // 會落回前段 cp（錯位）。
    let para = 2usize;
    let n = eng.paragraph_text(para).unwrap().chars().count();
    let mut checked = 0usize;
    let mut bad: Vec<String> = Vec::new();
    for cp in 0..n {
        let Some((page, x, y, _h)) = caret_rect(&eng, para, cp) else {
            bad.push(format!("混級段 cp {cp}：正向腿 None"));
            continue;
        };
        let Some((bp, bcp)) = caret_at(&eng, page, x + 1.0, y) else {
            bad.push(format!("混級段 cp {cp}：反向腿 None（x={x:.2}, y={y:.2}）"));
            continue;
        };
        checked += 1;
        if (bp, bcp) != (para, cp) {
            bad.push(format!(
                "混級段 cp {cp} → 反查 ({bp},{bcp})：正逆尺不同源／與斷行尺不同源（x={x:.2}, y={y:.2}）"
            ));
        }
    }
    assert!(checked >= 50, "混級段取樣不足（實得 {checked}）");
    assert!(
        bad.is_empty(),
        "燈 4 混級段尺源破口 {} 條：\n{}",
        bad.len(),
        bad.join("\n")
    );
}

// ---------------------------------------------------------------------------
// 燈 5（介面零擴權鎖，run4b 教訓）：caret JSON 出口欄位零變動——本卡只改
// 尺源，不導入私域型別、不新增欄位。恆綠護欄（BUGGY 態亦綠：它護介面形態）。
// ---------------------------------------------------------------------------

#[test]
fn u4i_lamp5_caret_json_surface_unchanged() {
    let eng = engine();
    let raw = eng.caret_rect_json(0, 0).expect("正文 caret 可得");
    let v: Value = serde_json::from_str(&raw).unwrap();
    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["height_pt", "page", "x_pt", "y_pt"],
        "caret 介面欄位零擴權（run4b 教訓：一切以可斷言介面為準）"
    );
    // 反查點由正向腿給出（行起帶在 U4F 判定制下恆命中——本燈是介面形態鎖，
    // 不得受其他卡回塞鎖的「點擊落點」語義牽動）。
    let (_pg, _cx, cy, _ch) = caret_rect(&eng, 0, 0).expect("正文 caret 可得");
    let back = eng
        .caret_at_json(0, 100.0, cy)
        .expect("正文反查可得（行起點 y 必命中）");
    let v2: Value = serde_json::from_str(&back).unwrap();
    let mut keys2: Vec<&str> = v2.as_object().unwrap().keys().map(String::as_str).collect();
    keys2.sort_unstable();
    assert_eq!(keys2, vec!["cp", "para"], "caret_at 介面欄位零擴權");
}
