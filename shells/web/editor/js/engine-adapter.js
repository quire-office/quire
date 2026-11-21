/**
 * Engine Adapter (真引擎接線適配器)
 * 負責：
 * 1. 載入並初始化 WebAssembly 產物 (quire_web.js + quire_web_bg.wasm)
 * 2. 封裝 WasmEngine 開檔 (openFile)、多頁 Canvas 繪製 (renderPage)
 * 3. 封裝 Mutation 操作 (InsertText, DeleteRange, InsertBreak, InsertTable) 與打字閉環 (typeAt, backspace, enter)
 * 4. 封裝復原／重做 (undo, redo) 與存檔下載 (save)
 * 5. 游標與座標反查 (caretAt, caretRect) 及版面定案狀態 (isSettled, reflowStep)
 */

import init, { initSync, WasmEngine } from '../../pkg/quire_web.js';
import {
  DEFAULT_DISPLAY_WIDTH_PX,
  renderDisplayListToCanvas,
  validateDocumentFile,
  sniffContainerKind
} from '../vendor/renderer.js';

/**
 * 預設示範文件 Base64 (fixtures/sample.docx, 1309 bytes)
 * 零網路依賴、離線即開
 */
export const DEFAULT_SAMPLE_DOCX_B64 =
  'UEsDBBQAAAAIAAAATl3JTxqw6wAAAK4BAAATAAAAW0NvbnRlbnRfVHlwZXNdLnhtbH1QvU7DMBDeeQrLK4' +
  'odGBBCSTrwMwJDeYCTfUks7LPlc0v79jht6YAK4933q69b7YIXW8zsIvXyRrVSIJloHU29/Fi/NPdScAGy' +
  '4CNhL/fIcjVcdet9QhZVTNzLuZT0oDWbGQOwigmpImPMAUo986QTmE+YUN+27Z02kQpSacriIYfuCUfY+C' +
  'Ked/V9LJLRsxSPR+KS1UtIyTsDpeJ6S/ZXSnNKUFV54PDsEl9XgtQXExbk74CT7q0uk51F8Q65vEKoLP0V' +
  's9U2mk2oSvW/zYWecRydwbN+cUs5GmSukwevzkgARz/99WHu4RtQSwMEFAAAAAgAAABOXbmBRHGwAAAAKg' +
  'EAAAsAAABfcmVscy8ucmVsc43POw7CMAwG4J1TRN5pWgaEUJMuCKkrKgeIEjeNaB5KwqO3JwMDIAZG278/' +
  'y233sDO5YUzGOwZNVQNBJ70yTjM4D8f1DkjKwikxe4cMFkzQ8VV7wlnkspMmExIpiEsMppzDntIkJ7QiVT' +
  '6gK5PRRytyKaOmQciL0Eg3db2l8d0A/mGSXjGIvWqADEvAf2w/jkbiwcurRZd/nPhKFFlEjZnB3UdF1atd' +
  'FRYob+nHi/wJUEsDBBQAAAAIAAAATl2xL20dIwIAAF8aAAARAAAAd29yZC9kb2N1bWVudC54bWztmc9v0z' +
  'AUx+/8FU85cKPpBoxR2uywCYkTv4rENU28NpDEIUmXlVNVDSiCjQlKRbdJHUJURYRtF6RqFPHHsDjNaf8C' +
  'blOGhJC4zZa8S2zr2e/j98339JxfWLVMWEGuZ2C7IM1kshIgW8O6YZcL0r3i9QvzEni+auuqiW1UkGrIkx' +
  'aUc/kgp2OtaiHbB5rB9nJBQar4vpOTZU+rIEv1MthBNo0tY9dSfbp0y3KAXd1xsYY8jwIsU57NZudkSzVs' +
  'SaEpS1ivjUdn8rnlToa7fs1EEORWVLMgFQ3fRJKs5OWTDeknnZfSyHTlK7erhosgab8gh5uk/YxsD0adt/' +
  'Hm06jTJ/0PZCcc7/bTM2nOE/w0Q1LvkHf7R4P60bfvcatP2kNYvAHx1hrZqUcHr2Dp5uJ9SHP9rDfunFct' +
  '59oSRPu95HMraaxFG10yCEefesfDLRqC6MeTZLeZBuLGIUz2Qxz2Rp0uPf/f6xQrCB5VDe0hlFwc2LCMV+' +
  'FB1XI8wPQngk/Dpvq4BjouZ6Z1j0mt7l/VHw9fxtt7pJ8Wtx6tv09239CqooMm+fiFbLyOnzfpfeIwhBkg' +
  'e1/p/Ax9ymgezTDLThax0Tya4SI7WcRG82iGS+xkERvNoxkus5NFbDSPZphjJ4vYaB7NcIWdLGKjeTTDPD' +
  'tZxEbzaIar7GQRG81l0ynLsP8iOptLQ7BsyInO5tIQDJtywrP/YQj59/ON/OdpSPkFUEsBAhQDFAAAAAgA' +
  'AABOXclPGrDrAAAArgEAABMAAAAAAAAAAAAAAIABAAAAAFtDb250ZW50X1R5cGVzXS54bWxQSwECFAMUAA' +
  'AACAAAAE5duYFEcbAAAAAqAQAACwAAAAAAAAAAAAAAgAEcAQAAX3JlbHMvLnJlbHNQSwECFAMUAAAACAAA' +
  'AE5dsS9tHSMCAABfGgAAEQAAAAAAAAAAAAAAgAH1AQAAd29yZC9kb2N1bWVudC54bWxQSwUGAAAAAAMAAw' +
  'C5AAAARwQAAAAA';

/**
 * 取得預設示範檔 Uint8Array 位元組
 */
export function getDefaultSampleBytes() {
  if (typeof Buffer !== 'undefined') {
    return new Uint8Array(Buffer.from(DEFAULT_SAMPLE_DOCX_B64, 'base64'));
  }
  const binaryString = atob(DEFAULT_SAMPLE_DOCX_B64);
  const bytes = new Uint8Array(binaryString.length);
  for (let i = 0; i < binaryString.length; i++) {
    bytes[i] = binaryString.charCodeAt(i);
  }
  return bytes;
}

/**
 * 建立 M1.1 InsertText 變更物件
 */
export function buildInsertTextMutation(para, cp, text, styleOverride = null) {
  return {
    InsertText: {
      target: 'Body',
      pos: { para: Number(para), cp: Number(cp) },
      text: String(text),
      style_override: styleOverride
    }
  };
}

/**
 * 建立 M1.2 DeleteRange 變更物件（單段半開區間）
 */
export function buildDeleteRangeMutation(para, startCp, endCp) {
  return {
    DeleteRange: {
      range: {
        target: 'Body',
        start: { para: Number(para), cp: Number(startCp) },
        end: { para: Number(para), cp: Number(endCp) }
      }
    }
  };
}

/**
 * 建立 M1.5 InsertBreak 變更物件
 * kind: 'LineWrapping' | 'Page' | 'Column' | 'Tab'
 */
export function buildInsertBreakMutation(para, cp, kind = 'LineWrapping') {
  return {
    InsertBreak: {
      target: 'Body',
      pos: { para: Number(para), cp: Number(cp) },
      kind
    }
  };
}

/**
 * K29e 底線形態映射（U4b）：把外殼的「底線樣式」語意折算成引擎
 * `UnderlineKind` 串列枚舉。錨點＝`core/engine-api/src/mutation.rs` 的
 * `enum UnderlineKind { Single, Double, Dotted, Dashed, None }`（serde 預設
 * 外標＝變體名原字串，非 snake_case，故此處逐字用大寫首字母形態）。
 *
 * 誠實紀律：引擎端 `w:u/@w:val` 寫入是五形態齊備的（`as_u_val()` 五臂），
 * 但**解析層只把 `w:u` 折成 `Option<bool>`（core/ooxml/src/styles.rs
 * `b"u" => rs.underline = Some(toggle_on_attr(e))`）**——形態不進 RunStyle，
 * 發射層因此只能繪「有／無底線」兩態。這是外殼接不通的真實斷點，
 * 逐字記在 docs/word-ui-k29e-remap-report.md §2.3；`format:underline-style`
 * 據此不得掛 live。
 */
export const UNDERLINE_KINDS = ['Single', 'Double', 'Dotted', 'Dashed', 'None'];

/**
 * 建立刪除線形態值（引擎 `StrikeKind { Single, Double, None }`）。
 * 同 UnderlineKind 口徑：枚舉名原字串外標。
 */
export const STRIKE_KINDS = ['Single', 'Double', 'None'];

/**
 * K29e 上下標值（引擎 `RunDirectPatch::vert_align` 直接沿用級聯層
 * `quire_ooxml::VertAlign { Baseline, Superscript, Subscript }`，單一真值源、
 * 不另造鏡像枚舉——見 mutation.rs 該欄註解）。
 */
export const VERT_ALIGN_VALUES = ['Baseline', 'Superscript', 'Subscript'];

/**
 * K29e highlight 十六個命名色（ECMA-376 §17.3.2.15 ST_HighlightColor）。
 * 對齊根＝`core/engine-api/src/mutation.rs` 的 `highlight_named_color()`：
 * 命中此表 → 寫 `<w:highlight w:val="<name>"/>`（Word 高亮 UI 的互通形態）；
 * 未命中 → 走 `<w:shd w:fill="RRGGBB"/>` 底紋路徑。兩條路徑在解析層同落
 * `RunStyle.highlight`，發射同為文字底方塊，故外殼兩態都算真接線。
 * alpha=0（0x00RRGGBB）＝明確移除，引擎寫 `<w:highlight w:val="none"/>`。
 */
export const HIGHLIGHT_NAMED_COLORS = [
  { name: 'black', rgb: 0x000000 },
  { name: 'blue', rgb: 0x0000FF },
  { name: 'cyan', rgb: 0x00FFFF },
  { name: 'darkblue', rgb: 0x000080 },
  { name: 'darkcyan', rgb: 0x008080 },
  { name: 'darkgray', rgb: 0x808080 },
  { name: 'darkgreen', rgb: 0x008000 },
  { name: 'darkmagenta', rgb: 0x800080 },
  { name: 'darkred', rgb: 0x800000 },
  { name: 'darkyellow', rgb: 0x808000 },
  { name: 'green', rgb: 0x00FF00 },
  { name: 'lightgray', rgb: 0xD3D3D3 },
  { name: 'magenta', rgb: 0xFF00FF },
  { name: 'red', rgb: 0xFF0000 },
  { name: 'white', rgb: 0xFFFFFF },
  { name: 'yellow', rgb: 0xFFFF00 }
];

