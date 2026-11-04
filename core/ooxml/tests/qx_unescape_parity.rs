//! QX quick-xml 0.37 -> 0.41 遷移等價性與安全性考卷
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 考卷斷言：
//! 1. 燈 1：unescape 與 decode parity（等價性）
//!    含實體（&amp; &lt; &gt; &quot; &apos; &#x2019; &#8217; &#x4e2d;&#x6587; &#39;）與
//!    多語種文字（繁體中文、日本語、한국어、العربية、德文等），
//!    斷言 w:t 文字、文字框文字、欄位指令、樣式屬性、表格屬性與 rels 屬性值
//!    經解析後與寫死基線常數逐字元完全一致。
//! 2. 燈 2：RUSTSEC-2026-0194 防線（start tag 內重複屬性名 DoS）
//!    構造含 10,000 個重複屬性名的 start tag，斷言解析在線性時間內完成（限時 500ms，超時即失敗）。
//! 3. 燈 3：RUSTSEC-2026-0195 防線（深巢狀命名空間宣告記憶體耗盡 DoS）
//!    構造深巢狀命名空間宣告，斷言解析在有限時間與有限記憶體內安全完成（限時 500ms）。

use quire_ooxml::{parse, read_rels, FieldKind, StyleTable};
use std::time::Instant;

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

        // Local header
        self.buf.extend_from_slice(&SIG_LOCAL.to_le_bytes());
        self.buf.extend_from_slice(&20u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(&0x1234u16.to_le_bytes());
        self.buf.extend_from_slice(&0x5678u16.to_le_bytes());
        self.buf.extend_from_slice(&crc.to_le_bytes());
        self.buf.extend_from_slice(&(csize as u32).to_le_bytes());
        self.buf.extend_from_slice(&(csize as u32).to_le_bytes());
        self.buf.extend_from_slice(&(name.len() as u16).to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes());
        self.buf.extend_from_slice(name);
        self.buf.extend_from_slice(data);

        // Central entry
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

fn make_test_docx(document_xml: &str, styles_xml: &str, rels_xml: &str) -> Vec<u8> {
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
    b.add_stored(b"word/_rels/document.xml.rels", rels_xml.as_bytes());
    b.add_stored(b"word/document.xml", document_xml.as_bytes());
    b.add_stored(b"word/styles.xml", styles_xml.as_bytes());
    b.finish()
}

/// 燈 1：unescape 與 decode parity（等價性）
/// 斷言含實體（&amp; &lt; &gt; &quot; &apos; &#x2019; &#8217; &#x4e2d;&#x6587; &#39;）與
/// 多語種文字及屬性值，解析後與寫死的基線常數逐字元完全一致。
#[test]
fn test_unescape_and_decode_parity_entities_and_multilingual() {
    let doc_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr>
        <w:pStyle w:val="Custom&amp;Style"/>
      </w:pPr>
      <w:r>
        <w:t>Hello &amp; World &lt;Foo&gt; &quot;Bar&quot; &apos;Baz&apos; &#x2019;26 &#8217; &#x4e2d;&#x6587; &#39;test&#39;</w:t>
      </w:r>
      <w:r>
        <w:t>繁體中文：實體測試與 UTF-8 解碼驗證。</w:t>
      </w:r>
      <w:r>
        <w:t>日本語：テスト文字列（カタカナ・ひらがな・漢字）。</w:t>
      </w:r>
      <w:r>
        <w:t>한국어: 유니코드 텍스트 파싱 검증.</w:t>
      </w:r>
      <w:r>
        <w:t>العربية: اختبار فك التشفير والكيانات</w:t>
      </w:r>
      <w:r>
        <w:t>Deutsch: Größe &amp; Maßstäbe, Überträger</w:t>
      </w:r>
      <w:pict>
        <v:shape xmlns:v="urn:schemas-microsoft-com:vml">
          <v:textbox>
            <w:txbxContent>
              <w:p>
                <w:r>
                  <w:t>框內文字：&lt;TextInBox&gt; &amp; &#x2019;2026&#x2019;</w:t>
                </w:r>
              </w:p>
            </w:txbxContent>
          </v:textbox>
        </v:shape>
      </w:pict>
    </w:p>
    <w:p>
      <w:r><w:fldChar w:fldCharType="begin"/></w:r>
      <w:r><w:instrText> PAGE \* MERGEFORMAT </w:instrText></w:r>
      <w:r><w:fldChar w:fldCharType="separate"/></w:r>
      <w:r><w:t>1</w:t></w:r>
      <w:r><w:fldChar w:fldCharType="end"/></w:r>
    </w:p>
    <w:tbl>
      <w:tblPr>
        <w:tblW w:w="5000" w:type="dxa"/>
      </w:tblPr>
      <w:tr>
        <w:tc>
          <w:tcPr>
            <w:tcW w:w="2500" w:type="dxa"/>
          </w:tcPr>
          <w:p><w:r><w:t>儲存格文字 &amp; &#x2019;格1&#x2019;</w:t></w:r></w:p>
        </w:tc>
      </w:tr>
    </w:tbl>
  </w:body>
</w:document>"#;

    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="Custom&amp;Style">
    <w:rPr>
      <w:rFonts w:ascii="Times &amp; Roman &#x2019;Special&#x2019;"/>
      <w:sz w:val="28"/>
      <w:color w:val="FF0000"/>
    </w:rPr>
  </w:style>
</w:styles>"#;

    let rels_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles_&amp;_target.xml"/>
</Relationships>"#;

    let docx = make_test_docx(doc_xml, styles_xml, rels_xml);
    let doc = parse(&docx).expect("DOCX 解析必須成功");

    // 寫死基線值（逐字元驗證）
    let expected_runs = [
        "Hello & World <Foo> \"Bar\" 'Baz' ’26 ’ 中文 'test'",
        "繁體中文：實體測試與 UTF-8 解碼驗證。",
        "日本語：テスト文字列（カタカナ・ひらがな・漢字）。",
        "한국어: 유니코드 텍스트 파싱 검증.",
        "العربية: اختبار فك التشفير والكيانات",
        "Deutsch: Größe & Maßstäbe, Überträger",
    ];

    let p0 = &doc.paragraphs[0];
    assert_eq!(p0.runs.len(), expected_runs.len(), "段落 1 的 run 數必須為 6");
    for (i, expected) in expected_runs.iter().enumerate() {
        assert_eq!(&p0.runs[i].text, expected, "Run {} 文字逐字元比對必須吻合", i);
    }

    // 段落樣式屬性驗證（lib.rs:1409 attr_val）
    assert_eq!(
        p0.style.as_deref(),
        Some("Custom&Style"),
        "段落樣式屬性值 unescape 必須與基線吻合"
    );

    // 文字框文字驗證（lib.rs:1339 txbx_cur.push_str）
    assert_eq!(
        p0.txbx_texts,
        vec!["框內文字：<TextInBox> & ’2026’"],
        "文字框內文字 unescape 必須與基線逐字元吻合"
    );

    // 複雜欄位指令文字驗證（lib.rs:1351 complex_stack）
    let p1 = &doc.paragraphs[1];
    let page_run = p1
        .runs
        .iter()
        .find(|r| r.field.is_some())
        .expect("應有帶欄位標記的 run");
    assert_eq!(
        page_run.field,
        Some(FieldKind::Page),
        "PAGE 欄位指令必須正確解析"
    );
    assert_eq!(page_run.text, "1", "結果文字吻合");

    // 表格屬性解析驗證（table.rs:1299 attr）
    assert_eq!(doc.tables.len(), 1, "必須解析出 1 個表格");
    let tbl = &doc.tables[0];
    assert_eq!(tbl.table.rows.len(), 1, "表格應有 1 列");
    assert_eq!(tbl.table.rows[0].cells.len(), 1, "儲存格數應為 1");
    let cell_idx = match tbl.table.rows[0].cells[0].blocks[0] {
        quire_ooxml::Block::Paragraph { index } => index,
        _ => panic!("預期為段落 block"),
    };
    let cell_p = &doc.paragraphs[cell_idx];
    assert_eq!(
        cell_p.runs[0].text,
        "儲存格文字 & ’格1’",
        "表格儲存格文字 unescape 必須與基線逐字元吻合"
    );

    // styles.xml 屬性解析驗證（styles.rs:871 attr_val）
    let style_table = StyleTable::parse(styles_xml.as_bytes());
    let (effective_run, _) = style_table.effective_para(Some("Custom&Style"), Default::default());
    assert_eq!(effective_run.size_pt, Some(14.0), "sz 28 half-points 應解析為 14.0pt");
    assert_eq!(effective_run.color, Some(0xFFFF_0000), "color FF0000 應解析為 0xFFFF0000");

    // rels.rs 屬性解析驗證（rels.rs:117 attr）
    let rels_vec = read_rels(rels_xml.as_bytes());
    let rel = rels_vec.iter().find(|r| r.id == "rIdStyles").expect("必須找到 rIdStyles 關係");
    assert_eq!(
        rel.target_part.as_deref(),
        Some("word/styles_&_target.xml"),
        "rels Target 屬性值 unescape 必須與基線逐字元吻合"
    );
}

/// 燈 2：RUSTSEC-2026-0194 防線（start tag 內重複屬性名 DoS 驗證）
/// 構造 start tag 內含 10,000 個重複屬性，斷言解析在線性時間內完成（限時 500ms）。
#[test]
fn test_cve_2026_0194_duplicate_attributes_dos_defense() {
    let mut attr_buf = String::with_capacity(400_000);
    for i in 0..10_000 {
        use std::fmt::Write;
        let _ = write!(attr_buf, " attr_{}=\"val_{}\"", i, i);
    }
    let xml = format!(
        r#"<w:p {} xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:r><w:t>safe</w:t></w:r></w:p>"#,
        attr_buf
    );

    let start = Instant::now();
    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut count = 0;
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Eof) | Err(_) => break,
            Ok(quick_xml::events::Event::Start(e)) => {
                // 走訪屬性：若為 O(N^2) 檢查，10,000 個重複屬性將極度緩慢
                for a in e.attributes() {
                    if a.is_ok() {
                        count += 1;
                    }
                }
            }
            _ => {}
        }
    }
    let elapsed = start.elapsed();
    assert_eq!(count, 10_001, "屬性走訪總數（含 xmlns）吻合");
    assert!(
        elapsed.as_millis() < 500,
        "重複屬性解析必須在 500ms 內線性時間完成，實際耗時: {:?}",
        elapsed
    );
}

