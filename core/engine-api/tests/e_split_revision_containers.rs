//! SplitParagraph 跨切點容器配對紅燈考卷（返修卡 t_e93fb47b）。
//!
//! 繁中註解，禁簡體禁 emoji，署名 ADT109119。
//!
//! # 缺陷（PM 獨立坐實，實錄見 tests/save-roundtrip-baseline/report.md
//! 「引擎突變層缺陷實錄」）
//! 切點落在**段內行內容器**（`w:ins`／`w:hyperlink`／`w:fldSimple`…）包裹的
//! run 內部時，`split_paragraph_xml` 只斷 run 文字流：
//! - 第一段把切點之前**開了沒關**的容器起點原樣留在段尾，直接接 `</w:p>`
//!   → `ill-formed document: expected `</w:ins>`, but `</w:p>` was found`；
//! - 第二段頭留下該容器的孤兒閉標籤。
//!   存檔整批失敗（`quire-cli save --mutation` rc=3，四檔紅燈）。上游容器配對
//!   本就完好（report.md 段內計數坐實），「未閉」是**我方切斷**造成——修法須
//!   自己收斂閉合，不得假設上游配對完好，也不得整段丟棄追蹤標記矇混過燈。
//!
//! # 修復契約（斷言層）
//! 1. 切點兩側容器配對補全：第一段尾由內而外補閉標籤、第二段頭由外而內補
//!    同名開標籤（同屬性逐字複製）；套用與存檔重開全鏈必須成功。
//! 2. 語意正確性以「存檔後檔重開、canonical 段落樹與文字流與未切時一致」
//!    為準——逐段比對引擎 canonical 文字流；切點段一分為二，兩半串接必等於
//!    未切原文（含 `w:delText` 在內的全部文字守恆）。
//! 3. 不丟追蹤標記：`w:ins`／`w:del`／`w:hyperlink`／`w:fldSimple` 開閉計數
//!    在切後**成對增加**且配對平衡（深度計數），作者／日期／r:id／instr 屬性
//!    逐字保留。
//! 4. 段落屬性不複製：拆段後每段 `<w:pPr>` 恰好一顆（舊缺陷＝雙份 pPr，
//!    Jupiter cp=3 態實錄）。
//! 5. Undo 閉環：拆段→Undo→存檔必須 byte-identical。
//!
//! # 燈號分派
//! - F1～F5：合成 fixture 五形態（w:ins＋巢裝 rPrChange／w:del＋w:delText／
//!   w:hyperlink 多 run／w:fldSimple／w:ins×w:hyperlink 深度 2 巢裝）。
//! - C1～C4：report.md 四檔精確重現（同 para／同 cp 零自由度）。
//! - R1：pPr 單份鎖（燈 4 形態）。
//! - U1：Undo 後存檔位元組恆等。
//!
//! # BUGGY 回塞鎖（比照 E_*_BUGGY／U4F_*_BUGGY 口徑）
//! `SPLIT_REV_BUGGY=1`（生產碼 `split_paragraph_xml` 內建 env 開關）＝回退
//! 「切點容器配對補全」（鏡像修前形態）：
//! - 燈 F1／F3／F4／F5／C1～C4／R1／U1：切點跨容器或踩 pPr 複製線 ⇒ BUGGY
//!   態必紅（回塞＝不配對補全＋沿用舊 pPr 前綴口徑，鏡像修前形態：
//!   `apply_mutation` 拍 `ill-formed document`／雙份 pPr 斷言紅）。
//! - 燈 F2（del 不跨切點形態）不受本鎖影響（切點不跨容器，配對補全本來
//!   就是恆等路徑），回塞態仍綠＝鑑別力不混同，檔頭記帳。

use quire_engine::{DocPos, Engine, Mutation, SubDocTarget};

fn settle(engine: &mut Engine) {
    while !engine.is_settled() {
        engine.reflow_step_json();
    }
}

fn load_corpus_bytes(rel: &str) -> Vec<u8> {
    let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/").to_string() + rel;
    std::fs::read(&full).unwrap_or_else(|e| panic!("讀取 corpus/{rel} 失敗: {e}"))
}

