// pages/copy-history.js · 成交历史。对应 docs/FRONTEND_DESIGN.md §6.11。
// 数据：GET /copier/me/copy-executions?since=&limit=&offset=&follow_id=&venue=&status=
// 主入口已并入 #/follows 页下方；#/copy-history 保留为跳转兼容。
import { el, dataTable, skeleton, emptyState } from '../components/ui.js';
import { listCopyExecutions } from '../lib/copier.js';
import { listMyFollows } from '../lib/follow.js';
import { toast } from '../store/toast.js';
import { t } from '../i18n/index.js';

const PAGE = 100;

/// 成交历史区块（无 shell），供 follows 页嵌入。
export function copyHistorySection() {
  const root = el('div', { id: 'copy-history', class: 'copy-history-section' });
  root.appendChild(el('div', { class: 'page-head' }, [
    el('h2', { text: t('copyHistory.title') }),
    el('button', { text: t('common.exportCsv'), onclick: () => exportCsv(filtersFromBar()) }),
  ]));

  const bar = el('div', { class: 'filter-bar' }, [
    field(t('copyHistory.time'), selectEl('ch-since', [['1d', t('copyHistory.last1d')], ['1w', t('copyHistory.last1w')], ['1m', t('copyHistory.last1m')], ['1y', t('copyHistory.last1y')], ['all', t('common.all')]], '1w')),
    field(t('copyHistory.follow'), selectEl('ch-follow_id', [['', t('common.all')]], '')),
    field('Venue', selectEl('ch-venue', [['', t('common.all')]], '')),
    field(t('copyHistory.status'), selectEl('ch-status', [['', t('common.all')], ['filled', 'filled'], ['skipped', 'skipped'], ['failed', 'failed']], '')),
  ]);
  root.appendChild(bar);

  const card = el('div', { class: 'card' }, [skeleton(6)]);
  root.appendChild(card);
  const pager = el('div', { class: 'pagination' });
  root.appendChild(pager);

  // 用 bar.querySelector 而非 document.getElementById：render() 在 root 挂载到
  // document 之前就被调用（router 在 await render() 完成后才 appendChild），此时
  // getElementById 找不到游离子树中的 select，会返回 null 并在 appendChild 时抛错。
  const filtersFromBar = () => ({
    since: sinceValue(bar.querySelector('#ch-since').value),
    follow_id: bar.querySelector('#ch-follow_id').value || undefined,
    venue: bar.querySelector('#ch-venue').value || undefined,
    status: bar.querySelector('#ch-status').value || undefined,
  });

  const state = { offset: 0, pageLen: 0, hasMore: false };
  const load = async () => {
    card.innerHTML = ''; card.appendChild(skeleton(6)); pager.innerHTML = '';
    try {
      const filters = filtersFromBar();
      const data = await listCopyExecutions({ ...filters, limit: PAGE, offset: state.offset });
      const rows = Array.isArray(data) ? data : (data?.items || []);
      state.pageLen = rows.length;
      state.hasMore = rows.length === PAGE;
      renderTable(card, rows);
      renderPager(pager, state, load);
    } catch (e) {
      card.innerHTML = '';
      card.appendChild(el('p', { class: 'muted', text: t('copyHistory.loadError', { message: e.message }) }));
    }
  };

  // 填充跟随/Venue 下拉后加载（不阻塞区块挂载）
  listMyFollows().then(follows => {
    const followSel = bar.querySelector('#ch-follow_id');
    const venueSel = bar.querySelector('#ch-venue');
    const venues = new Set();
    (follows || []).forEach(f => {
      const label = f.follow_alias || (f.follow_address ? f.follow_address.slice(0, 8) + '…' : f.id.slice(0, 8));
      followSel.appendChild(el('option', { value: f.id, text: label }));
      if (f.execute_venue) venues.add(f.execute_venue);
    });
    [...venues].sort().forEach(v => venueSel.appendChild(el('option', { value: v, text: v })));
  }).catch(() => {}).finally(() => {
    bar.querySelectorAll('select').forEach(s => s.onchange = () => { state.offset = 0; load(); });
    load();
  });

  return root;
}

