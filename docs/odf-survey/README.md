# ODF-C1：corpus-odt 50 檔容器級普查報告

任務卡：`ODF-C1 corpus-odt 語料建置＋授權登記＋容器級普查（report-only＋語料，零引擎碼）`

本調查建立並只讀 `corpus-odt/`，產出 `corpus-odt/manifest.json`、`corpus-odt/quarantine.json`、`docs/odf-survey/survey.json` 與本報告，未修改 `core/`、`shells/`、`tests/`、`Cargo.toml` 或任何既有語料檔。建置腳本為 `scripts/odf_build_corpus.py`（LO 轉出＋外部下載＋合成 fixture），普查腳本為 `scripts/odf_survey.py`；後者於 worktree 根目錄以 `python3 scripts/odf_survey.py` 重跑，依 `corpus-odt/manifest.json` 順序重新產生 `docs/odf-survey/survey.json`，輸出為冪等（同語料同 manifest 逐位元組相同，已實測 `cmp` 通過）。

## 1. 全域結果

本批正庫 50 檔：LO headless 轉出 40 檔（`lo_out/`）＋上游公開授權可查證的真實 ODT 10 檔（`ext/`），另有合成違例 fixture 3 檔（`fixtures/`，不計入 50，僅供普查先紅自證）。manifest 完備性自證通過：50 檔全有來源、授權、取得方式、sha256 與 bytes，實體檔 sha256 全數相符，缺欄位、重複、sha 不符、實體缺檔均為 0。quarantine 目前 `files` 為空，`tried_and_rejected` 登記 3 筆授權或取得查證不過的來源（見第 7 節）。

| 指標 | 數值 |
|---|---:|
| 正庫檔案數 | 50（lo-converted 40／real-world 10） |
| 檔案大小合計 | 14,417,605 bytes |
| 解壓後合計 | 38,894,373 bytes |
| zip entry 合計 | 942（單檔 6 至 31 個） |
| `text:p` 段數 | 19,687（lo_out 19,401＋ext 286） |
| `text:h` 標題數 | 248 |
| `text:span` 數 | 33,641 |
| `table:table` 數 | 120（28 檔含表；`table:table-row` 3,151、`table:table-cell` 14,745） |
| 合併格覆蓋格 | 47,207 個 `table:covered-table-cell`（17 檔） |
| `draw:frame` 數 | 83（27 檔）；`draw:image` 82（25 檔）；`draw:text-box` 4（4 檔） |
| `text:list` 數 | 230（28 檔） |
| 目錄 `text:table-of-content` | 5 檔；`text:outline` 恆 0（ODF 無此元素，見第 3 節） |
| 腳註/尾註錨 `text:note` | 28 個錨（12 檔） |
| 超鏈 `text:a` | 434（17 檔） |
| 軟分頁 `fo:break-before="page"` | 21 次（12 檔）；`fo:break-after="page"` 0 |

產生器分布：LO 24.8.0.3（Windows x86_64，本次 `--convert-to odt:writer8` 轉出）40 檔；其餘 10 檔為真實世界來源，涵蓋 MicrosoftOffice/15.0（Word 2013 直接存 ODT）、OpenOffice.org 2.3、LibreOffice 4.3.1.1／5.3.1.2／6.4.5.2／7.2.2.2 與兩個 LibreOfficeDev alpha 版，外加 2 檔無產生器記錄（加密檔 meta.xml 亦加密）。ODF 宣告版本：manifest 版 `1.3`×41、`1.2`×5、未宣告×4；`office:version` `1.3`×41、`1.2`×6、`1.0`×1、未宣告×2——本批以 ODF 1.3 為主但 1.2 與裸宣告必須同時支援。

## 2. 容器層與 mimetype 考證

50 檔的 `content.xml`、`styles.xml`、`meta.xml`、`settings.xml`、`META-INF/manifest.xml` 全部存在（缺任一部件者 0 檔）；`Configurations2/` 49 檔、`Thumbnails/` 45 檔、`manifest.rdf` 48 檔、`Pictures/` 25 檔、`Object*/` 巢狀物件目錄 9 檔。mimetype 值 50 檔全部正確為 `application/vnd.oasis.opendocument.text`。

