//! E-1 紅燈考卷：`RunDirectPatch` 欄位級合併——分次單欄呼叫不得互相覆蓋。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 缺陷（U4b t_b9ae819b 交付報告 §5／PM 於 merged main 實跑探針坐實）
//! 外殼每顆字元格式鈕都是「單欄 mutation」（`buildFormatTextRangeMutation`
//! 把未帶的欄位換算成 `null`＝引擎側 `None`）。契約（`RunDirectPatch` 文件
//! 開頭、`patch_rpr` 註解）宣稱「`None`＝保持原宣告，XML 原樣保留」，但
//! 實測同選區先點粗體再點斜體 → 粗體宣告蒸發：
//! `node shells/web/editor/tests/u4b_e1_probe.mjs` 輸出呼叫一 flags=1、
//! 呼叫二 flags=2（應 3）、呼叫三 flags=4（應 7），受標 run 只剩
//! `<w:u w:val="single"/>`。用戶可見：同選區先點底線再點粗體 → 底線消失。
//!
//! # 斷言層口徑
//! - 寫入面：改檔後 `word/document.xml` 明文逐字斷言（同族宣告俱在／覆寫／
//!   移除形態），另存檔重開（`save_docx` → `from_docx_bytes`）再驗一次，
//!   防止只在記憶體態正確。
//! - 發射面：`page_display_list` 的 `flags` 位元疊加（1→3→7），與探針同口徑。
//! - 無損紅線：全 `None` 補丁（零意圖）整檔 byte-identical——既有硬閂回歸鎖。
//!
//! # 燈號分派
//! 1. 分次單欄疊加（核心紅燈）：bold → italic → underline 三筆單欄呼叫後，
//!    `w:b`／`w:i`／`w:u` 三宣告俱在，flags 疊到 7。
//! 2. `None` 欄位繼承既有宣告：run 原自帶 `w:b`，只發 italic 補丁後 `w:b`
//!    仍在（不得因同段其他欄位觸碰而被波及）。
//! 3. `Some` 同族覆寫仍精確：`Some` 覆寫寫新值、舊同族宣告移除且不殘留重複。
//! 4. 顯式關閉形態不被誤刪：`Some(false)` 寫 `<w:b w:val="0"/>`，之後其他
//!    欄位的單欄呼叫不得把這條明確關閉宣告順手抹除。
//! 5. 零意圖硬閂：`RunDirectPatch::default()` → 存檔 byte-identical。
//! 6. 存檔重開閉環：燈 1 的三宣告經存檔＋重解析後讀回仍俱在（級聯真的看得見）。

use quire_engine::{
    DocPos, DocRange, DrawItem, Engine, Mutation, RunDirectPatch, SubDocTarget, UnderlineKind,
    FLAG_BOLD, FLAG_ITALIC, FLAG_UNDERLINE,
};

fn settle(engine: &mut Engine) {
    while !engine.is_settled() {
        engine.reflow_step_json();
    }
}

/// 受標段（para 1）的「這是一份」四碼選區。
fn target_range() -> DocRange {
    DocRange {
        target: SubDocTarget::Body,
        start: DocPos { para: 1, cp: 0 },
        end: DocPos { para: 1, cp: 4 },
    }
}

fn fmt(engine: &mut Engine, patch: RunDirectPatch) {
    engine
        .apply_mutation(Mutation::FormatTextRange {
            range: target_range(),
            patch,
        })
        .expect("套用 M2.1 FormatTextRange 失敗");
    settle(engine);
}

fn doc_xml(engine: &Engine) -> String {
    String::from_utf8(
        engine
            .part("word/document.xml")
            .expect("必須有 word/document.xml")
            .to_vec(),
    )
    .expect("document.xml 非 UTF-8")
}

/// 取出含 `needle` 的第一個 `<w:r>`…`</w:r>` 明文（受標 run 真值）。
fn run_xml_containing(xml: &str, needle: &str) -> String {
    let at = xml
        .find(needle)
        .unwrap_or_else(|| panic!("document.xml 必須含文字片段 {needle:?}"));
    let start = xml[..at]
        .rfind("<w:r>")
        .or_else(|| xml[..at].rfind("<w:r "))
        .unwrap_or_else(|| panic!("文字片段 {needle:?} 之前必須有 <w:r> 開標籤"));
    let end = xml[at..]
        .find("</w:r>")
        .map(|rel| at + rel + "</w:r>".len())
        .unwrap_or_else(|| panic!("文字片段 {needle:?} 之後必須有 </w:r>"));
    xml[start..end].to_string()
}

