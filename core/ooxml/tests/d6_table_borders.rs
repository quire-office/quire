//! D6 紅燈考卷：表格邊框採集與合併格繼承（ECMA-376 §17.4.4）。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 考卷核心斷言：
//! 1. 燈 1：vMerge continuation cell 的底線繼承規則按 ECMA-376 §17.4.4（tcBorders 沿用規則）實作，
//!    末格承接 restart cell 的 bottom 邊框，禁按「合併格無框」偷懶（修前 continuation cell 為 None）。
//! 2. 燈 2：vMerge 與 gridSpan 組合情境下 continuation cell 邊框繼承，跨欄又跨列底邊不丟失。
//! 3. 燈 3：tcBorders / tblBorders 的 w:start 與 w:end 方位採集支援（修前漏採集）。

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

        // Local header
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

/// 建造最小 DOCX 位元組，內含指定 document.xml。
fn make_docx(document_xml: &str) -> Vec<u8> {
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
    b.add_stored(b"word/document.xml", document_xml.as_bytes());
    b.finish()
}

/// 燈 1：vMerge continuation cell 的底線繼承規則按 ECMA-376 §17.4.4 實作。
/// 末格承接 restart cell 的 bottom 邊框，禁按「合併格無框」偷懶。
#[test]
fn lamp1_vmerge_continuation_cell_inherits_borders() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:tbl>
      <w:tblGrid>
        <w:gridCol w:w="3000"/>
        <w:gridCol w:w="3000"/>
      </w:tblGrid>
      <w:tr>
        <w:tc>
          <w:tcPr>
            <w:vMerge w:val="restart"/>
            <w:tcBorders>
              <w:top w:val="single" w:sz="8" w:space="0" w:color="000000"/>
              <w:left w:val="single" w:sz="8" w:space="0" w:color="000000"/>
              <w:bottom w:val="double" w:sz="16" w:space="0" w:color="FF0000"/>
              <w:right w:val="single" w:sz="8" w:space="0" w:color="000000"/>
            </w:tcBorders>
          </w:tcPr>
          <w:p><w:r><w:t>A1</w:t></w:r></w:p>
        </w:tc>
        <w:tc><w:p><w:r><w:t>B1</w:t></w:r></w:p></w:tc>
      </w:tr>
      <w:tr>
        <w:tc>
          <w:tcPr>
            <w:vMerge w:val="continue"/>
          </w:tcPr>
          <w:p><w:r><w:t>A2 (continue mid)</w:t></w:r></w:p>
        </w:tc>
        <w:tc><w:p><w:r><w:t>B2</w:t></w:r></w:p></w:tc>
      </w:tr>
      <w:tr>
        <w:tc>
          <w:tcPr>
            <w:vMerge w:val="continue"/>
          </w:tcPr>
          <w:p><w:r><w:t>A3 (continue tail)</w:t></w:r></w:p>
        </w:tc>
        <w:tc><w:p><w:r><w:t>B3</w:t></w:r></w:p></w:tc>
      </w:tr>
    </w:tbl>
  </w:body>
</w:document>"#;

    let docx = make_docx(xml);
    let doc = parse(&docx).expect("parse docx");
    assert_eq!(doc.tables.len(), 1, "必須解析出一個表格");
    let tbl = &doc.tables[0].table;
    assert_eq!(tbl.rows.len(), 3, "必須有 3 列");

    // 檢視第 1 列（中間 continuation 格）與第 2 列（末尾 continuation 格）：
    let mid_cell = &tbl.rows[1].cells[0];
    let tail_cell = &tbl.rows[2].cells[0];

    // 斷言：continuation cell 禁按「合併格無框」偷懶，props.borders 必須存在且繼承自 restart cell
    let tail_borders = tail_cell
        .props
        .borders
        .as_ref()
        .expect("燈 1 失敗：末尾 continuation cell 必須繼承邊框，不可為 None（修前缺線根因）");

    // 末格必須承接 restart cell 的 bottom 邊框（ECMA-376 §17.4.4）
    let bottom = tail_borders
        .bottom
        .as_ref()
        .expect("末尾 continuation cell 必須具有底邊邊框");
    assert_eq!(
        bottom.val.as_deref(),
        Some("double"),
        "末尾格底邊應繼承 restart cell 的 double 底線"
    );
    assert_eq!(
        bottom.sz.as_deref(),
        Some("16"),
        "末尾格底邊寬度應繼承 restart cell 的 sz=16"
    );
    assert_eq!(
        bottom.color.as_deref(),
        Some("FF0000"),
        "末尾格底邊顏色應繼承 restart cell 的 color=FF0000"
    );

    // 末格必須沿用 restart cell 的 left / right 邊框
    assert_eq!(
        tail_borders.left.as_ref().and_then(|b| b.val.as_deref()),
        Some("single"),
        "末尾格左邊應沿用 restart cell 的 left 邊框"
    );
    assert_eq!(
        tail_borders.right.as_ref().and_then(|b| b.val.as_deref()),
        Some("single"),
        "末尾格右邊應沿用 restart cell 的 right 邊框"
    );

    // 中間 continuation 格：按 ECMA-376 §17.4.4，內部橫線不畫（底邊應為 None 或無底線），但側邊 left/right 應沿用
    if let Some(mb) = &mid_cell.props.borders {
        assert!(
            mb.bottom.is_none() || mb.bottom.as_ref().unwrap().val.as_deref() == Some("none"),
            "中間 continuation 格底邊應被抑制（無內部橫線）"
        );
        assert_eq!(
            mb.left.as_ref().and_then(|b| b.val.as_deref()),
            Some("single"),
            "中間 continuation 格左邊應沿用 restart cell"
        );
    }
}

