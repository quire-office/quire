# E1-C 解剖：run 級 Trebuchet 的水平 advance 尺——負結果翻案＋尺入表（實測）

- 測試檔：`corpus/ooxmlexport/cell-grid-span.docx`
- 分支：`wt/t_ab6c5319`（HEAD 起點 `4ad8f6c`＝main，含 E1-A `face_line_factor` 1.3620）
- **本卡實跑改動＝1 行**：`core/engine-api/src/lib.rs` `face_advance_scale()`
  新增 `"trebuchet ms" | "trebuchetms" => 0.9744`（原本走 `_ => 1.0`）
- LO 二進位（三支全部本 run 實跑，非沿用）：
  - Linux LO 25.8：`~/.local/lo-linux/opt/libreoffice25.8/program/soffice`
  - **Windows LO（render_lab 板 `find_soffice()` 實測那支）**：`/mnt/c/Program Files/LibreOffice/program/soffice.exe`
  - Linux LO＋真 face（本 run 的定錨配方，§2）：per-process `FONTCONFIG_FILE=/tmp/ab6c-fontcfg/fonts.conf`
- 探針原始碼全部在 scratch（`verify_e1c.py`／`anchor_env.py`／`verify_band.py`／
  `verify_sweep_down.py`／`verify_after_both.py`／`verify_dblspace.py`／`blast_advance.py`），
  本報告每個數字都由這些腳本實跑產出。真值 JSON：`e1c_verify_truth.json`、`e1c_anchor.json`。

## 0. 結論先行（PM 先讀這段）

**卡面「A 軌 1.0359 入表為負結果」的成因找到了，且被翻案：不是「無尺可行」，是基準選錯。**

1. **環境口徑已定錨（卡面第一步(2)，這是本卡真正的解）**：本庫基準環境的既有慣例
   （`face_advance_scale` 文件區 k6cal 記帳）是「真 MS face 裝機 ⇒ LO 渲真 face」。
   照同一慣例用 private fontconfig 目錄 per-process 注入 `trebuc.ttf`（零全域副作用）後，
   **Linux LO 該段從「10 行／頂替 NotoSans」變成「9 行／span font=TrebuchetMS」，
   與 Windows LO（render_lab 板）逐行全等**。兩環境真值收斂 ⇒ 分歧是裝機假象，不是物理限制。
2. **前一版負結果（1.0359 入表）的方向是反的**：那是拿「頂替 face 的斷行」當基準量的尺
   （>1）。以定錨態斷行解可行尺 ⇒ 交集 **[0.9617, 0.9801) 非空**，中點 0.971，
   掃尺實測 **0.9620–0.9800 區間內我方貪婪斷行與定錨真值 9/9 逐行全等**。
3. **入表 0.9744 實跑**：該段對定錨真值（＝Windows 板）逐行全等 **0/9 → 6/9**；
   頁數零變動（cgs 2 頁、RA001 6 頁＝Windows 真值）；corpus A/B `regressed_files=0`。
   殘餘 3 行差異有獨立實測根因（源檔連續雙空格未折疊，§4.2）⇒ 另開卡 D，不在本切片。
4. **lamp1 p1 首末行燈仍紅，且不該由水平尺解**：我方 p0 比定錨真值**多吞** contact 段落
   整段（我方 contact 落 p0 y=702.2，LO 兩環境都落 p1）。這是 packer 首行計帶規則
   （前次 §5 已證：動它 → 400 檔中 6 檔頁數改變、其中 RA001×2 由真值 6 頁改壞成 7 頁）
   ⇒ 本卡不動，卡面「lamp1 轉綠解 ignore」這條驗收條件在不動 packer 的前提下不可達（§5#4）。
5. 垂直尺（已收案的 1.3620）與本次水平尺分屬兩套 face：1.3620＝NotoSans hhea（頂替口徑），
   真 Trebuchet hhea/win 皆 **1.1611**（實測 upm=2048/asc=1923/desc=-455/gap=0）。
   Windows LO 實測 pitch 13.30 ⇒ 反推 LH=13.30/(10×276/240)=1.1565（差 0.0046，PDF y 量化內）。
   我方該段 pitch 15.60（1.3620×10pt×1.15）＝頂替口徑 ⇒ **垂直尺對驗收板環境偏大 17%**，
   本卡準改面不動（卡面「垂直尺再動＝明確不做」），但這是 lamp1 切點燈的真正剩餘來源，已記帳（§6 卡 B）。

