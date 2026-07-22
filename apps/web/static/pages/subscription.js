// pages/subscription.js · 订阅（Pro+ 升级与权益管理）。对应 docs/FRONTEND_DESIGN.md §6.12。
import { el, skeleton } from '../components/ui.js';
import { withShell } from '../components/nav.js';
import { me, updateSubscription } from '../lib/account.js';
import { listMyFollows } from '../lib/follow.js';
import { toast } from '../store/toast.js';
import { navigate } from '../router.js';
import { t } from '../i18n/index.js';

const FREE_SLOTS = 3;
const PRO_PRICE = '$X/mo'; // F0 占位，待商务定价

export async function subscriptionPage() {
  const c = el('div', { class: 'container' });
  c.appendChild(el('h1', { text: t('subscription.title') }));

  const head = el('div', { class: 'card' }, [skeleton(2)]);
  c.appendChild(head);

  c.appendChild(el('h2', { text: t('subscription.tierComparison') }));
  const grid = el('div', { class: 'tier-grid' });
  c.appendChild(grid);

  let user = null, follows = [];
  try {
    [user, follows] = await Promise.all([me().catch(() => null), listMyFollows().catch(() => [])]);
  } catch (e) { /* ignore, degrade below */ }

  const tier = (user?.subscription_tier || 'free').toLowerCase();
  const isPro = tier === 'pro_plus';
  const until = user?.subscription_until;

  // 当前档位横幅
  head.innerHTML = '';
  head.appendChild(el('p', {}, [el('strong', { text: t('subscription.currentTierLabel') }), el('span', { class: 'sub-badge ' + (isPro ? 'pro' : 'free'), text: isPro ? t('subscription.proActiveBadge') : 'Free' })]));
  if (isPro && until) head.appendChild(el('p', { class: 'muted', text: t('subscription.subscribedUntil', { date: new Date(until).toLocaleDateString() }) }));
  if (!isPro) head.appendChild(el('p', { class: 'muted', text: t('subscription.upgradePitch') }));

  // Free 卡
  grid.appendChild(tierCard({
    name: 'Free', price: '$0', current: !isPro, isPro: false,
    features: [
      { ok: true, text: t('subscription.featChannelA') },
      { ok: false, text: t('subscription.featChannelBPro') },
      { ok: true, text: t('subscription.featSingleVenue') },
      { ok: false, text: t('subscription.featCrossVenuePro') },
      { ok: true, text: t('subscription.featBasicRisk') },
      { ok: false, text: t('subscription.featAdvancedRiskBasic') },
      { ok: true, text: t('subscription.featSlotsLimited', { count: FREE_SLOTS }) },
      { ok: false, text: t('subscription.featSlotsUnlimited') },
    ],
    actions: !isPro ? [] : [el('span', { class: 'muted', text: t('subscription.currentTierBadge') })],
  }));

  // Pro+ 卡
  grid.appendChild(tierCard({
    name: 'Pro+', price: PRO_PRICE, current: isPro, isPro: true,
    features: [
      { ok: true, text: t('subscription.featChannelA') },
      { ok: true, text: t('subscription.featChannelBZero') },
      { ok: true, text: t('subscription.featSingleCross') },
      { ok: true, text: t('subscription.featAdvancedRiskFull') },
      { ok: true, text: t('subscription.featSlotsUnlimited') },
    ],
    actions: isPro
      ? [el('button', { class: 'sm', text: t('subscription.renew'), onclick: () => openUpgrade(t('subscription.renewModalTitle')) })]
      : [el('button', { class: 'primary', text: t('subscription.upgrade'), onclick: () => openUpgrade(t('subscription.upgrade')) })],
  }));

  // Pro+ 用户权益使用
  if (isPro) {
    c.appendChild(el('h2', { text: t('subscription.usageTitle') }));
    const usage = el('div', { class: 'card' });
    const used = (follows || []).length;
    usage.appendChild(el('p', {}, [el('strong', { text: t('subscription.followSlotsLabel') }), el('span', { text: t('subscription.followSlotsCount', { used }) })]));
    usage.appendChild(el('p', { class: 'muted', text: t('subscription.channelBNote') }));
    usage.appendChild(el('div', { class: 'row' }, [
      el('button', { class: 'danger', text: t('subscription.cancelSubscription'), onclick: async () => {
        if (!confirm(t('subscription.cancelConfirm'))) return;
        try { await updateSubscription({ tier: 'free' }); toast(t('subscription.cancelSuccess'), 'success'); subscriptionPage().then(mount); }
        catch (e) { toast(e.message, 'error'); }
      } }),
    ]));
    c.appendChild(usage);
  }

  return withShell(c);
}

function tierCard({ name, price, current, isPro, features, actions }) {
  const card = el('div', { class: 'tier-card' + (isPro ? ' pro' : '') + (current ? ' current' : '') });
  card.appendChild(el('div', { class: 'tier-name', text: name }));
  card.appendChild(el('div', { class: 'tier-price', text: price }));
  const ul = el('ul');
  for (const f of features) ul.appendChild(el('li', { class: f.ok ? '' : 'no', text: f.text }));
  card.appendChild(ul);
  card.appendChild(el('div', { class: 'tier-actions' }, actions));
  return card;
}

/// F0 支付未接入：弹占位说明 + 测试环境直接开通入口。
function openUpgrade(title) {
  const backdrop = el('div', { class: 'modal-backdrop' });
  const modal = el('div', { class: 'modal' });
  backdrop.appendChild(modal);
  modal.appendChild(el('h2', { text: title }));
  modal.appendChild(el('p', { text: t('subscription.paymentComingSoon') }));
  modal.appendChild(el('p', { class: 'muted', text: t('subscription.paymentReplaceNote') }));
  const actions = el('div', { class: 'modal-actions' }, [
    el('button', { text: t('common.cancel'), onclick: () => backdrop.remove() }),
    el('button', { class: 'primary', text: t('subscription.activateTest'), onclick: async () => {
      try {
        const until = new Date(Date.now() + 30 * 864e5);
        await updateSubscription({ tier: 'pro_plus', until });
        toast(t('subscription.activateSuccess'), 'success');
        backdrop.remove();
        subscriptionPage().then(mount);
      } catch (e) { toast(e.message, 'error'); }
    } }),
  ]);
  modal.appendChild(actions);
  backdrop.addEventListener('click', e => { if (e.target === backdrop) backdrop.remove(); });
  document.body.appendChild(backdrop);
}

function mount(node) { const app = document.getElementById('app'); app.innerHTML = ''; app.appendChild(node); }
