# E-2b 交付報告：底線枚舉共型收尾——`UnderlineKind` 併入 render-trait `Underline`

任務：t_5e62f3b5（來源 E-2 報告 §6.1）。分支 `wt/t_5e62f3b5`，基線＝main tip
`8484404`（含 E-2 29053ce 與 U4e 合併；起手 `git worktree add -b wt/t_5e62f3b5 main`
自證）。繁中註解，署名 ADT109119，未 push。

## 1. 共型方案（卡面任務 1）

`mutation.rs` 的鏡像枚舉 `UnderlineKind`（K29e 五值）刪除，改為**別名**：

```rust
pub use quire_render_trait::Underline as UnderlineKind;
```

選別名而非直接刪符號的理由：`UnderlineKind` 是 `pub` 再出口符號
（`core/engine-api/src/lib.rs` re-export＋三張考卷 import），別名讓既有 import
路徑零斷裂；同時它是**同一型別**（非新type），編譯期構造器風暴把全部殘留舊態
用法逼出水面（本庫實炸四處：k29e 兩處 `UnderlineKind::None`→`Off`、e2_emit
燈 6 一處、`Underline` 非 `Copy` 導致 `Some(kind)` 移動錯誤兩處——皆語義同值換名
＋clone，斷言強度零動搖）。

三端同型後，`as_u_val()`（`w:u/@w:val` 檔契約字面）從 mutation.rs 私有助手上移
到 render-trait 單一真值源 `Underline::as_u_val() -> Option<&'static str>`，
補上鏡像枚舉漏接的 `Thick`（corpus 實見×5），`Raw(_)` 回 `None`。

**兩套字面不同家、禁互竄**（考卷逐字鎖死）：

| 形態 | serde 線契約（殼↔引擎） | 檔契約 `w:u/@w:val`（ECMA 正字） |
|---|---|---|
| Off | `"off"` | `"none"` |
| Single…Dashed | 小寫同字面 | 同字面 |
| Thick | `"thick"`（新通） | `"thick"`（新通） |
| Raw(s) | `{"raw":s}` | 不可寫（拒） |

### `Raw(_)` 寫入端語義定案（採卡面建議案：拒絕＋誠實報錯）

PM 未另裁示，依卡面「建議：寫入端拒絕 Raw 並誠實報錯」定案。兩層防線：

1. **入口閘門** `reject_unwritable_underline()`（mutation.rs）置於
   `apply_mutation` 最前、K29c/K29d 分流之前：`FormatTextRange` 與
   `UpsertNamedStyle` 兩臂（全庫 `patch_rpr` 寫入面的全部呼叫鏈，grep 坐實）
   共軌攔 `Some(Raw(_))` → `MutationError::XmlPatchRejected`（detail 指名被拒
   形態＋列出可寫形態）。拒絕在任何 XML 觸碰之前＝**零位元組位移**（考卷燈 3
   以 `document.xml` 逐位元組對照鎖死）。
2. **寫出臂二道防線**：`patch_rpr` 改吃 `u.as_u_val()`，回 `None`（即 Raw）＝
   **不寫**——防未來繞過入口的新呼叫點把 Raw 原字面直插 `w:val`（`"/><w:b/>`
   之流的 rPr 結構注入面，這是拒絕的硬性理由之一）。

拒絕的語義正當性：Raw 的繪製配方連發射端都還是 TODO（E-2 降級單線＋TODO），
寫入端無從替用戶編造未支援形態；Raw 只允許**讀取端**保形（wavy/words 從檔讀進
來照舊，發射降級單線，E-2 既有口徑零動搖）。serde 邊界對 Raw 照常讀寫不吞值
（燈 7）——拒絕只發生在引擎套用層，兩層分界考卷鎖死。

## 2. serde 線契約遷移（卡面任務 2；PM 紀律 3(b) 遷移期雙讀）

**破向後兼容顯式記帳**：`RunDirectPatch::underline` 的序列化輸出從鏡像枚舉的
變體名大寫（`"Single"|"Double"|"Dotted"|"Dashed"|"None"`）改為 canonical 小寫
（`"single"…|"off"`，`Raw` 外部標籤 `{"raw":"wavy"}`）——共型的定義性後果
（欄位型別即 `Underline`，serde 形即 `Underline` 既有形）。

**相容面（舊態 JSON 必有一盞反序列化相容燈）**：`Underline` 各變體加
`#[serde(alias = "Single"|"Double"|"Dotted"|"Dashed"|"None")]`——舊外殼 pkg 未
同步重建時發送的舊大寫 payload 繼續可反序列化（`"None"`→`Off` 逐值對映，不單邊
沉默丟值）。燈 4 逐值＋整件 `Mutation` payload 雙口徑鎖死。alias 表封閉：
非法字面（`"wavy"` 裸字串、`"SINGLE"` 大小寫錯位）必炸（燈 7）——雙讀只赦免
既成事實的舊態，不發明新常態。

**外殼同步（僅 1 檔，卡上預報 PM 逐檔清單如實兑现）**：
`shells/web/editor/js/engine-adapter.js`——`UNDERLINE_KINDS` 改 canonical 小寫
＋新增 `'thick'`（共型紅利：寫入端真通）；`normalizeUnderlineKind` 輸出 canonical
小寫、輸入端舊大寫/`dash`/`false`/`'0'` 全相容（`toLowerCase` 天然吞舊名）；
`formatUnderline` 切換預設 `'single'/'off'`；`clearDirectCharFormatting` 移除值
`'off'`；L264 欄位契約註解同步。`qword-core.js`／`index.html`／`qword-ui.js`
**零碰**（qword-core.js 只轉發 payload 不枚舉值域，線契約 consumption 在
adapter 一處，與活體卡 t_74458fe2/t_81e69488 檔案範圍互斥）。

