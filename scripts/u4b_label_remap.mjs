#!/usr/bin/env node
/**
 * u4b_label_remap.mjs — K29e 入庫標籤失效事件：65 顆逐鈕重掛工具（可重跑的映射表工具）
 *
 * 紀律（沿用 t_57b7b089 立的契約）：
 * - 嚴禁字串批次全替：以 `data-command` 值為單位逐顆比對，命中映射表才改
 *   `data-requirement`＋`title`；映射表缺項＝跳過不改（查不到依據就不改）。
 * - 每顆改動都記 evidence 列（cmd|舊標籤|新標籤|三查依據）輸出到 CSV。
 * - 改動前後統計 data-requirement 總數，鈕數守恆（無鈕消失、無鈕憑空出現）。
 *
 * 標籤面（本卡依卡面「真需引擎新變體者按 mutation.rs 實查點名缺口標籤」授權新建，
 * 每個面的語意在 qword-core.js DEPENDENCY_MAP 登記，tooltip 與報告同一口徑）：
 *   live                     三查全備（欄位宣告＋寫入臂＋外殼真呼叫）
 *   pending:shell            引擎欄位真實存在，缺口純在外殼接線／面板／游標邏輯
 *   pending:ext              缺口在外部資源（詞庫／翻譯服務），引擎Mutation 面無缺口
 *   pending:engine-review    校閱面引擎缺口（修訂模型寫入、批註模型、保護、w:lang）
 *   pending:engine-refs      參考資料面引擎缺口（目錄／引文／索引／題注／書籤域寫入）
 *   pending:engine-drawing   圖形物件面引擎缺口（z 序調整、群組、旋轉、自訂繞排點）
 *   pending:engine-page      頁面與文件級版面面引擎缺口（紙張絕對尺寸、gutter、
 *                            pgBorders、w:background、lnNumType、autoHyphenation）
 *   pending:engine-theme     主題與預設值面引擎缺口（theme1.xml 寫入、docDefaults）
 *   pending:engine-para      字元／段落級直接格式殘餘缺口（w:bidi；w:lang 列校閱面）
 *
 * 用法：node scripts/u4b_label_remap.mjs [--dry]
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
// 可覆輸入源（U4b 追加）：--html <path> 從基準樹（git show main:...）重放映射，
// --evid <path> 另存 evidence CSV。用途：零風險再生成逐鈕依據表（basis 擴寫後），
// 同時佐證「同一工具、同一映射表」從 main 可重現出目前 index.html——非手改。
const argVal = (flag) => {
  const i = process.argv.indexOf(flag);
  return i >= 0 && process.argv[i + 1] ? path.resolve(process.argv[i + 1]) : null;
};
const HTML = argVal('--html') || path.join(ROOT, 'shells/web/editor/index.html');
const EVID = argVal('--evid') || path.join(ROOT, 'docs/word-ui-k29e-remap-evidence.csv');

const LIVE_T = '引擎已支援（K29e 字元格式切片），外殼接線完成';

// 共同依據前綴（同一引擎缺口族共用，逐鈕仍各自點名具體欄位證據）
const GAP_REVIEW = '查二/三：core/engine-api/src/mutation.rs 實查無修訂／批註／保護／語言寫入欄位——w:ins／w:del／w:comment／documentProtection／w:lang 於 mutation*.rs 0 命中（w:ins／w:del 僅 core/ooxml 只讀渲染，L3-revisions 考卷鎖）';
const GAP_REFS = '查二/三：mutation.rs 實查無目錄／引文／索引／題注／書籤域寫入變體（fldSimple／instrText 於 core/ooxml 僅只讀投影，theme/TOC 命中皆只讀）';
const GAP_DRAW = '查二/三：M5.2 ResizeOrMoveImage 欄位僅尺寸／定位／繞排模式；relativeHeight 於 mutation*.rs 僅插入期常值（1 處），無 z 序調整欄';
const GAP_PAGE = '查二/三：SectionPropsPatch 實讀僅 cols／margin_*_pt／orientation／section_start 族——無 pgSz 絕對寬高、gutter、pgBorders、lnNumType、autoHyphenation 欄；w:background 於 core/ 0 命中';
const GAP_THEME = '查二/三：theme1.xml 於 mutation*.rs 0 命中（ThemeColors／ThemeFonts 僅 core/ooxml/src/theme.rs 只讀解析）；docDefaults 於 mutation*.rs 0 命中（僅 styles.rs 解析層消費）';
const GAP_PARA = '查二/三：ParaPropsPatch 實讀僅 style_id／page_break_before／line_spacing／alignment 四欄；RunDirectPatch 實讀十欄無 bidi／lang';
const EXT_NOTE = '查二/三：詞庫／翻譯服務屬外部資源，非引擎 Mutation 缺口——比對結果落地用既有 M1.1 InsertText／M1.2 DeleteRange 路徑';
const SHELL_M64 = '查二：M6.4 InsertNote{kind:NoteKind2::Footnote/Endnote,pos,initial_text} 已宣告且 mutation_k29d.rs 有真實寫回路徑（K29d 已入 main）；查三：游標跳換與面板屬純外殼邏輯，不需新引擎欄位';

// ---- 映射表：cmd → { req, title, basis }（三查依據逐鈕寫死，非模板套用）----
const MAP = {
  // ================= PM 裁決：insert:picture 轉 live =================
  'insert:picture': {
    req: 'live', title: '插入圖片（M5.1 InsertInlineImage）：真實位元組進引擎＋Image 顯示項＋存檔 word/media/',
    basis: 'PM 裁決（本卡 comment 2026-11-13 18:42 第 1 條）依據 U4 合併 d6a6af1＋docs/word-ui-u4-wiring-report.md §3 閉環實據：Image 顯示項 0→1、75 bytes 像素、存檔 word/media/、canvas hash 翻轉、undo 還原；查二/三：InsertInlineImage{target,pos,media_bytes,mime_type,width_pt,height_pt,alt_text} 六欄逐欄對齊 mutation.rs 宣告（t_e1b68330 metadata.field_names_verified 自行實讀查證）'
  },
  // ================= 項一：K29e 六欄已接線 → live =================
  'format:underline': {
    req: 'live', title: `${LIVE_T}：underline（M2.1 RunDirectPatch）`,
    basis: '查一：qword-core.js format:underline 分支＋qword-ui.js handleFormatUnderline＋adapter.formatUnderline→buildFormatTextRangeMutation→apply_mutation_json；查二：mutation.rs RunDirectPatch.underline: Option<UnderlineKind>；查三：patch_rpr() 寫入臂 remove_element("w:u")＋<w:u w:val="{as_u_val()}"/>'
  },
  'format:underline-style': {
    req: 'live', title: '底線形態選單（E-2 解析保形態＋E-2b 共型寫入端真通）：單條／雙條／粗條／點狀／虛線／無六形態逐字寫入 w:u/@w:val，切換即存檔件逐字對。wavy／words 等未支援形態寫入端誠實拒絕，不入選單值域',
    basis: 'U4f 轉 live（原 U4b 記帳「styles.rs 折 Option<bool>＋發射一態」前提由 E-2（t_b5a4d0bf，docs/word-ui-e2-underline-report.md）＋E-2b（t_5e62f3b5，docs/word-ui-e2b-underline-shared-type-report.md）推翻，非外殼自修）：查一：qword-ui.js initPopovers 底線選單填充（值域=adapter UNDERLINE_KINDS）＋handleFormatUnderlineStyle→adapter.formatUnderline→apply_mutation_json；查二：RunDirectPatch.underline＝render-trait Underline 共型六形態；查三：patch_rpr() 寫入臂 <w:u w:val="{as_u_val()}"/> 逐形態真通＋閉環背書 shells/web/editor/tests/u4f_underline_style_live_probe.mjs（真 DOM click→存檔件 w:u/@w:val 逐字對＋wavy 誠實拒）'
  },
  'format:strikethrough': {
    req: 'live', title: `${LIVE_T}：strike（M2.1 RunDirectPatch）`,
    basis: '查一：三側分支齊備（core 匯流／UI handleFormatStrikethrough／adapter.formatStrikethrough）；查二：RunDirectPatch.strike: Option<StrikeKind>；查三：patch_rpr() 寫 <w:strike/>＋（Double 態加 <w:dstrike/>）／None 態寫 w:val="0"'
  },
  'format:subscript': {
    req: 'live', title: `${LIVE_T}：vert_align=Subscript（M2.1 RunDirectPatch）`,
    basis: '查一：core 發 format:vert-align mode=subscript→UI handleFormatVertAlign→adapter.formatVertAlign；查二：RunDirectPatch.vert_align: Option<quire_ooxml::VertAlign>；查三：patch_rpr() 寫 <w:vertAlign w:val="subscript"/>；發射層折 size×0.58＋基線 +base/11（=+1.0pt@11pt，docs/k29e-redlight.md §2 LO 真值）'
  },
  'format:superscript': {
    req: 'live', title: `${LIVE_T}：vert_align=Superscript（M2.1 RunDirectPatch）`,
    basis: '查一：同 subscript 路徑，mode=superscript；查二：同欄；查三：<w:vertAlign w:val="superscript"/>；發射層折 size×0.58＋基線 −0.35×base（k29e-redlight §2）'
  },
  'format:highlight': {
    req: 'live', title: `${LIVE_T}：highlight（M2.1 RunDirectPatch）`,
    basis: '查一：adapter.formatHighlight→patch.highlight（parseColorArgb 折 0xAARRGGBB）；查二：RunDirectPatch.highlight: Option<u32>；查三：patch_rpr() 命名色寫 <w:highlight w:val=name/>（16 命名色表 highlight_named_color()）、非命名色寫 <w:shd w:val="clear" w:fill=RRGGBB/>，解析層兩臂同落 RunStyle.highlight，發射同為 Rect 底方塊'
  },
  'format:highlight-palette': {
    req: 'live', title: '選擇文字醒目提示色彩（開色彩盤；真效果走 format:highlight 引擎路徑）',
    basis: '查一：本鈕是 popover 開關（data-popover-target=popover-color-highlight）＋盤面色票 dispatch format:color:highlight→core→UI handleFormatHighlight→adapter.formatHighlight；查二/三：同 format:highlight 六欄證據'
  },
  'font:family': {
    req: 'live', title: `${LIVE_T}：font_family（M2.1 RunDirectPatch，w:rFonts 四欄同族名）`,
    basis: '查一：qword-ui.js select change→dispatch font:family→adapter.setFontFamily；查二：RunDirectPatch.font_family: Option<String>；查三：patch_rpr() 寫 <w:rFonts w:ascii/hAnsi/cs/eastAsia=族名/>（「整包選族」口徑，該欄註解明文），重解析後烘進 effective_fonts 同吃斷行測寬'
  },
  'font:clear-formatting': {
    req: 'live', title: '清除選取範圍的直接字元格式（粗體／斜體／底線／刪除線／上下標／醒目提示／字型族七欄一次寫入；樣式鏈宣告按級聯照常顯形）',
    basis: '查一：adapter.clearDirectCharFormatting 七欄全走明確移除形態；查二：六欄＋bold/italic 全在 RunDirectPatch；查三：明確移除形態逐欄真實——Some(false)/UnderlineKind::None/StrikeKind::None/Baseline/alpha=0/空字串族名。誠實邊界：樣式鏈另有宣告時仍顯形（引擎 highlight 欄取捨註解明文拒絕虛構第四態），title 已照實寫「直接字元格式」不誇大'
  },
  // ================= 項二：K29e 已交付 → 逐鈕按真實缺口 =================
  // ---- 轉 pending:shell（引擎欄位真實存在，缺口純外殼）----
  'references:insert-footnote': {
    req: 'pending:shell', title: '插入註腳（引擎已支援，M6.4 InsertNote）：插入點選取與註腳面板屬外殼接線中',
    basis: SHELL_M64 + '；本鈕標籤 K29e→shell 屬錯掛家族修正（t_57b7b089 報告 §2 已點名交 PM 裁，本卡執行）'
  },
  'references:insert-endnote': {
    req: 'pending:shell', title: '插入尾註（引擎已支援，M6.4 InsertNote）：插入點選取與尾註面板屬外殼接線中',
    basis: SHELL_M64 + '；同 insert-footnote，t_57b7b089 點名之清償'
  },
  'references:next-footnote': {
    req: 'pending:shell', title: '跳到下一個註腳（引擎已支援註腳模型 M6.4）：游標跳換屬外殼工作',
    basis: '查一：js 零 handler；' + SHELL_M64
  },
  'review:display-markup': {
    req: 'pending:shell', title: '顯示標記切換：修訂標記渲染引擎已有（w:ins 底線／w:del 刪除線只讀投影，L3-revisions 考卷鎖），顯示切換屬外殼檢視態',
    basis: '查一：js 零 handler；查二：修訂只讀渲染真實存在（core/ooxml/src/lib.rs w:ins／w:del 臂）；查三：開關是純檢視態不需寫入欄位——唯一保留的校閱面 shell 判定'
  },
  // ---- pending:ext（外部資源依賴，引擎 Mutation 面無缺口）----
  'review:translate': {
    req: 'pending:ext', title: '翻譯（需外部翻譯服務）：結果替換走既有 M1.1／M1.2 路徑，非引擎欄位缺口',
    basis: '查一：js 零 handler；' + EXT_NOTE
  },
  'review:thesaurus': {
    req: 'pending:ext', title: '同義字（需外部詞庫）：候選替換走既有 M1.1／M1.2 路徑，非引擎欄位缺口',
    basis: '查一：js 零 handler；' + EXT_NOTE
  },
  'review:quick-spelling': {
    req: 'pending:ext', title: '即時拼寫檢查（需外部詞庫）：外殼標記疊加層待接，非引擎欄位缺口',
    basis: '查一：js 零 handler；查二：spell 於 core/ 0 命中（無拼字面屬正當邊界——詞庫非引擎職能）；' + EXT_NOTE
  },
  'review:spelling-grammar': {
    req: 'pending:ext', title: '拼字及文法檢查（需外部詞庫／語法服務）：非引擎欄位缺口',
    basis: '查一同 quick-spelling；' + EXT_NOTE
  },
  'review:reverse-spellcheck': {
    req: 'pending:ext', title: '反向拼寫檢查（需外部詞庫）：非引擎欄位缺口',
    basis: '查一同 quick-spelling；' + EXT_NOTE
  },
  // ---- pending:engine-review（校閱面引擎缺口）----
  'review:track-changes': { req: 'pending:engine-review', title: '修訂追蹤：引擎無修訂寫入面（w:ins／w:del 只讀），開關＋寫入需引擎切片', basis: '查一：js 零 handler；' + GAP_REVIEW },
  'review:accept-change': { req: 'pending:engine-review', title: '接受修訂：引擎無修訂模型寫入面，套用需引擎切片', basis: '查一同 track-changes；' + GAP_REVIEW },
  'review:reject-change': { req: 'pending:engine-review', title: '拒絕修訂：引擎無修訂模型寫入面，捨棄需引擎切片', basis: '查一同 track-changes；' + GAP_REVIEW },
  'review:reviewing-pane': { req: 'pending:engine-review', title: '修訂窗格：修訂列舉需修訂模型寫入面（引擎未實作）', basis: '查一同 track-changes；' + GAP_REVIEW },
  'review:new-comment': { req: 'pending:engine-review', title: '新增批註：引擎無批註資料模型（w:comment／word/comments.xml 0 命中）', basis: '查一：js 零 handler；查二/三：w:comment 於 core/ 0 命中——非已發行 K29d（節／頁首頁尾／註腳）範圍' },
  'review:delete-comment': { req: 'pending:engine-review', title: '刪除批註：引擎無批註資料模型', basis: '查一：js 零 handler；查二/三：w:comment／word/comments.xml 於 core/ 0 命中——批註資料模型（新增／刪除／解決／顯示／導覽跳轉所需的列舉與寫入面）完全缺失，非已發行 K29d（節／頁首頁尾／註腳）範圍；' + GAP_REVIEW },
  'review:resolve-comment': { req: 'pending:engine-review', title: '解決批註：引擎無批註資料模型（無 resolved 態可寫）', basis: '查一：js 零 handler；查二/三：w:comment／word/comments.xml 於 core/ 0 命中——批註資料模型（新增／刪除／解決／顯示／導覽跳轉所需的列舉與寫入面）完全缺失，非已發行 K29d（節／頁首頁尾／註腳）範圍；' + GAP_REVIEW },
  'review:show-comments': { req: 'pending:engine-review', title: '顯示批註：引擎無批註資料模型可列舉', basis: '查一：js 零 handler；查二/三：w:comment／word/comments.xml 於 core/ 0 命中——批註資料模型（新增／刪除／解決／顯示／導覽跳轉所需的列舉與寫入面）完全缺失，非已發行 K29d（節／頁首頁尾／註腳）範圍；' + GAP_REVIEW },
  'review:prev-comment': { req: 'pending:engine-review', title: '上一個批註：引擎無批註資料模型（無列舉來源可跳）', basis: '查一：js 零 handler；查二/三：w:comment／word/comments.xml 於 core/ 0 命中——批註資料模型（新增／刪除／解決／顯示／導覽跳轉所需的列舉與寫入面）完全缺失，非已發行 K29d（節／頁首頁尾／註腳）範圍；' + GAP_REVIEW },
  'review:next-comment': { req: 'pending:engine-review', title: '下一個批註：引擎無批註資料模型（無列舉來源可跳）', basis: '查一：js 零 handler；查二/三：w:comment／word/comments.xml 於 core/ 0 命中——批註資料模型（新增／刪除／解決／顯示／導覽跳轉所需的列舉與寫入面）完全缺失，非已發行 K29d（節／頁首頁尾／註腳）範圍；' + GAP_REVIEW },
  'review:restrict-editing': { req: 'pending:engine-review', title: '限制編輯：documentProtection 引擎未實作（0 命中）', basis: '查一：js 零 handler；查二/三：documentProtection 於 core/ 0 命中；' + GAP_REVIEW },
  'review:restrict-formatting': { req: 'pending:engine-review', title: '限制格式：documentProtection 引擎未實作', basis: '查一：js 零 handler；查二/三：documentProtection（含 editRestriction／formattingRestrictions 子面）於 core/ 0 命中——保護宣告的寫入與強制開關欄完全缺失；' + GAP_REVIEW },
  'review:enforce-protect': { req: 'pending:engine-review', title: '開始強制保護：documentProtection 引擎未實作', basis: '查一：js 零 handler；查二/三：documentProtection（含 editRestriction／formattingRestrictions 子面）於 core/ 0 命中——保護宣告的寫入與強制開關欄完全缺失；' + GAP_REVIEW },
  'review:stop-protect': { req: 'pending:engine-review', title: '停止保護：documentProtection 引擎未實作', basis: '查一：js 零 handler；查二/三：documentProtection（含 editRestriction／formattingRestrictions 子面）於 core/ 0 命中——保護宣告的寫入與強制開關欄完全缺失；' + GAP_REVIEW },
  'review:language': { req: 'pending:engine-review', title: '語言設定：RunDirectPatch 實讀無 w:lang 欄（K29e 交付的是六欄，lang 不在內）', basis: '查一：js 零 handler；查二/三：RunDirectPatch 十欄逐欄實讀無 lang；w:lang 於 core/ 僅 ooxml 註解提及' },
  // ---- pending:engine-refs（參考資料面引擎缺口）----
  'references:toc': { req: 'pending:engine-refs', title: '目錄目錄域寫入：引擎無目錄域 Mutation（讀取面已有 TOC 點引投影）', basis: '查一：js 零 handler；' + GAP_REFS },
  'references:update-toc': { req: 'pending:engine-refs', title: '更新目錄：域重算觸發需引擎欄位更新 Mutation', basis: '查一同 toc；' + GAP_REFS },
  'references:update-figures-table': { req: 'pending:engine-refs', title: '更新圖表目錄：域重算觸發需引擎欄位更新 Mutation', basis: '查一同 toc；' + GAP_REFS },
  'references:update-index': { req: 'pending:engine-refs', title: '更新索引：域重算觸發需引擎欄位更新 Mutation', basis: '查一同 toc；' + GAP_REFS },
  'references:mark-entry': { req: 'pending:engine-refs', title: '標記索引項：XE 索引條目寫入無引擎變體', basis: '查一：js 零 handler；' + GAP_REFS },
  'references:insert-index': { req: 'pending:engine-refs', title: '插入索引：索引域與標記表寫入無引擎變體', basis: '查一同 mark-entry；' + GAP_REFS },
  'references:add-text': { req: 'pending:engine-refs', title: '索引文字標記：需索引條目寫入欄位（引擎缺口）', basis: '查一同 mark-entry；' + GAP_REFS },
  'references:insert-citation': { req: 'pending:engine-refs', title: '插入引文：引文域與來源檔寫入無引擎變體', basis: '查一同 mark-entry；' + GAP_REFS },
  'references:mark-citation': { req: 'pending:engine-refs', title: '標記引文：引文標記條目寫入無引擎變體', basis: '查一同 mark-entry；' + GAP_REFS },
  'references:citation-style': { req: 'pending:engine-refs', title: '引文格式：styleSet 寫入需引擎欄位', basis: '查一同 mark-entry；' + GAP_REFS },
  'references:manage-sources': { req: 'pending:engine-refs', title: '來源管理員：來源清單寫回需引擎欄位（bibliography 於 core/ 0 命中）', basis: '查一同 mark-entry；' + GAP_REFS },
  'references:bibliography': { req: 'pending:engine-refs', title: '參考書目：書目域寫入需引擎欄位（bibliography 於 core/ 0 命中）', basis: '查一同 mark-entry；' + GAP_REFS },
  'references:insert-caption': { req: 'pending:engine-refs', title: '插入題注：SEQ 域與標題編號寫入無引擎變體', basis: '查一同 mark-entry；' + GAP_REFS },
  'references:insert-figures-table': { req: 'pending:engine-refs', title: '插入圖表目錄：目錄域（圖表型）寫入無引擎變體', basis: '查一同 mark-entry；' + GAP_REFS },
  'references:insert-authorities': { req: 'pending:engine-refs', title: '插入法律用詞目錄：authorities 域寫入無引擎變體（0 命中）', basis: '查一同 mark-entry；' + GAP_REFS },
  'insert:bookmark': { req: 'pending:engine-refs', title: '書籤：正文面 w:bookmarkStart／End 寫入無引擎變體（僅 notes.rs 註腳面解析）', basis: '查一：js 零 handler；查二：bookmark 於 core/ 僅 core/ooxml/src/notes.rs 註腳面；' + GAP_REFS },
  'insert:cross-reference': { req: 'pending:engine-refs', title: '交叉參考：REF 域與書籤解析寫入無引擎變體', basis: '查一同 bookmark；' + GAP_REFS },
  'insert:toc-part': { req: 'pending:engine-refs', title: '目錄節：目錄域寫入無引擎變體', basis: '一同 references:toc；' + GAP_REFS },
  // ---- pending:engine-drawing（圖形物件面引擎缺口）----
  'layout:bring-forward': { req: 'pending:engine-drawing', title: '前移一層：z 序調整欄引擎未實作（relativeHeight 僅插入期常值）', basis: '查一：js 零 handler；' + GAP_DRAW },
  'layout:send-backward': { req: 'pending:engine-drawing', title: '後移一層：z 序調整欄引擎未實作', basis: '同 bring-forward；' + GAP_DRAW },
  'layout:rotate': { req: 'pending:engine-drawing', title: '旋轉：rot 寫入欄引擎未實作（無旋轉變體）', basis: '查一：js 零 handler；查二/三：M5.2 欄位無 rot；' + GAP_DRAW },
  'layout:group': { req: 'pending:engine-drawing', title: '群組：group／ungroup 寫入引擎未實作（v:group 僅只讀解析）', basis: '查一同 rotate；' + GAP_DRAW },
  'layout:align': { req: 'pending:engine-drawing', title: '物件對齊：物件級對齊基準需引擎欄位（段落對齊另有 live 路徑，勿混淆）', basis: '查一：js 零 handler；查二/三：無物件對齊變體；' + GAP_DRAW },
  'layout:edit-wrap-points': { req: 'pending:engine-drawing', title: '編輯繞排點：自訂繞排點欄引擎未實作（WrapModeSpec 僅五模式列舉）', basis: '查一同 rotate；' + GAP_DRAW },
  'dialog:arrange': { req: 'pending:engine-drawing', title: '排列對話框：z 序／對齊／群組／旋轉／繞排點五效果全屬引擎缺口', basis: '查一：js 零 handler；' + GAP_DRAW },
  // ---- pending:engine-page（頁面與文件級版面面引擎缺口）----
  'layout:size': { req: 'pending:engine-page', title: '紙張大小：絕對寬高寫入欄引擎未實作（開檔期 new_with_paper 非 Mutation 路徑）', basis: '查一：js 零 handler；' + GAP_PAGE },
  'design:layout-size': { req: 'pending:engine-page', title: '版面大小：同 layout:size——絕對寬高欄引擎未實作（M6.1 僅 orientation 互換）', basis: '同 layout:size；' + GAP_PAGE },
  'design:layout-gutter': { req: 'pending:engine-page', title: '裝訂線：gutter 於 mutation.rs 0 命中（解析期僅併入 left 邊距），寫入欄未實作', basis: '查一：js 零 handler；查二：gutter 僅 sectpr.rs 解析期併入 left＋mutation_k29d.rs 樣板寫 0；' + GAP_PAGE },
  'design:page-color': { req: 'pending:engine-page', title: '頁面色彩：w:background 於 core/ 0 命中，寫入欄未實作', basis: '查一：js 零 handler；' + GAP_PAGE },
  'design:page-borders': { req: 'pending:engine-page', title: '頁面框線：pgBorders 於 core/ 0 命中（與表格邊框同族引擎卡，PM backlog）', basis: '查一同 page-color；' + GAP_PAGE },
  'design:watermark': { req: 'pending:engine-page', title: '浮水印：w:background／頁首浮水印寫入欄未實作', basis: '查一同 page-color；' + GAP_PAGE },
  'layout:line-numbers': { req: 'pending:engine-page', title: '行號：lnNumType 於 core/ 0 命中（未解析亦無寫入欄）', basis: '查一同 page-color；' + GAP_PAGE },
  'layout:hyphenation': { req: 'pending:engine-page', title: '連字：autoHyphenation 設定欄未實作（uax14 斷字計價已有，非設定欄）', basis: '查一同 page-color；查二：hyphen 於 core/ 命中皆 layout/uax14.rs 計價旗標非設定欄；' + GAP_PAGE },
  // ---- pending:engine-theme（主題與預設值面引擎缺口）----
  'design:theme': { req: 'pending:engine-theme', title: '主題：theme1.xml 寫入無引擎變體（clrScheme／fontScheme／effectScheme 僅只讀投影）', basis: '查一：js 零 handler；' + GAP_THEME },
  'design:colors': { req: 'pending:engine-theme', title: '色彩：clrScheme 寫入無引擎變體（ThemeColors 僅只讀查表）', basis: '同 design:theme；' + GAP_THEME },
  'design:fonts': { req: 'pending:engine-theme', title: '字型：fontScheme 寫入無引擎變體（ThemeFonts 僅只讀解析）', basis: '同 design:theme；' + GAP_THEME },
  'design:effects': { req: 'pending:engine-theme', title: '效果：effectScheme 寫入無引擎變體（僅 fmtScheme 只讀解析）', basis: '同 design:theme；' + GAP_THEME },
  'design:style-set': { req: 'pending:engine-theme', title: '樣式集：M2.3 UpsertNamedStyle 限單一樣式，樣式集需 docDefaults＋批次欄位（未實作）', basis: '查一同 design:theme；' + GAP_THEME },
  'design:set-as-default': { req: 'pending:engine-theme', title: '設為預設值：docDefaults 寫入無引擎變體（0 命中，僅解析層）', basis: '同 design:style-set；' + GAP_THEME },
  'design:paragraph-spacing': { req: 'pending:engine-theme', title: '全文件預設段落間距：需 docDefaults 寫入（M2.2 ParaPropsPatch 只改選取段落）', basis: '同 design:style-set；' + GAP_THEME },
  // ---- pending:engine-para（字元／段落級直接格式殘餘缺口）----
  'design:layout-direction': { req: 'pending:engine-para', title: '文字方向：w:bidi 寫入欄引擎未實作（ParaPropsPatch 四欄實讀無 bidi）', basis: '查一：js 零 handler；查二：bidi 於 core/ 命中皆只讀註解／表格新式名；' + GAP_PARA }
};

const dry = process.argv.includes('--dry');
const html = fs.readFileSync(HTML, 'utf-8');
const lines = html.split('\n');
const evidence = [];
let changed = 0;
const skipped = [];

for (let i = 0; i < lines.length; i += 1) {
  const line = lines[i];
  const cmdMatch = line.match(/data-command="([^"]+)"/);
  if (!cmdMatch) continue;
  if ((line.match(/data-command="/g) || []).length > 1) {
    skipped.push(`${cmdMatch[1]}(一行多鈕，title 正則不安全，本工具不處理)`);
    continue;
  }
  const cmd = cmdMatch[1];
  const spec = MAP[cmd];
  if (!spec) {
    if (/data-requirement="pending:K29e"/.test(line) || /data-requirement="pending:K29b"/.test(line)) {
      skipped.push(`${cmd}(映射缺項，依紀律不改)`);
    }
    continue;
  }
  const reqMatch = line.match(/data-requirement="([^"]+)"/);
  if (!reqMatch) { skipped.push(`${cmd}(無 req 欄)`); continue; }
  const oldReq = reqMatch[1];
  let next = line;
  next = next.replace(/data-requirement="[^"]*"/, `data-requirement="${spec.req}"`);
  if (spec.title) {
    if (/title="[^"]*"/.test(next)) {
      next = next.replace(/title="[^"]*"/, `title="${spec.title}"`);
    } else {
      // 元素無 title 欄（font:family 的 <select>）：插到 data-requirement 之後
      next = next.replace(/data-requirement="[^"]*"/, `data-requirement="${spec.req}" title="${spec.title}"`);
    }
  }
  if (next !== line) {
    evidence.push([cmd, oldReq, spec.req, spec.basis].join('|'));
    if (!dry) lines[i] = next;
    changed += 1;
  }
}

const before = (html.match(/data-requirement="[^"]*"/g) || []).length;
const afterHtml = lines.join('\n');
const after = (afterHtml.match(/data-requirement="[^"]*"/g) || []).length;
console.log(`改動 ${changed} 顆；跳過 ${skipped.length} 顆`);
if (skipped.length) console.log('跳過清單:', skipped.join(' / '));
if (before !== after) {
  console.error(`鈕數不守恆！before=${before} after=${after}`);
  process.exit(1);
}
console.log(`data-requirement 總數守恆：${before} = ${after}`);
const census = {};
for (const m of afterHtml.match(/data-requirement="[^"]*"/g) || []) {
  census[m] = (census[m] || 0) + 1;
}
console.log('改後統計：');
for (const [k, v] of Object.entries(census).sort((a, b) => b[1] - a[1])) console.log(`  ${v}\t${k}`);
if (!dry) {
  fs.writeFileSync(HTML, afterHtml);
  fs.writeFileSync(EVID, 'command|old_label|new_label|三查依據\n' + evidence.join('\n') + '\n');
  console.log(`已寫入 ${path.relative(ROOT, HTML)} 與 ${path.relative(ROOT, EVID)}`);
}
