// pages/follows.js · 我的跟随 CRUD + 创建跟随。对应 docs/FRONTEND_DESIGN.md §6.9/§6.10。
import { el, skeleton, emptyState, traderLabel } from '../components/ui.js';
import { withShell } from '../components/nav.js';
import { listMyFollows, createFollow, updateFollow, deleteFollow } from '../lib/follow.js';
import { me } from '../lib/account.js';
import { listIdentities, listVenues } from '../lib/venue-hub.js';
import { openFollowFormModal } from '../components/follow-form.js';
import { copyHistorySection } from './copy-history.js';
import { toast } from '../store/toast.js';
import { navigate } from '../router.js';
import { t } from '../i18n/index.js';

const FREE_SLOTS = 3; // Free 档槽位上限（§6.12）；Pro+ 无限。

/// 我的跟随列表。§6.9（下方嵌入成交历史 §6.11）
export async function followsPage() {
  const c = el('div', { class: 'container' });

  const head = el('div', { class: 'page-head' }, [
    el('h1', { text: t('follows.pageTitle') }),
    el('div', { class: 'row', style: 'margin:0;align-items:center' }, [
      el('button', { class: 'primary', text: t('follows.create'), onclick: () => navigate('/follows/new') }),
    ]),
  ]);
  c.appendChild(head);

  const filterBar = el('div', { class: 'filter-bar' }, [
    field(t('follows.filter'), selectEl('filter', [
      ['all', t('follows.filterAll')],
      ['active', t('follows.filterActive')],
      ['paused', t('follows.filterPaused')],
    ], 'all')),
    field(t('follows.sort'), selectEl('sort', [
      ['created_desc', t('follows.sortCreatedDesc')],
      ['created_asc', t('follows.sortCreatedAsc')],
    ], 'created_desc')),
  ]);
  c.appendChild(filterBar);

  const list = el('div', { class: 'card' }, [skeleton(4)]);
  c.appendChild(list);

  // 成交历史紧跟跟随列表下方
  c.appendChild(copyHistorySection());

  let follows = [];
  let user = null;
  try {
    [follows, user] = await Promise.all([listMyFollows().catch(() => []), me().catch(() => null)]);
  } catch (e) {
    list.innerHTML = '';
    list.appendChild(el('p', { class: 'neg', text: t('follows.loadError', { message: e.message }) }));
    return withShell(c);
  }

  const render = () => renderList({ list, follows, user, filterBar });
  filterBar.querySelector('#filter').onchange = render;
  filterBar.querySelector('#sort').onchange = render;
  render();
  return withShell(c);
}

function renderList({ list, follows, user, filterBar }) {
  list.innerHTML = '';
  // 用 filterBar.querySelector 而非 document.getElementById：render() 在 root 挂载到
  // document 之前就被调用（router 在 await render() 完成后才 appendChild），此时
  // document.getElementById 找不到游离子树中的 #filter/#sort，会返回 null 并在访问
  // .value 时抛 "Cannot read properties of null (reading 'value')"。
  const filter = filterBar.querySelector('#filter').value;
  const sort = filterBar.querySelector('#sort').value;
  let rows = (follows || []).slice();
  if (filter === 'active') rows = rows.filter(f => f.active);
  else if (filter === 'paused') rows = rows.filter(f => !f.active);
  rows.sort((a, b) => sort === 'created_desc'
    ? new Date(b.created_at) - new Date(a.created_at)
    : new Date(a.created_at) - new Date(b.created_at));

  headCount(rows.length);

  if (rows.length === 0) {
    list.appendChild(emptyState({ icon: '🔗', text: t('follows.empty'), action: el('a', { href: '#/leaderboard', text: t('follows.emptyAction') }) }));
    return;
  }
  const wrap = el('div', { class: 'follow-list' });
  for (const f of rows) wrap.appendChild(followCard(f, () => followsPage().then(mount)));
  list.appendChild(wrap);
  // 槽位占用按 active 跟随计（暂停项不占槽），与当前筛选无关。
  const activeCount = (follows || []).filter(f => f.active).length;
  list.appendChild(slotBar(activeCount, user));
}

function headCount(n) {
  const head = document.querySelector('.page-head h1');
  if (head) head.textContent = t('follows.pageTitleCount', { count: n });
}