---

## 1. 第一步(1)：run 級 rFonts 宣告的查表路徑——實證（非推測）

前次 run5 已在 `para_advance_scale`（`core/engine-api/src/lib.rs`）插 `E1C_DEBUG` 探針實跑，
該段（para#17，9 run）輸出：

```text
[E1C] runs=9 fam=Some("Trebuchet MS") eff_some=9 direct_ascii=Some("Trebuchet MS") scale=1.0000
      head="Team, Passion, Innovation….t"
```

⇒ **卡面假設 (b)「run 級宣告未走到查表尺」死了**：9 run 的 `effective_fonts` 全 `Some`，
`select_text_family` 取到 `ascii="Trebuchet MS"`，查表正常送進去的，只是當時查無臂走 `_ => 1.0`。
源檔該段 pPr/rPr 段標與 9 個 run 的 rPr **都**宣告 Trebuchet（`probe_seg.py` 逐 run 印 rPr 原文），
級聯與 run 級兩條路徑同解。

本 run 補一條**行為級**證據（比探針更硬，因為它走的是出貨碼路徑）：入表前該段斷點對
定錨真值 0/9、入表後 6/9，且 `K7_ADVANCE_SCALE_BUGGY=1` 關掉查表後退回 0/9（§4.1 表）
⇒ 尺確實被該段斷行吃掉，無需再證查表臂。

查表臂現況（本 run 實拆 `face_advance_scale`）：既有 5 條臂（Calibri 0.8834／Cambria 0.9545／
TNR 0.9648／Arial 0.9627／NotoSerif 1.0328）全部 <1，本次新增 Trebuchet **0.9744 同側**
——即「內嵌子集系統性偏寬」這個既有結論對 Trebuchet 也成立，不是一次性調參。

## 2. 第一步(2)：跨環境 LO 真值定錨（本卡核心解）

前次把「裝機 face 有無造成分歧」當成不可動的前提，據此判「無尺可行、不入表」。
本 run 用**本庫自己的慣例**消掉這個前提：`lib.rs` 的 k6cal 記帳寫明本基準環境是
「真 MS face 裝機後，LO 渲真 MS face（Calibri／ArialMT／PMingLiU）」。
Trebuchet MS 同屬 MS face ⇒ 照同一口徑補裝真 face 就是定錨，不是調環境去湊答案。

配方（`anchor_env.py`，零全域副作用）：`/tmp/ab6c-fontcfg/fonts.conf` include 系統
`/etc/fonts/fonts.conf` ＋ `<dir>` 放 `trebuc.ttf` 副本，用 **per-process** 環境變數
`FONTCONFIG_FILE` 注入——不動 `~/.local/share/fonts`，不影響任何其他卡已記帳的 LO 真值。
`fc-match "Trebuchet MS"`：注入前 `DejaVuSans.ttf`、注入後 `trebuc.ttf: "Trebuchet MS" "Regular"`。

三環境實測（本 run `verify_e1c.py`＋`anchor_env.py`）：

| 指標 | LO(Linux) 未注入 | LO(Linux)＋真 face（定錨） | LO(Windows) 驗收板 | 我方引擎（入表後） |
|---|---|---|---|---|
| cell-grid-span 頁數 | 2 | 2 | 2 | **2** |
| Team 段行數 | 10 | **9** | **9** | **9** |
| Team 段渲染 face | NotoSans-Regular（頂替） | **TrebuchetMS** | **TrebuchetMS** | Trebuchet MS（內嵌子集） |
| Team 段 pitch | 15.65 | **13.30** | **13.30** | 15.60（垂直尺，本卡不動） |
| Team 段斷點 vs 定錨態 | — | 基準 | 9/9 逐行全等 | **6/9**（§4） |
| contact 段落所在頁 | p1 | **p0**（y0=652.47） | **p0**（p1 剩 `E:` 一行） | **p0**（y=702.2） |
| RA001-01／-02 頁數 | 7／7（末頁 0 行） | **7／7（末頁 0 行）** | **6／6** | **6／6** |

