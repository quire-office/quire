//! E 返修卡 t_c1dbc6d7 **S2-1 紅燈考卷**：`w:sdt`／`w:ins`／`w:del` 透明包裝內的
//! 巢狀表 → 游標反查路徑與 mutation 側口徑失配（界內錯位、靜默改錯表）。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 缺口（agy 獨立審查＋PM 讀碼＋corpus 實測坐實）
//! - `core/ooxml/src/table.rs` 的 `tbl_parent()`／`under()` **穿透**透明包裝
//!   （`Frame::Opaque`／`Skip`），把包裝**內**的表掛進 canonical 樹並計入
//!   `root_slot`。
//! - `core/engine-api/src/mutation_k29c.rs::locate_table` 走
//!   `direct_children(text, body, "tbl")`——**不穿透**任何包裝。
//! - 兩套口徑不同 ⇒ 被包裝節點之後只要還有 ≥1 個未包裝的直接兄弟，反查座標就
//!   **錯位且仍落在界內**（`Vec::get(idx)` 只擋越界、**完全無身分驗證**）——
//!   不報錯，直接靜默改錯／刪錯下一張表、列或格。
//!
//! # 斷言層口徑（本卡的鑑別核心）
//! 反查產物原封不動餵 M4（零第二輪翻譯），再**用 canonical 樹回讀「哪張表真的
//! 被改了」**（`commit_k29c` 必經 `rebuild_derived()` 重解析，故 `tables_json`
//! 就是編輯後的真相）：
//!   * 命中且改對 ⇒ `Ok` 且「被改表集合 == 反查宣稱的那一張」。
//!   * 界內錯位（缺陷態）⇒ `Ok` 但被改的是**另一張**表 → 本考卷紅。
//!   * 誠實拒絕（修後態）⇒ `Err(InvalidTableOperation)`，且所有表的 raw XML 零變化。
//! 三種結果只准出現第一與第三種；第二種就是 S2-1 的死亡形態，由燈 2 封死。
//!
//! # fixture
//! `fixtures/sdt_wrapped_table.docx`（本卡新產，`gen_sdt_wrapped_table_fixture.py`）：
//! body 依序＝`w:sdt` 包表 T_A（canonical root_slot 0）→ 裸表 T_B（root_slot 1）。
//! 燈 1 先以**檔內原始 XML 掃描**自證「錯位源確實在檔」（raw 直屬子節點只看得到
//! 一張 tbl，canonical 卻有兩張）——上游改掉包裝形態時該燈先紅，不會假綠。
//!
//! # 語料面
//! 燈 6 對 `corpus/` 的透明包裝語料檔（PM 實測 13 檔含包裝內表；本卡掃描坐實
//! 7 檔在場清單）逐檔、逐根表做同上三態判定。語料檔不在本卡白名單，缺檔→掛帳不紅。
//!
//! # 鑑別力證明
//! `E_SDT_TABLE_BUGGY=1` 把燈 7 的期望集合換成不可能值 ⇒ 該燈必紅，證明斷言真在
//! 比對「哪張表被改」而非恆真（實跑兩態記 docs）。

use quire_engine::{Engine, Mutation, SubDocTarget, TablePath, TablePropsPatch};
use serde_json::Value;

const SDT_FIXTURE: &str = "fixtures/sdt_wrapped_table.docx";

/// 透明包裝內含表的語料檔（掃描 `corpus/manifest.json` 全數檔案所得）。
const CORPUS_WRAPPED: &[&str] = &[
    "corpus/ooxmlexport/tdf106724.docx",
    "corpus/ooxmlexport/tdf114882.docx",
    "corpus/ooxmlexport/table-start-2-sdt.docx",
    "corpus/poi/test-data_document_Bug54771a.docx",
    "corpus/poi/test-data_document_Bug54849.docx",
    "corpus/xopt/TestFiles_HC031-Complicated-Document.docx",
    "corpus/xopt/NewDocxDocuments_ContentControlsNested.docx",
];

fn open(rel: &str) -> Engine {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../").to_string() + rel;
    let bytes = std::fs::read(&full).unwrap_or_else(|e| panic!("讀取 {full} 失敗: {e}"));
    Engine::from_docx_bytes(&bytes).unwrap_or_else(|e| panic!("開啟 {rel} 失敗: {e}"))
}

