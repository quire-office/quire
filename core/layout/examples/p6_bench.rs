//! Phase0-6 基準：500 頁檔輸入延遲閘門（卡片門檻 <8ms，debug info release build）。
//!
//! 跑法（CI 同）：
//!   CARGO_PROFILE_RELEASE_DEBUG=1 cargo run --release --example p6_bench
//! 可調：P6_TYPING_BUDGET_MS（預設 8.0；p95 超標即非零退出＝CI 紅燈）。
//!
//! 語料：結構決定的 ~500 頁文件（A4/12pt/1.4 行距 → 41 行/頁）。打字腳本
//! 在段落中插一個字（模擬 IME commit），分「文首段／文中段／文末段」三個
//! 位置取樣；對照組是「每筆編輯整篇重建」（舊骨架行為），用以證明增量有效。
use std::time::Instant;

use quire_layout::{LayoutEngine, LayoutParams};

fn doc(paras: usize) -> Vec<String> {
    (0..paras)
        .map(|i| match i % 7 {
            0 => format!("第 {i} 段：{} mixed words 混排。", "內容文字".repeat(40)),
            3 => format!("短段落 {i}。"),
            _ => format!("第 {i} 段：{}", "中文內容測試文字。".repeat(25)),
        })
        .collect()
}

fn rss_mb() -> u64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest
                .trim()
                .trim_end_matches(" kB")
                .trim()
                .parse()
                .unwrap_or(0)
                / 1024;
        }
    }
    0
}

/// 取樣「打字」：在段落 para 的固定碼點處反覆插入一個字。回傳每次重排毫秒清單。
fn type_samples(
    texts: &[String],
    para: usize,
    rounds: usize,
    budget: Option<usize>,
    full_rebuild: bool,
) -> Vec<f64> {
    let p = LayoutParams::default();
    let mut work = texts.to_vec();
    let mut eng = LayoutEngine::new(&work, p);
    if let Some(b) = budget {
        eng.set_sync_budget(Some(b));
    }
    let mut times = Vec::with_capacity(rounds);
    for k in 0..rounds {
        let cur: String = work[para].chars().take(60).collect();
        work[para] = format!("{}字{cur}", k % 10);
        let t0 = Instant::now();
        if full_rebuild {
            // 對照組：整篇重建（含全部段落重新 shaping＋KP＋分頁）。
            eng = LayoutEngine::new(&work, p);
            if let Some(b) = budget {
                eng.set_sync_budget(Some(b));
            }
        } else {
            eng.replace_paragraph_text(para, &work[para]);
            // 背景排程：打字空檔把軟分頁尾段補齊（真實 shell 的 idle 迴圈）。
            while !eng.is_settled() {
                eng.reflow_step();
            }
        }
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    times
}

fn stats(times: &[f64]) -> (f64, f64, f64) {
    let mut v = times.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    (mean, v[v.len() / 2], v[(v.len() as f64 * 0.95) as usize])
}

fn main() {
    let budget_ms: f64 = std::env::var("P6_TYPING_BUDGET_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8.0);
    let p = LayoutParams::default();
    // 湊齊 ~500 頁：先粗估段落數，再指數擴充到 ≥500 頁。
    let mut paras = 1200usize;
    loop {
        let eng = LayoutEngine::new(&doc(paras), p);
        if eng.page_count() >= 500 {
            break;
        }
        paras *= 2;
        if paras > 100_000 {
            panic!("語料擴充失敗");
        }
    }
    // 二分收斂到「剛剛 ≥500 頁」的最小段落數（語料規模老實報表頭）。
    let mut lo = paras / 2;
    let mut hi = paras;
    while lo + 5 < hi {
        let mid = (lo + hi) / 2;
        if LayoutEngine::new(&doc(mid), p).page_count() >= 500 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let paras = hi;
    let texts = doc(paras);
    let t0 = Instant::now();
    let eng = LayoutEngine::new(&texts, p);
    let open_ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!(
        "文件：{} 段 / {} 頁（目標 500+）｜開檔 {:.1}ms｜RSS {}MB",
        texts.len(),
        eng.page_count(),
        open_ms,
        rss_mb()
    );
    assert!(
        eng.page_count() >= 500,
        "語料必須 ≥500 頁（實得 {}）",
        eng.page_count()
    );

    let rounds = 150;
    // 對照組（整篇重建）每輪 ~260ms，跑 150 輪對 CI 太重：縮到 20 輪，
    // 只作為「增量確實有效」的量級參照；閘門只看增量 p95。
    let control_rounds: usize = std::env::var("P6_CONTROL_ROUNDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20)
        .min(rounds);
    let mid = texts.len() / 2;
    let mut worst_p95 = 0.0f64;
    for (label, para) in [
        ("文首段", 1usize),
        ("文中段", mid),
        ("文末段", texts.len() - 2),
    ] {
        let t_full = type_samples(&texts, para, control_rounds, None, true);
        let t_inc = type_samples(&texts, para, rounds, None, false);
        let t_soft = type_samples(&texts, para, rounds, Some(2), false);
        let (fm, fp50, fp95) = stats(&t_full);
        let (im, ip50, ip95) = stats(&t_inc);
        let (sm, sp50, sp95) = stats(&t_soft);
        println!(
            "{label}: 整篇重建 mean {fm:.2} p50 {fp50:.2} p95 {fp95:.2} ms｜增量 mean {im:.2} p50 {ip50:.2} p95 {ip95:.2} ms（{:.0}×）｜增量＋預算2頁 mean {sm:.2} p50 {sp50:.2} p95 {sp95:.2} ms",
            if im > 0.0 { fm / im } else { f64::INFINITY }
        );
        println!(
            "  含 idle reflow_step（軟分頁）在內；預算模式 p95 {sp95:.2}ms vs 全速 {ip95:.2}ms"
        );
        worst_p95 = worst_p95.max(ip95).max(sp95);
    }
    println!(
        "★ 輸入延遲閘門：p95 ≤ {budget_ms}ms → 實得最壞 p95 {worst_p95:.2}ms（RSS 尾值 {}MB）",
        rss_mb()
    );
    if worst_p95 >= budget_ms {
        eprintln!("✗ 輸入延遲超標：p95 {worst_p95:.2}ms ≥ {budget_ms}ms");
        std::process::exit(1);
    }
    println!(
        "✓ 通過（debug-info release build 下執行才算數；CI 已用 CARGO_PROFILE_RELEASE_DEBUG=1）"
    );
}