mimetype 規則（ODF 1.3 §3.3：必須是首 entry 且 stored 不壓縮）普查結果：47 檔合規（首 entry、stored）；3 檔違例，全部是「stored 但不在首位」——`ext/lo_core_sw_qa_odfexport_tdf140437.odt`（mimetype 在第 14 個 entry）、`ext/lo_core_sw_qa_odfexport_tdf152710.odt`（第 12）、`ext/lo_core_sw_qa_odfexport_tdf77961.odt`（第 5）。三檔都是 LibreOffice/core sw/qa 的真實 upstream 檔，也就是「老實讀 zip 首位」的解析器會直接誤判的現成病態。本批沒有出現「mimetype 被壓縮」的真實違例，該支規則由合成 fixture `fixtures/mimetype_compressed.odt` 先紅自證覆蓋。

合成 fixture 3 檔（由 `scripts/odf_build_corpus.py` 造，來源為正常 LO 轉出檔的再打包）：`mimetype_not_first.odt`（mimetype 挪至末位）、`mimetype_compressed.odt`（mimetype  deflate 壓縮）、`missing_manifest_xml.odt`（刪除 `META-INF/manifest.xml`）。

## 3. content.xml 結構統計

段落密度差距極端：單檔最多 `text:p` 14,161 段（`lo_out/lo__poi__test-data_document_bug65649.odt`，10.6 MB 大檔），最少 0 段（2 檔加密檔，正文為密文，見第 5 節）；`ext/` 真實檔平均遠小於 LO 轉出批次。表格集中在校準樣本：`table:table` 最多 21 個（`bug57031`、`bug65649`）、18 個（`tdf102466`）；合併格（覆蓋格）以 `bug65649` 43,181 個為最多，`tdf123705` 3,400 個次之；跨欄（`number-columns-spanned` 大於 1）6,550 處、跨列（`number-rows-spanned` 大於 1）1,841 處。本批實測覆蓋格全部是單數 `<table:covered-table-cell/>` 寫法，複數 range 寫法 `<table:covered-table-cells/>` 為 0——規格允許後者，投影模型兩者都必須映射，否則覆蓋格會渲染成實體格。

目錄考證（病態考證第一批）：卡面假設的 `text:outline` 在 ODF 中不存在，本批 50 檔出現 0 次；真實標籤是 `text:table-of-content`（5 檔：OASIS 規範檔、`tdf102466`、`tdf124637_sectionMargin`、`lo__selfgen__dropcap_toc_fields`、`TestFiles_HC031-Complicated-Document`），其下有 `text:table-of-content-source` 與 `text:table-of-content-entry-template` 骨架，章節標題走 `text:h`（248 個）而非段內大綱元素。斷頁訊號同理：LO 導出完全不用 `text:soft-page-break` 標籤，軟分頁以樣式屬性 `fo:break-before="page"` 表達（21 次、12 檔，單檔最多 `bug65649` 8 次），`fo:break-after="page"` 為 0。

錨點統計：腳註/尾註錨 `text:note` 28 個（12 檔，content.xml 側），每錨配對 `text:note-body`；`text:a` 超鏈 434 個（17 檔）全數可歸因；`xlink:href` 總數 556，無一筆無法歸因到起始標籤。嵌入物件方面 `draw:frame` 27 檔，media_type 混雜 `application/vnd.oasis.opendocument.chart`×4、`application/vnd.sun.star.oleobject`×4、`application/vnd.oasis.opendocument.formula`×2、`application/pdf`×1、`application/vnd.oasis.opendocument.spreadsheet`×1；嵌入字型僅 1 檔（`lo_out/lo__selfgen__embedded_font_odttf.odt`，application/x-font-ttf，11.0 MB 為本批最大檔）。

## 4. styles.xml 結構統計

50 檔 styles.xml 合計 default-style 194 個、named style 5,081 個；`style:list-style` 為 0——Writer 清單樣式在本批全部走 default outline 規則，列表樣式考題要從 list-style 為零的現實出發。`style:page-layout` 48/50 檔有，缺的 2 檔正是加密檔（`ext/lo_core_sw_qa_odfexport_sha1_correct.odt`、`sha1_wrong.odt`：styles.xml 僅 1,886 bytes、0 個 named style，版面資訊實際被加密搬進密文，不能據此推「本檔無版面」）。`style:master-page` 75 個、`style:header` 32 個（21 檔）、`style:footer` 29 個（19 檔）、`style:font-face` 664 個。頁首頁尾住在 styles.xml 的 master-page 到 page-layout 鏈上，投影器不能只讀 content.xml。

## 5. 病態清單（先紅後綠的落點）

