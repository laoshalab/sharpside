// pages/trader.js · 交易者详情。对应 docs/FRONTEND_DESIGN.md §6.1。
import { el, statCard, tagChips, dataTable, skeleton, emptyState, pnlClass, fmtPct, fmtUSD, fmtNum, traderLabel } from '../components/ui.js';
import { withShell } from '../components/nav.js';
import { getTrader, getPerformance, getEquityCurve, getPositions, getTrades } from '../lib/venue-hub.js';
import { navigate } from '../router.js';
import { isLoggedIn } from '../store/auth.js';
import { toast } from '../store/toast.js';
import { connectWalletFlow } from '../lib/wallet-connect.js';
import { createWatchlist } from '../lib/watchlist.js';
import { t } from '../i18n/index.js';

const PERIOD_KEYS = ['1d', '1w', '1m', '1y', 'ytd', 'all'];
function periods() {
  return PERIOD_KEYS.map(key => ({ key, label: t(`common.period.${key}`) }));
}

const BOT_RULE_KEYS = {
  high_freq_symmetric: 'trader.ruleHighFreq',
  wash_trade: 'trader.ruleWash',
  round_trip_scalper: 'trader.ruleRoundTrip',
  taker_only_scalper: 'trader.ruleTakerOnly',
  size_concentration: 'trader.ruleSizeConc',
  high_churn_no_edge: 'trader.ruleHighChurn',
};

function botRuleLabel(rule) {
  const key = BOT_RULE_KEYS[rule];
  return key ? t(key) : (rule || t('trader.botUnknownRule'));
}

