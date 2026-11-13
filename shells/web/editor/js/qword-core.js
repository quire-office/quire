/**
 * QWord Core Logic Layer (DOM-free)
 * 零 DOM 依賴的純邏輯層，可於 Node.js ESM 與瀏覽器中直接 import。
 * 負責：指令註冊表、需求依賴狀態（live / pending:<card>）、標籤與群組管理、
 * 縮放與檢視狀態機、標尺縮排計算、內部指令事件匯流排（onCommand）。
 */

export const TABS = [
  { id: 'tab-home', label: '常用', groupIds: ['clipboard', 'font', 'paragraph', 'styles', 'editing'] },
  { id: 'tab-insert', label: '插入', groupIds: ['pages', 'tables', 'illustrations', 'symbols', 'header_footer', 'text', 'structure'] },
  { id: 'tab-design', label: '設計', groupIds: ['themes', 'doc_formatting', 'page_background', 'page_layout_options'] },
  { id: 'tab-layout', label: '版面配置', groupIds: ['page_setup', 'arrange'] },
  { id: 'tab-references', label: '參考資料', groupIds: ['toc', 'footnotes', 'citations', 'captions', 'index', 'authorities'] },
  { id: 'tab-review', label: '校閱', groupIds: ['proofing', 'language', 'comments', 'tracking', 'protect'] },
  { id: 'tab-view', label: '檢視', groupIds: ['views', 'show', 'zoom', 'window', 'macros'] },
  { id: 'tab-help', label: '說明', groupIds: ['help_group', 'search_group'] }
];

export const GROUPS = {
  // 常用
  clipboard: { id: 'clipboard', tabId: 'tab-home', label: '剪貼簿' },
  font: { id: 'font', tabId: 'tab-home', label: '字型' },
  paragraph: { id: 'paragraph', tabId: 'tab-home', label: '段落' },
  styles: { id: 'styles', tabId: 'tab-home', label: '樣式' },
  editing: { id: 'editing', tabId: 'tab-home', label: '編輯' },

  // 插入
  pages: { id: 'pages', tabId: 'tab-insert', label: '頁面' },
  tables: { id: 'tables', tabId: 'tab-insert', label: '表格' },
  illustrations: { id: 'illustrations', tabId: 'tab-insert', label: '圖例' },
  symbols: { id: 'symbols', tabId: 'tab-insert', label: '符號' },
  header_footer: { id: 'header_footer', tabId: 'tab-insert', label: '頁首與頁尾' },
  text: { id: 'text', tabId: 'tab-insert', label: '文字' },
  structure: { id: 'structure', tabId: 'tab-insert', label: '結構' },

  // 設計
  themes: { id: 'themes', tabId: 'tab-design', label: '主題' },
  doc_formatting: { id: 'doc_formatting', tabId: 'tab-design', label: '文件格式設定' },
  page_background: { id: 'page_background', tabId: 'tab-design', label: '頁面背景' },
  page_layout_options: { id: 'page_layout_options', tabId: 'tab-design', label: '版面' },

  // 版面配置
  page_setup: { id: 'page_setup', tabId: 'tab-layout', label: '頁面設定' },
  arrange: { id: 'arrange', tabId: 'tab-layout', label: '排列' },

  // 參考資料
  toc: { id: 'toc', tabId: 'tab-references', label: '目錄' },
  footnotes: { id: 'footnotes', tabId: 'tab-references', label: '註腳' },
  citations: { id: 'citations', tabId: 'tab-references', label: '引文與書目' },
  captions: { id: 'captions', tabId: 'tab-references', label: '題注' },
  index: { id: 'index', tabId: 'tab-references', label: '索引' },
  authorities: { id: 'authorities', tabId: 'tab-references', label: '法律授權目錄' },

  // 校閱
  proofing: { id: 'proofing', tabId: 'tab-review', label: '校對' },
  language: { id: 'language', tabId: 'tab-review', label: '語言' },
  comments: { id: 'comments', tabId: 'tab-review', label: '註解' },
  tracking: { id: 'tracking', tabId: 'tab-review', label: '追蹤與修訂' },
  protect: { id: 'protect', tabId: 'tab-review', label: '溝通與保護' },

  // 檢視
  views: { id: 'views', tabId: 'tab-view', label: '檢視' },
  show: { id: 'show', tabId: 'tab-view', label: '顯示' },
  zoom: { id: 'zoom', tabId: 'tab-view', label: '顯示比例' },
  window: { id: 'window', tabId: 'tab-view', label: '視窗' },
  macros: { id: 'macros', tabId: 'tab-view', label: '巨集' },

  // 說明
  help_group: { id: 'help_group', tabId: 'tab-help', label: '說明' },
  search_group: { id: 'search_group', tabId: 'tab-help', label: '操作搜尋' }
};

/**
 * 依賴卡片說明對照表
 */
