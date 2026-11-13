/**
 * QWord UI DOM 綁定層 (AGY-U2)
 * 負責 DOM 事件接聽、Ribbon 標籤與鍵盤無障礙操作、標尺拖曳回饋、縮放變換、
 * 自訂 Tooltip 與彈出面板（表格網格、色盤、符號選擇器）。
 *
 * U2 增強：
 * - 整合 EngineAdapter 真引擎（WasmEngine / wasm 載入）
 * - 開檔管線（檔案選擇器與拖放上傳、validateDocumentFile 檢驗）
 * - 真分頁 Canvas 渲染堆疊（取代靜態 mock 頁）
 * - 點擊定位（caret_at_json）與游標繪製（caret_rect_json）
 * - 鍵盤輸入閉環（type / backspace / enter / undo / redo / save）
 * - QAT 標題與狀態列即時資訊同步（頁數、字數、定案狀態）
 */

import { QWordCore, TABS, DEPENDENCY_MAP } from './qword-core.js';
import { EngineAdapter, getDefaultSampleBytes } from './engine-adapter.js';

export class QWordUI {
  constructor() {
    this.core = new QWordCore();
    this.adapter = new EngineAdapter();
    this.activePopover = null;
    this.rulerDragState = null;
    this.pageWrappers = [];

    // 將核心、外殼與引擎適配器掛載至 window 供探針與外部腳本呼叫
    window.QWordCore = this.core;
    window.QWordEngine = this.adapter;
    window.QWordShell = {
      core: this.core,
      adapter: this.adapter,
      onCommand: (cmdId, payload = {}) => {
        const btn = document.querySelector(`[data-command="${cmdId}"]`);
        const req = btn ? btn.getAttribute('data-requirement') : null;
        return this.dispatchCommand(cmdId, payload, req);
      },
      getState: () => this.core.getState(),
      getLog: () => this.core.getLog(),
      openFile: (bytes, name) => this.loadFile(bytes, name),
      openDefaultSample: () => this.loadDefaultSample()
    };
    window.__qwordLog = this.core.commandLog;

    this.initDOM();
    this.bindEvents();
    this.renderRulerTicks();
    this.syncStateToUI();

    // 異步預載入 wasm 引擎
    this.adapter.init().catch(err => {
      console.warn('[QWordUI] Wasm 預先初始化提醒:', err);
    });
  }

  dispatchCommand(cmdId, payload = {}, req = null) {
    const res = this.core.onCommand(cmdId, payload, req);
    window.__qwordLog = this.core.commandLog;
    console.log(`[QWordShell] 指令: ${cmdId}`, {
      requirement: res.requirement,
      handled: res.handled,
      payload
    });
    return res;
  }

  initDOM() {
    this.appEl = document.querySelector('.qword-app');
    this.tabButtons = Array.from(document.querySelectorAll('.qword-tab-btn:not(.is-file)'));
    this.tabPanels = Array.from(document.querySelectorAll('.qword-tab-panel'));
    this.pageContainer = document.querySelector('.qword-page-container');
    this.zoomSlider = document.querySelector('.qword-zoom-slider');
    this.zoomPercentEl = document.querySelector('.qword-zoom-percent');
    this.rulerContainer = document.querySelector('.qword-ruler-container');
    this.rulerTrack = document.querySelector('.qword-ruler-track');
    this.rulerBody = document.querySelector('.qword-ruler-body');
    this.guidelineEl = document.querySelector('.qword-indent-guideline');

    // U2 真引擎相關 DOM
    this.viewportEl = document.querySelector('.qword-workspace-viewport');
    this.fileInput = document.getElementById('qword-file-input');
    this.mockPageEl = document.querySelector('.qword-doc-mock');
    this.inputSink = document.getElementById('qword-input-sink');
    this.qatTitleEl = document.querySelector('.qword-qat-title');
    this.statusPageEl = document.querySelector('.qword-status-page-text');
    this.statusWordEl = document.querySelector('.qword-status-word-text');
    this.statusSettledEl = document.querySelector('.qword-status-settled-text');

    // 建立全域 Tooltip DOM
    this.tooltipEl = document.createElement('div');
    this.tooltipEl.className = 'qword-tooltip';
    this.tooltipEl.setAttribute('role', 'tooltip');
    document.body.appendChild(this.tooltipEl);
  }

