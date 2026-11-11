//! 樣式級 `w:pageBreakBefore` 分頁整合考卷（oc-pbb-fix）。
//!
//! 讀**渲染產物**（引擎分頁結果），不讀內部 dump：
//! - 真檔 `corpus/ooxmlexport/fdo81492.docx`：Heading1 樣式帶
//!   `<w:pageBreakBefore/>`，段落僅 `pStyle`；LO 把 References 推到第 2 頁。
//!   修復前我方 1 頁（紅燈），修復後 2 頁且 References 在 page index 1。
//! - 同型最小 fixture（zip 灌 XML）：兩段落、第二段 `pStyle` 指向帶
//!   `pageBreakBefore` 的樣式；防「只有真檔過、語義寫錯」。

use quire_engine::Engine;

fn load(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀 {full}: {e}"));
    Engine::from_docx_bytes(&bytes).expect("open corpus docx")
}

fn page_text(eng: &Engine, page: usize) -> String {
    eng.page_lines(page)
        .unwrap_or_else(|| panic!("第 {page} 頁應存在"))
        .join("\n")
}

/// 燈 1（真檔紅轉綠）：fdo81492 樣式級 pageBreakBefore 生效。
#[test]
fn fdo81492_references_starts_on_second_page() {
    let eng = load("ooxmlexport/fdo81492.docx");
    assert_eq!(
        eng.page_count(),
        2,
        "樣式級 pageBreakBefore 未生效（t_f81492 紅證）：fdo81492 應為 2 頁"
    );
    let p0 = page_text(&eng, 0);
    let p1 = page_text(&eng, 1);
    assert!(
        !p0.contains("References"),
        "References 不得留在第 1 頁；p0＝{p0:?}"
    );
    assert!(
        p1.contains("References"),
        "References 應落在第 2 頁；p1＝{p1:?}"
    );
}

/// 燈 2（同型最小 fixture）：第二段僅靠 pStyle 指向帶 pageBreakBefore
/// 的樣式，即必須另起一頁。
#[test]
fn minimal_fixture_style_only_break_starts_second_page() {
    let mut z = ZipTestHelper::new();
    z.add(
        "[Content_Types].xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#,
    );
    z.add(
        "_rels/.rels",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
    );
    z.add(
        "word/_rels/document.xml.rels",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#,
    );
    z.add(
        "word/styles.xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:default="1" w:styleId="Normal"/>
  <w:style w:type="paragraph" w:styleId="BreakBefore">
    <w:pPr><w:pageBreakBefore/></w:pPr>
  </w:style>
</w:styles>"#,
    );
    z.add(
        "word/document.xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>First page content</w:t></w:r></w:p>
    <w:p>
      <w:pPr><w:pStyle w:val="BreakBefore"/></w:pPr>
      <w:r><w:t>Second page content</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#,
    );
    let bytes = z.finish();
    // 照卡面要求把展示檔落到 /tmp（寫不進去不影響考卷）。
    let _ = std::fs::write("/tmp/pbb_f81492_min.docx", &bytes);
    let eng = Engine::from_docx_bytes(&bytes).expect("開啟最小 fixture 失敗");
    assert_eq!(
        eng.page_count(),
        2,
        "僅靠 pStyle 的 pageBreakBefore 必須另起一頁（同型最小 fixture）"
    );
    assert!(
        page_text(&eng, 0).contains("First page content"),
        "第一段應留在第 1 頁"
    );
    assert!(
        page_text(&eng, 1).contains("Second page content"),
        "第二段應落在第 2 頁"
    );
}

struct ZipTestHelper {
    buf: Vec<u8>,
    centrals: Vec<Vec<u8>>,
    count: usize,
}

impl ZipTestHelper {
    fn new() -> Self {
        Self {
            buf: Vec::new(),
            centrals: Vec::new(),
            count: 0,
        }
    }

    fn add(&mut self, name: &str, data: &[u8]) {
        let offset = self.buf.len() as u64;
        let csize = data.len() as u64;
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let crc = crc.sum();

        self.buf.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
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
        self.buf.extend_from_slice(name.as_bytes());
        self.buf.extend_from_slice(data);

        let mut c = Vec::new();
        c.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
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
        c.extend_from_slice(name.as_bytes());
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

        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
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