export const DEPENDENCY_MAP = {
  'live': { name: '既有能力', desc: '外殼或當前引擎直接支援' },
  'pending:shell': { name: '外殼待接線', desc: '引擎已有對應能力（Mutation 欄位真實存在），瀏覽器外殼接線與對話框進行中；剪貼簿、尋找取代屬外殼自身功能' },
  'pending:K29b': { name: 'K29b 切片（開發中）', desc: '引擎端切片進行中；逐鈕真實缺口以該鈕 title 為準' },
  'pending:K29c': { name: 'K29c 表格與圖片切片', desc: '已隨 v0.1.0-beta.12 入 main（M4 表格＋M5 圖片）；本標籤僅供歷史追溯，鈕面已按逐鈕查證重掛' },
  'pending:K29d': { name: 'K29d 節與註腳切片', desc: '已隨 v0.1.0-beta.12 入 main（M6 節／頁首頁尾／註腳）；本標籤僅供歷史追溯，鈕面已按逐鈕查證重掛' },
  'pending:K29e': { name: 'K29e 引擎切片（開發中）', desc: '引擎端切片進行中（樣式面／校閱面／參考資料面）；逐鈕真實缺口以該鈕 title 為準' },
  'pending:K29s': { name: 'K29s 視窗與多重視圖', desc: '多視窗檢視、視窗重排與分割' },
  'pending:K29m': { name: 'K29m 巨集與腳本擴充', desc: '巨集錄製與腳本自動化' },
  'pending:K29f': { name: 'K29f 檔案與協同傳輸', desc: '本機/雲端儲存、共用與協同編輯' },
  'pending:K29u': { name: 'K29u 支援與說明連線', desc: '線上客戶支援與互動說明' }
};

/**
 * U4：外殼已真接引擎 Mutation 的指令表（第一批：M4 表格家族＋M5.1 圖片插入）。
 *
 * 每筆 spec 說明了「core 分支驗什麼、UI 層拿什麼去呼 adapter」。欄位鍵名對
 * `core/engine-api/src/mutation.rs` 的宣告，不是外殼自創：
 * - InsertTableRow  table／at_row／position        （mutation.rs:302-306）
 * - DeleteTableRow  table／row                     （mutation.rs:308）
 * - InsertTableColumn table／at_grid_col／position／width_pt（mutation.rs:310-315）
 * - DeleteTableColumn table／grid_col              （mutation.rs:317）
 * - SetTableProps   patch{width_pt／alignment／shading_fill}（mutation.rs:162-174）
 * - SetCellProps    patch{width_pt／shading_fill／vertical_align／grid_span／v_merge}
 *                                                  （mutation.rs:201-219）
 *
 * 「邊框」不在表內：TablePropsPatch／CellPropsPatch 查無任何 border 欄位，
 * 引擎目前無 `w:tblBorders` 寫入面（證據見 docs/word-ui-u4-wiring-report.md），
 * 接上去只會產出「引擎回 ok=true 但顯示列表零變化」的假成功。
 */
export const U4_TABLE_ROW_COMMANDS = {
  'table:row-insert-above': { mutation: 'InsertTableRow', indexKeys: ['row', 'at_row', 'atRow'], positionKey: 'position', fixedPosition: 'Before' },
  'table:row-insert-below': { mutation: 'InsertTableRow', indexKeys: ['row', 'at_row', 'atRow'], positionKey: 'position', fixedPosition: 'After' },
  'table:row-insert': { mutation: 'InsertTableRow', indexKeys: ['row', 'at_row', 'atRow'], positionKey: 'position' },
  'table:row-delete': { mutation: 'DeleteTableRow', indexKeys: ['row', 'rowIdx'] }
};

export const U4_TABLE_COL_COMMANDS = {
  'table:col-insert-left': { mutation: 'InsertTableColumn', indexKeys: ['gridCol', 'grid_col', 'col', 'atGridCol'], positionKey: 'position', fixedPosition: 'Before', widthKey: 'widthPt' },
  'table:col-insert-right': { mutation: 'InsertTableColumn', indexKeys: ['gridCol', 'grid_col', 'col', 'atGridCol'], positionKey: 'position', fixedPosition: 'After', widthKey: 'widthPt' },
  'table:col-insert': { mutation: 'InsertTableColumn', indexKeys: ['gridCol', 'grid_col', 'col', 'atGridCol'], positionKey: 'position', widthKey: 'widthPt' },
  'table:col-delete': { mutation: 'DeleteTableColumn', indexKeys: ['gridCol', 'grid_col', 'col'] }
};

