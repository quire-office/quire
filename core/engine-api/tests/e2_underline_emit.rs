//! E-2 紅燈考卷（發射層）：底線形態從級聯一路畫到渲染產物。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 卡面缺口（任務 3：發射多態）
//! 修前發射鏈只有布林旗標：`RunStyle.underline: Option<bool>` → engine 烘焙
//! `Run::underline: bool` → `DrawItem::Text.flags` 的 `FLAG_UNDERLINE` 位元 →
//! SVG 端 `text-decoration="underline"`（恆單線）。`double/thick/dotted/dashed`
//! 在解析層就被折損，發射端無形態可繪。
//!
//! # 斷言層口徑（卡面鐵律；K29e 同規）
//! 主斷言打渲染端 SVG 產物（`shells/native/src/render_svg.rs` 的 `page_to_svg`，
//! 與 `quire-cli render` 同一支發射器）。渲染端採「幾何底線」配方——本渲染器
//! 是骨架級幾何直譯（座標全來自 layout），底線用 `<path>` 畫在對應 `<text>`
//! 行的緊接行（段級關聯＝相鄰行）：
//! - `double`   → 兩條平行 `<path>`（雙線；SVG 1.1 text-decoration 對重複值
//!   渲染未定義，瀏覽器只畫一條，宣告式多值不可依賴）；
//! - `thick`    → 單條 `<path>`，`stroke-width` ≈ 字級×0.13（加粗）；
//! - `dotted`   → 單條 `<path>` 帶 `stroke-dasharray` 點級段長（<3pt）；
//! - `dashed`   → 單條 `<path>` 帶 `stroke-dasharray` 段級段長（≥3pt）；
//! - `single`   → 舊路徑 `text-decoration="underline"`（零位移鐵律：未宣告
//!   與單線段輸出與修前逐位元組相同，`<path>` 一條不發）；
//! - 未支援 `Raw("wavy")` → 降級走 `text-decoration="underline"` 單線舊路徑
//!   ＋不 panic（考卷鎖「渲染不炸＋照畫單線」，TODO 在發射端源碼）。
//!   輔助證據＝顯示列表 `DrawItem::Text.underline` 欄（形態透傳，JSON 零位移
//!   守門：未宣告/未見 w:u 的檔案該欄 None 不外露）。
//!
//! # 紅燈口徑（Phase A 拍紅形態折損；Phase B 全綠）
//! 修前 `DrawItem::Text` 無 `underline` 欄、SVG 對 double/thick/dotted/dashed
//! 一律 `text-decoration="underline"`——形態斷言全紅。

use quire_engine::{
    DocPos, DocRange, DrawItem, Engine, Mutation, RunDirectPatch, SubDocTarget, Underline,
    UnderlineKind, FLAG_UNDERLINE,
};

#[path = "../../../shells/native/src/render_svg.rs"]
mod render_svg;

/// 手寫 CRC32（zip 表式多項式；測試間零共用鐵律，自 oxml s1_zip_bomb 同款
/// 語義抄錄。位元式實作免查表靜態、不引 crc32fast 免動 dev-deps）。
fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    c ^ 0xFFFF_FFFF
}

fn settle(engine: &mut Engine) {
    while !engine.is_settled() {
        engine.reflow_step_json();
    }
}

/// 合成 docx（ZipBuilder＝e2_underline_kind.rs 同款，測試間零共用鐵律）。
mod zipkit {
    use crate::crc32;

    const SIG_LOCAL: u32 = 0x0403_4b50;
    const SIG_CENTRAL: u32 = 0x0201_4b50;
    const SIG_EOCD: u32 = 0x0605_4b50;

    pub struct ZipBuilder {
        buf: Vec<u8>,
        centrals: Vec<Vec<u8>>,
        count: usize,
    }

    impl ZipBuilder {
        pub fn new() -> Self {
            Self {
                buf: Vec::new(),
                centrals: Vec::new(),
                count: 0,
            }
        }

