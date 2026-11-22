//! E-2 紅燈考卷（解析層）：`w:u/@w:val` 保形態——底線形態枚舉進級聯。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 卡面缺口（U4b t_b9ae819b §4/§8 點名、PM 讀碼坐實）
//! `core/ooxml/src/styles.rs` 的 `RunStyle::underline` 為 `Option<bool>`——
//! 布林只分有無，`w:val="double"/"thick"/"dotted"/"dashed"` 等形態全被折成
//! `Some(true)`，發射端只能畫單線。本考卷先紅拍「形態被折損」這一缺陷，
//! 枚舉化落地後轉綠。
//!
//! # 先紅口徑（Phase A 只用既有可編譯 API 拍紅）
//! 斷言打在 `RunStyle` 的 serde JSON 形（級聯三態快照的機讀形態，
//! `Paragraph::prun`／`Run::direct` 同一類型）：修前 `underline` 欄是布林
//! `true`，無法區分 double/thick/dotted/dashed——斷言其為形態字面值即紅。
//! Phase B（枚舉落地後）補型別化斷言（`Underline::Style(UnderlineStyle::*)`）
//! 並保留本 JSON 口徑作為線契約（JSON 形逐字鎖死，防 serde 表示漂移）。
//!
//! # 三態語義（向 bold/italic 三態靠攏，PM 補正 2）
//! - `null`＝未宣告（繼承上層）；
//! - `"off"`＝明確關閉（`w:val="none"/"false"/"0"/"off"`），必須能熄滅上層宣告；
//! - 形態字面值（`"single"/"double"/"thick"/"dotted"/"dashed"`）＝開啟＋形態；
//! - 未支援形態（ECMA ST_UnderlineValues 其餘值，如 `wavy`/`words`）以
//!   `{"raw":"<原字面>"}` 保形保留（發射端降級單線＋TODO，禁 panic）。
//!
//! # corpus 實掃（卡面任務 1，口徑：unzip 全部 word/*.xml 後 grep 'w:u ' 帶
//! 'w:val=' 的命中；分母＝corpus 全 400 個 .docx）
//! - 含 `w:u/@w:val` 宣告的檔案：44/400；
//! - 值分佈（全 xml part 計）：single×1286、none×131、thick×5（FDO77715
//!   styles.xml）、dash×5（phpword 四檔 document.xml）、words×1（fdo61343
//!   styles.xml）、double×1（fdo61343 styles.xml）、dotted×1（xopt
//!   SolarOverview document.xml）。
//!   ⇒ 非 single 形態在 corpus **有真實命中**（非零命中），本卡屬實測缺口修復。
//!   `dash` 為 Word 相容讀取端的 `dashed` 別名（mutation.rs K29e 註解同口徑）。

use serde_json::json;

const SIG_LOCAL: u32 = 0x0403_4b50;
const SIG_CENTRAL: u32 = 0x0201_4b50;
const SIG_EOCD: u32 = 0x0605_4b50;

/// 最小 zip 建造器（d6_table_borders.rs 同款，STORED 條目，零依賴）。
struct ZipBuilder {
    buf: Vec<u8>,
    centrals: Vec<Vec<u8>>,
    count: usize,
}

impl ZipBuilder {
    fn new() -> Self {
        Self {
            buf: Vec::new(),
            centrals: Vec::new(),
            count: 0,
        }
    }

