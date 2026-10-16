//! 除錯用：量測 KP 斷行在 CJK 長文下的 RSS 曲線（PM 指認的 OOM 復現路徑）。
use quire_layout::{break_lines, TextMeasurer};

fn rss_kb() -> u64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest
                .trim()
                .trim_end_matches(" kB")
                .trim()
                .parse()
                .unwrap_or(0);
        }
    }
    0
}

fn main() {
    let m = TextMeasurer::new(12.0);
    for &n in &[200usize, 8000, 40000, 200000] {
        let text = "開源文書處理器測試".repeat(n / 10);
        let t0 = std::time::Instant::now();
        let lines = break_lines(&text, 451.0, &m);
        println!(
            "chars={:6} lines={:5} {:>7.2}s rss={:>9} MB",
            text.chars().count(),
            lines.len(),
            t0.elapsed().as_secs_f64(),
            rss_kb() / 1024
        );
    }
}