/// 合成 fixture：缺失時就地重跑產生器（byte 級冪等，比照 odf_encrypted_gate 口徑）。
fn fixture_bytes() -> Vec<u8> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/split_revision_containers.docx"
    );
    if let Ok(b) = std::fs::read(path) {
        return b;
    }
    let gen = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/gen_split_revision_fixture.py"
    );
    let status = std::process::Command::new("python3")
        .arg(gen)
        .status()
        .expect("python3 產生 fixture 失敗");
    assert!(status.success(), "產生器回傳失敗: {gen}");
    std::fs::read(path).expect("重跑產生器後仍讀不到 fixture")
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

// ---------------- 考卷內獨立掃描器（XML 層真值，不借引擎解析口徑） ----------------

/// 標籤流掃描器狀態：逐標籤回傳 (是否閉標籤, 完整元素名, 是否自閉合, 起, 迄)。
fn scan_tags(xml: &str) -> Vec<(bool, String, bool, usize, usize)> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while cursor < xml.len() {
        let Some(lt_rel) = xml[cursor..].find('<') else {
            break;
        };
        let lt = cursor + lt_rel;
        if xml[lt..].starts_with("<!--") {
            cursor = xml[lt..]
                .find("-->")
                .map(|i| lt + i + 3)
                .unwrap_or(xml.len());
            continue;
        }
        if xml[lt..].starts_with("<!") || xml[lt..].starts_with("<?") {
            // XML 宣告／指令／CDATA：不進堆疊（`<?xml` 若入堆＝永不到貨的閉標籤假紅）。
            cursor = xml[lt..].find('>').map(|i| lt + i + 1).unwrap_or(xml.len());
            continue;
        }
        let Some(gt_rel) = xml[lt..].find('>') else {
            break;
        };
        let gt = lt + gt_rel;
        let tag = &xml[lt..=gt];
        let is_close = tag.starts_with("</");
        let is_empty = tag.ends_with("/>");
        let head = tag
            .trim_start_matches("</")
            .trim_start_matches('<')
            .trim_end()
            .trim_end_matches('/')
            .trim_end_matches('>');
        let name = head.split_whitespace().next().unwrap_or("").to_string();
        out.push((is_close, name, is_empty, lt, gt + 1));
        cursor = gt + 1;
    }
    out
}

/// 全文（或切片）容器配對平衡檢查：回傳**段尾仍未閉合**的開標籤名清單
/// （由外而內）。閉標籤與堆疊頂不同名＝配錯（記名回傳，呼叫端判紅）。
fn unbalanced_names(xml: &str) -> Vec<String> {
    let mut stack: Vec<String> = Vec::new();
    let mut ghosts: Vec<String> = Vec::new();
    for (is_close, name, is_empty, _, _) in scan_tags(xml) {
        if is_empty {
            continue;
        }
        if !is_close {
            stack.push(name);
        } else {
            match stack.last() {
                Some(top) if *top == name => {
                    stack.pop();
                }
                _ => ghosts.push(format!("孤兒閉標籤 {name}")),
            }
        }
    }
    ghosts.extend(stack);
    ghosts
}

