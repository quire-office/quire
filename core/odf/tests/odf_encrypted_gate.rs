//! ODF-E1 紅燈考卷：加密 ODT 必須明確報「加密不支援」，不得靜默投影成空文件。
//!
//! 口徑（ODF-E1 卡＋docs/odf-e1-redlight.md）：
//! - 容器層（`OdfBundle::open`）照舊成功：加密檔是完好封裝，無損 round-trip
//!   與登記面不受影響（survey「只登記不解析，不得當損壞檔」紀律）。
//! - 消費層（`extract_body`／`project`）遇 manifest 宣告加密的條目必須回
//!   明確錯誤變體（Debug 含 `Encrypted`），訊息含「加密不支援」＋**演算法名**
//!   （`manifest:algorithm-name` 之資料，非寫死字串）。
//! - 攔截按消費面：`extract_body` 只吃 content.xml；`project` 吃
//!   content.xml＋styles.xml（級聯依賴）。meta.xml 加密不得拖垮投影。
//! - `algorithm-name` 未宣告時訊息寫「未宣告」，不得憑空捏造（不虛構鐵律）。
//!
//! BUGGY 鑑別力門（k25b 紀律：env 只在單一 test 函數內讀取，不跨執行緒污染）：
//! `ODF_E1_BUGGY=1` 把期望演算法名換成錯值（`Twofish CFB`），燈 1／燈 3 必紅
//! ⇒ 證明斷言真的在比 manifest 解析出來的演算法名，而非恆真字串比對。

use quire_odf::{OdfBundle, OdfParagraph};

const SHA1_CORRECT: &str = "ext/lo_core_sw_qa_odfexport_sha1_correct.odt";
const SHA1_WRONG: &str = "ext/lo_core_sw_qa_odfexport_sha1_wrong.odt";

fn corpus_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-odt")
}

fn read_fixture(name: &str) -> Vec<u8> {
    let p = corpus_root().join("fixtures").join(name);
    if !p.exists() {
        // 固定 input 的 byte 級冪等合成檔：缺失時就地重跑產生器（冪等，
        // sha256 已登記 corpus-odt/manifest.json fixtures 欄）。
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/gen_odf_encrypted_fixture.py");
        let status = std::process::Command::new("python3")
            .arg(&script)
            .status()
            .unwrap_or_else(|e| panic!("執行 fixture 產生器失敗 {}: {e}", script.display()));
        assert!(status.success(), "fixture 產生器退出非零: {status}");
    }
    std::fs::read(&p).unwrap_or_else(|e| panic!("讀取 fixture 失敗 {name}: {e}"))
}

fn read_corpus(rel: &str) -> Vec<u8> {
    let p = corpus_root().join(rel);
    std::fs::read(&p).unwrap_or_else(|e| panic!("讀取語料失敗 {rel}: {e}"))
}

/// 錯誤面共同斷言：必須是錯誤（非 Ok 零段）、訊息含「加密不支援」＋期望演算法名、
/// Debug repr 含明確變體名 `Encrypted`。`R: std::error::Error` 同時覆蓋
/// `Result<BodyModel,_>`／`Result<OdfProjection,_>` 兩軌。
fn assert_encrypted_error<R, E>(res: Result<R, E>, what: &str, want_algo: &str)
where
    E: std::error::Error,
{
    match res {
        Ok(_) => panic!(
            "{what}：加密文件必須回 Err（加密不支援），實際回 Ok——靜默空文件正是本卡要根除的缺陷"
        ),
        Err(e) => {
            let msg = e.to_string();
            let dbg = format!("{e:?}");
            assert!(
                msg.contains("加密不支援"),
                "{what}：錯誤訊息應含「加密不支援」，實際: {msg:?}"
            );
            assert!(
                msg.contains(want_algo),
                "{what}：錯誤訊息應含 manifest 宣告的演算法名 {want_algo:?}，實際: {msg:?}"
            );
            assert!(
                dbg.contains("Encrypted"),
                "{what}：錯誤應落在明確變體 `Encrypted`（Debug 含變體名），實際 Debug: {dbg}"
            );
        }
    }
}

fn assert_ok_body(body: &quire_odf::BodyModel, what: &str) {
    assert_eq!(
        body.text_p_count, 2,
        "{what}：明文 content.xml 之 text:p 應為 2"
    );
    assert_eq!(
        body.text_h_count, 1,
        "{what}：明文 content.xml 之 text:h 應為 1"
    );
    // C2 既有契約：paragraphs 清單只收 text:p（heading 走 text_h_count 計數）。
    assert_eq!(body.paragraphs.len(), 2, "{what}：段落清單（text:p）應為 2");
    let texts: Vec<String> = body
        .paragraphs
        .iter()
        .map(|p: &OdfParagraph| p.text.clone())
        .collect();
    assert!(
        texts.iter().any(|t| t == "第一段明文"),
        "{what}：明文段落文字必須進模型，實際 {texts:?}"
    );
    assert!(
        body.total_text_len > 0,
        "{what}：明文抽取之文字總量不得為零"
    );
}

