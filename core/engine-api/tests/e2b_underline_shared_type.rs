//! E-2b 紅燈考卷（共型收尾）：寫入端與解析/發射端**同一型別**＋serde 線契約遷移。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 卡面缺口（E-2 報告 §6.1 → 本卡）
//! E-2 把解析層與發射層統一為 `quire_render_trait::Underline`（單一真值源），
//! 寫入端 `RunDirectPatch::underline` 卻仍是 mutation.rs 自造的鏡像枚舉
//! `UnderlineKind`（五值，serde 形＝變體名 `Single|Double|Dotted|Dashed|None`）。
//! 兩枚舉語義等價但型別不同——口徑閉環只靠燈 6 的寫讀round-trip 鎖住，編譯期
//! 無法阻止再次分岔（新形態 `Thick` 就差點只進解析端、寫入端漏接）。
//!
//! # 本卡共型定案
//! - `UnderlineKind` ＝ `quire_render_trait::Underline` 的**別名**（同一型別，
//!   非鏡像）：`pub use ... as UnderlineKind`。既有 import 路徑零斷裂，但編譯期
//!   構造器風暴把殘留的舊態用法全部逼出水面（舊 `None` 變體 → `Off`）。
//! - `Raw(_)` 寫入端**誠實拒絕**：寫入端無從替用戶編造未支援形態，且 Raw 字面
//!   直接插值進 `w:val` 屬未驗證字串注入面——引擎在 Mutation 入口閘門回
//!   `XmlPatchRejected`，**零位元組位移**（拒絕不得留下半套寫入）。
//! - serde 遷移期**雙讀**：canonical 寫出走 `Underline` 小寫字面
//!   （`"single"…"off"`，`Raw` 外部標籤 `{"raw":"wavy"}`），舊態大寫變體名
//!   （`"Single"…"None"`）經 serde alias 繼續可反序列化——舊外殼 pkg 未同步
//!   重建不得炸。注意兩套字面口徑不同家：serde 是殼↔引擎契約（`Off`→`"off"`），
//!   `w:u/@w:val` 是 OOXML 檔契約（`Off`→`"none"`，ECMA 正字）。
//!
//! # 紅燈口徑（先紅後綠）
//! 修前本卷**編譯失敗即紅**：`patch.underline` 型別是鏡像 `UnderlineKind`，
//! 餵 `Underline::Thick` 型別不符（E0308）；`UnderlineKind::Thick` 不存在
//! （E0599）。共型落地後全綠。BUGGY 回塞鎖（`E2B_UNDERLINE_BUGGY=1`，
//! scripts/e2b_underline_buggy_replay.sh 隨貨）把寫入端塞回「舊五值＋Raw 靜默
//! 降級單線」語義——燈 2／燈 6 鑑別 Thick 折損、燈 3 鑑別 Raw 誠實拒失效。

use quire_engine::{
    DocPos, DocRange, DrawItem, Engine, Mutation, MutationError, RunDirectPatch, SubDocTarget,
    Underline, UnderlineKind,
};

/// 手寫 CRC32（zip 表式多項式；測試間零共用鐵律，e2 同款語義抄錄）。
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

/// 合成 docx（ZipBuilder＝e2_underline_kind／e2_underline_emit 同款，
/// 測試間零共用鐵律：各卷自帶副本，不引用他卷模組）。
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

/// 兩段式合成檔：目標段（無底線，寫入標的）＋對照段（永不碰，零位移鑑別用）。
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
<Relationships xmlns="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
    );
    let doc = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>底線目標段ABCDE</w:t></w:r></w:p>
    <w:p><w:r><w:t>對照段FGHIJ</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    b.add_stored(b"word/document.xml", doc.as_bytes());
    b.finish()
}

fn fixture_engine() -> Engine {
    let mut engine = Engine::from_docx_bytes(&fixture_bytes()).expect("開啟合成檔失敗");
    settle(&mut engine);
    engine
}

fn document_xml(engine: &Engine) -> Vec<u8> {
    engine
        .part("word/document.xml")
        .expect("必須有 document.xml")
        .to_vec()
}

