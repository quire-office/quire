//! shaping/pagination/table golden 快照產生器／驗證器（Phase0-4/0-5/3-1b 閘門）。
//!
//!   cargo run -p quire-layout --bin goldens            # 比對，差異即非零退出
//!   UPDATE_GOLDENS=1 cargo run -p quire-layout --bin goldens   # 重新生成
//!
//! 快照格式（每行 JSON）：{"id":..., "input":..., "max_w_pt":..., "lines":[{"text":..., "width_pt":...}]}
//! 快照捕捉的就是 **shaped advances 斷點**：斷點位移、字寬改變都會咬golden。
//! pagination.jsonl（Phase0-5）：{"id":..., "params":{...}, "breaks":[[page,first_para,
//! last_para,line_count,continues_paragraph],...]}——分頁斷點對段落歸屬的快照。
//! tables.jsonl（Phase3-1b）：{"id":..., "slices":[{...TableFrag 幾何...}]}——
//! 表格列高/欄寬/合併/vMerge/跨頁切片/巢狀表的幾何快照（合併儲存格、巢狀
//! 表格各 1 例；跨頁 2 例：fragment() 切片原語 1 例＋真正走 packer 放置路徑
//! 的 paginate_tables 1 例，後者鎖 FlowItem::Table 分支的蓋章與帶交接）。
use std::path::Path;

use quire_layout::table::{
    layout_table, CellBlock, CellInput, EdgeSetInput, PenInput, RowInput, TableInput, TableLayout,
    WidthSpec,
};
use quire_layout::{break_lines, page_breaks, LayoutParams, Line, TextMeasurer};

const SIZE_PT: f32 = 12.0;

fn cases() -> Vec<(&'static str, &'static str, f32)> {
    // 中英混排斷點是核心；其餘為回歸面（標點/連字/缺字/RTL/全形）。
    vec![
        ("mixed_basic", "Quire 是一個開源的中文文書處理器，目標是取代 Word。", 200.0),
        ("mixed_narrow", "The 快速棕狐 jumps over the 懶惰的狗 while 測試 mixed CJK。", 120.0),
        ("mixed_wide", "在 Rust 的 ecosystem 裡，parley 與 swash 負責 text shaping；斷行演算法採用 Knuth-Plass，再配上 UAX#14 的 CJK/Latin 混排斷詞規則。", 451.0),
        ("cjk_punctuation", "他說：「這個方案——真的嗎？」我回答：「千真萬確！」", 140.0),
        ("latin_words", "Supercalifragilisticexpialidocious and simple words mixed together in a narrow box", 100.0),
        ("fullwidth_latin", "全形ＡＢＣ與半形ABC混排test", 90.0),
        ("ligature_fi", "Office efficient affiliation — finally fine stuff.", 150.0),
        ("numbers_units", "預算 3,000,000 元（約 USD 95,000.50）於 2026-10-14 核定。", 160.0),
        ("long_token", "超長無空白Tokenabcdefghij中文混排繼續的很長的一個字串結束", 80.0),
        ("cyrillic", "Привет мир 混排 тест", 110.0),
        ("empty", "", 200.0),
        ("single_char", "中", 200.0),
    ]
}

// ---- Phase3-1b：表格幾何黃金集（合併儲存格／巢狀表格／跨頁表格各 1 例）----

fn pen(val: &str, sz: &str, color: &str) -> Option<PenInput> {
    Some(PenInput {
        val: Some(val.to_string()),
        sz: Some(sz.to_string()),
        color: Some(color.to_string()),
    })
}

fn tbl_borders() -> Option<EdgeSetInput> {
    Some(EdgeSetInput {
        top: pen("single", "4", "auto"),
        left: pen("single", "4", "auto"),
        bottom: pen("single", "4", "auto"),
        right: pen("single", "4", "auto"),
        inside_h: pen("single", "4", "auto"),
        inside_v: pen("single", "4", "auto"),
    })
}

fn tcell(text: &str, span: u32, vmerge: Option<bool>) -> CellInput {
    CellInput {
        blocks: if text.is_empty() {
            vec![]
        } else {
            vec![CellBlock::Text(text.to_string())]
        },
        grid_span: span,
        vmerge,
        ..Default::default()
    }
}

