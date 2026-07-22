// pages/leaderboard.js · 排行榜。对应 docs/FRONTEND_DESIGN.md §6.2。
import { el, dataTable, skeleton, emptyState, tagChips, traderLabel, pnlClass, platformIcon } from '../components/ui.js';
import { withShell } from '../components/nav.js';
import { listTraders, getSparklines } from '../lib/venue-hub.js';
import { navigate } from '../router.js';
import { isLoggedIn } from '../store/auth.js';
import { createWatchlist } from '../lib/watchlist.js';
import { toast } from '../store/toast.js';
import { t } from '../i18n/index.js';

const SORT_KEYS = ['roi', 'sharpe', 'win_rate', 'max_drawdown', 'realized_pnl', 'total_volume', 'updated_at'];
const PERIOD_KEYS = ['1d', '1w', '1m', '1y', 'ytd', 'all'];
// 站内分类：与 Polymarket Data API `/v1/leaderboard` category 枚举对齐
// （docs.polymarket.com OpenAPI）。OVERALL = 全部成交。
const CATEGORY_KEYS = [
  'OVERALL', 'POLITICS', 'SPORTS', 'ESPORTS', 'CRYPTO', 'CULTURE',
  'MENTIONS', 'WEATHER', 'ECONOMICS', 'TECH', 'FINANCE',
];

function sorts() {
  return SORT_KEYS.map(key => ({ key, label: t(`leaderboard.sort.${key}`) }));
}
function periods() {
  return PERIOD_KEYS.map(key => ({ key, label: t(`leaderboard.period.${key}`) }));
}
function categories() {
  return CATEGORY_KEYS.map(key => ({ key, label: t(`leaderboard.category.${key}`) }));
}

