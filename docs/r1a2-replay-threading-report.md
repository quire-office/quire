# R1-a 續：`replay` 語意貫穿 K29c／K29d 分流臂——交付報告（t_ba9bff8f）

卡片：`t_ba9bff8f`（父卡 R1-b `t_87414628` 已合入 main `46e8d0c`，發車條件成立後動工）。
性質：程式碼衛生（使用者可見缺陷已由 R1-a `t_2ecfe8ca`／main `5130709` 治癒，本卡收拾
殘留的語意未貫穿）。

## 一、改動面（三檔源碼＋一檔考卷＋一支腳本）

| 檔 | 改動 |
|---|---|
| `core/engine-api/src/mutation.rs` | 前置分派處把 `replay` 傳入兩分流入口；新增 `R1A_SHIELD_BUGGY`（護欄失效鎖）與 `R1A_UNTHREADED_BUGGY`（未貫穿鏡像鎖，`pub(crate)` 供兩檔共用）；`redo()`／`apply_mutation_inner()` 註解改口（護欄兜底角色已撤、原子性放回保留的分工）。 |
| `core/engine-api/src/mutation_k29c.rs` | `apply_k29c_mutation`＋15 支 `k29c_*` 中介＋`commit_k29c` 貫穿 `replay: bool`；else 臂無條件 `clear()` → `if !replay \|\| r1a_unthreaded_buggy()`；Irreversible 態成對清**原樣不動**＋註解白紙黑字寫明「勿順手統一」緣由；4 支函式補 `#[allow(clippy::too_many_arguments)]`（本檔既有三處同款慣例）。 |
| `core/engine-api/src/mutation_k29d.rs` | `apply_k29d_mutation`＋9 支 `k29d_*` 中介＋`commit_k29d` 同口徑貫穿；檔頭／入口註解改口。 |
| `core/engine-api/tests/r1a_replay_k29_family.rs` | 新考卷五盞（見下）。 |
| `scripts/r1a2_replay_k29_buggy_replay.sh` | 四態回塞鑑別腳本（buggy／good／shield-only／mirror-only）。 |

## 二、護欄取捨（卡面任務 2，PM 裁決點）

**兜底角色已撤、原子性放回邏輯永久保留。** 依據：

- 旗標貫穿後，K29c／K29d 重放不誤清棧由 `replay` 語意直接保證，護欄不再是任何一族的
  救命繩——`shield-only` 態（護欄失效＋旗標已貫穿）考卷全綠就是正向證據。
- 但護欄「重放期間保管剩餘棧、成功與失敗路徑皆放回」是 iron rule 3（中途 `?` 失敗時
  未重放 entries 必須仍在棧上）的唯一實作：`apply_mutation_inner` 重放臂中途返錯時，
  沒有任何其他機制能把棧復原。撤掉放回邏輯＝掉原子性保證，得不償失。
- 分工自此寫死在 `redo()` 註解：**旗標管「誰可以清棧」的語意，護欄管「重放中途炸了
  棧要無損」的原子性**。兩層各司其職，未來動 redo 路徑的人不再需要同時解讀兩層疊影。
- 若 PM 裁決連護欄本體一併撤除：iron rule 3 需要另立實作（例如重放臂回滾或 entries
  先複製後提交），屬獨立工程，不在本卡範圍。

## 三、考卷五盞（`r1a_replay_k29_family`，淨新增 5 盞）

| 盞 | 內容 | 判別 |
|---|---|---|
| 燈 1 | `k29c→主幹→k29c` 三筆 undo×3→redo×3 逐筆斷言（首筆即 K29c：重放時棧上尚有 2 筆）＋roundtrip pass＋白名單僅 document.xml | 行為面主鎖；外部 buggy 態必紅 |
| 燈 2 | `主幹→K29d InsertNote→主幹`（K29d 放中段：重放時棧上尚有 1 筆） | 行為面主鎖；外部 buggy 態必紅 |
| 燈 3 | 測試內令護欄失效（`R1A_SHIELD_BUGGY=1`）重跑燈 1 序列**必須仍逐筆成功** | 卡面完成條件「護欄撤除後考卷仍然為真」；**貫穿落地前必紅**（已實錄） |
| 燈 4 | 三態對照：鏡像單翻→綠（實錄「現況靠護欄過」）／兩鎖併翻→redo #2 必 `Ok(None)`（修前世界復辟）／只撤護欄→綠 | 鑑別力證明：紅綠分岔由鏡像驅動，考卷非假綠 |
| 燈 5 | redo 後再一筆 K29c 新操作 ⇒ `can_redo()==false` | iron rule 1 反向鎖（防把新操作斷鏈順手搬走）；外部 buggy 態必紅 |

env 紀律：`R1F_ENV` 靜態鎖 serialize；燈 3／燈 4 用 `EnvFlag`（set／Drop-restore 成對，
逐字還原原值——比 R1-a 尾端 `remove_var` 更嚴，外部閘門整跑時不會把平行盞需要的變數
誤清成假綠）；行為鎖刻意只摘 `R1_REDO_BUGGY`、保留外部 `R1A_*`，故 `buggy` 外部整跑
燈 1／2／5 如實翻紅。

