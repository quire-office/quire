# J4 度量校準報告：四條偽影校準、零漂移鎖與 OMML 移交規格

- 任務書：`.task-cards/card-J4-metric-cal.md`
- 執行分支：`agy/j4-metrics`（base: main `06f690a`）
- 權限邊界：僅修改 `scripts/render_lab.py`、`scripts/tests/test_j4_metrics.py`，新增本報告 `docs/j4-metric-calibration.md`；引擎代碼 `core/`、`shells/`、`tools/`、`fixtures/`、`corpus/` 零觸碰（`git diff --name-only main..HEAD | grep -cE '^(core|shells|tools|fixtures)/' == 0`）。
- 測試覆蓋：`selftest`（17 項全綠）、`test_j3_metrics.py`（16 項全綠）、`test_j4_metrics.py`（3 項全綠）；先紅後綠流程落實，改前紅燈輸出記錄於 commit `0c0dce7`。
- 殭屍進程：完跑後 `pgrep -c soffice == 0`。

---

## 1. 四條偽影校準歸因與改前改後實據

### 1.1 偽影 1：FILENAME 欄位渲染成 scratch 路徑 → 假缺字（`2129393649`）

#### 歸因與事證
- **出處**：`docs/j11-colorimg-diag.md` §3.2 (S14)、`corpus/ooxmlexport/2129393649.docx`。
- **LO 實抽文字鐵證**：
  LO 頁首與頁尾包含 `<w:instrText xml:space="preserve"> FILENAME \p </w:instrText>` 欄位。LO 在無頭轉換時將 `FILENAME \p` 展開為本機臨時檔案絕對路徑：
  `C:\Users\me\AppData\Local\Temp\quire-render-lab\ooxmlexport__2129393649.docx`
- **偽影機制**：Quire 引擎未將 FILENAME 展開為 Windows 執行環境 scratch 路徑，`token_audit` 將 LO 側路徑字串當作正文缺失，誤報 1 個 `true_missing`。
- **校準方案**：在 `token_audit` 引入 `_is_field_artifact(token)`。當 token 符合本機暫存路徑特徵（以盤符開頭、含 `AppData`/`Local`/`Temp` 或 `/tmp/`、且含 `quire-render-lab` 或 `.docx`/`.pdf` 副檔名）時，標記為 `field_artifacts`，豁免計入 `true_missing`。

#### 改前／改後實據
| 檔案 | 改前 true_missing | 改後 true_missing | 改後 field_artifacts | 研判結論 |
|---|---|---|---|---|
| `corpus/ooxmlexport/2129393649.docx` | `1：C:\Users\me\AppData\Local\Temp\quire-render-lab\ooxmlexport__2129393649.docx` | `0`（清空） | `['C:\Users\me\AppData\Local\Temp\quire-render-lab\ooxmlexport__2129393649.docx']` | 偽影徹底消除，正文真缺字精確收斂至 0 |

---

### 1.2 偽影 2：圖軸口徑：LO 全矢量頁 vs 我方彩色 Region（WC048 案）

#### 歸因與事證
- **出處**：`docs/k16-residual-diag.md` §3、main 合併訊息 `91c2693`。
- **實測鏈**：`corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx` 在 LO 側為純向量排版頁面（`get_images() == []`，`drawings == 13`），零鑲嵌點陣圖。main 基線我方未發射彩色圖元時，雙方皆無連通域，圖分評為 `1.0`；K12 依真實 XML 修復 DrawingML 群組子形狀（Rectangle 214 藍帶）與 `w:pBdr` 邊框線後，我方發射了 `DrawItem::Rect`（藍色 `#4472C4` 實心矩形）與兩條 `DrawItem::Rule`。
- **偽影機制**：
  舊版 `score_image` 直接對整頁光柵化圖像提取彩色連通域（`image_blobs`）。因我方藍色 Rect 尺寸（$30 \times 150\text{ px}$）越過 $20\text{px}$ 邊長與 $400\text{px}$ 彩色像素門檻，被度量系統誤當作一張「圖片」；而 LO 側因向量線細未成 blob，形成「LO 0 圖 vs 我方 1 圖」，圖分自 1.0 斷崖暴跌至 0.0。