/// ODF-E1 考卷本體（單函數依序跑六盞燈——BUGGY env 讀取不跨執行緒）。
#[test]
fn odf_e1_encrypted_gate_suite() {
    let buggy = std::env::var("ODF_E1_BUGGY").is_ok();
    // BUGGY 模式：期望值故意換成封裝裡不存在的演算法名 ⇒ 燈 1／燈 3 必紅。
    let algo_sha1 = if buggy { "Twofish CFB" } else { "Blowfish CFB" };
    let algo_aes = if buggy { "Twofish CFB" } else { "AES256.GCM" };

    // ============ 燈 1：語料兩檔 extract_body 必須報加密＋演算法名 ============
    for rel in [SHA1_CORRECT, SHA1_WRONG] {
        let bytes = read_corpus(rel);
        let bundle =
            OdfBundle::open(&bytes).unwrap_or_else(|e| panic!("{rel} 容器層必須開檔成功: {e}"));
        // manifest 偵測面（既有 API）：content.xml 必須被標記加密。
        assert!(
            bundle.manifest().is_encrypted("content.xml"),
            "{rel}：manifest 應宣告 content.xml 加密"
        );
        let res = bundle
            .extract_body()
            .map(|b| (b.paragraphs.len(), b.total_text_len));
        assert_encrypted_error(res, &format!("{rel} extract_body"), algo_sha1);
    }

    // ============ 燈 2：語料兩檔 project 必須報加密＋演算法名 ============
    //（odt-render CLI 走這條路；CLI 層非零退出碼由 shells/native 考卷守。）
    for rel in [SHA1_CORRECT, SHA1_WRONG] {
        let bytes = read_corpus(rel);
        let bundle =
            OdfBundle::open(&bytes).unwrap_or_else(|e| panic!("{rel} 容器層必須開檔成功: {e}"));
        let res = quire_odf::project(&bundle);
        assert_encrypted_error(res, &format!("{rel} project"), algo_sha1);
    }

    // ============ 燈 3：合成 AES256.GCM fixture——演算法名必須來自資料 ============
    //（語料只有 Blowfish 一種演算法；換演算法名還報對 ⇒ 名字是 parse 出來的。）
    let bytes = read_fixture("encrypted_aes256.odt");
    let bundle = OdfBundle::open(&bytes).expect("encrypted_aes256.odt 容器層開檔");
    assert_encrypted_error(
        bundle.extract_body(),
        "encrypted_aes256.odt extract_body",
        algo_aes,
    );
    assert_encrypted_error(
        quire_odf::project(&bundle),
        "encrypted_aes256.odt project",
        algo_aes,
    );

    // ============ 燈 4：encryption-data 無 algorithm 子元素——報「未宣告」不捏造 ============
    // 兩條消費軌對稱斷言（review S3：演算法缺位時 extract_body／project 口徑
    // 必須百分之百一致）。
    let bytes = read_fixture("encrypted_no_algorithm.odt");
    let bundle = OdfBundle::open(&bytes).expect("encrypted_no_algorithm.odt 容器層開檔");
    // 消費軌錯誤斷言用的錯誤型別別名（clippy type_complexity 友好）。
    type TrackResult = Result<(usize, usize), quire_odf::OdfError>;
    let tracks: [(&str, TrackResult); 2] = [
        (
            "extract_body",
            bundle
                .extract_body()
                .map(|b| (b.paragraphs.len(), b.total_text_len)),
        ),
        (
            "project",
            quire_odf::project(&bundle).map(|p| (p.paragraph_count(), p.total_text_len)),
        ),
    ];
    for (what, res) in tracks {
        match res {
            Ok(_) => panic!("encrypted_no_algorithm.odt {what}：宣告加密也必須回 Err，實際回 Ok"),
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("加密不支援"),
                    "{what}：無演算法宣告時仍必須報加密不支援，實際: {msg:?}"
                );
                assert!(
                    msg.contains("未宣告"),
                    "{what}：演算法未宣告時訊息必須寫「未宣告」，實際: {msg:?}"
                );
                for banned in ["Blowfish", "AES256"] {
                    assert!(
                        !msg.contains(banned),
                        "{what}：未宣告演算法不得捏造名 {banned}，實際: {msg:?}"
                    );
                }
                assert!(
                    format!("{e:?}").contains("Encrypted"),
                    "{what}：仍須落在 Encrypted 變體，實際 Debug: {e:?}"
                );
            }
        }
    }

    // ============ 燈 5：meta.xml-only 加密——投影不受影響（禁止過度拒收） ============
    let bytes = read_fixture("encrypted_meta_only.odt");
    let bundle = OdfBundle::open(&bytes).expect("encrypted_meta_only.odt 容器層開檔");
    let body = bundle
        .extract_body()
        .expect("meta.xml 加密不得阻擋明文 content.xml 的內文抽取");
    assert_ok_body(&body, "encrypted_meta_only.odt extract_body");
    let proj =
        quire_odf::project(&bundle).expect("meta.xml 加密不得阻擋投影（消費面不含 meta.xml）");
    assert_eq!(
        proj.paragraph_count(),
        3,
        "encrypted_meta_only.odt：投影段落數應為 3"
    );

    // ============ 燈 6：styles.xml-only 加密——投影必紅、內文抽取照常（雙軌極性） ==
    let bytes = read_fixture("encrypted_styles_only.odt");
    let bundle = OdfBundle::open(&bytes).expect("encrypted_styles_only.odt 容器層開檔");
    assert_encrypted_error(
        quire_odf::project(&bundle),
        "encrypted_styles_only.odt project",
        algo_aes,
    );
    let body = bundle
        .extract_body()
        .expect("styles.xml 加密不應阻擋不吃樣式的內文抽取");
    assert_ok_body(&body, "encrypted_styles_only.odt extract_body");
}
