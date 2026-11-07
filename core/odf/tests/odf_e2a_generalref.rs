//! ODF-E2a 考卷：quick-xml 0.41 一般實體（GeneralRef）還原。
//!
//! 紅燈卡 t_0eb593b3 的缺陷：0.37→0.41 把 `&apos;` `&quot;` `&amp;` `&#NN;`
//! 從 Text 事件改發 `Event::GeneralRef`，core/odf 的兩支 walker
//! （`layout_glue.rs::walk_paragraphs`、`body.rs::extract_body_from_bytes`）
//! 漏接，實體所在的字元整批消失。本考卷依卡面鐵律做**逐段完整文字斷言**，
//! 不以字數計數代替。
//!
//! 燈 1（ooo32780-1）：卡面寫「第 2 段含 `didn't`（U+0027）與 `-> `」。
//! 源檔實況（content.xml）：`didn&apos;t` 在投影第 2 段；唯一一個 `&gt;`
//! 在投影第 10 段 `Section Hidden IF (x==1) -&gt; 1`，兩者不同段（見紅燈
//! 報告的證據）；故本燈對第 2 段與第 10 段各自做完整逐字斷言，不把卡面
//! 描述硬湊成不存在的同段。
//!
//! 燈 2（poi/drawing）：報告點名的 `&quot;` 還原，取投影第 3 段
//! `Источник: РИА &quot;Новости&quot;. Горячая линия` 做完整逐字斷言。
//!
//! 燈 3（零回退）：既有 `odf_container`（5 盞）與 `odt_layout_projection`
//! （7 盞）考卷不動，由 `cargo test -p quire-odf` 全數跑到；本檔不重複抄寫。

use quire_odf::{project, OdfBundle};

fn corpus_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-odt")
}

fn open(rel: &str) -> OdfBundle {
    let path = corpus_root().join(rel);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("讀取失敗 {rel}: {e}"));
    OdfBundle::open(&bytes).unwrap_or_else(|e| panic!("開啟失敗 {rel}: {e}"))
}

/// 投影第 2 段：Section1 故事拷貝，含 `didn&apos;t` 與 `who&apos;d`。
const OOO_PARA2: &str = "He heard quiet steps behind him. That didn't bode well. Who could be following him this late at night and in this deadbeat part of town? And at this particular moment, just after he pulled off the big time and was making off with the greenbacks. Was there another crook who'd had the same idea, and was now watching him and waiting for a chance to grab the fruit of his labor? Or did the steps behind him mean that one of many law officers in town was on to him and just waiting to pounce and snap those cuffs on his wrists?";

/// 投影第 10 段：全檔唯一 `-&gt;` 所在（源檔 offset 5817）。
const OOO_PARA10: &str = "Section Hidden IF (x==1) -> 1";

/// 燈 1：ooo32780-1 的 `&apos;`／`&gt;` 逐段完整還原（投影與內文抽取兩支 walker 同驗）。
#[test]
fn lamp1_ooo32780_apos_and_gt_restored() {
    let bundle = open("ext/lo_core_sw_qa_odfimport_ooo32780-1.odt");

    let p = project(&bundle).expect("投影失敗");
    assert_eq!(p.paragraphs[2].text, OOO_PARA2, "投影第 2 段 &apos; 未還原");
    assert_eq!(
        p.paragraphs[10].text, OOO_PARA10,
        "投影第 10 段 &gt; 未還原"
    );

    let body = bundle.extract_body().expect("內文抽取失敗");
    assert_eq!(
        body.paragraphs[2].text, OOO_PARA2,
        "內文抽取第 2 段 &apos; 未還原"
    );
    assert_eq!(
        body.paragraphs[10].text, OOO_PARA10,
        "內文抽取第 10 段 &gt; 未還原"
    );
}

/// 燈 2：poi/drawing 的 `&quot;` 逐段完整還原（投影與內文抽取兩支 walker 同驗）。
#[test]
fn lamp2_poi_drawing_quot_restored() {
    let bundle = open("lo_out/lo__poi__test-data_document_drawing.odt");
    let expected = "Источник: РИА \"Новости\". Горячая линия";

    let p = project(&bundle).expect("投影失敗");
    assert_eq!(p.paragraphs[3].text, expected, "投影第 3 段 &quot; 未還原");

    let body = bundle.extract_body().expect("內文抽取失敗");
    assert_eq!(
        body.paragraphs[3].text, expected,
        "內文抽取第 3 段 &quot; 未還原"
    );
}