/// 受標段的 Text 項 flags（顯示列表發射面）。
fn flags_of(engine: &Engine, needle: &str) -> u8 {
    let dl = engine
        .page_display_list(0)
        .expect("第 0 頁顯示列表必須存在");
    dl.items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text { text, flags, .. } if text.contains(needle) => Some(*flags),
            _ => None,
        })
        .unwrap_or_else(|| panic!("顯示列表必須含文字片段 {needle:?}"))
}

/// 以明文 document.xml 組一個最小 docx（兩段；para 1 以「這是一份」開頭）。
fn docx_with_document_xml(document_xml: &str) -> Vec<u8> {
    let mut zip = ZipTestHelper::new();
    zip.add(
        "[Content_Types].xml",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#,
    );
    zip.add(
        "_rels/.rels",
        br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
    );
    zip.add("word/document.xml", document_xml.as_bytes());
    zip.finish()
}

fn body_wrap(runs: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>基準段</w:t></w:r></w:p>
    {runs}
    <w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr>
  </w:body>
</w:document>"#
    )
}

fn plain_para(rpr_inner: &str, text: &str) -> String {
    let rpr = if rpr_inner.is_empty() {
        String::new()
    } else {
        format!("<w:rPr>{rpr_inner}</w:rPr>")
    };
    format!("<w:p><w:r>{rpr}<w:t>{text}</w:t></w:r></w:p>")
}

fn engine_from(runs: &str) -> Engine {
    let bytes = docx_with_document_xml(&body_wrap(runs));
    let mut engine =
        Engine::from_docx_bytes(&bytes).expect("以明文 document.xml 組裝 docx 並開啟失敗");
    settle(&mut engine);
    engine
}

/// 燈 1（核心紅燈）：同選區分三次單欄呼叫，三宣告必須俱在且 flags 疊加。
#[test]
fn e1_lamp1_sequential_single_field_calls_accumulate() {
    let mut engine = engine_from(&plain_para("", "這是一份受標文字"));

    fmt(
        &mut engine,
        RunDirectPatch {
            bold: Some(true),
            ..Default::default()
        },
    );
    let flags_1 = flags_of(&engine, "這是一份");
    assert_eq!(
        flags_1 & FLAG_BOLD,
        FLAG_BOLD,
        "呼叫一（bold）後粗體旗標必須亮起"
    );

    fmt(
        &mut engine,
        RunDirectPatch {
            italic: Some(true),
            ..Default::default()
        },
    );
    let flags_2 = flags_of(&engine, "這是一份");
    fmt(
        &mut engine,
        RunDirectPatch {
            underline: Some(UnderlineKind::Single),
            ..Default::default()
        },
    );
    let flags_3 = flags_of(&engine, "這是一份");
    let xml = doc_xml(&engine);
    let run = run_xml_containing(&xml, "這是一份");
    println!("=== 燈 1 實測 ===");
    println!("呼叫一 flags = {flags_1}");
    println!("呼叫二 flags = {flags_2}（應為 3）");
    println!("呼叫三 flags = {flags_3}（應為 7）");
    println!("受標 run XML = {run}");

    assert_eq!(
        flags_2,
        FLAG_BOLD | FLAG_ITALIC,
        "呼叫二（單欄 italic）後粗體旗標必須存活——實測 flags={flags_2} 代表 italic 呼叫把 bold 覆蓋了"
    );
    assert_eq!(
        flags_3,
        FLAG_BOLD | FLAG_ITALIC | FLAG_UNDERLINE,
        "呼叫三（單欄 underline）後粗體＋斜體旗標必須存活——實測 flags={flags_3} 代表後筆呼叫抹掉了前筆宣告"
    );
    assert!(
        run.contains("<w:b") && run.contains("<w:i") && run.contains("<w:u"),
        "受標 run 的 rPr 必須同時含 w:b、w:i、w:u 三宣告，實測 = {run}"
    );
}

