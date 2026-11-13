# ODF-C2 容器層第一切片紅燈紀錄

## 測試環境
- 執行指令：`cargo test -p quire-odf --test odf_container`
- 工作目錄：`<repo>/.worktrees/agy-odfc2`
- 分支：`agy/odf-c2`
- 日期：2026-11-04

## 考卷設計
考卷 `core/odf/tests/odf_container.rs` 涵蓋任務卡五大驗收項：
1. `test_corpus_50_files_open_all`：全 50 檔容器開啟能力。
2. `test_corpus_50_files_roundtrip_byte_identical`：50 檔無損往返逐位元組相同驗證。
3. `test_mimetype_violation_classification`：3 檔違例檔（tdf140437/152710/77961）非首條目歸類正確性與合成 fixture 驗證。
4. `test_paragraph_count_matches_survey_19687`：段落計數與 survey.json 數字核對（總計 19687 段）。
5. `test_skeleton_and_xml_health`：核心 XML 存在性與語法健康度掃描。

## 紅燈輸出（先紅自證）

```text
     Running tests/odf_container.rs (target/debug/deps/odf_container-7479e99488892047)

running 5 tests
test test_corpus_50_files_roundtrip_byte_identical ... FAILED
test test_skeleton_and_xml_health ... FAILED
test test_mimetype_violation_classification ... FAILED
test test_corpus_50_files_open_all ... FAILED
test test_paragraph_count_matches_survey_19687 ... FAILED

failures:

---- test_corpus_50_files_roundtrip_byte_identical stdout ----

thread 'test_corpus_50_files_roundtrip_byte_identical' (43292) panicked at core/odf/tests/odf_container.rs:67:65:
ext/lo_core_sw_qa_odfexport_WordTest_edit.odt 開啟失敗: 缺少 content.xml
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- test_skeleton_and_xml_health stdout ----

thread 'test_skeleton_and_xml_health' (43295) panicked at core/odf/tests/odf_container.rs:218:65:
ext/lo_core_sw_qa_odfexport_WordTest_edit.odt 開啟失敗: 缺少 content.xml

---- test_mimetype_violation_classification stdout ----

thread 'test_mimetype_violation_classification' (43293) panicked at core/odf/tests/odf_container.rs:95:65:
ext/lo_core_sw_qa_odfexport_WordTest_edit.odt 開啟失敗: 缺少 content.xml

---- test_corpus_50_files_open_all stdout ----

thread 'test_corpus_50_files_open_all' (43291) panicked at core/odf/tests/odf_container.rs:53:65:
ext/lo_core_sw_qa_odfexport_WordTest_edit.odt 開啟失敗: 缺少 content.xml

---- test_paragraph_count_matches_survey_19687 stdout ----

thread 'test_paragraph_count_matches_survey_19687' (43294) panicked at core/odf/tests/odf_container.rs:180:65:
ext/lo_core_sw_qa_odfexport_WordTest_edit.odt 開啟失敗: 缺少 content.xml


failures:
    test_corpus_50_files_open_all
    test_corpus_50_files_roundtrip_byte_identical
    test_mimetype_violation_classification
    test_paragraph_count_matches_survey_19687
    test_skeleton_and_xml_health

test result: FAILED. 0 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

error: test failed, to rerun pass `-p quire-odf --test odf_container`
```