/// 顯示列表裡第一條含 `needle` 的 Text 項的 `underline` 欄（發射端形態證據）。
fn emitted_underline(engine: &Engine, needle: &str) -> Option<Underline> {
    let dl = engine
        .page_display_list(0)
        .expect("第 0 頁顯示列表必須存在");
    dl.items
        .iter()
        .find_map(|it| match it {
            DrawItem::Text {
                text, underline, ..
            } if text.contains(needle) => Some(underline.clone()),
            _ => None,
        })
        .expect("目標段必須在顯示列表")
}

/// 對第 0 段前 5 碼點套用底線寫入（共型後唯一合法路徑：直接餵 `Underline`）。
fn apply_underline(engine: &mut Engine, kind: Underline) -> Result<(), MutationError> {
    engine
        .apply_mutation(Mutation::FormatTextRange {
            range: DocRange {
                target: SubDocTarget::Body,
                start: DocPos { para: 0, cp: 0 },
                end: DocPos { para: 0, cp: 5 },
            },
            patch: RunDirectPatch {
                underline: Some(kind),
                ..Default::default()
            },
        })
        .map(|_| ())
}

// ───────────────────────────── 燈 1：編譯期共型 ─────────────────────────────

/// 燈 1（卡面任務 3：編譯期防再次分岔）：寫入端欄位與解析/發射端**同一型別**。
/// 本燈的斷言幾乎全是「能編過」——把 `Underline` 直接餵進
/// `RunDirectPatch::underline`、把 `Underline` 直接餵進顯示列表欄位比對，鏡像
/// 枚舉態必 E0308。`UnderlineKind` 別名與 `Underline` 同型（互相指定必過）＝
/// 舊 import 路徑零斷裂的機械證明。`as_u_val` 值域逐值鎖死（含新增 Thick，
/// Raw→None＝寫入端無此字面）。
#[test]
fn e2b_lamp1_write_read_shared_type_at_compile_time() {
    // 共型的機械證明：同一個 `Underline` 值在三端之間自由流通。
    let u = Underline::Thick;
    let patch = RunDirectPatch {
        underline: Some(u.clone()),
        ..Default::default()
    };
    assert_eq!(
        patch.underline.as_ref(),
        Some(&u),
        "寫入端欄位必須吃 Underline"
    );
    // 別名＝同型：兩個名字互相指定在鏡像枚舉態必 E0308。
    let alias: UnderlineKind = Underline::Double;
    let canonical: Underline = alias;
    assert_eq!(
        canonical,
        Underline::Double,
        "UnderlineKind 必須是 Underline 別名"
    );
    // `w:u/@w:val` 檔契約字面逐值（ECMA 正字；Off→none 非 off——serde 口徑
    // 見燈 5，兩套字面不同家，逐字鎖死不許互竄）。
    let cases: [(Underline, &str); 6] = [
        (Underline::Single, "single"),
        (Underline::Double, "double"),
        (Underline::Thick, "thick"),
        (Underline::Dotted, "dotted"),
        (Underline::Dashed, "dashed"),
        (Underline::Off, "none"),
    ];
    for (kind, want) in cases {
        assert_eq!(kind.as_u_val(), Some(want), "{kind:?} 的檔契約字面");
    }
    // Raw 在寫入端無字面（拒絕語義的型別層出口，引擎端點見燈 3）。
    assert_eq!(
        Underline::Raw("wavy".to_string()).as_u_val(),
        None,
        "Raw 不得有 w:val 字面——寫入端無從替用戶編造未支援形態"
    );
}

// ──────────────────────── 燈 2：Thick 寫入端真通 ────────────────────────