function followCard(f, reload) {
  const cfg = f.config || {};
  const sizing = cfg.sizing || {};
  const card = el('div', { class: 'follow-card' });
  const target = f.follow_identity_id
    ? t('follows.identityLabel', { idPrefix: String(f.follow_identity_id).slice(0, 8) })
    : traderLabel({ alias: f.follow_alias, address: f.follow_address, x_username: f.follow_x_username });
  const platform = f.follow_platform || '—';
  const statusCls = f.active ? 'active' : 'paused';
  const statusTxt = f.active ? '✅ active' : '⏸ paused';
  card.appendChild(el('div', { class: 'fc-head' }, [
    el('div', { class: 'fc-title' }, [el('a', { href: f.follow_platform && f.follow_address ? `#/traders/${encodeURIComponent(f.follow_platform)}/${encodeURIComponent(f.follow_address)}` : '#/follows', text: `${target} · ${platform}` })]),
    el('span', { class: 'badge ' + statusCls, text: statusTxt }),
  ]));
  card.appendChild(el('div', { class: 'fc-meta' }, [
    kv(t('follows.metaAddress'), f.follow_address ? f.follow_address.slice(0, 10) + '…' : '—'),
    kv(t('follows.metaChannel'), channelLabel(cfg.channel || f.channel)),
    kv(t('follows.metaExecute'), cfg.execute_venue || f.execute_venue || '—'),
  ]));
  card.appendChild(el('div', { class: 'fc-config', text: sizingSummary(cfg) }));
  card.appendChild(el('div', { class: 'fc-meta' }, [
    kv(t('follows.metaCreated'), f.created_at ? new Date(f.created_at).toLocaleDateString() : '—'),
    kv(t('follows.metaDailyMax'), cfg.daily_max_notional ? '$' + cfg.daily_max_notional : t('common.unlimited')),
    kv(t('follows.metaMaxOpen'), cfg.max_open_positions ? String(cfg.max_open_positions) : t('common.unlimited')),
  ]));
  const actions = el('div', { class: 'fc-actions' });
  actions.appendChild(el('button', { class: 'sm', text: f.active ? t('follows.pause') : t('follows.resume'), onclick: async () => {
    try { await updateFollow(f.id, { active: !f.active }); toast(f.active ? t('follows.toastPaused') : t('follows.toastResumed'), 'success'); reload(); }
    catch (e) { toast(e.message, 'error'); }
  } }));
  actions.appendChild(el('button', { class: 'sm', text: t('follows.edit'), onclick: () => openFollowFormModal({ follow: f, onSaved: async (body) => {
    await updateFollow(f.id, body); toast(t('follows.toastUpdated'), 'success'); reload();
  } }) }));
  actions.appendChild(el('button', { class: 'sm', text: t('follows.copyId'), onclick: () => { navigator.clipboard?.writeText(f.id); toast(t('follows.toastIdCopied'), 'success'); } }));
  actions.appendChild(el('div', { class: 'spacer' }));
  actions.appendChild(el('button', { class: 'sm danger', text: t('follows.delete'), onclick: async () => {
    if (!confirm(t('follows.confirmDelete'))) return;
    try { await deleteFollow(f.id); toast(t('follows.toastDeleted'), 'success'); reload(); }
    catch (e) { toast(e.message, 'error'); }
  } }));
  card.appendChild(actions);
  return card;
}

function slotBar(used, user) {
  const isPro = user?.tier && user.tier !== 'free';
  const limit = isPro ? Infinity : FREE_SLOTS;
  const bar = el('div', { class: 'slot-bar' });
  if (isPro) {
    bar.appendChild(el('span', { class: 'slot-label', text: t('follows.slotsPro', { used }) }));
    bar.appendChild(el('div', { class: 'spacer' }));
    bar.appendChild(el('span', { class: 'badge active', text: t('follows.slotsProBadge') }));
    return bar;
  }
  const full = used >= limit;
  bar.appendChild(el('span', { class: 'slot-label', text: t('follows.slotsFree', { used, limit }) }));
  const track = el('div', { class: 'slot-track' });
  track.appendChild(el('div', { class: 'slot-fill' + (full ? ' full' : ''), style: `width:${Math.min(100, used / limit * 100)}%` }));
  bar.appendChild(track);
  bar.appendChild(el('button', { class: 'sm primary', text: t('follows.upgradePro'), onclick: () => navigate('/settings/subscription') }));
  return bar;
}

