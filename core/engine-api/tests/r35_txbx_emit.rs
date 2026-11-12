//! R35 紅燈（發射層）：VML 框內嵌 w:drawing 採集狀態機缺口（卡 1/2/4）。
//!
//! 考卷＝`fixtures/r35_txbx_state.docx`（`gen_r35_fixture.py` selfgen，
//! 四張辨色像素圖 A 紅 2×2／B 綠 3×2／C 藍 2×2／D 灰 1×1，base64 可判源）。
//! 採集層五盞燈（卡 1..5 的 A/B 鑑別力）在 `core/ooxml/src/drawing.rs`
//! 單元測試；本檔吃**端到端發射**形態：
//! - 燈 1（卡 1 同段多圖只發第一張）：框1 同段 A＋B 兩張必須**都**發，
//!   加外框自身 imagedata C 共 3 張；`.find` 單發態（BUGGY）少一張。
//! - 燈 2（卡 3 誤殺的發射面後果）：C（藍）必須以**外框自身幾何**
//!   200×140 發射（BUGGY 態 C 被整批誤殺 ⇒ 退 200×140 灰色佔位 Rect）。
//! - 燈 3（卡 4 缺 extent 0×0 灰塊）：全頁不得出現 w≤0 或 h≤0 的 Rect
//!   （BUGGY 態 `ext.unwrap_or([0,0])` 必產一枚 ⇒ 紅）。
//! - 燈 4（卡 2 次要 blip 覆寫 rid）：框5 先 embed 後 link-only，A（紅）
//!   必須存活（BUGGY 態 None 蓋掉 rIdA ⇒ A 全滅）。
//! - 燈 5（零擾動／防假綠）：查無像素的框（BOX2／BOX5 宿主無 imagedata）
//!   照舊發誠實佔位灰 Rect＋四條框線；框內文字逐段照發——把發射一刀掐掉
//!   的錯誤修法會被本燈打紅。
//! - 卡 5（狀態洩漏）：畸形閉合形態在 corpus 級 parse() 的 well-formedness
//!   閘之前即被整檔拒絕（LO／Word 亦不產出），老實**不進 docx 考卷**；
//!   洩漏防禦由 drawing.rs 單元燈 `r35_txbx_content_close_flushes_frame_no_leak`
//!   以關閂直餵事件層覆蓋（誠實註記，不虛構通過）。
//!
//! 回塞鎖 `R35_TXBX_BUGGY=1`：把採集＋發射打回 R31 舊態（單槽覆寫＋挪用
//! `group_shapes`＋`.find` 單發＋finalize 整批誤殺＋0×0 入列）。燈 1..4 於
//! 回塞態必須轉紅（鑑別力由同一支測試內的 buggy 段落自證，k25b 紀律：
//! env 只在單一測試內依序設／清，不平行汙染）。

use flate2::read::ZlibDecoder;
use quire_engine::Engine;
use quire_render_trait::DrawItem;
use std::io::Read;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/r35_txbx_state.docx"
);

fn load() -> Engine {
    let bytes = std::fs::read(FIXTURE).expect("讀 r35_txbx_state.docx（先跑 gen_r35_fixture.py）");
    Engine::from_docx_bytes(&bytes).expect("open")
}

/// 頁 0 展開：Image（幾何＋首像素 RGB）／Rect／Text。
#[derive(Debug)]
struct Img {
    w: f32,
    h: f32,
    rgb: (u32, u32, u32),
}

#[derive(Debug)]
struct Scan {
    images: Vec<Img>,
    rects: Vec<(f32, f32, f32, f32, u32)>,
    texts: Vec<String>,
}

fn scan(eng: &Engine) -> Scan {
    let dl = eng.page_display_list(0).expect("頁 0 存在");
    let mut s = Scan {
        images: Vec::new(),
        rects: Vec::new(),
        texts: Vec::new(),
    };
    for it in &dl.items {
        match it {
            DrawItem::Image { w, h, data_b64, .. } => s.images.push(Img {
                w: *w,
                h: *h,
                rgb: first_pixel(data_b64),
            }),
            DrawItem::Rect {
                x,
                y,
                w,
                h,
                fill_argb,
            } => s.rects.push((*x, *y, *w, *h, *fill_argb)),
            DrawItem::Text { text, .. } => s.texts.push(text.clone()),
            _ => {}
        }
    }
    s
}

