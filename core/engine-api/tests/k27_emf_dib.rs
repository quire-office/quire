//! K27：EMF 內嵌 DIB 直讀——fdo76591 色軸地板（P5 處方路線 b）。
//!
//! 卡面口徑（`docs/p5-emf-diag.md` §1-3 為實據）：
//! - `corpus/ooxmlexport/fdo76591.docx` p0 色軸 0.000：LO 側 1.386% 彩色覆蓋
//!   **全部**來自 `word/media/image1.emf`（166×183）與 `image2.emf`（750×122）
//!   內嵌的 `EMR_STRETCHDIBITS`（16bpp BI_RGB）。我方現行 K4c-image 對 EMF 只
//!   發灰佔位框 ⇒ 彩色覆蓋 0。
//! - 處方＝不寫完整 EMF 向量解碼器，只直讀內嵌位圖抽出來當圖片渲染；
//!   **尺寸口徑用 EMF 檔頭 `rclFrame`**（0.01mm→pt），不用 `wp:extent`／VML
//!   shape box（P5 實測我方 box 比 LO 大 7~30%）。
//! - 退化序：EMF 有可抽 DIB ⇒ 抽圖渲染；無 DIB（純 vector，如 n779834
//!   浮水印）⇒ 維持既有灰佔位框（燈3 零回退鎖）。
//!
//! 斷言走 **JSON 層**（`page_display_list_json`）：紅燈階段（EMF 仍退佔位框）
//! 測試照常編譯、老實失敗，不是編譯錯。
//!
//! 回塞鎖：`K27_EMF_DIB_BUGGY=1` ⇒ 解析層關掉直讀 ⇒ 燈1／燈2 必紅。

use quire_engine::Engine;

/// fdo76591 的兩顆 EMF：rclFrame 換算尺寸（P5-diag §1-3，0.01mm×72/2540）。
const FRAME_IMG1_PT: (f32, f32) = (83.93, 93.15);
const FRAME_IMG2_PT: (f32, f32) = (210.73, 34.21);

/// 從單頁顯示列表抽出所有 Image 項。
fn image_items(json: &str) -> Vec<serde_json::Map<String, serde_json::Value>> {
    let v: serde_json::Value = serde_json::from_str(json).expect("顯示列表 JSON 可解析");
    v["items"]
        .as_array()
        .expect("items 陣列")
        .iter()
        .filter_map(|it| it.get("Image").and_then(|m| m.as_object()).cloned())
        .collect()
}

/// 圖片佔位框（fill＝IMAGE_PLACEHOLDER_FILL＝0xFFF0F0F0）數。
fn placeholder_rects(json: &str) -> usize {
    let v: serde_json::Value = serde_json::from_str(json).expect("顯示列表 JSON 可解析");
    v["items"]
        .as_array()
        .expect("items 陣列")
        .iter()
        .filter(|it| {
            it.get("Rect")
                .and_then(|r| r.get("fill_argb"))
                .and_then(|f| f.as_u64())
                == Some(0xFFF0_F0F0)
        })
        .count()
}

fn read_corpus(rel: &str) -> Vec<u8> {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("../../corpus");
    p.push(rel);
    std::fs::read(&p).unwrap_or_else(|e| panic!("讀 {}: {e}", p.display()))
}

/// PNG magic（base64 前綴 `iVBORw0KGgo`）。
fn is_png_b64(b64: &str) -> bool {
    b64.starts_with("iVBORw0KGgo")
}

