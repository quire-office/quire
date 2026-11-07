# ODF-E1 紅燈紀錄（加密 ODT 明確報「加密不支援」）

考卷：
- `core/odf/tests/odf_encrypted_gate.rs`（引擎面六盞燈，BUGGY 門控）
- `shells/native/tests/odt_render_encrypted.rs`（CLI 面：非零退出＋stderr 演算法名）
- 合成 fixture：`corpus-odt/fixtures/encrypted_{aes256,no_algorithm,meta_only,styles_only}.odt`
  （產生器 `scripts/gen_odf_encrypted_fixture.py`，固定 date_time／壓縮位元，重跑 byte 級冪等，
  sha256 已登記 `corpus-odt/manifest.json` fixtures 欄）

## 一、卡面前提實測（PM 開檔可核對）

兩檔 `corpus-odt/ext/lo_core_sw_qa_odfexport_sha1_correct.odt`／`…_sha1_wrong.odt`：

- `META-INF/manifest.xml` 對 5 個條目（styles/settings/meta/content/manifest.rdf）宣告
  `manifest:encryption-data`，`checksum-type="SHA1/1K"`、`algorithm-name="Blowfish CFB"`、
  `key-derivation-name="PBKDF2"`（iteration-count=100000）。實測 `unzip -p … | grep -c
  '<manifest:encryption-data '` 每檔 5 處。
- zip 完整性：`unzip -t` 兩檔皆 `No errors detected`（非損毀檔，卡面所述屬實）。
- `content.xml` 實測為密文：`unzip -p … content.xml | head -c 16 | xxd` 無 XML 標頭
  （correct 首 bytes `24 4f 49 9c …`、wrong 首 bytes `79 c9 50 ff …`）。
- LO 真值（LO 25.8 headless，`-env:UserInstallation=file:///tmp/odfe1prof`）：兩檔皆
  `Error: source file could not be loaded`、零輸出檔——LO 無密碼亦拒開，卡面「此為預期」屬實。
- 舊口徑缺陷重現（修復前實抓）：`quire-cli odt-render` 兩檔皆
  `{"page_breaks":[],"paragraph_count":0,"total_text_len":0}` 且 `exit=0`。

## 二、紅燈實測輸出（修復前，`cargo test -p quire-odf --test odf_encrypted_gate`）

