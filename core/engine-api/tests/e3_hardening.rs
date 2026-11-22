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
<w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/><w:gridSpan w:val="2"/><w:vMerge w:val="restart"/></w:tcPr><w:p><w:r><w:t>甲頂格</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/><w:gridSpan w:val="2"/><w:vMerge/></w:tcPr><w:p><w:r><w:t>甲續格</w:t></w:r></w:p></w:tc></w:tr>
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

// ===========================================================================
// S2-2：wasm32（32-bit usize）裸加法溢出防線
// ===========================================================================
//
// 紅燈用 `u32::MAX` 級畸形 `gridSpan`／`usize::MAX` 級 `at_grid_col` 探針；
// host 態斷言**回拒且零波及**，不依賴 panic（wasm32 debug 的
// `attempt to add with overflow` 在 64 位 host 無感，但回繞／超域寫回
// 在 host 同樣是「算出錯值」——同樣必紅）。

const OVERFLOW_GRIDSPAN_FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid>
<w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/><w:gridSpan w:val="4294967295"/></w:tcPr><w:p><w:r><w:t>溢位格</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>底左格</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>底右格</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>
</w:body></w:document>"#;

#[test]
fn e3_s2_2_lampB1_insert_column_into_u32max_span_rejects_not_miscomputes() {
    // `old_span + 1`（L1845）：wasm32 debug panic／release 回繞 0 ⇒
    // set_gridspan(0) 錯值；64 位 host 寫出 u32 裝不下的 4294967296——
    // canonical `CellProps::grid_span()` 以 u32 解析讀回＝1（同義兩制）。
    // 修後斷言＝回拒 InvalidTableOperation 且存檔位元組零波及。
    let e0 = open_docx(OVERFLOW_GRIDSPAN_FIXTURE);
    let base_saved = e0.save_docx().expect("基線存檔");
    let (_, hit) = locate_cell(&e0, 0, 0);
    let mut e = open_docx(OVERFLOW_GRIDSPAN_FIXTURE);
    let res = e.apply_mutation(Mutation::InsertTableColumn {
        table: path_of(&hit),
        // 插入點 1 落在 span=u32::MAX 的格**內部**（0 < 1 < saturating 右緣）
        // ⇒ 走 `old_span + 1` 分支——這正是缺陷載體路徑。
        at_grid_col: 1,
        position: InsertSide::Before,
        width_pt: 20.0,
    });
    let err = match res {
        Ok(_) => panic!(
            "u32::MAX 級畸形 gridSpan 的格內插欄必須回拒——成功＝把跨度寫成 \
             u32 裝不下的錯值（host）或回繞值（wasm32）"
        ),
        Err(err) => format!("{err:?}"),
    };
    assert!(
        err.contains("InvalidTableOperation"),
        "溢位必須回 InvalidTableOperation（誠實回拒），實得: {err}"
    );
    assert_eq!(
        e.save_docx().expect("拒絕後存檔"),
        base_saved,
        "拒絕必須零波及（位元組級）"
    );
}

