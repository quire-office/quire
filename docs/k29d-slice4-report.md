# K29d（Slice 4）收案報告：編輯 API M6 節屬性／分節符／頁首頁尾／註腳尾註家族＋SubDocTarget 完整路由

執行者：ow-core。基線 `e11ff88`（K29c 已入庫 main）。工作分支 `wt/t_11642f18`，嚴禁 push，
合併與 main 署名由 PM 執行。設計藍本 `docs/edit-api-design.md` §6.5。

## 交付物

- `core/engine-api/src/mutation.rs`：`Mutation` enum append-only 新增 M6.1–M6.4 共七變體
  （`SetSectionProperties`／`InsertSectionBreak`／`DeleteSectionBreak`／
  `ConfigureHeaderFooterSlot`／`InsertNote`／`DeleteNote`／`SetNoteProperties`），
  並在 `apply_mutation` 入口以 `is_k29d_mutation` 分流至獨立實作檔（K29c 分檔慣例；
  Slice 1/2/3 主幹既有變體行為零改動，diff 為純新增＋cargo fmt 對既有兩處的行折疊）。
- `core/engine-api/src/mutation_k29d.rs`（新檔，約 1500 行）：七變體實作＋非 Body 目標的
  `SubDocTarget` 文字路由（`InsertText`／`DeleteRange` 子樹同等實作）。
- `core/engine-api/src/lib.rs`：公開斷言面 `part()`／`part_names()`／`index_sync_json()`；
  `rebuild_derived()` 補 bundle 基準保真（見「底座修正」節）。
- `core/engine-api/tests/k29d_section_hf_notes.rs`（新檔）：四盞紅燈考卷共 11 test。
- `scripts/k29d_buggy_replay.sh`（新檔）：buggy／good 兩態回塞鑑別，字串就地置換、不碰 git 歷史。
- `docs/k29d-slice4-report.md`（本檔）。

範圍凍結紀律：M6 變體集合就此封頂，未自加新變體、未擴充既有變體語意。

## 先紅後綠實跑

### 實作前紅燈（考卷先行，`0e0bb4f` WIP1 態，同一 commit 實跑两次一致）

```
failures:
    k29d_lamp1_orientation_margins_and_writeback_without_sectpr
    k29d_lamp1_section_type_and_honest_rejects
    k29d_lamp2_configure_slots_minimal_touch
    k29d_lamp2_footer_and_note_body_slices_reuse_m1
    k29d_lamp2_header_text_routes_to_header_part_only
    k29d_lamp3_insert_note_anchors_to_footer_and_undoes_byte_identical
    k29d_lamp4_insert_section_break_keeps_four_index_sets_in_sync

test result: FAILED. 4 passed; 7 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.53s
error: test failed, to rerun pass `-p quire-engine --test k29d_section_hf_notes`
```

七條 FAILED 的原始斷言訊息（逐條留檔，非追述）：

1. `lamp1_orientation…`（考卷線 322）：`橫向必須寫回 pgSz w/h 互換（twip＝pt×20），實得  w:w="16840" w:h="11900" w:orient="landscape"/`
   ＝實作已寫出 16840/11900、斷言钉的是 16838/11906（PM 裁決 1：斷言錯）。
2. `lamp1_section_type…`（線 434）：`未知節 id 的拒絕型別必須是 SubDocNotFound，實得 XmlPatchRejected { detail: "本檔無任何 <w:sectPr>，查無 section_id=99" }`
   ＝實作偏航（PM 裁決 4：改實作）。
3. `lamp2_configure_slots…`（線 125）：`紅燈 2 停 titlePg：whitelisted_changed 必須恰為 ["word/document.xml"]  left: []`。
4. `lamp2_footer…`（線 530）：`頁尾 InsertText 失敗: XmlPatchRejected { detail: "首 run 解析失敗" }`。
5. `lamp2_header…`（線 451）：`hf_table_header.docx 頁數基準 2  left: 3  right: 2`
   （PM 裁決 2：釘 3→3）。
6. `lamp3_insert_note…`（線 762）：`新註腳 id 必須存在`（考卷輔助 `max_footnote_id` off-by-one）。
7. `lamp4_insert_section_break…`（線 1011）：`para_sec 長度必須恆等於段落數  left: 14  right: 13`
   （PM 裁決 3：改斷言為 base+1，實作零讓步）。