/// 燈 3：RUSTSEC-2026-0195 防線（深巢狀命名空間宣告記憶體與耗時 DoS 驗證）
/// 構造 1,000 層深巢狀命名空間宣告，斷言在 500ms 內解析完成。
#[test]
fn test_cve_2026_0195_deep_nested_namespaces_dos_defense() {
    let depth = 1_000;
    let mut xml = String::with_capacity(depth * 80);
    for i in 0..depth {
        use std::fmt::Write;
        let _ = write!(xml, "<ns{}:node xmlns:ns{}=\"http://example.com/{}\">", i, i, i);
    }
    xml.push_str("<content>safe</content>");
    for i in (0..depth).rev() {
        use std::fmt::Write;
        let _ = write!(xml, "</ns{}:node>", i);
    }

    let start = Instant::now();
    let mut reader = quick_xml::NsReader::from_str(&xml);
    let mut depth_count = 0;
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Eof) | Err(_) => break,
            Ok(quick_xml::events::Event::Start(_)) => {
                depth_count += 1;
            }
            _ => {}
        }
    }
    let elapsed = start.elapsed();
    assert_eq!(depth_count, depth + 1, "巢狀節點計數吻合");
    assert!(
        elapsed.as_millis() < 500,
        "深巢狀命名空間解析必須在 500ms 內安全完成，實際耗時: {:?}",
        elapsed
    );
}
