// pages/portfolio.js · 投资组合。对应 docs/FRONTEND_DESIGN.md §6.3。
import { el, statCard, dataTable, skeleton, emptyState, pnlClass, fmtUSD, fmtPct, fmtNum } from '../components/ui.js';
import { withShell } from '../components/nav.js';
import { getPortfolio, listCopyExecutions } from '../lib/copier.js';
import { toast } from '../store/toast.js';
import { t } from '../i18n/index.js';

const PERIOD_KEYS = ['1d', '1w', '1m', '1y', 'ytd', 'all'];
function periods() {
  return PERIOD_KEYS.map(key => ({ key, label: t(`common.period.${key}`) }));
}
const STEP_LABELS = ['<1s', '1-2s', '2-3s', '3-5s', '>5s'];

export async function portfolioPage() {
  const q = parseHashQuery();
  const state = { period: q.period || '1m' };

  const c = el('div', { class: 'container' });
  c.appendChild(el('h1', { text: t('portfolio.pageTitle') }));

  // 周期 + 导出
  const bar = el('div', { class: 'row' });
  bar.appendChild(field(t('portfolio.periodLabel'), selectEl('period', periods(), state.period, v => reload({ period: v }))));
  bar.appendChild(el('div', { class: 'field' }, [el('label', { text: ' ' }), el('button', { text: t('common.exportCsv'), onclick: exportCSV })]));
  c.appendChild(bar);

  const kpiCard = el('div', { class: 'card' }, [skeleton(2)]);
  c.appendChild(kpiCard);
  const walletCard = el('div', { class: 'card' }, [skeleton(2)]);
  c.appendChild(walletCard);
  const chartCard = el('div', { class: 'card' }, [skeleton(2)]);
  c.appendChild(chartCard);
  const brkCard = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(brkCard);
  const posCard = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(posCard);
  const latCard = el('div', { class: 'card' }, [skeleton(2)]);
  c.appendChild(latCard);
  const execCard = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(execCard);

  let lastPortfolio = null;

  function reload(patch) {
    Object.assign(state, patch);
    const qs = new URLSearchParams({ period: state.period }).toString();
    location.hash = '#/portfolio?' + qs;
    render();
  }

  async function render() {
    kpiCard.innerHTML = ''; kpiCard.appendChild(skeleton(2));
    walletCard.innerHTML = ''; walletCard.appendChild(skeleton(2));
    try {
      const p = await getPortfolio({ period: state.period });
      lastPortfolio = p;
      kpiCard.innerHTML = '';
      kpiCard.appendChild(el('div', { class: 'kpi-grid' }, [
        statCard({ label: t('portfolio.kpiTotalPnl'), value: fmtUSD(p.kpi.total_pnl), cls: pnlClass(p.kpi.total_pnl) }),
        statCard({ label: t('portfolio.kpiTotalRoi'), value: fmtPct(p.kpi.total_roi), cls: pnlClass(p.kpi.total_roi) }),
        statCard({ label: t('portfolio.kpiOpenMv'), value: fmtUSD(p.kpi.open_market_value) }),
        statCard({ label: t('portfolio.kpiWinRate'), value: fmtPct(p.kpi.win_rate, 0) }),
        statCard({ label: t('portfolio.kpiTradeCount'), value: fmtNum(p.kpi.trade_count, 0) }),
        statCard({ label: t('portfolio.kpiUnrealized'), value: p.kpi.unrealized_pnl === 0 ? t('portfolio.needsMarketData') : fmtUSD(p.kpi.unrealized_pnl), sub: 'Phase 2' }),
      ]));

      // 钱包 + 可用资金（EOA / Deposit Wallet / pUSD 现金）。对应 §6.4 资产权/交易权口径。
      walletCard.innerHTML = '';
      walletCard.appendChild(renderWallet(p.wallet));

      // 权益曲线
      chartCard.innerHTML = '';
      chartCard.appendChild(el('h3', { text: t('portfolio.equityTitle') }));
      if (!p.equity_curve || p.equity_curve.length === 0) {
        chartCard.appendChild(emptyState({ text: t('portfolio.equityEmpty') }));
      } else {
        chartCard.appendChild(equityChartSVG(p.equity_curve));
      }

      // 分跟随 / 分 Venue
      brkCard.innerHTML = '';
      brkCard.appendChild(el('div', { class: 'row' }, [
        breakdownPanel(t('portfolio.perFollow'), p.per_follow.map(f => ({ label: f.follow_relation_id.slice(0, 8) + '…', pnl: f.pnl, share: f.share }))),
        breakdownPanel(t('portfolio.perVenue'), p.per_venue.map(v => ({ label: v.venue, pnl: v.pnl, share: v.share }))),
      ]));

      // 当前持仓明细（FIFO 重建后剩余 open lots，成本口径；无 mark price 故无未实现 PnL）
      posCard.innerHTML = '';
      posCard.appendChild(el('h3', { text: t('portfolio.positionsTitle') }));
      if (!p.positions || p.positions.length === 0) {
        posCard.appendChild(emptyState({ text: t('portfolio.positionsEmpty') }));
      } else {
        posCard.appendChild(dataTable({
          columns: [
            { key: 'venue', label: 'Venue' },
            { key: 'market_id', label: t('portfolio.colMarket'), render: r => `<span title="${escapeAttr(r.market_id)}">${escapeText(r.market_id).slice(0, 12)}…</span>` },
            { key: 'token_id', label: 'Token', render: r => `<span title="${escapeAttr(r.token_id)}">${escapeText(r.token_id).slice(0, 10)}…</span>` },
            { key: 'size', label: t('portfolio.colSize'), render: r => fmtNum(r.size, 2) },
            { key: 'avg_cost', label: t('portfolio.colAvgCost'), render: r => fmtNum(r.avg_cost, 4) },
            { key: 'cost_basis', label: t('portfolio.colCostBasis'), render: r => fmtUSD(r.cost_basis) },
            { key: 'opened_at', label: t('portfolio.colOpenedAt'), render: r => r.opened_at ? new Date(r.opened_at).toLocaleString() : '—' },
          ],
          rows: p.positions,
        }));
      }

      // 延迟分布
      latCard.innerHTML = '';
      latCard.appendChild(el('h3', { text: t('portfolio.latencyTitle') }));
      latCard.appendChild(el('p', { class: 'muted', text: t('portfolio.latencySummary', {
        median: (p.latency.median_ms / 1000).toFixed(2),
        p95: (p.latency.p95_ms / 1000).toFixed(2),
        block0HitRate: p.latency.block0_enabled ? fmtPct(p.latency.block0_hit_rate, 0) : t('portfolio.block0Disabled'),
      }) }));
      latCard.appendChild(latencyHistogram(p.latency.buckets));

      // 近期成交
      execCard.innerHTML = '';
      execCard.appendChild(el('h3', { text: t('portfolio.execTitle') }));
      if (!p.recent_executions || p.recent_executions.length === 0) {
        execCard.appendChild(emptyState({ text: t('portfolio.execEmpty') }));
      } else {
        execCard.appendChild(dataTable({
          columns: [
            { key: 'executed_at', label: t('portfolio.colTime'), render: r => new Date(r.executed_at).toLocaleString() },
            { key: 'venue', label: 'Venue' },
            { key: 'side', label: t('portfolio.colSide'), render: r => `<span class="${r.side === 'BUY' ? 'pos' : 'neg'}">${r.side}</span>` },
            { key: 'filled_size', label: t('portfolio.colSize'), render: r => fmtNum(r.filled_size, 2) },
            { key: 'filled_price', label: t('portfolio.colPrice'), render: r => fmtNum(r.filled_price, 4) },
            { key: 'fee', label: t('portfolio.colFee'), render: r => fmtUSD(r.fee, 4) },
          ],
          rows: p.recent_executions,
        }));
      }
    } catch (e) {
      kpiCard.innerHTML = '';
      kpiCard.appendChild(el('p', { class: 'neg', text: t('portfolio.loadError', { message: e.message }) }));
    }
  }

  async function exportCSV() {
    try {
      const all = await listCopyExecutions({ limit: 10000 });
      const rows = [['executed_at', 'venue', 'market_id', 'token_id', 'side', 'filled_size', 'filled_price', 'fee', 'tx_hash']];
      for (const e of all) {
        rows.push([e.executed_at, e.venue, e.market_id, e.token_id, e.side, e.filled_size, e.filled_price, e.fee, e.tx_hash || '']);
      }
      const csv = rows.map(r => r.map(cell => `"${String(cell).replace(/"/g, '""')}"`).join(',')).join('\n');
      const blob = new Blob([csv], { type: 'text/csv' });
      const url = URL.createObjectURL(blob);
      const a = el('a', { href: url, download: `sharpside-executions-${state.period}.csv` });
      document.body.appendChild(a); a.click(); a.remove();
      URL.revokeObjectURL(url);
      toast(t('portfolio.exportSuccess'), 'success');
    } catch (e) { toast(t('portfolio.exportError', { message: e.message }), 'error'); }
  }

  render();
  return withShell(c);
}