/// base64 → PNG 首像素 RGB（考卷四張圖皆 color type 2／filter 0，最小解法）。
fn first_pixel(b64: &str) -> (u32, u32, u32) {
    let bytes = base64_decode(b64).expect("發射像素 base64 可解");
    assert_eq!(&bytes[0..8], b"\x89PNG\r\n\x1a\n", "發射像素必須是 PNG");
    let mut pos = 8;
    let mut idat = Vec::new();
    let mut ctype = 0u8;
    while pos + 8 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        let typ = &bytes[pos + 4..pos + 8];
        let body = &bytes[pos + 8..pos + 8 + len];
        match typ {
            b"IHDR" => ctype = body[9],
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        pos += 12 + len;
    }
    assert_eq!(ctype, 2, "考卷像素圖＝RGB8");
    let mut raw = Vec::new();
    ZlibDecoder::new(&idat[..])
        .read_to_end(&mut raw)
        .expect("IDAT 可解");
    assert_eq!(raw[0], 0, "filter 0");
    (u32::from(raw[1]), u32::from(raw[2]), u32::from(raw[3]))
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut v = [0u8; 4];
    let mut n = 0usize;
    let mut out = Vec::new();
    for ch in s.bytes() {
        if ch == b'=' {
            break;
        }
        let idx = T.iter().position(|&c| c == ch)?;
        v[n] = idx as u8;
        n += 1;
        if n == 4 {
            let x = (u32::from(v[0]) << 18)
                | (u32::from(v[1]) << 12)
                | (u32::from(v[2]) << 6)
                | u32::from(v[3]);
            out.push((x >> 16) as u8);
            out.push((x >> 8) as u8);
            out.push(x as u8);
            n = 0;
        }
    }
    Some(out)
}

/// 源圖辨色（考卷 selfgen 固定，非抄實測——生成器即規格）。
const RED: (u32, u32, u32) = (255, 0, 0);
const GREEN: (u32, u32, u32) = (0, 200, 0);
const BLUE: (u32, u32, u32) = (0, 0, 255);

