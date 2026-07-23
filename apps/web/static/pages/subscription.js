// pages/subscription.js · 订阅（Pro+ 升级与权益管理）。对应 docs/FRONTEND_DESIGN.md §6.12。
// 支付：Polygon USDC invoice（创建发票 → 转账 → getLogs/receipt 确认 → 轮询 /me）。
import { el, skeleton } from '../components/ui.js';
import { withShell } from '../components/nav.js';
import {
  me,
  updateSubscription,
  createBillingInvoice,
  getActiveBillingInvoice,
  submitBillingTx,
} from '../lib/account.js';
import { listMyFollows } from '../lib/follow.js';
import { toast } from '../store/toast.js';
import { remount } from '../router.js';
import { t } from '../i18n/index.js';

const FREE_SLOTS = 3;
const POLL_MS = 12_000;
const POLL_MAX = 40; // ~8 分钟

function isProduction() {
  return !!(typeof window !== 'undefined' && window.__SHARPSIDE__ && window.__SHARPSIDE__.production);
}

function fmtAmount(v) {
  if (v == null) return '—';
  const n = typeof v === 'number' ? v : Number(String(v));
  if (!Number.isFinite(n)) return String(v);
  return (Math.round(n * 1e6) / 1e6).toString();
}

function shortAddr(a) {
  const s = String(a || '');
  if (s.length < 12) return s;
  return `${s.slice(0, 6)}…${s.slice(-4)}`;
}

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    toast(t('subscription.copied'), 'success');
  } catch {
    toast(t('subscription.copyFailed'), 'error');
  }
}

