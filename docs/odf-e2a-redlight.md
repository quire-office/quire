# ODF-E2a 紅燈報告：GeneralRef 一般實體丟字

卡號：t_0eb593b3（ODF-E2a）
範圍：`core/odf/src/layout_glue.rs::walk_paragraphs`、
`core/odf/src/body.rs::extract_body_from_bytes`
考卷：`core/odf/tests/odf_e2a_generalref.rs`
真值來源：`docs/odf-survey/e2-flow-attribution.md` §2 D1、§3.1

## 1. 修前紅燈（先紅）

指令：`cargo test -p quire-odf --test odf_e2a_generalref`
結果：`0 passed; 2 failed`，兩盞皆在逐段完整文字斷言上失敗。

燈 1 失敗（投影第 2 段）：

```
left : "...That didnt bode well...who d had..."（實為 whod）
right: "...That didn't bode well...who'd had..."
```

燈 2 失敗（投影第 3 段）：

```
left : "Источник: РИА Новости. Горячая линия"
right: "Источник: РИА \"Новости\". Горячая линия"
```

失敗訊息原文摘要：

```
assertion `left == right` failed: 投影第 2 段 &apos; 未還原
assertion `left == right` failed: 投影第 3 段 &quot; 未還原
```

## 2. 事件流實證（evdump 探針）

### 2.1 ooo32780-1：`&apos;` 與 `&gt;`

```
Start <text:p> p_depth=1
Text "He heard quiet steps behind him. That didn" p_depth=1
GeneralRef &apos; p_depth=1
Text "t bode well. ... who" p_depth=1
GeneralRef &apos; p_depth=1
Text "d had the same idea ..." p_depth=1
End </text:p> p_depth=0
```

```
Text "Section Hidden IF (x==1) -" p_depth=1
GeneralRef &gt; p_depth=1
Text " " p_depth=1
Start <text:variable-set> p_depth=1
Text "1" p_depth=1
End </text:variable-set> p_depth=1
End </text:p> p_depth=0
```

### 2.2 poi/drawing：`&quot;`

```
Start <text:p> p_depth=1
Start <text:span> p_depth=1
Text "Источник: РИА " p_depth=1
GeneralRef &quot; p_depth=1
Text "Новости" p_depth=1
GeneralRef &quot; p_depth=1
Text ". Горячая линия" p_depth=1
End </text:span> p_depth=1
End </text:p> p_depth=0
```

兩支 walker 現況只收 `Event::Text`，`Event::GeneralRef` 落入 `_ => {}`，
所以上列實體所在字元整批消失。`core/ooxml/src/qx.rs::resolve_general_ref`
是已驗證參照口徑：字元參照走 `resolve_char_ref`，具名實體走
`resolve_predefined_entity`，其餘組織回 `&name;` 再交 `unescape` 老實解析，
未知實體不被虛構成別的字。

## 3. 卡面描述與源檔實況的對齊

卡面燈 1 寫「第 2 段含 `didn't`（U+0027）與 `-> `」。源檔 `content.xml`
實況：`didn&apos;t` 在投影第 2 段；全檔唯一一個 `&gt;` 在投影第 10 段
`Section Hidden IF (x==1) -&gt; 1`（offset 5817），兩者不同段。
故考卷不改寫事實：第 2 段逐字斷言 `didn't`／`who'd`，第 10 段逐字斷言
`-> `，同檔同修一併驗證，不硬湊成不存在的「同段」。

## 4. 修前 50 檔 A/B（0.37 基線 vs 0.41 修前）

口徑：以 `odtflow` 逐段投影探針跑 `corpus-odt/manifest.json` 全 50 檔。
0.37 基線取 `git archive 8f6238c` 解到工作區外另編，0.41 修前取本工作區。
兩檔加密（`sha1_correct`／`sha1_wrong`，content.xml 宣告 Blowfish CFB）
在 ODF-E1 閘門後投影報錯，兩版皆無可比較內文，故逐檔表只列其餘 48 檔；
48 檔合計：0.37 基線 593902 字、0.41 修前 593297 字，差 **−605**。

| 檔案 | 0.37 基線 | 0.41 修前 | 丟字 |
|---|---:|---:|---:|
| `lo_out/lo__poi__test-data_document_drawing.odt` | 57982 | 57618 | −364 |
| `lo_out/lo__poi__test-data_document_bug65649.odt` | 271801 | 271713 | −88 |
| `lo_out/lo__ooxmlexport__fdo78420.odt` | 43228 | 43193 | −35 |
| `lo_out/lo__ooxmlexport__tdf123705.odt` | 5332 | 5305 | −27 |
| `lo_out/lo__poi__test-data_document_bug59058.odt` | 118195 | 118168 | −27 |
| `ext/oasis_opendocument_v1p3_os_part1_introduction.odt` | 15007 | 14988 | −19 |
| `ext/lo_core_sw_qa_odfimport_ooo32780-1.odt` | 3082 | 3071 | −11 |
| `lo_out/lo__poi__test-data_document_bug57031.odt` | 7309 | 7300 | −9 |
| `lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Pluto.odt` | 1237 | 1232 | −5 |
| `lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Sun.odt` | 2121 | 2116 | −5 |
| `lo_out/lo__ooxmlexport__tdf104061_tableSectionColumns.odt` | 5289 | 5285 | −4 |
| `lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Mercury.odt` | 1983 | 1979 | −4 |
| `lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Saturn.odt` | 1390 | 1387 | −3 |
| `lo_out/lo__ooxmlexport__FDO78292.odt` | 1099 | 1097 | −2 |
| `lo_out/lo__poi__test-data_document_checkboxes.odt` | 119 | 118 | −1 |
| `lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Neptune.odt` | 758 | 757 | −1 |
| **合計（16 檔）** | **593902（48 檔）** | **593297（48 檔）** | **−605** |