## 3. 考卷（卡面任務 3；先紅後綠）

新卷 `core/engine-api/tests/e2b_underline_shared_type.rs` 7 盞：

| 燈 | 鎖什麼 | 修前紅態 |
|---|---|---|
| 1 | 編譯期共型：`Underline` 直接餵 `RunDirectPatch::underline`、別名互相指定、`as_u_val` 值域（含 Thick、Raw→None） | E0308/E0599 整卷編譯失敗（實錄 15 錯） |
| 2 | `Thick` 寫入端到端（寫入→存檔件 `w:val="thick"`→重解析→發射四環） | 鏡像枚舉無此變體 |
| 3 | Raw 誠實拒：`XmlPatchRejected`＋拒絕訊息指名形態＋**零位元組位移**＋拒絕後引擎健康＋`UpsertNamedStyle` 臂同款（無旁路）＋對照「serde 讀得進」兩層分界 | 鏡像枚舉無 Raw 可拒 |
| 4 | 舊態 JSON 相容（逐值＋整件 Mutation payload 舊大寫形） | n/a（遷移鎖） |
| 5 | canonical 新形鎖死（小寫六值＋`Raw` 標籤形＋序列化輸出**不得**再含大寫舊名） | n/a（遷移鎖） |
| 6 | canonical 全集寫讀閉環（六值逐值 寫入→存檔→重解析→發射；對照段逐位元組零波及）——PM 紀律 3(a)「燈 6 雛形擴全集」 | Thick 值鏡像態寫不出 |
| 7 | 線契約不吞值（Raw round-trip 保形）＋非法字面必炸 | n/a |

回歸基準：E-2 發射卷燈 6（`e2e_lamp6_write_read_roundtrip_kind_parity`）共型後
**仍綠**（卡面任務 3 明文；`UnderlineKind`→`Underline` 別名後逐值寫讀閉環語義
不變，僅 `None`→`Off` 換名＋clone，燈註解補 E-2b 記帳）。

BUGGY 回塞鎖 `E2B_UNDERLINE_BUGGY`（env 名稱 grep 現庫自訂；E-2 的
`E2_UNDERLINE_BUGGY` 為既有鎖、不相犯、可各自獨立翻）：`as_u_val` 把 `Thick`／
`Raw` 折回 single（鏡像枚舉語義）＋閘門同款放行。隨貨
`scripts/e2b_underline_buggy_replay.sh buggy|good`：buggy 態**恰 4 盞紅**
（燈 1/2/3/6——serde 三盞燈 4/5/7 不受 env 鎖管，正確保持綠，鑑別力不混同）；
good 態全綠＋`git status` 零殘留。

## 4. 驗證實錄

| 項 | 結果 |
|---|---|
| e2b 新卷 | 7/7 綠（先紅實錄：修前 15 個 E0308/E0599 compile-fail） |
| E-2 三卷（合併態） | kind 9／emit 14／k29e 15 全綠（燈 6 回歸基準在內） |
| e1_run_patch_merge／e_ppr_patch_merge | 17／8 綠（`patch_rpr` 共用面零回退） |
| 外殼真 wasm 閉環探針 | `shells/web/editor/tests/e2b_underline_shared_probe.mjs` 20/20（canonical thick/dashed 真通、舊大寫雙讀、wavy 外殼＋引擎雙層拒、切換語意 canonical） |
| BUGGY 回塞 | buggy 恰 4 盞紅／good 全綠零殘留 |
| 全閘 ow-gate.sh | 見 §5（合併態重跑） |
| 圓環 | roundtrip fixtures＋corpus 399/399 byte-identical（全閘內） |

## 5. 合併態

收案前 `git merge main` 進分支、合併態重跑全閘 fail=0（PM 發車紀律 1）。
本卡基線即 main tip，合併為快進確認（無漂移）。

## 6. 未做面／後續卡（老實記帳）

1. **`format:underline-style` 按鈕仍掛 pending**：`index.html` 的 title 與
   `qword-core.js` 的「誠實缺口」註解停在 U4b 時代（「形態不進 RunStyle」前提
   已被 E-2 推翻）。本卡白名單只申報了 engine-adapter.js 一檔，未順車翻 UI 標籤
   ——轉 live 屬 UI 閉環驗證（真瀏覽器逐鈕 click）範疇，建議另開 UI 卡：現況
   `formatUnderline('double'|'thick'|...)` 外殼通路已真通（探針 P1/P4 坐實），
   只欠 Ribbon 選單值域接上＋標籤誠實化＋瀏覽器閉環。
2. **strike 家族未共型**：`StrikeKind`（Single/Double/None）仍是鏡像枚舉——卡面
   範圍只有 underline，且 `w:strike` 無形態值域（bool 語義），分岔風險量級不同。
3. **ODF 端形態採集**：仍走 E-2 紀律（family 口徑全 None），另卡。
4. **`DrawItem::Text.underline` 的 web 殼消費**：外殼渲染器仍未吃形態欄繪幾何
   底線（E-2 未做面延續，本卡不動渲染面）。

E2B_DONE