#[test]
fn e3_s2_2_lampB2_merge_cells_span_w_overflow_rejects_not_bypasses_bounds() {
    // `right_grid_col - left_grid_col + 1`（L1981）與
    // `c.col_start + c.span > left_grid_col + span_w`（L2002-2009）：
    // wasm32 上兩側同時回繞 ⇒「嚴格涵蓋」檢查被繞過 ⇒ 靜默把超寬格合併、
    // 寫回回繞 gridSpan；64 位 host 上 span_w 巨大 ⇒ 涵蓋通過 ⇒
    // set_gridspan(span_w) 寫出 u32 裝不下的宣告。修後＝回拒且零波及。
    let e0 = open_docx(OVERFLOW_GRIDSPAN_FIXTURE);
    let base_saved = e0.save_docx().expect("基線存檔");
    let (_, hit) = locate_cell(&e0, 0, 0);
    let mut e = open_docx(OVERFLOW_GRIDSPAN_FIXTURE);
    let res = e.apply_mutation(Mutation::MergeTableCells {
        table: path_of(&hit),
        top_row: 0,
        bottom_row: 1,
        left_grid_col: 0,
        right_grid_col: usize::MAX / 2,
    });
    let err = match res {
        Ok(_) => panic!(
            "合併矩形寬度超出 gridSpan 可表示域必須回拒——成功＝寫回 u32 裝不下的\
             gridSpan（host）或回繞值（wasm32）"
        ),
        Err(err) => format!("{err:?}"),
    };
    assert!(
        err.contains("InvalidTableOperation"),
        "矩形溢位面必須回 InvalidTableOperation，實得: {err}"
    );
    assert_eq!(
        e.save_docx().expect("拒絕後存檔"),
        base_saved,
        "拒絕必須零波及（位元組級）"
    );
}

#[test]
fn e3_s2_2_lampB3_insert_column_after_huge_at_grid_col_rejects_cleanly() {
    // `at_grid_col + 1`（L1794）：usize::MAX 在 64 位 host 回繞成 0——
    // 「插入點 0」合法 ⇒ 靜默插到頭欄（錯值執行、越界檢查被繞過）；
    // wasm32 debug 直接 panic。修後斷言＝回拒且訊息帶越界語義（不誘導
    // 使用者以為要重選插入點位置以外的東西）。
    let e0 = open_docx(OVERFLOW_GRIDSPAN_FIXTURE);
    let base_xml = doc_xml(&e0);
    let base_saved = e0.save_docx().expect("基線存檔");
    let (_, hit) = locate_cell(&e0, 1, 0);
    let mut e = open_docx(OVERFLOW_GRIDSPAN_FIXTURE);
    let res = e.apply_mutation(Mutation::InsertTableColumn {
        table: path_of(&hit),
        at_grid_col: usize::MAX,
        position: InsertSide::After,
        width_pt: 20.0,
    });
    let err = match res {
        Ok(_) => panic!(
            "at_grid_col=usize::MAX 的回繞值 0 是合法插入點——修前在此靜默插頭欄；\
             修後必須在溢位處直接回拒"
        ),
        Err(err) => format!("{err:?}"),
    };
    assert!(
        err.contains("InvalidTableOperation"),
        "必須回 InvalidTableOperation，實得: {err}"
    );
    assert!(
        err.contains("越界"),
        "溢位插入點的拒絕訊息必須帶越界語義，實得: {err}"
    );
    assert_eq!(doc_xml(&e), base_xml, "拒絕後 document.xml 零變化");
    assert_eq!(
        e.save_docx().expect("拒絕後存檔"),
        base_saved,
        "拒絕必須零波及（位元組級）"
    );
}

/// 燈 B4（`table_map.rs::compute_cond_gates` L123，u32 運算＝host debug
/// 也炸）：畸形 `gridSpan w:val="4294967295"` 讓 `col.saturating_add(span)`
/// 把 `start` 推到 u32::MAX，第二格 `(start + 1) % 2` 在 **u32**（與 host
/// 位寬無關）溢出 ⇒ debug panic＝wasm 瀏覽器 abort。條件：表格帶
/// `w:tblStyle`（style_id Some）＋縱帶開（`w:noVBand="0"`）⇒
/// `compute_cond_gates` 在開檔 build_slots 就執行。修後＝開檔零 panic。
const COND_GATE_STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:style w:type="table" w:styleId="E3Band"><w:name w:val="E3Band"/>
<w:tblStylePr w:type="band1Vert"><w:rPr><w:color w:val="FF0000"/></w:rPr></w:tblStylePr>
</w:style>
</w:styles>"#;

