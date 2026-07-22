// pages/dashboard.js · 仪表盘。对应 docs/FRONTEND_DESIGN.md §6.6。
import { el, statCard, dataTable, skeleton, emptyState, pnlClass, fmtUSD, fmtPct } from '../components/ui.js';
import { withShell } from '../components/nav.js';
import { getDashboard } from '../lib/bff.js';
import { getPortfolio, getWallet, listCopyExecutions, listRecentOrders } from '../lib/copier.js';
import { listMyFollows } from '../lib/follow.js';
import { t } from '../i18n/index.js';

export async function dashboardPage() {
  const c = el('div', { class: 'container' });
  c.appendChild(el('h1', { text: t('dashboard.pageTitle') }));

  // BFF 聚合（含 portfolio_kpi / available_venues / jurisdiction）
  const bffCard = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(bffCard);
  let bffPortfolioKpi = null;
  try {
    const d = await getDashboard();
    bffCard.innerHTML = '';
    bffCard.appendChild(el('h2', { text: t('dashboard.overview') }));
    const venues = Array.isArray(d.available_venues) && d.available_venues.length
      ? d.available_venues.join(', ')
      : '—';
    bffCard.appendChild(el('p', { class: 'muted', text: t('dashboard.jurisdictionVenues', { jurisdiction: d.jurisdiction || '—', venues }) }));
    bffCard.appendChild(el('div', { class: 'kpi-grid' }, [
      statCard({ label: t('dashboard.activeFollows'), value: d.active_follows ?? '—' }),
      statCard({ label: t('dashboard.watchlistCount'), value: d.watchlist_count ?? '—' }),
      statCard({ label: t('dashboard.totalCopyOrders'), value: d.total_copy_orders ?? '—' }),
      statCard({ label: t('dashboard.totalExecutions'), value: d.total_executions ?? '—' }),
      statCard({ label: t('dashboard.totalPnl'), value: fmtUSD(d.total_pnl), cls: pnlClass(d.total_pnl) }),
    ]));
    bffPortfolioKpi = d.portfolio_kpi && typeof d.portfolio_kpi === 'object' && !Array.isArray(d.portfolio_kpi)
      ? d.portfolio_kpi
      : null;
  } catch (e) {
    bffCard.innerHTML = '';
    bffCard.appendChild(el('p', { class: 'muted', text: t('dashboard.bffFallback', { message: e.message }) }));
  }

  // 钱包余额快捷卡（充值/提现入口）。降级容错：拉取失败不阻塞页面。
  const walletCard = el('div', { class: 'card' }, [skeleton(1)]);
  c.appendChild(walletCard);
  getWallet().then(w => {
    walletCard.innerHTML = '';
    const bal = w.cash_balance;
    walletCard.appendChild(el('div', { style: 'display:flex;justify-content:space-between;align-items:center;flex-wrap:wrap;gap:8px' }, [
      el('div', {}, [
        el('span', { class: 'muted', text: t('dashboard.walletBalance') }),
        el('strong', { text: bal == null ? '—' : fmtUSD(bal) }),
        w.balance_note ? el('span', { class: 'muted', style: 'margin-left:6px', text: '(' + w.balance_note + ')' }) : null,
      ]),
      el('div', {}, [
        el('a', { href: '#/wallet', class: 'sm primary', text: t('dashboard.gotoWallet') }),
      ]),
    ]));
  }).catch(() => { walletCard.innerHTML = ''; walletCard.appendChild(el('p', { class: 'muted', text: t('dashboard.walletUnavailable') })); });

  // 投资组合：优先用 BFF portfolio_kpi，缺则再打 copier
  c.appendChild(el('h2', {}, [el('span', { text: t('dashboard.portfolioTitle') }), ' ', el('a', { href: '#/portfolio', class: 'muted', text: t('common.viewMore') })]));
  const portCard = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(portCard);
  try {
    let kpi = bffPortfolioKpi;
    if (!kpi) {
      const p = await getPortfolio({ period: '1m' });
      kpi = p.kpi || p;
    }
    portCard.innerHTML = '';
    portCard.appendChild(el('div', { class: 'kpi-grid' }, [
      statCard({ label: t('dashboard.kpiTotalPnl'), value: fmtUSD(kpi.total_pnl), cls: pnlClass(kpi.total_pnl) }),
      statCard({ label: t('dashboard.kpiTotalRoi'), value: fmtPct(kpi.total_roi), cls: pnlClass(kpi.total_roi) }),
      statCard({ label: t('dashboard.kpiOpenMv'), value: fmtUSD(kpi.open_market_value) }),
      statCard({ label: t('dashboard.kpiWinRate'), value: fmtPct(kpi.win_rate, 0) }),
      statCard({ label: t('dashboard.kpiTradeCount'), value: kpi.trade_count ?? '—' }),
      statCard({ label: t('dashboard.kpiUnrealized'), value: kpi.unrealized_pnl === 0 ? t('dashboard.needsMarketData') : fmtUSD(kpi.unrealized_pnl), sub: 'Phase 2' }),
    ]));
  } catch (e) {
    portCard.innerHTML = '';
    portCard.appendChild(el('p', { class: 'muted', text: t('dashboard.portfolioError', { message: e.message }) }));
  }

  // 我的跟随（降级视图）
  c.appendChild(el('h2', { text: t('dashboard.followsTitle') }));
  const followCard = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(followCard);
  try {
    const follows = await listMyFollows();
    followCard.innerHTML = '';
    if (!follows || follows.length === 0) {
      followCard.appendChild(emptyState({ text: t('dashboard.followsEmpty') }));
    } else {
      followCard.appendChild(dataTable({
        columns: [
          { key: 'follow_address', label: t('dashboard.colTarget'), render: r => r.follow_address ? r.follow_address.slice(0, 10) + '…' : t('dashboard.identityFallback') },
          { key: 'execute_venue', label: 'Venue' },
          { key: 'active', label: t('dashboard.colStatus'), render: r => r.active ? `<span class="pos">${t('dashboard.statusActive')}</span>` : `<span class="muted">${t('dashboard.statusPaused')}</span>` },
        ],
        rows: follows,
      }));
    }
  } catch (e) {
    followCard.innerHTML = '';
    followCard.appendChild(el('p', { class: 'neg', text: t('dashboard.followsError', { message: e.message }) }));
  }

  // 近期成交（copier 补点，降级）
  c.appendChild(el('h2', {}, [el('span', { text: t('dashboard.execTitle') }), ' ', el('a', { href: '#/follows', class: 'muted', text: t('common.viewAll') })]));
  const execCard = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(execCard);
  try {
    const execs = await listCopyExecutions({ limit: 20 });
    execCard.innerHTML = '';
    if (!execs || execs.length === 0) {
      execCard.appendChild(emptyState({ text: t('dashboard.execEmpty') }));
    } else {
      execCard.appendChild(dataTable({
        columns: [
          { key: 'executed_at', label: t('dashboard.colTime'), render: r => r.executed_at ? new Date(r.executed_at).toLocaleString() : '—' },
          { key: 'side', label: t('dashboard.colSide') },
          { key: 'size', label: t('dashboard.colSize') },
          { key: 'price', label: t('dashboard.colPrice') },
          { key: 'status', label: t('dashboard.colStatus') },
        ],
        rows: execs,
      }));
    }
  } catch (e) {
    execCard.innerHTML = '';
    execCard.appendChild(el('p', { class: 'muted', text: t('dashboard.execError', { message: e.message }) }));
  }

  // 近期跟单指令（含失败/跳过原因）
  c.appendChild(el('h2', { text: t('dashboard.ordersTitle') }));
  const ordCard = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(ordCard);
  try {
    const orders = await listRecentOrders({ limit: 20 });
    ordCard.innerHTML = '';
    if (!orders || orders.length === 0) {
      ordCard.appendChild(emptyState({ text: t('dashboard.ordersEmpty') }));
    } else {
      ordCard.appendChild(dataTable({
        columns: [
          { key: 'enqueued_at', label: t('dashboard.colTime'), render: r => r.enqueued_at ? new Date(r.enqueued_at).toLocaleString() : '—' },
          { key: 'side', label: t('dashboard.colSide') },
          { key: 'size', label: t('dashboard.colSize') },
          { key: 'price', label: t('dashboard.colPrice'), render: r => r.price != null ? Number(r.price).toFixed(2) : '—' },
          { key: 'execute_venue', label: 'Venue' },
          { key: 'status', label: t('dashboard.colStatus'), render: r => {
              const s = r.status || '—';
              const cls = s === 'filled' ? 'pos' : (s === 'failed' || s === 'skipped') ? 'neg' : 'muted';
              const icon = s === 'filled' ? '✅' : s === 'failed' ? '❌' : s === 'skipped' ? '⏭' : s === 'dispatched' ? '📤' : '⏳';
              return `<span class="${cls}">${icon} ${s}</span>`;
            }
          },
          { key: 'skip_reason', label: t('dashboard.colSkipReason'), render: r => r.skip_reason ? `<span class="neg" title="${escapeAttr(r.skip_reason)}">${escapeText(r.skip_reason).slice(0, 40)}${r.skip_reason.length > 40 ? '…' : ''}</span>` : '—' },
        ],
        rows: orders,
      }));
    }
  } catch (e) {
    ordCard.innerHTML = '';
    ordCard.appendChild(el('p', { class: 'muted', text: t('dashboard.ordersError', { message: e.message }) }));
  }

  return withShell(c);
}

function escapeText(s) {
  return String(s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}
function escapeAttr(s) {
  return escapeText(s).replace(/`/g, '&#96;');
}