/// `xml` 中第 `para` 顆頂層 `<w:p>`（扁平段落，口徑＝引擎 find_paragraph_span：
/// 巢裝 `w:p` 計深度、txbxContent／Fallback 子樹跳過）的半開字元區間。
fn top_level_para_span(xml: &str, para: usize) -> (usize, usize) {
    let mut index = 0usize;
    let mut cursor = 0usize;
    let mut fb_depth = 0usize;
    let mut txbx_depth = 0usize;
    let mut ac_stack: Vec<usize> = Vec::new();
    while cursor < xml.len() {
        let Some(lt_rel) = xml[cursor..].find('<') else {
            break;
        };
        let lt = cursor + lt_rel;
        let Some(gt_rel) = xml[lt..].find('>') else {
            break;
        };
        let gt = lt + gt_rel;
        let tag = &xml[lt..=gt];
        cursor = gt + 1;
        let is_close = tag.starts_with("</");
        let is_empty = tag.ends_with("/>");
        let head = tag
            .trim_start_matches("</")
            .trim_start_matches('<')
            .trim_end()
            .trim_end_matches('/')
            .trim_end_matches('>');
        let full = head.split_whitespace().next().unwrap_or("");
        let local = full.rsplit(':').next().unwrap_or("");
        if !is_close {
            if local == "AlternateContent" {
                ac_stack.push(fb_depth);
            } else if local == "Fallback" {
                fb_depth += 1;
            }
        } else if local == "Fallback" {
            fb_depth = fb_depth.saturating_sub(1);
        } else if local == "AlternateContent" {
            if let Some(prev) = ac_stack.pop() {
                fb_depth = prev;
            }
        }
        if fb_depth > 0 {
            continue;
        }
        if !is_close && local == "txbxContent" {
            txbx_depth += 1;
        } else if is_close && local == "txbxContent" {
            txbx_depth = txbx_depth.saturating_sub(1);
        }
        if txbx_depth > 0 {
            continue;
        }
        if !is_close && !is_empty && local == "p" {
            // 深度計數找配對閉標籤（巢裝 w:p 合法）。
            let mut depth = 1usize;
            let mut sc = gt + 1;
            let mut end = None;
            while sc < xml.len() {
                let Some(l2) = xml[sc..].find('<') else { break };
                let l2 = sc + l2;
                let Some(g2) = xml[l2..].find('>') else { break };
                let g2 = l2 + g2;
                let t2 = &xml[l2..=g2];
                sc = g2 + 1;
                let ic = t2.starts_with("</");
                let ie = t2.ends_with("/>");
                let h2 = t2
                    .trim_start_matches("</")
                    .trim_start_matches('<')
                    .trim_end()
                    .trim_end_matches('/')
                    .trim_end_matches('>');
                let f2 = h2.split_whitespace().next().unwrap_or("");
                if f2.rsplit(':').next().unwrap_or("") == "p" {
                    if !ic && !ie {
                        depth += 1;
                    } else if ic {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(g2 + 1);
                            break;
                        }
                    }
                }
            }
            let Some(end) = end else { continue };
            if index == para {
                return (lt, end);
            }
            index += 1;
            cursor = end;
        }
    }
    panic!("找不到頂層段 {para}（共 {index} 段）");
}

fn count_tag(xml: &str, open_prefix: &str, close_tag: &str) -> (usize, usize) {
    let open = xml.matches(open_prefix).count();
    let close = xml.matches(close_tag).count();
    (open, close)
}

/// 切片內全部文字流（`w:t`＋`w:delText` 明文串接）——文字守恆真值，獨立於
/// 引擎 cp 口徑（`w:delText` 不在 cp 空間卻在 canonical 文字流）。
fn xml_text_flow(xml: &str) -> String {
    let mut out = String::new();
    let mut cursor = 0usize;
    while cursor < xml.len() {
        let Some(lt_rel) = xml[cursor..].find('<') else {
            break;
        };
        let lt = cursor + lt_rel;
        let Some(gt_rel) = xml[lt..].find('>') else {
            break;
        };
        let gt = lt + gt_rel;
        let tag = &xml[lt..=gt];
        let is_close = tag.starts_with("</");
        let local = {
            let head = tag
                .trim_start_matches("</")
                .trim_start_matches('<')
                .trim_end()
                .trim_end_matches('/')
                .trim_end_matches('>');
            head.split_whitespace()
                .next()
                .unwrap_or("")
                .rsplit(':')
                .next()
                .unwrap_or("")
                .to_string()
        };
        if !is_close && (local == "t" || local == "delText") {
            let close_needle = format!("</w:{local}>");
            let Some(cl_rel) = xml[gt + 1..].find(&close_needle) else {
                break;
            };
            out.push_str(&xml[gt + 1..gt + 1 + cl_rel]);
            cursor = gt + 1 + cl_rel + close_needle.len();
            continue;
        }
        cursor = gt + 1;
    }
    out
}

fn split(engine: &mut Engine, para: usize, cp: usize) {
    engine
        .apply_mutation(Mutation::SplitParagraph {
            target: SubDocTarget::Body,
            pos: DocPos { para, cp },
        })
        .unwrap_or_else(|e| panic!("段 {para} cp {cp} 拆段必須成功（跨切點容器配對補全）: {e}"));
    settle(engine);
}

