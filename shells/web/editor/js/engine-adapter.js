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