/// 燈1（卡面燈1）：fdo76591 渲染後必須以**真像素**發射兩張 EMF 圖，且尺寸
/// ＝ EMF 檔頭 `rclFrame` 換算值（±1pt），不是灰佔位框。
///
/// 修前：EMF 走 `mime_of_media_path → None` ⇒ 全檔 0 個 Image、2 個佔位框。
#[test]
fn k27_lamp1_fdo76591_emf_emits_two_image_items_at_rclframe_size() {
    let eng = Engine::from_docx_bytes(&read_corpus("ooxmlexport/fdo76591.docx")).expect("open");
    let mut imgs: Vec<(f64, f64, String)> = Vec::new();
    let mut rects = 0usize;
    for page in 0..eng.page_count() {
        let json = eng.page_display_list_json(page).expect("頁存在");
        for im in image_items(&json) {
            imgs.push((
                im["w"].as_f64().expect("w"),
                im["h"].as_f64().expect("h"),
                im["data_b64"].as_str().expect("data_b64").to_string(),
            ));
        }
        rects += placeholder_rects(&json);
    }
    // 3 個＝image1.emf＋image2.emf（本卡標的）＋image3.png（P5-diag §3-3
    // 記載的「PNG 內嵌圖缺失」——同為 blip rId 漏收的受害者，一併修復）。
    assert_eq!(
        imgs.len(),
        3,
        "燈1：fdo76591 必須以真像素發射 2 張 EMF 圖＋1 張 PNG（共 3 個 Image）；實得 {} 個 Image、{rects} 個佔位框",
        imgs.len()
    );
    assert_eq!(rects, 0, "燈1：三張圖全數真像素發射 ⇒ 不得殘留任何灰佔位框");
    for (_w, _h, b64) in &imgs {
        assert!(
            is_png_b64(b64),
            "燈1：EMF 像素必須是重編好的 PNG（發射層白名單格式，K19 duotone 鏈只認 PNG）"
        );
    }
    let near = |a: f64, b: f32| (a - b as f64).abs() <= 1.0;
    let hit1 = imgs
        .iter()
        .any(|(w, h, _)| near(*w, FRAME_IMG1_PT.0) && near(*h, FRAME_IMG1_PT.1));
    let hit2 = imgs
        .iter()
        .any(|(w, h, _)| near(*w, FRAME_IMG2_PT.0) && near(*h, FRAME_IMG2_PT.1));
    assert!(
        hit1 && hit2,
        "燈1：尺寸必須吻合 rclFrame 換算值（image1 {:.1}×{:.1}pt／image2 {:.1}×{:.1}pt，±1pt）；實得 {imgs:?}",
        FRAME_IMG1_PT.0,
        FRAME_IMG1_PT.1,
        FRAME_IMG2_PT.0,
        FRAME_IMG2_PT.1,
    );
    // 尺寸口徑反鎖：我方 `wp:extent` 是 84×93／210.75×34.5，VML box 是
    // 99.95×100.2／225×45。後者是 P5 實測「比 LO 大 7~30%」的來源——照 VML
    // box 發射 ⇒ 尺寸必越界，本斷言必紅。
    let vml_leak = imgs
        .iter()
        .any(|(w, h, _)| (w - 99.95).abs() < 0.6 && (h - 100.2).abs() < 0.6);
    assert!(
        !vml_leak,
        "燈1：尺寸口徑不得用 VML shape box（99.95×100.2pt）——P5-diag 實測比 LO 大 7~30%"
    );
}

/// 燈1b：真像素必須真的帶色彩（色軸地板的根因是「彩色覆蓋 0」，只發一張全
/// 灰圖也會過燈1——本燈把「抽出來的像素確實是彩色」鎖死）。
#[test]
fn k27_lamp1b_fdo76591_emf_pixels_are_colorful() {
    let eng = Engine::from_docx_bytes(&read_corpus("ooxmlexport/fdo76591.docx")).expect("open");
    let mut found_color = false;
    let mut checked = 0usize;
    for page in 0..eng.page_count() {
        let json = eng.page_display_list_json(page).expect("頁存在");
        for im in image_items(&json) {
            let b64 = im["data_b64"].as_str().expect("data_b64");
            if !is_png_b64(b64) {
                continue;
            }
            // **只量 EMF 兩張**（按 rclFrame 尺寸認圖）：本檔另有 image3.png
            // （背景圖）也是真彩色像素，若不篩尺寸，回塞鎖關掉 EMF 直讀後
            // 本燈仍會因那張 PNG 而綠 ⇒ 假鎖（PM 實測 BUGGY 態確實如此）。
            let (iw, ih) = (im["w"].as_f64(), im["h"].as_f64());
            let is_emf = matches!((iw, ih), (Some(w), Some(h))
                if ((w - FRAME_IMG1_PT.0 as f64).abs() < 1.0
                    && (h - FRAME_IMG1_PT.1 as f64).abs() < 1.0)
                    || ((w - FRAME_IMG2_PT.0 as f64).abs() < 1.0
                        && (h - FRAME_IMG2_PT.1 as f64).abs() < 1.0));
            if !is_emf {
                continue;
            }
            checked += 1;
            // **不得在此 break**：兩張 EMF 都要量過，否則第一張帶色就收工，
            // 第二張漏色會漏網（PM 實測：早退時 checked=1 直接誤紅）。
            let px = png_rgba(decode_b64(b64));
            let colorful = px.chunks(4).any(|c| {
                let (r, g, b) = (c[0] as i32, c[1] as i32, c[2] as i32);
                r.max(g).max(b) - r.min(g).min(b) > 24
            });
            if colorful {
                found_color = true;
            }
        }
    }
    assert_eq!(
        checked, 2,
        "燈1b：必須量到兩張 EMF 圖（按 rclFrame 尺寸辨識）；實得 {checked}（BUGGY 態＝EMF 退佔位框 ⇒ 此燈必紅）"
    );
    assert!(
        found_color,
        "燈1b：EMF 抽出的 DIB 像素必須含彩色（色軸地板根因＝彩色覆蓋 0.000；全灰圖不該算過關）"
    );
}