### 中途態（逐里程碑）

- 本輪接手首跑（同 `0e0bb4f`）：`4 passed; 7 failed`。
- PM 三項裁決落地＋`enclosing_run` 雙字面修正後：`6 passed; 5 failed`。
- 五項 work 清單收掉後：`10 passed; 1 failed`（新建 part 落 `added` 非 `whitelisted_changed`）。
- GREEN 態：`test result: ok. 11 passed; 0 failed`（`cargo test -q -p quire-engine --test k29d_section_hf_notes`）。

## BUGGY 回塞兩態（`scripts/k29d_buggy_replay.sh` 實跑）

`buggy` 態（四處瑕疵全部成功置換、考卷必紅）：

```
[FLAG_P2_BUGGY_HOST_PARA] 成功置換
[FLAG_P3_BUGGY_FIELD_POLLUTION_INJECT] 成功置換
[FLAG_P4_BUGGY_REJECT_TYPE] 成功置換
[FLAG_P5_BUGGY_BUNDLE_BASELINE] 成功置換
test result: FAILED. 7 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out
BUGGY 態測試結束，exit_code=101（預期非 0）
```

四條必紅落點（`/tmp/k29d_buggy_final.log`）：

| 瑕疵 | 注入內容 | 必紅考卷 |
| :--- | :--- | :--- |
| P2 | 插分節符不新增宿主段 | `k29d_lamp4_insert_section_break_keeps_four_index_sets_in_sync` |
| P3 | `cp=0` 回到「首文字節點所在 run 之前」注入（踩進 fldChar 複雜欄） | `k29d_lamp2_footer_and_note_body_slices_reuse_m1` |
| P4 | 未知節 id 退回 `XmlPatchRejected` | `k29d_lamp1_section_type_and_honest_rejects` |
| P5 | `rebuild_derived` 不還原 bundle 基準 | `k29d_lamp3_insert_note_anchors_to_footer_and_undoes_byte_identical` |

`good` 態（還原全綠）：

```
[FLAG_P2_GOOD_HOST_PARA] 成功置換
[FLAG_P3_GOOD_INJECT_POINT] 成功置換
[FLAG_P4_GOOD_REJECT_TYPE] 成功置換
[FLAG_P5_GOOD_BUNDLE_BASELINE] 成功置換
test result: ok. 11 passed; 0 failed
GOOD 態測試結束，exit_code=0
```

鑑別力誠實聲明：另有一處「touched 自動退場失效」瑕疵經試跑證明對本考卷**不可觀測**
（`whitelisted_changed` 只記 `data_raw` 實際不同的 entry，淨零變動態好壞同果），故**不入腳本**，
避免虛報鑑別力；該紀律由燈 3 的 `undo → byte_identical` 與 k29a 紅燈 3 間接守衛。
四態瑕疵各自只打掉一盞燈（未見连带多紅，鑑別點互不污染）。

## 閘門實跑摘要

| 閘門 | 結果 |
| :--- | :--- |
| `cargo fmt --all --check` | `FMT_OK`（exit 0） |
| `cargo clippy --workspace --all-targets -- -D warnings` | `Finished dev profile`，exit 0 |
| `cargo test --workspace` | exit 0，全表 `test result: ok`，零 `FAILED`（全量輸出留存 `/tmp/k29d_ws_test.log`） |
| `cargo test -q -p quire-engine --test k29d_section_hf_notes` | `11 passed; 0 failed` |
| `cargo run -q -p quire-cli -- roundtrip corpus --check` | `399 檔（另有 1 檔隔離不計紅），byte-identical 399，失敗 0` |
| `cargo run -q -p quire-layout --bin goldens` | `shaping.jsonl 相符（12 例）／pagination.jsonl 相符（4 例）／tables.jsonl 相符（5 例）` |
| `cargo check -q --target wasm32-unknown-unknown -p quire-ooxml -p quire-layout -p quire-render-trait -p quire-crdt -p quire-engine -p quire-odf` | exit 0 |
| `bash scripts/check-layering.sh` | `✓ 分層檢查通過` |
| `bash scripts/k29d_buggy_replay.sh buggy` | `7 passed; 4 failed`，exit 101（預期非 0） |
| `bash scripts/k29d_buggy_replay.sh good` | `11 passed; 0 failed`，exit 0 |