export async function traderPage({ params }) {
  const { platform, address } = params;
  const c = el('div', { class: 'container' });

  c.appendChild(skeleton(3));
  let trader, perf;
  try {
    [trader, perf] = await Promise.all([
      getTrader(platform, address),
      getPerformance(platform, address).catch(() => null),
    ]);
  } catch (e) {
    c.innerHTML = '';
    c.appendChild(el('div', { class: 'card' }, [el('h2', { text: t('trader.loadErrorTitle') }), el('p', { class: 'neg', text: e.message })]));
    return withShell(c);
  }

  c.innerHTML = '';
  // 头部：标签 + 外链 + 跟随/观察 同一行，小间距对齐
  const actions = el('div', { class: 'row trader-actions' }, [
    tagChips(trader.is_hot ? [t('trader.tagHot')] : []),
    tagChips(perf?.tags || trader.tags || []),
  ]);
  const head = el('div', { class: 'card' }, [
    el('h1', { text: traderLabel(trader) }),
    el('p', { class: 'muted', text: `${trader.platform} · ${trader.address}` }),
    actions,
  ]);
  c.appendChild(head);

  // 机器人检测面板：读 perf.tag_attrs.bot（perf worker 调 crates/botfilter 产出）。
  // 无 bot 字段或 is_bot=false 时显示「正常」；命中规则时展示 confidence + 命中规则 + evidence 下钻。
  const botInfo = perf?.tag_attrs?.bot;
  if (botInfo) {
    c.appendChild(botPanel(botInfo));
  }

  // 官方主页外链（如 Polymarket profile）
  const officialUrl = venueOfficialProfileUrl(platform, address);
  if (officialUrl) {
    actions.appendChild(el('button', {
      class: 'sm',
      text: platform === 'polymarket' ? 'Polymarket ↗' : t('trader.officialProfile'),
      title: t('trader.officialTitle'),
      onclick: () => window.open(officialUrl, '_blank', 'noopener,noreferrer'),
    }));
  }

  // 跟随 / 观察 按钮
  if (isLoggedIn()) {
    actions.appendChild(el('button', { class: 'sm primary', text: t('trader.follow'), onclick: () => navigate(`/follows/new?platform=${encodeURIComponent(platform)}&address=${encodeURIComponent(address)}`) }));
    actions.appendChild(el('button', { class: 'sm', text: t('trader.watch'), onclick: async () => {
      try {
        await createWatchlist({ watch_platform: platform, watch_address: address });
        toast(t('trader.watchAdded'), 'success');
      } catch (e) {
        // 409 = 已收藏，友好提示而非报错
        if (e.status === 409) toast(t('trader.watchExists'), 'info');
        else toast(e.message, 'error');
      }
    } }));
  } else {
    actions.appendChild(el('button', {
      class: 'sm primary',
      text: t('trader.connectToFollow'),
      onclick: async () => {
        try {
          await connectWalletFlow({
            redirect: `/follows/new?platform=${encodeURIComponent(platform)}&address=${encodeURIComponent(address)}`,
          });
        } catch (e) {
          toast(e.message || t('common.connectFailed'), 'error');
        }
      },
    }));
  }

  // 周期 tab + 绩效 KPI + 权益曲线（按选中周期切片）
  const PERIODS = periods();
  const perfList = perf?.performance || [];
  let activePeriod = perfList.find(p => p.period === '1m')?.period
    || perfList[0]?.period
    || '1m';

  // 周期 tab 条
  const tabBar = el('div', { class: 'row', style: 'gap:8px;flex-wrap:wrap;margin-bottom:12px' });
  c.appendChild(tabBar);
  const tabBtns = {};
  for (const p of PERIODS) {
    const btn = el('button', { text: p.label, class: p.key === activePeriod ? 'primary' : '', onclick: () => selectPeriod(p.key) });
    tabBtns[p.key] = btn;
    tabBar.appendChild(btn);
  }

  const kpiHost = el('div');
  c.appendChild(kpiHost);

  c.appendChild(el('h2', { text: t('trader.equityTitle') }));
  c.appendChild(el('p', { class: 'muted', text: t('trader.equitySub'), style: 'margin-top:-6px;margin-bottom:10px' }));
  const chartCard = el('div', { class: 'card' }, [skeleton(2)]);
  c.appendChild(chartCard);

  let fullCurve = null;
  try {
    // granularity=auto：近 30 天小时级 + 30 天前日级，平衡平滑度与规模。
    fullCurve = await getEquityCurve(platform, address, { granularity: 'auto' });
  } catch (e) {
    fullCurve = null;
  }

  function renderKPI(period) {
    kpiHost.innerHTML = '';
    // 优先取 OVERALL 行（official_pnl 仅写在该 category）；缺失再回落任意同周期行。
    const row = perfList.find(p => p.period === period && p.category === 'OVERALL')
      || perfList.find(p => p.period === period);
    if (!row) {
      kpiHost.appendChild(emptyState({ text: t('trader.kpiEmpty', { period }) }));
      return;
    }
    // 官方盈亏：排行榜优先；非榜地址用 /value 周期 delta 兜底；皆无则回落自算。
    const officialPnl = row.official_pnl != null ? Number(row.official_pnl) : null;
    const officialSrc = row.official_source || '';
    const freshness = officialAgeLabel(row.official_pnl_at);
    const officialSub = officialSrc === 'polymarket_value_delta'
      ? [t('trader.officialPnlSubDelta'), freshness].filter(Boolean).join(' · ')
      : [t('trader.officialPnlSubLb'), freshness].filter(Boolean).join(' · ');
    const pnlCard = officialPnl != null
      ? statCard({ label: t('trader.officialPnl'), value: fmtUSD(officialPnl), cls: pnlClass(officialPnl), sub: officialSub })
      : statCard({ label: t('trader.realizedPnl'), value: fmtUSD(row.realized_pnl), cls: pnlClass(row.realized_pnl), sub: t('trader.realizedNoOfficial') });
    kpiHost.appendChild(el('div', { class: 'kpi-grid kpi-grid--row' }, [
      statCard({ label: `ROI (${period})`, value: fmtPct(row.roi), cls: pnlClass(row.roi) }),
      statCard({ label: 'Sharpe', value: fmtNum(row.sharpe) }),
      statCard({ label: t('trader.winRate'), value: fmtPct(row.win_rate, 0) }),
      statCard({ label: t('trader.maxDrawdown'), value: fmtPct(row.max_drawdown), cls: 'neg' }),
      pnlCard,
      statCard({ label: t('trader.realizedSelf'), value: fmtUSD(row.realized_pnl), cls: pnlClass(row.realized_pnl), sub: t('trader.realizedSelfSub') }),
      statCard({ label: t('trader.totalVolume'), value: fmtUSD(row.total_volume, 0) }),
      statCard({ label: t('trader.openPositions'), value: fmtNum(row.open_positions, 0) }),
      statCard({ label: t('trader.positionCount'), value: fmtNum(row.position_count, 0) }),
    ]));
  }

  /** 官方盈亏新鲜度副标（基于 official_pnl_at）。 */
  function officialAgeLabel(iso) {
    if (!iso) return '';
    const ms = Date.now() - new Date(iso).getTime();
    if (!Number.isFinite(ms) || ms < 0) return '';
    const mins = Math.floor(ms / 60000);
    if (mins < 60) return t('trader.officialFreshMins', { n: Math.max(0, mins) });
    const hrs = Math.floor(mins / 60);
    if (hrs < 48) return t('trader.officialFreshHours', { n: hrs });
    return t('trader.officialFreshDays', { n: Math.floor(hrs / 24) });
  }

  function renderCurve(period) {
    chartCard.innerHTML = '';
    if (!fullCurve || fullCurve.length === 0) {
      chartCard.appendChild(emptyState({ text: t('trader.equityEmpty') }));
      return;
    }
    const sliced = sliceCurve(fullCurve, period);
    if (sliced.length === 0) {
      chartCard.appendChild(emptyState({ text: t('trader.equityEmptyPeriod', { period }) }));
      return;
    }
    chartCard.appendChild(equityChartSVG(sliced));
    chartCard.appendChild(el('p', { class: 'muted', text: t('trader.dataPoints', {
      count: sliced.length,
      start: fmtTs(sliced[0].ts),
      end: fmtTs(sliced[sliced.length - 1].ts),
    }) }));
  }

  function selectPeriod(period) {
    activePeriod = period;
    for (const p of PERIODS) tabBtns[p.key].className = p.key === period ? 'primary' : '';
    renderKPI(period);
    renderCurve(period);
  }

  selectPeriod(activePeriod);

  // 当前持仓
  c.appendChild(el('h2', { text: t('trader.positionsTitle') }));
  const posCard = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(posCard);
  try {
    const positions = await getPositions(platform, address);
    posCard.innerHTML = '';
    const open = (positions || []).filter(p => !p.is_closed);
    if (open.length === 0) {
      posCard.appendChild(emptyState({ text: t('trader.positionsEmpty') }));
    } else {
      posCard.appendChild(dataTable({
        columns: [
          { key: 'market_title', label: t('trader.colMarket'), render: r => renderMarketCell(platform, r) },
          { key: 'outcome', label: t('trader.colOutcome'), render: r => r.outcome ? escapeHtml(r.outcome) : '—' },
          { key: 'final_open_size', label: t('trader.colSize'), render: r => fmtNum(r.final_open_size, 4) },
          { key: 'avg_cost', label: t('trader.colAvgCost'), render: r => fmtNum(r.avg_cost, 4) },
          { key: 'opened_at', label: t('trader.colOpenedAt'), render: r => fmtTs(r.opened_at) },
        ],
        rows: open,
      }));
    }
  } catch (e) {
    posCard.innerHTML = '';
    posCard.appendChild(el('p', { class: 'neg', text: t('trader.positionsError', { message: e.message }) }));
  }

  // 近期成交
  c.appendChild(el('h2', { text: t('trader.tradesTitle') }));
  const tradesCard = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(tradesCard);
  try {
    const trades = await getTrades(platform, address, { limit: 20 });
    tradesCard.innerHTML = '';
    if (!trades || trades.length === 0) {
      tradesCard.appendChild(emptyState({ text: t('trader.tradesEmpty') }));
    } else {
      tradesCard.appendChild(dataTable({
        columns: [
          { key: 'ts', label: t('trader.colTime'), render: r => fmtDateTime(r.ts) },
          { key: 'side', label: t('trader.colSide'), render: r => `<span class="${r.side === 'BUY' ? 'pos' : 'neg'}">${escapeHtml(r.side || '—')}</span>` },
          { key: 'size', label: t('trader.colQty'), render: r => fmtNum(r.size, 2) },
          { key: 'price', label: t('trader.colPrice'), render: r => fmtNum(r.price, 4) },
          { key: 'token_id', label: 'Token', render: r => `<code>${escapeHtml(String(r.token_id || '').slice(0, 12))}…</code>` },
        ],
        rows: trades,
      }));
    }
  } catch (e) {
    tradesCard.innerHTML = '';
    tradesCard.appendChild(el('p', { class: 'neg', text: t('trader.tradesError', { message: e.message }) }));
  }

  return withShell(c);
}

