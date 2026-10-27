//! R5 紅燈：textbox run 錯位（R1）＋發射錯序（R2）——shapes-with-text 治癒。
//!
//! 天然 fixture：`corpus/poi/test-data_document_shapes-with-text.docx`（LO 1 頁）。
//! 契約（報告 §5 R1/R2）：
//! 1. 五個框各為單一同質 Text 段（同 color／size），無 mid-word：同 y 同行的
//!    兩個 Text 項不得出現（本檔每框皆單 run，一行必須是一個完整 Text 項）。
//! 2. 抽取序＝LO 內容流序（報告 §2）：Floating → Square → Ellipse → Where →
//!    A group；把非空 Text 項以單一空格拼接必須逐字等於 LO 整頁文字。
//! 3. 楕圓框（第 2 框）文字項 color＝白（0xFFFFFFFF）。
//! 4. buggy 回塞鎖：`R5_R1_BUGGY=1`／`R5_R2_BUGGY=1` 關閉對應修復後，
//!    本測試的對應斷言必紅（見提交說明實測）。
use quire_engine::Engine;
use quire_render_trait::DrawItem;

const SHAPES: &str = "poi/test-data_document_shapes-with-text.docx";

/// LO 內容流拼接（報告 §2 七個 span 以單空格相接、忽略行內換行位置）。
const LO_JOINED: &str = "Floating text box A square shape with text inside \
An ellipse with text inside Where some contain text A group of shapes";

/// 五個框的完整來源句（文件序＝報告 §3.4；拼接序另由 LO_JOINED 約束）。
const BOXES: [&str; 5] = [
    "Floating text box",
    "A square shape with text inside",
    "An ellipse with text inside",
    "Where some contain text",
    "A group of shapes",
];

#[derive(Debug, Clone)]
struct T {
    text: String,
    x: f32,
    y: f32,
    size: f32,
    color: u32,
}

fn load() -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + SHAPES;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open")
}

/// 單頁內的非空 Text 項（保持顯示列表序＝發射抽取序）。
fn texts(eng: &Engine) -> Vec<T> {
    assert_eq!(eng.page_count(), 1, "本檔 LO 與我方皆 1 頁");
    let dl = eng.page_display_list(0).expect("頁存在");
    dl.items
        .iter()
        .filter_map(|it| match it {
            DrawItem::Text {
                x,
                y,
                size_pt,
                color_argb,
                text,
                ..
            } if !text.trim().is_empty() => Some(T {
                text: text.trim().to_string(),
                x: *x,
                y: *y,
                size: *size_pt,
                color: *color_argb,
            }),
            _ => None,
        })
        .collect()
}

/// 依 LO_JOINED 的片語起點，把每個 Text 項歸到第幾框（0..5）。
fn box_indexes(items: &[T]) -> Vec<usize> {
    let mut starts = Vec::with_capacity(BOXES.len());
    let mut pos = 0usize;
    for b in BOXES {
        starts.push(pos);
        pos += b.chars().count() + 1; // 片語間一個空格
    }
    let mut out = Vec::with_capacity(items.len());
    let mut cur = 0usize;
    for t in items {
        let bi = starts
            .iter()
            .rposition(|&s| cur >= s)
            .unwrap_or(0);
        out.push(bi);
        cur += t.text.chars().count() + 1;
    }
    out
}

#[test]
fn r5_txbx_no_midword_homogeneous_white_ellipse() {
    let items = texts(&load());

    // 紅燈 1a：同 y 同行的兩個 Text 項＝run 錯位把整行在詞中鋸開（本檔每框
    // 皆單 run，正確態一行恰一項）。
    for w in items.windows(2) {
        assert!(
            (w[0].y - w[1].y).abs() > 0.01,
            "同 y 同行雙發（mid-word 鋸詞）：{:?} | {:?}",
            w[0],
            w[1]
        );
    }

    // 紅燈 1b：拼接＝LO 整頁文字（順序＋逐字，mid-word 會多出空白）。
    let joined = items
        .iter()
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(joined, LO_JOINED, "文字框抽取序／內容 ≠ LO 內容流序");

    // 紅燈 1c：各框同質（同 size／color），楕圓框（第 2 框）必為白字。
    let bi = box_indexes(&items);
    for (b, phrase) in BOXES.iter().enumerate() {
        let idxs: Vec<usize> = bi
            .iter()
            .enumerate()
            .filter_map(|(i, &x)| (x == b).then_some(i))
            .collect();
        assert!(!idxs.is_empty(), "框「{phrase}」沒有任何 Text 項");
        let first = &items[idxs[0]];
        for &i in &idxs {
            assert!(
                (items[i].size - first.size).abs() < 0.01,
                "框「{phrase}」字級不同質：{:?}",
                idxs.iter().map(|&i| &items[i]).collect::<Vec<_>>()
            );
            assert_eq!(
                items[i].color, first.color,
                "框「{phrase}」顏色不同質：{:?}",
                idxs.iter().map(|&i| &items[i]).collect::<Vec<_>>()
            );
        }
    }
    let ellipse: Vec<&T> = bi
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == 2).then_some(&items[i]))
        .collect();
    assert!(
        ellipse.iter().all(|t| t.color == 0xFFFF_FFFF),
        "楕圓框（第 2 框）必須發白字（來源 w:color FFFFFF），實際 {ellipse:?}"
    );
    // 對照：Where（第 3 框）為黑字，證明顏色不是整頁染白。
    assert!(
        bi.iter()
            .enumerate()
            .filter(|(_, &b)| b == 3)
            .all(|(i, _)| items[i].color == 0xFF00_0000),
        "Where some contain text（第 3 框）必須為黑字"
    );
}

#[test]
fn r5_txbx_extraction_order_is_lo() {
    let items = texts(&load());
    // 各框首見次序：Floating(0)→Square(1)→Ellipse(2)→Where(3)→A group(4)。
    let bi = box_indexes(&items);
    let mut first_seen = Vec::new();
    for &b in &bi {
        if first_seen.last() != Some(&b) {
            first_seen.push(b);
        }
    }
    assert_eq!(first_seen, vec![0, 1, 2, 3, 4], "框序 ≠ LO 內容流序");

    // 群組兩框（Where 383pt 先於 group 352pt）——LO 內容流把 Where 排在
    // 「A group of shapes」之前；y-sort 會顛倒（R2 主兇）。
    let where_pos = items.iter().position(|t| t.text.starts_with("Where some")).unwrap();
    let group_pos = items
        .iter()
        .position(|t| t.text == "A group of shapes")
        .unwrap();
    assert!(
        where_pos < group_pos,
        "群組子框序必須 Where → A group（LO 內容流），實際 Where@{where_pos} group@{group_pos}"
    );
}