/// 燈 2（共型紅利）：`Thick` 是解析端（E-2 corpus 實見×5）就有、鏡像枚舉卻漏接
/// 的形態——共型後寫入端必須真通：寫入→存檔件 `<w:u w:val="thick"/>`→重解析
/// 讀回 `Thick`→發射欄 `Thick`（四環逐值，任何一環把 Thick 折成 Single 都紅）。
#[test]
fn e2b_lamp2_thick_writes_end_to_end() {
    let mut engine = fixture_engine();
    apply_underline(&mut engine, Underline::Thick).expect("Thick 寫入必須被接受");
    settle(&mut engine);
    let saved = engine.save_docx().expect("存檔失敗");
    let reopened = Engine::from_docx_bytes(&saved).expect("重開失敗");
    let xml = String::from_utf8(document_xml(&reopened)).expect("UTF-8");
    assert!(
        xml.contains(r#"<w:u w:val="thick"/>"#),
        "共型後 Thick 必須寫出 thick 字面（鏡像枚舉態根本無此變體）；片段：{}",
        &xml[..xml.len().min(600)]
    );
    assert_eq!(
        emitted_underline(&reopened, "底線目標段"),
        Some(Underline::Thick),
        "Thick 經存檔重解析發射必須仍是 Thick（禁折單線）"
    );
}

// ──────────────────── 燈 3：Raw 寫入端誠實拒絕＋零位移 ────────────────────

/// 燈 3（卡面任務 1：Raw 語義老實定案）：寫入端拒絕 `Raw(_)` 並誠實報錯。
/// 三層證據：(a) `FormatTextRange` 回 `XmlPatchRejected`；(b) 拒絕＝零位元組
/// 位移（`document.xml` 與未動前逐位元組相同，不得留半套寫入）；(c)
/// `UpsertNamedStyle` 臂同款拒絕（patch_rpr 共用的寫入面不得有旁路）。
/// 對照組：Raw **可**經 serde 反序列化（線契約不吞值，見燈 4/7）——拒絕發生在
/// 引擎套用層，兩層語義不得混同。
#[test]
fn e2b_lamp3_raw_write_honestly_rejected() {
    // (a)+(b) FormatTextRange 臂。
    let mut engine = fixture_engine();
    let before = document_xml(&engine);
    let err = apply_underline(&mut engine, Underline::Raw("wavy".to_string()))
        .expect_err("Raw 寫入必須被誠實拒絕（引擎無從編造未支援形態）");
    assert!(
        matches!(err, MutationError::XmlPatchRejected { .. }),
        "Raw 拒絕必須是 XmlPatchRejected（誠實報錯，非 Engine 黑話），實得：{err:?}"
    );
    assert!(
        err.to_string().contains("wavy") || err.to_string().contains("Raw"),
        "拒絕訊息必須指名被拒形態（老實告知，禁太魯閣式『操作失敗』），實得：{err}"
    );
    assert_eq!(
        document_xml(&engine),
        before,
        "拒絕必須零位元組位移（不得先寫一半再反悔）"
    );
    // 拒絕後引擎仍健康：合法形態照寫（拒絕不是熔斷）。
    apply_underline(&mut engine, Underline::Dashed).expect("拒絕 Raw 後合法寫入必須照常");

    // (c) UpsertNamedStyle 臂（patch_rpr 第二個呼叫端，閘門不得只裝一處）。
    let mut engine = fixture_engine();
    let err = engine
        .apply_mutation(Mutation::UpsertNamedStyle {
            style_id: "E2bRawStyle".to_string(),
            style_type: quire_engine::StyleType::Character,
            based_on: None,
            para_patch: None,
            run_patch: Some(RunDirectPatch {
                underline: Some(Underline::Raw("words".to_string())),
                ..Default::default()
            }),
        })
        .expect_err("UpsertNamedStyle 的 Raw 底線必須同款拒絕");
    assert!(
        matches!(err, MutationError::XmlPatchRejected { .. }),
        "樣式臂 Raw 拒絕同款 XmlPatchRejected，實得：{err:?}"
    );
}

// ────────────────── 燈 4：舊態 JSON 反序列化相容（遷移期雙讀） ──────────────────

/// 燈 4（PM 發車紀律 3(b)：舊外殼 pkg 未同步重建不得炸）：共型把 serde 形從
/// 變體名（`"Single"…"None"`）換成 canonical 小寫（`"single"…"off"`），舊態
/// JSON 必須經 serde alias **繼續可反序列化**，且逐值對映老實：
/// `"None"`（舊移除值）→ `Off`，不得單邊沉默丟值（None≠未宣告欄位語義）。
/// `Mutation::FormatTextRange` 整件舊態 payload（外殼實際發送的形狀）同款相容。
#[test]
fn e2b_lamp4_old_json_shape_still_deserializes() {
    // 逐值舊態：五個大寫變體名一個都不許炸。
    let old_cases: [(&str, Option<Underline>); 5] = [
        (r#""Single""#, Some(Underline::Single)),
        (r#""Double""#, Some(Underline::Double)),
        (r#""Dotted""#, Some(Underline::Dotted)),
        (r#""Dashed""#, Some(Underline::Dashed)),
        (r#""None""#, Some(Underline::Off)),
    ];
    for (json, want) in old_cases {
        let patch: RunDirectPatch = serde_json::from_str(&format!(r#"{{"underline":{json}}}"#))
            .unwrap_or_else(|e| panic!("舊態 underline={json} 反序列化不得炸：{e}"));
        assert_eq!(patch.underline, want, "舊態 {json} 的對映");
    }
    // 整件 Mutation payload 舊態（舊外殼 pkg 原樣發送的形狀；U4b 探針同款範圍）。
    let old_mutation = r#"{
        "FormatTextRange": {
            "range": {"target": "Body", "start": {"para": 0, "cp": 0}, "end": {"para": 0, "cp": 5}},
            "patch": {"bold": null, "italic": null, "color_rgb": null, "size_pt": null,
                      "char_style": null, "underline": "Dotted", "strike": null,
                      "vert_align": null, "highlight": null, "font_family": null}
        }
    }"#;
    let op: Mutation = serde_json::from_str(old_mutation)
        .expect("舊外殼整件 payload 必須繼續可反序列化（遷移期雙讀）");
    match op {
        Mutation::FormatTextRange { patch, .. } => {
            assert_eq!(patch.underline, Some(Underline::Dotted));
        }
        other => panic!("反序列化結果必須是 FormatTextRange，實得 {other:?}"),
    }
}

// ──────────────────── 燈 5：serde canonical 新形鎖死 ────────────────────

/// 燈 5（卡面任務 2：破向後兼容要顯式記帳＋考卷鎖新形）：canonical 序列化形
/// 逐字鎖死——小寫字面（與解析端 `Underline` 現有 serde 形**逐字相同**，共型
/// 的定義性後果）、`Off`→`"off"`（注意：serde 契約是 off，`w:u/@w:val` 檔契約
/// 是 none，燈 1 已鎖，兩套字面禁互竄）、`Raw` 外部標籤 `{"raw":"wavy"}`。
/// 破的是「寫入端欄位序列化輸出大寫變體名」這條舊線——記帳：外殼
/// engine-adapter.js 同步改 canonical＋雙讀 alias，舊形由燈 4 保證讀得進。
#[test]
fn e2b_lamp5_serde_canonical_form_locked() {
    let cases: [(Underline, &str); 6] = [
        (Underline::Single, r#""single""#),
        (Underline::Double, r#""double""#),
        (Underline::Thick, r#""thick""#),
        (Underline::Dotted, r#""dotted""#),
        (Underline::Dashed, r#""dashed""#),
        (Underline::Off, r#""off""#),
    ];
    for (kind, want_json) in cases {
        let patch = RunDirectPatch {
            underline: Some(kind.clone()),
            ..Default::default()
        };
        let json = serde_json::to_string(&patch).expect("序列化");
        assert!(
            json.contains(&format!(r#""underline":{want_json}"#)),
            "{kind:?} 的 canonical 序列化必須含 \"underline\":{want_json}，實得：{json}"
        );
    }
    let raw = RunDirectPatch {
        underline: Some(Underline::Raw("wavy".to_string())),
        ..Default::default()
    };
    let json = serde_json::to_string(&raw).expect("Raw 序列化");
    assert!(
        json.contains(r#""underline":{"raw":"wavy"}"#),
        "Raw 必須走外部標籤形保字面（線契約不吞值；引擎拒絕收是燈 3 的層），實得：{json}"
    );
    // 舊大寫形不得再是序列化輸出（canonical 唯一性：序列化若仍吐 'Single'，
    // 新外殼＋舊引擎就分岔——雙讀是讀端的寬容，不是寫端的權利）。
    let json = serde_json::to_string(&RunDirectPatch {
        underline: Some(Underline::Single),
        ..Default::default()
    })
    .expect("序列化");
    assert!(
        !json.contains("Single"),
        "序列化輸出不得再含舊大寫變體名：{json}"
    );
}

// ────────────────── 燈 6：canonical 全集寫讀閉環（燈 6 擴全集） ──────────────────

/// 燈 6（PM 發車紀律 3(a)：E-2 燈 6 雛形擴全集）：canonical 六值
/// single/double/thick/dotted/dashed/off **逐值**全走「寫入→存檔→重解析→發射」
/// 閉環，發射欄與寫入值**同型同值**（共型後這是編譯期就成立的斷言，運行期鎖
/// 的是 `w:val` 檔契約字面與重解析歸一——`off`→`none` 字面折算就發生在本閉環）。
/// 對照段全程零位移（未選取段不得被波及）。
#[test]
fn e2b_lamp6_canonical_full_set_roundtrip() {
    let cases: [(Underline, &str); 6] = [
        (Underline::Single, "single"),
        (Underline::Double, "double"),
        (Underline::Thick, "thick"),
        (Underline::Dotted, "dotted"),
        (Underline::Dashed, "dashed"),
        (Underline::Off, "none"),
    ];
    for (kind, want_val) in cases {
        let mut engine = fixture_engine();
        apply_underline(&mut engine, kind.clone()).expect("canonical 值寫入必須被接受");
        settle(&mut engine);
        let saved = engine.save_docx().expect("存檔失敗");
        let reopened = Engine::from_docx_bytes(&saved).expect("重開失敗");
        let xml = String::from_utf8(document_xml(&reopened)).expect("UTF-8");
        let tag = format!(r#"<w:u w:val="{want_val}"/>"#);
        assert!(xml.contains(&tag), "寫入端 {kind:?} 必須寫出 {tag}");
        let emitted = emitted_underline(&reopened, "底線目標段");
        if kind.on() {
            assert_eq!(
                emitted,
                Some(kind.clone()),
                "{kind:?} 經存檔重解析發射必須同值（口徑閉環，禁折損）"
            );
        } else {
            // Off（寫出 none）＝明確關閉：發射端無底線（None＝級聯未宣告態，
            // 本合成件無樣式層宣告，Off 與未宣告在顯示列表同落 None——E-2 口徑）。
            assert_eq!(emitted, None, "Off 必須熄滅底線（顯示列表 None）");
        }
        // 對照段零波及：目標段之外的段落 XML 切片不得被改寫（FormatTextRange
        // 只重組選取段，未選取段逐位元組原樣——圓環零位移的段級鏡像）。
        assert!(
            xml.contains(r#"<w:p><w:r><w:t>對照段FGHIJ</w:t></w:r></w:p>"#),
            "對照段必須逐位元組原樣（{kind:?} 寫入不得越段波及）；片段：{}",
            &xml[..xml.len().min(600)]
        );
    }
}

// ──────────────────── 燈 7：線契約不吞值＋拒絕在套用層 ────────────────────

/// 燈 7（遷移期「不得單邊沉默丟值」）：`Raw` 在 serde 邊界**讀得進、寫得出、
/// 存得回**（線契約層零吞值），拒絕只發生在引擎套用層（燈 3）——兩層分界鎖死。
/// 反向鎖：非法字面（`"wavy"` 裸字串，非 Raw 標籤形）必須炸錯，不得被 alias
/// 寬容吞成某個形態（雙讀只赦免既成事實的舊態大寫名，不發明新常態）。
#[test]
fn e2b_lamp7_wire_contract_does_not_swallow() {
    // Raw 標籤形讀得進（燈 3 的 serde 前置：能讀進才有「誠實拒絕」可談）。
    let patch: RunDirectPatch = serde_json::from_str(r#"{"underline":{"raw":"wavy"}}"#)
        .expect("Raw 外部標籤形必須可反序列化");
    assert_eq!(patch.underline, Some(Underline::Raw("wavy".to_string())));
    // 寫得出、存得回（序列化→反序列化逐字保形）。
    let json = serde_json::to_string(&patch).expect("序列化");
    let back: RunDirectPatch = serde_json::from_str(&json).expect("Round-trip 反序列化");
    assert_eq!(back, patch, "Raw 經 serde round-trip 必須逐字保形");
    // 非法裸字串必炸：alias 表是封閉的，未列字面不得憑空對映。
    for bad in [r#""wavy""#, r#""words""#, r#""thick ""#, r#""SINGLE""#] {
        let r = serde_json::from_str::<RunDirectPatch>(&format!(r#"{{"underline":{bad}}}"#));
        assert!(r.is_err(), "非法底線字面 {bad} 必須炸錯，不得沉默吞值");
    }
}