```
test odf_e1_encrypted_gate_suite ... FAILED
thread 'odf_e1_encrypted_gate_suite' panicked at core/odf/tests/odf_encrypted_gate.rs:44:18:
ext/lo_core_sw_qa_odfexport_sha1_correct.odt extract_body：加密文件必須回 Err（加密不支援），實際回 Ok——靜默空文件正是本卡要根除的缺陷

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

（燈 1 即紅：`extract_body` 舊實作把加密 content.xml 投成 `Ok(BodyModel::default())`。
燈序 1→6 依序紅，故一次一盞；紅燈 commit 先行入庫。）

## 三、綠燈實測輸出（修復後）

`cargo test -p quire-odf`（13 盞全綠，含既有 C2/C3 考卷零回退）：

```
odf_container.rs        5 passed; 0 failed
odf_encrypted_gate.rs   1 passed; 0 failed   ← odf_e1_encrypted_gate_suite
odt_layout_projection.rs 7 passed; 0 failed
```

`cargo test -p quire-cli --test odt_render_encrypted`：1 passed（含明文對照組 tdf152710
照常 `{"page_breaks":[24,38],"paragraph_count":49,"total_text_len":4854}`、退出 0）。

CLI 實抓（修復後）：

```
$ quire-cli odt-render …sha1_correct.odt
投影失敗 …sha1_correct.odt: 加密不支援：條目 content.xml 宣告加密（演算法 Blowfish CFB），本引擎不實作解密
exit=1
$ quire-cli odt-render fixtures/encrypted_no_algorithm.odt
投影失敗 …: 加密不支援：條目 content.xml 宣告加密（演算法 未宣告），本引擎不實作解密
exit=1
```

## 四、BUGGY 鑑別力證明（`ODF_E1_BUGGY=1`）

把考卷期望演算法名換成封裝中不存在的 `Twofish CFB`，燈 1 必紅 ⇒ 證明斷言比對的
演算法名真的來自 manifest 解析，而非恆真字串：

```
$ ODF_E1_BUGGY=1 cargo test -p quire-odf --test odf_encrypted_gate
test odf_e1_encrypted_gate_suite ... FAILED
…錯誤訊息應含 manifest 宣告的演算法名 "Twofish CFB"，實際: "加密不支援：條目 content.xml 宣告加密（演算法 Blowfish CFB），本引擎不實作解密"
test result: FAILED. 0 passed; 1 failed
```

正常態（無 env）同考卷綠，見 §三。BUGGY 讀取只在單一 test 函數內（k25b 紀律）。

## 五、實作口徑（消費面攔截，容器層不動）

- `OdfError` 新增明確變體：
  `Encrypted { full_path, algorithm }`，Display＝
  「加密不支援：條目 {full_path} 宣告加密（演算法 {algorithm}），本引擎不實作解密」。
- `ManifestEntry` 新增 `algorithm: Option<String>`（`manifest:algorithm-name` 資料），
  `Manifest::algorithm_of(path)` 查詢；缺位以 `ALGORITHM_UNSPECIFIED = "未宣告"` 入訊息
  ——不虛構鐵律：無宣告不得捏造演算法名。
- 攔截點按消費面（`OdfBundle::ensure_not_encrypted`，`pub(crate)`）：
  - `extract_body`：只閘 content.xml。
  - `project`：閘 content.xml＋styles.xml（字級級聯依賴樣式；styles 密文會讓級聯
    靜默降成 12pt 假值，故必須擋）。
  - meta.xml／settings.xml 加密**不擋**任何消費軌（fixture 燈 5／燈 6 鎖極性）。
- 容器層不動：`OdfBundle::open`／`save`／round-trip／`scan_skeleton`／mimetype 歸類
  全部照舊（survey「加密容器只登記不解析、不得當損壞檔」紀律；C2 考卷 1／2／3／5 零回退）。
- 直入純函數 `body::extract_body_from_bytes` 為第二道防線，口徑與 bundle 軌一致。
- `shells/native/src/main.rs` 不須改：odt-render 既有「開檔／投影失敗→非零退出」路徑
  直接承接新錯誤變體（CLI 考卷鎖行為，防日後被改回吞錯）。

## 六、既有考卷口徑同步（極性反轉處，逐條記）

- `odf_container.rs` 考卷 4（段落總數 19687）：加密兩檔改走「必須 Err(Encrypted)、
  計數貢獻 0」支路——總數 19687 不變（兩檔本來就計 0）。
- `odt_layout_projection.rs` 考卷 (a)（總段數 19935）：同上，加密兩檔改斷
  `Err(OdfError::Encrypted)`，不再斷 `Ok` 零段。
- 除上述兩處，`EXPECTED_COUNTS`／survey.json／baseline 腳本零觸碰。

## 七、語料端登記（卡面第 3 條）

`corpus-odt/README.md`（本卡新增）：兩檔標注為加密樣本（LO 端亦拒開），免日後被當成
語料瑕疵；四檔合成加密 fixture 一併登記（含 sha256 與用途）。

## 八、agy 外部 review（第二意見輪，gemini-3.8-flash-high 實跑）與處置

六條發現全數處置，逐條：

| # | 嚴重度 | 發現 | 處置 |
|---|--------|------|------|
| 1 | S2 | `project_bytes` 公有純函數入口無 Manifest 防線，密文直入可重現靜默零段 | 採方案 A：`project_bytes` doc 加「安全前置條件：呼叫端必須保證明文；持 `OdfBundle` 者嚴禁直呼，必經 `project` 閘門」契約註記（改簽名會破既有 C3 考卷入口，方案 B 記帳往後） |
| 2 | S2 | 四檔 fixture 二進位未入 diff，乾淨 checkout 考卷必 panic | 考卷 `read_fixture` 缺失時就地重跑產生器（byte 級冪等已證：移檔重跑 sha256 逐位元組復現 `eb52db…`）；四檔本體照常入 commit |
| 3 | S3 | 畸形 manifest（encryption-data 未閉合）可令 `in_encryption_data` 殘留污染後續條目 | `file-entry` Start 邊界防禦復位旗標（manifest.rs） |
| 4 | S3 | `EncryptedEntry` 成死變體 | 不刪：`docs/odf-design.md` §5.1／`docs/editor-sdk-design.md` 仍以其為「容器層攔截」契約文字——補 doc 註記兩變體口徑之別（容器層 vs 消費層），刪與否交 PM 對設計書定奪 |
| 5 | S3 | 考卷 (a) 錯誤軌判據只查 content.xml，與 project 雙閘口徑未對齊 | 判據改 `content.xml ∥ styles.xml` 雙查 |
| 6 | S3 | 燈 4 只測 extract_body 之「未宣告」，project 軌未對稱 | 燈 4 改雙軌對稱斷言 |

處置後全閘重跑見 §三（test 總數不變、BUGGY 門控仍紅）。
