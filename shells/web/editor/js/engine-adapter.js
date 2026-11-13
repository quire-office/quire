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
 * 建立 M2.1 FormatTextRange 變更物件
 * @param {Object} range - { start: { para, cp }, end: { para, cp }, target?: string }
 * @param {Object} patch - { bold?: boolean, italic?: boolean, color_rgb?: number, size_pt?: number, char_style?: string }
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
        char_style: patch.char_style !== undefined ? patch.char_style : null
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
 * `core/engine-api/src/mutation.rs:144-149`：target／root_slot／nested_steps）。
 *
 * nested_steps 每一步為 (row_idx, cell_idx, sub_table_idx)，指向該格內第幾個
 * 巢狀 `<w:tbl>`。外殼目前一律走根表（巢狀表定位需要引擎的表格幾何反查，
 * 而 `table_count()`／`tables_json()` 尚未綁進 wasm 殼——見 U4 報告殘留項）。
 *
 * @param {number} rootSlot - 根表序號（OoxmlDoc::tables 的序）
 * @param {Array<Array<number>>} [nestedSteps] - 巢狀步進，預設空陣列＝根表
 */
export function buildTablePath(rootSlot, nestedSteps = []) {
  return {
    target: 'Body',
    root_slot: Number(rootSlot),
    nested_steps: nestedSteps
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
 * patch 欄位**只有三欄**（`mutation.rs:162-174`）：width_pt／alignment／
 * shading_fill。引擎的 serde 對未知欄位是靜默忽略（實測 patch:{border_pt:6}
 * 回 ok=true 且顯示列表零變化），因此本建構器只放行這三欄、其餘一律丟棄，
 * 避免外殼送出「看起來成功、實際沒做事」的假接線。
 * **邊框／框線不是本變體的欄位**——引擎目前無 `w:tblBorders` 寫入面，
 * 該效果屬引擎缺口，見 U4 報告「判定引擎缺欄位的具體證據」。
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
    const jsonStr = this.engine.page_display_list_json(pageIndex);
    if (!jsonStr) return null;
    const dl = JSON.parse(jsonStr);
    return await renderDisplayListToCanvas(dl, canvas, displayWidthPx);
  }

  /**
   * 取得指定頁顯示列表物件
   */
  getPageDisplayList(pageIndex) {
    if (!this.engine) return null;
    const jsonStr = this.engine.page_display_list_json(pageIndex);
    return jsonStr ? JSON.parse(jsonStr) : null;
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
    const jsonStr = this.engine.caret_at_json(page, xPt, yPt);
    return jsonStr ? JSON.parse(jsonStr) : null;
  }

  /**
   * 游標幾何矩形反查 (para, cp -> CaretRect)
   */
  caretRect(para, cp) {
    if (!this.engine) return null;
    const jsonStr = this.engine.caret_rect_json(para, cp);
    return jsonStr ? JSON.parse(jsonStr) : null;
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
    const str = this.engine.reflow_step_json();
    return JSON.parse(str);
  }

  /**
   * 在指定或目前游標處打字 (M1.1 InsertText)
   */
  typeAt(para, cp, text) {
    if (!this.engine) throw new Error('尚未開啟文件，無法輸入文字');
    const targetPara = para !== undefined ? Number(para) : (this.caret ? this.caret.para : 0);
    const targetCp = cp !== undefined ? Number(cp) : (this.caret ? this.caret.cp : 0);
    const mutation = buildInsertTextMutation(targetPara, targetCp, text);
    const resStr = this.engine.apply_mutation_json(JSON.stringify(mutation));
    const res = JSON.parse(resStr);

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
    const resStr = this.engine.apply_mutation_json(JSON.stringify(mutation));
    const res = JSON.parse(resStr);

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
    const resStr = this.engine.apply_mutation_json(JSON.stringify(mutation));
    const res = JSON.parse(resStr);

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
    const resStr = this.engine.undo_json();
    const res = JSON.parse(resStr);

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
    const resStr = this.engine.redo_json();
    const res = JSON.parse(resStr);

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
    const resStr = this.engine.apply_mutation_json(JSON.stringify(mutation));
    const res = JSON.parse(resStr);

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
    const resStr = this.engine.apply_mutation_json(JSON.stringify(mutation));
    const res = JSON.parse(resStr);

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
    const resStr = this.engine.apply_mutation_json(JSON.stringify(mutation));
    const res = JSON.parse(resStr);

    if (res.ok) {
      this.isDirty = true;
      if (res.outcome && res.outcome.caret_after) {
        this.caret = { ...res.outcome.caret_after };
      }
    }
    return res;
  }

  // ---------------------------------------------------------------------
  // U4：表格家族（M4.3／M4.4／M4.6）與圖片插入（M5.1）真接線
  //
  // 紀律：幾何全部由引擎側 quire_layout::table::TableInput 決定，本層只產
  // 操作描述＋讀引擎回傳。下面所有「讀表格結構」的動作都靠引擎自己的誠實
  // 拒絕回訊息（例：列序越界時回「列序 99 越界（共 2 列）」）——實測證明
  // 該類探針零副作用、不入 undo 堆（探針八），因此不是 JS 端算欄寬／列高，
  // 而是把引擎給的真值讀出來。wasm 殼目前未綁 table_count()／tables_json()
  // （shells/web/src/lib.rs 無此導出），故暫以此途取結構，待補綁後替換。
  // ---------------------------------------------------------------------

  /**
   * 讀目前正文根表張數（探針：對越界 root_slot 發一筆必然被拒的變體，
   * 從引擎錯誤訊息讀「共 N 張」）。回 null＝讀不到（非預期格式，如實回 null，
   * 不猜數字）。
   */
  tableCount() {
    if (!this.engine) return null;
    const res = JSON.parse(this.engine.apply_mutation_json(JSON.stringify({
      DeleteTableColumn: { table: buildTablePath(1000000000), grid_col: 0 }
    })));
    const m = /共 (\d+) 張/.exec(res.error || '');
    return m ? Number(m[1]) : null;
  }

  /**
   * 讀指定根表的列數／網格欄數（同款探針，讀「共 N 列」／「共 N 欄」）。
   * @returns {{rows:number|null, cols:number|null, ok:boolean, error:string|null}}
   */
  tableShape(rootSlot = 0) {
    if (!this.engine) return { rows: null, cols: null, ok: false, error: '尚未開啟文件' };
    const probe = (mutation) => JSON.parse(this.engine.apply_mutation_json(JSON.stringify(mutation)));
    const rowRes = probe({ DeleteTableRow: { table: buildTablePath(rootSlot), row: 1000000000 } });
    const rowMatch = /共 (\d+) 列/.exec(rowRes.error || '');
    const colRes = probe({ DeleteTableColumn: { table: buildTablePath(rootSlot), grid_col: 1000000000 } });
    const colMatch = /共 (\d+) 欄/.exec(colRes.error || '');
    if (!rowMatch || !colMatch) {
      // 讀不到就老實說讀不到（列數可能因 vMerge 使引擎訊息措辭不同，屆時由呼叫端決定不動作）
      return { rows: null, cols: null, ok: false, error: rowRes.error || colRes.error || '無法讀取表格結構' };
    }
    return { rows: Number(rowMatch[1]), cols: Number(colMatch[1]), ok: true, error: null };
  }

  /**
   * 套用一筆表格類 Mutation：呼叫引擎、解析 outcome、回傳原始結果給上層。
   * 上層（qword-ui）依 ok 決定是否重排，失敗一律如實回報，不得寫成成功。
   */
  _applyTableMutation(mutation) {
    if (!this.engine) throw new Error('尚未開啟文件，無法操作表格');
    const res = JSON.parse(this.engine.apply_mutation_json(JSON.stringify(mutation)));
    if (res.ok) {
      this.isDirty = true;
      if (res.outcome && res.outcome.caret_after) {
        this.caret = { ...res.outcome.caret_after };
      }
    }
    return res;
  }

  /**
   * 解析目標表格路徑：payload 帶 rootSlot 用之；否則以游標段落後最近的根表
   * （無從反查時退到最後一張根表，並回傳 discovered=true 供 UI 在狀態列說明）。
   * 引擎未提供 caret→table 反查（wasm 殼未綁 tables_json），此處只用引擎給的
   * 張數真值，不自行猜座標。
   */
  _resolveTablePath(rootSlot) {
    if (rootSlot !== undefined && rootSlot !== null && rootSlot !== '') {
      const n = Number(rootSlot);
      if (!Number.isInteger(n) || n < 0) {
        return { path: null, error: `root_slot 需為非負整數，收到 ${rootSlot}` };
      }
      return { path: buildTablePath(n), error: null, rootSlot: n };
    }
    const count = this.tableCount();
    if (count === null) return { path: null, error: '無法讀取文件中的表格張數' };
    if (count === 0) return { path: null, error: '文件中沒有表格，請先插入表格' };
    return { path: buildTablePath(count - 1), error: null, rootSlot: count - 1, discovered: true };
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
    const shape = this.tableShape(resolved.rootSlot);

    let row = options.row, cell = options.cell;
    if ((row === undefined || row === null || row === '') || (cell === undefined || cell === null || cell === '')) {
      if (!shape.ok) return { ok: false, rejectedByShell: true, error: `未帶 row／cell 且無法讀取表格結構：${shape.error}` };
      if (row === undefined || row === null || row === '') row = shape.rows - 1;
      if (cell === undefined || cell === null || cell === '') cell = shape.cols - 1;
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

    // 尺寸：呼叫端指定就用；否則依圖片原始像素按 96dpi 換算 pt，並鉗在內容寬內
    // （讀檔頭中繼資料，不是排版計算；排版仍由引擎決定）。
    const paper = (() => {
      try { return JSON.parse(this.engine.paper_json()); } catch { return null; }
    })();
    const contentWidthPt = paper ? Math.max(72, paper.page_w_pt - (paper.margin_left_pt || 72) - (paper.margin_right_pt || 72)) : 451;
    const natural = readImageNaturalSize(bytes);
    let wPt = options.widthPt !== undefined && options.widthPt !== null ? Number(options.widthPt) : null;
    let hPt = options.heightPt !== undefined && options.heightPt !== null ? Number(options.heightPt) : null;
    if ((wPt === null || hPt === null)) {
      const baseW = natural ? natural.w : 240;
      const baseH = natural ? natural.h : 160;
      let w = (baseW * 72) / 96;
      let h = (baseH * 72) / 96;
      const maxW = contentWidthPt * 0.9;
      if (w > maxW) { h = (h * maxW) / w; w = maxW; }
      if (wPt === null) wPt = w;
      if (hPt === null) hPt = h;
    }
    if (!Number.isFinite(wPt) || wPt <= 0 || !Number.isFinite(hPt) || hPt <= 0) {
      return { ok: false, rejectedByShell: true, error: `圖片尺寸需為正數，收到 ${options.widthPt}×${options.heightPt}` };
    }

    const mutation = buildInsertInlineImageMutation({ para, cp }, bytes, mime, wPt, hPt, options.altText ?? null);
    const res = JSON.parse(this.engine.apply_mutation_json(JSON.stringify(mutation)));
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