  bindEvents() {
    // 監聽 Core 狀態改變
    this.core.subscribe((type, data) => {
      this.syncStateToUI(type, data);

      if (type === 'file:save') {
        this.saveCurrentDoc();
      } else if (type === 'file:open') {
        if (this.fileInput) this.fileInput.click();
      } else if (type === 'file:open-sample') {
        this.loadDefaultSample();
      } else if (type === 'edit:undo') {
        this.handleUndo();
      } else if (type === 'edit:redo') {
        this.handleRedo();
      } else if (type === 'edit:type') {
        this.handleType(data);
      } else if (type === 'edit:backspace') {
        this.handleBackspace();
      } else if (type === 'edit:enter') {
        this.handleEnter();
      } else if (type === 'format:bold') {
        this.handleFormatBold(data);
      } else if (type === 'format:italic') {
        this.handleFormatItalic(data);
      } else if (type === 'format:font-color') {
        this.handleFormatColor(data);
      } else if (type === 'font:grow') {
        this.handleGrowFont();
      } else if (type === 'font:shrink') {
        this.handleShrinkFont();
      } else if (type === 'font:size') {
        this.handleSetFontSize(data);
      } else if (type === 'paragraph:align') {
        this.handleAlign(data);
      } else if (type === 'style:apply') {
        this.handleApplyStyle(data);
      } else if (type === 'table:grid-insert') {
        this.handleInsertTable(data);
      } else if (type === 'page:change') {
        this.scrollToPage(data.currentPage - 1);
      }
    });

    // 1. 全域指令按鈕委派
    document.addEventListener('click', (e) => {
      // 關閉點擊外部的 popover
      if (this.activePopover && !this.activePopover.contains(e.target) && !e.target.closest('[data-popover-target]')) {
        this.closeAllPopovers();
      }

      const btn = e.target.closest('button[data-command]');
      if (!btn) return;

      const cmdId = btn.getAttribute('data-command');
      const req = btn.getAttribute('data-requirement') || 'live';
      this.dispatchCommand(cmdId, {}, req);
    });

    // 2. Ribbon 標籤切換與鍵盤導航
    this.tabButtons.forEach((btn, index) => {
      btn.addEventListener('click', () => {
        const targetTab = btn.getAttribute('data-tab-target');
        this.dispatchCommand(`tab:switch:${targetTab}`, {}, 'live');
      });

      btn.addEventListener('keydown', (e) => {
        let nextIndex = null;
        if (e.key === 'ArrowRight') {
          nextIndex = (index + 1) % this.tabButtons.length;
        } else if (e.key === 'ArrowLeft') {
          nextIndex = (index - 1 + this.tabButtons.length) % this.tabButtons.length;
        } else if (e.key === 'Home') {
          nextIndex = 0;
        } else if (e.key === 'End') {
          nextIndex = this.tabButtons.length - 1;
        }

        if (nextIndex !== null) {
          e.preventDefault();
          this.tabButtons[nextIndex].focus();
          const targetTab = this.tabButtons[nextIndex].getAttribute('data-tab-target');
          this.dispatchCommand(`tab:switch:${targetTab}`, {}, 'live');
        }
      });
    });

    // 3. Tooltip 懸浮處理
    document.addEventListener('mouseover', (e) => {
      const target = e.target.closest('button, [data-tooltip], [data-requirement]');
      if (!target || !document.body.contains(target)) return;

      const label = target.getAttribute('aria-label') || target.getAttribute('title') || target.innerText.trim();
      const req = target.getAttribute('data-requirement');
      const shortcut = target.getAttribute('data-shortcut');

      if (!label && !req) return;

      this.showTooltip(target, label, req, shortcut);
    });

    document.addEventListener('mouseout', (e) => {
      const target = e.target.closest('button, [data-tooltip], [data-requirement]');
      if (target) {
        this.hideTooltip();
      }
    });

    // 4. 縮放控制項事件
    if (this.zoomSlider) {
      this.zoomSlider.addEventListener('input', (e) => {
        this.dispatchCommand(`zoom:set:${e.target.value}`, {}, 'live');
      });
    }

    const zoomInBtn = document.querySelector('[data-command="zoom:in"]');
    if (zoomInBtn) {
      zoomInBtn.addEventListener('click', () => this.dispatchCommand('zoom:in', {}, 'live'));
    }

    const zoomOutBtn = document.querySelector('[data-command="zoom:out"]');
    if (zoomOutBtn) {
      zoomOutBtn.addEventListener('click', () => this.dispatchCommand('zoom:out', {}, 'live'));
    }

    const zoomFitBtn = document.querySelector('[data-command="zoom:fit-width"]');
    if (zoomFitBtn) {
      zoomFitBtn.addEventListener('click', () => this.dispatchCommand('zoom:fit-width', {}, 'live'));
    }

    if (this.zoomPercentEl) {
      this.zoomPercentEl.addEventListener('click', () => {
        this.dispatchCommand('zoom:100', {}, 'live');
      });
    }

    // 5. 檢視模式切換
    document.querySelectorAll('[data-view-mode-btn]').forEach(btn => {
      btn.addEventListener('click', () => {
        const mode = btn.getAttribute('data-view-mode-btn');
        this.dispatchCommand(`view:mode:${mode}`, {}, 'live');
      });
    });

    // 6. 標尺拖曳處理
    this.initRulerDrag();

    // 7. Popovers 觸發綁定
    this.initPopovers();

    // 8. 字型大小下拉選單變更
    const fontSizeSelect = document.querySelector('.qword-select-size');
    if (fontSizeSelect) {
      fontSizeSelect.addEventListener('change', (e) => {
        this.dispatchCommand('font:size', { size: e.target.value }, 'live');
      });
    }

    // 9. 開檔管線事件 (檔案選擇器與拖放上傳)
    this.initFilePipeline();

    // 10. 頁面點擊游標定位與鍵盤輸入打字閉環
    this.initTypingLoop();
  }

  initFilePipeline() {
    if (this.fileInput) {
      this.fileInput.addEventListener('change', async (e) => {
        const file = e.target.files && e.target.files[0];
        if (!file) return;
        const bytes = new Uint8Array(await file.arrayBuffer());
        await this.loadFile(bytes, file.name);
        this.fileInput.value = '';
      });
    }

    if (this.viewportEl) {
      this.viewportEl.addEventListener('dragover', (e) => {
        e.preventDefault();
        this.viewportEl.classList.add('drag-over');
      });
      this.viewportEl.addEventListener('dragleave', (e) => {
        if (!this.viewportEl.contains(e.relatedTarget)) {
          this.viewportEl.classList.remove('drag-over');
        }
      });
      this.viewportEl.addEventListener('drop', async (e) => {
        e.preventDefault();
        this.viewportEl.classList.remove('drag-over');
        const file = e.dataTransfer && e.dataTransfer.files && e.dataTransfer.files[0];
        if (!file) return;
        const bytes = new Uint8Array(await file.arrayBuffer());
        await this.loadFile(bytes, file.name);
      });
    }
  }

