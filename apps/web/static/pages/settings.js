// pages/settings.js · 设置基础。对应 docs/FRONTEND_DESIGN.md §6.12/§6.13/§6.5/§6.4。
import { el, skeleton, emptyState } from '../components/ui.js';
import { withShell } from '../components/nav.js';
import { me, listCredentials, listWallets } from '../lib/account.js';
import { t } from '../i18n/index.js';

export async function settingsPage() {
  const c = el('div', { class: 'container' });
  c.appendChild(el('h1', { text: t('settings.pageTitle') }));
  c.appendChild(el('p', { class: 'muted', text: t('settings.pageSubtitle') }));

  const hub = el('div', { class: 'settings-hub' }, [
    el('a', { class: 'settings-hub-item', href: '#/settings/subscription', html: `<b>${t('settings.hubSubTitle')}</b><span>${t('settings.hubSubDesc')}</span>` }),
    el('a', { class: 'settings-hub-item', href: '#/settings/credentials', html: `<b>${t('settings.hubCredTitle')}</b><span>${t('settings.hubCredDesc')}</span>` }),
    el('a', { class: 'settings-hub-item', href: '#/settings/delegation', html: `<b>${t('settings.hubDelTitle')}</b><span>${t('settings.hubDelDesc')}</span>` }),
    el('a', { class: 'settings-hub-item', href: '#/settings/daemon-key', html: `<b>Daemon Key</b><span>${t('settings.hubDaemonDesc')}</span>` }),
  ]);
  c.appendChild(hub);

  // 账户
  c.appendChild(el('h2', { text: t('settings.accountTitle') }));
  const acctCard = el('div', { class: 'card' }, [skeleton(2)]);
  c.appendChild(acctCard);
  try {
    const u = await me();
    const wallets = await listWallets();
    const primary = (wallets || []).find(w => w.is_primary) || (wallets || [])[0];
    acctCard.innerHTML = '';
    acctCard.appendChild(el('p', {}, [el('strong', { text: t('settings.connectedWallet') }), el('span', { text: primary?.address || '—' })]));
    acctCard.appendChild(el('p', { class: 'muted', text: t('settings.userId', { id: u.id || '—' }) }));
  } catch (e) {
    acctCard.innerHTML = '';
    acctCard.appendChild(el('p', { class: 'neg', text: t('settings.accountLoadError', { message: e.message }) }));
  }

  // 订阅
  c.appendChild(el('h2', {}, [el('span', { text: t('settings.subTitle') }), ' ', el('a', { href: '#/settings/subscription', class: 'muted', text: t('common.manage') })]));
  const subCard = el('div', { class: 'card' }, [skeleton(1)]);
  c.appendChild(subCard);
  try {
    const u = await me();
    subCard.innerHTML = '';
    subCard.appendChild(el('p', {}, [el('strong', { text: t('settings.currentTier') }), el('span', { text: u.subscription_tier || 'free' })]));
    if (u.subscription_until) subCard.appendChild(el('p', { class: 'muted', text: t('settings.subUntil', { date: new Date(u.subscription_until).toLocaleDateString() }) }));
    subCard.appendChild(el('a', { href: '#/settings/subscription', text: t('settings.upgradeLink') }));
  } catch (e) {
    subCard.innerHTML = '';
    subCard.appendChild(el('p', { class: 'muted', text: t('settings.subLoadError', { message: e.message }) }));
  }

  // Venue 凭证
  c.appendChild(el('h2', { text: t('settings.credTitle') }));
  const credCard = el('div', { class: 'card' }, [skeleton(2)]);
  c.appendChild(credCard);
  try {
    const creds = await listCredentials();
    credCard.innerHTML = '';
    if (!creds || creds.length === 0) {
      credCard.appendChild(emptyState({ text: t('settings.credEmpty'), action: el('a', { href: '#/settings/credentials', text: t('settings.credConfigure') }) }));
    } else {
      const list = el('ul', {}, creds.map(cr => el('li', {}, [
        el('strong', { text: cr.platform }),
        el('span', { class: 'muted', text: ` · ${cr.kind || 'unknown'}` }),
        t('settings.proxyAddress', { address: cr.proxy_address || '—' }),
      ])));
      credCard.appendChild(list);
    }
  } catch (e) {
    credCard.innerHTML = '';
    credCard.appendChild(el('p', { class: 'neg', text: t('settings.credLoadError', { message: e.message }) }));
  }

  // daemon API key
  c.appendChild(el('h2', {}, [el('span', { text: 'daemon API key' }), ' ', el('a', { href: '#/settings/daemon-key', class: 'muted', text: t('common.manage') })]));
  const keyCard = el('div', { class: 'card' });
  c.appendChild(keyCard);
  keyCard.appendChild(el('p', { class: 'muted', text: t('settings.daemonDesc') }));
  keyCard.appendChild(el('a', { href: '#/settings/daemon-key', text: t('settings.daemonGoto') }));

  return withShell(c);
}

function mount(node) {
  const app = document.getElementById('app');
  app.innerHTML = ''; app.appendChild(node);
}