**定錨結論**：cell-grid-span 的 Team 段，兩環境在真 face 就位後**完全收斂**（9 行、9/9 斷點、
contact 落 p0）⇒ 可用作入表基準。我方頁數 2、段行數 9、contact 落 p0 三項都等於定錨真值。

**RA001 未收斂，且原因不是 face（本卡老實記帳）**：注入真 face 後 Linux LO 仍 7 頁
（末頁 0 行）、Windows LO 仍 6 頁。Trebuchet 裝機與否解釋不了它 ⇒ 該分歧另有成因
（pymupdf `get_text("dict")` 抽出末頁 0 行；LO 對 tracked-revisions 檔的空頁判定在兩平台
不一致的其它機制）。我方 6 頁＝驗收板（Windows LO）真值，故「RA001 頁數不動」這條閘門
本卡實跑通過（入表前後都 6 頁）。RA001 的 Linux/Windows 頁數分歧另開卡（§6 卡 E）。

踩坑記帳（`soffice.exe` 三次失敗才對）：
1. CWD 落在 WSL UNC 路徑（`\\wsl.localhost\...`）會掛死 600s 不返回 ⇒ **cwd 必須在 `/mnt/c` 本機目錄**。
2. `--outdir` 給 Linux 絕對路徑（`/mnt/c/...`）時 `rc=0` 卻吐 `Entity: line 1: parser error :
   Document is empty`、不產 PDF ⇒ **`--outdir` 一律給 `.`（相對）**。

## 3. 第一步(3)：B 軌自然寬行＋候選尺與誤差帶——前次「空集」證明的適用範圍

前次 §3.2 的決定性實驗（我方子集度量 × 貪婪 × 逐行需求帶取交集 ⇒ 空集 [1.0139, 1.0104)）
**只在「目標＝Linux 頂替態斷行（10 行）」時成立**。本 run 換成定錨態斷行（9 行）重解：

| 目標斷行 | 度量 | 逐行需求帶交集 | 判定 |
|---|---|---|---|
| Linux LO 頂替態（10 行） | 我方內嵌子集 | **[1.0139, 1.0104) 空** | 無尺（前次結論，仍成立但基準錯） |
| **定錨態（9 行）＝Windows 板** | 我方內嵌子集 | **[0.9617, 0.9801) 非空** | 有尺：中點 0.971 |
| 定錨態（9 行） | **真 Trebuchet 自身度量** | [0.9922, 1.0069)（≈1.0） | 自證：基準斷點＝真 face 自然斷行 |
| 定錨態（9 行） | 我方子集，但 `’`／`…` 換 NotoSans 真值 | [0.9750, 0.9851) 非空 | 剔污染後帶遷移（卡 C 的量化依據） |

旁證（第 3 列）很重要：拿 Windows 裝機的**真 trebuc.ttf** 做貪婪斷行，版心 468／467／470pt
三種設定下都 **9/9 逐行全等 Windows LO**（`verify_realtreb.py`）⇒ 定錨態斷行不是難復現的怪結構，
就是真 face 的自然斷行；我方要做的就是讓量寬接近真 face。

真 face／我方子集 逐行自然寬比（n=9，Team 段逐行，10pt）：
中位 **0.9744**、min 0.9606、max 0.9863（`verify_realtreb.py`）。
可行帶 [0.9617, 0.9801) 的瓶頸在相鄰兩行夾殺：行 5 要求 `s ≥ 0.9617`、行 4 要求 `s < 0.9801`。
**取 B 軌中位 0.9744 入表**（落帶內，下緣餘 0.0127／上緣餘 0.0057；與 K7 既有的「Arial 取
B 軌覆核定案」同口徑，不用 A 軌大寫樣本抬高值）。誤差帶：逐行 ±1.3%（min/max 對中位）。

掃尺實測（`verify_sweep_down.py`，我方子集度量 × 貪婪 × 468pt，目標＝定錨態 9 行斷點）：

| 尺 | 我方行數 | 逐行全等 |
|---|---|---|
| 1.0000（入表前） | 9 | **0/9** |
| 1.0100 | 9 | 0/9 |
| 1.0200 | 10 | 0/9 |
| 0.9800–0.9620（步長 0.001，n=19） | 9 | **9/9** |
| 0.9610 | — | 落帶外 |