/// 极简 SVG 权益曲线（无依赖）。输入 [{ts, equity, drawdown_pct}]。
/// 用 Catmull-Rom 样条转 cubic bezier 生成平滑曲线（点数越多越平滑）。
function equityChartSVG(curve) {
  const W = 800, H = 240, PAD = 32;
  const equities = curve.map(p => Number(p.equity));
  const min = Math.min(...equities), max = Math.max(...equities);
  const range = max - min || 1;
  const xStep = (W - 2 * PAD) / Math.max(1, curve.length - 1);
  const xy = equities.map((e, i) => {
    const x = PAD + i * xStep;
    const y = H - PAD - ((e - min) / range) * (H - 2 * PAD);
    return [x, y];
  });
  const zeroY = H - PAD - ((0 - min) / range) * (H - 2 * PAD);
  const baseline = min <= 0 && max >= 0 ? `<line x1="${PAD}" y1="${zeroY.toFixed(1)}" x2="${W - PAD}" y2="${zeroY.toFixed(1)}" stroke="var(--c-border)" stroke-dasharray="4 4"/>` : '';
  const path = catmullRomToBezier(xy);
  return el('div', { html: `<svg viewBox="0 0 ${W} ${H}" style="width:100%;height:auto"><path d="${path}" fill="none" stroke="var(--c-accent)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>${baseline}</svg>` });
}