        pub fn add_stored(&mut self, name: &[u8], data: &[u8]) {
            let offset = self.buf.len() as u64;
            let csize = data.len() as u64;
            let crc = crc32(data);

            self.buf.extend_from_slice(&SIG_LOCAL.to_le_bytes());
            self.buf.extend_from_slice(&20u16.to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes()); // stored
            self.buf.extend_from_slice(&0x1234u16.to_le_bytes());
            self.buf.extend_from_slice(&0x5678u16.to_le_bytes());
            self.buf.extend_from_slice(&crc.to_le_bytes());
            self.buf.extend_from_slice(&(csize as u32).to_le_bytes());
            self.buf.extend_from_slice(&(csize as u32).to_le_bytes());
            self.buf
                .extend_from_slice(&(name.len() as u16).to_le_bytes());
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            self.buf.extend_from_slice(name);
            self.buf.extend_from_slice(data);

            let mut c = Vec::new();
            c.extend_from_slice(&SIG_CENTRAL.to_le_bytes());
            c.extend_from_slice(&0x031Eu16.to_le_bytes());
            c.extend_from_slice(&20u16.to_le_bytes());
            c.extend_from_slice(&0u16.to_le_bytes());
            c.extend_from_slice(&0u16.to_le_bytes());
            c.extend_from_slice(&0x1234u16.to_le_bytes());
            c.extend_from_slice(&0x5678u16.to_le_bytes());
            c.extend_from_slice(&crc.to_le_bytes());
            c.extend_from_slice(&(csize as u32).to_le_bytes());
            c.extend_from_slice(&(csize as u32).to_le_bytes());
            c.extend_from_slice(&(name.len() as u16).to_le_bytes());
            c.extend_from_slice(&0u16.to_le_bytes());
            c.extend_from_slice(&0u16.to_le_bytes());
            c.extend_from_slice(&0u16.to_le_bytes());
            c.extend_from_slice(&0u16.to_le_bytes());
            c.extend_from_slice(&0u32.to_le_bytes());
            c.extend_from_slice(&(offset as u32).to_le_bytes());
            c.extend_from_slice(name);
            self.centrals.push(c);
            self.count += 1;
        }