## 4. 入表與實跑效果

### 4.1 入表（唯一改動，`face_advance_scale` 新增臂）

`"trebuchet ms" | "trebuchetms" => 0.9744`，附完整記帳（定錨證據、可行帶、卡 C 重掃提醒）。
註解裡已把「為什麼前一版 A 軌 1.0359 是反的」寫進表上，避免下次再踩。

### 4.2 實跑（`verify_after_both.py`，同一支入表後 CLI 對三份真值）

```text
我方 2 頁、Team 段 9 行
  LO(Linux) 頂替態        10 行｜逐行全等 1/9｜頁 2
  LO(Linux) 定錨真face     9 行｜逐行全等 6/9｜頁 2
  LO(Windows) 驗收板       9 行｜逐行全等 6/9｜頁 2
```

入表前（main）對定錨真值＝**0/9**。殘餘 3 行差異（行 0/1/2）是同一個位移一格的錯位：
我方行 0 多吃了 `helped`，整段往後推一格。**根因實測＝源檔連續雙空格未折疊**
（`verify_dblspace.py`）：源檔該段有 3 處 `India.␣␣iBall’s`／`PC),␣␣during`／`September.␣␣iBall`，
我方原樣保留（`quire-cli page` 實抓該段仍有 1 行含雙空格，每處虛增 2.24pt），
LO 兩環境抽出的行 **0 行含雙空格**（LO 折疊空白）。3 處共 6.72pt，正好吃掉帶的餘量。
⇒ 這是跨檔的水平度量缺陷，**另開卡 D（空白折疊口徑）**，不在本卡「水平 advance 尺」切片內。
（`’`／`…` 全形污染是同源不同碼點，43 檔，見卡 C。）
⚠ 本節歸因已被後續覆核推翻（卡 D 覆核＋卡 E，2026-11-10）：LO rawdict 逐字實測對節點內
雙空格渲染**兩個空格字形**、不折疊（「0 行含雙空格」源自 `get_text("words")` 抽詞假象）；
真正的 6/9→9/9 臨門一腳＝卡 C 標點覆寫臂收窄行 0 的 U+2026。定案見
`docs/e1cd-xml-space-policy.md` ＋ `docs/e1c-card-e-team-99.md`。

### 4.3 零回退自證

- 頁數：cgs 2（不動）、RA001-01/-02 皆 6（不動＝Windows 真值）、CoverPage 2、fdo76591 1。
- corpus A/B（`ow-style-ab.py`，OW_BASE_CLI=main）：`scanned=399/400 regressed_files=0 regressed_items=0`。
- 400 檔頁數掃描（`blast_advance.py`，base=main vs new=入表後）：見 §7 執行紀錄（長跑）。
- 回塞鎖：`K7_ADVANCE_SCALE_BUGGY=1` ⇒ 全表退 1.0，該段退回 0/9（既有機制，實跑驗證）。

## 5. 卡面前提逐條更正（PM 須改卡）