function renderTable(card, rows) {
  card.innerHTML = '';
  if (!rows || rows.length === 0) {
    card.appendChild(emptyState({ icon: '📋', text: t('copyHistory.empty') }));
    return;
  }
  card.appendChild(dataTable({
    columns: [
      { key: 'executed_at', label: t('copyHistory.time'), render: r => r.executed_at ? new Date(r.executed_at).toLocaleString() : '—' },
      { key: 'follow', label: t('copyHistory.follow'), render: r => r.follow_alias || (r.follow_address || r.follow_relation_id || '—').toString().slice(0, 8) },
      { key: 'market_id', label: t('copyHistory.colMarket'), render: r => {
          const id = r.market_id || r.execute_market_id;
          if (!id) return '—';
          const s = String(id);
          return `<span class="muted" title="${escapeAttr(s)}">${escapeText(s.length > 14 ? s.slice(0, 12) + '…' : s)}</span>`;
        } },
      { key: 'side', label: t('copyHistory.colSide') },
      { key: 'filled_price', label: t('copyHistory.colPrice'), render: r => r.filled_price != null ? Number(r.filled_price).toFixed(4) : (r.price != null ? Number(r.price).toFixed(4) : '—') },
      { key: 'filled_size', label: t('copyHistory.colSize'), render: r => {
          const v = r.filled_size ?? r.size;
          return v != null && v !== '' && !Number.isNaN(Number(v)) ? Number(v).toFixed(4) : (v ?? '—');
        } },
      { key: 'pnl', label: 'P&L', render: r => r.realized_pnl != null ? `<span class="${r.realized_pnl >= 0 ? 'pos' : 'neg'}">${r.realized_pnl >= 0 ? '+' : ''}$${Number(r.realized_pnl).toFixed(2)}</span>` : t('copyHistory.openPosition') },
      { key: 'fee', label: 'fee', render: r => r.fee != null ? '$' + Number(r.fee).toFixed(2) : '—' },
      { key: 'status', label: t('copyHistory.status'), render: r => {
          const s = r.status || 'filled';
          const icon = s === 'filled' ? '✅' : s === 'skipped' ? '⏭' : s === 'failed' ? '❌' : '⏳';
          const cls = s === 'filled' ? 'pos' : (s === 'failed' || s === 'skipped') ? 'neg' : 'muted';
          const tip = r.skip_reason ? ` title="${escapeAttr(r.skip_reason)}"` : '';
          return `<span class="${cls}"${tip}>${icon} ${s}</span>`;
        } },
      { key: 'tx_hash', label: 'tx', render: r => r.tx_hash ? `<span class="muted" title="${escapeAttr(r.tx_hash)}">${String(r.tx_hash).slice(0, 10)}…</span>` : '—' },
    ],
    rows,
  }));
}

function renderPager(pager, state, load) {
  const cur = Math.floor(state.offset / PAGE) + 1;
  pager.appendChild(el('button', { class: 'sm', text: '←', disabled: state.offset === 0 ? 'disabled' : null, onclick: () => {
    if (state.offset > 0) { state.offset -= PAGE; load(); }
  } }));
  pager.appendChild(el('button', { class: 'sm active', text: String(cur) }));
  pager.appendChild(el('button', { class: 'sm', text: '→', disabled: state.hasMore ? null : 'disabled', onclick: () => {
    if (state.hasMore) { state.offset += PAGE; load(); }
  } }));
  const from = state.pageLen === 0 ? 0 : state.offset + 1;
  const to = state.offset + state.pageLen;
  pager.appendChild(el('span', { class: 'pg-info', text: state.pageLen === 0 ? t('copyHistory.noRecords') : t('copyHistory.showing', { from, to, more: state.hasMore ? '+' : '' }) }));
}

function sinceValue(key) {
  if (key === 'all') return undefined;
  const days = { '1d': 1, '1w': 7, '1m': 30, '1y': 365 }[key] || 7;
  return new Date(Date.now() - days * 864e5).toISOString();
}

async function exportCsv(filters) {
  try {
    const data = await listCopyExecutions({ ...filters, limit: 10000, offset: 0 });
    const rows = Array.isArray(data) ? data : (data?.items || []);
    const cols = ['executed_at', 'follow_relation_id', 'market_id', 'side', 'filled_price', 'filled_size', 'fee', 'status', 'tx_hash'];
    const csv = [cols.join(',')].concat(rows.map(r => cols.map(c => csvCell(r[c])).join(','))).join('\n');
    const blob = new Blob([csv], { type: 'text/csv' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob); a.download = 'copy-history.csv'; a.click();
    URL.revokeObjectURL(a.href);
    toast(t('copyHistory.exportSuccess'), 'success');
  } catch (e) { toast(t('copyHistory.exportFailed', { message: e.message }), 'error'); }
}
function csvCell(v) { const s = v == null ? '' : String(v); return /[",\n]/.test(s) ? '"' + s.replace(/"/g, '""') + '"' : s; }
function escapeText(s) { return String(s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c])); }
function escapeAttr(s) { return escapeText(s).replace(/`/g, '&#96;'); }
function field(label, child) { const w = el('div', { class: 'field' }); w.appendChild(el('label', { text: label })); w.appendChild(child); return w; }
function selectEl(name, options, val) { const s = el('select', { id: name }); for (const [v, l] of options) s.appendChild(el('option', { value: v, text: l, ...(v === val ? { selected: 'selected' } : {}) })); return s; }