/// 燈 F1～F5 通用斷言：拆段成功＋存檔重開成功＋配對平衡＋追蹤標記不丟。
/// `cross`＝跨切點容器（開標籤前綴, 閉標籤）：切後開閉計數必須**成對增加**；
/// `intact`＝段內其他容器：切後計數必須**逐字不變**（深度計數炸彈鎖）。
fn assert_split_ok(
    fixture: &[u8],
    para: usize,
    cp: usize,
    cross: &[(&str, &str)],
    intact: &[(&str, &str)],
) {
    let mut base = Engine::from_docx_bytes(fixture).expect("開 fixture 失敗");
    settle(&mut base);
    let base_xml = doc_xml(&base);
    let (b_start, b_end) = top_level_para_span(&base_xml, para);
    let base_para = &base_xml[b_start..b_end];
    let base_flow = xml_text_flow(base_para);
    let base_para_count = base.paragraph_count();

    let mut engine = Engine::from_docx_bytes(fixture).expect("開 fixture 失敗");
    settle(&mut engine);
    split(&mut engine, para, cp);
    assert_eq!(
        engine.paragraph_count(),
        base_para_count + 1,
        "拆段後扁平段落數必須 +1（段 {para} cp {cp}）"
    );

    let xml = doc_xml(&engine);
    // 配對平衡：整個 document.xml 的容器堆疊必須收斂（修前＝段尾未閉紅）。
    let ghosts = unbalanced_names(&xml);
    assert!(
        ghosts.is_empty(),
        "拆段後 document.xml 容器配對必須全數收斂，實得未閉／配錯: {ghosts:?}"
    );

    // 切後兩段（原段一分為二）。
    let (l_start, l_end) = top_level_para_span(&xml, para);
    let (r_start, r_end) = top_level_para_span(&xml, para + 1);
    let left = &xml[l_start..l_end];
    let right = &xml[r_start..r_end];

    // 文字流守恆：兩半串接＝未切原文（含 delText）。
    assert_eq!(
        xml_text_flow(left) + &xml_text_flow(right),
        base_flow,
        "切點段文字流必須逐字守恆（拆段只斷點、不得增刪文字）"
    );

    // 追蹤標記不丟：跨切點容器開標籤計數＝原段 +1（第一段補的閉、第二段補
    // 的開都成對）；段內其他容器計數逐字不變（修前深度計數截斷＝同類炸彈，
    // rPrChange 巢 rPr 語料實測 589 處）。兩半各自配對平衡。
    assert!(unbalanced_names(left).is_empty(), "第一段內部容器必須平衡");
    assert!(unbalanced_names(right).is_empty(), "第二段內部容器必須平衡");
    let joined = format!("{left}{right}");
    for (open_prefix, close_tag) in cross {
        let (b_open, b_close) = count_tag(base_para, open_prefix, close_tag);
        let (a_open, a_close) = count_tag(&joined, open_prefix, close_tag);
        assert_eq!(b_open, b_close, "fixture 原段容器本就配對完好（前提）");
        assert!(
            a_open > b_open && a_close > b_close,
            "跨切點容器 {open_prefix} 必須在切後成對補全（開 {b_open}→{a_open}、閉 {b_close}→{a_close}）"
        );
        assert_eq!(
            a_open - b_open,
            a_close - b_close,
            "跨切點容器 {open_prefix} 補全必須開閉同數（配對，不孤兒）"
        );
    }
    for (open_prefix, close_tag) in intact {
        let (b_open, b_close) = count_tag(base_para, open_prefix, close_tag);
        let (a_open, a_close) = count_tag(&joined, open_prefix, close_tag);
        // 切 run 會把該 run 的宣告（rPr 系）合法複製進兩半 ⇒ 只鎖「不丟、
        // 不孤兒」：計數不得低於原段、且開閉必須仍配對（修前首閉截斷炸彈＝
        // 計數丟＋配對崩壞，兩條都拍紅）。
        assert!(
            a_open >= b_open && a_close >= b_close,
            "非跨切點容器 {open_prefix} 不得丟失（實得 {b_open}/{b_close}→{a_open}/{a_close}）"
        );
        assert_eq!(
            a_open, a_close,
            "非跨切點容器 {open_prefix} 切後開閉必須仍配對（{a_open} vs {a_close}）"
        );
    }

    // 段 pPr 單份（雙份 pPr 缺陷鎖）。
    assert_eq!(
        left.matches("<w:pPr>").count(),
        right.matches("<w:pPr>").count(),
        "兩半 pPr 份數必須一致"
    );
    assert!(
        left.matches("<w:pPr>").count() <= 1,
        "第一段 pPr 必須恰好一顆（實得 {} 顆＝舊雙份缺陷）",
        left.matches("<w:pPr>").count()
    );

    // 存檔重開閉環：canonical 段落樹＋文字流與記憶體態一致。
    let saved = engine.save_docx().expect("拆段後存檔失敗");
    let mut reopened = Engine::from_docx_bytes(&saved).expect("拆段後重開失敗（ill-formed）");
    settle(&mut reopened);
    assert_eq!(
        reopened.paragraph_count(),
        base_para_count + 1,
        "重開後段落數必須保留 +1"
    );
    for p in 0..base_para_count {
        if p == para {
            continue; // 切點段一分為二，另行以兩半串接校驗
        }
        let after_p = if p < para { p } else { p + 1 };
        let before = base.paragraph_text(p).unwrap_or_default();
        let after = reopened.paragraph_text(after_p).unwrap_or_default();
        assert_eq!(
            before, after,
            "未觸碰段 {p}（重開槽 {after_p}）canonical 文字流必須逐字不變"
        );
    }
    let left_txt = reopened.paragraph_text(para).unwrap_or_default();
    let right_txt = reopened.paragraph_text(para + 1).unwrap_or_default();
    let base_txt = base.paragraph_text(para).unwrap_or_default();
    assert_eq!(
        format!("{left_txt}{right_txt}"),
        base_txt,
        "切點段 canonical 文字流兩半串接必須等於未切原文"
    );
}