const COND_GATE_OVERFLOW_DOC: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:tbl><w:tblPr><w:tblStyle w:val="E3Band"/><w:tblW w:w="6000" w:type="dxa"/><w:tblLayout w:type="fixed"/><w:tblLook w:val="00000021" w:noVBand="0"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid>
<w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/><w:gridSpan w:val="4294967295"/></w:tcPr><w:p><w:r><w:t>溢位格</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>第二格</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>
</w:body></w:document>"#;

#[test]
fn e3_s2_2_lampB4_cond_gate_start_plus_one_must_not_overflow_on_open() {
    // 修前紅：開檔即在 compute_cond_gates 的 `(start + 1) % 2` 上
    // `attempt to add with overflow` panic（wasm 態＝瀏覽器 abort）。
    // 修後綠：開檔、反查、顯示列表全綠（零 panic 由「能跑完」證明）。
    let e = open_docx_with(&[
        ("[Content_Types].xml", CT_XML),
        ("_rels/.rels", ROOT_RELS),
        ("word/_rels/document.xml.rels", DOC_RELS),
        ("word/styles.xml", COND_GATE_STYLES),
        ("word/document.xml", COND_GATE_OVERFLOW_DOC),
    ]);
    let _ = e.tables_summary_json();
    let _ = e.tables_json();
    for p in 0..e.paragraph_count() + 2 {
        let _ = locate(&e, p);
    }
    let _ = e.page_display_list(0);
}

// ===========================================================================
// S2-3：table_identity_error 漏 tr／tc 透明包裝層
// ===========================================================================
//
// 燈 C1（核心紅燈）：列級 `<w:ins>` 包裝列（`<w:tbl>…<w:ins><w:tr>…`）——
// canonical 穿透包裝照收（rows=3），raw `direct_children("tr")` 只認直屬 2
// 列 ⇒ 反查報 canonical row 1 的路徑餵給 M4 後，raw 側 `rows.get(1)` 拿到的
// 是**底列**（界內錯位）。修前：首段識別子仍落在 `<w:tbl>` 界內 ⇒ 放行 ⇒
// 靜默改錯列；修後：補驗列數／逐列格數配對 ⇒ 誠實拒絕且零波及。
//
// 燈 C2：`cell_grid_span` 的 `find_local_tag` 不看深度 ⇒ 穿透
// `<w:tcPrChange><w:tcPr>` 讀到修訂前舊值。

const INS_WRAPPED_ROW_FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid>
<w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>頂左格</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>頂右格</w:t></w:r></w:p></w:tc></w:tr>
<w:ins w:id="7" w:author="稽核" w:date="2026-01-01T00:00:00Z"><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>中左格</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>中右格</w:t></w:r></w:p></w:tc></w:tr></w:ins>
<w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>底左格</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>底右格</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>
</w:body></w:document>"#;

#[test]
fn e3_s2_3_lampC0_fixture_has_canonical_raw_row_count_divergence() {
    let e = open_docx(INS_WRAPPED_ROW_FIXTURE);
    let t = root_table(&e, 0);
    assert_eq!(
        t["rows"].as_array().expect("rows 是陣列").len(),
        3,
        "canonical 口徑：列級 w:ins 包裝的列照收（穿透幀）"
    );
    let xml = doc_xml(&e);
    assert_eq!(
        xml.matches("<w:tr>").count(),
        3,
        "fixture 自證：全檔 3 個 <w:tr> 開標籤"
    );
    assert!(
        xml.contains("<w:ins ") && xml.contains("</w:ins>"),
        "fixture 自證：w:ins 列級包裝在場（拿掉包裝時本燈先紅，燈 C1 不失指）"
    );
    let (_, hit) = locate_cell(&e, 1, 0);
    assert_eq!(
        hit["cell_path"]["row"].as_u64(),
        Some(1),
        "中段反查必須報 canonical row 1（此路徑餵 M4 即錯位載體）"
    );
}