/// 燈3（卡面燈3）：純 vector EMF（無可直讀 DIB）仍發灰佔位框——n779834 零
/// 回退鎖。本檔的 EMF 是 GDI+ 向量浮水印（182 筆記錄，BEGINPATH/FILLPATH×16），
/// `emf_embedded_dib_checked` 走 `VectorPaint` 否決 ⇒ 發射層必須保持佔位框。
#[test]
fn k27_lamp3_pure_vector_emf_still_placeholder() {
    let eng = Engine::from_docx_bytes(&read_corpus("ooxmlexport/n779834.docx")).expect("open");
    let mut imgs = 0usize;
    let mut rects = 0usize;
    for page in 0..eng.page_count() {
        let json = eng.page_display_list_json(page).expect("頁存在");
        imgs += image_items(&json).len();
        rects += placeholder_rects(&json);
    }
    assert_eq!(
        imgs, 0,
        "燈3：純 vector EMF 不得發 Image——只貼內嵌 DIB 就是虛構浮水印（該圖的藍色來自 vector fill，本卡不做）"
    );
    assert!(
        rects >= 1,
        "燈3：n779834 的 EMF 必須維持既有灰佔位框（實得 {rects} 個）"
    );
}

/// 燈2（卡面燈2）：合成最小檔——自產含 `EMR_STRETCHDIBITS`（16bpp BI_RGB）的
/// EMF 塞進 docx media，渲染必須出圖。5-5-5（BI_RGB）與 5-6-5（BI_BITFIELDS）
/// 兩變體各一。
///
/// 為什麼必紅：修前 EMF 走 `mime_of_media_path → None`，合成檔同樣只發佔位框。
#[test]
fn k27_lamp2_synthetic_16bpp_emf_variants_render() {
    for (name, variant) in [
        ("5-5-5 BI_RGB", Variant::B555),
        ("5-6-5 BI_BITFIELDS", Variant::B565),
    ] {
        let docx = docx_with_emf(variant);
        let eng = Engine::from_docx_bytes(&docx).unwrap_or_else(|e| panic!("{name} 開檔失敗: {e}"));
        let mut hit: Option<(f64, f64, String)> = None;
        let mut rects = 0usize;
        for page in 0..eng.page_count() {
            let json = eng.page_display_list_json(page).expect("頁存在");
            for im in image_items(&json) {
                hit = Some((
                    im["w"].as_f64().expect("w"),
                    im["h"].as_f64().expect("h"),
                    im["data_b64"].as_str().expect("data_b64").to_string(),
                ));
            }
            rects += placeholder_rects(&json);
        }
        let (w, h, b64) = hit.unwrap_or_else(|| {
            panic!("{name}：合成 EMF 必須發 DrawItem::Image（實得 0 個 Image、{rects} 個佔位框）")
        });
        assert!(is_png_b64(&b64), "{name}：像素必須是 PNG");
        // 合成 rclFrame＝2000×2000 個 0.01mm ⇒ 56.69×56.69pt（**不是** wp:extent
        // 的 100×100pt ⇒ 本斷言同時鎖死 rclFrame 口徑）。
        assert!(
            (w - 56.69).abs() <= 1.0 && (h - 56.69).abs() <= 1.0,
            "{name}：尺寸必須＝rclFrame 換算 56.69×56.69pt（±1），實得 {w}×{h}（若得 100×100＝誤用 wp:extent）"
        );
        // 像素內容驗證：合成 DIB 是 2×2（頂列紅／白或紅／黑遮罩組合），
        // 解出來必須有純紅像素（R 滿、G/B 空）。
        let px = png_rgba(decode_b64(&b64));
        let has_red = px.chunks(4).any(|c| c[0] > 200 && c[1] < 40 && c[2] < 40);
        assert!(
            has_red,
            "{name}：解出的像素必須含紅色（頂列 0xF800／0xF800）"
        );
    }
}

