# R1-b 效能基線實錄（先量測後動刀）

卡片：`t_87414628`。本檔是改碼**前**的基線考卷原始數字，全部為實測，非推算。
改碼後同卷重跑，數字寫進 `docs/r1b-perf-after.md` 對照（或由 PM 直接在驗收單記帳）。

## 硬體與 profile

- 機器：WSL2（Ubuntu 24.04.1，kernel 6.18.58.1-microsoft-standard-WSL2），
  Intel Core i5-14400F（16 邏輯處理器），15 GiB RAM，NVMe 於 Windows 宿主。
- Build：`CARGO_PROFILE_RELEASE_DEBUG=1 cargo build --release -p quire-engine --example r1b_rebuild_bench`
  （release ＋ debug info，與 p6_bench／CI 同口徑）。
- 取樣：`R1B_ROUNDS=8`（bench 預設 12；基線單輪全檔成本 1.5s 級，8 輪×5 動作×2 workload
  已具統計意義且燒線可控）。

## 指令（PM 驗收同口徑）

```bash
CARGO_PROFILE_RELEASE_DEBUG=1 cargo run --release -p quire-engine --example r1b_rebuild_bench
# 回塞鑑別（修復落地後）：
R1B_BUGGY=1 cargo run --release -p quire-engine --example r1b_rebuild_bench
# 分相探針（引擎開檔管線分解）：
R1B_PROFILE=1 cargo run --release -p quire-engine --example r1b_phase_probe
```

## workload（實測頁數，老實回報）

| 檔 | 說明 | 段數 | 頁數 |
|---|---|---|---|
| `corpus/poi/test-data_document_bug65649.docx` | PM 親測 44s 級真實公文檔（橫向/直向混排多節） | 15,934 | **190**（`page_count()` 實測） |
| 合成 500 頁（bench 內建合成器，記憶體產出） | 表格每 47 段一張（4 列×2 行）、註腳每 9 段一錨（60 條註腳循環引用） | 4,382 | **503**（`page_count()` 實測） |

合成檔說明：語料內無現成 500 頁含表格＋註腳檔，故以 bench 內純 std 合成器生成，
以指數擴充＋二分收斂取「剛剛 ≥500 頁的最小規模」＝ 3,750 段輸入（含表格段與註腳段），
開檔後實測 4,382 段／503 頁（表格格內段與 marker 段使段數大於輸入段數）。

## 基線數字（改碼前，2026-11-28 實測）

### workload 1：bug65649.docx（真實 190 頁）

開檔 1861ms｜RSS 175MB

| 動作 | mean (ms) | p50 (ms) | p95 (ms) | 觸發路徑 |
|---|---|---|---|---|
| (a) `SetParagraphStyle` | 1746.28 | 1715.46 | **1960.48** | rebuild_derived（全檔重開） |
| (b) `undo()` | 1811.42 | 1814.76 | **1877.70** | rebuild_derived |
| (c) `InsertText` | 968.32 | 968.02 | **1047.29** | 增量＋**末端 recompute_notes**（見分相） |
| (d) `FormatTextRange` | 1813.79 | 1832.32 | **1871.35** | rebuild_derived |
| (e) 零編輯 `save_docx()` | 9.26 | 7.79 | 19.58 | `OoxmlDoc::save()` 全 entry 解壓比對 |

### workload 2：合成 503 頁（表格＋註腳）

開檔 1430ms｜RSS 384MB

| 動作 | mean (ms) | p50 (ms) | p95 (ms) |
|---|---|---|---|
| (a) `SetParagraphStyle` | 1359.61 | 1374.61 | **1466.81** |
| (b) `undo()` | 1451.92 | 1472.09 | **1496.03** |
| (c) `InsertText` | 988.79 | 994.09 | **1041.73** |
| (d) `FormatTextRange` | 1387.58 | 1387.11 | **1410.17** |
| (e) 零編輯 `save_docx()` | 0.38 | 0.35 | 0.67 |

### 基線即綠的門檻（誠實記帳）

- **(e) 180 頁零編輯存檔 < 1.5s：基線已達標**（p95 = 19.58ms，非基線 44s 級——
  PM 卡面的「20.9s 存檔＋23.5s 重排 ≈ 44s」是**編輯＋重排**的總和路徑，
  零編輯 `save_docx()` 本身從未被證明是 20.9s 的元兇；分相數據證明見下）。