#[test]
fn e3_s2_3_lampC1_row_level_ins_wrapper_must_not_silently_edit_wrong_row() {
    let e0 = open_docx(INS_WRAPPED_ROW_FIXTURE);
    let base_xml = doc_xml(&e0);
    let (_, hit) = locate_cell(&e0, 1, 0); // canonical 中段（藏在 w:ins 內）
    let mut e = open_docx(INS_WRAPPED_ROW_FIXTURE);
    match e.apply_mutation(Mutation::SetRowProps {
        table: path_of(&hit),
        row: 1,
        patch: RowPropsPatch {
            cant_split: Some(true),
            ..Default::default()
        },
    }) {
        Ok(_) => {
            let xml = doc_xml(&e);
            let at = xml.find("底左格").expect("底左格在場");
            let row_at = xml[..at].rfind("<w:tr>").expect("底左格前要有 <w:tr>");
            assert!(
                !xml[row_at..at].contains("<w:cantSplit"),
                "列級 w:ins 錯位檔：trPr 絕不得靜默落在『底』列（raw 界內錯位＝\
                 改錯列）；修後唯一合法的另一出口是誠實拒絕"
            );
        }
        Err(err) => {
            let s = format!("{err:?}");
            assert!(
                s.contains("路徑與文件不符") && s.contains("透明包裝"),
                "列級透明包裝錯位的拒絕必須點名『路徑與文件不符（透明包裝）』，實得: {s}"
            );
            assert_eq!(
                doc_xml(&e), base_xml,
                "拒絕後 document.xml 必須零變化（零半寫）"
            );
        }
    }
}

const TCPARCHANGE_FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid>
<w:tr><w:tc><w:tcPr><w:tcW w:w="6000" w:type="dxa"/><w:tcPrChange w:id="9" w:author="稽核" w:date="2026-01-01T00:00:00Z"><w:tcPr><w:tcW w:w="3000" w:type="dxa"/><w:gridSpan w:val="2"/></w:tcPr></w:tcPrChange></w:tcPr><w:p><w:r><w:t>修訂格</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>底左格</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>底右格</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>
</w:body></w:document>"#;

#[test]
fn e3_s2_3_lampC2_tcprchange_old_value_must_not_leak_into_grid_span() {
    // canonical 側（core/ooxml/src/table.rs，E-2 禁改面）把 tcPrChange 的
    // 內層 tcPr 也當 TcPr 幀、讀到舊值 "2"——本燈**不依賴**其對錯，只鎖
    // mutation 側行為：現行宣告無直屬 gridSpan ⇒ 有效跨欄數必為 1 ⇒ 插入點
    // 1 落在該格**右緣**（非內部）⇒ 該格宣告不得被動、列尾新增一格。
    // 修前態：cell_grid_span 的 find_local_tag 不看深度、穿透 tcPrChange
    // 讀到舊值 2 ⇒ 走「格內拆分」分支把 gridSpan 改寫成 3（宣告被污染）。
    let e0 = open_docx(TCPARCHANGE_FIXTURE);
    let t0 = root_table(&e0, 0);
    let baseline_span = t0["rows"][0]["cells"][0]["props"]["grid_span_raw"].clone();
    let baseline_cells = t0["rows"][0]["cells"].as_array().expect("cells").len();
    let (_, hit) = locate_cell(&e0, 1, 0);
    let mut e = open_docx(TCPARCHANGE_FIXTURE);
    e.apply_mutation(Mutation::InsertTableColumn {
        table: path_of(&hit),
        at_grid_col: 1,
        position: InsertSide::Before,
        width_pt: 20.0,
    })
    .expect("正常插欄不得失敗");
    let t = root_table(&e, 0);
    assert_eq!(
        t["rows"][0]["cells"][0]["props"]["grid_span_raw"],
        baseline_span,
        "tcPrChange 修訂前舊值不得洩漏進現行 span 計算：插欄後修訂格的 \
         grid_span_raw 必須與基線一致（修前態＝被改寫成 \"3\"）"
    );
    assert_eq!(
        t["rows"][0]["cells"].as_array().expect("cells").len(),
        baseline_cells + 1,
        "span 有效值=1 時插入點 1＝格右緣 ⇒ 必須以**新增一格**落地，\
         不得走格內拆分（拆分＝把舊值當成現行跨欄數的死亡形態）"
    );
}