| # | 卡面 | 實測 | 證據 |
|---|---|---|---|
| 1 | 「我方 9 行、LO 10 行 ⇒ 切點差 1 行」 | 對 **Windows LO／Linux LO＋真 face 都是 9 行＝我方**；10 行只是真 face 缺席時的頂替假象 | §2 |
| 2 | 「A 軌 1.0359 入表：該段行數不動」 | 前次記帳與實測相反：1.016 起該段 9→10 行（行數會動）。本次 0.9744 入表行數仍 9（＝真值）、斷點 0/9→6/9 | §3、§4.2 |
| 3 | 「run 級宣告可能未走到查表尺」 | 走到（探針 `fam=Some("Trebuchet MS")`＋入表前後 0/9→6/9 的行為級雙證據） | §1 |
| 4 | 「e1_pagination lamp1 轉綠解 ignore」 | **不動 packer 首行計帶規則就不可達**：我方 p0 多吞 contact 段落（我方 contact y=702.2 落 p0，定錨真值落 p1），要翻轉需動 `pack_run` 首行計帶＝400 檔 6 檔頁數改變、RA001×2 由真值 6 頁改壞 7 頁（前次 §5 實測）。本卡未動 ⇒ lamp1 p1 首末行燈維持 `#[ignore]`，解鎖條件改掛 packer/垂直口徑（§6 卡 B） | §0#4 |
| 5 | 「RA001 頁數不動」為閘門 | 我方 6 頁＝驗收板（Windows LO）真值，入表前後都不動 ⇒ 通過。但 **RA001 的 Linux 7 頁分歧與 Trebuchet 裝機無關**（注入真 face 後仍 7 頁），另有成因 | §2 |
| 6 | 「候選尺待選」/前次「無候選尺可行」 | 定錨態可行尺存在：[0.9617, 0.9801)，取 B 軌中位 0.9744 入表。前次空集結論只對「頂替態斷行」這個錯基準成立 | §3 |
| 7 | 垂直尺（已收案）與水平尺同 face | 同一 face 名有兩套真實行高：真 Trebuchet 1.1611／頂替 NotoSans 1.3620，現行表值是頂替口徑 ⇒ 對驗收板偏大 17%，是切點燈的剩餘來源 | §0#5 |

## 6. 給 PM 的下一步建議（本卡已用 kanban_create 掛出，parents=[t_ab6c5319]）

- **卡 C（`core/layout/assets`）——內嵌子集全形標點 advance**：子集 `’`(U+2019)／`…`(U+2026)
  advance 實測 **1.000 em 全形**（`glyph_advance` 直讀），LO NotoSans 真值 **0.175／0.791 em**。
  corpus 400 檔中**含任一者 43 檔**（含 U+2019 41 檔／U+2026 10 檔，`probe_subset_cov.py`
  ＋本 run 重掃 `e1c_subset_cov.json`）。修它屬發射／量測同源，不動 face 族表。
  ⚠ 連動：卡 C 落地後本臂可行帶遷移至 [0.9750, 0.9851)（§3 表第 4 列），0.9744 將落在帶外 ⇒
  **卡 C 必須連動重掃重定 Trebuchet 臂**，`e1c_advance_trebuchet` 斷行鎖會先紅，不會靜默走樣。
- **卡 D（`core/layout/` 空白口徑）——源檔連續雙空格折疊**：LO 折疊、我方不折疊（§4.2 實測，
  該段 3 處共 6.72pt，正是本卡 6/9→9/9 的臨門一腳）。這是跨檔水平度量缺陷，優先級與卡 C 同級。
- **卡 B（基準環境口徑定案）**：把「真 face 裝機」明定為 LO 真值基準（render_lab 用的是
  Windows exe 版，本身就自帶真 face），並記帳 `face_line_factor` 的 Trebuchet 1.3620 是
  頂替口徑（真值 1.1611）；垂直尺一併在該卡重定錨。定案後才談 lamp1 p1 首末行燈解 ignore。
- **卡 E（RA001 跨平台頁數分歧）**：Linux 7 頁（末頁 0 行）vs Windows 6 頁，已排除 Trebuchet
  裝機因素（§2），成因待查（tracked revisions＋空頁判定）。

## 7. 本卡未做的事（誠實記帳）

- **已解一盞 `#[ignore]`**：`e1_pagination` 燈 1 頁數盞（本 run 實跑轉 CI 常態綠）。
  仍掛 ignore 的是 **lamp1 p1 首末行盞**（我方 p1 首行 `Claire George`，真值
  `For further media information, contact:`）＋燈 3 兩盞；依 §5#4 p1 首末行盞不可由本切片解，
  ignore 字串已改寫為新解鎖條件（不動斷言，零回退安全）。
  ⚠ 順帶發現（未處理，屬 E1-B 範圍）：`-- --ignored` 實跑顯示**燈 3 兩盞現在已綠**
  （CoverPage 頁數／空白頁），但仍掛 ignore ⇒ 建議另開一張解 ignore 收尾卡。
