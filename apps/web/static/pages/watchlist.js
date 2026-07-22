// pages/watchlist.js · 我的观察名单。对应 Watchlist 功能规划。
// 顶部 ImportBox 可导入地址并加入观察；下列出已收藏的 trader / identity，
// 支持删除与「升级为跟随」（消费式升级，见 components/upgrade-form.js）。
import { el, skeleton, emptyState, traderLabel, fmtPct, fmtUSD, pnlClass, tagChips } from '../components/ui.js';
import { withShell } from '../components/nav.js';
import { listMyWatchlists, deleteWatchlist, createWatchlist } from '../lib/watchlist.js';
import { getPerformance } from '../lib/venue-hub.js';
import { openUpgradeModal } from '../components/upgrade-form.js';
import { importBox } from './import.js';
import { toast } from '../store/toast.js';
import { navigate } from '../router.js';
import { t } from '../i18n/index.js';

export async function watchlistPage() {
  const c = el('div', { class: 'container' });

  const head = el('div', { class: 'page-head' }, [
    el('h1', { text: t('watchlist.title') }),
    el('a', { href: '#/leaderboard', class: 'sm primary', text: t('watchlist.discover') }),
  ]);
  c.appendChild(head);

  // 导入框：导入回填后自动加入观察名单并刷新列表。
  const list = el('div', { class: 'card' }, [skeleton(4)]);
  let items = [];

  const reload = async () => {
    try {
      items = await listMyWatchlists();
    } catch (e) {
      list.innerHTML = '';
      list.appendChild(el('p', { class: 'neg', text: t('watchlist.loadError', { message: e.message }) }));
      return;
    }
    renderList({ list, items, reload: () => reload() });
  };

  const box = await importBox({
    onDone: async (_res, { platform, address }) => {
      try {
        await createWatchlist({ watch_platform: platform, watch_address: address });
        toast(t('watchlist.addSuccess'), 'success');
      } catch (e) {
        // 已在名单中等冲突：仍刷新列表，不阻断
        if (!/already|exists|冲突|409/i.test(e.message || '')) {
          toast(e.message || t('watchlist.addFailed'), 'error');
        }
      }
      await reload();
    },
  });
  c.appendChild(box);
  box.style.marginBottom = 'var(--sp-4)';
  c.appendChild(list);

  await reload();
  return withShell(c);
}

function renderList({ list, items, reload }) {
  list.innerHTML = '';
  headCount(items.length);
  if (!items || items.length === 0) {
    list.appendChild(emptyState({ icon: '👁', text: t('watchlist.empty'), action: el('a', { href: '#/leaderboard', text: t('watchlist.emptyAction') }) }));
    return;
  }
  const wrap = el('div', { class: 'follow-list' });
  for (const w of items) wrap.appendChild(watchCard(w, reload));
  list.appendChild(wrap);
}

function headCount(n) {
  const h = document.querySelector('.page-head h1');
  if (h) h.textContent = t('watchlist.titleCount', { count: n });
}

function watchCard(w, reload) {
  const card = el('div', { class: 'follow-card' });
  const isIdentity = !!w.watch_identity_id;
  const target = isIdentity
    ? t('watchlist.identityPrefix', { id: String(w.watch_identity_id).slice(0, 8) })
    : traderLabel({ address: w.watch_address });
  const platform = w.watch_platform || '—';
  const href = !isIdentity && w.watch_platform && w.watch_address
    ? `#/traders/${encodeURIComponent(w.watch_platform)}/${encodeURIComponent(w.watch_address)}`
    : '#/watchlist';

  card.appendChild(el('div', { class: 'fc-head' }, [
    el('div', { class: 'fc-title' }, [el('a', { href, text: `${target} · ${platform}` })]),
    el('span', { class: 'chip', text: t('watchlist.watchingChip') }),
  ]));
  card.appendChild(el('div', { class: 'fc-meta' }, [
    kv(t('watchlist.address'), w.watch_address ? w.watch_address.slice(0, 10) + '…' : '—'),
    kv(t('watchlist.savedAt'), w.created_at ? new Date(w.created_at).toLocaleDateString() : '—'),
  ]));

  // 绩效快照（trader 才拉；identity 暂不拉，跨平台聚合需 identity 端点）。
  const perfHost = el('div', { class: 'fc-meta' }, [el('span', { class: 'muted', text: t('watchlist.perfLoading') })]);
  card.appendChild(perfHost);
  if (!isIdentity && w.watch_platform && w.watch_address) {
    getPerformance(w.watch_platform, w.watch_address).then(p => {
      perfHost.innerHTML = '';
      const row = (p?.performance || []).find(x => x.period === '1m') || (p?.performance || [])[0];
      if (!row) { perfHost.appendChild(el('span', { class: 'muted', text: t('watchlist.noPerf') })); return; }
      perfHost.appendChild(kv('ROI(1m)', fmtPct(row.roi), pnlClass(row.roi)));
      perfHost.appendChild(kv(t('watchlist.winRate'), fmtPct(row.win_rate, 0)));
      perfHost.appendChild(kv(t('watchlist.realized'), fmtUSD(row.realized_pnl), pnlClass(row.realized_pnl)));
      const tags = p?.tags || row.tags || [];
      if (tags.length) perfHost.appendChild(tagChips(tags));
    }).catch(() => {
      perfHost.innerHTML = '';
      perfHost.appendChild(el('span', { class: 'muted', text: t('watchlist.perfUnavailable') }));
    });
  } else {
    perfHost.innerHTML = '';
    perfHost.appendChild(el('span', { class: 'muted', text: t('watchlist.identityPerfPending') }));
  }

  const actions = el('div', { class: 'fc-actions' });
  actions.appendChild(el('button', { class: 'sm primary', text: t('watchlist.upgradeToFollow'), onclick: () => openUpgradeModal({
    watchlist: w,
    onDone: async (follow) => {
      toast(t('watchlist.upgradeSuccess'), 'success');
      reload();
      // 升级后引导用户去跟随页确认配置
      navigate('/follows');
    },
  }) }));
  actions.appendChild(el('div', { class: 'spacer' }));
  actions.appendChild(el('button', { class: 'sm danger', text: t('watchlist.remove'), onclick: async () => {
    if (!confirm(t('watchlist.removeConfirm'))) return;
    try { await deleteWatchlist(w.id); toast(t('watchlist.removeSuccess'), 'success'); reload(); }
    catch (e) { toast(e.message, 'error'); }
  } }));
  card.appendChild(actions);
  return card;
}

function kv(k, v, cls) {
  const val = el('b', { text: String(v) });
  if (cls) val.className = cls;
  return el('span', { class: 'kv' }, [el('span', {}, k + ': '), val]);
}