export async function subscriptionPage() {
  const c = el('div', { class: 'container' });
  c.appendChild(el('h1', { text: t('subscription.title') }));

  const head = el('div', { class: 'card' }, [skeleton(2)]);
  c.appendChild(head);

  c.appendChild(el('h2', { text: t('subscription.tierComparison') }));
  const grid = el('div', { class: 'tier-grid' });
  c.appendChild(grid);

  let user = null, follows = [], activeInv = null;
  try {
    [user, follows, activeInv] = await Promise.all([
      me().catch(() => null),
      listMyFollows().catch(() => []),
      getActiveBillingInvoice().catch(() => null),
    ]);
  } catch (e) { /* degrade */ }

  const tier = (user?.subscription_tier || 'free').toLowerCase();
  const isPro = tier === 'pro_plus';
  const until = user?.subscription_until;

  head.innerHTML = '';
  head.appendChild(el('p', {}, [
    el('strong', { text: t('subscription.currentTierLabel') }),
    el('span', { class: 'sub-badge ' + (isPro ? 'pro' : 'free'), text: isPro ? t('subscription.proActiveBadge') : 'Free' }),
  ]));
  if (isPro && until) {
    head.appendChild(el('p', { class: 'muted', text: t('subscription.subscribedUntil', { date: new Date(until).toLocaleDateString() }) }));
  }
  if (!isPro) {
    head.appendChild(el('p', { class: 'muted', text: t('subscription.upgradePitch') }));
  }

  // 未完成支付的 pending 发票：醒目续办入口
  if (!isPro && activeInv && activeInv.status === 'pending') {
    const banner = el('div', { class: 'card', style: 'margin-top:12px' });
    banner.appendChild(el('p', {}, [
      el('strong', { text: t('subscription.pendingInvoiceTitle') }),
    ]));
    banner.appendChild(el('p', {
      class: 'muted',
      text: t('subscription.pendingInvoiceHint', {
        amount: fmtAmount(activeInv.amount_usdc),
        expires: new Date(activeInv.expires_at).toLocaleString(),
      }),
    }));
    banner.appendChild(el('button', {
      class: 'primary sm',
      text: t('subscription.resumePayment'),
      onclick: () => openPayModal(activeInv),
    }));
    head.appendChild(banner);
  }

  const proPrice = activeInv?.amount_usdc != null
    ? t('subscription.priceUsdcMo', { amount: fmtAmount(activeInv.amount_usdc) })
    : t('subscription.priceUsdcLabel');

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

  grid.appendChild(tierCard({
    name: 'Pro+', price: proPrice, current: isPro, isPro: true,
    features: [
      { ok: true, text: t('subscription.featChannelA') },
      { ok: true, text: t('subscription.featChannelBZero') },
      { ok: true, text: t('subscription.featSingleCross') },
      { ok: true, text: t('subscription.featAdvancedRiskFull') },
      { ok: true, text: t('subscription.featSlotsUnlimited') },
    ],
    actions: isPro
      ? [
          el('button', {
            class: 'sm primary',
            text: t('subscription.renew'),
            onclick: () => startCheckout(t('subscription.renewModalTitle')),
          }),
        ]
      : [
          el('button', {
            class: 'primary',
            text: t('subscription.upgrade'),
            onclick: () => startCheckout(t('subscription.upgrade')),
          }),
        ],
  }));

  if (isPro) {
    c.appendChild(el('h2', { text: t('subscription.usageTitle') }));
    const usage = el('div', { class: 'card' });
    const used = (follows || []).length;
    usage.appendChild(el('p', {}, [
      el('strong', { text: t('subscription.followSlotsLabel') }),
      el('span', { text: t('subscription.followSlotsCount', { used }) }),
    ]));
    usage.appendChild(el('p', { class: 'muted', text: t('subscription.channelBNote') }));
    usage.appendChild(el('div', { class: 'row' }, [
      el('button', {
        class: 'danger',
        text: t('subscription.cancelSubscription'),
        onclick: async () => {
          if (!confirm(t('subscription.cancelConfirm'))) return;
          try {
            await updateSubscription({ tier: 'free' });
            toast(t('subscription.cancelSuccess'), 'success');
            remount();
          } catch (e) {
            toast(e.message, 'error');
          }
        },
      }),
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

/// 创建发票并打开支付面板；计费未配置时非生产可回退测试开通。
async function startCheckout(title) {
  try {
    const inv = await createBillingInvoice({ period_days: 30 });
    openPayModal(inv, title);
  } catch (e) {
    const msg = e.message || String(e);
    const billingOff = /计费未配置|BILLING_TREASURY|billing/i.test(msg);
    if (billingOff && !isProduction()) {
      openDevFallback(title, msg);
      return;
    }
    toast(msg, 'error');
  }
}

function openDevFallback(title, errMsg) {
  const backdrop = el('div', { class: 'modal-backdrop' });
  const modal = el('div', { class: 'modal' });
  backdrop.appendChild(modal);
  modal.appendChild(el('h2', { text: title }));
  modal.appendChild(el('p', { text: t('subscription.billingNotConfigured') }));
  modal.appendChild(el('p', { class: 'muted', text: errMsg }));
  modal.appendChild(el('div', { class: 'modal-actions' }, [
    el('button', { text: t('common.cancel'), onclick: () => backdrop.remove() }),
    el('button', {
      class: 'primary',
      text: t('subscription.activateTest'),
      onclick: async () => {
        try {
          const until = new Date(Date.now() + 30 * 864e5);
          await updateSubscription({ tier: 'pro_plus', until });
          toast(t('subscription.activateSuccess'), 'success');
          backdrop.remove();
          remount();
        } catch (err) {
          toast(err.message, 'error');
        }
      },
    }),
  ]));
  backdrop.addEventListener('click', (ev) => { if (ev.target === backdrop) backdrop.remove(); });
  document.body.appendChild(backdrop);
}

function openPayModal(inv, title) {
  let stopped = false;
  let pollCount = 0;
  let timer = null;

  const backdrop = el('div', { class: 'modal-backdrop' });
  const modal = el('div', { class: 'modal' });
  backdrop.appendChild(modal);

  const close = () => {
    stopped = true;
    if (timer) clearTimeout(timer);
    backdrop.remove();
  };

  modal.appendChild(el('h2', { text: title || t('subscription.payTitle') }));
  modal.appendChild(el('p', { class: 'muted', text: t('subscription.payIntro') }));

  const amount = fmtAmount(inv.amount_usdc);
  const rows = [
    [t('subscription.payAmount'), `${amount} USDC`, String(inv.amount_usdc)],
    [t('subscription.payTreasury'), shortAddr(inv.treasury_address), inv.treasury_address],
    [t('subscription.payToken'), shortAddr(inv.token_address), inv.token_address],
    [t('subscription.payChain'), `Polygon (${inv.chain_id || 137})`, String(inv.chain_id || 137)],
    [t('subscription.payExpires'), new Date(inv.expires_at).toLocaleString(), null],
  ];

  const table = el('div', { class: 'pay-rows' });
  for (const [label, display, copyVal] of rows) {
    const row = el('div', { class: 'row', style: 'align-items:center;gap:8px;margin:6px 0;flex-wrap:wrap' });
    row.appendChild(el('strong', { style: 'min-width:5.5em', text: label }));
    row.appendChild(el('code', { text: display }));
    if (copyVal) {
      row.appendChild(el('button', {
        class: 'sm',
        text: t('subscription.copy'),
        onclick: () => copyText(copyVal),
      }));
    }
    table.appendChild(row);
  }
  modal.appendChild(table);

  modal.appendChild(el('p', {
    class: 'muted',
    text: t('subscription.payExactHint', { amount }),
  }));

  // 可选 submit-tx
  const txRow = el('div', { class: 'row', style: 'gap:8px;margin-top:12px;flex-wrap:wrap;align-items:center' });
  const txInput = el('input', {
    type: 'text',
    placeholder: t('subscription.txHashPlaceholder'),
    style: 'flex:1;min-width:180px',
  });
  txRow.appendChild(txInput);
  txRow.appendChild(el('button', {
    class: 'sm',
    text: t('subscription.submitTx'),
    onclick: async () => {
      const hash = (txInput.value || '').trim();
      if (!hash) {
        toast(t('subscription.txHashRequired'), 'error');
        return;
      }
      try {
        await submitBillingTx(inv.id, hash);
        toast(t('subscription.submitTxOk'), 'success');
        statusEl.textContent = t('subscription.waitingConfirm');
      } catch (e) {
        toast(e.message, 'error');
      }
    },
  }));
  modal.appendChild(txRow);
  modal.appendChild(el('p', { class: 'muted', text: t('subscription.submitTxOptional') }));

  const statusEl = el('p', { class: 'muted', text: t('subscription.waitingConfirm') });
  modal.appendChild(statusEl);

  modal.appendChild(el('div', { class: 'modal-actions' }, [
    el('button', { text: t('common.close'), onclick: close }),
    el('button', {
      class: 'primary',
      text: t('subscription.refreshStatus'),
      onclick: () => checkOnce(true),
    }),
  ]));

  backdrop.addEventListener('click', (ev) => { if (ev.target === backdrop) close(); });
  document.body.appendChild(backdrop);

  async function checkOnce(manual) {
    if (stopped) return;
    try {
      const u = await me();
      if ((u?.subscription_tier || '').toLowerCase() === 'pro_plus') {
        statusEl.textContent = t('subscription.paymentConfirmed');
        toast(t('subscription.paymentConfirmed'), 'success');
        close();
        remount();
        return;
      }
      const still = await getActiveBillingInvoice().catch(() => null);
      if (!still || still.status !== 'pending') {
        statusEl.textContent = t('subscription.invoiceGone');
        if (manual) toast(t('subscription.invoiceGone'), 'error');
        return;
      }
      if (new Date(still.expires_at).getTime() < Date.now()) {
        statusEl.textContent = t('subscription.invoiceExpired');
        return;
      }
      statusEl.textContent = t('subscription.waitingConfirm');
      if (manual) toast(t('subscription.stillWaiting'), 'info');
    } catch (e) {
      if (manual) toast(e.message, 'error');
    }
  }

  function schedule() {
    if (stopped || pollCount >= POLL_MAX) {
      if (!stopped && pollCount >= POLL_MAX) {
        statusEl.textContent = t('subscription.pollTimeout');
      }
      return;
    }
    pollCount += 1;
    timer = setTimeout(async () => {
      await checkOnce(false);
      schedule();
    }, POLL_MS);
  }

  checkOnce(false).then(schedule);
}