## 考卷斷言修正記帳（全部發生在 k29d 新檔內，PM 逐項裁決授權；既有考卷斷言一個未改）

1. **燈 1 橫向 pgSz：`16838/11906` → `16840/11900`**（PM 裁決 1）。依據：該 lamp 開
   `fixtures/sample.docx`，PM 與執行者雙實查該檔零 `sectPr` 零 `styles.xml`，生效幾何是
   `LayoutParams::default()` 的整數 A4 595×842pt（`core/layout/src/lib.rs:352`），橫向翻轉即
   16840/11900。實跑 TRACE 證明引擎本來就寫出 16840/11900＋`w:orient="landscape"`——實作對、斷言錯。
   **另卡議題記帳**：`16838/11906` 是 Word 真 A4（210mm）的 twip 精度值；本引擎預設頁寸採整數 pt，
   與 Word twip 精度差 **2/6 twips**，屬「預設頁寸精度」另卡議題，本卡嚴禁為湊斷言改引擎預設
   （會動到全庫所有無 `sectPr` 檔的分頁落點）。
2. **燈 2 頁數基準：`page_count()==2` → 釘死 `3`（編輯前 3、頁首路由編輯後仍 3）**（PM 裁決 2）。
   金燈 `k9_hf_align.rs` 的 `>=2` 是寬鬆基準、不敵對；本卡要鎖「編輯零擾動」，釘死實測基線 3
   比 `>=2` 強。燈內座標斷言一律只盯第 0／第 1 頁，不受頁數影響。嚴禁反改引擎湊 2 頁。
3. **燈 4 `para_sec` 長度：`== base(13)` → `== base+1(14)`**（PM 裁決 3，採「新增宿主页」口徑）。
   `InsertSectionBreak` 插入自帶 `pPr/sectPr` 的新承載段，段落數 base+1 為正確行為
   （python-docx `add_section()` 生態慣例；掛既有段會讓無辜段落悄然變成節收束點，違反編輯
   API 非破壞性優先）。ECMA §17.2.3 只要求宿主為真實段落，兩口徑皆合規。斷言口徑同步改為
   「插入後 `para_sec.len()`＝插入後段落數＝base+1」＋「其後段落索引整體 +1（12→13）」＋
   「`DeleteSectionBreak` 後全部回到 base」。**實作零讓步**（未把 14 往 13 湊）。
4. **燈 1 未知節 id 拒絕型別：不動斷言，改實作**（PM 裁決 4）。各入口把 `section_id`／`note_id`
   查不到的路徑統一映射 `MutationError::SubDocNotFound`。取捨記帳：`SubDocTarget` 沒有「裸節 id」
   變體，錯誤承載體借 `Header{section_id, slot: Default}` 當診斷載體（型別正確性重於承載體美化）。
5. **燈 2 停 `titlePg` 的觸碰面斷言：`["word/document.xml"]` → `[]`＋`byte_identical` 斷言**。
   這是 PM 工作清單第 3 項（「查 touched/whitelist 語意」）的調查結論，非擅自放寬：Slice 1 主幹
   有「touched 自動退場」紀律（`mutation.rs:2593` 與 k29a 紅燈 3 同口徑），on→off 淨零變動 ⇒
   該 part 必須退出白名單、存檔回到原檔。原斷言把「碰過」與「最終內容不同」混為一談，屬考卷
   口徑錯誤。「off 真的移除旗標」由同燈的 `!doc.contains("<w:titlePg")` 獨立鎖住，不靠觸碰面背書。
6. **`assert_touch_face` 口徑：`whitelisted_changed` → `whitelisted_changed ∪ added`**。
   新建 part（`word/header2.xml`）在原檔無 entry，`diff_zip_structure` 把它記進 `added` 而非
   `whitelisted_changed`（`core/ooxml/src/diff.rs:216` 只對既有 entry 填 `whitelisted_changed`）。
   兩者同屬「本次編輯造成的內容變動」，故併入比較；其餘 entry 仍逐一 `zero_loss()` 硬鎖。