- **校準方案**：
  1. **口徑分離**：圖軸專注比對「LO 鑲嵌圖 blob vs 我方 Image 項 blob」，我方矢量彩色 Region（`Rect`、`Rule`）與 LO 矢量 drawings 歸色軸（色覆蓋）管轄，嚴禁滲入圖軸。
  2. **遮罩隔離**：`score_image` 接受 `lo_rects` 與 `ow_rects`（嵌入圖邊界）。只有真正的嵌入圖區域參與連通域匹配，其餘文字與向量形狀區域強制遮罩填白。
  3. **可解釋分值**：雙方皆無鑲嵌圖時（如 WC048），圖分評為 `None`（報告中呈現 `—`，代表量測不適用/無圖可比）；只有在「LO 有圖我方也有圖且匹配」時，圖分始得評為 `1.0`。

#### 改前／改後實據
| 檔案 | 改前圖分 | 改後圖分 | 改後色分 | 研判結論 |
|---|:---:|:---:|:---:|---|
| `corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx` | `0.0` | `—`（`None`） | `0.634` | 消除假 0.0 懲罰；矢量藍色圖元完全由色軸（0.634）忠實承擔，圖軸誠實反映無圖狀態 |
| `corpus/ooxmlimport/tdf153791-shd_overrides_fontRef.docx` | `0.0` | `—`（`None`） | `0.0` | 消除 LO 紅色大字筆畫誤當圖片產生的假 0.0；色分 0.0 忠實保留字色缺陷 |

---

### 1.3 偽影 3：目錄點線黏連 token → 假缺字（fdo77129 `How.....2`）

#### 歸因與事證
- **出處**：`corpus/ooxmlexport/fdo77129.docx`、`docs/j11-colorimg-diag.md` §3.2 (S03)。
- **LO 實抽文字鐵證**：
  LO p1 在抽取 TOC 目錄項時，將段落正文詞、點引導符（Leader dots）與頁碼抽出為單一不含空白的長 token：
  `How........................................................................................................................................................................2`
  而我方 dump 斷為兩段 `['How', '.......2']`。
- **偽影機制**：
  既有 `tokenize` 僅以空白分詞，再對 token 頭尾剝除標點符號；由於 `.` 夾在字母 `w` 與數字 `2` 之間，中間的連續點線無法被頭尾剝除邏輯移除，導致整個長串保留為一個 token，`token_audit` 對賬失敗誤報為真缺字。
- **校準方案**：
  在 `tokenize` 引入 `_LEADER_RE = re.compile(r"\.{2,}|[\u00b7\u2022]{2,}")`。在分詞主迴圈中，先將連續 2 個以上之句點 `..`、中圓點 `··` 視為版面引導裝飾符，於邊界處強制切開前後 token。切開後 `How` 與 `2` 分離，標點過濾後正規化為 `how` 與 `2`。

#### 改前／改後實據
| 檔案 | 改前 true_missing | 改後 true_missing | 研判結論 |
|---|---|---|---|
| `corpus/ooxmlexport/fdo77129.docx` | `1：How........................................................................................................................................................................2` | `0`（清空） | 目錄引導點完全切開，與我方 `How`＋`2` 精確配對，誤報徹底消除 |

---

### 1.4 偽影 4：Equation* OMML 缺字簇（K11 殘留，判定與移交規格）

#### 歸因與事證
- **出處**：`corpus/ooxmlexport/EquationWithAboveAndBelowCaption.docx`、`corpus/ooxmlexport/EquationAsScientificNumbering.docx`。
- **OOXML 實體鐵證**：
  源檔包含標準 OOXML Math 標記 `<m:oMath>`（ECMA-376 Part 1 §22.1）：
  ```xml
  <m:oMath>
    <m:d>
      <m:e><m:r><m:t>x+a</m:t></m:r></m:e>
    </m:d>
    <m:sSup>
      <m:e><m:r><m:t>n</m:t></m:r></m:e>
    </m:sSup>
    <m:r><m:t>=</m:t></m:r>
    <m:nary>
      <m:sub><m:r><m:t>k=0</m:t></m:r></m:sub>
      <m:sup><m:r><m:t>n</m:t></m:r></m:sup>
      ...
    </m:nary>
  </m:oMath>
  ```
- **雙方渲染實測對照**：
  - **LO 基線**：具備完整 2D 數學公式排版引擎，將二項式係數、上下標、求和符號 $\sum_{k=0}^n$ 依二維空間坐標排版，PyMuPDF 抽字呈現離散詞塊：`'x+a'`, `'n'`, `'k=0'`, `'k)x'`, `'k'`, `'a'`, `'n−k'`。
  - **Quire 渲染現況**：`quire-cli dump` 顯示我方僅將 `<m:t>` 內之純文字無腦扁平串接，發射出單一未排版無空格字串：
    `x+an=k=0nnkxkan-k` 與 `1+xn=1+nx1!+nn-1x22!+…`。