        pub fn finish(&self) -> Vec<u8> {
            let mut out = self.buf.clone();
            let cd_off = out.len() as u64;
            for c in &self.centrals {
                out.extend_from_slice(c);
            }
            let cd_size = out.len() as u64 - cd_off;

            out.extend_from_slice(&SIG_EOCD.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&(self.count as u16).to_le_bytes());
            out.extend_from_slice(&(self.count as u16).to_le_bytes());
            out.extend_from_slice(&(cd_size as u32).to_le_bytes());
            out.extend_from_slice(&(cd_off as u32).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out
        }
    }
}

use zipkit::ZipBuilder;

/// 四段式合成檔：單線／雙線／點線／虛線／粗線／未支援／無底線 各一段，
/// 段文字自帶形態標籤方便 SVG/顯示列表反查。
fn fixture_bytes() -> Vec<u8> {
    let mut b = ZipBuilder::new();
    b.add_stored(
        b"[Content_Types].xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#,
    );
    b.add_stored(
        b"_rels/.rels",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
    );
    let para = |u_tag: &str, text: &str| -> String {
        format!("<w:p><w:r><w:rPr>{u_tag}</w:rPr><w:t>{text}</w:t></w:r></w:p>")
    };
    let doc = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    {}
    {}
    {}
    {}
    {}
    {}
  </w:body>
</w:document>"#,
        para(r#"<w:u w:val="single"/>"#, "單線段ABCDE"),
        para(r#"<w:u w:val="double"/>"#, "雙線段ABCDE"),
        para(r#"<w:u w:val="dotted"/>"#, "點線段ABCDE"),
        para(r#"<w:u w:val="dashed"/>"#, "虛線段ABCDE"),
        para(r#"<w:u w:val="thick"/>"#, "粗線段ABCDE"),
        para(r#"<w:u w:val="wavy"/>"#, "未支援段ABCDE"),
    );
    b.add_stored(b"word/document.xml", doc.as_bytes());
    b.finish()
}

fn fixture_engine() -> Engine {
    let mut engine = Engine::from_docx_bytes(&fixture_bytes()).expect("開啟合成檔失敗");
    settle(&mut engine);
    engine
}

fn svg_of(engine: &Engine, page: usize) -> String {
    let dl = engine
        .page_display_list(page)
        .expect("第 0 頁顯示列表必須存在");
    render_svg::page_to_svg(&dl)
}

/// 渲染端段級關聯＝底線 `<path>` 緊接其 `<text>` 行（相鄰行）。回傳
/// 「含 needle 的 `<text>` 行＋其後連續的 `<path>` 行」整段（段級斷言用）。
fn svg_block(svg: &str, needle: &str) -> String {
    let v: Vec<&str> = svg.lines().collect();
    for i in 0..v.len() {
        if v[i].contains("<text ") && v[i].contains(needle) {
            let mut block = String::from(v[i]);
            let mut j = i + 1;
            while j < v.len() && v[j].starts_with("<path") {
                block.push('\n');
                block.push_str(v[j]);
                j += 1;
            }
            return block;
        }
    }
    panic!(
        "SVG 必須含文字段 {needle:?}；片段：{}",
        &svg[..svg.len().min(600)]
    );
}

/// 顯示列表裡第一條含 `needle` 的 Text 項 (flags, underline 欄)。
fn text_item_with(engine: &Engine, needle: &str) -> (u8, Option<Underline>) {
    let dl = engine
        .page_display_list(0)
        .expect("第 0 頁顯示列表必須存在");
    dl.items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text {
                flags,
                underline,
                text,
                ..
            } if text.contains(needle) => Some((*flags, underline.clone())),
            _ => None,
        })
        .unwrap_or_else(|| panic!("顯示列表必須含文字片段 {needle:?}"))
}

/// 燈 1：形態進顯示列表欄——double/dotted/dashed/thick 各自保形，
/// 未支援 wavy 以 Raw 保形；全部帶 `FLAG_UNDERLINE` 旗標（舊消費者口徑）。
#[test]
fn e2e_lamp1_kinds_reach_display_list() {
    let e = fixture_engine();
    let cases: [(&str, Underline); 5] = [
        ("雙線段", Underline::Double),
        ("點線段", Underline::Dotted),
        ("虛線段", Underline::Dashed),
        ("粗線段", Underline::Thick),
        ("未支援段", Underline::Raw("wavy".to_string())),
    ];
    for (needle, want) in cases {
        let (flags, ul) = text_item_with(&e, needle);
        assert_ne!(flags & FLAG_UNDERLINE, 0, "{needle} 必須帶 FLAG_UNDERLINE");
        assert_eq!(ul, Some(want), "{needle} 的形態欄必須保形透傳到顯示列表");
    }
    let (flags, ul) = text_item_with(&e, "單線段");
    assert_ne!(flags & FLAG_UNDERLINE, 0);
    assert_eq!(ul, Some(Underline::Single), "單線段形態欄＝Single");
}

/// 取 block 內 `<path>` 行（底線幾何行）。
fn path_lines(block: &str) -> Vec<String> {
    block
        .lines()
        .filter(|l| l.starts_with("<path"))
        .map(String::from)
        .collect()
}

/// 解析 `stroke-dasharray="a b"` 的首段長（pt）。
fn first_dash(block: &str) -> f32 {
    let i = block
        .find("stroke-dasharray=\"")
        .expect("必須帶 stroke-dasharray");
    let rest = &block[i + "stroke-dasharray=\"".len()..];
    let head = rest.split([' ', '"']).next().expect("dash 首段");
    head.parse::<f32>()
        .unwrap_or_else(|_| panic!("dash 首段非數值：{head}"))
}

/// 燈 2：SVG 渲染端消費形態——double 落兩條平行 `<path>`、dotted/dashed
/// 帶 stroke-dasharray（點級 vs 段級可分）、thick 帶加粗 stroke-width、
/// single 零 `<path>`（走 text-decoration 舊路徑）。修前四段全發
/// `text-decoration="underline"` 同形→紅。
#[test]
fn e2e_lamp2_svg_renders_kinds() {
    let e = fixture_engine();
    let svg = svg_of(&e, 0);

    // double：兩條平行 path（宣告式 text-decoration 重複值瀏覽器只畫一條，
    // 不可依賴——這就是渲染端改幾何的原因，考卷鎖幾何實繪）。
    let dbl = svg_block(&svg, "雙線段");
    let dbl_paths = path_lines(&dbl);
    assert_eq!(
        dbl_paths.len(),
        2,
        "double 必須發兩條平行 path（實得 block：{dbl}）"
    );
    assert_ne!(
        dbl_paths[0], dbl_paths[1],
        "兩條 path 的 y 必須不同（平行雙線；實得 {dbl_paths:?}）"
    );

    // single：零 path＋text-decoration 舊路徑（零位移鐵律）。
    let sgl = svg_block(&svg, "單線段");
    assert!(
        sgl.contains("text-decoration=\"underline\""),
        "single 必須走 text-decoration 舊路徑（實得 {sgl}）"
    );
    assert!(
        path_lines(&sgl).is_empty(),
        "single 不得發幾何 path（修前行為逐位元組保持；實得 {sgl}）"
    );

    // dotted vs dashed：同有 dasharray、尺級可分（點級 <3pt、段級 ≥3pt）。
    let dot = svg_block(&svg, "點線段");
    let dot_paths = path_lines(&dot);
    assert_eq!(dot_paths.len(), 1, "dotted 一條底線 path（實得 {dot}）");
    assert!(
        dot.contains("stroke-dasharray"),
        "dotted 必須帶 stroke-dasharray（實得 {dot}）"
    );
    let dash = svg_block(&svg, "虛線段");
    let dash_paths = path_lines(&dash);
    assert_eq!(dash_paths.len(), 1, "dashed 一條底線 path（實得 {dash}）");
    assert!(
        dash.contains("stroke-dasharray"),
        "dashed 必須帶 stroke-dasharray（實得 {dash}）"
    );
    let dot_len = first_dash(&dot);
    let dash_len = first_dash(&dash);
    assert!(dot_len < 3.0, "dotted 段長應點級（<3pt；實得 {dot_len}）");
    assert!(
        dash_len >= 3.0,
        "dashed 段長應段級（≥3pt；實得 {dash_len}）"
    );
    assert!(
        dash_len > dot_len,
        "dashed 段長必須比 dotted 長（實得 dot={dot_len} dash={dash_len}）"
    );

    // thick：單條加粗 path（stroke-width 顯著粗於單線字級倍率）。
    let thick = svg_block(&svg, "粗線段");
    let thick_paths = path_lines(&thick);
    assert_eq!(thick_paths.len(), 1, "thick 一條底線 path（實得 {thick}）");
    assert!(
        thick_paths[0].contains("stroke-width"),
        "thick 必須帶 stroke-width（實得 {thick_paths:?}）"
    );
}

/// 燈 3：未支援形態降級單線＋不 panic——wavy 段照常發射文字、渲染端落
/// 單線 text-decoration（考卷鎖降級行為，TODO 在發射端源碼）。
#[test]
fn e2e_lamp3_unsupported_degrades_to_single_line() {
    let e = fixture_engine();
    let (flags, ul) = text_item_with(&e, "未支援段");
    assert_ne!(flags & FLAG_UNDERLINE, 0, "Raw 形態照舊點亮旗標");
    assert!(
        matches!(ul, Some(Underline::Raw(_))),
        "Raw 原字面必須透傳（渲染端據此降級）"
    );
    let svg = svg_of(&e, 0);
    let block = svg_block(&svg, "未支援段");
    assert!(
        block.contains("text-decoration=\"underline\""),
        "wavy 必須降級成單線 text-decoration（實得 {block}）"
    );
    assert!(
        !block.contains("dash") && path_lines(&block).is_empty(),
        "降級單線不得帶 dash 殘影或幾何 path（實得 {block}）"
    );
}

/// 燈 4：JSON 零位移守門——無 `w:u` 宣告的 run，`underline` 欄 None 且
/// **不得**出現在序列化輸出（K29e `family` 欄同款 serde 紀律；防全 corpus
/// 顯示列表 JSON 膨脹）。
#[test]
fn e2e_lamp4_json_zero_displacement_when_undeclared() {
    let mut b = ZipBuilder::new();
    b.add_stored(
        b"[Content_Types].xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#,
    );
    b.add_stored(
        b"_rels/.rels",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
    );
    b.add_stored(
        b"word/document.xml",
        // br#""# 不收非 ASCII，含中文字段改 r#""#＋as_bytes（語義同）。
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body><w:p><w:r><w:t>純文字段ABCDE</w:t></w:r></w:p></w:body>
</w:document>"#
            .as_bytes(),
    );
    let mut engine = Engine::from_docx_bytes(&b.finish()).expect("開啟失敗");
    settle(&mut engine);
    let dl = engine.page_display_list(0).expect("第 0 頁必須存在");
    let item = dl
        .items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text {
                text, underline, ..
            } if text.contains("純文字段") => Some(underline.clone()),
            _ => None,
        })
        .expect("顯示列表必須含該段");
    assert_eq!(item, None, "無 w:u 宣告＝級聯未宣告＝None");
    let json = serde_json::to_string(&dl.items).expect("序列化");
    assert!(
        !json.contains("underline"),
        "None 欄不得外露進 JSON（零位移鐵律）：{json}"
    );
}

/// 燈 6（卡面任務 4：圓環）：寫入端 `UnderlineKind`（mutation.rs K29e，本卡
/// 白名單外）與解析端 `Underline`（E-2）的**口徑閉環**——同一 ECMA `w:u/@w:val`
/// 字面，經「寫入→存檔→重解析→發射」必須落回同形態，且讀取端改動不改寫位元組。
///
/// 共型取捨（E-2 老實記帳 → **E-2b 已落地**）：E-2 時 `UnderlineKind` 是
/// mutation.rs 自造的鏡像枚舉（白名單禁碰＋pub 線契約），本燈以**語義等價閉環**
/// 鎖死兩制。E-2b（t_5e62f3b5）把 `UnderlineKind` 改為 `Underline` 的別名——
/// 三端同型後本燈仍是回歸基準（逐值寫讀閉環不受共型影響，卡面任務 3 的「共型後
/// 燈 6 應仍綠」由本燈自證）；編譯期同型防分岔改由 `e2b_underline_shared_type`
/// 燈 1 接手，serde 線契約遷移由該卷燈 4／5／7 鎖死。
#[test]
fn e2e_lamp6_write_read_roundtrip_kind_parity() {
    // (a) 寫入端四形態 + 明確關閉：逐值 → 存檔件字面 → 重解析形態 → 發射。
    // E-2b 共型記帳：本燈的 `UnderlineKind` 已是 `Underline` 別名（同一型別），
    // 舊 `None` 變體換名 `Off`（語義不變）；(b) 段的零位元組位移斷言原樣保留。
    for (kind, want_val, want_kind) in [
        (UnderlineKind::Single, "single", Some(Underline::Single)),
        (UnderlineKind::Double, "double", Some(Underline::Double)),
        (UnderlineKind::Dotted, "dotted", Some(Underline::Dotted)),
        (UnderlineKind::Dashed, "dashed", Some(Underline::Dashed)),
        (UnderlineKind::Off, "none", None),
    ] {
        let mut engine = fixture_engine();
        engine
            .apply_mutation(Mutation::FormatTextRange {
                range: DocRange {
                    target: SubDocTarget::Body,
                    start: DocPos { para: 0, cp: 0 },
                    end: DocPos { para: 0, cp: 5 },
                },
                patch: RunDirectPatch {
                    // E-2b 共型：`Underline` 非 Copy（Raw 帶字串），逐值餵入改 clone。
                    underline: Some(kind.clone()),
                    ..Default::default()
                },
            })
            .expect("套用 FormatTextRange（底線形態）失敗");
        settle(&mut engine);

        let saved = engine.save_docx().expect("存檔失敗");
        let reopened = Engine::from_docx_bytes(&saved).expect("重開失敗");
        let xml = String::from_utf8(
            reopened
                .part("word/document.xml")
                .expect("重開件必須有 document.xml")
                .to_vec(),
        )
        .expect("UTF-8");
        let tag = format!(r#"<w:u w:val="{want_val}"/>"#);
        assert!(
            xml.contains(&tag),
            "寫入端 {kind:?} 必須寫出 {tag}；document.xml 前 600 字元：{}",
            &xml[..xml.len().min(600)]
        );

        // 重解析讀回的形態必須與寫入端同義（單線段被覆寫後的目標段）。
        let (flags, ul) = text_item_with(&reopened, "單線段");
        if want_kind.is_some() {
            assert_ne!(flags & FLAG_UNDERLINE, 0, "{kind:?} 必須點亮旗標");
        } else {
            assert_eq!(flags & FLAG_UNDERLINE, 0, "none 必須熄滅旗標");
        }
        assert_eq!(
            ul, want_kind,
            "{kind:?} 經存檔重解析必須讀回同形態（口徑閉環，禁折損）"
        );
    }

    // (b) 讀取端改動不改寫位元組：未動過的合成件，存檔件的 word/document.xml
    // 必須與來源逐位元組相同（corpus roundtrip --check 的逐檔口徑在本合成件
    // 同款成立——形態欄是純派生，不得回寫 XML）。
    let bytes = fixture_bytes();
    let origin = Engine::from_docx_bytes(&bytes).expect("開啟來源件失敗");
    let want = origin
        .part("word/document.xml")
        .expect("來源件必須有 document.xml")
        .to_vec();
    let mut engine = Engine::from_docx_bytes(&bytes).expect("開啟失敗");
    settle(&mut engine);
    let saved = engine.save_docx().expect("存檔失敗");
    let reopened = Engine::from_docx_bytes(&saved).expect("重開失敗");
    let got = reopened
        .part("word/document.xml")
        .expect("存檔件必須有 document.xml")
        .to_vec();
    assert_eq!(
        got, want,
        "讀取端形態改動後，未編輯件的 document.xml 必須逐位元組保持（零位元組位移）"
    );
}

/// 燈 5：明確關閉熄滅渲染——`w:u val="none"` 段：旗標熄滅、SVG 無任何
/// underline 裝飾（PM 補正 2：off 必須壓過上層，發射端不畫）。
#[test]
fn e2e_lamp5_off_extinguishes_render() {
    let mut b = ZipBuilder::new();
    b.add_stored(
        b"[Content_Types].xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#,
    );
    b.add_stored(
        b"_rels/.rels",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
    );
    b.add_stored(
        b"word/document.xml",
        // br#""# 不收非 ASCII，含中文字段改 r#""#＋as_bytes（語義同）。
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:rPr><w:u w:val="none"/></w:rPr><w:t>熄滅段ABCDE</w:t></w:r></w:p>
  </w:body>
</w:document>"#
            .as_bytes(),
    );
    b.add_stored(
        b"word/styles.xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal">
    <w:rPr><w:u w:val="double"/></w:rPr>
  </w:style>
</w:styles>"#,
    );
    let mut engine = Engine::from_docx_bytes(&b.finish()).expect("開啟失敗");
    settle(&mut engine);
    let (flags, _ul) = text_item_with(&engine, "熄滅段");
    assert_eq!(
        flags & FLAG_UNDERLINE,
        0,
        "直接 off 贏過樣式層 double：旗標必須熄滅"
    );
    let svg = svg_of(&engine, 0);
    let block = svg_block(&svg, "熄滅段");
    assert!(
        !block.contains("underline") && path_lines(&block).is_empty(),
        "熄滅段 SVG 不得帶任何底線裝飾或幾何 path（實得 {block}）"
    );
}