// ===========================================================================
// S3-4：classify_locate_error 一刀切覆寫
// ===========================================================================
//
// 燈 D1：根表張數分岔檔的**非 Body 目標**（SubDocNotFound）被一刀切改寫成
// 「透明包裝錯位」——錯標籤線索；修後必須沿用 SubDocNotFound 原身分。
// 燈 D2：兩套口徑相等的真越界不得被覆寫成透明包裝。
// 燈 D3：巢狀層透明包裝（根表數相等）舊態漏歸類 ⇒ 必須歸類透明包裝。

/// 兩張根表、其中一張藏在 `w:sdt` 內（根表張數兩套口徑分岔＝1 vs 2）。
const TWO_ROOTS_ONE_WRAPPED: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:sdt><w:sdtContent><w:tbl><w:tblPr><w:tblW w:w="3000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>包內表</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:sdtContent></w:sdt>
<w:tbl><w:tblPr><w:tblW w:w="3000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>裸表</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
</w:body></w:document>"#;

#[test]
fn e3_s3_4_lampD1_subdoc_not_found_must_keep_its_identity() {
    let mut e = open_docx(TWO_ROOTS_ONE_WRAPPED);
    let path = TablePath {
        target: SubDocTarget::Header {
            section_id: 0,
            slot: HfSlotKind::Default,
        },
        root_slot: 0,
        nested_steps: Vec::new(),
    };
    let err = match e.apply_mutation(Mutation::DeleteTable { table: path }) {
        Ok(_) => panic!("無頁首 part 的 DeleteTable 必須失敗"),
        Err(err) => format!("{err:?}"),
    };
    assert!(
        err.contains("SubDocNotFound"),
        "非 Body 子樹不存在必須維持 SubDocNotFound 身分（根表張數分岔不改變\
         失敗原因），不得被一刀切覆寫成透明包裝錯位，實得: {err}"
    );
    assert!(
        !err.contains("透明包裝"),
        "SubDocNotFound 摻『透明包裝』＝假線索（誘導使用者重選表），實得: {err}"
    );
}

#[test]
fn e3_s3_4_lampD2_genuine_root_overrun_must_stay_overrun() {
    let mut e = open_docx(VMERGE_FIXTURE);
    let path = TablePath {
        target: SubDocTarget::Body,
        // 真越界（canonical 根表數 1 ＝ raw 直屬根表數 1，口徑未分岔）。
        root_slot: 5,
        nested_steps: Vec::new(),
    };
    let err = match e.apply_mutation(Mutation::DeleteTable { table: path }) {
        Ok(_) => panic!("root_slot 5 對單表檔必須越界失敗"),
        Err(err) => format!("{err:?}"),
    };
    assert!(
        err.contains("越界"),
        "兩套口徑相等的真越界必須沿用『越界』原訊息，實得: {err}"
    );
    assert!(
        !err.contains("透明包裝"),
        "根表數相等（1==1）時覆寫成透明包裝＝錯歸類，實得: {err}"
    );
}

const NESTED_INS_FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="6000"/></w:tblGrid>
<w:tr><w:tc><w:tcPr><w:tcW w:w="6000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>宿主格</w:t></w:r></w:p><w:ins w:id="3" w:author="稽核" w:date="2026-01-01T00:00:00Z"><w:tbl><w:tblPr><w:tblW w:w="2000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>包內子表</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:ins></w:tc></w:tr>
</w:tbl>
<w:p><w:r><w:t>尾段</w:t></w:r></w:p>
</w:body></w:document>"#;