- 未動 `pack_run` 首行計帶（前次已證會改壞 RA001，爆炸半徑不可控）。
- 未動垂直尺（卡面明確不做）。
- **render_lab 驗收板六檔 A/B（本 run 實跑，base＝main@4ad8f6c、new＝本分支）**：
  `scripts/render_lab.py` 逐檔跑 `cell-grid-span／fdo76591／RA001-01／CoverPage／
  WC048-Text-Box-in-Cell／btlr-textbox` 兩態各一次，**兩份報告除生成時間戳外逐行相同**
  （`diff` 只剩時間戳行）⇒ 零回退。實測值：`fdo76591` **版面相似度 1.0**（內容 0.98）、
  頁 1＝LO 1；`RA001-01` 頁 6＝LO 6（內容 [1.0, 0.663, 0.896, 0.927, 0.997, 0.979]）；
  `CoverPage`／`WC048`／`btlr-textbox` 頁數與內容皆零變動；`cell-grid-span` 頁 2＝LO 2、
  p0 內容 0.991／p1 內容 0.542（**入表前後同值** ⇒ p1 偏低是本卡未解的 contact 切點欠債，
  非本次引入）。該板 LO 由 `find_soffice()` 實測回 `soffice.exe`＝**Windows LO 真 face 態**，
  與本卡定錨口徑一致（§2）。卡面寫的 `WC048`／`btlr` 在 corpus 的實際路徑是
  `xopt/TestFiles_WC_WC048-Text-Box-in-Cell.docx`／`ooxmlimport/btlr-frame-vml.docx`＋
  `ooxmlexport/btlr-textbox.docx`（manifest 查無名為 `WC048` 的檔）。
- LO 頂替 face 的全庫 VCL 走查仍未做（只做了 cgs／RA001 的 span font 實據）。
- 400 檔頁數掃描（`blast_advance.py`，base＝main@4ad8f6c CLI vs new＝本分支入表後 CLI）：
  **`scanned=400 failed=0 頁數改變檔數=0`** ⇒ 水平尺入表的頁數爆炸半徑為零
  （scratch `blast_advance.log`）。corpus A/B 發射樣式：`scanned=399/400
  regressed_files=0 regressed_items=0`（`ow-style-ab.py`，OW_BASE_CLI=main）。
- 八閘（`ow-gate.sh`，本 worktree）：`GATE_SUMMARY fail=0`（FMT／CLIPPY／test --workspace／
  分層／roundtrip fixtures／roundtrip corpus／wasm check／goldens 全 PASS）。
- 考卷三態實跑紀錄：
  - `cargo test -p quire-engine --test e1c_advance_trebuchet` → `3 passed; 0 failed`（正常態）。
  - `K7_ADVANCE_SCALE_BUGGY=1 cargo test -p quire-engine --test e1c_advance_trebuchet`
    → `2 passed; 1 failed`（燈 2 `實得 0/9` 必紅 ⇒ 鑑別力成立，比照 `k7_advance_scale.rs` 口徑）。
  - `cargo test -p quire-engine --test e1_pagination` → `3 passed; 0 failed; 3 ignored`
    （燈 1 頁數盞解 ignore 轉 CI 常態綠）；`-- --ignored` → `2 passed; 1 failed`
    （欠債清單剩 lamp1 p1 首末行盞 ＋ 燈 3 兩盞已綠但仍掛 ignore，隨父燈解鎖）。

## 8. 重現指令（全部本 run 實跑過）

```bash
S=/home/jerry/.hermes/kanban/boards/quire/workspaces/t_ab6c5319
SCRATCH=$S /usr/bin/python3 $S/verify_e1c.py        # 三環境頁數/段行數/末頁行數
SCRATCH=$S /usr/bin/python3 $S/anchor_env.py        # fontconfig 注入真 face ⇒ 定錨
SCRATCH=$S /usr/bin/python3 $S/verify_band.py       # 逐行需求帶交集（三口徑）
SCRATCH=$S /usr/bin/python3 $S/verify_realtreb.py   # 真 face 貪婪 9/9 自證＋逐行比值
SCRATCH=$S /usr/bin/python3 $S/verify_sweep_down.py # 掃尺找完美區間 [0.9620,0.9800]
SCRATCH=$S /usr/bin/python3 $S/verify_after_both.py <cli>  # 入表前後對三份真值
SCRATCH=$S /usr/bin/python3 $S/verify_dblspace.py <cli>    # 雙空格歸因
```
