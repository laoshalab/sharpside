// pages/shadow-health.js · 影子数据健康。对应 docs/SHADOW_MODE.md §8。
import { el, dataTable, skeleton, emptyState, statCard, fmtPct, fmtNum, escHtml } from '../components/ui.js';
import { nav } from '../components/nav.js';
import { shadowSummary, shadowHeatmap, shadowTopDiffs, shadowAudits } from '../api/admin.js';

const HOURS_OPTS = [6, 24, 72, 168];
const STATUS_OPTS = ['', 'ok', 'warn', 'alert'];

export async function shadowHealthPage() {
  const state = { hours: 24, topStatus: 'alert', auditStatus: '', platform: '', address: '', metric: '' };
  const root = el('div');
  root.appendChild(nav());
  const c = el('div', { class: 'container' });
  root.appendChild(c);
  c.appendChild(el('h1', { text: '数据健康' }));
  c.appendChild(el('p', {
    class: 'muted',
    text: '影子模式交叉校验报表：只读审计，不进用户展示。目标近窗 ok 率 > 95%。',
  }));

  const bar = el('div', { class: 'row' });
  bar.appendChild(field('时间窗', selectEl(String(state.hours), HOURS_OPTS.map(String), v => {
    state.hours = Number(v);
    load();
  }, h => `${h}h`)));
  c.appendChild(bar);

  const kpi = el('div', { class: 'kpi-grid' }, [skeleton(1)]);
  c.appendChild(kpi);

  c.appendChild(el('h2', { text: '偏离热力（metric × period）' }));
  const heatCard = el('div', { class: 'card' }, [skeleton(2)]);
  c.appendChild(heatCard);

  c.appendChild(el('h2', { text: 'Top 偏离' }));
  const topBar = el('div', { class: 'row' });
  topBar.appendChild(field('status', selectEl(state.topStatus, STATUS_OPTS, v => {
    state.topStatus = v;
    loadTop();
  }, s => s || '全部（按 |diff_pct|）')));
  c.appendChild(topBar);
  const topCard = el('div', { class: 'card' }, [skeleton(2)]);
  c.appendChild(topCard);

  c.appendChild(el('h2', { text: '审计明细' }));
  const filt = el('div', { class: 'row' });
  filt.appendChild(field('status', selectEl(state.auditStatus, STATUS_OPTS, v => {
    state.auditStatus = v;
  }, s => s || '全部')));
  filt.appendChild(field('platform', el('input', {
    value: '',
    placeholder: 'polymarket',
    oninput: e => { state.platform = e.target.value.trim(); },
  })));
  filt.appendChild(field('address', el('input', {
    value: '',
    placeholder: '0x…',
    oninput: e => { state.address = e.target.value.trim(); },
  })));
  filt.appendChild(field('metric', el('input', {
    value: '',
    placeholder: 'roi',
    oninput: e => { state.metric = e.target.value.trim(); },
  })));
  filt.appendChild(el('div', { class: 'field' }, [
    el('label', { text: ' ' }),
    el('button', { class: 'primary', text: '查询', onclick: () => loadAudits() }),
  ]));
  c.appendChild(filt);
  const auditCard = el('div', { class: 'card' }, [skeleton(2)]);
  c.appendChild(auditCard);

  async function load() {
    await Promise.all([loadSummary(), loadHeat(), loadTop(), loadAudits()]);
  }

  async function loadSummary() {
    kpi.innerHTML = '';
    kpi.appendChild(skeleton(1));
    try {
      const s = await shadowSummary(state.hours);
      kpi.innerHTML = '';
      const rateCls = s.ok_rate >= (s.target_ok_rate ?? 0.95) ? 'pos' : 'neg';
      kpi.appendChild(statCard({
        label: `近 ${s.hours}h 一致率`,
        value: fmtPct(s.ok_rate),
        sub: `目标 ≥ ${fmtPct(s.target_ok_rate ?? 0.95)}`,
        cls: rateCls,
      }));
      kpi.appendChild(statCard({ label: '总计', value: String(s.total ?? 0) }));
      kpi.appendChild(statCard({ label: 'ok', value: String(s.ok_count ?? 0), cls: 'pos' }));
      kpi.appendChild(statCard({ label: 'warn', value: String(s.warn_count ?? 0), cls: 'neutral' }));
      kpi.appendChild(statCard({ label: 'alert', value: String(s.alert_count ?? 0), cls: 'neg' }));
    } catch (e) {
      kpi.innerHTML = '';
      kpi.appendChild(el('p', { class: 'neg', text: '汇总失败：' + e.message }));
    }
  }

  async function loadHeat() {
    heatCard.innerHTML = '';
    heatCard.appendChild(skeleton(2));
    try {
      const rows = await shadowHeatmap(state.hours);
      heatCard.innerHTML = '';
      if (!rows || rows.length === 0) {
        heatCard.appendChild(emptyState({ text: '该时间窗无审计数据' }));
        return;
      }
      heatCard.appendChild(dataTable({
        columns: [
          { key: 'metric_name', label: 'metric' },
          { key: 'period', label: 'period' },
          { key: 'ok_count', label: 'ok' },
          { key: 'warn_count', label: 'warn' },
          {
            key: 'alert_count',
            label: 'alert',
            render: r => r.alert_count > 0 ? `<span class="neg">${r.alert_count}</span>` : String(r.alert_count),
          },
        ],
        rows,
      }));
    } catch (e) {
      heatCard.innerHTML = '';
      heatCard.appendChild(el('p', { class: 'neg', text: '热力失败：' + e.message }));
    }
  }

  async function loadTop() {
    topCard.innerHTML = '';
    topCard.appendChild(skeleton(2));
    try {
      const rows = await shadowTopDiffs({
        hours: state.hours,
        status: state.topStatus || undefined,
        limit: 20,
      });
      topCard.innerHTML = '';
      if (!rows || rows.length === 0) {
        topCard.appendChild(emptyState({ text: '无偏离记录' }));
        return;
      }
      topCard.appendChild(auditTable(rows));
    } catch (e) {
      topCard.innerHTML = '';
      topCard.appendChild(el('p', { class: 'neg', text: 'Top 偏离失败：' + e.message }));
    }
  }

  async function loadAudits() {
    auditCard.innerHTML = '';
    auditCard.appendChild(skeleton(2));
    try {
      const rows = await shadowAudits({
        hours: state.hours,
        platform: state.platform || undefined,
        address: state.address || undefined,
        metric: state.metric || undefined,
        status: state.auditStatus || undefined,
        limit: 100,
      });
      auditCard.innerHTML = '';
      if (!rows || rows.length === 0) {
        auditCard.appendChild(emptyState({ text: '无匹配审计行' }));
        return;
      }
      auditCard.appendChild(auditTable(rows));
    } catch (e) {
      auditCard.innerHTML = '';
      auditCard.appendChild(el('p', { class: 'neg', text: '明细失败：' + e.message }));
    }
  }

  load();
  return root;
}