enum Variant {
    B555,
    B565,
}

// ───────────────────────── 合成 EMF ─────────────────────────
//
// 欄位版式對齊真實語料（fdo76591 hex dump，記錄起點為基準）：
//   +48=offBmi +52=cbBmi +56=offBits +60=cbBits +64=iRopColorUse +68=iRop

const EMR_HEADER: u32 = 1;
const EMR_STRETCHDIBITS: u32 = 81;
const BI_RGB: u32 = 0;
const BI_BITFIELDS: u32 = 3;
const SRCCOPY: u32 = 0x00CC_0020;

fn u32le(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

fn u16le(v: u16) -> [u8; 2] {
    v.to_le_bytes()
}

fn i32le(v: i32) -> [u8; 4] {
    v.to_le_bytes()
}

/// 88B 檔頭（rclFrame 2000×1000 個 0.01mm ⇒ 56.69×28.35pt）＋記錄串。
fn wrap_emf(frame: (i32, i32), recs: &[Vec<u8>]) -> Vec<u8> {
    let n_size = 88usize;
    let total: usize = n_size + recs.iter().map(|r| r.len()).sum::<usize>();
    let mut out = vec![0u8; n_size];
    out[0..4].copy_from_slice(&u32le(EMR_HEADER));
    out[4..8].copy_from_slice(&u32le(n_size as u32));
    out[24..28].copy_from_slice(&i32le(0));
    out[28..32].copy_from_slice(&i32le(0));
    out[32..36].copy_from_slice(&i32le(frame.0));
    out[36..40].copy_from_slice(&i32le(frame.1));
    out[40..44].copy_from_slice(b" EMF");
    out[48..52].copy_from_slice(&u32le(total as u32));
    out[52..56].copy_from_slice(&u32le((recs.len() + 1) as u32));
    for r in recs {
        out.extend_from_slice(r);
    }
    out
}

/// 2×2、16bpp、bottom-up（stride＝4B＝恰兩像素，無補齊）。
fn dib16(rows_bottom_first: [[u16; 2]; 2]) -> Vec<u8> {
    let mut b = Vec::new();
    for row in rows_bottom_first {
        for px in row {
            b.extend_from_slice(&px.to_le_bytes());
        }
    }
    b
}

/// 造 16bpp `EMR_STRETCHDIBITS`（記錄相對版式）。
fn stretch_dib_16(variant: &Variant, bits: &[u8]) -> Vec<u8> {
    let (comp, masks) = match variant {
        Variant::B555 => (BI_RGB, None),
        Variant::B565 => (BI_BITFIELDS, Some((0xF800u32, 0x07E0, 0x001F))),
    };
    // 遮罩走 BIH 延伸（cbBmi=52）⇒ 像素區無前綴。
    let bih_len: usize = if masks.is_some() { 52 } else { 40 };
    let mut rec: Vec<u8> = vec![0u8; 80];
    rec[0..4].copy_from_slice(&u32le(EMR_STRETCHDIBITS));
    rec[40..44].copy_from_slice(&i32le(2));
    rec[44..48].copy_from_slice(&i32le(2));
    let mut bih = vec![0u8; bih_len];
    bih[0..4].copy_from_slice(&u32le(bih_len as u32));
    bih[4..8].copy_from_slice(&i32le(2));
    bih[8..12].copy_from_slice(&i32le(2));
    bih[12..14].copy_from_slice(&u16le(1));
    bih[14..16].copy_from_slice(&u16le(16));
    bih[16..20].copy_from_slice(&u32le(comp));
    if let Some((rm, gm, bm)) = masks {
        bih[40..44].copy_from_slice(&u32le(rm));
        bih[44..48].copy_from_slice(&u32le(gm));
        bih[48..52].copy_from_slice(&u32le(bm));
    }
    let off_bmi = 80usize;
    let off_bits = off_bmi + bih_len;
    rec[48..52].copy_from_slice(&u32le(off_bmi as u32));
    rec[52..56].copy_from_slice(&u32le(bih_len as u32));
    rec[56..60].copy_from_slice(&u32le(off_bits as u32));
    rec[60..64].copy_from_slice(&u32le(bits.len() as u32));
    rec[64..68].copy_from_slice(&u32le(0)); // iRopColorUse＝0
    rec[68..72].copy_from_slice(&u32le(SRCCOPY));
    rec.extend_from_slice(&bih);
    rec.extend_from_slice(bits);
    let n = rec.len();
    rec[4..8].copy_from_slice(&u32le(n as u32));
    rec
}

/// 自產 docx：最小正文＋一張 `wp:inline` 圖指向合成的 `word/media/image1.emf`。
///
/// 走「真實 corpus 檔＋`ZipBundle` 覆寫」而非手刻 zip 容器：借用引擎自己的
/// zip 寫路徑（crc/sizes 由 `Change::Replace`／`extras` 處理），測試只負責
/// XML 與 EMF 本體，不引入第二套 zip 實作。
fn docx_with_emf(variant: Variant) -> Vec<u8> {
    let bits = match variant {
        // 頂列（像素區第二列）＝(0xF800 紅, 0x7FFF 白)；底列＝(0x001F 藍, 0x07C0 綠)。
        Variant::B555 => dib16([[0x001F, 0x07C0], [0xF800, 0x7FFF]]),
        Variant::B565 => dib16([[0x001F, 0x07E0], [0xF800, 0xFFFF]]),
    };
    // rclFrame 與 DIB 同長寬比（2×2 ⇒ 正方形）：不同比時閘 2 正確否決，
    // 合成檔要測的是「可直讀 ⇒ 發射」，不是閘 2（閘 2 由 ooxml 層單元測試鎖）。
    let emf = wrap_emf((2000, 2000), &[stretch_dib_16(&variant, &bits)]);

    let base = read_corpus("selfgen/patho_autofit_implicit.docx");
    let bundle = quire_ooxml::ZipBundle::open(&base).expect("開 base");
    let part = |name: &str| -> String {
        String::from_utf8(
            bundle
                .entry(name)
                .unwrap_or_else(|| panic!("base 缺 part {name}"))
                .decoded()
                .expect("解壓 base part"),
        )
        .expect("base part 是 UTF-8")
    };
    let doc_xml = part("word/document.xml");
    let rels_xml = part("word/_rels/document.xml.rels");
    // extent 100×100pt＝1270000 EMU：**故意**與 rclFrame（56.69×28.35）不同，
    // 讓尺寸斷言能分辨口徑取自哪裡。
    let drawing = "<w:p><w:r><w:drawing>\
         <wp:inline xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\">\
           <wp:extent cx=\"1270000\" cy=\"1270000\"/>\
           <a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\
             <a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
               <pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
                 <a:blip xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:embed=\"rIdEmf\"/>\
               </pic:pic>\
             </a:graphicData>\
           </a:graphic>\
         </wp:inline></w:drawing></w:r></w:p>";
    let doc_new = doc_xml.replace("</w:body>", &format!("{drawing}</w:body>"));
    assert_ne!(doc_new, doc_xml, "base document.xml 必須含 </w:body>");
    let rels_new = rels_xml.replace(
        "</Relationships>",
        "<Relationship Id=\"rIdEmf\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/image1.emf\"/></Relationships>",
    );
    assert_ne!(rels_new, rels_xml, "base rels 必須含 </Relationships>");

    let mut changes = std::collections::BTreeMap::new();
    changes.insert(
        "word/document.xml".to_string(),
        quire_ooxml::Change::Replace(doc_new.into_bytes()),
    );
    changes.insert(
        "word/_rels/document.xml.rels".to_string(),
        quire_ooxml::Change::Replace(rels_new.into_bytes()),
    );
    let mut extras = std::collections::BTreeMap::new();
    extras.insert("word/media/image1.emf".to_string(), emf);
    bundle.serialize(&changes, &extras).expect("serialize")
}

// ───────────────────────── 測試用 PNG／base64 工具 ─────────────────────────

const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn decode_b64(s: &str) -> Vec<u8> {
    let mut acc = 0u32;
    let mut bits = 0u32;
    let mut out = Vec::new();
    for &c in s.as_bytes() {
        if c == b'=' || c == b'\n' || c == b'\r' {
            continue;
        }
        let v = B64
            .iter()
            .position(|&b| b == c)
            .unwrap_or_else(|| panic!("非法 base64 字元 {c}")) as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

/// 解非交錯 8bit RGB/RGBA PNG 的 IDAT ⇒ RGBA 平鋪（無外部依賴；dev-deps 的
/// flate2 在此使用，與 `k19_duotone.rs` 同先例）。
fn png_rgba(png: Vec<u8>) -> Vec<u8> {
    assert_eq!(&png[0..8], b"\x89PNG\r\n\x1a\n", "PNG magic");
    let mut pos = 8usize;
    let (mut w, mut h, mut channels, mut color_type) = (0u32, 0u32, 0usize, 0u8);
    let mut plte: Vec<u8> = Vec::new();
    let mut idat = Vec::new();
    while pos + 8 <= png.len() {
        let len = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
        let typ = &png[pos + 4..pos + 8];
        let body = &png[pos + 8..pos + 8 + len];
        match typ {
            b"IHDR" => {
                w = u32::from_be_bytes(body[0..4].try_into().unwrap());
                h = u32::from_be_bytes(body[4..8].try_into().unwrap());
                assert_eq!(body[8], 8, "非 8bit：本測試不處理");
                color_type = body[9];
                channels = match color_type {
                    0 => 1,
                    2 => 3,
                    3 => 1, // 索引色（image3.png 即此式）：取色時查 PLTE
                    6 => 4,
                    ct => panic!("不支援的色彩型 {ct}"),
                };
            }
            b"PLTE" => plte.extend_from_slice(body),
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        pos += 12 + len;
    }
    let raw = {
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::ZlibDecoder::new(&idat[..]), &mut buf)
            .expect("IDAT inflate");
        buf
    };
    let stride = w as usize * channels;
    let mut out = vec![0u8; h as usize * w as usize * 4];
    let mut prev = vec![0u8; stride];
    let mut rp = 0usize;
    for y in 0..h as usize {
        let ft = raw[rp];
        rp += 1;
        let mut line = raw[rp..rp + stride].to_vec();
        rp += stride;
        match ft {
            0 => {}
            1 => {
                for i in channels..stride {
                    line[i] = line[i].wrapping_add(line[i - channels]);
                }
            }
            2 => {
                for i in 0..stride {
                    line[i] = line[i].wrapping_add(prev[i]);
                }
            }
            3 => {
                for i in 0..stride {
                    let a = if i >= channels { line[i - channels] } else { 0 };
                    line[i] = line[i].wrapping_add(((a as usize + prev[i] as usize) / 2) as u8);
                }
            }
            4 => {
                for i in 0..stride {
                    let a: i32 = if i >= channels {
                        line[i - channels] as i32
                    } else {
                        0
                    };
                    let b: i32 = prev[i] as i32;
                    let c: i32 = if i >= channels {
                        prev[i - channels] as i32
                    } else {
                        0
                    };
                    let p = a + b - c;
                    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
                    let pr = if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    };
                    line[i] = line[i].wrapping_add(pr as u8);
                }
            }
            f => panic!("不支援的 PNG filter {f}"),
        }
        for x in 0..w as usize {
            let s = x * channels;
            let d = y * w as usize * 4 + x * 4;
            match color_type {
                3 => {
                    let idx = line[s] as usize;
                    out[d] = plte[idx * 3];
                    out[d + 1] = plte[idx * 3 + 1];
                    out[d + 2] = plte[idx * 3 + 2];
                }
                2 => out[d..d + 3].copy_from_slice(&line[s..s + 3]),
                6 => out[d..d + 4].copy_from_slice(&line[s..s + 4]),
                _ => {
                    out[d] = line[s];
                    out[d + 1] = line[s];
                    out[d + 2] = line[s];
                }
            }
            out[d + 3] = 255;
        }
        prev = line;
    }
    out
}