export const U4_TABLE_PROPS_COMMANDS = {
  // 底紋（shading_fill）＋對齊（alignment）＋寬度（width_pt）：三個引擎真實存在
  // 且可在 page_display_list 驗證的效果。
  'table:shading': { mutation: 'SetTableProps', fillKey: 'fill' },
  'table:align-left': { mutation: 'SetTableProps', alignKey: 'alignment', patch: { alignment: 'left' } },
  'table:align-center': { mutation: 'SetTableProps', alignKey: 'alignment', patch: { alignment: 'center' } },
  'table:align-right': { mutation: 'SetTableProps', alignKey: 'alignment', patch: { alignment: 'right' } },
  'table:width': { mutation: 'SetTableProps', widthKey: 'widthPt' }
};

export const U4_CELL_PROPS_COMMANDS = {
  // 儲存格層可驗證效果：底紋 → 顯示列表多一筆 Rect；垂直對齊 → 格內文字 y 位移。
  'cell:shading': { mutation: 'SetCellProps', fillKey: 'fill', cellKeys: ['row', 'cell'] },
  'cell:v-align-top': { mutation: 'SetCellProps', valignKey: 'verticalAlign', patch: { verticalAlign: 'top' }, cellKeys: ['row', 'cell'] },
  'cell:v-align-center': { mutation: 'SetCellProps', valignKey: 'verticalAlign', patch: { verticalAlign: 'center' }, cellKeys: ['row', 'cell'] },
  'cell:v-align-bottom': { mutation: 'SetCellProps', valignKey: 'verticalAlign', patch: { verticalAlign: 'bottom' }, cellKeys: ['row', 'cell'] }
};

/** 引擎 validate_media 認可的圖片 MIME（mutation_k29c.rs:643-649） */
export const ENGINE_SUPPORTED_IMAGE_MIMES = [
  'image/png', 'image/jpeg', 'image/jpg', 'image/gif', 'image/webp', 'image/svg+xml'
];

/** InsertTableColumn.width_pt 是引擎必填欄；未帶時用引擎同款預設 100pt */
const DEFAULT_COL_WIDTH_PT = 100;

/**
 * 參數非法時的標籤降級：呼叫端若自稱 live，一律降到真實缺口 pending:shell
 * （引擎不會被呼叫＝能力未被使用，稱 live 就是假 live）。
 */
function demoteRequirement(requirement) {
  return String(requirement || '') === 'live' ? 'pending:shell' : requirement;
}

/** 讀 root_slot（選填）：缺欄回 value=null（由 UI 層向引擎問真值），非法回 error */
function pickRootSlot(payload) {
  const v = payload?.rootSlot ?? payload?.root_slot;
  if (v === undefined || v === null || v === '') return { value: null, error: null };
  const n = Number(v);
  if (!Number.isInteger(n) || n < 0) return { value: null, error: `root_slot 需為非負整數，收到 ${v}` };
  return { value: n, error: null };
}

/** 讀索引欄（依候選鍵名取第一個存在的）：缺欄回 value=null，非法回 error */
function pickIndex(payload, keys) {
  for (const k of keys) {
    const v = payload?.[k];
    if (v === undefined || v === null || v === '') continue;
    const n = Number(v);
    if (!Number.isInteger(n) || n < 0) return { value: null, error: `${k} 需為非負整數，收到 ${v}` };
    return { value: n, error: null };
  }
  return { value: null, error: null };
}

/** InsertSide 二態正規化（引擎只有 Before／After，mutation.rs:132-137） */
function normalizeSide(side) {
  const s = String(side ?? '').trim().toLowerCase();
  if (!s) return null;
  if (s === 'before' || s === 'left' || s === 'above') return 'Before';
  if (s === 'after' || s === 'right' || s === 'below') return 'After';
  return null;
}