普查 `flags` 詞彙與命中數（布爾旗標按檔數、計數旗標按出現次數）：`soft_page_break` 12 檔、`raster_or_svg_image` 25 檔、`merged_cells` 17 檔、`notes_anchored` 12 檔、`text_box` 4 檔、`mimetype_not_first` 3 檔、`external_data_ref` 12 檔、`manifest_encryption_data` 2 檔、`manifest_declares_absent_part` 1 檔、`pii_residual` 2 檔。扣除特徵旗標後的病態檔 18 檔，逐檔清單在 `survey.json` 的 `summary.pathology_files`。重點病態：

1. mimetype 非首位 3 檔（第 2 節）：真實 upstream 檔，先紅規則由合成 fixture 雙支補齊（壓縮支、非首位支），對照組綠燈證明規則不過寬。
2. 加密容器 2 檔：`ext/lo_core_sw_qa_odfexport_sha1_correct.odt` 與 `sha1_wrong.odt`，鏡像 fdo76591 待遇入庫。每檔 5 個加密 entry，演算法 `Blowfish CFB`、金鑰派生 `PBKDF2`；`META-INF/manifest.xml` 帶 `manifest:encryption-data`。容器層必須能辨識、登記、跳過，不得當損壞檔；本期不實作解密。
3. 宣告不等於實際 1 檔：`ext/lo_core_sw_qa_odfexport_tdf152710.odt` 的 manifest 宣告了 8 個 zip 內不存在的 entry（6 個 `Pictures/` 圖片與 2 個 `Thumbnails/`），無損容器必須保留「宣告與實際的差異」，round-trip 後原樣還原而非憑猜補齊。
4. 外部資料引用 12 檔 48 條：分類為 `absolute_uri` 14 條（`bug59058` 的 http 圖鏈等，指向網路）、`parent_escape` 1 條（`ext/lo_core_sw_qa_odfimport_ooo32780-1.odt` 的 `../sw/qa/core/binfilter/-%20Update/Done/section_linked.sdw`，指向容器外上層路徑的連結文件，真外部引用）、`dangling_internal` 33 條（資源性 href 於 zip 內以精確字串找不到 entry）。dangling_internal 的主因是 LO 把 OLE 物件容器引用成 `./Object 2` 這種不含尾斜線的目錄前綴，實際檔案在 `Object 2/` 子樹下——解析器必須把目錄前綴解析為合法，否則這 33 條會變永久誤報；tdf152710 的 5 條 Pictures 則是真正的懸空（與病態 3 同源）。
5. 消毒哨兵與公開聯絡：`lo_out/lo__ooxmlexport__FDO78292.odt` 的 `svg:desc` 殘留 `C:\Users\redacted\…`（消毒後的哨兵路徑，非個資）、`ext/oasis_opendocument_v1p3_os_part1_introduction.odt` 帶 12 個公開 mailto（規範文件作者聯絡）。個資掃描器要把「消毒結果＋公開出版聯絡」與真實個資分開，否則紅燈永遠關不掉。

## 6. ODF-2 必蓋 case 清單

以下 13 項由 `scripts/odf_survey.py` 的 `required_cases()` 從本批實測反推生成，機讀版在 `survey.json` 上層 `required_cases`（每項附實檔證據），對應 core/odf crate 第一期的考題：

| 必蓋 case | 檔案證據 | 備註 |
|---|---|---|
| ODF2-01 mimetype 首 entry＋stored | `ext/…tdf140437.odt`、`ext/…tdf152710.odt`、`ext/…tdf77961.odt` | 真實違例 3 檔；壓縮支靠 fixture 先紅 |
| ODF2-02 加密容器只登記不解析 | `ext/…sha1_correct.odt`、`ext/…sha1_wrong.odt` | Blowfish CFB＋PBKDF2，每檔 5 加密 entry |
| ODF2-03 manifest 宣告但 zip 未帶 | `ext/…tdf152710.odt` | 8 個宣告缺檔，round-trip 保留差異 |
| ODF2-04 檔外資源引用 | 12 檔（`bug59058`、`ooo32780-1`、`fdo76098` 等） | 目錄前綴誤報 33 條要能解析；真懸空在 tdf152710 |
| ODF2-05 覆蓋格單複數寫法 | 17 檔（`bug65649` 43,181 個最多） | 本批複數寫法 0 現，規格允許，兩者都得吃 |
| ODF2-06 目錄是 text:table-of-content | 5 檔（OASIS 規範檔、`dropcap_toc_fields` 等） | `text:outline` 恆 0：卡面假設被實測推翻 |
| ODF2-07 軟分頁走 fo:break-before | 12 檔（`bug65649` 8 次最多） | LO 導出不用 text:soft-page-break |
| ODF2-08 腳註/尾註錨成對 | 12 檔（`form_footnotes`、`phpword reader` 等） | content 錨與 styles configuration 成對 |
| ODF2-09 頁首/頁尾 | 21 檔頁首、19 檔頁尾 | 住 styles.xml master-page 鏈 |
| ODF2-10 嵌入物件 draw:frame 家族 | 27 檔（`embedded-pdf.odt`、HC031 等） | chart/formula/OLE/PDF/spreadsheet 混雜 |
| ODF2-11 嵌入字型 | `lo__selfgen__embedded_font_odttf.odt` | x-font-ttf，容器一級公民 |
| ODF2-12 文字框 draw:text-box | 4 檔（`fdo77887`、`tdf105688`、WordWithAttachments、checkboxes） | K18 撐列模型輸入，位置尺寸屬性必留 |
| ODF2-13 消毒哨兵不誤報 | `FDO78292.odt`、OASIS 規範檔 | 消毒路徑＋公開 mailto 不計個資 |