/// 燈 1／2／3／4 正常態＋回塞態鑑別力（單一支測試內依序 A/B，k25b 紀律）。
#[test]
fn r35_emits_all_textbox_embedded_drawings() {
    // ---- 正常態 ----
    let eng = load();
    let s = scan(&eng);

    // 燈 1（卡 1）：框1 同段 A＋B 都發＋宿主 C＝3 張；全檔共 5 張。
    let reds: Vec<&Img> = s.images.iter().filter(|i| i.rgb == RED).collect();
    let greens: Vec<&Img> = s.images.iter().filter(|i| i.rgb == GREEN).collect();
    let blues: Vec<&Img> = s.images.iter().filter(|i| i.rgb == BLUE).collect();
    assert_eq!(
        s.images.len(),
        5,
        "框1 三張（C 宿主＋A＋B）＋框5 兩張（A′＋B′）＝5：{:?}",
        s.images
    );
    assert!(
        reds.iter()
            .any(|i| (i.w - 40.0).abs() < 0.01 && (i.h - 30.0).abs() < 0.01),
        "A（紅 40×30）必須在列——同段第 2 張 B 不得把它擠掉：{:?}",
        s.images
    );
    assert!(
        greens
            .iter()
            .any(|i| (i.w - 30.0).abs() < 0.01 && (i.h - 20.0).abs() < 0.01),
        "B（綠 30×20，同段第 2 張）必須發射（R31 `.find` 只發第一張＝缺陷本體）：{:?}",
        s.images
    );
    // 燈 2（卡 3 誤殺）：宿主 C 以**外框自身幾何**發射（非內嵌圖 extent）。
    assert_eq!(
        blues.len(),
        1,
        "框1 宿主 imagedata C 必須存活（誤殺臂根除）"
    );
    let c = &blues[0];
    assert!(
        (c.w - 200.0).abs() < 0.01 && (c.h - 140.0).abs() < 0.01,
        "C 尺寸＝外框 style 200×140，不得被框內某張圖的 wp:extent 搶走：{c:?}",
    );
    // 燈 3（卡 4）：零退化 Rect。
    let degen: Vec<_> = s
        .rects
        .iter()
        .filter(|(_, _, w, h, _)| *w <= 0.0 || *h <= 0.0)
        .collect();
    assert!(
        degen.is_empty(),
        "缺 extent 不得刷 0×0 灰塊吞空段步進（應老實不入列）：{:?}",
        s.rects
    );
    // 燈 4（卡 2）：link-only blip 不得搶走已捕 rid。
    assert!(
        reds.iter()
            .any(|i| (i.w - 25.0).abs() < 0.01 && (i.h - 15.0).abs() < 0.01),
        "框5 先 embed 後 link-only 的 A′（紅 25×15）必須存活（首見為準）：{:?}",
        s.images
    );
    assert!(
        greens
            .iter()
            .any(|i| (i.w - 18.0).abs() < 0.01 && (i.h - 12.0).abs() < 0.01),
        "框5 B′（綠 18×12）同段照發：{:?}",
        s.images
    );
    // 燈 5（防假綠）：框內文字逐段照發。
    for want in ["首段", "末段", "无图段", "步进步照", "link段", "R35宿主段"] {
        assert!(
            s.texts.iter().any(|t| t == want),
            "框內／宿主文字 {want:?} 不得被發射端改動弄丟：{:?}",
            s.texts
        );
    }
    let no_pixel_frames = s
        .rects
        .iter()
        .filter(|(_, _, w, h, f)| *f == 0xFFF0_F0F0 && *w > 0.0 && *h > 0.0)
        .count();
    assert!(
        no_pixel_frames >= 2,
        "查無像素的框（BOX2／BOX5 宿主）照舊誠實佔位灰（R34/K4c 口徑零擾動）：{:?}",
        s.rects
    );

    // ---- 回塞態（鑑別力：燈 1/2/3/4 必紅）----
    std::env::set_var("R35_TXBX_BUGGY", "1");
    let bug_eng = load();
    let b = scan(&bug_eng);
    std::env::remove_var("R35_TXBX_BUGGY");
    let b_degen: Vec<_> = b
        .rects
        .iter()
        .filter(|(_, _, w, h, _)| *w <= 0.0 || *h <= 0.0)
        .collect();
    assert!(
        !b_degen.is_empty(),
        "回塞態必須重現 0×0 灰塊（否則燈 3 未真正約束採集期擋件）：{:?}",
        b.rects
    );
    let b_red_40 = b
        .images
        .iter()
        .any(|i| i.rgb == RED && (i.w - 40.0).abs() < 0.01);
    let b_green_30 = b
        .images
        .iter()
        .any(|i| i.rgb == GREEN && (i.w - 30.0).abs() < 0.01);
    assert!(
        !(b_red_40 && b_green_30),
        "回塞態同段兩張必缺一張（發射端 `.find` 單發＝卡 1 缺陷本體）：{:?}",
        b.images
    );
    assert!(
        b.images.iter().all(|i| i.rgb != BLUE),
        "回塞態 finalize 整批誤殺 ⇒ 宿主 C（藍）退灰色佔位 Rect（卡 3 缺陷本體）：{:?}",
        b.images
    );
    assert!(
        !b.images
            .iter()
            .any(|i| i.rgb == RED && (i.w - 25.0).abs() < 0.01),
        "回塞態 link-only blip 的 None 覆寫 ⇒ A′ 全滅（卡 2 缺陷本體）：{:?}",
        b.images
    );
    for want in ["首段", "末段", "link段"] {
        assert!(
            b.texts.iter().any(|t| t == want),
            "回塞態文字仍在（兩態只差在圖形採集，本鎖斷的是同一件事）"
        );
    }
}

/// 燈 6（卡 4 的排版面副產物鎖）：缺 extent 段落的**步進不被吞**——
/// 「步進步照」與「無圖段」的基線距在兩態必須相同（BUGGY 態 0×0 Rect
/// 走 has_image 分支跳過空段步進，本燈把該偏移量化留檔）。
#[test]
fn r35_missing_extent_paragraph_does_not_swallow_step() {
    let base = text_y(&load(), "步进步照") - text_y(&load(), "无图段");
    std::env::set_var("R35_TXBX_BUGGY", "1");
    let bug = text_y(&load(), "步进步照") - text_y(&load(), "无图段");
    std::env::remove_var("R35_TXBX_BUGGY");
    // 兩態同值＝0×0 灰塊在此 fixture 未吞掉步進（誠實記錄：本檔燈 3 已直接
    // 斷 Rect 退化形態，本燈只鎖「不被本卡改動弄出額外位移」的零擾動面）。
    assert!(
        (base - bug).abs() < 0.01,
        "空段步進基線距 修前={base:.2} 修後={bug:.2} 應一致（零擾動）"
    );
}

fn text_y(eng: &Engine, want: &str) -> f32 {
    let dl = eng.page_display_list(0).expect("頁 0");
    for it in &dl.items {
        if let DrawItem::Text { text, y, .. } = it {
            if text == want {
                return *y;
            }
        }
    }
    panic!("找不到文字 {want:?}");
}