export async function leaderboardPage({ params }) {
  // 解析 query（hash 内 ?key=val 由 location.hash 提供）
  const q = parseHashQuery();
  const state = {
    platform: q.platform || '',
    period: q.period || '1m',
    category: q.category || 'OVERALL',
    sort: q.sort || 'roi',
    sort_desc: q.sort_desc !== 'false',
    q_text: q.q || '',
    hot_only: q.hot_only === 'true',
    verified_only: q.verified_only === 'true',
    // 隐藏机器人（默认 true）。toggle 勾选 → include_bots=false（排除 bot 标签）；取消 → include_bots=true。
    hide_bots: q.hide_bots !== 'false',
    // 仅本周期（多条件共同筛选，默认 true）：require_perf=true → 周期/分类参与 AND 过滤，
    // 剔除没有该周期/分类绩效行的交易者。取消 → 退回旧行为（周期/分类仅决定展示哪行绩效）。
    require_perf: q.require_perf !== 'false',
    limit: 50,
    offset: parseInt(q.offset || '0', 10),
  };

  const c = el('div', { class: 'container' });
  c.appendChild(el('h1', { text: t('leaderboard.title') }));

  // 过滤栏：分类 / 主控件 / 多条件共同筛选 分三行，避免 nowrap 横向滚动把勾选框挤出可视区。
  const filter = el('div', { class: 'card' });
  c.appendChild(filter);

  const catRow = el('div', { class: 'filter-bar filter-bar--categories' });
  filter.appendChild(catRow);
  catRow.appendChild(segmented(categories(), state.category, v => reload({ category: v })));

  const row = el('div', { class: 'filter-bar filter-bar--inline' });
  filter.appendChild(row);

  row.appendChild(field(inputEl('q', state.q_text, v => reload({ q: v }))));
  row.appendChild(field(selectEl('platform', [
    { key: '', label: t('leaderboard.allPlatforms') },
    { key: 'polymarket', label: 'polymarket' },
    { key: 'zeitgeist', label: 'zeitgeist' },
    { key: 'azuro', label: 'azuro' },
  ], state.platform, v => reload({ platform: v }))));
  row.appendChild(field(selectEl('sort', sorts(), state.sort, v => reload({ sort: v }))));
  row.appendChild(field(segmented(periods(), state.period, v => reload({ period: v }))));
  row.appendChild(field(toggle(t('leaderboard.sortDesc'), state.sort_desc, v => reload({ sort_desc: v }))));

  // 多条件共同筛选：独立可见行（可换行），全部条件 AND 组合。
  const andRow = el('div', { class: 'filter-bar filter-bar--and' });
  filter.appendChild(andRow);
  andRow.appendChild(el('span', { class: 'filter-and-label', text: t('leaderboard.andFilters') }));
  andRow.appendChild(toggle(t('leaderboard.hotOnly'), state.hot_only, v => reload({ hot_only: v })));
  andRow.appendChild(toggle(t('leaderboard.verifiedOnly'), state.verified_only, v => reload({ verified_only: v })));
  andRow.appendChild(toggle(t('leaderboard.hideBots'), state.hide_bots, v => reload({ hide_bots: v })));
  andRow.appendChild(toggle(t('leaderboard.requirePerf'), state.require_perf, v => reload({ require_perf: v })));
  const filterSummary = el('span', { class: 'filter-and-summary muted', text: '' });
  andRow.appendChild(filterSummary);

  // 分页夹在筛选卡与表格卡之间，右对齐；数字页可直达（含第 10 页）。
  const pager = el('div', { class: 'pagination pagination--between' });
  c.appendChild(pager);

  const tableWrap = el('div', { class: 'card' }, [skeleton(5)]);
  c.appendChild(tableWrap);

  function reload(patch) {
    // 筛选变更未显式带 offset 时回到第 1 页；翻页调用会传入 offset。
    if (!('offset' in patch)) patch = { ...patch, offset: 0 };
    Object.assign(state, patch);
    // 显式编码布尔值，避免 false 被跳过导致「取消勾选」无法持久化（原 bug）。
    const qs = new URLSearchParams();
    qs.set('period', state.period);
    qs.set('category', state.category);
    qs.set('sort', state.sort);
    qs.set('sort_desc', String(state.sort_desc));
    qs.set('hot_only', String(state.hot_only));
    qs.set('verified_only', String(state.verified_only));
    qs.set('hide_bots', String(state.hide_bots));
    qs.set('require_perf', String(state.require_perf));
    if (state.platform) qs.set('platform', state.platform);
    if (state.q_text) qs.set('q', state.q_text);
    if (state.offset) qs.set('offset', String(state.offset));
    location.hash = '#/leaderboard?' + qs.toString();
    renderTable();
  }

  async function renderTable() {
    tableWrap.innerHTML = '';
    tableWrap.appendChild(skeleton(5));
    try {
      const resp = await listTraders({
        platform: state.platform || undefined,
        period: state.period,
        category: state.category || 'OVERALL',
        sort: state.sort,
        sort_desc: state.sort_desc,
        q: state.q_text || undefined,
        hot_only: state.hot_only,
        verified_only: state.verified_only,
        include_bots: !state.hide_bots,
        require_perf: state.require_perf,
        limit: state.limit,
        offset: state.offset,
        with_count: true,
      });
      // with_count=true → {rows, total}；兜底纯数组（旧消费者或降级）。
      const rows = Array.isArray(resp) ? resp : (resp && resp.rows) || [];
      const total = Array.isArray(resp) ? null : (resp && resp.total != null ? resp.total : null);
      filterSummary.textContent = formatFilterSummary(state, total);
      tableWrap.innerHTML = '';
      if (!rows || rows.length === 0) {
        tableWrap.appendChild(emptyState({
          icon: '🔍',
          text: state.require_perf
            ? t('leaderboard.emptyStrict')
            : t('leaderboard.empty'),
        }));
      } else {
        // 补 #序号（文档 §6.2 行规格要求）。
        rows.forEach((r, i) => { r._rank = state.offset + i + 1; });
        // Bot 列展示 botfilter 置信度百分比；仅在取消「隐藏机器人」时展示（勾选时结果集无 bot，列无意义）。
        const columns = [
          { key: '_rank', label: t('leaderboard.colRank'), class: 'col-rank', render: r => `<span class="muted">${r._rank}</span>` },
          { key: 'name', label: t('leaderboard.colTrader'), class: 'col-trader', render: r => traderCell(r) },
          { key: 'sparkline', label: t('leaderboard.colSpark'), class: 'col-spark', render: r => `<div class="spark" data-p="${encodeURIComponent(r.platform)}" data-a="${encodeURIComponent(r.address)}"><span class="muted" style="font-size:11px">···</span></div>` },
          { key: 'roi', label: t('leaderboard.colRoi'), render: r => `<span class="${pnlClass(r.roi)}">${r.roi == null ? '—' : (Number(r.roi) * 100).toFixed(1) + '%'}</span>` },
          { key: 'sharpe', label: t('leaderboard.colSharpe'), render: r => r.sharpe == null ? '—' : Number(r.sharpe).toFixed(2) },
          { key: 'win_rate', label: t('leaderboard.colWinRate'), render: r => r.win_rate == null ? '—' : (Number(r.win_rate) * 100).toFixed(0) + '%' },
          { key: 'max_drawdown', label: t('leaderboard.colDrawdown'), render: r => r.max_drawdown == null ? '—' : (Number(r.max_drawdown) * 100).toFixed(1) + '%' },
          { key: 'realized_pnl', label: t('leaderboard.colPnl'), render: r => `<span class="${pnlClass(r.realized_pnl)}">${r.realized_pnl == null ? '—' : '$' + Number(r.realized_pnl).toLocaleString('en-US', { maximumFractionDigits: 0 })}</span>` },
          { key: 'platform', label: t('leaderboard.colPlatform'), class: 'col-platform', render: r => platformIcon(r.platform) },
          { key: 'tags', label: t('leaderboard.colTags'), render: r => {
            const tags = (r.tags || []).filter(tag => tag !== 'bot' && !String(tag).startsWith('bot:'));
            return tags.length
              ? tags.slice(0, 2).map(tag => `<span class="chip">${escapeHtml(tag)}</span>`).join('') + (tags.length > 2 ? `<span class="muted"> +${tags.length - 2}</span>` : '')
              : '<span class="neutral">—</span>';
          } },
        ];
        if (!state.hide_bots) {
          columns.push({
            key: 'bot',
            label: t('leaderboard.colBot'),
            class: 'col-bot',
            render: r => {
              if (r.bot_confidence == null) return '<span class="neutral">—</span>';
              const pct = (Number(r.bot_confidence) * 100).toFixed(0) + '%';
              const isBot = r.tags && r.tags.includes('bot');
              return isBot
                ? `<span class="chip" style="background:var(--c-down);color:#fff" title="${t('leaderboard.botMarked')}">${pct}</span>`
                : `<span class="muted" title="${t('leaderboard.botConfidence')}">${pct}</span>`;
            },
          });
        }
        columns.push({
          key: 'watch',
          label: t('leaderboard.colWatch'),
          class: 'col-watch',
          render: r => isLoggedIn()
            ? `<button type="button" class="sm btn-watch" data-p="${encodeURIComponent(r.platform)}" data-a="${encodeURIComponent(r.address)}" title="${t('leaderboard.watchTitle')}">👁</button>`
            : '<span class="neutral">—</span>',
        });
        tableWrap.appendChild(dataTable({
          className: 'leaderboard-table',
          columns,
          rows,
          onRowClick: r => navigate(`/traders/${encodeURIComponent(r.platform)}/${encodeURIComponent(r.address)}`),
        }));
        // 渲染后逐行拉 equity 曲线（granularity 按 period 选），注入 sparkline SVG。
        loadSparklines(rows, state.period);
        // 绑定「观察」按钮：阻止冒泡到行点击导航。
        wireWatchButtons(rows);
      }
      renderPager(pager, {
        offset: state.offset,
        limit: state.limit,
        len: rows ? rows.length : 0,
        total,
        onPage: page => reload({ offset: (page - 1) * state.limit }),
      });
    } catch (e) {
      tableWrap.innerHTML = '';
      tableWrap.appendChild(el('p', { class: 'neg', text: t('common.loadFailedColon', { msg: e.message }) }));
      pager.innerHTML = '';
    }
  }

  renderTable();
  return withShell(c);
}