/// 讀語料檔：缺檔回 `None` 讓呼叫端掛帳（語料不在本卡白名單、禁改；缺檔不得
/// 讓考卷紅——但燈 6 的「在場檔數下限」會擋住「語料全不在＝本燈空轉」的假綠）。
fn corpus_rel(rel: &str) -> Option<Vec<u8>> {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../").to_string() + rel;
    match std::fs::read(&p) {
        Ok(b) => Some(b),
        Err(_) => {
            eprintln!("掛帳：語料檔不在場 {rel}（corpus 不在本卡白名單，非缺陷）");
            None
        }
    }
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

/// 反查：段落 → 表格路徑 JSON（表外段＝None）。
fn locate(e: &Engine, para: usize) -> Option<Value> {
    e.table_locator_json(para).map(json)
}

/// 由反查 JSON 還原 `TablePath`（零第二輪翻譯：欄位名逐字吃）。
fn path_of(hit: &Value) -> TablePath {
    TablePath {
        target: SubDocTarget::Body,
        root_slot: hit["table_path"]["root_slot"].as_u64().expect("root_slot") as usize,
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

/// 對 `path` 那張表發 `SetTableProps{alignment:"center"}`（M4.6）。
fn set_center(e: &mut Engine, path: TablePath) -> Result<(), String> {
    e.apply_mutation(Mutation::SetTableProps {
        table: path,
        patch: TablePropsPatch {
            alignment: Some("center".to_string()),
            ..Default::default()
        },
    })
    .map(|_| ())
    .map_err(|err| format!("{err:?}"))
}

/// 節點（含其巢狀子表）任一處带 `props.justify == "center"`。
fn subtree_centered(node: &Value) -> bool {
    if node["props"]["justify"].as_str() == Some("center") {
        return true;
    }
    node["rows"]
        .as_array()
        .map(|rows| {
            rows.iter().any(|r| {
                r["cells"].as_array().map(|cs| {
                    cs.iter().any(|c| {
                        c["blocks"].as_array().map(|bs| {
                            bs.iter().any(|b| {
                                b.as_object()
                                    .and_then(|o| o.get("Table"))
                                    .is_some_and(subtree_centered)
                            })
                        })
                        .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// 被 centre 到的根表 root_slot 清單（含巢狀子表命中＝該根表入列）。
fn centered_root_slots(e: &Engine) -> Vec<usize> {
    json(e.tables_json())
        .as_array()
        .expect("tables_json 是根表陣列")
        .iter()
        .enumerate()
        .filter(|(_, t)| subtree_centered(&t["table"]))
        .map(|(i, _)| i)
        .collect()
}

/// 取 `root_slot` 內任一可反查段落與其實反查 JSON。
fn locate_first_in_slot(e: &Engine, slot: usize) -> (usize, Value) {
    (0..e.paragraph_count())
        .filter_map(|p| locate(e, p).map(|h| (p, h)))
        .find(|(_, h)| h["table_path"]["root_slot"] == slot as u64)
        .unwrap_or_else(|| panic!("root_slot {slot} 要有可反查的段落"))
}

/// `w:body` 的**直屬** `w:tbl` 子節點數——測試側獨立實作，口徑同
/// `mutation_k29c::direct_children`（深度 0 才計、同名巢狀整段跳過）。
/// 「raw 側看到幾張表」的獨立真相來源，不借用被審碼的 helper。
fn raw_direct_body_tbl_count(xml: &str) -> usize {
    let body_open = xml.find("<w:body>").expect("要有 <w:body>") + "<w:body>".len();
    let body_close = xml.rfind("</w:body>").expect("要有 </w:body>");
    let local_of = |tag: &str| -> String {
        let raw = if tag.starts_with("</") {
            &tag[2..tag.len() - 1]
        } else if tag.trim_end().ends_with("/>") {
            &tag[1..tag.len() - 2]
        } else {
            &tag[1..tag.len() - 1]
        };
        raw.split_whitespace()
            .next()
            .unwrap_or("")
            .rsplit(':')
            .next()
            .unwrap_or("")
            .to_string()
    };
    let mut cursor = body_open;
    let mut depth = 0usize;
    let mut count = 0usize;
    while let Some(rel) = xml[cursor..body_close].find('<') {
        let lt = cursor + rel;
        let gt = xml[lt..].find('>').expect("標籤要有 '>'") + lt;
        let tag = &xml[lt..=gt];
        let closing = tag.starts_with("</");
        let empty = !closing && tag.trim_end().ends_with("/>");
        let local = local_of(tag);
        if depth == 0 && !closing && local == "tbl" {
            count += 1;
            // 整段跳過這張表（含其巢狀表），與 direct_children 同口徑。
            let mut d = 1usize;
            let mut c = gt + 1;
            while let Some(r2) = xml[c..body_close].find('<') {
                let t2 = c + r2;
                let g2 = xml[t2..].find('>').expect("標籤要有 '>'") + t2;
                let tg = &xml[t2..=g2];
                let cl = tg.starts_with("</");
                let em = !cl && tg.trim_end().ends_with("/>");
                if local_of(tg) == "tbl" {
                    if cl {
                        d -= 1;
                        if d == 0 {
                            c = g2 + 1;
                            break;
                        }
                    } else if !em {
                        d += 1;
                    }
                }
                c = g2 + 1;
            }
            cursor = c;
            continue;
        }
        if !closing && !empty {
            depth += 1;
        } else if closing {
            depth = depth.saturating_sub(1);
        }
        cursor = gt + 1;
    }
    count
}

/// 拆出「`<w:sdt>…</w:sdt>` 整段（T_A 所在）」與「sdt 之後的裸表 T_B 整段」。
/// 兩段是「誰被改」的逐字元對照基準（raw 層，不走 canonical）。
fn raw_split(xml: &str) -> (String, String) {
    let sdt_at = xml.find("<w:sdt>").expect("fixture 要有 <w:sdt>");
    let sdt_close = xml.find("</w:sdt>").expect("要有 </w:sdt>") + "</w:sdt>".len();
    let b_at = sdt_close
        + xml[sdt_close..]
            .find("<w:tbl>")
            .expect("sdt 之後要有裸表 T_B");
    let b_end = b_at
        + xml[b_at..]
            .find("</w:tbl>")
            .expect("T_B 要有閉標籤")
            + "</w:tbl>".len();
    (
        xml[sdt_at..sdt_close].to_string(),
        xml[b_at..b_end].to_string(),
    )
}

// ---------------------------------------------------------------------------
// 燈 1：錯位源在檔內確實存在（fixture 自證＋canonical 真相）
// ---------------------------------------------------------------------------

#[test]
fn lamp1_wrapped_table_is_canonical_root_but_invisible_to_raw_scan() {
    let e = open(SDT_FIXTURE);
    // canonical 側：兩張根表（T_A 經 sdt 透明包裝仍入樹）。
    assert_eq!(e.table_count(), 2, "canonical 口徑：sdt 內的表也是根表");
    let rows = json(e.tables_summary_json())
        .as_array()
        .expect("摘要陣列")
        .clone();
    assert_eq!(rows.len(), 2, "一張根表一項");
    let u = |v: &Value| v.as_u64().expect("數值欄位") as usize;
    assert!(
        u(&rows[0]["para_start"]) < u(&rows[1]["para_start"]),
        "root_slot 序＝文件序（T_A 在前）"
    );
    // raw 側：`w:body` 直屬子節點只看得到一張 tbl（T_B）——這正是錯位源。
    let raw = raw_direct_body_tbl_count(&doc_xml(&e));
    assert_eq!(
        raw, 1,
        "fixture 前提：raw direct_children 口徑下 body 只有 T_B 一張；\
         本燈紅代表上游已改掉包裝形態、本檔不再證明 S2-1，需重產 fixture"
    );
    assert_eq!(
        raw,
        e.table_count() - 1,
        "錯位量＝被透明包裝藏住的根表數（本檔＝1）"
    );
}

// ---------------------------------------------------------------------------
// 燈 2（本卡核心紅燈）：反查 T_A → 餵 M4 → 准「改對」或「誠實拒絕」，
// 唯獨不准「成功但改到 T_B」
// ---------------------------------------------------------------------------

#[test]
fn lamp2_located_wrapped_table_never_silently_mutates_the_wrong_table() {
    let e0 = open(SDT_FIXTURE);
    let (para, hit) = locate_first_in_slot(&e0, 0);
    assert!(
        hit["table_path"]["nested_steps"]
            .as_array()
            .unwrap()
            .is_empty(),
        "T_A 是根表（root_slot 0），巢狀步必空"
    );

    let (a0, b0) = raw_split(&doc_xml(&e0));

    let mut e = open(SDT_FIXTURE);
    match set_center(&mut e, path_of(&hit)) {
        Ok(()) => {
            let changed = centered_root_slots(&e);
            assert_eq!(
                changed,
                vec![0usize],
                "反查段 {para}（root_slot 0＝SDT 內表 T_A）餵 M4 後，被 centre 的\
                 根表集合必須恰為 [0]；實得 {changed:?} ⇒ 界內錯位：反查說改 T_A、\
                 實際改到別張表（S2-1）"
            );
        }
        Err(err) => {
            assert!(
                err.contains("InvalidTableOperation") || err.contains("路徑與文件不符"),
                "拒絕必須是 InvalidTableOperation（誠實拒絕），實得: {err}"
            );
            let (a1, b1) = raw_split(&doc_xml(&e));
            assert_eq!(b1, b0, "誠實拒絕後裸表 T_B 的 raw XML 必須零變化");
            assert_eq!(a1, a0, "誠實拒絕後 SDT 段 raw XML 必須零變化");
            assert_eq!(
                centered_root_slots(&e),
                Vec::<usize>::new(),
                "拒絕後不得有任何表被 centre（零半寫）"
            );
        }
    }
}

/// 燈 2b：修後走誠實拒絕時，detail 必須點名「路徑與文件不符（疑似 sdt/ins
/// 透明包裝錯位）」——外殼拿到的不是「root_slot 越界」這種會誤導使用者重選表
/// 的假線索。仍靜默成功態 ⇒ 由燈 2 報紅，本燈只掛帳不重複報。
#[test]
fn lamp2b_honest_refusal_names_the_real_cause_not_out_of_range() {
    let e0 = open(SDT_FIXTURE);
    let (_, hit) = locate_first_in_slot(&e0, 0);
    let mut e = open(SDT_FIXTURE);
    match set_center(&mut e, path_of(&hit)) {
        Ok(()) => eprintln!("掛帳：S2-1 尚未修復（反查路徑仍靜默成功），燈 2 已紅"),
        Err(err) => {
            assert!(
                err.contains("路徑與文件不符") && err.contains("透明包裝"),
                "誠實拒絕的 detail 必須點名透明包裝錯位（不誤導成越界），實得: {err}"
            );
            assert!(
                !err.contains("越界"),
                "界內錯位的拒絕不得摻『越界』字樣（會誤導外殼以為張數選錯）: {err}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 燈 3：零波及鎖——同一檔內**每一條**反查路徑都不得半寫；且解析失敗的原因
// 必須歸屬正確（不得把「口徑分岔」報成「使用者選錯第幾張表」）
// ---------------------------------------------------------------------------

/// 本 fixture 的根表張數在兩套口徑下就不相等（canonical 2／raw 直屬 1）⇒ 連
/// 裸表 T_B 也無法由 raw 側以 canonical 的 `root_slot 1` 對號，因此其正解是
/// **誠實拒絕＋全檔零變化**，不是「照常成功」（舊版此燈誤把「未包裝＝必須成功」
/// 寫成期望，那是對未實作行為的臆測，非卡面斷言）。
///
/// 「防線不得誤擋正常檔」的正向保證在燈 4（未包裝多表檔逐張可改）＋
/// `zz_probe_guard`（全語料假拒收計數），本燈負責的是**零波及**。
#[test]
fn lamp3_no_path_in_a_wrapped_file_may_half_write() {
    let e0 = open(SDT_FIXTURE);
    let (a0, b0) = raw_split(&doc_xml(&e0));
    let base = centered_root_slots(&e0);

    // 取該檔全部可反查根表路徑（T_A／T_B 各一條）。
    let mut slots: Vec<usize> = Vec::new();
    for p in 0..e0.paragraph_count() {
        if let Some(h) = locate(&e0, p) {
            let s = h["table_path"]["root_slot"].as_u64().unwrap() as usize;
            if !slots.contains(&s) {
                slots.push(s);
            }
        }
    }
    assert_eq!(slots.len(), 2, "本 fixture 要有 T_A／T_B 兩條可反查根表路徑");

    for slot in slots {
        let (_, hit) = locate_first_in_slot(&e0, slot);
        let mut e = open(SDT_FIXTURE);
        match set_center(&mut e, path_of(&hit)) {
            Ok(()) => assert_eq!(
                centered_root_slots(&e),
                vec![slot],
                "root_slot {slot} 若成功，被改集合必須恰為 [{slot}]（不得波及他表）"
            ),
            Err(err) => {
                // 本檔 root 張數兩套口徑分岔 ⇒ 拒絕原因必須點名透明包裝，
                // 不得留下「越界」這種會誘導使用者重選表的假線索。
                assert!(
                    err.contains("路徑與文件不符") && err.contains("透明包裝"),
                    "root_slot {slot} 的拒絕必須點名透明包裝錯位，實得: {err}"
                );
                assert!(
                    !err.contains("越界"),
                    "界內錯位／口徑分岔的拒絕不得摻『越界』字樣: {err}"
                );
                let (a1, b1) = raw_split(&doc_xml(&e));
                assert_eq!((a1, b1), (a0.clone(), b0.clone()), "拒絕後全檔 raw 零變化");
                assert_eq!(
                    centered_root_slots(&e),
                    base.clone(),
                    "拒絕後不得有任何表被 centre（零半寫）"
                );
            }
        }
    }
    assert!(
        base.is_empty(),
        "fixture 前提：初始零張表自帶 center（否則零半寫斷言失去鑑別力）"
    );
}

// ---------------------------------------------------------------------------
// 燈 4：零回退鎖——未包裝的多表檔（table_multi）反查→M4 照常逐張可改
// ---------------------------------------------------------------------------

#[test]
fn lamp4_ordinary_multi_table_files_keep_working_unchanged() {
    let rel = "fixtures/table_multi.docx";
    for slot in 0..3usize {
        let e0 = open(rel);
        let (_, hit) = locate_first_in_slot(&e0, slot);
        let mut e = open(rel);
        set_center(&mut e, path_of(&hit))
            .unwrap_or_else(|err| panic!("root_slot {slot} 路徑必須成功（誤擋回退）: {err}"));
        assert_eq!(
            centered_root_slots(&e),
            vec![slot],
            "root_slot {slot} 必須只改到自己"
        );
    }
    // 無損紅線：未編輯檔案 roundtrip 零失真。
    let diff = open(rel)
        .roundtrip_check()
        .expect("roundtrip_check 失敗");
    assert!(diff.pass, "未編輯檔案 roundtrip 必須通過: {:?}", diff.violations);
    assert!(diff.byte_identical, "未編輯檔案必須 byte-identical");
}

// ---------------------------------------------------------------------------
// 燈 5：反查面契約不變（表外段回 None 而非錯誤；越界零 panic）
// ---------------------------------------------------------------------------

#[test]
fn lamp5_locator_surface_contract_unchanged() {
    let e = open(SDT_FIXTURE);
    // 本 fixture 有一條表外正文段（表後尾段）⇒ 必須回 None，不是錯誤。
    let nones: Vec<usize> = (0..e.paragraph_count())
        .filter(|p| locate(&e, *p).is_none())
        .collect();
    assert_eq!(nones.len(), 1, "表後尾段是唯一表外段，實得 {nones:?}");
    assert!(e.table_locator_json(e.paragraph_count() + 999).is_none());
    assert!(
        e.table_locator_json(usize::MAX).is_none(),
        "usize::MAX 不得溢位 panic（wasm 邊 panic＝瀏覽器 abort）"
    );
}

// ---------------------------------------------------------------------------
// 燈 6（語料面）：透明包裝語料檔一律准「改對」或「誠實拒絕」
// ---------------------------------------------------------------------------

#[test]
fn lamp6_corpus_wrapped_tables_never_silently_hit_the_wrong_table() {
    let mut files = 0usize;
    let mut paths = 0usize;
    let mut applied = 0usize;
    let mut refused = 0usize;
    for rel in CORPUS_WRAPPED {
        let bytes = match corpus_rel(rel) {
            Some(b) => b,
            None => continue,
        };
        let Ok(e0) = Engine::from_docx_bytes(&bytes) else {
            eprintln!("掛帳：語料檔開檔失敗（非本卡範圍）{rel}");
            continue;
        };
        files += 1;
        // 每檔取每張根表的第一個可反查段落（根表張數有限，全數掃不爆時間）。
        let mut per_slot: Vec<(usize, Value)> = Vec::new();
        for p in 0..e0.paragraph_count() {
            if let Some(h) = locate(&e0, p) {
                let slot = h["table_path"]["root_slot"].as_u64().unwrap() as usize;
                if !per_slot.iter().any(|(s, _)| *s == slot) {
                    per_slot.push((slot, h));
                }
            }
        }
        for (slot, hit) in per_slot {
            paths += 1;
            let mut e = match Engine::from_docx_bytes(&bytes) {
                Ok(e) => e,
                Err(_) => continue,
            };
            match set_center(&mut e, path_of(&hit)) {
                Ok(()) => {
                    applied += 1;
                    let changed = centered_root_slots(&e);
                    assert_eq!(
                        changed,
                        vec![slot],
                        "{rel} root_slot {slot} 反查路徑餵 M4 後被改的根表集合必須恰為 \
                         [{slot}]，實得 {changed:?} ⇒ 界內錯位（S2-1 在語料檔上發作）"
                    );
                }
                Err(err) => {
                    assert!(
                        err.contains("路徑與文件不符") || err.contains("InvalidTableOperation"),
                        "{rel} root_slot {slot} 的拒絕必須誠實（InvalidTableOperation），實得: {err}"
                    );
                    refused += 1;
                    // 零半寫基準＝**該檔原始狀態**已被 centre 的根表集合（語料檔
                    // 本來就可能自帶 center 表，拿空集合當基準會把「檔案原樣」
                    // 誤判成半寫）。基準由同一 bytes 重開一份引擎取得。
                    let pristine = Engine::from_docx_bytes(&bytes)
                        .expect("基準引擎必須開得起來（前面已開過）");
                    assert_eq!(
                        centered_root_slots(&e),
                        centered_root_slots(&pristine),
                        "{rel} root_slot {slot} 拒絕後被 centre 的根表集合必須與編輯前\
                         完全一致（零半寫）"
                    );
                }
            }
        }
    }
    assert!(
        files >= 5,
        "語料樣本必須實質在場（缺太多代表語料不該由本燈假綠）：實得 {files} 檔"
    );
    eprintln!("燈 6 實測：{files} 檔／{paths} 條反查路徑；套改 {applied} 條、誠實拒絕 {refused} 條");
}

// ---------------------------------------------------------------------------
// 燈 7：鑑別力——BUGGY 期望門控（證明「哪張表被改」的斷言是活的）
// ---------------------------------------------------------------------------

#[test]
fn lamp7_buggy_gate_proves_the_attribution_assertion_is_live() {
    let e0 = open(SDT_FIXTURE);
    let (_, hit) = locate_first_in_slot(&e0, 0);
    let mut e = open(SDT_FIXTURE);
    let res = set_center(&mut e, path_of(&hit));
    let changed = centered_root_slots(&e);
    // 斷言的實質內容（綠態必須成立的那件事）：
    //   靜默成功態 ⇒ 被改的恰為反查宣稱的 [0]；誠實拒絕態 ⇒ 零張被改。
    let attribution_holds = match &res {
        Ok(()) => changed == vec![0usize],
        Err(_) => changed.is_empty(),
    };
    if std::env::var("E_SDT_TABLE_BUGGY").is_ok() {
        // BUGGY 態：期望被換成不可能值 ⇒ 本燈必紅（證明燈 2 的斷言真在比對
        // 「哪張表被改」，不是只要不 panic 就綠）。
        assert_eq!(
            changed,
            vec![777],
            "BUGGY 態：期望被換成不可能值 ⇒ 此燈必須紅（實得 {changed:?}）"
        );
    } else {
        assert!(
            attribution_holds,
            "綠態：歸屬斷言必須成立（被改集合 {changed:?}，M4 結果 {res:?}）"
        );
    }
}