/// 燈 2：vMerge 與 gridSpan 組合情境下，跨欄又跨列 continuation cell 邊框繼承。
#[test]
fn lamp2_vmerge_with_gridspan_inherits_borders() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:tbl>
      <w:tblGrid>
        <w:gridCol w:w="3000"/>
        <w:gridCol w:w="3000"/>
        <w:gridCol w:w="3000"/>
      </w:tblGrid>
      <w:tr>
        <w:tc>
          <w:tcPr>
            <w:gridSpan w:val="2"/>
            <w:vMerge w:val="restart"/>
            <w:tcBorders>
              <w:bottom w:val="thick" w:sz="24" w:space="0" w:color="0000FF"/>
              <w:left w:val="single" w:sz="8" w:space="0" w:color="000000"/>
              <w:right w:val="single" w:sz="8" w:space="0" w:color="000000"/>
            </w:tcBorders>
          </w:tcPr>
          <w:p><w:r><w:t>Col 0-1 Row 0</w:t></w:r></w:p>
        </w:tc>
        <w:tc><w:p><w:r><w:t>Col 2 Row 0</w:t></w:r></w:p></w:tc>
      </w:tr>
      <w:tr>
        <w:tc>
          <w:tcPr>
            <w:gridSpan w:val="2"/>
            <w:vMerge w:val="continue"/>
          </w:tcPr>
          <w:p><w:r><w:t>Col 0-1 Row 1 (tail)</w:t></w:r></w:p>
        </w:tc>
        <w:tc><w:p><w:r><w:t>Col 2 Row 1</w:t></w:r></w:p></w:tc>
      </w:tr>
    </w:tbl>
  </w:body>
</w:document>"#;

    let docx = make_docx(xml);
    let doc = parse(&docx).expect("parse docx");
    let tbl = &doc.tables[0].table;
    let tail_cell = &tbl.rows[1].cells[0];

    let tail_borders = tail_cell
        .props
        .borders
        .as_ref()
        .expect("燈 2 失敗：跨欄又跨列 continuation cell 必須繼承邊框");
    let bottom = tail_borders
        .bottom
        .as_ref()
        .expect("跨欄又跨列 continuation cell 必須具有底邊邊框");
    assert_eq!(bottom.val.as_deref(), Some("thick"));
    assert_eq!(bottom.sz.as_deref(), Some("24"));
    assert_eq!(bottom.color.as_deref(), Some("0000FF"));
}

/// 燈 3：tcBorders 與 tblBorders 支援 w:start 與 w:end 方位採集（ECMA-376 / ISO 29500）。
#[test]
fn lamp3_start_and_end_borders_collected() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:tbl>
      <w:tblPr>
        <w:tblBorders>
          <w:start w:val="dashed" w:sz="12" w:space="0" w:color="00AA00"/>
          <w:end w:val="dotted" w:sz="12" w:space="0" w:color="00AA00"/>
        </w:tblBorders>
      </w:tblPr>
      <w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid>
      <w:tr>
        <w:tc>
          <w:tcPr>
            <w:tcBorders>
              <w:start w:val="single" w:sz="8" w:space="0" w:color="FF0000"/>
              <w:end w:val="single" w:sz="8" w:space="0" w:color="FF0000"/>
            </w:tcBorders>
          </w:tcPr>
          <w:p><w:r><w:t>Cell 1</w:t></w:r></w:p>
        </w:tc>
      </w:tr>
    </w:tbl>
  </w:body>
</w:document>"#;

    let docx = make_docx(xml);
    let doc = parse(&docx).expect("parse docx");
    let tbl = &doc.tables[0].table;

    // 表格級 start / end 映射至 left / right
    let tbl_borders = tbl.props.borders.as_ref().expect("tblPr tblBorders 應存在");
    assert_eq!(
        tbl_borders.left.as_ref().and_then(|b| b.val.as_deref()),
        Some("dashed"),
        "tblBorders start 應映射至 left"
    );
    assert_eq!(
        tbl_borders.right.as_ref().and_then(|b| b.val.as_deref()),
        Some("dotted"),
        "tblBorders end 應映射至 right"
    );

    // 儲存格級 start / end 映射至 left / right
    let cell_borders = tbl.rows[0].cells[0]
        .props
        .borders
        .as_ref()
        .expect("tcBorders 應存在");
    assert_eq!(
        cell_borders.left.as_ref().and_then(|b| b.val.as_deref()),
        Some("single"),
        "tcBorders start 應映射至 left"
    );
    assert_eq!(
        cell_borders.right.as_ref().and_then(|b| b.val.as_deref()),
        Some("single"),
        "tcBorders end 應映射至 right"
    );
}