function sizingSummary(cfg) {
  const s = cfg.sizing || {};
  if (s.mode === 'fixed') return `sizing: fixed $${s.value?.amount ?? '?'}`;
  if (s.mode === 'proportional') return `sizing: proportional ${(s.value?.ratio ?? '?') * 100}%`;
  if (s.mode === 'percent_of_balance') return `sizing: %ofBalance ${(s.value?.pct ?? '?') * 100}%`;
  return 'sizing: —';
}
function channelLabel(ch) { return ch === 'tg' ? t('follows.channelTg') : ch === 'daemon' ? t('follows.channelDaemon') : (ch || '—'); }
function kv(k, v) { return el('span', { class: 'kv' }, [el('span', {}, k + ': '), el('b', { text: String(v) })]); }

/// 创建跟随表单。§6.10
export async function newFollowPage({ params }) {
  const q = parseHashQuery();
  const c = el('div', { class: 'container narrow' });
  c.appendChild(el('h1', { text: t('follows.newTitle') }));

  // 跟随对象：单 Venue 交易者 / 跨 Venue 身份
  // 平台改为下拉：从 listVenues() 拉取已接入 Venue，失败时回退到静态列表。
  const PLATFORM_FALLBACK = [['polymarket', 'polymarket'], ['zeitgeist', 'zeitgeist'], ['azuro', 'azuro']];
  const platformSel = el('select', { id: 'platform' });
  platformSel.appendChild(el('option', { value: '', text: t('common.loading') }));
  const fillPlatform = (opts, preferred) => {
    platformSel.innerHTML = '';
    const cur = preferred || (opts[0] && opts[0][0]) || '';
    for (const [v, l] of opts) platformSel.appendChild(el('option', { value: v, text: l, ...(v === cur ? { selected: 'selected' } : {}) }));
  };
  listVenues().then(vs => {
    fillPlatform(vs && vs.length ? vs.map(v => [v.platform, v.display_name || v.platform]) : PLATFORM_FALLBACK, q.platform);
  }).catch(() => fillPlatform(PLATFORM_FALLBACK, q.platform || 'polymarket'));

  const tabs = el('div', { class: 'radio-tabs' }, [
    el('label', {}, [el('input', { type: 'radio', name: 'target_type', value: 'trader', checked: 'checked' }), t('follows.targetTrader')]),
    el('label', {}, [el('input', { type: 'radio', name: 'target_type', value: 'identity' }), t('follows.targetIdentity')]),
  ]);
  c.appendChild(tabs);

  const traderPanel = el('div', {}, [
    field(t('follows.platform'), platformSel),
    field(t('follows.address'), el('input', { id: 'address', value: q.address || '', placeholder: '0x…' })),
  ]);
  const identitySel = el('select', { id: 'identity_id' });
  identitySel.appendChild(el('option', { value: '', text: t('common.loading') }));
  const identityHint = el('p', { class: 'muted', text: t('follows.identityHint') });
  const identityPanel = el('div', {}, [
    field(t('follows.identity'), identitySel),
    identityHint,
  ]);
  c.appendChild(traderPanel);
  c.appendChild(identityPanel);
  identityPanel.style.display = 'none';
  // 预填 ?identity_id= 时自动切到跨 Venue 身份
  if (q.identity_id) {
    document.querySelector('input[name=target_type][value=identity]').checked = true;
    traderPanel.style.display = 'none';
    identityPanel.style.display = '';
  }
  tabs.querySelectorAll('input[name=target_type]').forEach(r => r.onchange = () => {
    const isId = document.querySelector('input[name=target_type]:checked').value === 'identity';
    traderPanel.style.display = isId ? 'none' : '';
    identityPanel.style.display = isId ? '' : 'none';
  });
  // 加载已校对身份下拉
  listIdentities().then(ids => {
    identitySel.innerHTML = '';
    if (!ids || ids.length === 0) {
      identitySel.appendChild(el('option', { value: '', text: t('follows.noIdentities') }));
      identityHint.textContent = t('follows.noIdentitiesHint');
      return;
    }
    identitySel.appendChild(el('option', { value: '', text: t('follows.selectIdentity') }));
    for (const idn of ids) {
      const label = (idn.alias || t('follows.unnamed')) + ' · #' + String(idn.id).slice(0, 8);
      identitySel.appendChild(el('option', {
        value: idn.id,
        text: label,
        ...(q.identity_id === idn.id ? { selected: 'selected' } : {}),
      }));
    }
  }).catch(e => {
    identitySel.innerHTML = '';
    identitySel.appendChild(el('option', { value: '', text: t('follows.identityLoadFailed') }));
    identityHint.textContent = t('follows.identityLoadFailedHint', { message: e.message });
  });

  c.appendChild(el('h3', { text: t('follows.execConfig') }));
  const venueF = field(t('follows.executeVenue'), el('input', { id: 'venue', value: q.platform || 'polymarket' }));
  const channelF = field(t('follows.channel'), selectEl('channel', [['tg', t('follows.channelTgOpt')], ['daemon', t('follows.channelDaemonOpt')]], 'tg'));
  const sizingModeF = field('sizing mode', selectEl('sizing_mode', [['fixed', t('follows.sizingFixed')], ['proportional', t('follows.sizingProportional')]], 'fixed'));
  const sizingValF = field(t('followForm.sizingValue'), el('input', { id: 'sizing_value', type: 'number', step: '0.01', min: '0', value: '10' }));
  const sameVenueF = field('', el('label', {}, [el('input', { id: 'same_venue_only', type: 'checkbox', checked: 'checked' }), t('follows.sameVenueOnly')]));
  [venueF, channelF, sizingModeF, sizingValF, sameVenueF].forEach(n => c.appendChild(n));

  const adv = el('details', { class: 'advanced' });
  adv.appendChild(el('summary', { text: t('follows.advanced') }));
  adv.appendChild(field(t('follows.maxOrder'), el('input', { id: 'max_order', type: 'number', step: '0.1', min: '0', value: '0', placeholder: t('follows.placeholderUnlimited') })));
  adv.appendChild(field(t('follows.dailyMax'), el('input', { id: 'daily_max', type: 'number', step: '0.1', min: '0', value: '0', placeholder: t('follows.placeholderUnlimited') })));
  adv.appendChild(field(t('follows.maxOpen'), el('input', { id: 'max_open', type: 'number', step: '1', min: '0', value: '0', placeholder: t('follows.placeholderUnlimited') })));
  c.appendChild(adv);

  const errP = el('p', { class: 'error' });
  c.appendChild(errP);
  const btnRow = el('div', { class: 'row' }, [
    el('button', { class: 'primary', text: t('follows.submit'), onclick: submit }),
    el('button', { text: t('common.cancel'), onclick: () => navigate('/follows') }),
  ]);
  c.appendChild(btnRow);

  async function submit() {
    errP.textContent = '';
    const isId = document.querySelector('input[name=target_type]:checked').value === 'identity';
    const venue = val('venue') || val('platform');
    const channel = document.getElementById('channel').value;
    const mode = document.getElementById('sizing_mode').value;
    const sv = Number(document.getElementById('sizing_value').value || 0);
    if (!(sv > 0)) { errP.textContent = t('follows.errorSizing'); return; }
    const sizingValue = mode === 'fixed' ? { amount: sv } : mode === 'proportional' ? { ratio: sv } : { pct: sv };
    const config = {
      sizing: { mode, value: sizingValue },
      execute_venue: venue, channel,
      same_venue_only: document.getElementById('same_venue_only').checked,
      max_notional_per_order: numOr0('max_order'),
      daily_max_notional: numOr0('daily_max'),
      max_open_positions: intOr0('max_open'),
    };
    const body = { execute_venue: venue, channel, config };
    if (isId) {
      const id = document.getElementById('identity_id').value;
      if (!id) { errP.textContent = t('follows.errorIdentity'); return; }
      body.follow_identity_id = id;
    } else {
      body.follow_platform = val('platform');
      body.follow_address = val('address');
      if (!body.follow_platform || !body.follow_address) { errP.textContent = t('follows.errorPlatformAddress'); return; }
    }
    try {
      await createFollow(body);
      toast(t('follows.toastCreated'), 'success');
      navigate('/follows');
    } catch (e) { errP.textContent = e.message; }
  }
  return withShell(c);
}

function numOr0(id) { const v = Number(document.getElementById(id)?.value || ''); return Number.isFinite(v) && v > 0 ? v : 0; }
function intOr0(id) { const v = parseInt(document.getElementById(id)?.value || '', 10); return Number.isFinite(v) && v > 0 ? v : 0; }
function mount(node) { const app = document.getElementById('app'); app.innerHTML = ''; app.appendChild(node); }
function parseHashQuery() { const h = location.hash.slice(1); const i = h.indexOf('?'); return i < 0 ? {} : Object.fromEntries(new URLSearchParams(h.slice(i + 1))); }
function val(id) { return document.getElementById(id).value.trim(); }
function field(label, child) { const w = el('div', { class: 'field' }); if (label) w.appendChild(el('label', { text: label })); w.appendChild(child); return w; }
function selectEl(name, options, val) { const s = el('select', { id: name }); for (const [v, l] of options) s.appendChild(el('option', { value: v, text: l, ...(v === val ? { selected: 'selected' } : {}) })); return s; }
