/**
 * QWord UI DOM 綁定層
 * 負責 DOM 事件接聽、Ribbon 標籤與鍵盤無障礙操作、標尺拖曳回饋、縮放變換、
 * 自訂 Tooltip 與彈出面板（表格網格、色盤、符號選擇器）。
 * 核心指令一律透過 QWordCore.onCommand 派發。
 */

import { QWordCore, TABS, DEPENDENCY_MAP } from './qword-core.js';

export class QWordUI {
  constructor() {
    this.core = new QWordCore();
    this.activePopover = null;
    this.rulerDragState = null;

    // 將核心與外殼掛載至 window 供探針與後續卡整合
    window.QWordCore = this.core;
    window.QWordShell = {
      core: this.core,
      onCommand: (cmdId, payload = {}) => {
        const btn = document.querySelector(`[data-command="${cmdId}"]`);
        const req = btn ? btn.getAttribute('data-requirement') : null;
        return this.dispatchCommand(cmdId, payload, req);
      },
      getState: () => this.core.getState(),
      getLog: () => this.core.getLog()
    };
    window.__qwordLog = this.core.commandLog;

    this.initDOM();
    this.bindEvents();
    this.renderRulerTicks();
    this.syncStateToUI();
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

    // 7. Popovers 觸發綁定 (表格網格、色盤、特殊符號)
    this.initPopovers();
  }

  showTooltip(target, label, req, shortcut) {
    let html = `<strong>${label || ''}</strong>`;
    if (shortcut) {
      html += ` <span style="opacity:0.7">(${shortcut})</span>`;
    }

    if (req && req.startsWith('pending:')) {
      const cardId = req.replace('pending:', '');
      const depInfo = DEPENDENCY_MAP[req] || { name: cardId, desc: '開發中' };
      html += `<span class="qword-tooltip-req">⚠️ 需要編輯 API（開發中: ${depInfo.name}）</span>`;
    } else if (req === 'live') {
      html += `<span style="display:block;font-size:9px;color:#85e085;margin-top:2px;">● 現已可用（外殼/檢視能力）</span>`;
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
    // 每公分繪製一大刻度，每半公分繪製一小刻度
    const totalCm = 16;
    for (let cm = 0; cm <= totalCm; cm++) {
      const pct = (cm / totalCm) * 100;

      // 主要刻度與數字
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
          this.dispatchCommand(`format:color:${mode}`, { color: col }, 'pending:K29a');
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
