//! E1-C 卡 D：`w:t`／`w:delText` 的 `xml:space` 節點級口徑考卷（先紅後綠）。
//!
//! # 為什麼有這張考卷
//!
//! 舊實作把每個 `Event::Text` 分片直接入文。quick-xml 會在實體邊界把同一個文字節點
//! 切成多個 Text 分片，任何「逐分片裁剪」的口徑都會把**跨分片的頭尾空白**誤判成節點內
//! 空白而丟字（例：`A  &amp;  B  ` 無 preserve 時，逐片裁會退化成 `A&B`，正文消失的
//! 空白直接虛減量寬）。本卡定案口徑（ECMA-376 §17.3.2.31＋LO rawdict 實測，逐條見
//! `docs/e1cd-xml-space-policy.md`）：
//! 1. 文字節點整段緩衝、End 時一次裁剪：未帶 `xml:space="preserve"` 時剝除**節點頭尾**
//!    XML 空白（空格／TAB／CR／LF），節點內連續空白一律保留；
//! 2. `xml:space="preserve"` 時節點頭尾空白原樣保留；
//! 3. **LO 不折疊節點內連續空白**——受控 fixture（Mono 10pt）rawdict 實測 sp1／sp2／sp3
//!    gap＝6／12／18pt，雙空格＝兩個空格字形。卡面舊歸因「LO 折疊雙空格」係
//!    `get_text("words")` 抽詞把空白折成單字的抽取器假象，已被推翻並記帳。
//!
//! # 鑑別力（buggy 回塞）
//!
//! 燈 3（跨分片守恆）在「逐分片 trim」的舊口徑下必紅（`A  &  B` 變 `A&B`）；
//! 燈 2（無 preserve 頭尾剝除）在「一律 preserve」的口徑下必紅。兩燈方向相反，
//! 互相封死「全保留」與「逐片裁」兩個退化實作，證明斷言真的在比口徑、不是恆真。
//!
//! # 本檔**不**斷言的事（老實記帳）
//!
//! - 不斷言框內（`w:txbxContent`）文字的 xml:space 路徑——其口徑由實作與正文共用
//!   同一裁剪函數，但本檔的 docx 構造不含圖形文字框；該路徑由 workspace 級渲染考卷
//!   （d9 家族）間接守衛。
//! - 不斷言版面層斷行推進（cell-grid-span 6/9 殘差屬 E1-C 卡 E，根因另查）。

use quire_ooxml::parse;

const SIG_LOCAL: u32 = 0x0403_4b50;
const SIG_CENTRAL: u32 = 0x0201_4b50;
const SIG_EOCD: u32 = 0x0605_4b50;

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
        self.buf.extend_from_slice(&0u16.to_le_bytes());
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

/// 以給定段落 XML 片段構造最小 DOCX 並解析，回傳各段全文串接（依段落序）。
fn doc_para_texts(body: &str) -> Vec<String> {
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>{body}</w:body>
</w:document>"#
    );
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
    b.add_stored(b"word/_rels/document.xml.rels", b"<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"/>");
    b.add_stored(b"word/document.xml", document.as_bytes());
    let docx = b.finish();
    let doc = parse(&docx).expect("最小 DOCX 應解析成功");
    doc.paragraphs
        .iter()
        .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
        .collect()
}

/// 燈 1：preserve 節點——頭尾空白與**節點內連續空白**全部原樣保留。
/// 封死「折疊雙空格」退化口徑（LO rawdict 實測不折疊：sp2 gap＝12pt＝兩個空格字形）。
#[test]
fn preserve_keeps_interior_and_edge_spaces_verbatim() {
    let texts = doc_para_texts(
        r#"<w:p><w:r><w:t xml:space="preserve">  Team  Passion   done  </w:t></w:r></w:p>"#,
    );
    assert_eq!(texts.len(), 1);
    assert_eq!(texts[0], "  Team  Passion   done  ");
}

/// 燈 2：無 preserve——節點**頭尾** XML 空白剝除（ECMA-376 §17.3.2.31），
/// 節點內連續空白**不得**順帶折疊或丟失。封死「全保留」與「連內文一起裁」兩個退化口徑。
#[test]
fn no_preserve_strips_only_node_edge_whitespace() {
    let texts =
        doc_para_texts(r#"<w:p><w:r><w:t>   Tab1&#x9;Tab2   mid  dbl   </w:t></w:r></w:p>"#);
    assert_eq!(texts.len(), 1);
    // 頭尾空白剝除；TAB 與節點內雙空格保留（TAB 只限頭尾剝除範圍內才動）。
    assert_eq!(texts[0], "Tab1\u{9}Tab2   mid  dbl");
}

/// 燈 3：跨分片守恆——quick-xml 在實體邊界把同一節點切成多分片時，裁剪必須以
/// **整節點**為單位。無 preserve 的「  A  &amp;  B  」：舊逐片 trim 口徑會退化成
/// 「A&B」（吞掉跨片空白，燈必紅＝先紅後綠的 buggy 回塞點）；本口徑得「A  &  B」。
#[test]
fn entity_split_node_trims_whole_node_not_each_chunk() {
    let texts = doc_para_texts(r#"<w:p><w:r><w:t>  A  &amp;  B  </w:t></w:r></w:p>"#);
    assert_eq!(texts.len(), 1);
    assert_eq!(texts[0], "A  &  B");

    // preserve 態同節點：跨片頭尾空白一律保留，證明燈 2 的剝除只由「無 preserve」觸發。
    let texts_p =
        doc_para_texts(r#"<w:p><w:r><w:t xml:space="preserve">  A  &amp;  B  </w:t></w:r></w:p>"#);
    assert_eq!(texts_p.len(), 1);
    assert_eq!(texts_p[0], "  A  &  B  ");
}

/// 燈 4：`w:delText`（修訂刪除文字）與正文 `w:t` 共用同一節點口徑——
/// 無 preserve 頭尾剝除、節點內空白保留；且刪除文字仍入文（刪除線渲染由版面層決定，
/// 解析層不得因空白口徑吞字）。
#[test]
fn deltext_shares_node_whitespace_policy() {
    let texts = doc_para_texts(
        r#"<w:p><w:del w:id="1" w:author="pm" w:date="2026-11-09T00:00:00Z"><w:r><w:delText>   old  text   </w:delText></w:r></w:del></w:p>"#,
    );
    assert_eq!(texts.len(), 1);
    assert_eq!(texts[0], "old  text");
}