fn table_cases() -> Vec<(&'static str, TableInput, f32, u32)> {
    // (id, 輸入, 可用寬 pt, 單頁預算帶數)——跨頁例用窄預算逼出第二切片。
    let grid3 = vec![120.0, 100.0, 100.0];

    // 合併儲存格：gridSpan（第 2 列右兩欄合一）＋ vMerge（第 1 欄跨末兩列）。
    let merged = TableInput {
        grid_pt: grid3.clone(),
        width: Some(WidthSpec::Pt(320.0)),
        borders: tbl_borders(),
        rows: vec![
            RowInput {
                cells: vec![
                    tcell("項目", 1, None),
                    tcell("第一季", 1, None),
                    tcell("第二季", 1, None),
                ],
                ..Default::default()
            },
            RowInput {
                cells: vec![tcell("營收", 1, None), tcell("12,345", 2, None)],
                ..Default::default()
            },
            RowInput {
                cells: vec![
                    tcell("備註", 1, Some(true)),
                    tcell("資料甲", 1, Some(true)),
                    tcell("資料乙", 1, None),
                ],
                ..Default::default()
            },
            RowInput {
                cells: vec![
                    // continue 格**帶非空文字**：快照須證明其內容被忽略
                    // （ECMA-376 語義——渲染時不出現）。
                    tcell("續欄甲", 1, Some(false)),
                    tcell("續欄乙", 1, Some(false)),
                    tcell("尾列", 1, None),
                ],
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    // 巢狀表格：第 2 格內「文字＋子表」交錯（防呆：格內**非空**文字，舊版填空
    // 字串正好掩蓋了文字與子表 y=0 重疊的疊印 bug——快照必須能抓到）。
    let inner = TableInput {
        grid_pt: vec![40.0, 40.0],
        rows: vec![
            RowInput {
                cells: vec![tcell("內甲", 1, None), tcell("內乙", 1, None)],
                ..Default::default()
            },
            RowInput {
                cells: vec![tcell("內丙", 1, None), tcell("內丁", 1, None)],
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let nested = TableInput {
        grid_pt: vec![150.0, 150.0],
        width: Some(WidthSpec::Pt(300.0)),
        borders: tbl_borders(),
        rows: vec![RowInput {
            cells: vec![tcell("外層", 1, None), tcell("外乙", 1, None)],
            ..Default::default()
        }],
        ..Default::default()
    };
    let mut nested = {
        let mut n = nested;
        n.rows[0].cells[1]
            .blocks
            .push(CellBlock::Table(Box::new(inner)));
        n
    };
    // 巢狀例補一列湊足兩列，快照才看得到列間界。
    nested.rows.push(RowInput {
        cells: vec![tcell("末列", 2, None)],
        ..Default::default()
    });

    // 跨頁表格：5 列×每列 1 帶，單頁預算 2 帶 → 切成多片。
    let cross = TableInput {
        grid_pt: vec![90.0, 90.0],
        width: Some(WidthSpec::Pt(180.0)),
        borders: tbl_borders(),
        rows: (0..5)
            .map(|i| RowInput {
                cells: vec![
                    tcell(&format!("列{i}甲"), 1, None),
                    tcell(&format!("列{i}乙"), 1, None),
                ],
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };

    // 格內圖片（t_a1397522）：單格「文字＋圖」堆疊＋第二列純圖格——鎖
    // CellBlock::Image 的帶高累加與跨頁切片可見高度換算（單頁預算 3 帶
    // 逼出跨片續排；快照 "i" 欄捕捉 y/h）。
    let cell_img = TableInput {
        grid_pt: vec![200.0],
        width: Some(WidthSpec::Pt(200.0)),
        borders: tbl_borders(),
        rows: vec![
            RowInput {
                cells: vec![CellInput {
                    blocks: vec![
                        CellBlock::Text("圖上文字".to_string()),
                        CellBlock::Image {
                            w_pt: 120.0,
                            h_pt: 50.0,
                        },
                    ],
                    ..Default::default()
                }],
                ..Default::default()
            },
            RowInput {
                cells: vec![CellInput {
                    blocks: vec![CellBlock::Image {
                        w_pt: 900.0, // 超寬：鉗進格內寬
                        h_pt: 100.0,
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    vec![
        ("merged_cells", merged, 340.0, 64),
        ("nested_table", nested, 340.0, 64),
        ("cross_page", cross, 200.0, 2),
        ("cell_image", cell_img, 240.0, 3),
    ]
}

fn snap_pen(p: &Option<quire_layout::table::Pen>) -> String {
    match p {
        None => "null".to_string(),
        Some(v) => format!(
            "{{\"w\":{:.2},\"c\":\"{:08x}\",\"d\":{}}}",
            v.width_pt,
            v.color_argb,
            match &v.dash {
                None => "null".to_string(),
                Some(d) => json_str(d),
            }
        ),
    }
}

fn snap_frag(f: &quire_layout::table::TableFrag, depth: usize) -> String {
    let mut s = format!(
        "{{\"x\":{:.1},\"y\":{:.1},\"w\":{:.1},\"st\":{},\"co\":{},\"rows\":[",
        f.x_pt, f.y_pt, f.width_pt, f.starts_table, f.continues
    );
    for (i, r) in f.rows.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "{{\"h\":{:.1},\"b0\":{},\"ri\":{},\"sr\":{},\"er\":{},\"cells\":[",
            r.height_pt, r.band_start, r.row_index, r.starts_row, r.ends_row
        ));
        for (j, c) in r.cells.iter().enumerate() {
            if j > 0 {
                s.push(',');
            }
            let edges: Vec<String> = c.edges.iter().map(snap_pen).collect();
            // t_a1397522 起：格內圖片切片進快照（空＝不輸出欄位，舊行位元組
            // 不變；新行帶 "i" 鎖死圖片 y/h 切片幾何）。
            let imgs_json = if c.images.is_empty() {
                String::new()
            } else {
                format!(
                    ",\"i\":[{}]",
                    c.images
                        .iter()
                        .map(|im| format!(
                            "{{\"x\":{:.1},\"y\":{:.1},\"w\":{:.1},\"h\":{:.1}}}",
                            im.x_pt, im.y_pt, im.w_pt, im.h_pt
                        ))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            };
            s.push_str(&format!(
                "{{\"x\":{:.1},\"w\":{:.1},\"e\":[{},{},{},{}],\"fill\":{},\"t\":[{}],\"n\":[{}]{}}}",
                c.x_pt,
                c.w_pt,
                edges[0],
                edges[1],
                edges[2],
                edges[3],
                match c.fill_argb {
                    None => "null".to_string(),
                    Some(v) => format!("\"{v:08x}\""),
                },
                c.texts
                    .iter()
                    .map(|t| format!(
                        "{{\"x\":{:.1},\"y\":{:.1},\"s\":{}}}",
                        t.x_pt,
                        t.y_pt,
                        json_str(&t.text)
                    ))
                    .collect::<Vec<_>>()
                    .join(","),
                c.nested
                    .iter()
                    .map(|(dy, nf)| format!(
                        "[{:.1},{}]",
                        dy,
                        // 巢狀深度護欄：快照只展開兩層（本黃金集最深巢狀一層）。
                        if depth < 1 {
                            snap_frag(nf, depth + 1)
                        } else {
                            "\"…\"".to_string()
                        }
                    ))
                    .collect::<Vec<_>>()
                    .join(","),
                imgs_json,
            ));
        }
        s.push_str("]}");
    }
    s.push_str("]}");
    let _ = depth;
    s
}

fn snap_table(id: &str, input: &TableInput, avail_w: f32, page_bands: u32) -> String {
    let m = TextMeasurer::new(SIZE_PT);
    let tl: TableLayout = layout_table(input, avail_w, 16.8, &m, 0);
    let mut s = format!(
        "{{\"id\":{},\"avail_w\":{:.1},\"page_bands\":{},\"x_off\":{:.1},\"width\":{:.1},\"row_units\":[{}],\"slices\":[",
        json_str(id),
        avail_w,
        page_bands,
        tl.x_off,
        tl.width,
        tl.row_units
            .iter()
            .map(|u| format!("{u:.2}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    // 沿 (row, band) 游標連續切片直到排完（跨頁表格→多片）。
    let mut cur = (0usize, 0u32);
    let mut first = true;
    loop {
        let (frag, next, _used) = tl.fragment(cur.0, cur.1, page_bands, false, 72.0);
        if frag.rows.is_empty() {
            break;
        }
        if !first {
            s.push(',');
        }
        first = false;
        s.push_str(&snap_frag(&frag, 0));
        if next == cur {
            break;
        }
        cur = next;
        if cur.0 >= tl.row_count() {
            break;
        }
    }
    s.push_str("]}");
    s
}

/// 跨頁表格（packer 路徑）golden：真正走 `paginate_tables`——前段 1＋10 列
/// 表（10 帶）＋後段 1，頁面只容 7 帶。鎖的是 `fragment()` 原語快照鎖不到
/// 的 pack_run FlowItem::Table 分支：切片 y 蓋章（非零）、starts_table/
/// continues 旗標、頁起頭 FlowPos、表後文字行帶交接。
fn snap_packer_cross_page() -> String {
    use quire_layout::{layout_paragraphs, paginate_tables, TableSlot};
    let p = LayoutParams {
        page_w_pt: 400.0,
        page_h_pt: 200.0,
        margins_pt: [40.0; 4],
        font_size_pt: SIZE_PT,
        line_spacing: 1.4,
        // 頁底預留 0：golden 鎖的是「無預留」的既有版面（預留 0 逐位元組不變）。
        footnote_reserve_pt: 0.0,
        // 逐頁預留空表：golden 鎖的是「無逐頁預留」的既有版面（空表逐位元組不變）。
        footnote_reserves_pt: Vec::new(),
        endnote_reserves_pt: Vec::new(),
    };
    let input = TableInput {
        grid_pt: vec![100.0],
        width: Some(WidthSpec::Pt(100.0)),
        borders: tbl_borders(),
        rows: (0..10)
            .map(|i| RowInput {
                cells: vec![tcell(&format!("列{i}"), 1, None)],
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let mut texts = vec!["前段".to_string()];
    texts.extend((0..10).map(|i| format!("列{i}")));
    texts.push("後段".to_string());
    let m = TextMeasurer::new(SIZE_PT);
    let tl = layout_table(&input, p.content_w_pt(), p.line_h_pt(), &m, 0);
    let tables = vec![TableSlot {
        para_start: 1,
        para_end: 11,
        layout: tl,
    }];
    let pls = layout_paragraphs(&texts, &p);
    let (pages, starts) = paginate_tables(&pls, &p, &[], &tables);

    let mut s = format!(
        "{{\"id\":\"cross_page_packer\",\"page_bands\":{},\"starts\":[{}],\"pages\":[",
        p.per_page_lines(),
        starts
            .iter()
            .map(|(a, b, c)| format!("[{a},{b},{c}]"))
            .collect::<Vec<_>>()
            .join(",")
    );
    for (i, pg) in pages.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "{{\"bands\":[{}],\"lines\":[{}],\"tables\":[",
            pg.line_bands
                .iter()
                .map(|b| b.to_string())
                .collect::<Vec<_>>()
                .join(","),
            pg.lines
                .iter()
                .map(|l| format!("{{\"p\":{},\"t\":{}}}", l.para_index, json_str(&l.text)))
                .collect::<Vec<_>>()
                .join(",")
        ));
        for (j, tp) in pg.tables.iter().enumerate() {
            if j > 0 {
                s.push(',');
            }
            s.push_str(&format!(
                "{{\"slot\":{},\"anchor\":{},\"band\":{},\"frag\":{}}}",
                tp.slot,
                tp.anchor_para,
                tp.band,
                snap_frag(&tp.frag, 0)
            ));
        }
        s.push_str("]}");
    }
    s.push_str("]}");
    s
}

fn pagination_cases() -> Vec<(&'static str, Vec<String>, LayoutParams)> {
    // 分頁斷點：段落跨頁歸屬（KP 斷行使行數/續排點敏感於斷行演算法）。
    let p = LayoutParams::default();
    let long: Vec<String> = (0..25)
        .map(|i| format!("第 {i} 段：{}", "內文測試文字。".repeat(40)))
        .collect();
    let mixed: Vec<String> = std::iter::once("超長段落：".to_string() + &"字".repeat(3000))
        .chain((0..12).map(|i| format!("短段 {i}：中英 mixed 標點，！？「引號」")))
        .chain(std::iter::once(String::new()))
        .chain((0..6).map(|i| format!("尾段 {i}")))
        .collect();
    vec![
        ("uniform25", long, p.clone()),
        ("mixed_overflow", mixed, p.clone()),
        (
            "narrow_page",
            (0..15)
                .map(|i| format!("窄頁段落 {i}：{}", "文".repeat(300)))
                .collect(),
            LayoutParams {
                page_w_pt: 300.0,
                page_h_pt: 500.0,
                margins_pt: [36.0; 4],
                ..LayoutParams::default()
            },
        ),
        ("single_empty", vec![String::new()], p.clone()),
    ]
}

fn snap_break(id: &str, p: &LayoutParams, brks: &[quire_layout::PageBreak]) -> String {
    // 快照只收 uniform 邊界的文件（本黃金集全部四邊等寬）；margins_pt[0] 即其單一值。
    let mut s = format!(
        "{{\"id\":{},\"params\":{{\"page_w_pt\":{:.1},\"page_h_pt\":{:.1},\"margin_pt\":{:.1},\"font_size_pt\":{:.1},\"line_spacing\":{:.2}}},\"breaks\":[",
        json_str(id),
        p.page_w_pt,
        p.page_h_pt,
        p.margins_pt[0],
        p.font_size_pt,
        p.line_spacing
    );
    for (i, b) in brks.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "[{},{},{},{},{}]",
            b.page, b.first_para, b.last_para, b.line_count, b.continues_paragraph
        ));
    }
    s.push_str("]}");
    s
}

fn snap_line(id: &str, input: &str, max_w: f32, lines: &[Line]) -> String {
    let mut s = format!(
        "{{\"id\":{},\"input\":{},\"max_w_pt\":{:.3},\"lines\":[",
        json_str(id),
        json_str(input),
        max_w
    );
    for (i, l) in lines.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "{{\"text\":{},\"width_pt\":{:.3}}}",
            json_str(&l.text),
            l.width_pt
        ));
    }
    s.push_str("]}");
    s
}

/// 不引依賴的 JSON 字串編碼（golden 需穩定、可 diff）。
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("goldens");
    let m = TextMeasurer::new(SIZE_PT);
    let shaping: Vec<String> = cases()
        .into_iter()
        .map(|(id, input, max_w)| snap_line(id, input, max_w, &break_lines(input, max_w, &m)))
        .collect();
    let pagination: Vec<String> = pagination_cases()
        .into_iter()
        .map(|(id, texts, p)| snap_break(id, &p, &page_breaks(&texts, &p)))
        .collect();
    let tables: Vec<String> = table_cases()
        .into_iter()
        .map(|(id, input, avail_w, page_bands)| snap_table(id, &input, avail_w, page_bands))
        .chain(std::iter::once(snap_packer_cross_page()))
        .collect();

    let update = std::env::var("UPDATE_GOLDENS").is_ok_and(|v| v != "0");
    let mut fails = 0usize;
    let snaps: [(&str, &[String]); 3] = [
        ("shaping.jsonl", &shaping),
        ("pagination.jsonl", &pagination),
        ("tables.jsonl", &tables),
    ];
    for (name, snapshot) in snaps {
        let path = root.join(name);
        if update {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, format!("{}\n", snapshot.join("\n"))).unwrap();
            println!("✓ {name} 已更新（{} 例）", snapshot.len());
            continue;
        }
        let want = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("讀不到 {name}（先 UPDATE_GOLDENS=1 生成）：{e}"));
        for (got, want_line) in snapshot.iter().zip(want.lines()) {
            if got != want_line {
                fails += 1;
                eprintln!("✗ {name} 不符：\n  got  {got}\n  want {want_line}");
            }
        }
        if snapshot.len() != want.lines().count() {
            fails += 1;
            eprintln!(
                "✗ {name} 案例數不符：got {} want {}",
                snapshot.len(),
                want.lines().count()
            );
        }
        println!("✓ {name} 相符（{} 例）", snapshot.len());
    }
    if fails > 0 {
        panic!(
            "{fails} 個 golden 不符——若是刻意的演算法變更，UPDATE_GOLDENS=1 重生成並逐條審 diff"
        );
    }
}
