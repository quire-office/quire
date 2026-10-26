# L1-columns：`w:cols` 分欄回流交付報告（oc/l1-columns）

任务卡：t_26c3607d（run10）。基线 = PM 代 commit 的 `e7dbde9`（sectpr 侧
`SectionCols` 半成品，认领续做不重写）。

## 实作范围（三个 commit）

1. `491f100` feat：分栏回流全链路。
2. `3fe1086` fix：tools/line-metrics 跟随 `SectionGeometry.cols`（E0063）。
3. `f1c6cc8` chore：移除两个 l1_probe 诊断 example（PM 纪律(1)）。

### ooxml 侧（`core/ooxml/src/sectpr.rs`）

- `SectionGeometry.cols: SectionCols`（serde default，旧快照兼容）。
- scan 分支：`w:cols` 读 `@num`/`@space`（缺=720twip=36pt）、`w:col` 逐栏
  `@w:w`/`@w:space` 入头 4 栏固定槽（Copy，无 Vec，PM 骨架）；`w:cols`
  重开重置 col_seen；超 4 栏槽位截断、num 原值保留（呼叫端 clamp）。

### layout 侧（`core/layout/src/lib.rs`，主战场）

- layout 侧镜像 `SectionCols`＋`col_layout(&geo)`：`num<=1`/未宣告 →
  `None`（**单栏零扰动闸门**）；宣告逐栏宽 → 用宣告值 else 等宽平分；
  右侧距 = 宣告 `col_space` else `space_pt`。产出 `col_x[]`/`col_w[]`。
- `pack_run` 回流：版心宽按栏切片（`section_break_w_pt`）→ 段落填满当前
  栏（`cur_used` 对 `cap`）→ 容量溢出先换栏（`col_break_now`：`cur_col+1`、
  `cur_used=page_used`，页内位移，不动页 flush/收敛键）→ 栏尽（最后一栏
  溢出）才换页（`fresh_page` 重置 `cur_col/page_used`）。
  硬断（`w:br`）与 `pageBreakBefore` 越过换栏直接换页；非 continuous
  section 边界照旧强制换页；continuous 边界只免边界本身。
- `Line.col: u8`（serde default 0）：行盖栏序章；goldens 只收
  text/width_pt，不扰动（实测三档全相符）。
- 跨栏项简化（卡上已定）：表格/整幅图占满版心宽；多栏下以页级
  `page_used` 让每一栏顶端共同让位；单栏时 `place_band==cur_used`
  与旧算式逐位元组等价。多栏表格加 remain=0 保护（先换页重放，防整项
  被丢）。

### engine 侧（`core/engine-api/src/lib.rs`，唯一接线点）

- ooxml→layout `SectionCols` 逐栏对应（分栏铁律：layout 不依赖 ooxml）。
- 级联断行宽统一走 `section_break_w_pt`（与 layout 断行同口径）。
- 发射层：文字项 `x = margin_left + col_x[line.col]`；单栏/越界回退
  `margin_left`（旧行为逐位元组不变）。

## 红灯（PM 纪律(3)：先写先红→红绿即 commit）

- **红灯1（engine 级）** `core/engine-api/tests/l1_two_col.docx`（zipfile
  手搓，1262 bytes，commit 入库）＋`tests/l1_columns.rs`：
  - `l1_two_column_fixture_has_two_x_origins`：p0 文字项 x 必须出现两个
    栏起点（72.0 与 315.65=72+207.65+36），栏距（第二栏起差）=宣告
    栏宽+space≈243.65pt。→ 绿。
  - `l1_two_column_lines_fit_column_width`：断行宽=栏宽（行长字符数
    远小于单栏 451.3pt 口径）。→ 绿。
  - `l1_single_column_unaffected`：单栏档（sample.docx）首文字项恒从
    版心左缘起（buggy 回塞把未宣告误当两栏即红）。→ 绿。