function parseHashQuery() {
  const h = location.hash.slice(1);
  const i = h.indexOf('?');
  if (i < 0) return {};
  return Object.fromEntries(new URLSearchParams(h.slice(i + 1)));
}

// 分页条：← 数字页 … → + 范围说明 + 填页跳转。有 total 时渲染可点击页码（近首页展开到 10）。
function renderPager(pager, { offset, limit, len, total, onPage }) {
  pager.innerHTML = '';
  const start = len === 0 ? 0 : offset + 1;
  const end = offset + len;
  const cur = Math.floor(offset / limit) + 1;
  const totalPages = total != null ? Math.max(1, Math.ceil(total / limit)) : null;
  const isLast = totalPages != null ? cur >= totalPages : len < limit;

  pager.appendChild(el('span', {
    class: 'pg-info',
    text: total != null
      ? t('leaderboard.showing', { start, end, total })
      : t('leaderboard.showingPartial', { start, end }) + (isLast ? t('leaderboard.lastPage') : ''),
  }));

  const go = page => {
    const n = Math.floor(Number(page));
    if (!Number.isFinite(n)) return;
    let target = n;
    if (totalPages != null) target = Math.min(Math.max(1, n), totalPages);
    else if (n < 1) return;
    if (target === cur) return;
    onPage(target);
  };

  pager.appendChild(el('button', {
    class: 'sm',
    text: '←',
    disabled: cur <= 1 ? 'disabled' : null,
    onclick: () => go(cur - 1),
  }));

  if (totalPages != null) {
    for (const item of pageItems(cur, totalPages)) {
      if (item === '…') {
        pager.appendChild(el('span', { class: 'pg-ellipsis', text: '…' }));
      } else {
        pager.appendChild(el('button', {
          class: 'sm' + (item === cur ? ' active' : ''),
          text: String(item),
          onclick: () => go(item),
        }));
      }
    }
  } else {
    pager.appendChild(el('button', { class: 'sm active', text: String(cur) }));
  }

  pager.appendChild(el('button', {
    class: 'sm',
    text: '→',
    disabled: isLast ? 'disabled' : null,
    onclick: () => go(cur + 1),
  }));

  // 填页跳转：回车或点「跳转」；越界自动夹到 1…totalPages。
  const jump = el('form', { class: 'pg-jump', onsubmit: e => {
    e.preventDefault();
    go(input.value);
  } });
  jump.appendChild(el('span', { class: 'pg-jump-label', text: t('leaderboard.jumpTo') }));
  const input = el('input', {
    type: 'number',
    class: 'pg-jump-input',
    min: '1',
    ...(totalPages != null ? { max: String(totalPages) } : {}),
    value: String(cur),
    inputmode: 'numeric',
    'aria-label': t('leaderboard.pageLabel'),
  });
  jump.appendChild(input);
  if (totalPages != null) {
    jump.appendChild(el('span', { class: 'pg-jump-total', text: `/ ${totalPages}` }));
  } else {
    jump.appendChild(el('span', { class: 'pg-jump-label', text: t('leaderboard.page') }));
  }
  jump.appendChild(el('button', { class: 'sm', type: 'submit', text: t('leaderboard.jump') }));
  pager.appendChild(jump);
}

