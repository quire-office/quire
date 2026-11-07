# corpus-odt（ODT 語料庫 50 檔＋合成 fixture）

真實世界 ODT：LibreOffice headless 自既有 `corpus/` DOCX 轉出 40 檔＋上游公開授權
可查證的真實 ODT 10 檔。逐檔來源、授權、取得方式、sha256 登記於
`manifest.json`；授權／完整性查證不過者進 `quarantine.json` 不入正庫
（採集器：`scripts/odf_build_corpus.py`；普查器：`scripts/odf_survey.py`，
產出 `docs/odf-survey/`）。

## 加密樣本兩檔（預期行為，非語料瑕疵）

- `ext/lo_core_sw_qa_odfexport_sha1_correct.odt`
- `ext/lo_core_sw_qa_odfexport_sha1_wrong.odt`

兩檔是 LibreOffice 上游 `sw/qa/extras/odfexport/data/` 的**真加密樣本**（檔名
correct／wrong 指 SHA1/1K 摘要是否吻合，非檔案損毀）：

- `META-INF/manifest.xml` 對 5 個條目宣告 `manifest:encryption-data`
  （`Blowfish CFB`＋`PBKDF2`／`SHA1/1K` checksum）；zip 完整性檢查 OK。
- `content.xml` 等條目實為密文，**無密碼**——LibreOffice 亦拒開
  （`Error: source file could not be loaded`，LO 25.8 實測）。**此為預期行為，
  不是語料瑕疵，也不是比較器缺陷**，日後巡檢勿再回報成「ODT 打不開／文字遺漏」。
- 我方引擎口徑（ODF-E1）：容器層照常開檔、無損 round-trip、骨架與 mimetype
  歸類照舊；消費層（`extract_body`／`project`／`quire-cli odt-render`）必須報
  `OdfError::Encrypted`（訊息含演算法名）、CLI 非零退出。詳見
  [`docs/odf-e1-redlight.md`](../docs/odf-e1-redlight.md)。
- 考卷：`core/odf/tests/odf_encrypted_gate.rs`（燈 1／2）、
  `shells/native/tests/odt_render_encrypted.rs`。

## 合成加密 fixture（fixtures/，不計入 50 檔正庫）

`encrypted_aes256.odt`（演算法名 AES256.GCM：證明訊息來自資料）／
`encrypted_no_algorithm.odt`（無 algorithm 子元素：訊息寫「未宣告」、不捏造）／
`encrypted_meta_only.odt`（僅 meta.xml 加密：消費面不得過度拒收）／
`encrypted_styles_only.odt`（僅 styles.xml 加密：投影必紅、內文抽取照常）。
產生器 `scripts/gen_odf_encrypted_fixture.py`（重跑 byte 級冪等，sha256 見
`manifest.json` fixtures 欄）。用途僅限考卷鑑別力自證，不得當成真實語料。
