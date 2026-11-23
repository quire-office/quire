//! T4AA 臨時診斷（不入庫）：真用戶檔逐格內行重放探針 CELL 判定，找落點漂移形態。
//!
//! 紀律更正（PM 接手時修）：run 2 原版走 `eng.layout_ref()`／`layout.caret_rect()`
//! 等**不存在的 API**（Engine 未暴露 layout 參考），從未被編譯過，因此這份診斷
//! 從未被跑過。本檔改走 Engine 的 JSON 出口（`caret_rect_json`／`caret_at_json`／
//! `table_locator_json`／`paragraph_count`／`paragraph_text`），與外殼同口徑。
//!
//! 隱私鐵律：只輸出結構度量（段號/坐標/長度），禁輸出內文。
use quire_engine::Engine;

fn json_field<T>(s: Option<String>, key: &str) -> Option<T>
where
    T: serde::de::DeserializeOwned,
{
    let v: serde_json::Value = serde_json::from_str(&s?).ok()?;
    serde_json::from_value(v.get(key)?.clone()).ok()
}

#[test]
fn diag_cell_roundtrip_user_doc() {
    let path = match std::env::var("T4AA_DOCX") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("no T4AA_DOCX");
            return;
        }
    };
    let bytes = std::fs::read(&path).unwrap();
    let eng = Engine::from_docx_bytes(&bytes).expect("open");
    let paras = eng.paragraph_count();

    // 身分：table_locator 命中＝格內段
    let mut cell_paras = Vec::new();
    for p in 0..paras {
        if eng.table_locator_json(p).is_some() {
            cell_paras.push(p);
        }
    }

    // 逐段逐 cp 掃 rect（探針同款：步進 4、cap 段長與 400 取小），同 (page, y) 只取第一筆
    let mut bands: Vec<(usize, usize, usize, f32, f32)> = Vec::new(); // (page, para, cp, x, y)
    for &p in &cell_paras {
        let text_len = eng.paragraph_text(p).map(|t| t.chars().count()).unwrap_or(0);
        let cap = text_len.min(400);
        let mut last_key = String::new();
        let mut cp = 0usize;
        while cp <= cap {
            let rect = eng.caret_rect_json(p, cp);
            if let (Some(page), Some(x), Some(y)) = (
                json_field::<usize>(rect.clone(), "page"),
                json_field::<f32>(rect.clone(), "x_pt"),
                json_field::<f32>(rect.clone(), "y_pt"),
            ) {
                let key = format!("{page}:{}", y.round());
                if key != last_key {
                    bands.push((page, p, cp, x, y));
                    last_key = key;
                }
            }
            cp += 4;
        }
    }

    let mut bad = 0usize;
    let mut total = 0usize;
    let mut bad_same_para = 0usize; // 同段但 cp 漂移（char_offset 累加形態）
    let mut bad_none = 0usize; // 反向完全落空（行跨度判制未覆蓋）
    for (page, para, cp, x, y) in &bands {
        total += 1;
        let back = eng.caret_at_json(*page, x + 0.5, y + 1.0);
        let got_para = json_field::<usize>(back.clone(), "para");
        let got_cp = json_field::<usize>(back.clone(), "cp");
        match (got_para, got_cp) {
            (Some(gp), Some(gc)) if gp == *para && gc == *cp => {}
            (Some(gp), Some(gc)) => {
                bad += 1;
                if gp == *para {
                    bad_same_para += 1;
                }
                if bad <= 25 {
                    println!("BAD pg={page} para={para} cp={cp} x={x:.1} y={y:.1} got=({gp},{gc})");
                }
            }
            _ => {
                bad += 1;
                bad_none += 1;
                if bad <= 25 {
                    println!("BAD pg={page} para={para} cp={cp} x={x:.1} y={y:.1} got=None");
                }
            }
        }
    }
    println!(
        "TOTAL={total} BAD={bad}（同段cp漂移={bad_same_para}／反向落空={bad_none}）cell_paras={}",
        cell_paras.len()
    );
}