/// 燈 F5 專用：巢裝容器（w:ins 內 w:hyperlink）屬性逐字保留鎖。
fn assert_attrs_preserved(xml: &str, attrs: &[&str]) {
    for a in attrs {
        assert!(xml.contains(a), "容器複製開標籤必須逐字保留屬性 {a:?}");
    }
}

// ---------------- 合成 fixture 五形態 ----------------

/// 燈 F1：`w:ins`（內含巢裝 `w:rPrChange` 之 rPr）包裹 run，切點落在 run 內
/// ——n830205 形態；修前 expected `</w:ins>` 紅。
#[test]
fn split_rev_lamp_f1_tracked_insert() {
    assert_split_ok(
        &fixture_bytes(),
        1,
        12,
        &[("<w:ins ", "</w:ins>")],
        &[("<w:rPrChange ", "</w:rPrChange>")],
    );
}

/// 燈 F2：切點段含 `w:del`＋`w:delText`——delText 不在 cp 空間但在 canonical
/// 文字流，拆段不得吞；修前此形態不跨切點（配對補全為恆等路徑），修後必須
/// 逐字守恆（本燈在 BUGGY 態仍綠＝鑑別力不混同，檔頭已記帳）。
#[test]
fn split_rev_lamp_f2_deltext_conserved() {
    let fx = fixture_bytes();
    let mut base = Engine::from_docx_bytes(&fx).expect("開 fixture 失敗");
    settle(&mut base);
    let base_xml = doc_xml(&base);
    let (b_start, b_end) = top_level_para_span(&base_xml, 2);
    let base_flow = xml_text_flow(&base_xml[b_start..b_end]);

    let mut engine = Engine::from_docx_bytes(&fx).expect("開 fixture 失敗");
    settle(&mut engine);
    split(&mut engine, 2, 4);
    let xml = doc_xml(&engine);
    let (l_start, l_end) = top_level_para_span(&xml, 2);
    let (r_start, r_end) = top_level_para_span(&xml, 3);
    let flow: String = xml_text_flow(&xml[l_start..l_end]) + &xml_text_flow(&xml[r_start..r_end]);
    assert_eq!(
        flow, base_flow,
        "w:delText 文字流必須逐字守恆（拆段只斷點、不得吞刪除標記文字）",
    );
    let saved = engine.save_docx().expect("含 del 段拆段後存檔失敗");
    Engine::from_docx_bytes(&saved).expect("含 del 段拆段後重開失敗");
}