7. **考卷輔助 `max_footnote_id` off-by-one 修正**：`w:id="` 長度是 6，值起點原寫 `pos+7` 吃掉值首
   字元 ⇒ 解析恆失敗 ⇒ 燈 3 誤報「新註腳 id 必須存在」。實作側 `InsertNote` 從未失敗（探針實據：
   `PROBE_AFTER_NOTE mark=4 ids=1`，新註體已落地 `footnotes.xml`）。
8. **燈 2「正文首行 y」改為按正文標記字串過濾**（新增 `body_marker_min_y`）。原 `first_body_y` 取
   全頁最小 y，在「頁首含文字」的檔會先撞上頁首文字項（本引擎顯示列表不帶頁首／正文分帶標籤；
   k7b 金燈的 Bug51170 頁首是圖才無此誤撞）。推下鎖必須盯正文行本身——實測基準態 p1 正文首行
   72.0pt、頁首插字增高後 324.0pt（下推 252pt），k7b 口徑成立。

## 底座修正（必須动 `lib.rs` 的取捨，白名單內「僅必要欄位同步面」的從寬解釋，老實標記）

`rebuild_derived()` 於派生重建後把 `fresh.doc.bundle` 換回開檔基準 bundle。根因（探針實據，
非猜測）：`from_docx_bytes(&saved)` 的 bundle 是「編輯中途存檔件」，被編輯過的 entry 在裡頭是
引擎**重新 deflate** 的壓縮流；一旦覆蓋进来，`undo()` 把 parts 還原成開檔基線後，`OoxmlDoc::save()`
仍判該 entry「與 entry 明文不同」而再次重建壓縮流 ⇒ 全檔 `byte_identical` **假性失敗**
（實測：`mem_eq_orig=true`、`saved_eq_orig=true`、`touched` 已退場，`byte_identical=false`）。
此路徑是 K29b 起就存在的底座缺陷，只是 K29b/K29c 的考卷未把「跨 rebuild 的 undo → byte_identical」
當硬鎖而未被觸發；本卡燈 3 首次硬鎖即打紅。修正為純基準保真（未觸碰 entry 的逐位元組搬運依據
永不污染），全庫 `cargo test --workspace` exit 0、roundtrip corpus 399/399 零回歸為其安全性證據。

同輪修復的主幹級 bug（`mutation_k29d.rs` 內，不影響既有行為）：`enclosing_run` 開標籤有
`<w:r>` 與 `<w:r attrs…>` 兩種字面，舊寫法 `head.rfind("<w:r>")?.max(head.rfind("<w:r ")?)` 用 `?`
讓單邊 `None` 短路成整段解析失敗 ⇒ hf_table_header 的 `footer1.xml`（首段全是無屬性 `<w:r>`）
判「首 run 解析失敗」。

`SubDocTarget` 文字插入的 `cp=0` 注入點由「首文字節點所在 run 之前」改為「段落起點（`pPr` 之後）」：
fldChar 複雜欄（`begin→instrText→separate→結果 run→end`）段內，前者把新 run 塞進 `separate`～
`end` 之間，會被發射層欄位代換一併覆寫成頁碼（實測「頁尾覆核」變 `2`，即燈 2 的 P3 瑕疵態）。
段首注入在欄位結構之外，與 Word 游標段首語意一致且零欄位污染。

## 設計書偏差與未做事項（老實寫明，不虛構湊數）

1. **`SubDocTarget::TextBox` 仍未開放寫入**：`resolve_subdoc_part` 對 TextBox 回
   `SubDocNotFound`（誠實拒絕）。卡面「把 Header/Footer/Footnote/Endnote/TextBox 補齊成完整可寫
   子樹路由」中的 **TextBox 一項未達成**——`<w:txbxContent>` 的段落座標系與宿主段索引映射需要
   獨立的子樹段落點設計，塞進本片會超出可動面（要动發射層的框內投影）。建議另卡。