## 7. 語料來源、授權與隔離機制

LO 轉出 40 檔（batch `odf-c1-lo`）：來源為既有 `corpus/` 的 DOCX（ooxmlexport 16、xopt 9、poi 8、phpword 3、selfgen 3、ooxmlimport 1），選樣刻意覆蓋表格、圖片、欄位、頁首、目錄、修訂、嵌入物件、雙向文字（`tdf87533_bidi`）等維度。轉換器 `/mnt/c/Program Files/LibreOffice/program/soffice.exe` 24.8.0.3（buildid `0bdf1299c94fe897b119f97f3c613e9dca6be583`），命令 `--convert-to odt:writer8`，每檔逐筆記於 manifest 的 `converter` 欄。LO 進程鐵律：`scripts/odf_build_corpus.py` 內建 `kill_own_lo()`，對齊 `scripts/render_lab.py` 口徑，只殺本輪自己以專屬 scratch profile 路徑拉起的 soffice，每批轉換後必自殺，不殘留 Windows 進程。soffice `--version` 在 WSL interop 下掛死，版本由 version.ini buildid 加 Windows 檔案版本屬性取得。LO 產出物授權口徑對齊既有 `corpus/`：上游來源檔授權逐檔繼承登記（MPL-2.0/GPL-3.0/LGPL-3.0 三重授權或來源專案授權），產出物隨來源授權走。

外部真實 ODT 10 檔（batch `odf-c1-ext`）：9 檔來自 LibreOffice/core 倉庫 `sw/qa/extras/odfexport/data` 與 `sw/qa/extras/odfimport/data`（MPL-2.0/GPL-3.0/LGPL-3.0 三重授權，tdf bugzilla 匿名化真實用戶文件），1 檔為 OASIS ODF 1.3 Part1 官方規範文件（docs.oasis-open.org 標準軌版權政策允許未修改再散布，授權原文摘錄登記於 manifest）。全部直接下載上游原檔，下載後以 sha256 固定，未經本機改寫。

quarantine 機制（`corpus-odt/quarantine.json`，鏡像 `corpus/quarantine.json` 格式）：授權查證不過不得入正庫。目前 `files` 空（無入庫後又被隔離者）；`tried_and_rejected` 記 3 筆：ODF-Alliance 測試語料 zip（回 200 但內容是 114 bytes 停放網域跳頁，授權與完整性雙查證不過）、EU Joinup ODF test suite（HTTPS 連線直接失敗 rc=6）、Wikimedia Commons ODT 批次（API 回 403，無法逐檔實查授權欄位，未實查不得登記授權）。

## 8. 自證與自抽查驗

三道閘門全綠（機讀版在 `survey.json` 上層 `gate`）：

1. **manifest 完備性自證**：`manifest_completeness()` 抓 50 檔的來源、授權、取得方式、sha256、bytes、重複、count 相符、quarantine 不重疊，本批 0 問題；授權欄寫裸字（僅 `MIT`、`Apache-2.0` 等字面值而無出處語境）會被擋。
2. **先紅自證**：`--self-test` 以 3 個合成 fixture 斷言 mimetype 非首位、mimetype 壓縮、缺 manifest.xml 三條規則確實報紅，再以對照組（正常 LO 檔）證明規則不過寬，實測輸出 RED-OK×3＋GREEN-OK×1。
3. **record 數自證**：survey.json 50 條對 manifest count 50，absent 0。

