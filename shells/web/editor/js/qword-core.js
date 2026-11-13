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
      // M4.1 InsertTable 真接線：本層只做指令記帳與事件匯流，實際 Mutation
      // 由 UI 層的 handleInsertTable 呼叫 adapter.insertTable 落地（與
      // format:bold 等同一分層契約）。誠實紀律：尺寸非法時不得沿用呼叫端
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

  getLog() {
    return [...this.commandLog];
  }

  clearLog() {
    this.commandLog = [];
  }
}