## 四、先紅後綠雙態實錄（PM 會親自重跑）

### 紅燈實錄（貫穿落地**前**，燈 3 打機制面必紅）

護欄尚在場時行為面全綠（卡面預期的「缺陷被兜住」），紅燈打在撤除護欄的機制面：

```text
（源碼未貫穿＋R1A_SHIELD_BUGGY=1 態）
---- r1af_lamp3_shield_off_still_redoes_every_step stdout ----
panicked at core/engine-api/tests/r1a_replay_k29_family.rs:
護欄失效態（R1A_SHIELD_BUGGY=1）三筆重放必須逐筆成功且剩餘棧存活
——replay 旗標未貫穿 K29c 分流臂時本斷言紅（＝本卡要收拾的語意未貫穿）
test result: FAILED. 4 passed; 1 failed; 0 ignored
```

### 綠燈態（`bash scripts/r1a2_replay_k29_buggy_replay.sh good`）

```text
=== 正常態：考卷預期必綠 ===
確認（good）：r1a_replay_k29_family 必綠
GOOD 態通過（env 鎖零改檔，工作區乾淨）
```

### BUGGY 態（`... buggy`＝護欄撤除＋鏡像在場＝修前世界）

```text
=== 回塞態（護欄撤除＋分流臂未貫穿）：跨家族重放鎖預期必紅 ===
確認（buggy）：r1a_replay_k29_family 必紅（3 盞失敗燈）
燈1：重放 K29c 首筆後**剩餘 redo 棧不得被誤清**——還有 2 筆待重放
燈2：重放 K29d 那筆後**剩餘 redo 棧不得被誤清**——還有 1 筆待重放
燈5：redo 後仍有 2 筆待重放——重放 K29c 那筆不得誤清剩餘棧
test result: FAILED. 2 passed; 3 failed
```

### 輔助雙態（護欄／旗標分工證明，皆非缺陷態）

```text
=== 只撤護欄（旗標已貫穿）：考卷預期仍必綠＝兜底角色可撤的正向證據 ===
確認（shield-only）：r1a_replay_k29_family 必綠
=== 只翻鏡像（護欄在場）：考卷預期仍必綠＝卡面「現況靠護欄過」實錄 ===
確認（mirror-only）：r1a_replay_k29_family 必綠
```

R1-a 原考卷雙態不受本卡影響（`scripts/r1a_redo_buggy_replay.sh buggy|good` 兩態原樣通過，
實測坐見）。

## 五、完成閘門實錄

| 閘門 | 結果 |
|---|---|
| `cargo fmt --all -- --check` | 乾淨（貫穿造成的機械性 reflow 已 `cargo fmt` 落庫） |
| `cargo clippy -p quire-engine -p quire-web --all-targets -- -D warnings` | rc=0 |
| `cargo test --workspace` | **1174 passed / 0 failed / 2 ignored**（基線 1169→淨新增 5＝本考卷五盞；卡面基線 1157 為舊數，R1-b／R1-c 入鏈後實測基線 1169） |
| `bash scripts/check-layering.sh` | ✓ 分層檢查通過 |
| 雙態輸出 | 兩端貼見上節＋隨貨腳本 |

## 六、記帳一筆：r1b_note_seed 既有 flake（非本卡引入，不修、上報）

首輪全量跑 `r1b_lamp1_zero_change_recompute_relayouts_zero` 偶發紅（`left: 1, right: 0`），
非我引入：該考卷用**行程全域**計數器 `Engine::relayout_calls()`，而同二進制燈 1／燈 2
平行同程執行，燈 2 的 `insert_char` 可落進燈 1 的測量窗。實據：本卡代碼原樣未動下
`--test-threads=2` 連跑 10 次復現 1 次（run10 rc=101）；單跑 6/6 綠；重跑全量 1174 全綠。
修法（若 PM 開卡）：燈 1／燈 2 拆檔、或計數器改 per-engine、或該檔宣告
`--test-threads=1`。屬 R1-b（t_87414628）考卷的既有race，本卡禁區外不動。

## 七、紀律自查

- 只在 worktree branch（`wt/t_ba9bff8f`）三筆 commit：`77e4a1a`（貫穿）＋
  `9171a63`（簡繁自檢修字）＋`475d4d3`（考卷）；**未 push、未 reset／checkout**。
- editor 三檔（`js/`、`vendor/renderer.js`、`deploy-pages.yml`）零碰觸（`git status` 坐見）。
- 繁中純度：依 traditional-chinese-purity 掃新增行，真洩漏一處（`可见`→`可見`）已修；
  「誰准清棧」改寫為「誰可以清棧」避 准／準 歧義。
- 署名 `ADT109119 <106337749+ADT109119@users.noreply.github.com>`（三筆一致）。