  initTypingLoop() {
    // 畫布滑鼠點擊定位與拖曳選取
    if (this.pageContainer) {
      let isPointerDown = false;
      let startPos = null;

      this.pageContainer.addEventListener('pointerdown', (e) => {
        const realPage = e.target.closest('.qword-real-page');
        if (!realPage || !this.adapter.engine) return;

        const pageIndex = parseInt(realPage.getAttribute('data-page-index'), 10);
        const canvas = realPage.querySelector('.qword-page-canvas');
        if (!canvas) return;

        const rect = canvas.getBoundingClientRect();
        const dl = this.adapter.getPageDisplayList(pageIndex);
        const widthPt = (dl && dl.width_pt) || 595;
        const scale = rect.width / widthPt;

        const clickXPt = (e.clientX - rect.left) / scale;
        const clickYPt = (e.clientY - rect.top) / scale;

        const pos = this.adapter.caretAt(pageIndex, clickXPt, clickYPt);
        if (pos) {
          isPointerDown = true;
          startPos = pos;
          this.adapter.caret = pos;
          this.adapter.selection = null;
          this.renderCaret();
          this.renderSelection();
        }
        if (this.inputSink) {
          this.inputSink.focus();
        }
      });

      this.pageContainer.addEventListener('pointermove', (e) => {
        if (!isPointerDown || !startPos || !this.adapter.engine) return;
        const realPage = e.target.closest('.qword-real-page');
        if (!realPage) return;

        const pageIndex = parseInt(realPage.getAttribute('data-page-index'), 10);
        const canvas = realPage.querySelector('.qword-page-canvas');
        if (!canvas) return;

        const rect = canvas.getBoundingClientRect();
        const dl = this.adapter.getPageDisplayList(pageIndex);
        const widthPt = (dl && dl.width_pt) || 595;
        const scale = rect.width / widthPt;

        const moveXPt = (e.clientX - rect.left) / scale;
        const moveYPt = (e.clientY - rect.top) / scale;

        const curPos = this.adapter.caretAt(pageIndex, moveXPt, moveYPt);
        if (curPos) {
          if (curPos.para !== startPos.para || curPos.cp !== startPos.cp) {
            this.adapter.selection = {
              target: 'Body',
              start: startPos,
              end: curPos
            };
            this.adapter.caret = curPos;
            this.renderCaret();
            this.renderSelection();
          }
        }
      });

      const onPointerEnd = () => {
        if (isPointerDown) {
          isPointerDown = false;
          if (this.adapter.selection) {
            const s = this.adapter.selection.start;
            const ed = this.adapter.selection.end;
            if (s.para === ed.para && s.cp === ed.cp) {
              this.adapter.selection = null;
            }
          }
          this.renderSelection();
        }
      };

      window.addEventListener('pointerup', onPointerEnd);
      window.addEventListener('pointercancel', onPointerEnd);
    }

    // 全域鍵盤快捷鍵與輸入處理
    document.addEventListener('keydown', (e) => {
      // 若聚焦於其他對話框內部的 input/textarea 則放行
      if (e.target && e.target !== this.inputSink && (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA')) {
        return;
      }

      // Ctrl+S / Cmd+S 存檔
      if ((e.ctrlKey || e.metaKey) && (e.key === 's' || e.key === 'S')) {
        e.preventDefault();
        this.dispatchCommand('file:save');
        return;
      }

      // Ctrl+Z / Cmd+Z 復原 (Shift 轉重做)
      if ((e.ctrlKey || e.metaKey) && (e.key === 'z' || e.key === 'Z')) {
        e.preventDefault();
        if (e.shiftKey) {
          this.dispatchCommand('edit:redo');
        } else {
          this.dispatchCommand('edit:undo');
        }
        return;
      }

      // Ctrl+Y / Cmd+Y 重複
      if ((e.ctrlKey || e.metaKey) && (e.key === 'y' || e.key === 'Y')) {
        e.preventDefault();
        this.dispatchCommand('edit:redo');
        return;
      }

      // Ctrl+O / Cmd+O 開啟
      if ((e.ctrlKey || e.metaKey) && (e.key === 'o' || e.key === 'O')) {
        e.preventDefault();
        this.dispatchCommand('file:open');
        return;
      }

      // Ctrl+B / Cmd+B 粗體
      if ((e.ctrlKey || e.metaKey) && (e.key === 'b' || e.key === 'B')) {
        e.preventDefault();
        this.dispatchCommand('format:bold');
        return;
      }

      // Ctrl+I / Cmd+I 斜體
      if ((e.ctrlKey || e.metaKey) && (e.key === 'i' || e.key === 'I')) {
        e.preventDefault();
        this.dispatchCommand('format:italic');
        return;
      }

      // Ctrl+L 靠左對齊
      if ((e.ctrlKey || e.metaKey) && (e.key === 'l' || e.key === 'L')) {
        e.preventDefault();
        this.dispatchCommand('paragraph:align-left');
        return;
      }

      // Ctrl+E 置中對齊
      if ((e.ctrlKey || e.metaKey) && (e.key === 'e' || e.key === 'E')) {
        e.preventDefault();
        this.dispatchCommand('paragraph:align-center');
        return;
      }

      // Ctrl+R 靠右對齊
      if ((e.ctrlKey || e.metaKey) && (e.key === 'r' || e.key === 'R')) {
        e.preventDefault();
        this.dispatchCommand('paragraph:align-right');
        return;
      }

      // Ctrl+J 左右對齊
      if ((e.ctrlKey || e.metaKey) && (e.key === 'j' || e.key === 'J')) {
        e.preventDefault();
        this.dispatchCommand('paragraph:align-justify');
        return;
      }

      if (!this.adapter.engine) return;

      if (e.key === 'Backspace') {
        e.preventDefault();
        this.handleBackspace();
        return;
      }

      if (e.key === 'Enter') {
        e.preventDefault();
        this.handleEnter();
        return;
      }

      if (e.key === 'ArrowLeft') {
        e.preventDefault();
        if (this.adapter.caret && this.adapter.caret.cp > 0) {
          this.adapter.caret.cp--;
          this.adapter.selection = null;
          this.renderCaret();
          this.renderSelection();
        }
        return;
      }

      if (e.key === 'ArrowRight') {
        e.preventDefault();
        if (this.adapter.caret) {
          this.adapter.caret.cp++;
          this.adapter.selection = null;
          this.renderCaret();
          this.renderSelection();
        }
        return;
      }
    });

    if (this.inputSink) {
      this.inputSink.addEventListener('input', () => {
        const val = this.inputSink.value;
        this.inputSink.value = '';
        if (val && this.adapter.engine) {
          this.handleType({ text: val });
        }
      });
    }
  }

  async loadFile(bytes, fileName) {
    try {
      await this.adapter.init();
      this.adapter.openFile(bytes, fileName);
      await this.renderDocument();
      this.syncEngineStats();
      console.log(`[QWordUI] 成功載入文件: ${fileName}`);
    } catch (err) {
      console.error('[QWordUI] 開檔失敗:', err);
      let userMsg = err.message || '檔案格式無法讀取';
      if (err.reason === 'FILE_EMPTY') {
        userMsg = '檔案內容為空（0 位元組），請選擇有效的 DOCX 文件。';
      } else if (err.reason === 'INVALID_EXTENSION') {
        userMsg = '不支援的檔案副檔名，編輯器目前僅支援 DOCX 格式。';
      } else if (err.reason === 'NOT_ZIP_CONTAINER') {
        userMsg = '所選檔案缺少 ZIP (PK) 封裝標頭，非合規 DOCX 容器。';
      }
      this.showErrorModal('開啟檔案失敗', userMsg, err.message);
    }
  }

  async loadDefaultSample() {
    const bytes = getDefaultSampleBytes();
    await this.loadFile(bytes, 'sample.docx');
  }

  async renderDocument() {
    if (!this.adapter.engine) return;

    // 隱藏靜態 mock 頁
    if (this.mockPageEl) {
      this.mockPageEl.style.display = 'none';
    }

    // 移除先前繪製的真頁面
    this.pageContainer.querySelectorAll('.qword-real-page').forEach(el => el.remove());

    const count = this.adapter.pageCount();
    this.pageWrappers = [];

    for (let i = 0; i < count; i++) {
      const wrapper = document.createElement('div');
      wrapper.className = 'qword-real-page';
      wrapper.setAttribute('data-page-index', i.toString());

      const canvas = document.createElement('canvas');
      canvas.className = 'qword-page-canvas';
      wrapper.appendChild(canvas);

      const caretLayer = document.createElement('div');
      caretLayer.className = 'qword-caret-layer';
      const caretEl = document.createElement('div');
      caretEl.className = 'qword-caret';
      caretLayer.appendChild(caretEl);
      wrapper.appendChild(caretLayer);

      this.pageContainer.appendChild(wrapper);
      this.pageWrappers.push(wrapper);

      await this.adapter.renderPage(i, canvas, 794);
    }

    this.renderCaret();
  }

  renderCaret() {
    if (!this.adapter.engine || !this.adapter.caret) return;
    const { para, cp } = this.adapter.caret;
    const rectJson = this.adapter.caretRect(para, cp);
    if (!rectJson) return;

    const pageIdx = rectJson.page;
    const wrapper = this.pageWrappers && this.pageWrappers[pageIdx];
    if (!wrapper) return;

    // 隱藏其他頁面的游標
    this.pageContainer.querySelectorAll('.qword-caret').forEach(c => {
      c.style.display = 'none';
    });

    const caretEl = wrapper.querySelector('.qword-caret');
    const canvas = wrapper.querySelector('.qword-page-canvas');
    if (!caretEl || !canvas) return;

    const dl = this.adapter.getPageDisplayList(pageIdx);
    const widthPt = (dl && dl.width_pt) || 595;
    const scale = canvas.width / widthPt;

    caretEl.style.left = `${Math.round(rectJson.x_pt * scale)}px`;
    caretEl.style.top = `${Math.round(rectJson.y_pt * scale)}px`;
    caretEl.style.height = `${Math.round((rectJson.height_pt || 12) * scale)}px`;
    caretEl.style.display = 'block';

    if (this.inputSink) {
      this.inputSink.focus();
    }
  }

  renderSelection() {
    if (!this.pageContainer) return;
    this.pageContainer.querySelectorAll('.qword-selection-rect').forEach(el => el.remove());

    if (!this.adapter.engine || !this.adapter.selection) return;

    let s = this.adapter.selection.start;
    let e = this.adapter.selection.end;
    if (s.para > e.para || (s.para === e.para && s.cp > e.cp)) {
      const tmp = s;
      s = e;
      e = tmp;
    }
    if (s.para === e.para && s.cp === e.cp) return;

    for (let p = s.para; p <= e.para; p++) {
      const text = this.adapter.paragraphText(p) || '';
      const totalCp = Array.from(text).length;
      const sCp = (p === s.para) ? s.cp : 0;
      const eCp = (p === e.para) ? e.cp : totalCp;

      let currentLine = null;
      for (let cp = sCp; cp < eCp; cp++) {
        const r = this.adapter.caretRect(p, cp);
        const rNext = this.adapter.caretRect(p, cp + 1);
        if (!r || !rNext) continue;

        if (!currentLine || currentLine.y_pt !== r.y_pt || currentLine.page !== r.page) {
          if (currentLine) {
            this._appendSelectionRect(currentLine);
          }
          currentLine = {
            page: r.page,
            x_pt: r.x_pt,
            y_pt: r.y_pt,
            width_pt: Math.max(2, rNext.x_pt - r.x_pt),
            height_pt: r.height_pt || 12
          };
        } else {
          currentLine.width_pt = Math.max(currentLine.width_pt, rNext.x_pt - currentLine.x_pt);
        }
      }
      if (currentLine) {
        this._appendSelectionRect(currentLine);
      }
    }
  }

  _appendSelectionRect(rectData) {
    const wrapper = this.pageWrappers && this.pageWrappers[rectData.page];
    if (!wrapper) return;
    const caretLayer = wrapper.querySelector('.qword-caret-layer');
    const canvas = wrapper.querySelector('.qword-page-canvas');
    if (!caretLayer || !canvas) return;

    const dl = this.adapter.getPageDisplayList(rectData.page);
    const widthPt = (dl && dl.width_pt) || 595;
    const scale = canvas.width / widthPt;

    const div = document.createElement('div');
    div.className = 'qword-selection-rect';
    div.style.left = `${Math.round(rectData.x_pt * scale)}px`;
    div.style.top = `${Math.round(rectData.y_pt * scale)}px`;
    div.style.width = `${Math.round(rectData.width_pt * scale)}px`;
    div.style.height = `${Math.round(rectData.height_pt * scale)}px`;
    caretLayer.appendChild(div);
  }

  async handleFormatBold(payload) {
    if (!this.adapter.engine) return;
    const res = this.adapter.formatBold(payload?.bold);
    if (res.ok) {
      await this.refreshPages();
      this.renderCaret();
      this.renderSelection();
      this.syncEngineStats();
    }
  }

  async handleFormatItalic(payload) {
    if (!this.adapter.engine) return;
    const res = this.adapter.formatItalic(payload?.italic);
    if (res.ok) {
      await this.refreshPages();
      this.renderCaret();
      this.renderSelection();
      this.syncEngineStats();
    }
  }

  async handleFormatColor(payload) {
    if (!this.adapter.engine) return;
    const color = payload?.color || '#e74c3c';
    const res = this.adapter.formatColor(color);
    if (res.ok) {
      await this.refreshPages();
      this.renderCaret();
      this.renderSelection();
      this.syncEngineStats();
    }
  }

  async handleGrowFont() {
    if (!this.adapter.engine) return;
    const res = this.adapter.growFont();
    if (res.ok) {
      await this.refreshPages();
      this.renderCaret();
      this.renderSelection();
      this.syncEngineStats();
    }
  }

  async handleShrinkFont() {
    if (!this.adapter.engine) return;
    const res = this.adapter.shrinkFont();
    if (res.ok) {
      await this.refreshPages();
      this.renderCaret();
      this.renderSelection();
      this.syncEngineStats();
    }
  }

  async handleSetFontSize(payload) {
    if (!this.adapter.engine) return;
    const size = payload?.size || 12;
    const res = this.adapter.setFontSize(size);
    if (res.ok) {
      await this.refreshPages();
      this.renderCaret();
      this.renderSelection();
      this.syncEngineStats();
    }
  }

  async handleAlign(payload) {
    if (!this.adapter.engine) return;
    const align = payload?.alignment || 'left';
    const res = this.adapter.setAlignment(align);
    if (res.ok) {
      await this.refreshPages();
      this.renderCaret();
      this.renderSelection();
      this.syncEngineStats();
    }
  }

  async handleApplyStyle(payload) {
    if (!this.adapter.engine) return;
    const styleId = payload?.styleId || 'Normal';
    const res = this.adapter.setStyle(styleId);
    if (res.ok) {
      await this.refreshPages();
      this.renderCaret();
      this.renderSelection();
      this.syncEngineStats();
    } else {
      console.warn(`[QWordUI] 樣式 ${styleId} 套用失敗:`, res.error);
    }
  }

  async handleType(payload = {}) {
    if (!this.adapter.engine) return;
    const text = payload.text || '';
    if (!text) return;

    const res = this.adapter.typeAt(payload.para, payload.cp, text);
    if (res.ok) {
      this.adapter.selection = null;
      await this.refreshPages();
      this.renderCaret();
      this.renderSelection();
      this.syncEngineStats();
    } else {
      console.warn('[QWordUI] 打字變更被拒絕:', res.error);
    }
  }

  async handleBackspace() {
    if (!this.adapter.engine) return;
    const res = this.adapter.backspace();
    if (res.ok) {
      this.adapter.selection = null;
      await this.refreshPages();
      this.renderCaret();
      this.renderSelection();
      this.syncEngineStats();
    } else if (res.error) {
      console.log('[QWordUI] 退格未執行:', res.error);
    }
  }

  async handleEnter() {
    if (!this.adapter.engine) return;
    const res = this.adapter.enter();
    if (res.ok) {
      this.adapter.selection = null;
      await this.refreshPages();
      this.renderCaret();
      this.renderSelection();
      this.syncEngineStats();
    }
  }

  async handleUndo() {
    if (!this.adapter.engine) return;
    const res = this.adapter.undo();
    if (res.ok && res.undone) {
      await this.refreshPages();
      this.renderCaret();
      this.renderSelection();
      this.syncEngineStats();
    }
  }

  async handleRedo() {
    if (!this.adapter.engine) return;
    const res = this.adapter.redo();
    if (res.ok && res.redone) {
      await this.refreshPages();
      this.renderCaret();
      this.renderSelection();
      this.syncEngineStats();
    }
  }

  async handleInsertTable(payload = {}) {
    const rows = Number(payload?.rows);
    const cols = Number(payload?.cols);
    if (!Number.isInteger(rows) || !Number.isInteger(cols) || rows <= 0 || cols <= 0) {
      this.showErrorModal('表格尺寸不合法', `需要列數與欄數皆為正整數，收到 ${payload?.rows} × ${payload?.cols}。`);
      return;
    }
    if (!this.adapter.engine) {
      // 誠實回報：引擎尚未載入文件時不得讓使用者以為表格已插入。
      this.showErrorModal('尚未開啟文件', '目前尚未載入任何 DOCX 文件，無法插入表格。');
      return;
    }
    const res = this.adapter.insertTable(rows, cols);
    if (res.ok) {
      await this.refreshPages();
      this.renderCaret();
      this.renderSelection();
      this.syncEngineStats();
      console.log(`[QWordUI] M4.1 InsertTable 成功：${rows} × ${cols}（段落數=${this.adapter.paragraphCount()}）`);
    } else {
      console.warn('[QWordUI] M4.1 InsertTable 引擎拒絕:', res.error);
      this.showErrorModal('表格插入失敗', String(res.error || '引擎拒絕了本次插入操作。'));
    }
  }

  async refreshPages() {
    const count = this.adapter.pageCount();
    if (!this.pageWrappers || this.pageWrappers.length !== count) {
      await this.renderDocument();
      return;
    }
    for (let i = 0; i < count; i++) {
      const wrapper = this.pageWrappers[i];
      const canvas = wrapper.querySelector('.qword-page-canvas');
      if (canvas) {
        await this.adapter.renderPage(i, canvas, 794);
      }
    }
  }

  saveCurrentDoc() {
    if (!this.adapter.engine) {
      this.showErrorModal('尚未開啟文件', '目前尚未載入任何 DOCX 文件，無法進行存檔操作。');
      return;
    }
    const bytes = this.adapter.save();
    console.log(`[QWordUI] 已成功存檔並下載 (${bytes.length} 位元組)`);
  }

  syncEngineStats() {
    if (!this.adapter.engine) return;
    const pageCount = this.adapter.pageCount();
    const wordCount = this.adapter.getWordCount();
    const isSettled = this.adapter.isSettled();
    const fileName = this.adapter.fileName;
    const curPage = (this.adapter.caret && this.adapter.caretRect(this.adapter.caret.para, this.adapter.caret.cp)?.page + 1) || 1;

    this.core.setDocStats({
      pageCount,
      currentPage: curPage,
      wordCount,
      isSettled,
      fileName
    });

    if (this.statusPageEl) {
      this.statusPageEl.textContent = `第 ${curPage} 頁，共 ${pageCount} 頁`;
    }
    if (this.statusWordEl) {
      this.statusWordEl.textContent = `${wordCount} 個字`;
    }
    if (this.statusSettledEl) {
      this.statusSettledEl.textContent = isSettled ? '● 已完成排版 (定案)' : '○ 背景排版中...';
    }
    if (this.qatTitleEl) {
      this.qatTitleEl.textContent = `${fileName} - Quire 文書編輯器`;
    }

    // 同步字型樣式按鈕 active 狀態與控制項數值
    const formatState = this.adapter.getCursorFormatState();
    const boldBtn = document.querySelector('[data-command="format:bold"]');
    if (boldBtn) boldBtn.classList.toggle('active', Boolean(formatState.bold));
    const italicBtn = document.querySelector('[data-command="format:italic"]');
    if (italicBtn) italicBtn.classList.toggle('active', Boolean(formatState.italic));
    const sizeSelect = document.querySelector('.qword-select-size');
    if (sizeSelect && formatState.size_pt) {
      const rounded = String(Math.round(formatState.size_pt));
      if (Array.from(sizeSelect.options).some(o => o.value === rounded)) {
        sizeSelect.value = rounded;
      }
    }
  }

  scrollToPage(pageIdx) {
    if (this.pageWrappers && this.pageWrappers[pageIdx]) {
      this.pageWrappers[pageIdx].scrollIntoView({ behavior: 'smooth', block: 'start' });
    }
  }

  showErrorModal(title, message, detail = '') {
    const modal = document.getElementById('modal-error');
    const titleEl = document.getElementById('modal-error-title');
    const msgEl = document.getElementById('modal-error-message');
    const detailEl = document.getElementById('modal-error-detail');

    if (titleEl) titleEl.textContent = title;
    if (msgEl) msgEl.textContent = message;
    if (detailEl) {
      detailEl.textContent = detail ? `錯誤細節代碼: ${detail}` : '';
      detailEl.style.display = detail ? 'block' : 'none';
    }
    if (modal) modal.classList.add('open');
  }

  showTooltip(target, label, req, shortcut) {
    let html = `<strong>${label || ''}</strong>`;
    if (shortcut) {
      html += ` <span style="opacity:0.7">(${shortcut})</span>`;
    }

    if (req && req.startsWith('pending:')) {
      const cardId = req.replace('pending:', '');
      const depInfo = DEPENDENCY_MAP[req] || { name: cardId, desc: '開發中' };
      if (req === 'pending:shell') {
        // 誠實措辭：引擎側 Mutation 欄位已真實存在，缺口是瀏覽器外殼接線，
        // 不可再對使用者宣稱「需要編輯 API（開發中）」。
        html += `<span class="qword-tooltip-req">⚠️ 編輯 API 已支援，外殼接線中（${depInfo.name}）</span>`;
      } else {
        html += `<span class="qword-tooltip-req">⚠️ 需要編輯 API（開發中: ${depInfo.name}）</span>`;
      }
    } else if (req === 'live') {
      html += `<span style="display:block;font-size:9px;color:#85e085;margin-top:2px;">● 現已可用（真引擎/檢視能力）</span>`;
    }

    this.tooltipEl.innerHTML = html;
    this.tooltipEl.classList.add('visible');

    const rect = target.getBoundingClientRect();
    let top = rect.bottom + 6;
    let left = rect.left + (rect.width / 2) - (this.tooltipEl.offsetWidth / 2);

    if (left < 4) left = 4;
    if (left + this.tooltipEl.offsetWidth > window.innerWidth - 4) {
      left = window.innerWidth - this.tooltipEl.offsetWidth - 4;
    }
    if (top + this.tooltipEl.offsetHeight > window.innerHeight - 4) {
      top = rect.top - this.tooltipEl.offsetHeight - 6;
    }

    this.tooltipEl.style.top = `${top}px`;
    this.tooltipEl.style.left = `${left}px`;
  }

  hideTooltip() {
    this.tooltipEl.classList.remove('visible');
  }

  syncStateToUI(type, data) {
    const state = this.core.getState();

    // 同步 Active Tab
    this.tabButtons.forEach(btn => {
      const tabTarget = btn.getAttribute('data-tab-target');
      const isActive = tabTarget === state.activeTabId;
      btn.classList.toggle('active', isActive);
      btn.setAttribute('aria-selected', isActive ? 'true' : 'false');
    });

    this.tabPanels.forEach(panel => {
      const panelId = panel.id;
      const isActive = panelId === state.activeTabId;
      panel.classList.toggle('active', isActive);
      panel.setAttribute('aria-hidden', isActive ? 'false' : 'true');
    });

    // 同步縮放
    if (this.pageContainer) {
      this.pageContainer.style.transform = `scale(${state.zoom / 100})`;
    }
    if (this.zoomSlider) {
      this.zoomSlider.value = state.zoom;
    }
    if (this.zoomPercentEl) {
      this.zoomPercentEl.textContent = `${state.zoom}%`;
    }

    // 同步檢視模式
    if (this.appEl) {
      this.appEl.setAttribute('data-view-mode', state.viewMode);
    }
    document.querySelectorAll('[data-view-mode-btn]').forEach(btn => {
      const mode = btn.getAttribute('data-view-mode-btn');
      btn.classList.toggle('active', mode === state.viewMode);
    });

    // 同步標尺可見性
    if (this.rulerContainer) {
      this.rulerContainer.style.display = state.rulerVisible ? 'flex' : 'none';
    }
    const vruler = document.querySelector('.qword-vruler');
    if (vruler) {
      vruler.style.display = state.rulerVisible ? 'flex' : 'none';
    }
  }

  renderRulerTicks() {
    const ticksContainer = document.querySelector('.qword-ruler-ticks');
    if (!ticksContainer) return;

    ticksContainer.innerHTML = '';
    // A4 頁寬為 21cm，兩邊留白各約 2.5cm，中間編輯區為 16cm
    const totalCm = 16;
    for (let cm = 0; cm <= totalCm; cm++) {
      const pct = (cm / totalCm) * 100;

      const majorTick = document.createElement('div');
      majorTick.className = 'qword-ruler-tick major';
      majorTick.style.left = `${pct}%`;
      ticksContainer.appendChild(majorTick);

      const num = document.createElement('div');
      num.className = 'qword-ruler-tick-num';
      num.style.left = `${pct}%`;
      num.textContent = cm.toString();
      ticksContainer.appendChild(num);

      if (cm < totalCm) {
        const minorTick = document.createElement('div');
        minorTick.className = 'qword-ruler-tick minor';
        minorTick.style.left = `${pct + (100 / totalCm / 2)}%`;
        ticksContainer.appendChild(minorTick);
      }
    }
  }

  initRulerDrag() {
    const markers = document.querySelectorAll('.qword-ruler-marker');
    markers.forEach(marker => {
      marker.addEventListener('pointerdown', (e) => {
        e.preventDefault();
        marker.setPointerCapture(e.pointerId);
        const markerType = marker.getAttribute('data-marker-type');
        const startX = e.clientX;
        const rect = this.rulerBody.getBoundingClientRect();

        this.rulerDragState = {
          marker,
          markerType,
          startX,
          trackRect: rect
        };

        if (this.guidelineEl) {
          this.guidelineEl.style.display = 'block';
        }

        const onPointerMove = (moveEvt) => {
          if (!this.rulerDragState) return;
          const currentX = moveEvt.clientX;
          const offset = currentX - rect.left;
          const ratio = Math.max(0, Math.min(1, offset / rect.width));
          const cm = ratio * 16;

          marker.style.left = `${ratio * 100}%`;
          if (this.guidelineEl) {
            this.guidelineEl.style.left = `calc(96px + ${ratio * 100 * 0.76}%)`;
          }
          this.core.setIndent(markerType, cm);
        };

        const onPointerUp = (upEvt) => {
          marker.releasePointerCapture(upEvt.pointerId);
          marker.removeEventListener('pointermove', onPointerMove);
          marker.removeEventListener('pointerup', onPointerUp);
          if (this.guidelineEl) {
            this.guidelineEl.style.display = 'none';
          }
          this.rulerDragState = null;
        };

        marker.addEventListener('pointermove', onPointerMove);
        marker.addEventListener('pointerup', onPointerUp);
      });
    });
  }

  initPopovers() {
    // 綁定具有 [data-popover-target] 的按鈕
    document.querySelectorAll('[data-popover-target]').forEach(trigger => {
      trigger.addEventListener('click', (e) => {
        e.stopPropagation();
        const targetId = trigger.getAttribute('data-popover-target');
        const popover = document.getElementById(targetId);
        if (!popover) return;

        if (popover.classList.contains('open')) {
          this.closeAllPopovers();
        } else {
          this.closeAllPopovers();
          this.openPopover(trigger, popover);
        }
      });
    });

    // 1. 表格 8x5 網格互動
    const tableGrid = document.querySelector('.qword-table-grid');
    const tableTitle = document.querySelector('.qword-table-picker-title');
    if (tableGrid) {
      tableGrid.innerHTML = '';
      for (let r = 1; r <= 5; r++) {
        for (let c = 1; c <= 8; c++) {
          const cell = document.createElement('div');
          cell.className = 'qword-table-cell';
          cell.setAttribute('data-row', r);
          cell.setAttribute('data-col', c);

          cell.addEventListener('mouseenter', () => {
            if (tableTitle) {
              tableTitle.textContent = `${c} × ${r} 表格`;
            }
            document.querySelectorAll('.qword-table-cell').forEach(other => {
              const or = parseInt(other.getAttribute('data-row'), 10);
              const oc = parseInt(other.getAttribute('data-col'), 10);
              other.classList.toggle('highlight', or <= r && oc <= c);
            });
          });

          cell.addEventListener('click', () => {
            this.dispatchCommand('table:grid-select', { rows: r, cols: c }, 'live');
            this.closeAllPopovers();
          });

          tableGrid.appendChild(cell);
        }
      }
    }

    // 2. 特殊符號面板填充
    const symbolGrid = document.querySelector('.qword-symbol-grid');
    if (symbolGrid) {
      const symbols = [
        '「', '」', '『', '』', '《', '》', '〈', '〉',
        '【', '】', '〔', '〕', '—', '…', '·', '、',
        '。', '，', '！', '？', '：', '；', '“', '”',
        '§', '¶', '©', '®', '™', '±', '×', '÷',
        '≠', '≤', '≥', '∞', '√', '∑', '∫', '★'
      ];
      symbolGrid.innerHTML = '';
      symbols.forEach(char => {
        const item = document.createElement('button');
        item.type = 'button';
        item.className = 'qword-symbol-item';
        item.textContent = char;
        item.setAttribute('aria-label', `符號 ${char}`);
        item.setAttribute('data-requirement', 'live');
        item.addEventListener('click', () => {
          this.dispatchCommand('symbol:palette-select', { char }, 'live');
          this.closeAllPopovers();
        });
        symbolGrid.appendChild(item);
      });
    }

    // 3. 色彩調色盤填充
    document.querySelectorAll('.qword-color-picker-grid').forEach(palette => {
      const colors = [
        '#ffffff', '#000000', '#eeece1', '#1f497d', '#4f81bd', '#c0504d', '#9bbb59', '#8064a2', '#4bacc6', '#f79646',
        '#f2f2f2', '#7f7f7f', '#ddd9c3', '#c6d9f0', '#dbe5f1', '#f2dcdb', '#ebf1dd', '#e5e0ec', '#dbeef3', '#fdeada',
        '#d8d8d8', '#595959', '#c4bd97', '#8db3e2', '#b8cce4', '#e5b9b7', '#d7e3bc', '#ccc1da', '#b7dde8', '#fbd5b5',
        '#bfbfbf', '#3f3f3f', '#938953', '#548dd4', '#95b3d7', '#d99694', '#c3d69b', '#b2a2c7', '#92cddc', '#fac08f',
        '#a5a5a5', '#262626', '#494429', '#17365d', '#366092', '#953734', '#76933c', '#5f497a', '#31859b', '#e36c09',
        '#7f7f7f', '#0c0c0c', '#1d1b10', '#0f243e', '#244061', '#632423', '#4f6128', '#3f3151', '#205867', '#974806'
      ];
      palette.innerHTML = '';
      colors.forEach(col => {
        const swatch = document.createElement('div');
        swatch.className = 'qword-color-swatch';
        swatch.style.backgroundColor = col;
        swatch.setAttribute('data-color', col);
        swatch.addEventListener('click', () => {
          const mode = palette.getAttribute('data-color-target') || 'font';
          const req = mode === 'font' ? 'live' : 'pending:K29b';
          this.dispatchCommand(`format:color:${mode}`, { color: col }, req);
          this.closeAllPopovers();
        });
        palette.appendChild(swatch);
      });
    });
  }

  openPopover(trigger, popover) {
    const rect = trigger.getBoundingClientRect();
    popover.style.top = `${rect.bottom + 4}px`;
    popover.style.left = `${Math.min(rect.left, window.innerWidth - 300)}px`;
    popover.classList.add('open');
    this.activePopover = popover;
  }

  closeAllPopovers() {
    document.querySelectorAll('.qword-popover.open').forEach(p => p.classList.remove('open'));
    this.activePopover = null;
  }
}

// 瀏覽器載入後自動啟動
if (typeof window !== 'undefined') {
  window.addEventListener('DOMContentLoaded', () => {
    new QWordUI();
  });
}