/// 燈 2：`None` 欄位＝繼承既有宣告——run 原自帶 `w:b`，單欄 italic 呼叫不得動它。
#[test]
fn e1_lamp2_none_field_inherits_existing_declaration() {
    let mut engine = engine_from(&plain_para("<w:b/>", "這是一份受標文字"));

    fmt(
        &mut engine,
        RunDirectPatch {
            italic: Some(true),
            ..Default::default()
        },
    );
    let run = run_xml_containing(&doc_xml(&engine), "這是一份");
    println!("=== 燈 2 實測 ===\n受標 run XML = {run}");

    assert!(
        run.contains("<w:b/>"),
        "補丁 bold=None 時既有 `<w:b/>` 必須原樣保留（None＝不碰 XML），實測 = {run}"
    );
    assert!(
        run.contains("<w:i/>"),
        "補丁 italic=Some(true) 必須寫入 `<w:i/>`，實測 = {run}"
    );
    assert_eq!(
        flags_of(&engine, "這是一份") & (FLAG_BOLD | FLAG_ITALIC),
        FLAG_BOLD | FLAG_ITALIC,
        "發射面必須同時讀到既有粗體與新寫斜體"
    );
}

/// 燈 3：`Some` 同族覆寫仍須精確——覆寫新值、舊宣告不殘留重複。
#[test]
fn e1_lamp3_some_field_overwrites_same_family_without_dupes() {
    let mut engine = engine_from(&plain_para(
        r#"<w:b/><w:sz w:val="22"/>"#,
        "這是一份受標文字",
    ));

    fmt(
        &mut engine,
        RunDirectPatch {
            size_pt: Some(14.0),
            bold: Some(false),
            ..Default::default()
        },
    );
    let run = run_xml_containing(&doc_xml(&engine), "這是一份");
    println!("=== 燈 3 實測 ===\n受標 run XML = {run}");

    assert!(
        run.contains(r#"<w:sz w:val="28"/>"#),
        "size_pt=Some(14.0) 必須覆寫成半磅 28，實測 = {run}"
    );
    assert!(
        !run.contains(r#"w:val="22""#),
        "同族舊宣告 `w:sz w:val=\"22\"` 必須被移除，實測 = {run}"
    );
    assert!(
        run.contains(r#"<w:b w:val="0"/>"#),
        "bold=Some(false) 必須寫顯式關閉形態，實測 = {run}"
    );
    assert_eq!(
        run.matches("<w:sz").count(),
        1,
        "同族不得殘留重複宣告（應恰為 1 條 w:sz），實測 = {run}"
    );
    assert_eq!(
        run.matches("<w:b").count(),
        1,
        "同族不得殘留重複宣告（應恰為 1 條 w:b），實測 = {run}"
    );
}

/// 燈 4：明確關閉宣告是狀態，不是「無宣告」——後續單欄呼叫不得順手抹除。
#[test]
fn e1_lamp4_explicit_off_survives_later_single_field_calls() {
    let mut engine = engine_from(&plain_para("", "這是一份受標文字"));

    fmt(
        &mut engine,
        RunDirectPatch {
            bold: Some(false),
            ..Default::default()
        },
    );
    fmt(
        &mut engine,
        RunDirectPatch {
            italic: Some(true),
            ..Default::default()
        },
    );
    let run = run_xml_containing(&doc_xml(&engine), "這是一份");
    println!("=== 燈 4 實測 ===\n受標 run XML = {run}");

    assert!(
        run.contains(r#"<w:b w:val="0"/>"#),
        "第二筆呼叫（bold=None）必須保留第一筆的 `<w:b w:val=\"0\"/>` 明確關閉宣告，實測 = {run}"
    );
}

/// 燈 5：零意圖硬閂——全 `None` 補丁整檔 byte-identical（回歸鎖）。
///
/// 口徑同 k29e 燈 2：**整段全選**（選取後段為空、不產生 run 裂解），全
/// `None` patch 的合法產物就是 part 逐位元組原樣。破鎖形態＝空
/// `<w:rPr></w:rPr>` 被材料化，或既有 rPr 被重排。
#[test]
fn e1_lamp5_empty_patch_is_byte_identical() {
    let runs = plain_para(r#"<w:b/><w:i/>"#, "這是一份受標文字");
    let bytes = docx_with_document_xml(&body_wrap(&runs));
    let mut engine = Engine::from_docx_bytes(&bytes).expect("開啟失敗");
    settle(&mut engine);
    let before = engine.save_docx().expect("存檔失敗");

    engine
        .apply_mutation(Mutation::FormatTextRange {
            range: DocRange {
                target: SubDocTarget::Body,
                start: DocPos { para: 1, cp: 0 },
                end: DocPos { para: 1, cp: 8 },
            },
            patch: RunDirectPatch::default(),
        })
        .expect("套用 M2.1 FormatTextRange 失敗");
    settle(&mut engine);
    let after = engine.save_docx().expect("存檔失敗");

    let before_xml = doc_xml(&engine);
    assert!(
        before_xml.contains("<w:rPr><w:b/><w:i/></w:rPr>"),
        "全 None 補丁後既有 rPr 宣告必須逐字原樣（既有硬閂口徑），實測 = {before_xml}"
    );
    assert_eq!(
        before, after,
        "全 None 補丁（零意圖）必須零位元組位移——既有硬閂不得被欄位級合併改動波及"
    );
}

/// 燈 6：存檔重開閉環——燈 1 的三宣告經存檔＋重解析後仍讀得到。
#[test]
fn e1_lamp6_accumulated_declarations_survive_save_and_reopen() {
    let mut engine = engine_from(&plain_para("", "這是一份受標文字"));
    for patch in [
        RunDirectPatch {
            bold: Some(true),
            ..Default::default()
        },
        RunDirectPatch {
            italic: Some(true),
            ..Default::default()
        },
        RunDirectPatch {
            underline: Some(UnderlineKind::Single),
            ..Default::default()
        },
    ] {
        fmt(&mut engine, patch);
    }

    let saved = engine.save_docx().expect("存檔失敗");
    let mut reopened = Engine::from_docx_bytes(&saved).expect("重開失敗");
    settle(&mut reopened);
    let flags = flags_of(&reopened, "這是一份");
    println!("=== 燈 6 實測 ===\n重開後 flags = {flags}（應為 7）");
    assert_eq!(
        flags & (FLAG_BOLD | FLAG_ITALIC | FLAG_UNDERLINE),
        FLAG_BOLD | FLAG_ITALIC | FLAG_UNDERLINE,
        "存檔重開後三宣告都必須被解析層讀回（flags 應含 1|2|4），實測 = {flags}"
    );
}

// ── ZIP 組裝工具（與 d6_table_borders.rs 同款口徑：store 模式＋最小中央目錄）──

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

/// 燈 7（同族前綴碰撞）：`w:bCs`／`w:iCs`／`w:szCs` 與 `w:b`／`w:i`／`w:sz`
/// 共享字串前綴。欄位級合併讓既有宣告真正進入重組路徑後，`remove_element`
/// 若在同名前綴的假配對上放棄整輪掃描，舊宣告會殘留成重複宣告（覆寫失效）。
/// 本燈釘死「覆寫一定贏」＋「同族不重複」＋「schema 序不亂」。
#[test]
fn e1_lamp7_same_prefix_complex_script_declarations_survive_overwrite() {
    // 刻意把 bCs 放在 b 之前（非 schema 序）——真文書偶見此形態，且它正是
    // 前綴掃描會先撞上假配對的排列。
    let mut engine = engine_from(&plain_para(
        r#"<w:bCs/><w:b/><w:iCs/><w:i/><w:szCs w:val="22"/><w:sz w:val="22"/>"#,
        "這是一份受標文字",
    ));

    fmt(
        &mut engine,
        RunDirectPatch {
            bold: Some(false),
            italic: Some(true),
            size_pt: Some(14.0),
            ..Default::default()
        },
    );
    let run = run_xml_containing(&doc_xml(&engine), "這是一份");
    println!("=== 燈 7 實測 ===\n受標 run XML = {run}");

    assert_eq!(
        run.matches("<w:b ").count() + run.matches("<w:b/>").count(),
        1,
        "w:b 必須恰為 1 條（覆寫不得殘留舊宣告），實測 = {run}"
    );
    assert_eq!(
        run.matches("<w:i ").count() + run.matches("<w:i/>").count(),
        1,
        "w:i 必須恰為 1 條，實測 = {run}"
    );
    assert_eq!(
        run.matches("<w:sz ").count(),
        1,
        "w:sz 必須恰為 1 條（w:szCs 不計入同名比對），實測 = {run}"
    );
    // 複雜文字同族（未被補丁觸碰）必須存活。
    assert!(
        run.contains("<w:bCs/>")
            && run.contains("<w:iCs/>")
            && run.contains(r#"<w:szCs w:val="22"/>"#),
        "w:bCs／w:iCs／w:szCs 未被補丁觸碰，必須逐字原樣保留，實測 = {run}"
    );
    // CT_RPr 宣告序：b(2) → bCs(3) → i(4) → iCs(5) → sz(23) → szCs(24)。
    let pos = |needle: &str| {
        run.find(needle)
            .unwrap_or_else(|| panic!("應含 {needle}：{run}"))
    };
    assert!(
        pos("<w:b ") < pos("<w:bCs")
            && pos("<w:bCs") < pos("<w:i/>")
            && pos("<w:i/>") < pos("<w:iCs"),
        "CT_RPr 宣告序必須維持 b→bCs→i→iCs，實測 = {run}"
    );
    assert!(
        pos("<w:sz ") < pos("<w:szCs"),
        "CT_RPr 宣告序必須維持 sz→szCs，實測 = {run}"
    );
}

/// 燈 8（巢裝 rPr 截斷）：`w:rPrChange`（追蹤修訂）本身內含一顆巢裝
/// `w:rPr`（語料 n830205.docx 實測 589 處）。`leading_element` 若按「首個
/// `</w:rPr>`」收尾，會在**內層**閉標籤截斷，拿到的 rPr 本身就少掉尾段閉標籤。
/// 欄位級合併前這被「宣告全數丟掉」掩蓋（輸出永遠是乾淨的 `<w:rPr>{add}`）；
/// 合併落地後同一段截斷會變成**不平衡 XML**（兩個開標籤只配一個閉標籤），
/// 屬本次修正直接引入的新風險，故必須連同 `leading_element` 一起修成深度計數。
#[test]
fn e1_lamp8_nested_rpr_change_survives_as_balanced_xml() {
    let inner = r#"<w:rPr><w:b/><w:rPrChange w:id="8" w:author="王小明"><w:rPr><w:i/></w:rPr></w:rPrChange></w:rPr>"#;
    let mut engine = engine_from(&format!(
        "<w:p><w:r>{inner}<w:t>這是一份受標文字</w:t></w:r></w:p>"
    ));

    fmt(
        &mut engine,
        RunDirectPatch {
            underline: Some(UnderlineKind::Single),
            ..Default::default()
        },
    );
    let run = run_xml_containing(&doc_xml(&engine), "這是一份");
    println!("=== 燈 8 實測 ===\n受標 run XML = {run}");

    // 平衡性：rPr 開標籤數必須等於閉標籤數（不平衡＝下游解析器直接報錯）。
    assert_eq!(
        run.matches("<w:rPr>").count(),
        run.matches("</w:rPr>").count(),
        "w:rPr 開閉標籤必須平衡（巢裝 rPrChange 不得被截斷），實測 = {run}"
    );
    assert_eq!(
        run.matches("<w:rPrChange").count(),
        run.matches("</w:rPrChange>").count(),
        "w:rPrChange 開閉標籤必須平衡，實測 = {run}"
    );
    // 語意面：未被觸碰的粗體與追蹤修訂標記都必須存活。
    assert!(
        run.contains("<w:b/>"),
        "既有 `<w:b/>` 必須存活，實測 = {run}"
    );
    assert!(
        run.contains(r#"<w:u w:val="single"/>"#),
        "補丁的 `<w:u w:val=\"single\"/>` 必須寫入，實測 = {run}"
    );
    assert!(
        run.contains("w:author=\"王小明\""),
        "w:rPrChange 作者屬性必須存活（不得被截斷吞掉），實測 = {run}"
    );
}