    fn add_stored(&mut self, name: &[u8], data: &[u8]) {
        let offset = self.buf.len() as u64;
        let csize = data.len() as u64;
        let mut h = crc32fast::Hasher::new();
        h.update(data);
        let crc = h.finalize();

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

    fn finish(&self) -> Vec<u8> {
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

/// 建造最小 DOCX 位元組（可選 styles.xml）。
fn make_docx(document_xml: &str, styles_xml: Option<&str>) -> Vec<u8> {
    let mut b = ZipBuilder::new();
    let ct = if styles_xml.is_some() {
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#
    } else {
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#
    };
    b.add_stored(b"[Content_Types].xml", ct.as_bytes());
    b.add_stored(
        b"_rels/.rels",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
    );
    b.add_stored(b"word/document.xml", document_xml.as_bytes());
    if let Some(s) = styles_xml {
        b.add_stored(b"word/styles.xml", s.as_bytes());
    }
    b.finish()
}

/// 段落 rPr 模板：`w:u` 標籤由引數注入。
fn para_with_u(u_tag: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r>
        <w:rPr>{u_tag}</w:rPr>
        <w:t>底線形態段</w:t>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#
    )
}

/// 取第 0 段第 0 個 run 的 `direct` 三態快照之 serde JSON 的 `underline` 欄。
fn direct_underline_json(docx: &[u8]) -> serde_json::Value {
    let doc = quire_ooxml::parse(docx).expect("parse docx");
    let runs = &doc.paragraphs[0].runs;
    let direct = runs[0]
        .direct
        .as_ref()
        .expect("rPr 必須材料化 direct 三態快照");
    serde_json::to_value(direct).expect("RunStyle 必須可序列化")["underline"].clone()
}

/// 燈 1（核心紅燈）：`<w:u w:val="double"/>` 的形態必須保留在級聯三態快照，
/// 修前布林 `true` 無法區分 double→紅。
#[test]
fn e2_lamp1_double_val_preserves_kind() {
    let docx = make_docx(&para_with_u(r#"<w:u w:val="double"/>"#), None);
    assert_eq!(
        direct_underline_json(&docx),
        json!("double"),
        r#"<w:u w:val="double"/> 的形態必須保形為 "double"（修前布林折損為 true）"#
    );
}

/// 燈 2：thick／dotted／dashed 三形態逐一保形（ECMA §17.3.2.41；寫入端
/// K29e `UnderlineKind` 四值集的讀取面對稱）。
#[test]
fn e2_lamp2_thick_dotted_dashed_preserve_kind() {
    for (tag, expect) in [
        (r#"<w:u w:val="thick"/>"#, json!("thick")),
        (r#"<w:u w:val="dotted"/>"#, json!("dotted")),
        (r#"<w:u w:val="dashed"/>"#, json!("dashed")),
    ] {
        let docx = make_docx(&para_with_u(tag), None);
        assert_eq!(
            direct_underline_json(&docx),
            expect,
            "{tag} 的形態必須保形（修前布林折損）"
        );
    }
}

/// 燈 3：`single` 與無 `@w:val`（ECMA 預設＝single）都是單線；
/// `dash` 是 Word 相容讀取端的 `dashed` 別名，必須歸一（corpus phpword×5
/// 實見值，歸一後發射端才認得）。
#[test]
fn e2_lamp3_single_default_and_dash_alias() {
    let docx = make_docx(&para_with_u(r#"<w:u w:val="single"/>"#), None);
    assert_eq!(direct_underline_json(&docx), json!("single"));
    let docx = make_docx(&para_with_u("<w:u/>"), None);
    assert_eq!(
        direct_underline_json(&docx),
        json!("single"),
        "無 @w:val＝ECMA 預設 single（toggle 開啟語義不變）"
    );
    let docx = make_docx(&para_with_u(r#"<w:u w:val="dash"/>"#), None);
    assert_eq!(
        direct_underline_json(&docx),
        json!("dashed"),
        "dash 別名必須歸一為 dashed（Word 相容口徑；corpus phpword 四檔實見）"
    );
}

/// 燈 4：明確關閉三態——`none/false/0/off` 全部讀成 `"off"`（非未宣告、
/// 非單線），與 `toggle_attr_on` 語義銜接；級聯兜底不得再補底線。
#[test]
fn e2_lamp4_explicit_off_four_spellings() {
    for tag in [
        r#"<w:u w:val="none"/>"#,
        r#"<w:u w:val="false"/>"#,
        r#"<w:u w:val="0"/>"#,
        r#"<w:u w:val="off"/>"#,
    ] {
        let docx = make_docx(&para_with_u(tag), None);
        assert_eq!(
            direct_underline_json(&docx),
            json!("off"),
            "{tag} 必須讀成明確關閉 off（修前布林 false 與未宣告的區分僅存於 Option 層）"
        );
    }
}

/// 燈 5：未支援形態保形為 `{"raw":"原字面"}`（禁 panic、禁折損成單線態丟形）——
/// `wavy`（ECMA 未支援面）與 `words`（corpus fdo61343 styles.xml 實見值）。
#[test]
fn e2_lamp5_unsupported_kinds_kept_as_raw() {
    let docx = make_docx(&para_with_u(r#"<w:u w:val="wavy"/>"#), None);
    assert_eq!(
        direct_underline_json(&docx),
        json!({"raw": "wavy"}),
        "未支援形態必須以 raw 保形（發射端降級單線＋TODO）"
    );
    let docx = make_docx(&para_with_u(r#"<w:u w:val="words"/>"#), None);
    assert_eq!(
        direct_underline_json(&docx),
        json!({"raw": "words"}),
        "corpus 實見 words（fdo61343）必須 raw 保形"
    );
}

/// 燈 6：未宣告＝null（繼承語義不破）。`<w:b/>` 等其他 rPr 標籤不得把
/// underline 材料化成任何非 null 值。
#[test]
fn e2_lamp6_undeclared_stays_null() {
    let docx = make_docx(&para_with_u("<w:b/>"), None);
    assert_eq!(
        direct_underline_json(&docx),
        serde_json::Value::Null,
        "未宣告 w:u 時 underline 欄必須是 null（未宣告態）"
    );
}

/// 燈 7：級聯熄滅——樣式層宣告 `w:u val="double"`，run 直接格式
/// `w:u val="none"`：有效值必須是明確關閉 off（不得被樣式層的 double 壓回）。
/// 這是 PM 補正 2 的核心：off 能熄滅樣式層宣告，不是二態。
#[test]
fn e2_lamp7_direct_off_extinguishes_style_kind() {
    let styles = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal">
    <w:rPr><w:u w:val="double"/></w:rPr>
  </w:style>
</w:styles>"#;
    let doc_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r>
        <w:rPr><w:u w:val="none"/></w:rPr>
        <w:t>樣式雙線被直接格式熄滅</w:t>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let t = quire_ooxml::StyleTable::parse(styles.as_bytes());
    let (base, _para) = t.effective_para(None, quire_ooxml::ParaStyle::default());
    // 樣式層自身的形態保留（級聯基底）。
    assert_eq!(
        serde_json::to_value(&base).expect("serialize")["underline"],
        json!("double"),
        "樣式層 double 必須保形進級聯基底"
    );
    // 直接格式 off 疊上去必須贏。
    let docx = make_docx(doc_xml, Some(styles));
    let doc = quire_ooxml::parse(&docx).expect("parse");
    let direct = doc.paragraphs[0].runs[0]
        .direct
        .as_ref()
        .expect("direct 必須材料化");
    let eff = quire_ooxml::StyleTable::resolve_run(base, direct.clone());
    assert_eq!(
        serde_json::to_value(&eff).expect("serialize")["underline"],
        json!("off"),
        "直接格式 off 必須熄滅樣式層的 double（三態語義，非二態有無）"
    );
}

/// 燈 8：run 級 bool 投影向後相容——`Run::underline`（有損 bool 投影）在
/// 任何非 off 形態下仍為 true、off/未宣告為 false（既有消費者零動搖）。
#[test]
fn e2_lamp8_bool_projection_still_gates_on_off() {
    for (tag, want_bool) in [
        (r#"<w:u w:val="double"/>"#, true),
        (r#"<w:u w:val="thick"/>"#, true),
        (r#"<w:u/>"#, true),
        (r#"<w:u w:val="none"/>"#, false),
        (r#"<w:u w:val="wavy"/>"#, true), // 未支援形態＝有底線（發射端降級單線）。
    ] {
        let docx = make_docx(&para_with_u(tag), None);
        let doc = quire_ooxml::parse(&docx).expect("parse");
        assert_eq!(
            doc.paragraphs[0].runs[0].underline, want_bool,
            "{tag} 的 bool 投影必須為 {want_bool}"
        );
    }
}

/// 燈 9（防風暴樁）：舊版 `RunStyle` JSON（`"underline":null`）必須仍可反序列化
/// 並落回未宣告——serde 線契約的向後兼容（與 lib.rs:2540 同款口徑）。
#[test]
fn e2_lamp9_legacy_runstyle_json_still_parses() {
    let old = r#"{"size_pt":12.0,"bold":true,"italic":null,"underline":null,"strike":null,"color":null,"color_set":false}"#;
    let rs: quire_ooxml::RunStyle =
        serde_json::from_str(old).expect("舊 RunStyle JSON 必須可反序列化");
    assert!(rs.underline.is_none(), "舊 JSON underline=null → 未宣告");
    // None 不得外露（序列化零位移）。
    let j = serde_json::to_value(&rs).expect("serialize");
    assert_eq!(j["underline"], serde_json::Value::Null);
}
