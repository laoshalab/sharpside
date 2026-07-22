// components/ui.js · 通用组件工厂。对应 docs/FRONTEND_DESIGN.md §8 通用组件库。
// 全部返回 HTMLElement，无框架依赖。

import { t } from '../i18n/index.js';

/// el(tag, props, children) — 极简 DOM 构造器。
export function el(tag, props = {}, children = []) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (k === 'class') node.className = v;
    else if (k === 'text') node.textContent = v;
    else if (k === 'html') node.innerHTML = v;
    else if (k === 'onclick') node.onclick = v;
    else if (k.startsWith('on') && typeof v === 'function') node.addEventListener(k.slice(2).toLowerCase(), v);
    else if (v !== undefined && v !== null) node.setAttribute(k, v);
  }
  const kids = Array.isArray(children) ? children : [children];
  for (const c of kids) {
    if (c == null) continue;
    node.appendChild(typeof c === 'string' ? document.createTextNode(c) : c);
  }
  return node;
}

/// statCard({ label, value, sub, cls }) — KPI 卡。
export function statCard({ label, value, sub, cls = '' }) {
  return el('div', { class: 'kpi' }, [
    el('div', { class: 'label', text: label }),
    el('div', { class: 'value ' + cls, text: value }),
    sub ? el('div', { class: 'sub', text: sub }) : null,
  ]);
}

/// tagChips(tags) — 标签 chip 组。
export function tagChips(tags = []) {
  return el('span', {}, (tags || []).map(tag => el('span', { class: 'chip', text: tag })));
}

/// periodTabs(periods, active, onChange) — 周期切换 tabs（1d/1w/1m/1y/ytd/all）。
/// `periods` 元素可为字符串或 `{ key, label }`（key 为 API 值，label 为显示文案）。
export function periodTabs(periods = [
  { key: '1d', label: t('common.period.1d') },
  { key: '1w', label: t('common.period.1w') },
  { key: '1m', label: t('common.period.1m') },
  { key: '1y', label: t('common.period.1y') },
  { key: 'ytd', label: t('common.period.ytd') },
  { key: 'all', label: t('common.period.all') },
], active = '1m', onChange) {
  const wrap = el('div', { class: 'row' });
  const field = el('div', { class: 'field' }, [
    el('label', { text: t('ui.periodLabel') }),
    (() => {
      const sel = el('select', { onchange: e => onChange(e.target.value) });
      for (const p of periods) {
        const v = typeof p === 'object' ? p.key : p;
        const label = typeof p === 'object' ? p.label : p;
        sel.appendChild(el('option', { value: v, text: label, ...(v === active ? { selected: 'selected' } : {}) }));
      }
      return sel;
    })(),
  ]);
  wrap.appendChild(field);
  return wrap;
}

/// dataTable({ columns, rows, onRowClick, className }) — 通用表格。
/// columns: [{ key, label, render?(row), class? }]
///   class?: 同时应用到 th/td，用于按列宽度/截断等样式控制。
/// className?: 应用到 <table>（如 leaderboard-table）。
/// rows: 对象数组
export function dataTable({ columns, rows = [], onRowClick, className }) {
  const table = el('table', className ? { class: className } : {});
  const thead = el('thead', {}, [el('tr', {}, columns.map(c => el('th', { text: c.label, ...(c.class ? { class: c.class } : {}) })))]);
  table.appendChild(thead);
  const tbody = el('tbody');
  for (const row of rows) {
    const tr = el('tr', onRowClick ? { class: 'clickable', onclick: () => onRowClick(row) } : {});
    for (const c of columns) {
      const val = c.render ? c.render(row) : row[c.key];
      tr.appendChild(el('td', { html: val == null ? '<span class="neutral">—</span>' : String(val), ...(c.class ? { class: c.class } : {}) }));
    }
    tbody.appendChild(tr);
  }
  table.appendChild(tbody);
  return table;
}

/// skeleton(n=3) — 骨架占位。
export function skeleton(n = 3) {
  return el('div', {}, Array.from({ length: n }, () => el('div', { class: 'skeleton line' })));
}

/// emptyState({ icon, text, action }) — 空态。
export function emptyState({ icon = '∅', text = t('ui.emptyDefault'), action }) {
  return el('div', { class: 'empty' }, [
    el('div', { class: 'icon', text: icon }),
    el('p', { text }),
    action ? el('p', {}, [action]) : null,
  ]);
}

/// errorBox(message) — 错误展示。
export function errorBox(message) {
  return el('div', { class: 'card' }, [
    el('h2', { text: t('ui.errorTitle') }),
    el('p', { class: 'neg', text: message }),
  ]);
}

/// fmtPct(0.123) -> '12.3%'；null/undefined -> '—'。
export function fmtPct(v, digits = 1) {
  if (v == null || Number.isNaN(v)) return '—';
  return (Number(v) * 100).toFixed(digits) + '%';
}

/// fmtNum(v) -> 千分位；null -> '—'。
export function fmtNum(v, digits = 2) {
  if (v == null || Number.isNaN(v)) return '—';
  return Number(v).toLocaleString('en-US', { maximumFractionDigits: digits });
}

/// fmtUSD(v) -> '$1,234.56'；null -> '—'。
export function fmtUSD(v, digits = 2) {
  if (v == null || Number.isNaN(v)) return '—';
  return '$' + fmtNum(v, digits);
}

/// pnlClass(v) -> 'pos'|'neg'|'neutral'。
export function pnlClass(v) {
  if (v == null || Number.isNaN(v) || v === 0) return 'neutral';
  return Number(v) > 0 ? 'pos' : 'neg';
}

/// 已知平台图标路径；未知平台回退为首字母 badge。
const PLATFORM_ICONS = {
  polymarket: '/icons/platforms/polymarket.svg',
  kalshi: '/icons/platforms/kalshi.svg',
  manifold: '/icons/platforms/manifold.svg',
  zeitgeist: '/icons/platforms/zeitgeist.svg',
  azuro: '/icons/platforms/azuro.svg',
};

/// platformIcon(platform) — 平台小图标 HTML（表格列 / badge 用）。
/// 已知平台用本地 SVG；未知平台用首字母兜底，title 始终为平台名。
export function platformIcon(platform) {
  const name = String(platform || '').trim();
  if (!name) return '<span class="neutral">—</span>';
  const safe = name.replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  const src = PLATFORM_ICONS[name.toLowerCase()];
  if (src) {
    return `<img class="platform-icon" src="${src}" alt="${safe}" title="${safe}" width="20" height="20" loading="lazy">`;
  }
  const letter = safe.slice(0, 1).toUpperCase();
  return `<span class="platform-icon platform-icon-fallback" title="${safe}">${letter}</span>`;
}

/// traderHref(platform, address) -> '#/traders/p/a'。
export function traderHref(platform, address) {
  return `#/traders/${encodeURIComponent(platform)}/${encodeURIComponent(address)}`;
}

/// traderLabel(row) -> alias || @x_username || address 前缀。
export function traderLabel(row) {
  return row.alias || (row.x_username ? '@' + row.x_username : null) || (row.address || '').slice(0, 8) + '…';
}