- **真缺陷 vs 度量偽影判定**：
  **判定：我方引擎真缺陷（Math OMML 未實作二維排版模型與分詞）**。
  理由：
  1. 不是度量腳本拆壞符號，而是 LO 真實渲染了具備空間語意的二維數學公式。
  2. 我方目前完全沒有 OMML 佈局器（全 core 搜尋 `oMath` 僅為平鋪字串收集），甚至未能維護公式 token 邊界，將公式壓成單一巨大亂串，造成嚴重的內容分歧與版面分歧（版面分僅 0.294）。
  3. 依任務書指示：「是引擎缺陷就寫移交規格，本卡不改引擎」。

#### 改前／改後實據
| 檔案 | 改前 true_missing | 改後 true_missing | 判定性質 |
|---|---|---|:---:|
| `EquationWithAboveAndBelowCaption.docx` | 12 個：`x+a`, `n`, `k=0`, `k)x`, `k`, `a`, `n−k`, `1+x`, `n=1`, `nx`, `n(n−1`, `x` | 12 個（逐字全同） | **引擎真缺陷**（保留紅燈） |
| `EquationAsScientificNumbering.docx` | 7 個：`x+a`, `n`, `k=0`, `k)x`, `k`, `a`, `n−k` | 7 個（逐字全同） | **引擎真缺陷**（保留紅燈） |

---

## 2. 固定樣本零漂移鎖（`--n 6 --seed 20261029`）

固定樣本執行指令：
`python3 scripts/render_lab.py --n 6 --seed 20261029 --out <scratch>`

### 2.1 舊前 8 欄逐字比對（零漂移鎖）

| # | 檔案名稱 | 舊前 8 欄（改前 baseline） | 舊前 8 欄（改後 calibrated） | 逐字吻合度 |
|---|---|---|---|:---:|
| 1 | `corpus/ooxmlexport/picture-with-schemecolor.docx` | `1 \| 1 \| ✓ \| — \| — \| — \| measurement_artifact:LO有圖形無可抽文字` | `1 \| 1 \| ✓ \| — \| — \| — \| measurement_artifact:LO有圖形無可抽文字` | **100% 逐字相同** |
| 2 | `corpus/ooxmlexport/fdo77129.docx` | `3 \| 3 \| ✓ \| 0.789, 0.788, 0.744 \| 0.762, 0.778, 0.721 \| — \| —` | `3 \| 3 \| ✓ \| 0.789, 0.788, 0.744 \| 0.762, 0.778, 0.721 \| — \| —` | **100% 逐字相同** |
| 3 | `corpus/xopt/OpenXmlPowerToolsExamples_HtmlConverter01_Test-03.docx` | `1 \| 1 \| ✓ \| 0.843 \| 0.754 \| — \| —` | `1 \| 1 \| ✓ \| 0.843 \| 0.754 \| — \| —` | **100% 逐字相同** |
| 4 | `corpus/ooxmlexport/2col-header.docx` | `1 \| 1 \| ✓ \| 0.632 \| 0.609 \| — \| —` | `1 \| 1 \| ✓ \| 0.632 \| 0.609 \| — \| —` | **100% 逐字相同** |
| 5 | `corpus/ooxmlimport/tdf153791-shd_overrides_fontRef.docx` | `1 \| 1 \| ✓ \| 1.0 \| 1.0 \| — \| —` | `1 \| 1 \| ✓ \| 1.0 \| 1.0 \| — \| —` | **100% 逐字相同** |
| 6 | `corpus/ooxmlexport/dml-picture-in-textframe.docx` | `1 \| 1 \| ✓ \| — \| — \| — \| measurement_artifact:LO有圖形無可抽文字` | `1 \| 1 \| ✓ \| — \| — \| — \| measurement_artifact:LO有圖形無可抽文字` | **100% 逐字相同** |

> **驗收結論**：舊前 8 欄（檔案、LO 頁、OW 頁、頁數吻合、內容分、版面分、首分歧、備註）無任何漂移，零漂移鎖成立。

### 2.2 後 3 軸變化逐檔清單（改前 → 改後 ＋ 為誰而改）