- **红灯2（layout 级）** `tests::cols_*` 7 例：未宣告/num<=1 → None；
  Letter 两栏等宽平分几何（216/252）；40+40 段栏内回流第 81 段起第 2
  页；逐栏宣告宽优先；栏满换页。→ 全绿（layout lib 126 passed）。
- **红灯3（ooxml 级）** `cols_*` 4 例：未宣告默认/宣告逐字投影（twip→pt）/
  自闭合+跨节独立/超 4 栏截断。→ 全绿（ooxml lib 167 passed）。
- **buggy 回塞双红锁死**：`col_break_now = false && …` 人为关换栏 →
  layout `cols_reflow` 与 engine `two_x_origins` 同步必红（实测），恢复后
  全绿。测试真咬得住，非恒真断言。

## 全闸門（PM 纪律(4)）

- `cargo test --workspace`：layout 126 ✓、ooxml 167 ✓、engine 154 ✓、
  其余套件全绿（0 failed）。
- `cargo clippy --all-targets -- -D warnings`：RC=0。
- goldens：shaping 12 / pagination 4 / tables 5 三档相符（**单栏零扰动
  铁律达成**——`Line.col` serde default 不入 golden 口径）。
- `scripts/check-layering.sh`：✓（layout 不依赖 ooxml；engine 唯一接线）。
- 真实语料探针（rm 前 l1_probe）：n830205（OOO 导出双栏节）6 页 → 5 页，
  p1 出现第二栏起点 315.6/354.4/378.9/383.9pt（BASE 全页无 >300pt 起点）。

## 仪表盘 A/B（seed42 双目录，基线 e7dbde9 worktree vs HEAD）

- 25 档（seed=42, n=25）逐档比对：**25/25 分数逐位元组零变动**（WORSE=0、
  BETTER=0），content avg 0.666 / layout avg 0.584 / page_match 21/25
  两侧全等。零劣化达成。
- 样本含 3 个双栏档（phpword reader / reader-styles / clone-delete-block），
  dump 实测其双栏段的**断行确实变短**（如 clone-delete-block p0 段行从
  单栏长行切成栏宽短行），但分数不动的原因要讲明：
  1. `content_key` 口径忽略换行位置（J2 校正设计）；
  2. `layout_key` 只比每页前 8 行拼接，这 3 档的双栏段都不落在前 8 行；
  3. 这些档双栏段内容短、不溢出栏，页数不变（`pages_lo/pages_ow=None`
     =LO 侧转换不可比）。
  即：**A/B 零劣化可信（无档变差），零改善是度量口径盲区，不是分栏未
  生效**——生效证据 = 红灯组 + n830205 探针（6→5 页、第二栏 x 315.6）。
  证据存档：`out/ab-base/scores.jsonl`、`out/ab-head/scores.jsonl`。

## 与 LO 的已知残差（简化项，注释已写明）

1. **跨栏项占满全幅**：表格/整幅图不切栏、占版心宽；多栏下各栏顶端
   共同让位（页级 `page_used`）。LO 会把跨栏表格按整幅插断点；我们不做
   栏内表格切片。
2. **不做栏平衡（column balancing）**：末栏不回填平衡高度（continuous
   节 LO 会平衡各栏高度；我们是贪心填满）。n830205 页数 5 页 vs LO 若
   平衡可能 4 页，属此残差。
3. **逐栏宣告超 4 栏截断**：corpus 实见 ≤4 栏（alphabeticalIndex 4 栏），
   >4 畸形档用平分近似。
4. **栏内浮动图形定位**：浮动图仍以页为参照（未做栏内绕排），与 K4 系列
   既有残差同源。

## 未尽事项

- seed42 样本无溢出型双栏档（内容都不满栏），BETTER 侧无法在本样本内
  呈现；建议下轮以多栏专档子集（scan_cols.py 列出的 22 档）跑定向 A/B。
- 未 push（依铁律等用户指令）。
