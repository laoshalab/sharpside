// components/ui.js · 通用组件工厂。对应 docs/FRONTEND_DESIGN.md §8 通用组件库。
// 全部返回 HTMLElement，无框架依赖。

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
  return el('span', {}, (tags || []).map(t => el('span', { class: 'chip', text: t })));
}

/// periodTabs(periods, active, onChange) — 周期切换 tabs（1d/1w/1m/1y/ytd/all，对应前端 1天/1周/1个月/1年/年初至今/全部）。
export function periodTabs(periods = ['1d', '1w', '1m', '1y', 'ytd', 'all'], active = '1m', onChange) {
  const labels = { '1d': '1天', '1w': '1周', '1m': '1个月', '1y': '1年', 'ytd': '年初至今', 'all': '全部' };
  const wrap = el('div', { class: 'row' });
  const field = el('div', { class: 'field' }, [
    el('label', { text: '周期' }),
    (() => {
      const sel = el('select', { onchange: e => onChange(e.target.value) });
      for (const p of periods) {
        sel.appendChild(el('option', { value: p, text: labels[p] || p, ...(p === active ? { selected: 'selected' } : {}) }));
      }
      return sel;
    })(),
  ]);
  wrap.appendChild(field);
  return wrap;
}

/// dataTable({ columns, rows, onRowClick }) — 通用表格。
/// columns: [{ key, label, render?(row) }]
/// rows: 对象数组
export function dataTable({ columns, rows = [], onRowClick }) {
  const table = el('table');
  const thead = el('thead', {}, [el('tr', {}, columns.map(c => el('th', { text: c.label })))]);
  table.appendChild(thead);
  const tbody = el('tbody');
  for (const row of rows) {
    const tr = el('tr', onRowClick ? { class: 'clickable', onclick: () => onRowClick(row) } : {});
    for (const c of columns) {
      const val = c.render ? c.render(row) : row[c.key];
      tr.appendChild(el('td', { html: val == null ? '<span class="neutral">—</span>' : String(val) }));
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
export function emptyState({ icon = '∅', text = '暂无数据', action }) {
  return el('div', { class: 'empty' }, [
    el('div', { class: 'icon', text: icon }),
    el('p', { text }),
    action ? el('p', {}, [action]) : null,
  ]);
}

/// errorBox(message) — 错误展示。
export function errorBox(message) {
  return el('div', { class: 'card' }, [
    el('h2', { text: '出错了' }),
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

/// traderHref(platform, address) -> '#/traders/p/a'。
export function traderHref(platform, address) {
  return `#/traders/${encodeURIComponent(platform)}/${encodeURIComponent(address)}`;
}

/// traderLabel(row) -> alias || @x_username || address 前缀。
export function traderLabel(row) {
  return row.alias || (row.x_username ? '@' + row.x_username : null) || (row.address || '').slice(0, 8) + '…';
}
