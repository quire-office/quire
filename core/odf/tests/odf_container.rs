//! ODF 容器層第一切片考卷（ODF-C2 卡）。
//!
//! 考卷要求：
//! 1. 50 檔全讀：corpus-odt 全量 50 檔皆能成功開啟。
//! 2. 50 檔 roundtrip byte-identical：無修改存回逐位元組相同。
//! 3. 3 違例檔歸類正確：tdf140437/152710/77961 標記為非首條目，其餘 47 檔合規。
//! 4. 段落總數與 survey 一致：50 檔總段落數實測 19687 段，逐檔核對 survey.json。
//! 5. 骨架與 XML 健康度掃描：掃描核心 XML 串流存在性與語法健全度。

use quire_odf::{MimetypeStatus, OdfBundle, OdfError};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-odt")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn manifest_files() -> Vec<String> {
    let p = corpus_root().join("manifest.json");
    let content = std::fs::read_to_string(&p).expect("讀取 manifest.json 失敗");
    let json: Value = serde_json::from_str(&content).expect("解析 manifest.json 失敗");
    let files = json["files"]
        .as_array()
        .expect("files 欄位應為陣列")
        .iter()
        .map(|f| f["file"].as_str().expect("file 欄位應為字串").to_string())
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 50, "語料庫應恰有 50 檔");
    files
}

fn read_file(rel: &str) -> Vec<u8> {
    let path = corpus_root().join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("讀取檔案失敗 {rel}: {e}"))
}

/// 考卷 1：50 檔全讀
#[test]
fn test_corpus_50_files_open_all() {
    let files = manifest_files();
    for rel in &files {
        let bytes = read_file(rel);
        let bundle = OdfBundle::open(&bytes).unwrap_or_else(|e| panic!("{rel} 開啟失敗: {e}"));
        assert!(
            bundle.part_names().contains(&"content.xml".to_string()),
            "{rel} 必須包含 content.xml"
        );
    }
}

/// 考卷 2：50 檔 roundtrip byte-identical
#[test]
fn test_corpus_50_files_roundtrip_byte_identical() {
    let files = manifest_files();
    for rel in &files {
        let bytes = read_file(rel);
        let bundle = OdfBundle::open(&bytes).unwrap_or_else(|e| panic!("{rel} 開啟失敗: {e}"));
        let saved = bundle
            .save()
            .unwrap_or_else(|e| panic!("{rel} 存檔失敗: {e}"));
        assert_eq!(
            saved.as_slice(),
            bytes.as_slice(),
            "{rel}：未修改存檔必須與原始位元組逐位元組相同"
        );
    }
}

/// 考卷 3：3 違例檔歸類正確
#[test]
fn test_mimetype_violation_classification() {
    let files = manifest_files();
    let expected_violations: HashSet<&str> = [
        "ext/lo_core_sw_qa_odfexport_tdf140437.odt",
        "ext/lo_core_sw_qa_odfexport_tdf152710.odt",
        "ext/lo_core_sw_qa_odfexport_tdf77961.odt",
    ]
    .into_iter()
    .collect();

    let mut detected_violations = HashSet::new();

    for rel in &files {
        let bytes = read_file(rel);
        let bundle = OdfBundle::open(&bytes).unwrap_or_else(|e| panic!("{rel} 開啟失敗: {e}"));
        let status = bundle.mimetype_status();

        if expected_violations.contains(rel.as_str()) {
            assert!(
                status.is_not_first(),
                "{rel} 應被歸類為非首條目違例檔，實際狀態為: {:?}",
                status
            );
            assert!(!status.is_compliant(), "{rel} 不得標記為合規");
            detected_violations.insert(rel.clone());
        } else {
            assert!(
                status.is_compliant(),
                "{rel} 應為合規檔案，實際狀態為: {:?}",
                status
            );
        }
    }

    assert_eq!(detected_violations.len(), 3, "必須精確抓出三檔違例檔");

    // 測試合成 fixture 檔
    let fixtures_dir = corpus_root().join("fixtures");
    if fixtures_dir.exists() {
        // 1. mimetype_not_first.odt
        let p_not_first = fixtures_dir.join("mimetype_not_first.odt");
        if p_not_first.exists() {
            let bytes = std::fs::read(&p_not_first).unwrap();
            let b = OdfBundle::open(&bytes).expect("開啟 mimetype_not_first.odt 成功");
            assert!(b.mimetype_status().is_not_first());
        }

        // 2. mimetype_compressed.odt
        let p_comp = fixtures_dir.join("mimetype_compressed.odt");
        if p_comp.exists() {
            let bytes = std::fs::read(&p_comp).unwrap();
            let b = OdfBundle::open(&bytes).expect("開啟 mimetype_compressed.odt 成功");
            assert!(matches!(
                b.mimetype_status(),
                MimetypeStatus::Compressed { .. }
            ));
        }

        // 3. missing_manifest_xml.odt
        let p_no_man = fixtures_dir.join("missing_manifest_xml.odt");
        if p_no_man.exists() {
            let bytes = std::fs::read(&p_no_man).unwrap();
            let res = OdfBundle::open(&bytes);
            assert!(
                matches!(res, Err(OdfError::MissingManifest)),
                "缺 manifest.xml 必須報錯 MissingManifest"
            );
        }
    }
}