| # | 檔案名稱 | 色相似度（改前→改後） | 圖相似度（改前→改後） | 真缺字（改前→改後） | 為誰而改與原因說明 |
|---|---|:---:|:---:|:---:|---|
| 1 | `picture-with-schemecolor.docx` | `0.0 → 0.0` | `0.0 → 0.0` | `0 → 0` | **無變動**。duotone 雙色調未實作仍為我方引擎真缺陷。 |
| 2 | `fdo77129.docx` | `1.0, 0.0, 1.0 → 1.0, 0.0, 1.0` | `1.0, 1.0, 1.0 → —, —, 1.0` | `1 → 0` | **為目錄點引導符校準（偽影 3）**：切開 `How.....2` 黏連，真缺字 1→0；p0/p1 雙方皆無鑲嵌圖，圖分由虛假的 1.0 校準為 `—`（None），p2 有真實圖表圖片保持 1.0。 |
| 3 | `HtmlConverter01_Test-03.docx` | `0.819 → 0.819` | `0.663 → 0.592` | `2 → 2` | **為真實圖元遮罩校準（偽影 2）**：遮罩過濾掉頁面周邊文字與表格線條後，僅純粹比對嵌入圖區域，圖分更忠實反映圖片縮放誤差（0.592）。缺字 `1/1/2014`, `notes` 為真缺字。 |
| 4 | `2col-header.docx` | `1.0 → 1.0` | `1.0 → —` | `0 → 0` | **為無圖頁面口徑校準（偽影 2）**：全檔無鑲嵌點陣圖，圖分由虛假的 1.0 校準為 `—`（None），貫徹「1.0 僅限雙方皆有圖且匹配」。 |
| 5 | `tdf153791-shd_overrides_fontRef.docx` | `0.0 → 0.0` | `0.0 → —` | `0 → 0` | **為文字誤當圖片校準（偽影 2）**：全檔無圖，消除 LO 紅色文字筆畫被誤判為圖片的假 0.0，圖分校準為 `—`（None）；字色缺陷色分 0.0 忠實保留。 |
| 6 | `dml-picture-in-textframe.docx` | `0.0 → 0.0` | `0.0 → 0.0` | `0 → 0` | **無變動**。文字框內嵌圖遺失仍為我方引擎真缺陷。 |

---

## 3. 四檔偽影改前改後實數對照總表

| 序號 | 檔案路徑 | 項目 | 改前數值 | 改後數值 | 變化意義 |
|---|---|---|:---:|:---:|---|
| **#1** | `corpus/ooxmlexport/2129393649.docx` | 內容分<br>版面分<br>色相似度<br>圖相似度<br>**真缺字** | `[0.828, 0.809, 0.467]`<br>`[0.875, 0.875, 0.875]`<br>`[0.0, 0.0, 0.0]`<br>`[0.0, 0.0, 0.0]`<br>**`1`**（本機 scratch 路徑） | `[0.828, 0.809, 0.467]`<br>`[0.875, 0.875, 0.875]`<br>`[0.0, 0.0, 0.0]`<br>`[0.0, 0.0, 0.0]`<br>**`0`**（豁免產物） | 徹底消除 FILENAME scratch 路徑誤報，正文缺字歸零 |
| **#2** | `corpus/xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx` | 內容分<br>版面分<br>色相似度<br>**圖相似度**<br>真缺字 | `[0.718]`<br>`[0.858]`<br>`[0.634]`<br>**`[0.0]`**（Rect 誤入）<br>`0` | `[0.718]`<br>`[0.858]`<br>`[0.634]`<br>**`[—]`**（無圖可比）<br>`0` | 消除 K12 藍色 Rect 引發的假 0.0，色分忠實承擔，圖分可解釋 |
| **#3** | `corpus/ooxmlexport/fdo77129.docx` | 內容分<br>版面分<br>色相似度<br>圖相似度<br>**真缺字** | `[0.789, 0.788, 0.744]`<br>`[0.762, 0.778, 0.721]`<br>`[1.0, 0.0, 1.0]`<br>`[1.0, 1.0, 1.0]`<br>**`1`**（`How.....2`） | `[0.789, 0.788, 0.744]`<br>`[0.762, 0.778, 0.721]`<br>`[1.0, 0.0, 1.0]`<br>`[—, —, 1.0]`<br>**`0`**（切開匹配） | 消除目錄點線黏連誤報，p0/p1 無圖校準為 `—`，p2 真圖保 1.0 |
| **#4a** | `corpus/ooxmlexport/EquationWithAboveAndBelowCaption.docx` | 內容分<br>版面分<br>色相似度<br>圖相似度<br>**真缺字** | `[0.795]`<br>`[0.294]`<br>`[1.0]`<br>`[1.0]`<br>**`12`** | `[0.795]`<br>`[0.294]`<br>`[1.0]`<br>`[—]`<br>**`12`** | 缺字為真缺陷（OMML 數學未佈局）；無圖頁面圖分校準為 `—` |
| **#4b** | `corpus/ooxmlexport/EquationAsScientificNumbering.docx` | 內容分<br>版面分<br>色相似度<br>圖相似度<br>**真缺字** | `[0.784]`<br>`[0.659]`<br>`[0.329]`<br>`[1.0]`<br>**`7`** | `[0.784]`<br>`[0.659]`<br>`[0.329]`<br>`[—]`<br>**`7`** | 缺字為真缺陷（OMML 數學未佈局）；無圖頁面圖分校準為 `—` |