修前逐段探針（ooo32780-1）投影前 11 段：

```
0  14  Section Simple
1   0
2 522  He heard ... That didnt bode well ... whod had ...
...
10 28  Section Hidden IF (x==1) - 1
```

修前逐段探針（poi/drawing）投影前 4 段：

```
0 14  МОНИТОРИНГ СМИ
1 14  12.04-13.04.06
2  0
3 36  Источник: РИА Новости. Горячая линия
```

## 5. 卡住點

無。紅燈成因、事件流、兩支 walker 漏接位置皆與歸因報告 D1 一致，
沒有語義模糊到需要停下來的點；唯一需要書面定錨的是第 3 節的卡面段落
描述與源檔不同段，已按實況拆成兩個同檔逐段斷言。

## 6. 綠燈補記（修後實測）

### 6.1 考卷

`cargo test -p quire-odf --test odf_e2a_generalref`：`2 passed; 0 failed`。
`cargo test -p quire-odf`：

- `odf_container`：5 盞全綠（燈 3 零回退）。
- `odt_layout_projection`：7 盞全綠（燈 3 零回退）。
- `odf_e2a_generalref`：2 盞全綠（本卡新考卷）。
- `odf_encrypted_gate`：1 盞全綠（既有閘門未動）。

### 6.2 50 檔探針重跑（修後）

同一支 `odtflow` 逐段投影探針、同一份 `manifest.json`。修後 48 檔可投影
（另 2 檔為加密檔，ODF-E1 閘門照舊報錯，與修前一致）合計 593902 字，
**與 0.37 基線逐檔完全相同，48/48 檔差值皆為 0**；相對 0.41 修前合計
回復 605 字。逐檔「修前丟字」與「修後丟字」如下（修後丟字＝修後字數與
0.37 基線之差）：

| 檔案 | 0.37 基線 | 修前 | 修後 | 修前丟字 | 修後丟字 |
|---|---:|---:|---:|---:|---:|
| `lo_out/lo__poi__test-data_document_drawing.odt` | 57982 | 57618 | 57982 | −364 | 0 |
| `lo_out/lo__poi__test-data_document_bug65649.odt` | 271801 | 271713 | 271801 | −88 | 0 |
| `lo_out/lo__ooxmlexport__fdo78420.odt` | 43228 | 43193 | 43228 | −35 | 0 |
| `lo_out/lo__ooxmlexport__tdf123705.odt` | 5332 | 5305 | 5332 | −27 | 0 |
| `lo_out/lo__poi__test-data_document_bug59058.odt` | 118195 | 118168 | 118195 | −27 | 0 |
| `ext/oasis_opendocument_v1p3_os_part1_introduction.odt` | 15007 | 14988 | 15007 | −19 | 0 |
| `ext/lo_core_sw_qa_odfimport_ooo32780-1.odt` | 3082 | 3071 | 3082 | −11 | 0 |
| `lo_out/lo__poi__test-data_document_bug57031.odt` | 7309 | 7300 | 7309 | −9 | 0 |
| `lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Pluto.odt` | 1237 | 1232 | 1237 | −5 | 0 |
| `lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Sun.odt` | 2121 | 2116 | 2121 | −5 | 0 |
| `lo_out/lo__ooxmlexport__tdf104061_tableSectionColumns.odt` | 5289 | 5285 | 5289 | −4 | 0 |
| `lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Mercury.odt` | 1983 | 1979 | 1983 | −4 | 0 |
| `lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Saturn.odt` | 1390 | 1387 | 1390 | −3 | 0 |
| `lo_out/lo__ooxmlexport__FDO78292.odt` | 1099 | 1097 | 1099 | −2 | 0 |
| `lo_out/lo__poi__test-data_document_checkboxes.odt` | 119 | 118 | 119 | −1 | 0 |
| `lo_out/lo__xopt__OpenXmlPowerToolsExamples_DocumentBuilder04_Neptune.odt` | 758 | 757 | 758 | −1 | 0 |
| **合計（16 檔）** | **593902（48 檔）** | **593297（48 檔）** | **593902（48 檔）** | **−605** | **0** |

### 6.3 修後逐段探針

ooo32780-1（投影第 2 段與第 10 段）：

```
2  524  He heard ... That didn't bode well ... who'd had ...
10  29  Section Hidden IF (x==1) -> 1
```

poi/drawing（投影第 3 段）：

```
3  38  Источник: РИА "Новости". Горячая линия
```