/**
 * 正規化底線形態輸入：接受引擎枚舉名（'Single'…）、OOXML 字面（'single'…）
 * 或 `false`／'none'（明確移除）。回傳引擎枚舉名，非法值回傳 null（呼叫端
 * 據此拒絕，不得憑空選一個形態塞給引擎）。
 */
export function normalizeUnderlineKind(value) {
  if (value === undefined || value === null) return null;
  if (value === false) return 'None';
  if (typeof value === 'string') {
    const lowered = value.trim().toLowerCase();
    if (lowered === 'none' || lowered === '0' || lowered === 'false') return 'None';
    // OOXML 字面 `dashed` 與相容別名 `dash` 都收（mutation.rs 註解：Word
    // 相容讀取端也吃 dash）；`thick`／`wavy` 等未入引擎枚舉的形態一律拒。
    const alias = { single: 'Single', double: 'Double', dotted: 'Dotted', dashed: 'Dashed', dash: 'Dashed' };
    return alias[lowered] || null;
  }
  return null;
}

/**
 * 正規化上下標輸入：接受 'superscript'／'subscript'／'baseline'（不分大小寫）、
 * 引擎枚舉名，或布林 true（上標）。非法回 null。
 */
export function normalizeVertAlign(value) {
  if (value === undefined || value === null) return null;
  if (value === true) return 'Superscript';
  if (value === false) return 'Baseline';
  if (typeof value === 'string') {
    const lowered = value.trim().toLowerCase();
    const alias = {
      superscript: 'Superscript', super: 'Superscript', sup: 'Superscript',
      subscript: 'Subscript', sub: 'Subscript',
      baseline: 'Baseline', normal: 'Baseline'
    };
    return alias[lowered] || null;
  }
  return null;
}

/**
 * 把 '#RRGGBB'／'#AARRGGBB'／數值 折成引擎要的 0xAARRGGBB。
 * 傳回 undefined＝輸入不可解析（呼叫端據此拒絕，不憑空退黑色）。
 * alpha 缺省補 0xFF（引擎 highlight 欄以 alpha==0 判「明確移除」，
 * 故外殼絕不能把一個可見色誤補成 alpha=0）。
 */
export function parseColorArgb(color) {
  if (typeof color === 'number' && Number.isFinite(color)) {
    return (color >>> 0);
  }
  if (typeof color === 'string') {
    const clean = color.replace('#', '').trim();
    if (/^[0-9a-fA-F]{6}$/.test(clean)) {
      return (0xFF000000 | parseInt(clean, 16)) >>> 0;
    }
    if (/^[0-9a-fA-F]{8}$/.test(clean)) {
      return parseInt(clean, 16) >>> 0;
    }
  }
  return undefined;
}

/**
 * 建立 M2.1 FormatTextRange 變更物件
 * @param {Object} range - { start: { para, cp }, end: { para, cp }, target?: string }
 * @param {Object} patch - { bold?, italic?, color_rgb?, size_pt?, char_style?,
 *                           underline?, strike?, vert_align?, highlight?, font_family? }
 *
 * 欄位契約逐欄對齊 `core/engine-api/src/mutation.rs` 的 `RunDirectPatch`（11 欄：
 * bold／italic／color_rgb／size_pt／char_style／underline／strike／vert_align／
 * highlight／font_family ＋ serde default）。語意紀律（引擎檔頭明文，全欄統一）：
 * - `undefined`／未帶 → `null`＝保持原宣告（patch 不碰該欄，XML 原樣保留）；
 * - 帶值 → 明確設定；
 * - 移除類值（`false`／'None'／'Baseline'／alpha=0 色／空字串族名）＝明確移除，
 *   引擎寫出顯式關閉形態，級聯兜底不得再補回。
 * 這裡不得把 `null` 折成某個具體值，否則會憑空對文件寫入移除宣告。
 */
export function buildFormatTextRangeMutation(range, patch = {}) {
  const normStart = range?.start || { para: 0, cp: 0 };
  const normEnd = range?.end || normStart;

  let pStart = { para: Number(normStart.para), cp: Number(normStart.cp) };
  let pEnd = { para: Number(normEnd.para), cp: Number(normEnd.cp) };
  if (pStart.para > pEnd.para || (pStart.para === pEnd.para && pStart.cp > pEnd.cp)) {
    const tmp = pStart;
    pStart = pEnd;
    pEnd = tmp;
  }

  return {
    FormatTextRange: {
      range: {
        target: range?.target || 'Body',
        start: pStart,
        end: pEnd
      },
      patch: {
        bold: patch.bold !== undefined ? patch.bold : null,
        italic: patch.italic !== undefined ? patch.italic : null,
        color_rgb: patch.color_rgb !== undefined ? patch.color_rgb : null,
        size_pt: patch.size_pt !== undefined ? patch.size_pt : null,
        char_style: patch.char_style !== undefined ? patch.char_style : null,
        // K29e 六欄（U4b 接線）：呼叫端已正規化（見 normalizeUnderlineKind／
        // normalizeVertAlign／parseColorArgb），此處只做「未帶→null＝不碰」的
        // 欄位級換算，不再二次猜值。傳給引擎的形態：
        //   underline  = 'Single'|'Double'|'Dotted'|'Dashed'|'None'（serde 外部標
        //                的無欄位列舉＝字串變體名）
        //   strike     = 'Single'|'Double'|'None'
        //   vert_align = 'Superscript'|'Subscript'|'Baseline'
        //   highlight  = 0xAARRGGBB 數值（alpha=0＝明確移除）
        //   font_family= 字串（空字串＝明確移除直接層宣告）
        underline: patch.underline !== undefined ? patch.underline : null,
        strike: patch.strike !== undefined ? patch.strike : null,
        vert_align: patch.vert_align !== undefined ? patch.vert_align : null,
        highlight: patch.highlight !== undefined ? patch.highlight : null,
        font_family: patch.font_family !== undefined ? patch.font_family : null
      }
    }
  };
}

/**
 * 建立 M2.2 SetParagraphStyle 變更物件
 * @param {Object|number} paraRange - { start: number, end: number } 或單一數字
 * @param {Object} patch - { alignment?: string, style_id?: string, page_break_before?: boolean, line_spacing?: number }
 */
export function buildSetParagraphStyleMutation(paraRange, patch = {}) {
  let start = 0;
  let end = 0;
  if (typeof paraRange === 'number') {
    start = paraRange;
    end = paraRange;
  } else if (paraRange) {
    start = Number(paraRange.start || 0);
    end = Number(paraRange.end !== undefined ? paraRange.end : start);
  }
  if (start > end) {
    const tmp = start;
    start = end;
    end = tmp;
  }

  return {
    SetParagraphStyle: {
      target: 'Body',
      para_range: { start, end },
      patch: {
        style_id: patch.style_id !== undefined ? patch.style_id : null,
        page_break_before: patch.page_break_before !== undefined ? patch.page_break_before : null,
        line_spacing: patch.line_spacing !== undefined ? patch.line_spacing : null,
        alignment: patch.alignment !== undefined ? patch.alignment : null
      }
    }
  };
}

/**
 * 建立 M4.1 InsertTable 變更物件（K29c 切片，引擎側 mutation.rs 的
 * `Mutation::InsertTable` 宣告為準：target／after_para／rows／cols／
 * col_widths_pt 五欄，逐欄對應，未帶欄位以 null 明確缺席）。
 * @param {number|null} afterPara - 插入錨點段落序（null＝body 尾段之後）
 * @param {number} rows - 列數（引擎要求 > 0）
 * @param {number} cols - 欄數（引擎要求 > 0）
 * @param {Array<number>|null} colWidthsPt - 各欄寬（pt）；null＝引擎預設 100pt
 */
export function buildInsertTableMutation(afterPara, rows, cols, colWidthsPt = null) {
  return {
    InsertTable: {
      target: 'Body',
      after_para: afterPara === null || afterPara === undefined ? null : Number(afterPara),
      rows: Number(rows),
      cols: Number(cols),
      col_widths_pt: colWidthsPt || null
    }
  };
}

/**
 * 建立表格穩定路徑 TablePath（欄位名逐欄照抄
 * `core/engine-api/src/mutation.rs` 的 `pub struct TablePath`：target／
 * root_slot／nested_steps）。
 *
 * nested_steps 每一步為 (row_idx, cell_idx, sub_table_idx)，指向該格內第幾個
 * 巢狀 `<w:tbl>`。serde 契約是 `Vec<(usize, usize, usize)>`——JSON 必須是
 * **數值陣列**；實測傳 `{row_idx,cell_idx,sub_table_idx}` 物件會被引擎拒
 * （`invalid type: map, expected a tuple of size 3`，U4d 考卷 L3 實錄）。
 * 故本層把兩種輸入形態（陣列／具名欄位物件）正規化成陣列，且**不完整或
 * 長度不對的形態直接拋錯**——把壞形態靜默送引擎等於把失敗藏進錯誤訊息裡。
 * 巢狀表定位走 `table_locator_json()` 的真反查（wasm 殼已綁，合併點 db865bf），
 * 外殼不再靠越界探針考古。
 *
 * @param {number} rootSlot - 根表序號（OoxmlDoc::tables 的序）
 * @param {Array<Array<number>|{row_idx:number,cell_idx:number,sub_table_idx:number}>} [nestedSteps]
 *        巢狀步進，預設空陣列＝根表
 */
export function buildTablePath(rootSlot, nestedSteps = []) {
  if (!Array.isArray(nestedSteps)) {
    throw new Error(`nested_steps 需為陣列，收到 ${JSON.stringify(nestedSteps)}`);
  }
  const steps = nestedSteps.map((step, i) => {
    if (Array.isArray(step)) {
      if (step.length !== 3 || step.some((v) => !Number.isInteger(Number(v)) || Number(v) < 0)) {
        throw new Error(`nested_steps[${i}] 需為 [row_idx, cell_idx, sub_table_idx] 三個非負整數，收到 ${JSON.stringify(step)}`);
      }
      return step.map(Number);
    }
    if (step && typeof step === 'object') {
      const triple = [step.row_idx, step.cell_idx, step.sub_table_idx].map(Number);
      if (triple.some((v) => !Number.isInteger(v) || v < 0)) {
        throw new Error(`nested_steps[${i}] 物件形態需齊備 row_idx／cell_idx／sub_table_idx 非負整數，收到 ${JSON.stringify(step)}`);
      }
      return triple;
    }
    throw new Error(`nested_steps[${i}] 形態非法（三元組陣列或具名欄位物件）：${JSON.stringify(step)}`);
  });
  return {
    target: 'Body',
    root_slot: Number(rootSlot),
    nested_steps: steps
  };
}