function breakdownPanel(title, items) {
  const wrap = el('div', { class: 'field', style: 'flex:1' });
  wrap.appendChild(el('div', { class: 'section-title', text: title }));
  if (items.length === 0) { wrap.appendChild(el('p', { class: 'muted', text: t('portfolio.breakdownEmpty') })); return wrap; }
  const list = el('div');
  for (const it of items) {
    list.appendChild(el('div', { style: 'display:flex;justify-content:space-between;padding:4px 0' }, [
      el('span', { text: it.label }),
      el('span', { class: pnlClass(it.pnl), text: `${fmtUSD(it.pnl)} (${fmtPct(it.share, 0)})` }),
    ]));
  }
  wrap.appendChild(list);
  return wrap;
}

/// 渲染钱包卡：资产权（Deposit Wallet）/ 交易权（owner EOA）+ pUSD 可用余额。
/// 对应 `docs/FRONTEND_DESIGN.md` §6.4 资产权/交易权双卡口径与 §6.3 可用资金补点。
/// w 为 null = 无 polymarket 凭证，引导前往凭证页预配。
function renderWallet(w) {
  if (!w) {
    return el('div', {}, [
      el('div', { class: 'section-title', text: t('portfolio.walletTitle') }),
      emptyState({ text: t('portfolio.noCredential'), action: el('a', { href: '#/settings/credentials', text: t('portfolio.gotoCredentials') }) }),
    ]);
  }
  const card = el('div', {});
  card.appendChild(el('div', { class: 'section-title', text: t('portfolio.walletTitle') }));
  // 资产权 / 交易权双栏
  card.appendChild(el('div', { class: 'row' }, [
    walletField(t('portfolio.depositWallet'), w.deposit_wallet_address, t('portfolio.depositHint')),
    walletField(t('portfolio.ownerEoa'), w.owner_address, t('portfolio.ownerHint')),
  ]));
  // 可用余额
  const bal = w.cash_balance == null
    ? el('span', { class: 'muted', text: w.balance_note || t('portfolio.balanceUnknown') })
    : el('strong', { text: fmtUSD(w.cash_balance) });
  card.appendChild(el('p', { style: 'margin-top:12px' }, [
    el('span', { class: 'muted', text: t('portfolio.cashBalance') }),
    el('br'),
    bal,
  ]));
  return card;
}