/// 燈 F3：`w:hyperlink` 多 run，切點落第二顆 run——WordWithAttachments／
/// tdf154695 形態；修前 expected `</w:hyperlink>` 紅。
#[test]
fn split_rev_lamp_f3_hyperlink() {
    let fx = fixture_bytes();
    assert_split_ok(
        &fx,
        3,
        9,
        &[("<w:hyperlink ", "</w:hyperlink>")],
        &[("<w:rPr>", "</w:rPr>")],
    );
    let xml = {
        let mut e = Engine::from_docx_bytes(&fx).unwrap();
        settle(&mut e);
        split(&mut e, 3, 9);
        doc_xml(&e)
    };
    assert_attrs_preserved(&xml, &[r#"r:id="rIdHyper""#, r#"w:history="1""#]);
}

/// 燈 F4：`w:fldSimple` 欄位段，切點落欄位內 run——Jupiter 形態；修前
/// expected `</w:fldSimple>` 紅。
#[test]
fn split_rev_lamp_f4_fld_simple() {
    assert_split_ok(
        &fixture_bytes(),
        4,
        9,
        &[("<w:fldSimple ", "</w:fldSimple>")],
        &[("<w:rPr>", "</w:rPr>")],
    );
}

/// 燈 F5：深度 2 巢裝（`w:ins` 內 `w:hyperlink`）——多層鏈必須由內而外補閉、
/// 由外而內補開（順序錯＝配錯，unbalanced_names 拍紅）。
#[test]
fn split_rev_lamp_f5_nested_chain() {
    assert_split_ok(
        &fixture_bytes(),
        5,
        3,
        &[("<w:ins ", "</w:ins>"), ("<w:hyperlink ", "</w:hyperlink>")],
        &[("<w:rPr>", "</w:rPr>")],
    );
    let fx = fixture_bytes();
    let mut e = Engine::from_docx_bytes(&fx).unwrap();
    settle(&mut e);
    split(&mut e, 5, 3);
    assert_attrs_preserved(
        &doc_xml(&e),
        &[r#"w:author="QuireProbe""#, r#"r:id="rIdHyper""#],
    );
}

// ---------------- corpus 四檔精確重現（report.md 實錄口徑） ----------------

/// 通用 corpus 重現盞：同 para／同 cp 零自由度；修前即卡面紅燈原文。
/// 文字流守恆走 **XML 層真值**（`xml_text_flow`）：版面層 `paragraph_text`
/// 有 TOC 前導點等口徑分歧（save_roundtrip_lab 檔頭同款教訓：cp 空間與版面
/// 文字長度可能分歧，混用造成假紅），canonical 樹校驗另以段落數＋重開成功
/// 把關。
fn assert_corpus_split_ok(rel: &str, para: usize, cp: usize) {
    let bytes = load_corpus_bytes(rel);
    let mut base = Engine::from_docx_bytes(&bytes).expect("開 corpus 檔失敗");
    settle(&mut base);
    let base_count = base.paragraph_count();
    let base_xml = doc_xml(&base);
    let (b_start, b_end) = top_level_para_span(&base_xml, para);
    let base_flow = xml_text_flow(&base_xml[b_start..b_end]);

    let mut engine = Engine::from_docx_bytes(&bytes).expect("開 corpus 檔失敗");
    settle(&mut engine);
    split(&mut engine, para, cp);
    assert_eq!(engine.paragraph_count(), base_count + 1, "拆段後段落數 +1");
    let xml = doc_xml(&engine);
    let ghosts = unbalanced_names(&xml);
    assert!(
        ghosts.is_empty(),
        "{rel}: 拆段後容器必須收斂，實得 {ghosts:?}"
    );
    let (l_start, l_end) = top_level_para_span(&xml, para);
    let (r_start, r_end) = top_level_para_span(&xml, para + 1);
    let flow: String = xml_text_flow(&xml[l_start..l_end]) + &xml_text_flow(&xml[r_start..r_end]);
    assert_eq!(
        flow, base_flow,
        "{rel}: 切點段 XML 文字流兩半串接必須等於未切原文（段 {para} cp {cp}）"
    );
    // 未觸碰段逐段零變動（XML 層，逐段切片對照）。
    for p in 0..base_count {
        if p == para {
            continue;
        }
        let (bs, be) = top_level_para_span(&base_xml, p);
        let (as_, ae) = top_level_para_span(&xml, if p < para { p } else { p + 1 });
        assert_eq!(
            &base_xml[bs..be],
            &xml[as_..ae],
            "{rel}: 未觸碰段 {p} 切片必須逐字不變"
        );
    }
    let saved = engine
        .save_docx()
        .unwrap_or_else(|e| panic!("{rel}: 拆段後存檔失敗: {e}"));
    let mut reopened =
        Engine::from_docx_bytes(&saved).unwrap_or_else(|e| panic!("{rel}: 拆段後重開失敗: {e}"));
    settle(&mut reopened);
    assert_eq!(reopened.paragraph_count(), base_count + 1, "重開段落數 +1");
    let re_xml = doc_xml(&reopened);
    let (rl_start, rl_end) = top_level_para_span(&re_xml, para);
    let (rr_start, rr_end) = top_level_para_span(&re_xml, para + 1);
    let re_flow: String =
        xml_text_flow(&re_xml[rl_start..rl_end]) + &xml_text_flow(&re_xml[rr_start..rr_end]);
    assert_eq!(
        re_flow, base_flow,
        "{rel}: 存檔重開後切點段文字流必須仍等於未切原文"
    );
}

/// 燈 C1：ooxmlexport/n830205.docx（enter 臂；卡面最小重現 para 10 cp 29，
/// 切點落 `w:ins` 包裹 run；上游另含 `w:del`／巢裝 `w:rPrChange`）。
#[test]
fn split_rev_lamp_c1_n830205() {
    assert_corpus_split_ok("ooxmlexport/n830205.docx", 10, 29);
}

/// 燈 C2：poi/test-data_document_WordWithAttachments.docx（para 4 cp 23，
/// 切點落 `w:hyperlink` 包裹 run）。
#[test]
fn split_rev_lamp_c2_word_with_attachments() {
    assert_corpus_split_ok("poi/test-data_document_WordWithAttachments.docx", 4, 23);
}

/// 燈 C3：xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Jupiter.docx
/// （para 2 cp 8，切點落 `w:fldSimple` 欄位內 run）。
#[test]
fn split_rev_lamp_c3_document_builder_jupiter() {
    assert_corpus_split_ok(
        "xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Jupiter.docx",
        2,
        8,
    );
}

/// 燈 C4：ooxmlimport/tdf154695-ToC_no_numbers.docx（para 1 cp 5，切點落
/// TOC `w:hyperlink` 包裹 run，段內另有 `w:fldChar` 欄位鏈）。
#[test]
fn split_rev_lamp_c4_toc_no_numbers() {
    assert_corpus_split_ok("ooxmlimport/tdf154695-ToC_no_numbers.docx", 1, 5);
}

// ---------------- pPr 單份鎖＋Undo 閉環 ----------------

/// 燈 R1：拆段不得複製 pPr（雙份 pPr 缺陷＝修前 Jupiter cp=3 實錄，段內
/// 出現兩顆 `<w:pPr>`）。切點刻意落 `w:fldSimple` **之前**的 run，同時拍
/// 「配對補全未觸發時也不得雙寫 pPr」。
#[test]
fn split_rev_lamp_r1_ppr_not_duplicated() {
    let bytes = load_corpus_bytes("xopt/OpenXmlPowerToolsExamples_DocumentBuilder04_Jupiter.docx");
    let mut engine = Engine::from_docx_bytes(&bytes).expect("開檔失敗");
    settle(&mut engine);
    split(&mut engine, 2, 3);
    let xml = doc_xml(&engine);
    let (l_start, l_end) = top_level_para_span(&xml, 2);
    let (r_start, r_end) = top_level_para_span(&xml, 3);
    assert_eq!(
        xml[l_start..l_end].matches("<w:pPr>").count(),
        1,
        "第一段 pPr 必須恰好一顆"
    );
    assert_eq!(
        xml[r_start..r_end].matches("<w:pPr>").count(),
        1,
        "第二段 pPr 必須恰好一顆"
    );
    // sectPr 只准留在末段（既有口徑）。
    assert_eq!(
        xml[r_start..r_end].matches("<w:sectPr").count(),
        0,
        "拆段段不得複寫 sectPr"
    );
}

/// 燈 U1：拆段→Undo→存檔 byte-identical（Undo 逆補丁逐位元組還原；配對補全
/// 不得污染基線）。
#[test]
fn split_rev_lamp_u1_undo_restores_bytes() {
    let bytes = load_corpus_bytes("ooxmlimport/tdf154695-ToC_no_numbers.docx");
    let mut engine = Engine::from_docx_bytes(&bytes).expect("開檔失敗");
    settle(&mut engine);
    split(&mut engine, 1, 5);
    let count_after = engine.paragraph_count();
    engine.undo().expect("undo 拆段失敗");
    settle(&mut engine);
    assert_eq!(
        engine.paragraph_count(),
        count_after - 1,
        "undo 後段落數必須復原"
    );
    let saved = engine.save_docx().expect("undo 後存檔失敗");
    let diff = quire_engine::diff_untouched(&bytes, &saved).expect("zip 對照失敗");
    assert!(
        diff.byte_identical,
        "undo 拆段後存檔必須 byte-identical（pass={}/violations={:?}/missing={:?}）",
        diff.pass, diff.violations, diff.missing
    );
}