/// Catmull-Rom 样条 → SVG cubic bezier path（过所有点，平滑）。
function catmullRomToBezier(pts) {
  if (pts.length === 0) return '';
  if (pts.length === 1) return `M ${pts[0][0]},${pts[0][1]}`;
  if (pts.length === 2) return `M ${pts[0][0]},${pts[0][1]} L ${pts[1][0]},${pts[1][1]}`;
  let d = `M ${pts[0][0].toFixed(1)},${pts[0][1].toFixed(1)}`;
  for (let i = 0; i < pts.length - 1; i++) {
    const p0 = pts[i - 1] || pts[i];
    const p1 = pts[i];
    const p2 = pts[i + 1];
    const p3 = pts[i + 2] || p2;
    const c1x = p1[0] + (p2[0] - p0[0]) / 6;
    const c1y = p1[1] + (p2[1] - p0[1]) / 6;
    const c2x = p2[0] - (p3[0] - p1[0]) / 6;
    const c2y = p2[1] - (p3[1] - p1[1]) / 6;
    d += ` C ${c1x.toFixed(1)},${c1y.toFixed(1)} ${c2x.toFixed(1)},${c2y.toFixed(1)} ${p2[0].toFixed(1)},${p2[1].toFixed(1)}`;
  }
  return d;
}

/// 按周期切片小时级曲线。以最后一个点为基准向前截。
/// `1d`/`1w`/`1m`/`1y` 取最近 N 天；`ytd` 取当年 1 月 1 日起；`all` 不截。
/// 容错：任一端点 `ts` 非法（null/空/不可解析）时回落到全曲线，避免 `Invalid time value` 导致整页崩溃。
function sliceCurve(curve, period) {
  if (!curve || curve.length === 0) return [];
  if (period === 'all') return curve;
  const end = new Date(curve[curve.length - 1].ts);
  if (Number.isNaN(end.getTime())) return curve;
  let start;
  if (period === 'ytd') {
    start = new Date(Date.UTC(end.getUTCFullYear(), 0, 1));
  } else {
    const days = period === '1d' ? 1 : period === '1w' ? 7 : period === '1m' ? 30 : period === '1y' ? 365 : null;
    if (!days) return curve;
    start = new Date(end.getTime() - days * 86400000);
  }
  const startMs = start.getTime();
  return curve.filter(p => {
    const tsVal = new Date(p.ts).getTime();
    return !Number.isNaN(tsVal) && tsVal >= startMs;
  });
}

/// 时间戳是否可被 `Date` 解析为有效时刻（排除 null/空串/NaN/非法字符串）。
function isValidTs(v) {
  if (v == null) return false;
  const tsVal = new Date(v).getTime();
  return !Number.isNaN(tsVal);
}

/// ISO/Unix 时间戳 → 本地短日期；非法值返回 `—`，绝不抛 `Invalid time value`。
function fmtTs(ts) {
  if (!isValidTs(ts)) return '—';
  try { return new Date(ts).toLocaleDateString(); } catch { return '—'; }
}