- 真正的 44s 級成本在**結構編輯路徑**：(a)(b)(d) 全部 1.4–2.0s／次，
  且 `InsertText`（打字路徑）也被末端 `recompute_notes()` 拖到 ~970ms。

## 分相實錄（`R1B_PROFILE=1`，bug65649，開檔管線＝rebuild_derived 的整包成本）

```
[R1B] ooxml_parse       155.6 ms   ← quire_ooxml::parse（zip 解壓 10ms ＋ XML 145ms）
[R1B] cascade            72.4 ms   ← 樣式級聯投影（15,934 段）
[R1B] grid+tablemap     271.2 ms   ← 帶尺量化＋表格槽幾何
[R1B] layout_build      229.0 ms   ← LayoutEngine::with_breaks_grid（斷行＋分頁）
[R1B] baselines          42.9 ms   ← part_baselines 全量明文克隆（14.6 MiB）
[R1B] engine_struct       0.1 ms
[R1B] recompute_notes: hdr1 0.0 + foot 786.3 + end 0.0 + hdr2 0.0 = 786.3 ms
engine full open:      1481–1616 ms
```

### 分相結論（決定動刀順序）

1. **`compute_footnotes` 786ms ＝ 開檔成本的 50%**，且它在
   `apply_mutation` 末端（lib.rs:2900）與 `rebuild_derived` 內（開檔臂）**都被呼叫**。
   根因坐實卡面根因 2：收斂迴圈起始 `applied = Vec::new()`（不從現行
   `self.params.footnote_reserves_pt` 初始比對）⇒ 第 1 輪 `update_params()`
   無條件清光 `LayoutEngine` 全部斷行快取＋`rebuild_full()`（對 bug65649＝
   205ms＋一輪純計算），即使「套用值==現行值」也照付。實測每輪
   `update_params` 重排 ≈205ms、`footnote_pass` ≈150ms，3 輪上限實走 2–3 輪。
   → **修法 2（recompute_notes 收斂）是單項最大止血**：它同時把
   `InsertText`（非結構編輯）從 ~970ms 打回增量路徑應有的尺。
2. `rebuild_derived` 的 ZIP 整包重開（卡面根因 1）實測構成：
   `save()` 全檔 deflate（9.26ms 零髒態，含 14.6 MiB 明文 deflate）＋
   zip 重解壓 11ms ＋ `parse()` 145ms ＋ bundle 15MB 克隆 ＋ 上述級聯／
   表格／版面／註腳全套重建。ZIP 往來本體（~20ms）相比派生層重建（~1300ms）
   是小頭——**只拆 ZIP 往來不夠**，必須讓派生層重建走「parts 直入」並讓
   recompute_notes 收斂（同一支刀）。
3. `OoxmlDoc::save()` 對**所有** entry 解壓比對（卡面根因 3）：bug65649 態
   (e)=9.26ms 主要是 deflate 14.6 MiB（styles.xml 2.5MB＋document.xml 12.5MB
   明文比對）；190 頁檔未破 1.5s 門檻，但 500 頁合成檔(0.38ms)與真實檔的
   差異說明成本隨明文體積線性——髒快路徑仍要做（卡面修法 3），量到才記帳。
4. **`part_baselines` 全量明文克隆實測（卡面第 4 點，只量測不改）**：
   - bug65649：parts 明文常駐總量 **14.6 MiB**（document.xml 12.0 MiB ＋
     styles.xml 2.5 MiB 為主）＝`part_baselines` 額外常駐 **≈14.6 MiB**
     （開檔 `doc.parts.clone()` 全量，`baselines` 相實測 42.9ms＝克隆時間）。
   - RSS 佐證：190 頁真實檔開檔後 175MB、503 頁合成檔 384MB。
   - 換算：每 1MB 明文 ⇒ ≈1MB 常駐基線克隆＋開檔 ≈3ms。摘要化（crc32,len）
     可回收約一半明文常駐，但牽動 Undo 還原語義——**依卡面另開卡**，本卡不動。

## 基線完整性斷言（bench 內建，跑過即綠）