2. **頁首頁尾 slot 解析只走 body 級末節 `sectPr`**：`resolve_hf_part_exact` 取
   `all_sectprs().last()`（body 級），**未走節級 `headerReference` 繼承鏈**（ECMA §17.2 的
   「前節未宣告即繼承前前節」）。單節檔與「末節自宣告」檔完全正確（本卡考卷全在此態）；多節檔
   若第 0 節宣告頁首、末節未宣告，寫入會落在末節而非繼承來源。屬實裝簡化，非假裝完成：
   函式名 `resolve_hf_part_exact` 與註解已明載「不走兜底繼承」。
3. **`SetNoteProperties` 只寫文件級 `settings.xml` 的 `w:footnotePr`／`w:endnotePr`**，節級覆寫
   （`sectPr/footnotePr`）未做（設計書 §2.3.7 節級路徑留後續片）。`NotePropsPatch.pos` 欄位
   **未實裝**：只給 `pos` 的 patch 會通過 `is_empty()` 檢查卻產生實質 no-op（不寫任何屬性），
   且 touched 自動退場使存檔保持 `byte_identical`——誠實無效，但未如實回 `XmlPatchRejected`，
   屬本片的已知缺口，建議另卡補拒絕型別。
4. **`DeleteSectionBreak` 只拆段落級 `sectPr`**；根級 body 尾 `sectPr` 恆回
   `ProtectedStructure`（§2.3.6-2），宿主段拆掉 `sectPr` 後若殘留空段 ⇒ 整段移除（與插分節互為逆、
   索引回穩），有內容的段 ⇒ 只拆 `sectPr`（段落保留）。
5. **解除頁首／頁尾綁定（`HfBindAction::Unbind`）不刪 part 本體與 rels／CT 註冊**（誠實邊界：
   解除綁定 ≠ 刪 part）。逆補丁仍可完整還原綁定，屬可逆。
6. **`InsertSectionBreak` 不保證新節頁數與 LO 對齊**：卡面明文「分頁結果頁數與 LO 對齊由考卷內
   斷言既有 golden 不動負責」，本片考卷只斷言自家顯示列表自洽（分節後頁數增加、四套索引與
   存檔重開乾淨投影逐位相同）。既有 golden（k7b／r7 全檔守門）經 `cargo test --workspace` 全綠。
7. **無 `sectPr` 文檔的合成件採開檔生效幾何**（`synthesized_sectpr`）：首寫零視覺位移（燈 1 以
   「top 邊距未變時正文首行 y 不移位 ±0.6pt」鎖住），但合成值是整數 pt 口徑（見斷言記帳 1 的
   精度另卡議題）。

## 不可逆邊界

本片七變體全數宣告 `ReversibilityLevel::StrictByteReversible`，**無 Irreversible 路徑**、無 Undo
堆疊截斷。逆補丁採「整 part 原文快照」（`XmlReversePatch.original_slice`），Undo 將 part 完整
還原為變動前位元組；`ConfigureHeaderFooterSlot` 新建的 part 走 `PartLifecycleInverse::RemoveSynthesizedPart`
在 Undo 時整份移除（不進 `part_baselines`，移除後自然回到開檔 part 集合）。燈 3（插註腳＋編註文
→ `undo()` ×2 → 存檔全檔 `byte_identical`）與燈 4（插分節→刪分節→`undo()` ×2 → `byte_identical`）
為其實測證據。未觸碰 entry 的零失真由 `diff_zip_structure` 白名單外判準守住，roundtrip corpus
399/399 byte-identical 未回歸。

## 收尾紀律

- `core/engine-api/tests/k29d_debug.rs`（145 行除錯 scaffolding）與臨時探針檔
  `k29d_probe_tmp.rs` 已刪除；`mutation_k29d.rs` 的除錯鉤 `dbg_locate` 與 `eprintln!("TRACE_SYNTH=…")`
  已移除（grep 實據：`core/engine-api/src/` 零 `println!/eprintln!` 殘留）。
- 既有考卷斷言一個未改（`git diff --stat` 對 `core/engine-api/tests/` 僅新增本卡考卷檔）。
- 禁動面零觸碰：`core/odf/`、`core/layout/`、`shells/`、`corpus*/`、`fixtures/` 既有檔、
  `mutation_k29c.rs`。
- git：僅在 `wt/t_11642f18` 分支 commit 保護（ADT109119 署名），未 push、未 checkout/reset/rebase/stash/clean。