/// 考卷 4：段落總數與 survey 一致（實測 19687 段）
#[test]
fn test_paragraph_count_matches_survey_19687() {
    let survey_path = repo_root().join("docs/odf-survey/survey.json");
    let content = std::fs::read_to_string(&survey_path).expect("讀取 survey.json 失敗");
    let json: Value = serde_json::from_str(&content).expect("解析 survey.json 失敗");

    let files_rec = json["files"].as_array().expect("files 欄位應為陣列");

    let mut total_paragraphs = 0usize;
    let mut non_empty_text_files = 0usize;

    for rec in files_rec {
        let rel = rec["file"].as_str().expect("檔案路徑");
        let expected_p = rec["content"]["text_p"].as_u64().unwrap_or(0) as usize;

        let bytes = read_file(rel);
        let bundle = OdfBundle::open(&bytes).unwrap_or_else(|e| panic!("{rel} 開啟失敗: {e}"));

        // ODF-E1 口徑：manifest 宣告 content.xml 加密者，內文抽取必須明確報
        // 加密不支援（Err），不得靜默回零段——計數貢獻 0，但走的是錯誤軌。
        if bundle.manifest().is_encrypted("content.xml") {
            let err = bundle.extract_body().err().unwrap_or_else(|| {
                panic!("{rel}：加密 content.xml 必須回 Err（ODF-E1），不得靜默零段")
            });
            assert!(
                matches!(err, OdfError::Encrypted { .. }),
                "{rel}：加密條目必須落在 OdfError::Encrypted 明確變體，實際: {err:?}"
            );
            continue;
        }

        let body = bundle
            .extract_body()
            .unwrap_or_else(|e| panic!("{rel} 抽取內文失敗: {e}"));

        assert_eq!(
            body.paragraphs.len(),
            expected_p,
            "{rel}：段落數應與 survey.json 一致"
        );
        assert_eq!(
            body.text_p_count, expected_p,
            "{rel}：text_p_count 應與 survey.json 一致"
        );

        total_paragraphs += body.paragraphs.len();
        if body.total_text_len > 0 {
            non_empty_text_files += 1;
        }
    }

    assert_eq!(total_paragraphs, 19687, "50 檔段落總數必須恰好等於 19687");
    assert!(non_empty_text_files > 40, "絕大多數檔案應有文字總量");
}

/// 考卷 5：骨架與 XML 健康度掃描
#[test]
fn test_skeleton_and_xml_health() {
    let files = manifest_files();
    for rel in &files {
        let bytes = read_file(rel);
        let bundle = OdfBundle::open(&bytes).unwrap_or_else(|e| panic!("{rel} 開啟失敗: {e}"));
        let skel = bundle.scan_skeleton();

        assert!(skel.has_content_xml, "{rel} 應具備 content.xml");
        assert!(skel.has_manifest_xml, "{rel} 應具備 manifest.xml");

        // 若非加密檔，content.xml 應為健全 XML
        if !bundle.manifest().is_encrypted("content.xml") {
            assert!(
                skel.content_xml_well_formed,
                "{rel} 未加密之 content.xml 應為健全 XML"
            );
        }
    }
}
