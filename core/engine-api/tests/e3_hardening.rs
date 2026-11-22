//! E-3 引擎返修卡 t_81e69488 **紅燈考卷**（agy 批次審查坐實，先紅後綠）。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 四條缺陷（卡面錨點＝mutation_k29c.rs@a33944e）
//! 1. **S2-1** `k29c_delete_row` 的 vMerge restart 判定（L1748）：
//!    `attr_in_tag(tcpr, "w:val")` 對**整段 tcPr** 取首個 `w:val`——依 ECMA
//!    子元素序 `w:cnfStyle`／`w:gridSpan` 排在 `w:vMerge` 之前，
//!    `<w:gridSpan w:val="2"/>…<w:vMerge w:val="restart"/>` 時恆讀到 "2" ⇒
//!    restart=false ⇒ 刪頂列後延續格漏晉升、變孤兒 continue。修法比照同檔
//!    `cell_grid_span`（定位開標籤再取 attr）。
//! 2. **S2-2** wasm32（32-bit usize）裸加法溢出防線漏網：`old_span + 1`
//!    （L1845）、`c.col_start + c.span > left_grid_col + span_w`（L2002-2009）、
//!    `at_grid_col + 1`（L1794）、`table_map.rs::compute_cond_gates`
//!    `(start + 1) % 2`（L123，u32 運算＝host debug 也炸）。修法：
//!    `checked_add`／`saturating_add`＋寫回值守 u32 同口徑；host 態以
//!    **回拒斷言**驗，不依賴 panic。
//! 3. **S2-3** `table_identity_error` 只驗首段落、漏 tr／tc 透明包裝層：
//!    `<w:ins>` 包在**列層級**時首段落仍在 `<w:tbl>` 界內獲放行，隨後
//!    `direct_children("tr"/"tc")` 照樣界內錯位改錯列格。另
//!    `cell_grid_span` 的 `find_local_tag` 不看深度會穿透
//!    `<w:tcPrChange><w:tcPr>` 讀到修訂前舊值。修法：補驗
//!    `direct_children("tr").len()==rows.len()` 與每列 `tc` 數量；
//!    `tcPrChange` 子樹排除。
//! 4. **S3-4** `classify_locate_error`（L1519-1536）一刀切覆寫：見
//!    `raw_root_count != doc.tables.len()` 就把 `SubDocNotFound`／真越界
//!    全覆寫成「透明包裝錯位」；反之巢狀層透明包裝（根表數相等）又漏歸類。
//!    修法：僅 `target==Body` 且 canonical 樹可定位而 raw 定位失敗才歸類。
//!
//! # 斷言層口徑（零第二輪翻譯）
//! 路徑一律由反查面（`table_locator_json`）鑄出原封不動餵 M4；真相用
//! canonical 回讀（`commit_k29c` 必經 `rebuild_derived()`，`tables_json`
//! ＝編輯後事實）＋ `word/document.xml` 明文掃描雙軌。拒絕態必驗**零波及**
//! （存檔位元組與基線相同）。

use quire_engine::{Engine, HfSlotKind, InsertSide, Mutation, RowPropsPatch, SubDocTarget, TablePath};
use serde_json::Value;

// ---------------------------------------------------------------------------
// 最小 docx 合成器（純 stdlib、STORE、零內嵌資源——同 e1_run_patch_merge 紀律）
// ---------------------------------------------------------------------------

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

fn docx_with_parts(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = ZipTestHelper::new();
    for (name, data) in parts {
        zip.add(name, data.as_bytes());
    }
    zip.finish()
}

const CT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const DOC_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#;

fn open_docx(document_xml: &str) -> Engine {
    open_docx_with(&[
        ("[Content_Types].xml", CT_XML),
        ("_rels/.rels", ROOT_RELS),
        ("word/_rels/document.xml.rels", DOC_RELS),
        ("word/document.xml", document_xml),
    ])
}

fn open_docx_with(parts: &[(&str, &str)]) -> Engine {
    Engine::from_docx_bytes(&docx_with_parts(parts)).expect("合成 docx 開檔失敗")
}

fn doc_xml(e: &Engine) -> String {
    String::from_utf8(
        e.part("word/document.xml")
            .expect("必須有 word/document.xml")
            .to_vec(),
    )
    .expect("document.xml 非 UTF-8")
}

fn json(s: String) -> Value {
    serde_json::from_str(&s).expect("引擎輸出必須是合法 JSON")
}

fn locate(e: &Engine, para: usize) -> Option<Value> {
    e.table_locator_json(para).map(json)
}

/// 反查產物原封不動還原成 `TablePath`（零第二輪翻譯）。
fn path_of(hit: &Value) -> TablePath {
    TablePath {
        target: SubDocTarget::Body,
        root_slot: hit["table_path"]["root_slot"]
            .as_u64()
            .expect("root_slot") as usize,
        nested_steps: hit["table_path"]["nested_steps"]
            .as_array()
            .expect("nested_steps 是陣列")
            .iter()
            .map(|s| {
                let a = s.as_array().expect("巢狀步是三元組");
                (
                    a[0].as_u64().expect("row") as usize,
                    a[1].as_u64().expect("cell") as usize,
                    a[2].as_u64().expect("sub") as usize,
                )
            })
            .collect(),
    }
}