/** 底紋值正規化：回 'RRGGBB'／'auto'；非法回 null；未帶回 undefined */
function normalizeFill(value) {
  if (value === undefined || value === null || value === '') return undefined;
  const s = String(value).trim().replace(/^#/, '');
  if (s.toLowerCase() === 'auto') return 'auto';
  if (/^[0-9a-fA-F]{6}$/.test(s)) return s.toUpperCase();
  return null;
}

/** 表格對齊正規化（引擎寫 w:jc/@w:val：left／center／right） */
function normalizeTableAlign(value) {
  if (value === undefined || value === null || value === '') return undefined;
  const s = String(value).trim().toLowerCase();
  return ['left', 'center', 'right'].includes(s) ? s : null;
}

/** 儲存格垂直對齊正規化（引擎寫 w:vAlign/@w:val：top／center／bottom） */
function normalizeVerticalAlign(value) {
  if (value === undefined || value === null || value === '') return undefined;
  const s = String(value).trim().toLowerCase();
  return ['top', 'center', 'bottom'].includes(s) ? s : null;
}

export class QWordCore {
  constructor(options = {}) {
    this.activeTabId = options.initialTab || 'tab-home';
    this.zoom = options.initialZoom || 100; // 60 ~ 180
    this.viewMode = options.initialViewMode || 'print'; // 'print' | 'read' | 'web'
    this.rulerVisible = options.rulerVisible !== undefined ? options.rulerVisible : true;
    this.gridlinesVisible = false;
    this.navPaneVisible = false;

    // 標尺縮排狀態 (公分 cm)
    this.indents = {
      firstLine: 0,
      hanging: 0,
      left: 0,
      right: 15.5
    };

    // 真引擎文件狀態
    this.currentPage = 1;
    this.pageCount = 1;
    this.wordCount = 0;
    this.isSettled = true;
    this.fileName = '文件 1 - Quire 編輯器原型外殼';

    // 命令歷史紀錄
    this.commandLog = [];
    this.listeners = new Set();
  }

  /**
   * 註冊狀態監聽器
   */
  subscribe(fn) {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  _notify(type, data) {
    for (const listener of this.listeners) {
      try {
        listener(type, data, this.getState());
      } catch (err) {
        // 確保單一 listener 失敗不影響其他處理器
      }
    }
  }

  getState() {
    return {
      activeTabId: this.activeTabId,
      zoom: this.zoom,
      viewMode: this.viewMode,
      rulerVisible: this.rulerVisible,
      gridlinesVisible: this.gridlinesVisible,
      navPaneVisible: this.navPaneVisible,
      indents: { ...this.indents },
      logCount: this.commandLog.length,
      currentPage: this.currentPage,
      pageCount: this.pageCount,
      wordCount: this.wordCount,
      isSettled: this.isSettled,
      fileName: this.fileName
    };
  }

  /**
   * 更新文件統計資訊與定案狀態
   */
  setDocStats({ pageCount, currentPage, wordCount, isSettled, fileName } = {}) {
    if (pageCount !== undefined) this.pageCount = Math.max(1, pageCount);
    if (currentPage !== undefined) this.currentPage = Math.max(1, Math.min(this.pageCount, currentPage));
    if (wordCount !== undefined) this.wordCount = Math.max(0, wordCount);
    if (isSettled !== undefined) this.isSettled = Boolean(isSettled);
    if (fileName !== undefined) this.fileName = String(fileName);
    this._notify('doc:stats', this.getDocStats());
  }

  getDocStats() {
    return {
      currentPage: this.currentPage,
      pageCount: this.pageCount,
      wordCount: this.wordCount,
      isSettled: this.isSettled,
      fileName: this.fileName
    };
  }

  nextPage() {
    if (this.currentPage < this.pageCount) {
      this.currentPage++;
      this._notify('page:change', { currentPage: this.currentPage, pageCount: this.pageCount });
      return true;
    }
    return false;
  }

  prevPage() {
    if (this.currentPage > 1) {
      this.currentPage--;
      this._notify('page:change', { currentPage: this.currentPage, pageCount: this.pageCount });
      return true;
    }
    return false;
  }

  gotoPage(p) {
    const clamped = Math.max(1, Math.min(this.pageCount, p));
    this.currentPage = clamped;
    this._notify('page:change', { currentPage: this.currentPage, pageCount: this.pageCount });
    return clamped;
  }

  /**
   * 切換 Ribbon 標籤
   */
  setActiveTab(tabId) {
    const exists = TABS.some(t => t.id === tabId);
    if (!exists) return false;
    this.activeTabId = tabId;
    this._notify('tab:change', { tabId });
    return true;
  }

  /**
   * 設定縮放比例（60 ~ 180）
   */
  setZoom(percentage) {
    const clamped = Math.max(60, Math.min(180, Math.round(percentage)));
    this.zoom = clamped;
    this._notify('zoom:change', { zoom: clamped });
    return clamped;
  }

  zoomIn(step = 10) {
    return this.setZoom(this.zoom + step);
  }

  zoomOut(step = 10) {
    return this.setZoom(this.zoom - step);
  }

  resetZoom() {
    return this.setZoom(100);
  }

  fitPageWidth() {
    // 預設頁寬適配至 125%
    return this.setZoom(125);
  }

  /**
   * 設定檢視模式
   */
  setViewMode(mode) {
    if (['print', 'read', 'web'].includes(mode)) {
      this.viewMode = mode;
      this._notify('view:mode', { mode });
      return true;
    }
    return false;
  }

  /**
   * 切換標尺顯示
   */
  toggleRuler(visible) {
    this.rulerVisible = visible !== undefined ? Boolean(visible) : !this.rulerVisible;
    this._notify('ruler:toggle', { visible: this.rulerVisible });
    return this.rulerVisible;
  }

  /**
   * 切換網格線
   */
  toggleGridlines(visible) {
    this.gridlinesVisible = visible !== undefined ? Boolean(visible) : !this.gridlinesVisible;
    this._notify('gridlines:toggle', { visible: this.gridlinesVisible });
    return this.gridlinesVisible;
  }

  /**
   * 切換導航窗格
   */
  toggleNavPane(visible) {
    this.navPaneVisible = visible !== undefined ? Boolean(visible) : !this.navPaneVisible;
    this._notify('navpane:toggle', { visible: this.navPaneVisible });
    return this.navPaneVisible;
  }

  /**
   * 設定標尺縮排值
   */
  setIndent(marker, cm) {
    if (this.indents[marker] !== undefined) {
      this.indents[marker] = Math.max(0, Math.min(21, Number(cm.toFixed(1))));
      this._notify('ruler:indent', { marker, value: this.indents[marker] });
      return this.indents[marker];
    }
    return null;
  }

  /**
   * 核心指令匯流排：onCommand
   * 每一次點擊按鈕或觸發指令皆先抵達此處
   *
   * U4 誠實紀律（沿用本卡上游 t_57b7b089 為 table:grid-select 定的契約，
   * 逐條寫明，避免兩套口徑）：
   * - `requirement`＝能力面真相：只在「adapter 有對應 build*Mutation、
   *   core 分支真的匯流、UI 分支真的呼叫 adapter」三者齊備時給 `live`；
   *   呼叫端帶入的 live 若參數非法（引擎根本不會被呼叫），一律降回真實缺口
   *   標籤，不得沿用。
   * - `handled`＝該指令在此層是否被受理（參數合規且會送引擎）。參數非法＝
   *   false，且必須降標籤。
   * - 引擎真的拒絕時（回 ok=false），由 UI 層呼叫 recordCommandFailure()
   *   補一筆 handled=false 的失敗日誌，考卷才讀得到真實失敗，不會看到
   *   「全部成功」的假象。
   */
  onCommand(cmdId, payload = {}, requirement = null) {
    const timestamp = Date.now();
    let handled = false;
    let req = requirement;

    // 處理 live 內建邏輯
    if (cmdId.startsWith('tab:switch:')) {
      const targetTab = cmdId.replace('tab:switch:', '');
      handled = this.setActiveTab(targetTab);
      req = 'live';
    } else if (cmdId === 'zoom:in') {
      this.zoomIn();
      handled = true;
      req = 'live';
    } else if (cmdId === 'zoom:out') {
      this.zoomOut();
      handled = true;
      req = 'live';
    } else if (cmdId === 'zoom:reset' || cmdId === 'zoom:100') {
      this.resetZoom();
      handled = true;
      req = 'live';
    } else if (cmdId === 'zoom:fit-width') {
      this.fitPageWidth();
      handled = true;
      req = 'live';
    } else if (cmdId === 'zoom:one-page') {
      this.setZoom(100);
      handled = true;
      req = 'live';
    } else if (cmdId === 'zoom:multiple-pages') {
      this.setZoom(75);
      handled = true;
      req = 'live';
    } else if (cmdId === 'zoom:dialog') {
      handled = true;
      req = 'live';
    } else if (cmdId.startsWith('zoom:set:')) {
      const val = parseInt(cmdId.replace('zoom:set:', ''), 10);
      if (!isNaN(val)) {
        this.setZoom(val);
        handled = true;
        req = 'live';
      }
    } else if (cmdId.startsWith('view:mode:')) {
      const mode = cmdId.replace('view:mode:', '');
      handled = this.setViewMode(mode);
      req = 'live';
    } else if (cmdId === 'view:outline' || cmdId === 'view:focus' || cmdId === 'view:draft') {
      handled = true;
      req = 'live';
    } else if (cmdId === 'file:save') {
      handled = true;
      req = 'live';
      this._notify('file:save', payload);
    } else if (cmdId === 'file:open') {
      handled = true;
      req = 'live';
      this._notify('file:open', payload);
    } else if (cmdId === 'file:open-sample') {
      handled = true;
      req = 'live';
      this._notify('file:open-sample', payload);
    } else if (cmdId === 'edit:undo') {
      handled = true;
      req = 'live';
      this._notify('edit:undo', payload);
    } else if (cmdId === 'edit:redo') {
      handled = true;
      req = 'live';
      this._notify('edit:redo', payload);
    } else if (cmdId === 'edit:type' || cmdId === 'type') {
      handled = true;
      req = 'live';
      this._notify('edit:type', payload);
    } else if (cmdId === 'edit:backspace' || cmdId === 'backspace') {
      handled = true;
      req = 'live';
      this._notify('edit:backspace', payload);
    } else if (cmdId === 'edit:enter' || cmdId === 'enter') {
      handled = true;
      req = 'live';
      this._notify('edit:enter', payload);
    } else if (cmdId === 'format:bold') {
      handled = true;
      req = 'live';
      this._notify('format:bold', payload);
    } else if (cmdId === 'format:italic') {
      handled = true;
      req = 'live';
      this._notify('format:italic', payload);
    } else if (cmdId === 'format:font-color' || cmdId === 'format:font-color-palette' || cmdId === 'format:color:font') {
      handled = true;
      req = 'live';
      this._notify('format:font-color', payload);
    } else if (cmdId === 'font:grow') {
      handled = true;
      req = 'live';
      this._notify('font:grow', payload);
    } else if (cmdId === 'font:shrink') {
      handled = true;
      req = 'live';
      this._notify('font:shrink', payload);
    } else if (cmdId === 'font:size' || cmdId.startsWith('font:size:')) {
      handled = true;
      req = 'live';
      const sizeVal = payload.size || (cmdId.startsWith('font:size:') ? cmdId.replace('font:size:', '') : null);
      this._notify('font:size', { size: sizeVal, ...payload });
    } else if (cmdId === 'paragraph:align-left' || cmdId === 'align:left') {
      handled = true;
      req = 'live';
      this._notify('paragraph:align', { alignment: 'left', ...payload });
    } else if (cmdId === 'paragraph:align-center' || cmdId === 'align:center') {
      handled = true;
      req = 'live';
      this._notify('paragraph:align', { alignment: 'center', ...payload });
    } else if (cmdId === 'paragraph:align-right' || cmdId === 'align:right') {
      handled = true;
      req = 'live';
      this._notify('paragraph:align', { alignment: 'right', ...payload });
    } else if (cmdId === 'paragraph:align-justify' || cmdId === 'align:justify') {
      handled = true;
      req = 'live';
      this._notify('paragraph:align', { alignment: 'both', ...payload });
    } else if (cmdId === 'style:normal') {
      handled = true;
      req = 'live';
      this._notify('style:apply', { styleId: 'Normal', ...payload });
    } else if (cmdId === 'style:no-spacing') {
      handled = true;
      req = 'live';
      this._notify('style:apply', { styleId: 'NoSpacing', ...payload });
    } else if (cmdId === 'style:h1') {
      handled = true;
      req = 'live';
      this._notify('style:apply', { styleId: 'Heading1', ...payload });
    } else if (cmdId === 'style:h2') {
      handled = true;
      req = 'live';
      this._notify('style:apply', { styleId: 'Heading2', ...payload });
    } else if (cmdId === 'style:h3') {
      handled = true;
      req = 'live';
      this._notify('style:apply', { styleId: 'Heading3', ...payload });
    } else if (cmdId === 'page:next') {
      handled = true;
      req = 'live';
      this.nextPage();
    } else if (cmdId === 'page:prev') {
      handled = true;
      req = 'live';
      this.prevPage();
    } else if (cmdId.startsWith('page:goto:')) {
      const p = parseInt(cmdId.replace('page:goto:', ''), 10);
      if (!isNaN(p)) {
        this.gotoPage(p);
        handled = true;
        req = 'live';
      }
    } else if (cmdId === 'status:page') {
      handled = true;
      req = 'live';
      this._notify('status:page', payload);
    } else if (cmdId === 'status:word-count') {
      handled = true;
      req = 'live';
      this._notify('status:word-count', payload);
    } else if (cmdId === 'status:collab') {
      handled = true;
      req = 'live';
    } else if (cmdId === 'ruler:toggle') {
      handled = true;
      this.toggleRuler();
      req = 'live';
    } else if (cmdId === 'gridlines:toggle') {
      handled = true;
      this.toggleGridlines();
      req = 'live';
    } else if (cmdId === 'navpane:toggle') {
      handled = true;
      this.toggleNavPane();
      req = 'live';
    } else if (cmdId === 'help:open') {
      handled = true;
      req = 'live';
    } else if (cmdId === 'table:grid-select') {
      // M4.1 InsertTable 真接線（上游 t_57b7b089）：本層只做指令記帳與事件匯流，
      // 實際 Mutation 由 UI 層的 handleInsertTable 呼叫 adapter.insertTable 落地
      // （與 format:bold 等同一分層契約）。誠實紀律：尺寸非法時不得沿用呼叫端
      // 帶入的 live 宣稱——引擎不會被呼叫，故強制降回真實缺口標籤。
      const rows = Number(payload?.rows);
      const cols = Number(payload?.cols);
      const valid = Number.isInteger(rows) && Number.isInteger(cols) && rows > 0 && cols > 0;
      handled = valid;
      if (valid) {
        req = 'live';
        this._notify('table:grid-insert', { rows, cols });
      } else {
        req = String(requirement || '') === 'live' ? 'pending:shell' : requirement;
      }
    } else if (U4_TABLE_ROW_COMMANDS[cmdId] || U4_TABLE_COL_COMMANDS[cmdId]) {
      // M4.3／M4.4 行列變體：core 驗證參數＋匯流事件，UI 層呼 adapter 落地。
      const spec = U4_TABLE_ROW_COMMANDS[cmdId] || U4_TABLE_COL_COMMANDS[cmdId];
      const slot = pickRootSlot(payload);
      const idx = pickIndex(payload, spec.indexKeys);
      // 插入類才有 position（InsertSide 二態）；刪除類引擎不收這個欄位，
      // 因此只在 spec.positionKey 存在時才要求它——否則刪除鈕會被誤擋。
      const needsSide = !!spec.positionKey;
      const side = normalizeSide(payload?.[spec.positionKey] ?? spec.fixedPosition);
      const widthPt = spec.widthKey ? Number(payload?.[spec.widthKey] ?? DEFAULT_COL_WIDTH_PT) : null;
      const problems = [];
      if (slot.error) problems.push(slot.error);
      if (idx.error) problems.push(idx.error);
      if (needsSide && !side) problems.push(`position 需為 Before／After，收到 ${payload?.[spec.positionKey] ?? spec.fixedPosition}`);
      if (spec.widthKey && !(Number.isFinite(widthPt) && widthPt > 0)) problems.push(`width_pt 需為正數，收到 ${payload?.[spec.widthKey]}`);
      if (problems.length === 0) {
        handled = true;
        req = 'live';
        this._notify('table:row-col-mutate', {
          cmdId,
          mutation: spec.mutation,
          rootSlot: slot.value,
          index: idx.value,
          position: side,
          widthPt
        });
      } else {
        req = demoteRequirement(requirement);
        this._notify('table:row-col-rejected', { cmdId, problems });
      }
    } else if (U4_TABLE_PROPS_COMMANDS[cmdId]) {
      // M4.6（表格面）：底紋＋對齊＋寬度三個引擎真實存在且可在顯示列表驗證的
      // 效果。邊框不在表內——TablePropsPatch／CellPropsPatch 查無 border 欄位，
      // 接上去只會得到「引擎回 ok=true 但顯示列表零變化」的假成功（U4 報告存證）。
      const spec = U4_TABLE_PROPS_COMMANDS[cmdId];
      const slot = pickRootSlot(payload);
      const problems = [];
      if (slot.error) problems.push(slot.error);
      const patch = {};
      if (spec.patch) Object.assign(patch, spec.patch);
      if (spec.fillKey) {
        const fill = normalizeFill(payload?.[spec.fillKey] ?? patch.shadingFill);
        if (fill === null) problems.push(`底紋色需為 #RRGGBB 或 auto，收到 ${payload?.[spec.fillKey]}`);
        else if (fill !== undefined) patch.shadingFill = fill;
      }
      if (spec.alignKey) {
        const al = normalizeTableAlign(payload?.[spec.alignKey] ?? patch.alignment);
        if (al === null) problems.push(`表格對齊需為 left／center／right，收到 ${payload?.[spec.alignKey]}`);
        else if (al !== undefined) patch.alignment = al;
      }
      if (spec.widthKey) {
        const w = payload?.[spec.widthKey];
        if (w !== undefined && w !== null && w !== '') {
          const n = Number(w);
          if (!Number.isFinite(n) || n <= 0) problems.push(`width_pt 需為正數，收到 ${w}`);
          else patch.widthPt = n;
        }
      }
      if (Object.keys(patch).length === 0) problems.push('未帶任何可套用的屬性');
      if (problems.length === 0) {
        handled = true;
        req = 'live';
        this._notify('table:props-apply', {
          cmdId,
          mutation: spec.mutation,
          rootSlot: slot.value,
          patch
        });
      } else {
        req = demoteRequirement(requirement);
        this._notify('table:props-rejected', { cmdId, problems });
      }
    } else if (U4_CELL_PROPS_COMMANDS[cmdId]) {
      // M4.6（儲存格面）：底紋 → 顯示列表多一筆 Rect；垂直對齊 → 格內文字 y 位移。
      const spec = U4_CELL_PROPS_COMMANDS[cmdId];
      const slot = pickRootSlot(payload);
      const problems = [];
      if (slot.error) problems.push(slot.error);
      const patch = {};
      if (spec.patch) Object.assign(patch, spec.patch);
      if (spec.fillKey) {
        const fill = normalizeFill(payload?.[spec.fillKey] ?? patch.shadingFill);
        if (fill === null) problems.push(`底紋色需為 #RRGGBB 或 auto，收到 ${payload?.[spec.fillKey]}`);
        else if (fill !== undefined) patch.shadingFill = fill;
      }
      if (spec.valignKey) {
        const va = normalizeVerticalAlign(payload?.[spec.valignKey] ?? patch.verticalAlign);
        if (va === null) problems.push(`垂直對齊需為 top／center／bottom，收到 ${payload?.[spec.valignKey]}`);
        else if (va !== undefined) patch.verticalAlign = va;
      }
      const rowIdx = pickIndex(payload, ['row']);
      const cellIdx = pickIndex(payload, ['cell']);
      if (rowIdx.error) problems.push(rowIdx.error);
      if (cellIdx.error) problems.push(cellIdx.error);
      if (rowIdx.value === null) problems.push('缺少 row 座標（儲存格操作必須指定列）');
      if (cellIdx.value === null) problems.push('缺少 cell 座標（儲存格操作必須指定欄）');
      if (Object.keys(patch).length === 0) problems.push('未帶任何可套用的屬性');
      if (problems.length === 0) {
        handled = true;
        req = 'live';
        this._notify('cell:props-apply', {
          cmdId,
          mutation: spec.mutation,
          rootSlot: slot.value,
          row: rowIdx.value,
          cell: cellIdx.value,
          patch
        });
      } else {
        req = demoteRequirement(requirement);
        this._notify('table:props-rejected', { cmdId, problems });
      }
    } else if (cmdId === 'insert:picture') {
      // M5.1 InsertInlineImage：本層只認「有帶圖片位元組」與「請開檔案選取器」
      // 兩態都是合法請求；真正的成敗由引擎回傳決定（UI 層補失敗日誌）。
      const hasBytes = payload?.bytes !== undefined && payload?.bytes !== null;
      if (hasBytes) {
        const bytes = payload.bytes instanceof Uint8Array ? payload.bytes : Uint8Array.from(payload.bytes || []);
        const mime = String(payload?.mimeType || payload?.type || '').trim().toLowerCase();
        const problems = [];
        if (bytes.length === 0) problems.push('圖片位元組為空');
        if (!ENGINE_SUPPORTED_IMAGE_MIMES.includes(mime)) {
          problems.push(`引擎只支援 ${ENGINE_SUPPORTED_IMAGE_MIMES.join('／')}，收到 ${payload?.mimeType || '未宣告 MIME'}`);
        }
        if (problems.length === 0) {
          handled = true;
          req = 'live';
          this._notify('image:inline-insert', { bytes, mimeType: mime, altText: payload?.altText ?? null });
        } else {
          req = demoteRequirement(requirement);
          this._notify('image:inline-rejected', { cmdId, problems });
        }
      } else {
        // 沒有位元組＝請求開啟檔案選取器（走既有 #qword-file-input 路徑）。
        // 這是外殼能力面的一環，但圖片尚未進引擎，故不宣稱 live，沿用呼叫端
        // 標籤並匯流事件。
        handled = true;
        this._notify('image:pick-request', {});
      }
    } else if (cmdId === 'symbol:palette-select') {
      handled = true;
      req = 'live';
    }

    const logEntry = {
      timestamp,
      cmdId,
      requirement: req,
      payload,
      handled
    };

    this.commandLog.push(logEntry);
    this._notify('command', logEntry);

    return {
      ok: true,
      cmdId,
      requirement: req,
      handled,
      payload,
      timestamp,
      state: this.getState()
    };
  }

  /**
   * U4：引擎真實拒絕時由 UI 層補一筆失敗日誌（誠實回報的落點）。
   *
   * 為何要補：onCommand 的 handled=true 只代表「參數合規、有送到引擎」；
   * 引擎回 ok=false（例：表格結構座標越界、媒體檔頭與 MIME 不符）時，若
   * 日誌仍停留在 handled=true，考卷讀指令流就會看到「全部成功」的假象。
   * 這裡追加第二筆 handled=false ＋ engineRejected=true ＋引擎原始錯誤訊息，
   * 失敗事實才可被驗證（不覆寫第一筆，保留完整時序）。
   *
   * requirement 軸維持呼叫端原值（通常是 live）：**引擎被真的呼叫了**，
   * 能力面是活的，只是這一次操作被拒——把失敗誤標成 pending 會反過來謊稱
   * 能力不存在，那是另一種不誠實。
   *
   * @param {string} cmdId
   * @param {Object} payload
   * @param {string} engineError 引擎／外殼回傳的原始錯誤訊息
   * @param {string} [requirement] 原派發條目的 requirement（如實沿用）
   * @param {boolean} [engineCalled] 引擎是否真的被呼叫（false＝外殼前置檢查就擋下）
   * @returns {Object} 追加的日誌條目
   */
  recordCommandFailure(cmdId, payload = {}, engineError = '', requirement = 'live', engineCalled = true) {
    const logEntry = {
      timestamp: Date.now(),
      cmdId,
      requirement,
      payload: { ...payload, engineError: String(engineError || '操作失敗，未提供錯誤訊息') },
      handled: false,
      engineRejected: engineCalled,
      rejectedByShell: !engineCalled
    };
    this.commandLog.push(logEntry);
    this._notify('command:failed', logEntry);
    return logEntry;
  }

  getLog() {
    return [...this.commandLog];
  }

  clearLog() {
    this.commandLog = [];
  }
}