// 页码窗口：≤10 页全展示；靠前时展开 1…10 便于直达第 10 页；中间/靠后用省略号收拢。
function pageItems(cur, total) {
  if (total <= 10) return Array.from({ length: total }, (_, i) => i + 1);
  const set = new Set([1, total]);
  let lo, hi;
  if (cur <= 6) {
    lo = 1;
    hi = 10;
  } else if (cur >= total - 5) {
    lo = total - 9;
    hi = total;
  } else {
    lo = cur - 2;
    hi = cur + 2;
  }
  for (let i = lo; i <= hi; i++) {
    if (i >= 1 && i <= total) set.add(i);
  }
  const sorted = [...set].sort((a, b) => a - b);
  const out = [];
  let prev = 0;
  for (const p of sorted) {
    if (prev && p - prev > 1) out.push('…');
    out.push(p);
    prev = p;
  }
  return out;
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

// 共同筛选摘要：把当前 AND 条件 + 命中总数摊开，便于确认多条件是否生效。
function formatFilterSummary(state, total) {
  const periodLabel = (periods().find(p => p.key === state.period) || {}).label || state.period;
  const catLabel = (categories().find(c => c.key === state.category) || {}).label || state.category;
  const parts = [
    periodLabel,
    catLabel,
    state.platform || t('leaderboard.allPlatformsShort'),
  ];
  if (state.q_text) parts.push(t('leaderboard.searchTerm', { q: state.q_text }));
  if (state.hot_only) parts.push(t('leaderboard.hotOnly'));
  if (state.verified_only) parts.push(t('leaderboard.verifiedOnly'));
  if (state.hide_bots) parts.push(t('leaderboard.hideBots'));
  if (state.require_perf) parts.push(t('leaderboard.requirePerf'));
  const count = total != null ? t('leaderboard.peopleCount', { n: Number(total).toLocaleString('en-US') }) : '';
  return parts.join(' · ') + count;
}

// 小工具
function field(child) { return el('div', { class: 'field' }, [child]); }
function selectEl(name, options, val, onChange) {
  const s = el('select', { onchange: e => onChange(e.target.value) });
  for (const o of options) {
    const v = typeof o === 'object' ? o.key : o;
    s.appendChild(el('option', { value: v, text: typeof o === 'object' ? o.label : o, ...(v === val ? { selected: 'selected' } : {}) }));
  }
  return s;
}
function inputEl(name, val, onChange) {
  let timer;
  const i = el('input', { type: 'text', value: val || '', placeholder: t('leaderboard.searchPlaceholder'), oninput: e => {
    // 300ms 防抖（文档 §6.2），避免每次按键都重发请求 + 改 hash。
    clearTimeout(timer);
    timer = setTimeout(() => onChange(e.target.value), 300);
  } });
  return i;
}
function toggle(label, checked, onChange) {
  const wrap = el('label', { class: 'filter-toggle' });
  const cb = el('input', { type: 'checkbox', ...(checked ? { checked: 'checked' } : {}) });
  cb.addEventListener('change', () => onChange(cb.checked));
  wrap.appendChild(cb);
  wrap.appendChild(document.createTextNode(label));
  return wrap;
}

// 分段按钮组（周期切换用）
function segmented(options, active, onChange) {
  const wrap = el('div', { class: 'seg-group' });
  for (const o of options) {
    const v = typeof o === 'object' ? o.key : o;
    const label = typeof o === 'object' ? o.label : o;
    wrap.appendChild(el('button', {
      class: 'sm' + (v === active ? ' primary' : ''),
      text: label,
      onclick: () => onChange(v),
    }));
  }
  return wrap;
}

// trader-cell：头像 + alias/地址缩略 + 平台 badge + 🔥热钥 + ✓验证（文档 §6.2 行规格）。
// 头像缺失时用 alias/地址首两字符兜底。
// 注意：DB 里不少「alias」实际是 address-时间戳（无空格长串）。traderLabel 会优先返回 alias，
// 旧逻辑只对「纯 address 兜底」做 slice(0,8)+…，所以长 alias 会撑爆列宽并换行。
// 这里统一中间截断；完整原文放 title，悬停可看。
function midTruncate(s, max = 14) {
  s = String(s == null ? '' : s);
  if (s.length <= max) return s;
  const head = Math.ceil((max - 1) / 2);
  const tail = Math.floor((max - 1) / 2);
  return s.slice(0, head) + '…' + s.slice(-tail);
}
function traderCell(r) {
  const href = `#/traders/${encodeURIComponent(r.platform)}/${encodeURIComponent(r.address)}`;
  const full = traderLabel(r);
  const name = escapeHtml(midTruncate(full));
  const initials = escapeHtml((r.alias || r.user_name || r.address || '?').slice(0, 2).toUpperCase());
  const avatar = r.profile_image
    ? `<img class="trader-avatar" src="${escapeHtml(r.profile_image)}" alt="" onerror="this.style.visibility='hidden'">`
    : `<span class="trader-avatar trader-avatar-fallback">${initials}</span>`;
  const marks = [
    r.is_hot ? `<span title="${t('leaderboard.hotMark')}">🔥</span>` : '',
    r.verified_badge ? `<span title="${t('leaderboard.verifiedMark')}" style="color:var(--c-up)">✓</span>` : '',
  ].join(' ');
  return `<span class="trader-cell">${avatar}<a class="trader-name" href="${href}" title="${escapeHtml(full)}">${name}</a><span class="muted trader-marks">${marks}</span></span>`;
}

// ── sparkline：单次批量拉 equity 曲线并注入 SVG（方案 B，消除 N+1）──
// 后端 `GET /traders/sparklines?ids=...&period=...` 已按 period 截断 + 降采样到 ≤40 点。
// 颜色按净趋势：末值≥首值 → 涨色，否则 → 跌色。auto-scale 只看形状。
// 样式：mini area chart，折线下方带垂直线性渐变填充（顶部 ~25% 不透明 → 底部透明）。
//   涨色用霓虹薄荷绿 #00f2ad，跌色用标准红 #ff4d4f。无坐标轴/网格/数据点。
let _sparkGradSeq = 0;
function sparklineSvg(values, W = 80, H = 28) {
  if (!values || values.length < 2) return '';
  let min = Infinity, max = -Infinity;
  for (const v of values) { if (v < min) min = v; if (v > max) max = v; }
  const up = values[values.length - 1] >= values[0];
  const color = up ? '#00f2ad' : '#ff4d4f';
  const span = max - min || 1;
  const n = values.length;
  // 折线路径 + 闭合到底边的填充路径（共享点序列，避免重复计算）。
  let line = '';
  let pts = [];
  for (let i = 0; i < n; i++) {
    const x = (i / (n - 1) * W).toFixed(1);
    const y = (H - (values[i] - min) / span * H).toFixed(1);
    pts.push([x, y]);
    line += (i === 0 ? 'M' : 'L') + x + ' ' + y + ' ';
  }
  const area = line + `L${pts[pts.length - 1][0]} ${H} L${pts[0][0]} ${H} Z`;
  const gid = `spark-grad-${_sparkGradSeq++}`;
  // viewBox 固定坐标系；CSS 将 svg 限宽到列内，避免溢出到「平台」列。
  return `<svg width="${W}" height="${H}" viewBox="0 0 ${W} ${H}" style="display:block;margin:0 auto;max-width:100%" preserveAspectRatio="none">
    <defs>
      <linearGradient id="${gid}" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" stop-color="${color}" stop-opacity="0.28"/>
        <stop offset="100%" stop-color="${color}" stop-opacity="0"/>
      </linearGradient>
    </defs>
    <path d="${area}" fill="url(#${gid})" stroke="none"/>
    <path d="${line}" fill="none" stroke="${color}" stroke-width="1.25" stroke-linejoin="round" stroke-linecap="round"/>
  </svg>`;
}
async function loadSparklines(rows, period) {
  const ids = rows.map(r => `${r.platform}:${r.address}`);
  try {
    const map = await getSparklines(ids, period);
    if (!map || typeof map !== 'object') return;
    for (const r of rows) {
      const pts = map[`${r.platform}:${r.address}`];
      if (!Array.isArray(pts) || pts.length < 2) continue;
      const svg = sparklineSvg(pts.map(p => Number(p.equity)));
      if (!svg) continue;
      const cell = document.querySelector(`.spark[data-p="${encodeURIComponent(r.platform)}"][data-a="${encodeURIComponent(r.address)}"]`);
      if (cell) cell.innerHTML = svg;
    }
  } catch { /* 批量失败静默，保留占位 */ }
}

// 绑定排行榜「观察」按钮：阻止冒泡到行点击导航，调用 createWatchlist。
function wireWatchButtons(_rows) {
  document.querySelectorAll('.btn-watch').forEach(btn => {
    btn.addEventListener('click', async (ev) => {
      ev.preventDefault();
      ev.stopPropagation();
      const p = decodeURIComponent(btn.getAttribute('data-p') || '');
      const a = decodeURIComponent(btn.getAttribute('data-a') || '');
      if (!p || !a) return;
      btn.disabled = true;
      try {
        await createWatchlist({ watch_platform: p, watch_address: a });
        toast(t('leaderboard.watchAdded'), 'success');
      } catch (e) {
        if (e.status === 409) toast(t('leaderboard.watchExists'), 'info');
        else toast(e.message || t('leaderboard.watchFailed'), 'error');
      } finally {
        btn.disabled = false;
      }
    });
  });
}