- 零編輯 `save_docx()` 每輪 `saved == bytes`（byte-identical）：通過。
- 全部編輯 `undo` 淨空後 `save_docx()` 回到 byte-identical
  （K29 bundle 基準保真鐵律）：通過。

## 門檻對照（卡面完成閘門 2，基線態）

| 門檻 | 基線 p95（bug65649） | 狀態 |
|---|---|---|
| 零編輯存檔 < 1.5s | 19.58ms | ✅ 基線已綠 |
| InsertText p95 < 16ms | 1047.29ms | ❌ 差 65 倍（主因＝recompute_notes） |
| SetParagraphStyle p95 < 50ms | 1960.48ms | ❌ 差 39 倍 |
| undo() p95 < 50ms | 1877.70ms | ❌ 差 37 倍 |

---

# 改後實錄（run 323，commit 8d8d6c4 後態，rounds=8，同機同條件）

**加速來源（誠實記帳）**：本卡目前落地的是**根因 2 止血**——
`recompute_notes` 收斂種子化（2832ec4）＋ `LayoutEngine::update_pack_params`
打包軸免斷行快取（8d8d6c4）。**`rebuild_derived()` 本身仍走整包重開**
（save＋from_docx_bytes）；(a)(b)(d) 的降幅全部來自每次 rebuild 末端必跑的
recompute_notes 從「無條件清快取 2 輪重排」收斂為「零變動零重排」。
根因 1（局部重解析免 ZIP 往來）未落地，續工見卡面 block 記帳。

| 動作 | 基線 p95（bug65649｜合成） | 改後 p95（bug65649｜合成） | 門檻 | 狀態 |
|---|---|---|---|---|
| (a) SetParagraphStyle | 1960｜1467 ms | **920｜420 ms** | <50ms | ❌ 未達（剩餘成本＝整包重開） |
| (b) undo() | 1878｜1496 ms | **909｜448 ms** | <50ms | ❌ 未達（同上） |
| (c) InsertText | 1047｜1042 ms | **172｜100 ms** | <16ms | ❌ 未達 10 倍（剩餘＝每 keystroke 的 compute_footnotes 全量 note_pass） |
| (d) FormatTextRange | 1871｜1410 ms | **932｜449 ms** | <50ms | ❌ 未達 |
| (e) 零編輯 save_docx() | 19.6｜0.67 ms | **13.6｜0.57 ms** | <1.5s | ✅ 綠 |

- 降幅：(a)(b)(d) 約 **2.1×（bug）／3.5×（synth）**；(c) 約 **6×／10×**。
- 完整性：零編輯 byte-identical＋Undo 全淨 byte-identical 斷言每輪通過。
- 回塞鑑別（閘門 3）：`R1B_BUGGY=1` 復辟「收斂空表起步＋打包全清快取」，
  (c) p95 退回基線級（172→1047 級），考卷測的就是這條路徑。
- 紅燈考卷 `r1b_note_seed` 四盞：正常態全綠；`R1B_NOTE_SEED_BUGGY=1` 燈 1 必紅
  （實錄 left: 2, right: 0）；`R1B_PACK_CACHE_BUGGY=1` 慢路徑終態不變。
  回塞腳本 `scripts/r1b_note_seed_buggy_replay.sh`。

---

# 改後實錄 run 5（根因 1 落地：commit 60835a4＋b220e70，rounds=8，同機同條件）

**加速來源（誠實記帳）**：根因 1 落地——`rebuild_derived()` 拆掉「全檔 `save()`
＋`from_docx_bytes` 整包重開」，改走記憶體 `doc.parts` → `derive()`（60835a4）：
- `core/ooxml`：`parse()` 的正典投影抽成 `DocProjection`＋`project_parts()`，
  免 ZIP I/O 可達；
- `core/engine-api`：`from_docx_bytes_with_paper` 拆 parse／derive 兩段，
  `derive()` 成為派生層**唯一**建設者（開檔與 rebuild 共用同一口徑＝同形性由
  構造保證，非由約定保證）；
- `rebuild_derived` 免 ZIP 壓縮／解壓、免 `OoxmlDoc` 全量重建、免 `part_baselines`
  全量明文克隆（rebuild 態直接建空表）；