/// 时间戳 → 本地日期时间；非法值返回 `—`。
function fmtDateTime(ts) {
  if (!isValidTs(ts)) return '—';
  try { return new Date(ts).toLocaleString(); } catch { return '—'; }
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

/// Venue 官方交易者主页 URL。未知平台返回 null。
function venueOfficialProfileUrl(platform, address) {
  const addr = String(address || '').trim();
  if (!addr) return null;
  if (platform === 'polymarket') return `https://polymarket.com/profile/${encodeURIComponent(addr)}`;
  return null;
}

/// Venue 市场页 URL（优先 event slug）。未知平台 / 无 slug 返回 null。
function venueMarketUrl(platform, eventSlug, marketSlug) {
  const slug = String(eventSlug || marketSlug || '').trim();
  if (!slug) return null;
  if (platform === 'polymarket') return `https://polymarket.com/event/${encodeURIComponent(slug)}`;
  return null;
}

/// 持仓「市场」列：标题（可点外链）+ token_id tooltip；无标题时回退截断 token_id。
function renderMarketCell(platform, r) {
  const title = (r.market_title && String(r.market_title).trim())
    || (r.token_id ? `${String(r.token_id).slice(0, 12)}…` : '—');
  const tip = r.token_id || r.condition_id || title;
  const url = venueMarketUrl(platform, r.event_slug, r.market_slug);
  const text = escapeHtml(title);
  if (url) {
    return `<a href="${escapeHtml(url)}" target="_blank" rel="noopener noreferrer" title="${escapeHtml(tip)}">${text}</a>`;
  }
  return `<span title="${escapeHtml(tip)}">${text}</span>`;
}

// ── 机器人检测面板 ──
// 读 perf.tag_attrs.bot（perf worker 调 crates/botfilter 产出，写入 trader_tag.tag_attrs）。
// 结构：{ is_bot, confidence, hit_rules: [{ rule, confidence, evidence }] }。
// rule 序列化为 snake_case（与 bot:* 标签一致）：high_freq_symmetric / wash_trade / ...。
function botPanel(bot) {
  const isBot = !!bot.is_bot;
  const conf = Number(bot.confidence || 0);
  const hits = Array.isArray(bot.hit_rules) ? bot.hit_rules : [];
  const card = el('div', { class: 'card', style: isBot ? 'border-left:4px solid var(--c-down)' : '' });
  card.appendChild(el('h2', { text: t('trader.botTitle') }));

  // 状态行
  const status = isBot
    ? el('span', { class: 'chip', style: 'background:var(--c-down);color:#fff', text: 'BOT' })
    : el('span', { class: 'chip', style: 'background:var(--c-up);color:#fff', text: t('trader.botNormal') });
  const head = el('div', { class: 'row', style: 'align-items:center;gap:12px;flex-wrap:wrap' }, [
    status,
    el('span', { class: 'muted', text: t('trader.botConfidence', { percent: (conf * 100).toFixed(0) }) }),
    hits.length ? el('span', { class: 'muted', text: t('trader.botRulesHit', { count: hits.length }) }) : null,
  ]);
  card.appendChild(head);

  if (isBot) {
    card.appendChild(el('p', { class: 'neg', style: 'margin-top:8px', text: t('trader.botWarning') }));
  }

  // 命中规则 + evidence 下钻
  if (hits.length) {
    const list = el('div', { style: 'margin-top:12px;display:flex;flex-direction:column;gap:10px' });
    for (const h of hits) {
      const label = botRuleLabel(h.rule);
      const item = el('div', { style: 'border:1px solid var(--c-border);border-radius:6px;padding:8px 10px' });
      item.appendChild(el('div', { class: 'row', style: 'justify-content:space-between;align-items:center' }, [
        el('span', { text: label, style: 'font-weight:600' }),
        el('span', { class: 'muted', text: `conf ${(Number(h.confidence || 0) * 100).toFixed(0)}%` }),
      ]));
      // evidence：JSON 展开（key-value 紧凑展示）
      const ev = h.evidence;
      if (ev && typeof ev === 'object' && Object.keys(ev).length) {
        const evPre = el('pre', {
          text: JSON.stringify(ev, null, 2),
          style: 'margin:6px 0 0;font-size:11px;color:var(--c-muted);white-space:pre-wrap;word-break:break-all',
        });
        item.appendChild(evPre);
      }
      list.appendChild(item);
    }
    card.appendChild(list);
  } else if (!isBot) {
    card.appendChild(el('p', { class: 'muted', style: 'margin-top:8px', text: t('trader.botNoRules') }));
  }

  return card;
}