/// 找「列 r 第 c 格」內段落的反查結果（本檔所有 fixture 每格恰一段）。
fn locate_cell(e: &Engine, row: usize, cell: usize) -> (usize, Value) {
    for p in 0..e.paragraph_count() {
        if let Some(h) = locate(e, p) {
            if h["cell_path"]["row"].as_u64() == Some(row as u64)
                && h["cell_path"]["cell"].as_u64() == Some(cell as u64)
            {
                return (p, h);
            }
        }
    }
    panic!("列 {row} 格 {cell} 要有可反查段落");
}

fn root_table(e: &Engine, slot: usize) -> Value {
    json(e.tables_json())[slot]["table"].clone()
}

/// 某格 vMerge 有效態（明文口徑，不吃 canonical——canonical 側 `tcPrChange`
/// 洩漏屬 E-2 禁改面，本卡不得依賴其正確性）。回傳：Some(true)＝restart／
/// Some(false)＝continue／None＝格內直屬 vMerge 缺席。
fn vmerge_state_of_cell(xml: &str, cell_needle: &str) -> Option<bool> {
    let at = xml
        .find(cell_needle)
        .unwrap_or_else(|| panic!("document.xml 必須含 {cell_needle:?}"));
    let tc_at = xml[..at]
        .rfind("<w:tc>")
        .or_else(|| xml[..at].rfind("<w:tc "))
        .unwrap_or_else(|| panic!("{cell_needle:?} 之前必須有 <w:tc>"));
    let tc_end = xml[at..]
        .find("</w:tc>")
        .map(|rel| at + rel)
        .expect("受標格要有 </w:tc>");
    let tc = &xml[tc_at..tc_end];
    let vm = tc.find("<w:vMerge")?;
    let open_end = tc[vm..].find('>').expect("vMerge 開標籤要有 '>'") + vm + 1;
    Some(tc[vm..open_end].contains("restart"))
}

// ===========================================================================
// S2-1：k29c_delete_row 的 vMerge restart 判定讀錯元素
// ===========================================================================
//
// fixture 自證（燈 A0）：受標格的 `<w:gridSpan w:val="2"/>` 必須**先於**
// `<w:vMerge w:val="restart"/>`，且兩者之間無其他 `w:val`——上游改掉形態時
// 本燈先紅，燈 A1 不失指。
//
// 燈 A1（核心紅燈）：刪 restart 頂列後，下一列同欄 continue 格必須晉升
// restart。修前：`attr_in_tag(tcpr,"w:val")` 恆讀到 gridSpan 的 "2" ⇒
// restart=false ⇒ 跳過晉升 ⇒ 存續格仍是 `<w:vMerge/>`（孤兒 continue）。

const VMERGE_FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid>
<w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/><w:gridSpan w:val="2"/><w:vMerge w:val="restart"/></w:tcPr><w:p><w:r><w:t>甲顶格</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/><w:gridSpan w:val="2"/><w:vMerge/></w:tcPr><w:p><w:r><w:t>甲续格</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>底左格</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>底右格</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>
</w:body></w:document>"#;

#[test]
fn e3_s2_1_lampA0_fixture_form_is_self_proving() {
    let xml = doc_xml(&open_docx(VMERGE_FIXTURE));
    let gs = xml
        .find(r#"<w:gridSpan w:val="2"/>"#)
        .expect("fixture 要有 gridSpan(2) 前綴");
    let gs_end = gs + r#"<w:gridSpan w:val="2"/>"#.len();
    let vm_rel = xml[gs_end..]
        .find(r#"<w:vMerge w:val="restart"/>"#)
        .expect("gridSpan 之後要有 vMerge restart");
    assert!(
        !xml[gs_end..gs_end + vm_rel].contains("w:val="),
        "gridSpan 與 vMerge 之間不得有其他 w:val（否則燈 A1 的『首個 w:val』錯讀失去指向）"
    );
    let e = open_docx(VMERGE_FIXTURE);
    assert_eq!(e.table_count(), 1, "fixture 恰一張根表");
    let t = root_table(&e, 0);
    assert_eq!(
        t["rows"][0]["cells"].as_array().expect("cells 是陣列").len(),
        1,
        "頂列恰一格（跨 2 欄）"
    );
}

#[test]
fn e3_s2_1_delete_restart_row_promotes_continuation_even_with_gridspan_prefix() {
    // 修前紅：首個 w:val 讀到 gridSpan 的 "2" ⇒ restart 誤判 false ⇒
    // 續格不晉升（孤兒 continue）。
    let mut e = open_docx(VMERGE_FIXTURE);
    let (_, hit) = locate_cell(&e, 0, 0);
    e.apply_mutation(Mutation::DeleteTableRow {
        table: path_of(&hit),
        row: 0,
    })
    .expect("DeleteTableRow 不得失敗");
    let xml = doc_xml(&e);
    assert_eq!(
        vmerge_state_of_cell(&xml, "甲续格"),
        Some(true),
        "刪掉 restart 頂列後，原續格必須晉升 restart（否則成孤兒 continue）；\
         修前形態＝attr_in_tag 對整段 tcPr 取首個 w:val，恆讀到 gridSpan 的 \"2\""
    );
}