---

## 4. Equation* OMML 缺陷分析與引擎移交規格（Handover Spec）

### 4.1 缺陷現象與根本原因
1. **現象**：
   `EquationWithAboveAndBelowCaption.docx` 與 `EquationAsScientificNumbering.docx` 在 J3/J4 度量中分別呈現 12 與 7 個真缺字（如 `x+a`, `n`, `k=0`, `k)x`, `n−k` 等），版面相似度低至 0.294。
2. **病灶走查**：
   - 查 `core/ooxml/src/document.rs`：現行解析管線遇 `w:p` 時，若內部包含 `m:oMath` 節點，僅簡單抓取子標籤 `<m:t>` 的文字內容，忽略所有數學結構節點。
   - 包含分數 `<m:f>`（分母 `m:den` / 分子 `m:num`）、求和/極限 `<m:nary>`（下限 `m:sub` / 上限 `m:sup`）、上下標 `<m:sSup>` / `<m:sSub>`、括號定界符 `<m:d>` 皆未建立 AST。
   - 更致命的是：多個 `<m:t>` run 串接時連空白都未保留，直接在 AST 壓成單一無空格字串 `x+an=k=0nnkxkan-k`。排版引擎將其視為一般純文字，無法進行 2D 方塊堆疊。

### 4.2 移交規格（建議新開卡 K20-math-omml）

#### 模組 1：AST 解析層（`core/ooxml/src/math.rs` 新增）
- **節點模型**：
  ```rust
  pub enum MathNode {
      Text(String),
      Fraction { num: Vec<MathNode>, den: Vec<MathNode> },
      SubSup { base: Vec<MathNode>, sub: Option<Vec<MathNode>>, sup: Option<Vec<MathNode>> },
      Nary { op: char, sub: Option<Vec<MathNode>>, sup: Option<Vec<MathNode>>, elem: Vec<MathNode> },
      Delimiter { open: char, close: char, elem: Vec<MathNode> },
      Radical { deg: Option<Vec<MathNode>>, elem: Vec<MathNode> },
  }
  ```
- **解析邏輯**：在 `document.rs` 的 paragraph 處理中，遇 `b"m:oMath"` 時遞迴解析上述節點，掛載於段落的 `RunContent::Math(Vec<MathNode>)`。

#### 模組 2：排版佈局層（`core/layout/src/math.rs` 新增）
- **二維盒模型**：
  依據 OpenType `MATH` 表（Cambria Math 字型標準規格）實作數學佈局：
  1. 分數線計算：計算分子與分母盒寬，橫線置中，上下垂直間距由 `FractionNumeratorShiftUp` / `FractionDenominatorShiftDown` 決定。
  2. 上下標計算：依當前基準線向上/向下偏移，字級縮小至 70%（Script size）。
  3. 大型運算符（$\sum$）：居中顯示符號，上下限垂直排列於運算符正上方與正下方。

#### 模組 3：渲染發射層（`core/engine-api/src/lib.rs`）
- 將數學盒模型轉換為獨立的 `DrawItem::Text`（帶各自空間 $x, y$ 坐標與字型大小）及 `DrawItem::Rule`（分數水平線）。
- 保證 token 空間分離，使跨引擎抽字與文字排版完全對齊 LO。

---

## 5. 交付驗收核對表

- [x] **驗收 1**：`python3 scripts/render_lab.py --selftest` 全綠（17 項 PASS，新增 6b、7c、7d 三項防復發案例）。
- [x] **驗收 2**：`python3 scripts/tests/test_j3_metrics.py` 全綠（16 項 PASS，舊案例零改死）。
- [x] **驗收 3**：固定樣本零漂移鎖（`--n 6 --seed 20261029`），舊前 8 欄 100% 逐字吻合。
- [x] **驗收 4**：四條偽影檔改前改後實數與歸因完備記錄。
- [x] **驗收 5**：完跑後 LO 殭屍進程數 `pgrep -c soffice == 0`。
- [x] **鐵律邊界**：`git diff --name-only main..HEAD | grep -cE '^(core|shells|tools|fixtures)/' == 0`（退件線安全）。