獨立手抽（X1a §7 口徑，`unzip -p`＋`grep -o` 對照 survey.json，全部逐筆相符）：

| 檔案 | 手抽 | survey.json |
|---|---|---|
| `lo_out/lo__poi__test-data_document_bug65649.odt` | `text:p` 14,161；覆蓋格單數 43,181、複數 0；17 entries | 14,161；43,181（複數 0）；17 |
| `ext/oasis_opendocument_v1p3_os_part1_introduction.odt` | `text:p` 177；`table:table` 0；10 entries | 177；0；10 |
| `lo_out/lo__selfgen__dropcap_toc_fields.odt` | `text:table-of-content` 1；`style:list-style` 0；17 entries | 1；0；17 |
| `ext/lo_core_sw_qa_odfexport_tdf140437.odt` | `unzip -l` 首位是 `Configurations2/accelerator/`（mimetype 不在首） | `mimetype_first_index=14` 違例 |
| `ext/lo_core_sw_qa_odfexport_WordTest_edit.odt` | `unzip -l` 首位 `mimetype` | `mimetype_first_index=0` 合規 |

冪等性：連跑兩次 `python3 scripts/odf_survey.py`，`cmp` 逐位元組相同。

繁中純度：以 opencc s2t 逐字掃描本卡觸碰面全部文字檔（s2t 後不變者為繁，改變者為簡體洩漏；白名單對齊 traditional-chinese-purity 口徑），首輪命中 11 處（八種字：帶、這、裡、體、無、庫、殘、應的各簡體寫法，散落於兩支腳本註解與 manifest／quarantine 的 why 欄），已全部修正為繁後複掃歸零；掃描腳本屬一次性工具，依觸碰面鐵律不入 repo。

## 9. survey.json 欄位導讀

`files` 陣列每檔一筆 record：`file`、`size`、`sha256`（含與 manifest 相符旗標）、`entries`（zip 層：index、name、compress_type、compressed、compress_size、file_size、encrypted_flag）、`n_entries`、`uncompressed_total`、`container`（has_content_xml／has_styles_xml／has_meta_xml／has_settings_xml／has_manifest_xml／has_manifest_rdf／has_thumbnails／has_configurations2／has_pictures／has_objects）、`content`（text_p、text_h、text_span、table_table、table_row、table_cell、table_covered_cells、跨欄跨列計數、draw_frame、draw_text_box、draw_image、text_outline_present、toc_present、toc_count、toc_source、toc_entry_templates、text_list、text_note、text_note_body、soft_page_break_fo、break_after_page_fo、text_a、office_version、xlink_href_total、external_data_ref_kinds、hyperlink_ref_count、external_href_count、external_href_samples）、`styles`（default_styles、named_styles、list_styles、page_layout_present、master_pages、font_faces、headers、footers、page_breaks）、`manifest`（manifest_version、media_types、encrypted、algorithms、key_derivations、checksum_types、declared_but_absent、present_but_undeclared）、`meta`（generator）、`pii_hits`、`pathology`（mimetype_present／mimetype_first_entry／mimetype_stored／mimetype_ok／mimetype_value／mimetype_first_index／encrypted_entries／缺件旗標）與 `flags`。

`summary` 收攏全域：`content_totals`、各特徵檔清單（`files_with_table`、`files_with_merged_cells`、`files_with_draw_frame`、`files_with_toc`、`files_with_notes`、`files_with_soft_page_break`、`files_with_text_box`、`files_with_header_style`、`files_with_footer_style`、`files_with_page_layout` 等）、各類病態清單（`mimetype_violation_files`、`encrypted_files`、`external_href_files`、`manifest_declares_absent_files`、`pii_files`、`sha_mismatch_files`）、版本與產生器分布（`manifest_versions`、`office_versions`、`media_types`、`generators`、`encryption_algorithms`）與 `pathology_files` 逐檔彙整。上層另有 `gate`（三閘門結果與先紅逐行輸出）與 `required_cases`（第 6 節 13 項機讀版）。

## 10. 後續（ODF-2 起）

core/odf crate 從第 6 節 13 項考題出發；第一批要優先鎖死的是 mimetype 三違例檔的讀取端容錯、加密 entry 的「辨識跳過報明」、宣告不等於實際的 round-trip 保留，以及 `./Object 2` 目錄前綴解析（不做則 33 條外部引用誤報永遠在）。Wikimedia Commons 那批 CC/PD 語料改天補 UA 或走鏡像站實查授權後再議入庫。