function walletField(label, addr, hint) {
  return el('div', { class: 'field', style: 'flex:1' }, [
    el('label', { text: label }),
    el('div', { style: 'font-family:monospace;font-size:13px;word-break:break-all' }, [
      el('span', { text: addr || '—', title: addr || '' }),
    ]),
    el('div', { class: 'muted', style: 'font-size:11px;margin-top:2px', text: hint }),
  ]);
}

function latencyHistogram(buckets) {
  const max = Math.max(1, ...buckets);
  const bars = el('div', { style: 'display:flex;align-items:flex-end;gap:8px;height:120px;margin-top:12px' });
  for (let i = 0; i < buckets.length; i++) {
    const h = (buckets[i] / max) * 100;
    bars.appendChild(el('div', { style: 'flex:1;text-align:center' }, [
      el('div', { style: `height:${h}%;background:var(--c-accent);border-radius:4px 4px 0 0;min-height:2px` }),
      el('div', { class: 'muted', style: 'font-size:11px;margin-top:4px', text: STEP_LABELS[i] }),
      el('div', { class: 'muted', style: 'font-size:11px', text: String(buckets[i]) }),
    ]));
  }
  return bars;
}

function equityChartSVG(curve) {
  const W = 800, H = 220, PAD = 32;
  const vals = curve.map(p => Number(p.equity));
  const min = Math.min(...vals, 0), max = Math.max(...vals, 1);
  const range = max - min || 1;
  const xStep = (W - 2 * PAD) / Math.max(1, curve.length - 1);
  const pts = vals.map((v, i) => {
    const x = PAD + i * xStep;
    const y = H - PAD - ((v - min) / range) * (H - 2 * PAD);
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  }).join(' ');
  return el('div', { html: `<svg viewBox="0 0 ${W} ${H}" style="width:100%;height:auto"><polyline points="${pts}" fill="none" stroke="var(--c-accent)" stroke-width="2"/></svg>` });
}

function parseHashQuery() {
  const h = location.hash.slice(1); const i = h.indexOf('?');
  return i < 0 ? {} : Object.fromEntries(new URLSearchParams(h.slice(i + 1)));
}
function escapeText(s) {
  return String(s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}
function escapeAttr(s) {
  return escapeText(s).replace(/`/g, '&#96;');
}
function field(label, child) { return el('div', { class: 'field' }, [el('label', { text: label }), child]); }
function selectEl(name, options, val, onChange) {
  const s = el('select', { onchange: e => onChange(e.target.value) });
  for (const o of options) {
    const v = typeof o === 'object' ? o.key : o;
    s.appendChild(el('option', { value: v, text: typeof o === 'object' ? o.label : o, ...(v === val ? { selected: 'selected' } : {}) }));
  }
  return s;
}