/**
 * 把 hex 色彩正規化成引擎要的 RRGGBB 六碼（大寫）。
 *
 * 為何必須在外殼擋：`TablePropsPatch.shading_fill`／`CellPropsPatch.shading_fill`
 * 的非法值（如 'GGGGGG'、'FF000000'）引擎回 `ok=true` 卻完全不發射 Rect
 * （實測：填 'GGGGGG' → ok=true、Rect 0 筆）。若不擋，使用者按了底紋鈕看到
 * 沒變化、外殼卻回報成功——這是把失敗寫成成功，紅燈考卷明令禁止。
 *
 * @param {string|null|undefined} value - '#RRGGBB'／'RRGGBB'／'auto'／null
 * @returns {string|null} 正規化後的 RRGGBB；null＝明確不動該欄
 */
export function normalizeHex6Fill(value) {
  if (value === null || value === undefined) return null;
  const s = String(value).trim().replace(/^#/, '');
  if (s.toLowerCase() === 'auto') return 'auto';
  if (/^[0-9a-fA-F]{6}$/.test(s)) return s.toUpperCase();
  return null;
}

/**
 * 是否為引擎可判讀的底紋值（'auto' 或六碼 hex）。非法值回 false 讓上層誠實拒絕。
 */
export function isFillValueValid(value) {
  if (value === null || value === undefined) return true;
  return normalizeHex6Fill(value) !== null;
}

/**
 * 列／欄插入方位正規化（引擎 `InsertSide` 只有 Before／After 兩個變體，
 * `mutation.rs:132-137`；其他字面值 serde 直接 reject，故在此先擋）。
 */
export function normalizeInsertSide(side, fallback = 'After') {
  const s = String(side ?? fallback).trim().toLowerCase();
  if (s === 'before' || s === 'left' || s === 'above') return 'Before';
  if (s === 'after' || s === 'right' || s === 'below') return 'After';
  return null;
}

/**
 * 儲存格垂直對齊正規化（`CellPropsPatch.vertical_align` 寫 `w:vAlign/@w:val`，
 * `mutation.rs:210-212`；引擎按字面值寫入，故外殼只放行三個合規值）。
 */
export function normalizeVerticalAlign(value) {
  if (value === null || value === undefined) return null;
  const s = String(value).trim().toLowerCase();
  if (s === 'top' || s === 'center' || s === 'bottom') return s;
  return null;
}

/**
 * 建立 M4.3 InsertTableRow 變更物件
 * 欄位逐欄對 `mutation.rs:302-306`：table／at_row／position
 * @param {Object} table - buildTablePath() 產出的 TablePath
 * @param {number} atRow - 基準列序
 * @param {string} [position] - 'Before' | 'After'
 */
export function buildInsertTableRowMutation(table, atRow, position = 'After') {
  return {
    InsertTableRow: {
      table,
      at_row: Number(atRow),
      position
    }
  };
}

/**
 * 建立 M4.3 DeleteTableRow 變更物件（連帶 vMerge 頂格晉升由引擎處理）
 * 欄位對 `mutation.rs:308`：table／row
 */
export function buildDeleteTableRowMutation(table, row) {
  return {
    DeleteTableRow: {
      table,
      row: Number(row)
    }
  };
}

/**
 * 建立 M4.4 InsertTableColumn 變更物件（同步修補 tblGrid 與各列由引擎做）
 * 欄位對 `mutation.rs:310-315`：table／at_grid_col／position／width_pt
 * width_pt 是引擎的**必填** f32（非 Option），故未帶時給引擎同款預設 100pt。
 */
export function buildInsertTableColumnMutation(table, atGridCol, position = 'After', widthPt = 100) {
  return {
    InsertTableColumn: {
      table,
      at_grid_col: Number(atGridCol),
      position,
      width_pt: Number(widthPt)
    }
  };
}

/**
 * 建立 M4.4 DeleteTableColumn 變更物件
 * 欄位對 `mutation.rs:317`：table／grid_col
 */
export function buildDeleteTableColumnMutation(table, gridCol) {
  return {
    DeleteTableColumn: {
      table,
      grid_col: Number(gridCol)
    }
  };
}

/**
 * 建立 M4.6 SetTableProps 變更物件
 * patch 欄位**只放行三欄**：width_pt／alignment／shading_fill。引擎的 serde
 * 對未知欄位是靜默忽略（實測 patch:{bogus_field:123} 回 ok=true 且顯示列表
 * 零變化），因此本建構器只放行這三欄、其餘一律丟棄，避免外殼送出「看起來
 * 成功、實際沒做事」的假接線。
 * 邊框（U4d 重估）：E 卡 2cb6a18 已把 `borders: Option<TableBordersPatch>`
 * 送進引擎（合規 patch 實測有 Rule 發射），但外殼邊框接線不在本卡工作面——
 * 未接線前本建構器仍不放行 borders／borderPt，等 PM 開工時一併接上。
 */
export function buildSetTablePropsMutation(table, patch = {}) {
  return {
    SetTableProps: {
      table,
      patch: {
        width_pt: patch.width_pt !== undefined && patch.width_pt !== null ? Number(patch.width_pt) : null,
        alignment: patch.alignment !== undefined ? patch.alignment : null,
        shading_fill: patch.shading_fill !== undefined ? patch.shading_fill : null
      }
    }
  };
}

/**
 * 建立 M4.6 SetCellProps 變更物件
 * patch 欄位五欄（`mutation.rs:201-219`）：width_pt／shading_fill／
 * vertical_align／grid_span／v_merge。同樣只放行宣告過的欄位。
 * v_merge 為 `VMergeState` 三態（None／Restart／Continue，`mutation.rs:190-199`），
 * serde 口徑是變體名字串串。
 */
export function buildSetCellPropsMutation(table, row, cell, patch = {}) {
  return {
    SetCellProps: {
      table,
      row: Number(row),
      cell: Number(cell),
      patch: {
        width_pt: patch.width_pt !== undefined && patch.width_pt !== null ? Number(patch.width_pt) : null,
        shading_fill: patch.shading_fill !== undefined ? patch.shading_fill : null,
        vertical_align: patch.vertical_align !== undefined ? patch.vertical_align : null,
        grid_span: patch.grid_span !== undefined && patch.grid_span !== null ? Number(patch.grid_span) : null,
        v_merge: patch.v_merge !== undefined ? patch.v_merge : null
      }
    }
  };
}

/**
 * 建立 M5.1 InsertInlineImage 變更物件
 * 欄位逐欄對 `mutation.rs:353-361`：target／pos／media_bytes／mime_type／
 * width_pt／height_pt／alt_text。
 *
 * media_bytes 是 `Vec<u8>`——serde_json 的預設口徑是**數字陣列**（實測
 * Array.from(uint8array) 可被引擎接受並寫進 word/media/*）。媒體位元組真的
 * 進引擎，外殼不畫佔位框。
 */
export function buildInsertInlineImageMutation(pos, mediaBytes, mimeType, widthPt, heightPt, altText = null) {
  return {
    InsertInlineImage: {
      target: 'Body',
      pos: { para: Number(pos.para), cp: Number(pos.cp) },
      media_bytes: Array.from(mediaBytes),
      mime_type: String(mimeType),
      width_pt: Number(widthPt),
      height_pt: Number(heightPt),
      alt_text: altText === null || altText === undefined || altText === '' ? null : String(altText)
    }
  };
}

/**
 * 從圖片位元組讀取原始像素寬高（僅讀檔頭中繼資料，不是排版計算）。
 * 用途：插入時給引擎一個維持原始長寬比的合理尺寸，避免拉伸變形。
 * 不支援的格式回 null，由呼叫端退到中性預設值。
 * @param {Uint8Array} bytes
 * @returns {{w:number,h:number}|null}
 */
export function readImageNaturalSize(bytes) {
  if (!bytes || bytes.length < 24) return null;
  const u = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  const be16 = off => (u[off] << 8) | u[off + 1];
  const le16 = off => u[off] | (u[off + 1] << 8);
  const be32 = off => ((u[off] << 24) >>> 0) + (u[off + 1] << 16) + (u[off + 2] << 8) + u[off + 3];
  // PNG：IHDR 的 width／height 在 16／20 位元組（大端 u32）
  if (u[0] === 0x89 && u[1] === 0x50 && u[2] === 0x4e && u[3] === 0x47) {
    const w = be32(16), h = be32(20);
    return w > 0 && h > 0 ? { w, h } : null;
  }
  // GIF：Logical Screen Descriptor 在 6／8（小端 u16）
  if (u[0] === 0x47 && u[1] === 0x49 && u[2] === 0x46) {
    const w = le16(6), h = le16(8);
    return w > 0 && h > 0 ? { w, h } : null;
  }
  // JPEG：走 SOF0/SOF2 區段取寬高
  if (u[0] === 0xff && u[1] === 0xd8) {
    let off = 2;
    while (off + 9 < u.length) {
      if (u[off] !== 0xff) { off++; continue; }
      const marker = u[off + 1];
      const len = be16(off + 2);
      if (marker >= 0xc0 && marker <= 0xcf && marker !== 0xc4 && marker !== 0xc8 && marker !== 0xcc) {
        const h = be16(off + 5), w = be16(off + 7);
        return w > 0 && h > 0 ? { w, h } : null;
      }
      if (marker === 0xd8 || marker === 0x01 || (marker >= 0xd0 && marker <= 0xd9)) { off += 2; continue; }
      off += 2 + len;
    }
    return null;
  }
  return null;
}

/**
 * U3 轉為 live 的指令清單與映射表
 */
export const U3_LIVE_COMMANDS = [
  'format:bold',
  'format:italic',
  'format:font-color',
  'format:font-color-palette',
  'format:color:font',
  'font:grow',
  'font:shrink',
  'font:size',
  'paragraph:align-left',
  'paragraph:align-center',
  'paragraph:align-right',
  'paragraph:align-justify',
  'style:normal',
  'style:no-spacing',
  'style:h1',
  'style:h2',
  'style:h3'
];

/**
 * U2 / U3 累積 live 指令清單與映射表
 */
export const LIVE_COMMANDS = [
  // 檔案與儲存
  'file:save',
  'file:open',
  'file:open-sample',
  // 編輯與歷程
  'edit:undo',
  'edit:redo',
  'edit:type',
  'edit:backspace',
  'edit:enter',
  // U3 樣式與段落格式 live
  ...U3_LIVE_COMMANDS,
  // 縮放控制
  'zoom:in',
  'zoom:out',
  'zoom:reset',
  'zoom:100',
  'zoom:fit-width',
  'zoom:one-page',
  'zoom:multiple-pages',
  'zoom:dialog',
  // 檢視模式與頁面
  'view:mode:print',
  'view:mode:read',
  'view:mode:web',
  'view:outline',
  'view:focus',
  'view:draft',
  'page:next',
  'page:prev',
  'page:goto',
  'status:page',
  'status:word-count',
  'status:collab',
  // 外殼基本能力
  'ruler:toggle',
  'gridlines:toggle',
  'navpane:toggle',
  'help:open',
  'table:grid-select',
  'symbol:palette-select'
];

/**
 * 自 U1 pending 清單晉升至 live 的按鈕指令集合 (U2 + U3)
 */
export const TRANSFERRED_LIVE_COMMANDS = [
  'file:save', // pending:K29f -> live (save_docx_bytes)
  'edit:undo', // pending:K29a -> live (undo_json)
  'edit:redo', // pending:K29a -> live (redo_json)
  ...U3_LIVE_COMMANDS
];

/**
 * 正規化一筆引擎回傳字串（U4d L9：wasm 邊界的誠實底線）。
 *
 * 為何必須包：引擎回傳是**外部資料**——壞字串（OOM 截斷、panic 前殘字、未來
 * API 改版）若直接 JSON.parse，會把一次編輯操作炸成未捕獲例外，用戶看到的是
 * 整個編輯器無回應，而不是「這次操作失敗」。
 *
 * 兩級口徑（皆不猜、不重試、不把壞字串當成功）：
 * - safeEngineJson：唯讀面（顯示列表／游標反查／索引摘要…）。壞字串與空字串
 *   一律回 null（「無資料」語義）＋console.warn 留痕，呼叫端既有 null 分支天然
 *   處理。
 * - engineMutationResult：變更面（apply/undo/redo）。回傳必須帶 ok:false＋可讀
 *   error 給上層彈窗與失敗日誌，絕不回 null 讓 `res.ok` 炸在呼叫端。
 * @param {string|null|undefined} text - 引擎回傳的 JSON 字串
 * @param {string} label - 哪個 API 的回傳（進錯誤訊息與 warn，供查案）
 * @returns {Object|null}
 */
function safeEngineJson(text, label) {
  if (text === null || text === undefined || text === '') return null;
  try {
    return JSON.parse(text);
  } catch (err) {
    console.warn(`[EngineAdapter] ${label} 回傳非合法 JSON（${err.message}）：${String(text).slice(0, 80)}`);
    return null;
  }
}

/** 變更面專用：壞字串/空字串 → ok:false＋rejectedByShell（上層走失敗路徑，零未捕獲例外）。 */
function engineMutationResult(text, label) {
  if (text === null || text === undefined || text === '') {
    return { ok: false, rejectedByShell: true, badEngineJson: true, error: `${label} 回傳空字串，無法判定結果` };
  }
  try {
    return JSON.parse(text);
  } catch (err) {
    const head = String(text).slice(0, 80);
    return {
      ok: false,
      rejectedByShell: true,
      badEngineJson: true,
      error: `${label} 回傳非合法 JSON（${err.message}）：${head}`
    };
  }
}

/**
 * 引擎適配器類別
 */
export class EngineAdapter {
  constructor() {
    this.engine = null;
    this.fileName = 'sample.docx';
    this.caret = { para: 0, cp: 0 };
    this.selection = null; // { start: { para, cp }, end: { para, cp } }
    this.isInitialized = false;
    this.isDirty = false;
    /**
     * 最近一次 InsertTable 落地後的根表序（U4d L6）：由 insertTable 用引擎真
     * 索引反查寫入，**不是** tableCount()-1 的盲設猜測——游標在前段插表時新表
     * 不排在最後，盲設會指向舊表。null＝尚未反查得出來。
     */
    this.lastInsertedRootSlot = null;
  }

  /**
   * 初始化 wasm 模組（支援瀏覽器與 Node.js 執行環境）
   */
  async init(customSource) {
    if (this.isInitialized) return;

    if (customSource) {
      if (typeof initSync === 'function') {
        initSync(customSource);
      } else {
        await init(customSource);
      }
      this.isInitialized = true;
      return;
    }

    // Node.js 環境適配
    if (typeof window === 'undefined' && typeof process !== 'undefined' && process.versions?.node) {
      const fs = await import('node:fs');
      const path = await import('node:path');
      const wasmPath = path.resolve(
        path.dirname(new URL(import.meta.url).pathname),
        '../../pkg/quire_web_bg.wasm'
      );
      const wasmBytes = fs.readFileSync(wasmPath);
      initSync({ module: wasmBytes });
      this.isInitialized = true;
      return;
    }

    // 瀏覽器標準 ESM 載入
    await init();
    this.isInitialized = true;
  }

  /**
   * 載入 DOCX 位元組陣列並建立真引擎實例
   */
  openFile(bytes, fileName = 'sample.docx') {
    const val = validateDocumentFile(fileName, bytes);
    if (!val.valid) {
      const err = new Error(val.error || '不合法的檔案格式');
      err.reason = val.reason;
      throw err;
    }

    try {
      this.engine = new WasmEngine(bytes);
      this.fileName = fileName;
      this.caret = { para: 0, cp: 0 };
      this.isDirty = false;
      return this.engine;
    } catch (e) {
      const msg = e && e.message ? e.message : String(e);
      throw new Error(`引擎解析文件失敗: ${msg}`);
    }
  }

  /**
   * 開啟預設示範文件
   */
  openDefaultSample() {
    const bytes = getDefaultSampleBytes();
    return this.openFile(bytes, 'sample.docx');
  }

  /**
   * 繪製指定頁面顯示列表至 Canvas
   */
  async renderPage(pageIndex, canvas, displayWidthPx = DEFAULT_DISPLAY_WIDTH_PX) {
    if (!this.engine) throw new Error('尚未載入文件，無法渲染頁面');
    const dl = safeEngineJson(this.engine.page_display_list_json(pageIndex), 'page_display_list_json');
    if (!dl) return null;
    return await renderDisplayListToCanvas(dl, canvas, displayWidthPx);
  }

  /**
   * 取得指定頁顯示列表物件
   */
  getPageDisplayList(pageIndex) {
    if (!this.engine) return null;
    return safeEngineJson(this.engine.page_display_list_json(pageIndex), 'page_display_list_json');
  }

  /**
   * 總頁數
   */
  pageCount() {
    return this.engine ? this.engine.page_count() : 0;
  }

  /**
   * 總段落數
   */
  paragraphCount() {
    return this.engine ? this.engine.paragraph_count() : 0;
  }

  /**
   * 段落純文字
   */
  paragraphText(para) {
    return this.engine ? this.engine.paragraph_text(para) || '' : '';
  }

  /**
   * 統計總字數（正文字元碼點總和）
   */
  getWordCount() {
    if (!this.engine) return 0;
    const count = this.engine.paragraph_count();
    let total = 0;
    for (let i = 0; i < count; i++) {
      const t = this.engine.paragraph_text(i) || '';
      total += Array.from(t.trim()).length;
    }
    return total;
  }

  /**
   * 游標座標反查 (點擊 page, x_pt, y_pt -> CaretPos)
   */
  caretAt(page, xPt, yPt) {
    if (!this.engine) return null;
    return safeEngineJson(this.engine.caret_at_json(page, xPt, yPt), 'caret_at_json');
  }

  /**
   * 游標幾何矩形反查 (para, cp -> CaretRect)
   */
  caretRect(para, cp) {
    if (!this.engine) return null;
    return safeEngineJson(this.engine.caret_rect_json(para, cp), 'caret_rect_json');
  }

  /**
   * 版面是否已定案
   */
  isSettled() {
    return this.engine ? this.engine.is_settled() : true;
  }

  /**
   * 背景續排一步
   */
  reflowStep() {
    if (!this.engine) return { settled: true, events: [] };
    return safeEngineJson(this.engine.reflow_step_json(), 'reflow_step_json') || { settled: true, events: [] };
  }

  /**
   * 在指定或目前游標處打字 (M1.1 InsertText)
   */
  typeAt(para, cp, text) {
    if (!this.engine) throw new Error('尚未開啟文件，無法輸入文字');
    const targetPara = para !== undefined ? Number(para) : (this.caret ? this.caret.para : 0);
    const targetCp = cp !== undefined ? Number(cp) : (this.caret ? this.caret.cp : 0);
    const mutation = buildInsertTextMutation(targetPara, targetCp, text);
    const res = engineMutationResult(this.engine.apply_mutation_json(JSON.stringify(mutation)), 'apply_mutation_json（InsertText）');

    if (res.ok) {
      this.isDirty = true;
      if (res.outcome && res.outcome.caret_after) {
        this.caret = { ...res.outcome.caret_after };
      } else {
        this.caret = { para: targetPara, cp: targetCp + Array.from(text).length };
      }
    }
    return res;
  }

  /**
   * 退格鍵 (M1.2 DeleteRange 單段前刪)
   */
  backspace(para, cp) {
    if (!this.engine) throw new Error('尚未開啟文件，無法退格');
    const targetPara = para !== undefined ? Number(para) : (this.caret ? this.caret.para : 0);
    const targetCp = cp !== undefined ? Number(cp) : (this.caret ? this.caret.cp : 0);

    if (targetCp <= 0) {
      return {
        ok: false,
        error: '已位於段落開頭，單段刪除暫不支援跨段合併'
      };
    }

    const mutation = buildDeleteRangeMutation(targetPara, targetCp - 1, targetCp);
    const res = engineMutationResult(this.engine.apply_mutation_json(JSON.stringify(mutation)), 'apply_mutation_json（DeleteRange）');

    if (res.ok) {
      this.isDirty = true;
      if (res.outcome && res.outcome.caret_after) {
        this.caret = { ...res.outcome.caret_after };
      } else {
        this.caret = { para: targetPara, cp: targetCp - 1 };
      }
    }
    return res;
  }

  /**
   * Enter 鍵 (M1.5 InsertBreak 軟換行)
   */
  enter(para, cp) {
    if (!this.engine) throw new Error('尚未開啟文件，無法換行');
    const targetPara = para !== undefined ? Number(para) : (this.caret ? this.caret.para : 0);
    const targetCp = cp !== undefined ? Number(cp) : (this.caret ? this.caret.cp : 0);
    const mutation = buildInsertBreakMutation(targetPara, targetCp, 'LineWrapping');
    const res = engineMutationResult(this.engine.apply_mutation_json(JSON.stringify(mutation)), 'apply_mutation_json（InsertBreak）');

    if (res.ok) {
      this.isDirty = true;
      if (res.outcome && res.outcome.caret_after) {
        this.caret = { ...res.outcome.caret_after };
      }
    }
    return res;
  }

  /**
   * 復原操作
   */
  undo() {
    if (!this.engine) throw new Error('尚未開啟文件，無法復原');
    const res = engineMutationResult(this.engine.undo_json(), 'undo_json');

    if (res.ok && res.undone) {
      this.isDirty = true;
      if (res.outcome && res.outcome.caret_after) {
        this.caret = { ...res.outcome.caret_after };
      }
    }
    return res;
  }

  /**
   * 重做操作
   */
  redo() {
    if (!this.engine) throw new Error('尚未開啟文件，無法重做');
    const res = engineMutationResult(this.engine.redo_json(), 'redo_json');

    if (res.ok && res.redone) {
      this.isDirty = true;
      if (res.outcome && res.outcome.caret_after) {
        this.caret = { ...res.outcome.caret_after };
      }
    }
    return res;
  }

  /**
   * 存檔並觸發瀏覽器下載
   */
  save(fileName = this.fileName || 'document.docx') {
    if (!this.engine) throw new Error('尚未開啟文件，無法存檔');
    const bytes = this.engine.save_docx_bytes();

    if (typeof document !== 'undefined' && typeof window !== 'undefined') {
      const blob = new Blob([bytes], {
        type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
      });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = fileName.endsWith('.docx') ? fileName : `${fileName}.docx`;
      document.body.appendChild(a);
      a.click();
      setTimeout(() => {
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
      }, 200);
    }
    this.isDirty = false;
    return bytes;
  }

  /**
   * 取得有效格式化文字範圍（優先使用 selection；未選取時降級至整段）
   */
  getEffectiveRange() {
    if (this.selection && (this.selection.start.para !== this.selection.end.para || this.selection.start.cp !== this.selection.end.cp)) {
      return this.selection;
    }
    const para = this.caret ? this.caret.para : 0;
    const text = this.paragraphText(para) || '';
    const cpEnd = Array.from(text).length;
    return {
      target: 'Body',
      start: { para, cp: 0 },
      end: { para, cp: cpEnd }
    };
  }

  /**
   * 取得有效段落範圍（優先使用 selection 所涵蓋段落；未選取時為目前段）
   */
  getEffectiveParaRange() {
    if (this.selection) {
      const start = Math.min(this.selection.start.para, this.selection.end.para);
      const end = Math.max(this.selection.start.para, this.selection.end.para);
      return { start, end };
    }
    const para = this.caret ? this.caret.para : 0;
    return { start: para, end: para };
  }

  /**
   * 套用字元級直接格式 (M2.1 FormatTextRange)
   */
  formatTextRange(range, patch = {}) {
    if (!this.engine) throw new Error('尚未開啟文件，無法格式化文字');
    const targetRange = range || this.getEffectiveRange();
    const mutation = buildFormatTextRangeMutation(targetRange, patch);
    const res = engineMutationResult(this.engine.apply_mutation_json(JSON.stringify(mutation)), 'apply_mutation_json（FormatTextRange）');

    if (res.ok) {
      this.isDirty = true;
      if (res.outcome && res.outcome.caret_after) {
        this.caret = { ...res.outcome.caret_after };
      }
    }
    return res;
  }

  /**
   * 套用段落級屬性或樣式 (M2.2 SetParagraphStyle)
   */
  setParagraphStyle(paraRange, patch = {}) {
    if (!this.engine) throw new Error('尚未開啟文件，無法設定段落樣式');
    const targetRange = paraRange || this.getEffectiveParaRange();
    const mutation = buildSetParagraphStyleMutation(targetRange, patch);
    const res = engineMutationResult(this.engine.apply_mutation_json(JSON.stringify(mutation)), 'apply_mutation_json（SetParagraphStyle）');

    if (res.ok) {
      this.isDirty = true;
      if (res.outcome && res.outcome.caret_after) {
        this.caret = { ...res.outcome.caret_after };
      }
    }
    return res;
  }

  /**
   * 插入表格 (M4.1 InsertTable，K29c 引擎能力)
   * @param {number} rows - 列數
   * @param {number} cols - 欄數
   * @param {Object} options - { afterPara?: number|null, colWidthsPt?: number[]|null }
   */
  insertTable(rows, cols, options = {}) {
    if (!this.engine) throw new Error('尚未開啟文件，無法插入表格');
    const afterPara = options.afterPara !== undefined
      ? options.afterPara
      : (this.caret ? this.caret.para : 0);
    const mutation = buildInsertTableMutation(afterPara, rows, cols, options.colWidthsPt || null);
    const res = engineMutationResult(this.engine.apply_mutation_json(JSON.stringify(mutation)), 'apply_mutation_json（InsertTable）');

    if (res && res.ok) {
      this.isDirty = true;
      if (res.outcome && res.outcome.caret_after) {
        this.caret = { ...res.outcome.caret_after };
      }
      // U4d L6：記下新表 root_slot——用引擎真索引反查（游標段落落點），不給
      // 呼叫端 tableCount()-1 的盲設值。實測（U4d 探針 E）：root_slot 按 XML
      // 文檔序排，游標在前段插表時新表可能落在**任何** slot（探針實錄：第二張
      // 插的表拿到 root_slot 0，先插的反被擠到 1）——「新表在末張」只是觀察，
      // 不是契約。契約只認反查真值：引擎回 caret_after（插表後游標落進新表首
      // 格，探針實錄）、該段落反查命中哪張，哪張才是剛插的表；引擎未給回訊或
      // 反查不可得時老實記 null（游標可能還停在舊表，沿用會誤記），由 UI 誠實
      // 回報，絕不猜。
      const caretReported = !!(res.outcome && res.outcome.caret_after);
      const located = caretReported ? this.currentTableCell() : null;
      this.lastInsertedRootSlot = located && Number.isInteger(located.rootSlot) ? located.rootSlot : null;
      res.inserted = { rootSlot: this.lastInsertedRootSlot, afterPara };
    }
    return res;
  }

  // ---------------------------------------------------------------------
  // U4：表格家族（M4.3／M4.4／M4.6）與圖片插入（M5.1）真接線
  //
  // 紀律：幾何全部由引擎側 quire_layout::table::TableInput 決定，本層只產
  // 操作描述＋讀引擎回傳。U4d 起，「讀表格結構」一律走引擎真索引（t_f91e5773
  // 綁定，合併點 db865bf）：
  //   張數      engine.table_count()
  //   行列／欄數 engine.tables_summary_json()（每張根表 {root_slot, rows, grid_cols,…}）
  //   游標落點  engine.table_locator_json(para) → {table_path, cell_path}
  // 舊版在此處用「對越界 root_slot／列序發必然被拒的 Mutation、從錯誤訊息正則
  // 反推行列數」的零副作用探針補位——那是在真索引缺席時的考古手段，且有致命
  // 缺陷：引擎對 len()<=1 的表**先**回「不得刪除表格僅剩的一列／一欄」，訊息
  // 不含「共 N 列」，單列／單欄表格的探針必失效（mutation_k29c.rs 列路徑與欄
  // 路徑的 len()<=1 早退）。真索引落地後探針已全數撤除（U4d L1，紅燈考卷實錄
  // 見 docs/word-ui-u4d-fixes-report.md）。
  // ---------------------------------------------------------------------

  /**
   * 目前正文根表張數（引擎 canonical 投影，只讀）。回 null＝引擎未載入。
   */
  tableCount() {
    if (!this.engine) return null;
    const n = this.engine.table_count();
    return Number.isInteger(n) && n >= 0 ? n : null;
  }

  /**
   * 引擎表格索引摘要（每張根表一筆：root_slot／rows／grid_cols／para_start／
   * para_end…）。回傳字串壞掉時老實回 null，不猜。
   * @returns {Array<Object>|null}
   */
  tableSummaries() {
    if (!this.engine) return null;
    const parsed = safeEngineJson(this.engine.tables_summary_json(), 'tables_summary_json');
    if (!parsed) return null;
    return Array.isArray(parsed) ? parsed : null;
  }

  /**
   * 讀指定根表的列數／網格欄數——真索引版：在 tables_summary_json 的清單裡
   * 找 root_slot 對應的那筆，直接讀 rows／grid_cols。單列／單欄表格與多列表格
   * 同路徑，不再有「越界探針在 len()<=1 時失效」的死角。
   * @returns {{rows:number|null, cols:number|null, ok:boolean, error:string|null}}
   */
  tableShape(rootSlot = 0) {
    if (!this.engine) return { rows: null, cols: null, ok: false, error: '尚未開啟文件' };
    const slot = Number(rootSlot);
    if (!Number.isInteger(slot) || slot < 0) {
      return { rows: null, cols: null, ok: false, error: `root_slot 需為非負整數，收到 ${rootSlot}` };
    }
    const summaries = this.tableSummaries();
    if (summaries === null) {
      return { rows: null, cols: null, ok: false, error: '無法讀取引擎表格索引（tables_summary_json 回傳異常）' };
    }
    const hit = summaries.find((s) => Number(s.root_slot) === slot);
    if (!hit) {
      return { rows: null, cols: null, ok: false, error: `根表序 ${slot} 不存在（文件共 ${summaries.length} 張根表）` };
    }
    return { rows: Number(hit.rows), cols: Number(hit.grid_cols), ok: true, error: null };
  }

  /**
   * 游標段落 → 表格座標反查（引擎 table_locator_json，真索引）。
   * @param {number} para - 段落序（省略＝目前游標）
   * @returns {{rootSlot:number, row:number|null, cell:number|null, nestedSteps:Array, tablePath:Object, cellPath:Object}|null}
   *          該段在表（含巢狀表）格內回結構；表外正文段或反查不可得回 null（null 不是錯誤）
   */
  locateTableCell(para) {
    if (!this.engine) return null;
    const target = para !== undefined && para !== null ? Number(para) : (this.caret ? this.caret.para : null);
    if (!Number.isInteger(target) || target < 0) return null;
    const raw = this.engine.table_locator_json(target);
    if (raw === null || raw === undefined || raw === '') return null; // 表外正文段：引擎如實回 None
    const loc = safeEngineJson(raw, 'table_locator_json');
    if (!loc || !loc.table_path) return null;
    return {
      rootSlot: Number(loc.table_path.root_slot),
      nestedSteps: Array.isArray(loc.table_path.nested_steps) ? loc.table_path.nested_steps : [],
      row: loc.cell_path ? Number(loc.cell_path.row) : null,
      cell: loc.cell_path ? Number(loc.cell_path.cell) : null,
      tablePath: loc.table_path,
      cellPath: loc.cell_path || null
    };
  }

  /** 目前游標是否落在某張表的格內（省略座標的表格操作以此決定目標表）。 */
  currentTableCell() {
    return this.locateTableCell(this.caret ? this.caret.para : undefined);
  }

  /**
   * 套用一筆表格類 Mutation：呼叫引擎、解析 outcome、回傳原始結果給上層。
   * 上層（qword-ui）依 ok 決定是否重排，失敗一律如實回報，不得寫成成功。
   *
   * U4d L5（幽靈游標）：表格類 Mutation 的 outcome.caret_after 實測恆 null
   * （引擎不給游標回訊），刪列後游標段落可能已隨 XML 縮掉而越界——之後打字
   * 引擎會回「段落索引越界」（紅燈實錄）。這裡在成功且引擎未給游標回訊時
   * 自查越界並重置到安全段落（末段起點），不讓游標留在不存在的位置。
   */
  _applyTableMutation(mutation) {
    if (!this.engine) throw new Error('尚未開啟文件，無法操作表格');
    const res = engineMutationResult(this.engine.apply_mutation_json(JSON.stringify(mutation)), 'apply_mutation_json（表格操作）');
    if (res && res.ok) {
      this.isDirty = true;
      if (res.outcome && res.outcome.caret_after) {
        this.caret = { ...res.outcome.caret_after };
      } else if (this.caret) {
        const pc = this.paragraphCount();
        if (this.caret.para >= pc) {
          this.caret = { para: Math.max(0, pc - 1), cp: 0 };
        }
      }
    }
    return res;
  }

  /**
   * 解析目標表格路徑：payload 帶 rootSlot 用之；否則用引擎真反查「游標落在
   * 哪張表」（table_locator_json）；表外正文段時，文件只有一張表才退到 0
   * （discovered=true 供 UI 於狀態列說明），多張表未指定一律誠實報錯——
   * **禁止靜默退回 count-1**（U4d L6：插入錨點是游標段落，新表不必然排在
   * 最後，盲設末張會操作到舊表）。
   */
  _resolveTablePath(rootSlot) {
    if (rootSlot !== undefined && rootSlot !== null && rootSlot !== '') {
      const n = Number(rootSlot);
      if (!Number.isInteger(n) || n < 0) {
        return { path: null, error: `root_slot 需為非負整數，收到 ${rootSlot}` };
      }
      return { path: buildTablePath(n), error: null, rootSlot: n };
    }
    const located = this.currentTableCell();
    if (located && Number.isInteger(located.rootSlot) && located.rootSlot >= 0) {
      return {
        path: buildTablePath(located.rootSlot, located.nestedSteps),
        error: null,
        rootSlot: located.rootSlot,
        discovered: true,
        viaCaret: true
      };
    }
    const count = this.tableCount();
    if (count === null) return { path: null, error: '無法讀取文件中的表格張數' };
    if (count === 0) return { path: null, error: '文件中沒有表格，請先插入表格' };
    if (count === 1) return { path: buildTablePath(0), error: null, rootSlot: 0, discovered: true };
    return {
      path: null,
      error: `文件中有 ${count} 張表格且游標不在任何表格內，無法判定操作目標（請把游標移進目標表格，或明確指定 root_slot）`
    };
  }

  /**
   * 插入一列 (M4.3 InsertTableRow)。
   * @param {string} position - 'Before' | 'After'（引擎 InsertSide 二態）
   * @param {Object} options - { rootSlot?, atRow?, widthPt? }
   */
  insertTableRow(position = 'After', options = {}) {
    const side = normalizeInsertSide(position, 'After');
    if (!side) return { ok: false, rejectedByShell: true, error: `插入方位需為 Before／After，收到 ${position}` };
    const resolved = this._resolveTablePath(options.rootSlot);
    if (resolved.error) return { ok: false, rejectedByShell: true, error: resolved.error };
    const shape = this.tableShape(resolved.rootSlot);
    let atRow = options.atRow;
    if (atRow === undefined || atRow === null || atRow === '') {
      if (!shape.ok) return { ok: false, rejectedByShell: true, error: `未帶 at_row 且無法讀取列數：${shape.error}` };
      atRow = side === 'Before' ? 0 : shape.rows - 1;
    }
    atRow = Number(atRow);
    if (!Number.isInteger(atRow) || atRow < 0) {
      return { ok: false, rejectedByShell: true, error: `at_row 需為非負整數，收到 ${options.atRow}` };
    }
    const res = this._applyTableMutation(buildInsertTableRowMutation(resolved.path, atRow, side));
    return { ...res, target: { rootSlot: resolved.rootSlot, atRow, position: side } };
  }

  /**
   * 刪除一列 (M4.3 DeleteTableRow)。
   * @param {Object} options - { rootSlot?, row? }；未帶 row 時刪末列（引擎真值）
   */
  deleteTableRow(options = {}) {
    const resolved = this._resolveTablePath(options.rootSlot);
    if (resolved.error) return { ok: false, rejectedByShell: true, error: resolved.error };
    const shape = this.tableShape(resolved.rootSlot);
    let row = options.row;
    if (row === undefined || row === null || row === '') {
      if (!shape.ok) return { ok: false, rejectedByShell: true, error: `未帶 row 且無法讀取列數：${shape.error}` };
      row = shape.rows - 1;
    }
    row = Number(row);
    if (!Number.isInteger(row) || row < 0) {
      return { ok: false, rejectedByShell: true, error: `row 需為非負整數，收到 ${options.row}` };
    }
    const res = this._applyTableMutation(buildDeleteTableRowMutation(resolved.path, row));
    return { ...res, target: { rootSlot: resolved.rootSlot, row } };
  }

  /**
   * 插入一欄 (M4.4 InsertTableColumn)。width_pt 是引擎必填欄，預設 100pt
   * （與 InsertTable 未帶 col_widths_pt 時同款預設）。
   */
  insertTableColumn(position = 'After', options = {}) {
    const side = normalizeInsertSide(position, 'After');
    if (!side) return { ok: false, rejectedByShell: true, error: `插入方位需為 Before／After，收到 ${position}` };
    const resolved = this._resolveTablePath(options.rootSlot);
    if (resolved.error) return { ok: false, rejectedByShell: true, error: resolved.error };
    const shape = this.tableShape(resolved.rootSlot);
    let atGridCol = options.atGridCol;
    if (atGridCol === undefined || atGridCol === null || atGridCol === '') {
      if (!shape.ok) return { ok: false, rejectedByShell: true, error: `未帶 at_grid_col 且無法讀取欄數：${shape.error}` };
      atGridCol = side === 'Before' ? 0 : shape.cols - 1;
    }
    atGridCol = Number(atGridCol);
    if (!Number.isInteger(atGridCol) || atGridCol < 0) {
      return { ok: false, rejectedByShell: true, error: `at_grid_col 需為非負整數，收到 ${options.atGridCol}` };
    }
    const widthPt = options.widthPt === undefined || options.widthPt === null ? 100 : Number(options.widthPt);
    if (!Number.isFinite(widthPt) || widthPt <= 0) {
      return { ok: false, rejectedByShell: true, error: `width_pt 需為正數，收到 ${options.widthPt}` };
    }
    const res = this._applyTableMutation(buildInsertTableColumnMutation(resolved.path, atGridCol, side, widthPt));
    return { ...res, target: { rootSlot: resolved.rootSlot, atGridCol, position: side, width_pt: widthPt } };
  }

  /**
   * 刪除一欄 (M4.4 DeleteTableColumn)。
   */
  deleteTableColumn(options = {}) {
    const resolved = this._resolveTablePath(options.rootSlot);
    if (resolved.error) return { ok: false, rejectedByShell: true, error: resolved.error };
    const shape = this.tableShape(resolved.rootSlot);
    let gridCol = options.gridCol;
    if (gridCol === undefined || gridCol === null || gridCol === '') {
      if (!shape.ok) return { ok: false, rejectedByShell: true, error: `未帶 grid_col 且無法讀取欄數：${shape.error}` };
      gridCol = shape.cols - 1;
    }
    gridCol = Number(gridCol);
    if (!Number.isInteger(gridCol) || gridCol < 0) {
      return { ok: false, rejectedByShell: true, error: `grid_col 需為非負整數，收到 ${options.gridCol}` };
    }
    const res = this._applyTableMutation(buildDeleteTableColumnMutation(resolved.path, gridCol));
    return { ...res, target: { rootSlot: resolved.rootSlot, gridCol } };
  }

  /**
   * 套用表格屬性 (M4.6 SetTableProps)。
   * 引擎只有 width_pt／alignment／shading_fill 三欄——本方法只放行這三欄，
   * 且底紋值非法時**先拒絕不上引擎**（引擎對非法 hex 回 ok=true 卻零效果，
   * 照送就是假成功）。
   * @param {Object} patch - { widthPt?, alignment?, shadingFill? }
   */
  setTableProps(patch = {}, options = {}) {
    const resolved = this._resolveTablePath(options.rootSlot);
    if (resolved.error) return { ok: false, rejectedByShell: true, error: resolved.error };

    const out = {};
    if (patch.widthPt !== undefined && patch.widthPt !== null && patch.widthPt !== '') {
      const w = Number(patch.widthPt);
      if (!Number.isFinite(w) || w <= 0) return { ok: false, rejectedByShell: true, error: `width_pt 需為正數，收到 ${patch.widthPt}` };
      out.width_pt = w;
    }
    if (patch.alignment !== undefined && patch.alignment !== null && patch.alignment !== '') {
      const al = String(patch.alignment).trim().toLowerCase();
      // 引擎寫 w:jc/@w:val（left／center／right，mutation.rs:168-170）
      if (!['left', 'center', 'right'].includes(al)) {
        return { ok: false, rejectedByShell: true, error: `表格對齊需為 left／center／right，收到 ${patch.alignment}` };
      }
      out.alignment = al;
    }
    if (patch.shadingFill !== undefined && patch.shadingFill !== null && patch.shadingFill !== '') {
      const fill = normalizeHex6Fill(patch.shadingFill);
      if (fill === null) {
        return { ok: false, rejectedByShell: true, error: `底紋色需為 #RRGGBB 或 auto，收到 ${patch.shadingFill}` };
      }
      out.shading_fill = fill;
    }
    if (out.width_pt === undefined && out.alignment === undefined && out.shading_fill === undefined) {
      return { ok: false, rejectedByShell: true, error: '未帶任何可套用的表格屬性（width_pt／alignment／shading_fill 皆缺）' };
    }
    const res = this._applyTableMutation(buildSetTablePropsMutation(resolved.path, out));
    return { ...res, applied: out, target: { rootSlot: resolved.rootSlot } };
  }

  /**
   * 套用儲存格屬性 (M4.6 SetCellProps)。同 SetTableProps：只放行引擎宣告過的
   * 五欄，非法值先拒絕。
   * @param {Object} patch - { widthPt?, shadingFill?, verticalAlign?, gridSpan?, vMerge? }
   * @param {Object} options - { rootSlot?, row?, cell? }
   */
  setCellProps(patch = {}, options = {}) {
    const resolved = this._resolveTablePath(options.rootSlot);
    if (resolved.error) return { ok: false, rejectedByShell: true, error: resolved.error };

    let row = options.row, cell = options.cell;
    const missingRow = row === undefined || row === null || row === '';
    const missingCell = cell === undefined || cell === null || cell === '';
    if (missingRow || missingCell) {
      // 座標缺席才需要真值補座標——延遲觸發，不讓帶座標的操作被結構讀取連坐
      // （U4d L1：舊版無條件讀 tableShape，探針失效時連帶把 setCellProps 一起打死）。
      // 順序：游標反查（用戶點哪格＝哪格，且對 uneven 表格比「末格」安全）→
      // summary 末格（rows-1／cols-1，引擎真值）。兩者皆不可得才誠實報錯。
      const located = this.currentTableCell();
      const inTarget = located && located.rootSlot === resolved.rootSlot;
      if (row === undefined || row === null || row === '') row = inTarget ? located.row : null;
      if (cell === undefined || cell === null || cell === '') cell = inTarget ? located.cell : null;
      if (!Number.isInteger(row) || !Number.isInteger(cell)) {
        const shape = this.tableShape(resolved.rootSlot);
        if (!shape.ok) return { ok: false, rejectedByShell: true, error: `未帶 row／cell 且無法讀取表格結構：${shape.error}` };
        if (!Number.isInteger(row)) row = shape.rows - 1;
        if (!Number.isInteger(cell)) cell = shape.cols - 1;
      }
    }
    row = Number(row); cell = Number(cell);
    if (!Number.isInteger(row) || row < 0 || !Number.isInteger(cell) || cell < 0) {
      return { ok: false, rejectedByShell: true, error: `row／cell 需為非負整數，收到 row=${options.row} cell=${options.cell}` };
    }

    const out = {};
    if (patch.widthPt !== undefined && patch.widthPt !== null && patch.widthPt !== '') {
      const w = Number(patch.widthPt);
      if (!Number.isFinite(w) || w <= 0) return { ok: false, rejectedByShell: true, error: `width_pt 需為正數，收到 ${patch.widthPt}` };
      out.width_pt = w;
    }
    if (patch.shadingFill !== undefined && patch.shadingFill !== null && patch.shadingFill !== '') {
      const fill = normalizeHex6Fill(patch.shadingFill);
      if (fill === null) return { ok: false, rejectedByShell: true, error: `底紋色需為 #RRGGBB 或 auto，收到 ${patch.shadingFill}` };
      out.shading_fill = fill;
    }
    if (patch.verticalAlign !== undefined && patch.verticalAlign !== null && patch.verticalAlign !== '') {
      const va = normalizeVerticalAlign(patch.verticalAlign);
      if (!va) return { ok: false, rejectedByShell: true, error: `垂直對齊需為 top／center／bottom，收到 ${patch.verticalAlign}` };
      out.vertical_align = va;
    }
    if (patch.gridSpan !== undefined && patch.gridSpan !== null && patch.gridSpan !== '') {
      const gs = Number(patch.gridSpan);
      if (!Number.isInteger(gs) || gs < 1) return { ok: false, rejectedByShell: true, error: `grid_span 需為 ≥1 的整數，收到 ${patch.gridSpan}` };
      out.grid_span = gs;
    }
    if (patch.vMerge !== undefined && patch.vMerge !== null && patch.vMerge !== '') {
      const vm = String(patch.vMerge).trim();
      if (!['None', 'Restart', 'Continue'].includes(vm)) {
        return { ok: false, rejectedByShell: true, error: `v_merge 需為 None／Restart／Continue，收到 ${patch.vMerge}` };
      }
      out.v_merge = vm;
    }
    if (Object.keys(out).length === 0) {
      return { ok: false, rejectedByShell: true, error: '未帶任何可套用的儲存格屬性' };
    }
    const res = this._applyTableMutation(buildSetCellPropsMutation(resolved.path, row, cell, out));
    return { ...res, applied: out, target: { rootSlot: resolved.rootSlot, row, cell } };
  }

  /**
   * 插入行內圖片 (M5.1 InsertInlineImage)。媒體位元組真的進引擎
   * （引擎寫 word/media/*＋[Content_Types].xml＋rels，實測 touched_parts
   * 四筆），外殼不畫佔位框。
   *
   * @param {Uint8Array} bytes - 圖片位元組
   * @param {string} mimeType - image/png｜image/jpeg｜image/gif｜image/webp｜image/svg+xml
   *        （引擎 validate_media 只認這五類，且會核檔頭與 MIME 是否相符）
   * @param {Object} options - { para?, cp?, widthPt?, heightPt?, altText? }
   */
  insertInlineImage(bytes, mimeType, options = {}) {
    if (!this.engine) throw new Error('尚未開啟文件，無法插入圖片');
    if (!bytes || bytes.length === 0) {
      return { ok: false, rejectedByShell: true, error: '圖片位元組為空' };
    }
    const mime = String(mimeType || '').trim().toLowerCase();
    const SUPPORTED = ['image/png', 'image/jpeg', 'image/jpg', 'image/gif', 'image/webp', 'image/svg+xml'];
    if (!SUPPORTED.includes(mime)) {
      return { ok: false, rejectedByShell: true, error: `引擎只支援 ${SUPPORTED.join('／')}，收到 ${mimeType || '未宣告 MIME'}` };
    }
    const para = options.para !== undefined && options.para !== null ? Number(options.para) : (this.caret ? this.caret.para : 0);
    const cp = options.cp !== undefined && options.cp !== null ? Number(options.cp) : (this.caret ? this.caret.cp : 0);
    if (!Number.isInteger(para) || para < 0 || !Number.isInteger(cp) || cp < 0) {
      return { ok: false, rejectedByShell: true, error: `插入點需為非負整數，收到 para=${para} cp=${cp}` };
    }

    // 尺寸：呼叫端指定就用；單邊指定必須按原始長寬比等比折算另一邊——實測
    // 舊行為只給 widthPt 時高度取 natural 換算值不按 wPt 折算，2:1 圖給 100pt
    // 寬會被撐成 100×75（U4d L4 紅燈實錄：非正方形圖拉伸）。兩邊都缺時依
    // 96dpi 換算並鉗在內容寬內（讀檔頭中繼資料，不是排版計算；排版仍由引擎決定）。
    const paper = safeEngineJson(this.engine.paper_json(), 'paper_json');
    const contentWidthPt = paper
      ? Math.max(72, paper.page_w_pt - (paper.margin_left_pt || 72) - (paper.margin_right_pt || 72))
      : 451;
    const natural = readImageNaturalSize(bytes);
    const naturalW = natural ? natural.w : 240;
    const naturalH = natural ? natural.h : 160;
    let wPt = options.widthPt !== undefined && options.widthPt !== null ? Number(options.widthPt) : null;
    let hPt = options.heightPt !== undefined && options.heightPt !== null ? Number(options.heightPt) : null;
    if (wPt !== null && hPt === null) {
      // 單邊指定：另一邊 = 指定邊 × natural 長寬比（等比，不拉伸）
      hPt = (wPt * naturalH) / naturalW;
    } else if (hPt !== null && wPt === null) {
      wPt = (hPt * naturalW) / naturalH;
    } else if (wPt === null && hPt === null) {
      let w = (naturalW * 72) / 96;
      let h = (naturalH * 72) / 96;
      const maxW = contentWidthPt * 0.9;
      if (w > maxW) { h = (h * maxW) / w; w = maxW; }
      wPt = w; hPt = h;
    }
    if (!Number.isFinite(wPt) || wPt <= 0 || !Number.isFinite(hPt) || hPt <= 0) {
      return { ok: false, rejectedByShell: true, error: `圖片尺寸需為正數，收到 ${options.widthPt}×${options.heightPt}` };
    }

    const mutation = buildInsertInlineImageMutation({ para, cp }, bytes, mime, wPt, hPt, options.altText ?? null);
    const res = engineMutationResult(this.engine.apply_mutation_json(JSON.stringify(mutation)), 'apply_mutation_json（InsertInlineImage）');
    if (res.ok) {
      this.isDirty = true;
      if (res.outcome && res.outcome.caret_after) this.caret = { ...res.outcome.caret_after };
    }
    return { ...res, placed: { para, cp, width_pt: wPt, height_pt: hPt, mime_type: mime } };
  }

  /**
   * 取得目前游標所在處的樣式狀態
   */
  getCursorFormatState() {
    if (!this.engine) return { bold: false, italic: false, size_pt: 12, color_argb: 0xFF000000 };
    const para = this.caret ? this.caret.para : 0;
    const rect = this.caretRect(para, this.caret ? this.caret.cp : 0);
    const pageIdx = rect ? rect.page : 0;
    const dl = this.getPageDisplayList(pageIdx);
    if (!dl || !dl.items) return { bold: false, italic: false, size_pt: 12, color_argb: 0xFF000000 };

    let matchedItem = null;
    let minDist = Infinity;
    const targetX = rect ? rect.x_pt : 72;
    const targetY = rect ? rect.y_pt : 72;

    for (const it of dl.items) {
      if (it.Text) {
        const dx = Math.abs(it.Text.x - targetX);
        const dy = Math.abs(it.Text.y - targetY);
        const dist = dx + dy * 2;
        if (dist < minDist) {
          minDist = dist;
          matchedItem = it.Text;
        }
      }
    }

    if (matchedItem) {
      return {
        bold: (matchedItem.flags & 1) === 1,
        italic: (matchedItem.flags & 2) === 2,
        size_pt: matchedItem.size_pt || 12,
        color_argb: matchedItem.color_argb || 0xFF000000
      };
    }
    return { bold: false, italic: false, size_pt: 12, color_argb: 0xFF000000 };
  }

  /**
   * 粗體切換 / 設定
   */
  formatBold(boldVal) {
    const cur = this.getCursorFormatState();
    const bold = boldVal !== undefined ? Boolean(boldVal) : !cur.bold;
    return this.formatTextRange(null, { bold });
  }

  /**
   * 斜體切換 / 設定
   */
  formatItalic(italicVal) {
    const cur = this.getCursorFormatState();
    const italic = italicVal !== undefined ? Boolean(italicVal) : !cur.italic;
    return this.formatTextRange(null, { italic });
  }

  /**
   * 字型色彩設定
   */
  formatColor(color) {
    let colorU32 = 0xFFFF0000;
    if (typeof color === 'number') {
      colorU32 = color >>> 0;
    } else if (typeof color === 'string') {
      const cleanHex = color.replace('#', '').trim();
      if (cleanHex.length === 6) {
        colorU32 = (0xFF000000 | parseInt(cleanHex, 16)) >>> 0;
      } else if (cleanHex.length === 8) {
        colorU32 = parseInt(cleanHex, 16) >>> 0;
      }
    }
    return this.formatTextRange(null, { color_rgb: colorU32 });
  }

  /**
   * 字級設定
   */
  setFontSize(sizePt) {
    const pt = parseFloat(sizePt) || 12.0;
    return this.formatTextRange(null, { size_pt: pt });
  }

  // ==========================================================================
  // K29e 字元格式六欄（U4b 接線）
  //
  // 三查依據（逐欄，全部由本 run 實讀 core/ 取得，行號以實讀為準）：
  //   查二：`core/engine-api/src/mutation.rs` `RunDirectPatch` 宣告 underline／
  //         strike／vert_align／highlight／font_family 五欄（K29e 補齊，欄名與
  //         本檔建構器逐字一致；`serde(default)` 允許缺欄）。
  //   查三：`patch_rpr()` 的 K29e 寫入臂逐欄真實存在——`remove_element("w:u")`
  //         ＋`<w:u w:val="{as_u_val()}"/>`、`w:strike`＋`w:dstrike`、
  //         `w:vertAlign`、`w:highlight`／`w:shd` 雙族、`w:rFonts` 四欄同族名。
  //
  // 誠實斷點（故 `format:underline-style` 不得掛 live，見報告 §2.3）：
  //   解析層 `core/ooxml/src/styles.rs` 把 `w:u` 折成 `Option<bool>`，形態不進
  //   `RunStyle`；發射層 `run_style()` 只能發 FLAG_UNDERLINE（4）一態。五形態
  //   寫進 XML 是真的、存檔讀回也是真的，但「用戶看見那層」無差異 ⇒ 鈕留
  //   pending（引擎面缺口，非外殼可修）。
  // ==========================================================================

  /**
   * 取得游標處的字元格式探針（K29e 擴充態）。
   *
   * 探針只讀 `page_display_list`（用戶看見那層），不讀任何自報旗標：
   * - underline／strike 讀 `Text.flags` 的 FLAG_UNDERLINE(4)／FLAG_STRIKE(8)
   *   （`core/render-trait/src/lib.rs` 定義，發射源 `run_style()`）；
   * - vert_align 無法從旗標讀出（發射層把上下標折成 size_pt×0.58＋基線位移，
   *   無獨立旗標），故以「該 Text 項字級 ÷ 該頁最大字級」的比例推定，
   *   容差比照引擎發射常數 0.58（docs/k29e-redlight.md §2 LO 真值）。推定不出
   *   來時回 null，呼叫端據此走「明確設定」而非憑空猜一個 toggle 方向。
   */
  getCursorCharFormatProbe() {
    const fallback = {
      underline: false, strike: false, vert_align: null,
      size_pt: 12, family: null, detected: false
    };
    if (!this.engine) return fallback;
    const para = this.caret ? this.caret.para : 0;
    const rect = this.caretRect(para, this.caret ? this.caret.cp : 0);
    const pageIdx = rect ? rect.page : 0;
    const dl = this.getPageDisplayList(pageIdx);
    if (!dl || !Array.isArray(dl.items)) return fallback;

    const targetX = rect ? rect.x_pt : 72;
    const targetY = rect ? rect.y_pt : 72;
    let matchedItem = null;
    let minDist = Infinity;
    let maxSeenSize = 0;
    for (const it of dl.items) {
      if (!it.Text) continue;
      const t = it.Text;
      if (typeof t.size_pt === 'number' && t.size_pt > maxSeenSize) maxSeenSize = t.size_pt;
      const dx = Math.abs(t.x - targetX);
      const dy = Math.abs(t.y - targetY);
      const dist = dx + dy * 2;
      if (dist < minDist) {
        minDist = dist;
        matchedItem = t;
      }
    }
    if (!matchedItem) return fallback;

    const flags = matchedItem.flags || 0;
    let vert = null;
    if (maxSeenSize > 0 && typeof matchedItem.size_pt === 'number') {
      const ratio = matchedItem.size_pt / maxSeenSize;
      // 引擎發射口徑：sup/sub 段 size = base×0.58（core/engine-api/src/lib.rs
      // K29e 上下標發射契約）。0.58±0.05 帶內＝該段是上下標態；再依基線位移
      // 方向分上／下標：上標 y 較同段正文上移 0.35×base，下標下移 base/11。
      // 此處只判「是上下標」，上下標之間的分別交由呼叫端逐鈕語意決定。
      if (Math.abs(ratio - 0.58) < 0.05) vert = 'Short';
    }
    return {
      underline: (flags & 4) === 4,
      strike: (flags & 8) === 8,
      vert_align: vert,
      size_pt: matchedItem.size_pt || 12,
      family: matchedItem.family || null,
      detected: true
    };
  }

  /**
   * 底線切換／設定（K29e；引擎 `RunDirectPatch::underline`）。
   * @param {string|boolean|null} kind - 'single'|'double'|'dotted'|'dashed'|
   *                                     'none'／false＝明確移除；未帶＝切換。
   */
  formatUnderline(kind) {
    const normalized = (kind === undefined || kind === null)
      ? null
      : normalizeUnderlineKind(kind);
    if (kind !== undefined && kind !== null && normalized === null) {
      return { ok: false, error: `不支援的底線形態：${String(kind)}（引擎枚舉僅 single/double/dotted/dashed/none）` };
    }
    const next = normalized !== null
      ? normalized
      : (this.getCursorCharFormatProbe().underline ? 'None' : 'Single');
    return this.formatTextRange(null, { underline: next });
  }

  /**
   * 刪除線切換／設定（K29e；引擎 `RunDirectPatch::strike`）。
   * @param {string|boolean|null} kind - 'single'|'double'|'none'／false＝明確
   *                                     移除；未帶＝切換。
   */
  formatStrikethrough(kind) {
    let normalized = null;
    if (kind !== undefined && kind !== null) {
      const lowered = String(kind).trim().toLowerCase();
      if (kind === false || lowered === 'none') normalized = 'None';
      else if (lowered === 'single') normalized = 'Single';
      else if (lowered === 'double') normalized = 'Double';
      else return { ok: false, error: `不支援的刪除線形態：${String(kind)}（引擎 StrikeKind 僅 Single/Double/None）` };
    }
    const next = normalized !== null
      ? normalized
      : (this.getCursorCharFormatProbe().strike ? 'None' : 'Single');
    return this.formatTextRange(null, { strike: next });
  }

  /**
   * 上標／下標設定（K29e；引擎 `RunDirectPatch::vert_align`）。
   * 同一鈕重複點＝退回基線（切換語意）。
   * @param {string} want - 'superscript'|'subscript'|'baseline'
   */
  formatVertAlign(want) {
    const normalized = normalizeVertAlign(want);
    if (normalized === null) {
      return { ok: false, error: `不支援的上下標值：${String(want)}` };
    }
    const probe = this.getCursorCharFormatProbe();
    const next = (probe.detected && probe.vert_align === 'Short') ? 'Baseline' : normalized;
    return this.formatTextRange(null, { vert_align: next });
  }

  /**
   * 文字醒目提示（K29e；引擎 `RunDirectPatch::highlight`）。
   * 命名色走 `<w:highlight w:val="..."/>`、非命名色走 `<w:shd w:fill=.../>`——
   * 兩條路徑在解析層同落 `RunStyle.highlight`，發射同為 Rect 底方塊。
   * @param {string|number} color - '#RRGGBB'／'#AARRGGBB'／數值；
   *                                'none'／false／alpha=0 ＝明確移除。
   */
  formatHighlight(color) {
    if (color === 'none' || color === false || color === 0) {
      return this.formatTextRange(null, { highlight: 0x00FFFFFF });
    }
    const argb = parseColorArgb(color);
    if (argb === undefined) {
      return { ok: false, error: `醒目色無法解析：${String(color)}` };
    }
    return this.formatTextRange(null, { highlight: argb });
  }

  /**
   * 字型族設定（K29e；引擎 `RunDirectPatch::font_family`）。
   * 引擎寫入口徑＝`w:rFonts` 的 ascii／hAnsi／cs／eastAsia 四欄同一族名
   * （「整包選族」語義，mutation.rs 該欄註解明文）。
   * @param {string} family - 族名；空字串＝明確移除直接層宣告。
   */
  setFontFamily(family) {
    if (typeof family !== 'string') {
      return { ok: false, error: `字型族名必須是字串，實得 ${typeof family}` };
    }
    return this.formatTextRange(null, { font_family: family.trim() });
  }

  /**
   * 清除所有字元直接格式（K29e 六欄＋粗斜體全走「明確移除」形態）。
   *
   * 誠實邊界：本方法清的是**直接格式層**（引擎寫顯式關閉宣告，級聯不得補回）；
   * 樣式鏈（docDefaults／字元樣式／段落樣式）若另有宣告，仍會如 LO/Word 那般
   * 顯形——引擎在 highlight 欄的取捨註解明文拒絕虛構第四態。故本鈕的title
   * 寫「清除直接格式」而非「清除所有格式」，不誇大效果。
   */
  clearDirectCharFormatting() {
    return this.formatTextRange(null, {
      bold: false,
      italic: false,
      underline: 'None',
      strike: 'None',
      vert_align: 'Baseline',
      highlight: 0x00FFFFFF,
      font_family: ''
    });
  }

  /**
   * 放大字型
   */
  growFont() {
    const SIZES = [8, 9, 10, 11, 12, 14, 16, 18, 20, 22, 24, 26, 28, 36, 48, 72];
    const cur = this.getCursorFormatState();
    const curSize = cur.size_pt || 12;
    const nextSize = SIZES.find(s => s > curSize) || (curSize + 2);
    return this.setFontSize(nextSize);
  }

  /**
   * 縮小字型
   */
  shrinkFont() {
    const SIZES = [8, 9, 10, 11, 12, 14, 16, 18, 20, 22, 24, 26, 28, 36, 48, 72];
    const cur = this.getCursorFormatState();
    const curSize = cur.size_pt || 12;
    const prevSize = [...SIZES].reverse().find(s => s < curSize) || Math.max(1, curSize - 2);
    return this.setFontSize(prevSize);
  }

  /**
   * 段落對齊 (left, center, right, both)
   */
  setAlignment(align) {
    return this.setParagraphStyle(null, { alignment: align });
  }

  /**
   * 段落樣式 (Normal, Heading1, Heading2, Heading3 等)
   */
  setStyle(styleId) {
    return this.setParagraphStyle(null, { style_id: styleId });
  }
}