/// 零段落＋列級包裝：目標表整顆無段落（純空白格）時首段識別子不存在，
/// 走「逐層張數配對」分支——舊實作該分支只驗 tbl 數量，列級 `w:ins` 照樣
/// 放行。本 fixture 自證列數分岔（canonical 2 列 vs raw 直屬 1 列）。
const ZERO_PARA_INS_ROW_FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:t>正文段</w:t></w:r></w:p>
<w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/><w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid>
<w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p/></w:tc><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p/></w:tc></w:tr>
<w:ins w:id="11" w:author="稽核" w:date="2026-01-01T00:00:00Z"><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p/></w:tc><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p/></w:tc></w:tr></w:ins>
</w:tbl>
</w:body></w:document>"#;

#[test]
fn e3_s2_3_lampC3_zero_paragraph_table_still_pairs_row_counts() {
    let e0 = open_docx(ZERO_PARA_INS_ROW_FIXTURE);
    let t = root_table(&e0, 0);
    assert_eq!(
        t["rows"].as_array().expect("rows").len(),
        2,
        "fixture 自證：canonical 穿透列級 w:ins ⇒ 2 列"
    );
    // 表內零可反查段落（每格都是空段）⇒ 走零段落分支。手鑄列 1（藏在
    // w:ins 內的那一列）的路徑。
    let path = TablePath {
        target: SubDocTarget::Body,
        root_slot: 0,
        nested_steps: Vec::new(),
    };
    let mut e = open_docx(ZERO_PARA_INS_ROW_FIXTURE);
    let err = match e.apply_mutation(Mutation::DeleteTableRow { table: path, row: 1 }) {
        Ok(_) => panic!(
            "零段落表的列級包裝錯位（raw 直屬 1 列 vs canonical 2 列）若成功＝\
             刪錯列（raw 直屬列 1 不存在，但 canonical 列 1 存在）"
        ),
        Err(err) => format!("{err:?}"),
    };
    assert!(
        err.contains("路徑與文件不符") && err.contains("透明包裝"),
        "零段落分支的列級分岔必須歸類透明包裝，實得: {err}"
    );
}

#[test]
fn e3_s3_4_lampD3_wrapped_subtable_must_be_classified_as_transparent_wrapper() {
    let e0 = open_docx(NESTED_INS_FIXTURE);
    // 反查面取「帶巢狀步」的那條路徑（包內子表段落）——零第二輪翻譯。
    let hit = (0..e0.paragraph_count())
        .filter_map(|p| locate(&e0, p))
        .find(|h| {
            !h["table_path"]["nested_steps"]
                .as_array()
                .unwrap()
                .is_empty()
        })
        .expect("包內子表段落要可反查出巢狀步");
    assert_eq!(
        hit["table_path"]["nested_steps"][0][2].as_u64(),
        Some(0),
        "巢狀步 sub＝0（本檔宿主格只有這張子表）"
    );
    let mut e = open_docx(NESTED_INS_FIXTURE);
    let err = match e.apply_mutation(Mutation::DeleteTable {
        table: path_of(&hit),
    }) {
        Ok(_) => panic!("raw 口徑下子表不存在（藏在 w:ins 內），必須失敗"),
        Err(err) => format!("{err:?}"),
    };
    assert!(
        err.contains("透明包裝"),
        "根表數相等但巢狀層透明包裝導致的定位失敗必須歸類透明包裝（修前=\
         『巢狀步 0 子表 0 越界』假線索），實得: {err}"
    );
    assert!(
        !err.contains("越界"),
        "界內錯位／包裝分岔的拒絕不得摻『越界』字樣，實得: {err}"
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
        vmerge_state_of_cell(&xml, "甲續格"),
        Some(true),
        "刪掉 restart 頂列後，原續格必須晉升 restart（否則成孤兒 continue）；\
         修前形態＝attr_in_tag 對整段 tcPr 取首個 w:val，恆讀到 gridSpan 的 \"2\""
    );
}