- 派生失敗原子性：全部產物先進區域 `Derived`，成功才一次提交 `self`；
- `PaperSpec` 以 `paper_override: Option<PaperSpec>` 本體保留（舊整包路徑靜默
  沖銷）、`sync_budget` 跨 rebuild 保留（舊整包路徑靜默 reset 為 `None`）——
  兩條均由新考卷 `r1b_local_isomorph` 燈 1／燈 2 先紅後綠鎖死。

## 決定性新結論：ZIP／整包重開不是大頭（量化坐實）

run 5 用同機同 workload 直接 A/B 比對兩條路徑（`R1B_BUGGY=1` 回塞＝舊整包重開）：

| 動作 p95 | 整包重開（BUGGY） | 局部 derive（正常） | ZIP／OoxmlDoc 重開的淨成本 |
|---|---|---|---|
| (a) bug65649 | 928.60 ms | 868.12 ms | **~60 ms** |
| (b) bug65649 | 862.97 ms | 880.49 ms | ≦雜音 |
| (d) bug65649 | 912.33 ms | 880.69 ms | ~32 ms |
| (a) 合成503頁 | 423.09 ms | 450.35 ms | ≦雜音（局部態反慢） |

**局部 derive 對 (a)(b)(d) 的實測增益只有 0–7%**（run 323 的 920ms → 868ms，
再降的主要是 `part_baselines` 克隆而非 ZIP）。這與基線分相早已給出的信號一致
（ZIP 往來僅 ~20-60ms）：根因 1 的**正確性價值**（原子性、PaperSpec／
sync_budget 保留、bundle 鐵律構造性加固）全部到手，但**效能價值有限**——
(a)(b)(d) 剩餘 ~850ms 的全部構成是派生層**全量重算**本身：
`build_slots+images 240ms ＞ layout 205ms ＞ notes 147ms（note_pass 32ms×3 輪
＋build_note_entries 43ms）＞ ooxml parse 111ms ＞ cascade 60ms`。
要壓到卡面 50ms 門檻需要 layout 派生層的**增量收尾**（髒段局部重排／表格槽位
增建／note_pass 增量化），那是超出本卡「止血」定位的獨立工程，續工清單見卡面
block 記帳，由 PM 裁決另立卡。

## run 5 五項數字（rounds=8；門檻判定歸 PM）

| 動作 p95 | 基線（run 322） | run 323（根因2） | **run 5（根因1，本 commit）** | 門檻 | 狀態 |
|---|---|---|---|---|---|
| (a) SetParagraphStyle bug｜synth | 1960｜1467 | 920｜420 | **868｜450** | ≤50ms | ❌（17×／9×） |
| (b) undo() bug｜synth | 1878｜1496 | 909｜448 | **880｜421** | ≤50ms | ❌（18×／8×） |
| (c) InsertText bug｜synth | 1047｜1042 | 172｜100 | **169｜94** | ≤172ms 不劣化 | ✅ |
| (d) FormatTextRange bug｜synth | 1871｜1410 | 932｜449 | **881｜441** | ≤50ms | ❌（18×／9×） |
| (e) 零編輯存檔 bug｜synth | 19.6｜0.67 | 13.6｜0.57 | **11.8｜0.57** | <1.5s | ✅ |

- 累計降幅（基線→run 5）：(a)(b)(d) **2.2-2.3×（bug）／3.3×（synth）**；
  (c) **6.2×／11×**。門檻未達項目的剩余成本定性為派生層全量重算（見上表）。
- 完整性（零回退鐵律）：`cargo test --workspace` **1164 passed／0 failed**
  （基線 1154 一條不掉＋run 323 新 5 盞＋本 run 新 5 盞）；
  `roundtrip corpus --check` **399/399 byte-identical**；bench 內建零編輯＋
  Undo 全淨 byte-identical 斷言每輪綠。
- 回塞鑑別（閘門 3）：`R1B_BUGGY=1`（整包重開復辟）(a)(b)(d) 回到 860-930ms 級
  且燈 1／燈 2 必紅（`paper_override_applied` 沖銷、`sync_budget left:None
  right:Some(3)` 實錄在案）；`R1B_NOTE_SEED_BUGGY`／`R1B_PACK_CACHE_BUGGY`
  兩鎖不變。
- wasm：首包 gzip **1723.8KB** ≤ 3MB；fmt／clippy `-D warnings`／分層全綠。