function auditTable(rows) {
  return dataTable({
    columns: [
      {
        key: 'status',
        label: 'status',
        render: r => `<span class="${r.status === 'alert' ? 'neg' : r.status === 'warn' ? 'neutral' : 'pos'}">${escHtml(r.status)}</span>`,
      },
      { key: 'platform', label: '平台' },
      {
        key: 'address',
        label: '地址',
        render: r => `<code>${escHtml(String(r.address).slice(0, 10))}…</code>`,
      },
      { key: 'metric_name', label: 'metric' },
      { key: 'period', label: 'period' },
      { key: 'self_value', label: 'self', render: r => fmtNum(num(r.self_value), 4), html: false },
      { key: 'third_party_value', label: '3rd', render: r => fmtNum(num(r.third_party_value), 4), html: false },
      { key: 'diff_pct', label: 'diff%', render: r => r.diff_pct == null ? '—' : fmtNum(num(r.diff_pct), 2) + '%', html: false },
      {
        key: 'audited_at',
        label: '时间',
        render: r => r.audited_at ? new Date(r.audited_at).toLocaleString() : '—',
        html: false,
      },
    ],
    rows,
  });
}

function num(v) {
  if (v == null) return null;
  const n = Number(v);
  return Number.isNaN(n) ? null : n;
}
function field(label, child) { return el('div', { class: 'field' }, [el('label', { text: label }), child]); }
function selectEl(val, options, onChange, labelFn) {
  const s = el('select', { onchange: e => onChange(e.target.value) });
  for (const o of options) {
    s.appendChild(el('option', {
      value: o,
      text: labelFn ? labelFn(o) : o,
      ...(String(o) === String(val) ? { selected: 'selected' } : {}),
    }));
  }
  return s;
}
