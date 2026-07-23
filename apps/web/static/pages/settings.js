// pages/settings.js · 设置基础。对应 docs/FRONTEND_DESIGN.md §6.12/6.13/6.5/6.4。
import { el, skeleton, emptyState } from '../components/ui.js';
import { withShell } from '../components/nav.js';
import { me, listCredentials, listWallets, walletNonce, linkWallet, unlinkWallet } from '../lib/account.js';
import { openWalletPicker } from '../lib/wallet-connect.js';
import { connect, personalSign, buildSiwe } from '../lib/siwe.js';
import { toast } from '../store/toast.js';
import { t } from '../i18n/index.js';

export async function settingsPage() {
  const c = el('div', { class: 'container' });
  c.appendChild(el('h1', { text: t('settings.pageTitle') }));
  c.appendChild(el('p', { class: 'muted', text: t('settings.pageSubtitle') }));

  const hub = el('div', { class: 'settings-hub' }, [
    el('a', { class: 'settings-hub-item', href: '#/settings/subscription', html: `<b>${t('settings.hubSubTitle')}</b><span>${t('settings.hubSubDesc')}</span>` }),
    el('a', { class: 'settings-hub-item', href: '#/settings/credentials', html: `<b>${t('settings.hubCredTitle')}</b><span>${t('settings.hubCredDesc')}</span>` }),
    el('a', { class: 'settings-hub-item', href: '#/settings/delegation', html: `<b>${t('settings.hubDelTitle')}</b><span>${t('settings.hubDelDesc')}</span>` }),
    el('a', { class: 'settings-hub-item', href: '#/settings/daemon-key', html: `<b>${t('settings.hubDaemonTitle')}</b><span>${t('settings.hubDaemonDesc')}</span>` }),
  ]);
  c.appendChild(hub);

  // 账户
  c.appendChild(el('h2', { text: t('settings.accountTitle') }));
  const acctCard = el('div', { class: 'card' }, [skeleton(2)]);
  c.appendChild(acctCard);

  // 已绑定钱包
  c.appendChild(el('h2', { text: t('settings.walletsTitle') }));
  c.appendChild(el('p', { class: 'muted', text: t('settings.walletsIntro') }));
  const walletCard = el('div', { class: 'card' }, [skeleton(2)]);
  c.appendChild(walletCard);

  let currentUser = null;

  async function renderAccount() {
    try {
      const u = await me();
      currentUser = u;
      const wallets = await listWallets();
      const primary = (wallets || []).find(w => w.is_primary) || (wallets || [])[0];
      acctCard.innerHTML = '';
      acctCard.appendChild(el('p', {}, [el('strong', { text: t('settings.connectedWallet') }), el('span', { text: primary?.address || '—' })]));
      acctCard.appendChild(el('p', { class: 'muted', text: t('settings.userId', { id: u.id || '—' }) }));
    } catch (e) {
      acctCard.innerHTML = '';
      acctCard.appendChild(el('p', { class: 'neg', text: t('settings.accountLoadError', { message: e.message }) }));
    }
  }

  async function renderWallets() {
    try {
      const wallets = await listWallets();
      walletCard.innerHTML = '';
      if (!wallets || wallets.length === 0) {
        walletCard.appendChild(emptyState({ text: t('settings.walletNoWallets') }));
      } else {
        const list = el('ul', { style: 'list-style:none;padding:0;display:flex;flex-direction:column;gap:8px' },
          wallets.map(w => {
            const head = el('div', { style: 'display:flex;align-items:center;gap:8px;flex-wrap:wrap' }, [
              el('code', { style: 'word-break:break-all', text: w.address }),
              w.is_primary ? el('span', { class: 'muted', text: `· ${t('settings.walletPrimary')}` }) : null,
              w.label ? el('span', { class: 'muted', text: `· ${w.label}` }) : null,
            ]);
            if (!w.is_primary) {
              const btn = el('button', { class: 'sm', text: t('settings.walletUnlinkBtn') });
              btn.onclick = async () => {
                if (!confirm(t('settings.walletUnlinkConfirm'))) return;
                try {
                  await unlinkWallet(w.address);
                  toast(t('settings.walletUnlinkSuccess'), 'success');
                  await renderWallets();
                } catch (e) {
                  toast(t('settings.walletUnlinkError', { message: e.message || '' }), 'error');
                }
              };
              return el('li', {}, [head, btn]);
            }
            return el('li', {}, [head]);
          }),
        );
        walletCard.appendChild(list);
      }

      const bindBtn = el('button', { class: 'primary', style: 'margin-top:10px', text: t('settings.walletBindBtn') });
      bindBtn.onclick = async () => {
        bindBtn.disabled = true;
        bindBtn.textContent = t('settings.walletBinding');
        try {
          await bindNewWallet();
          toast(t('settings.walletBindSuccess'), 'success');
          await renderWallets();
        } catch (e) {
          toast(t('settings.walletBindError', { message: e.message || '' }), 'error');
        } finally {
          bindBtn.disabled = false;
          bindBtn.textContent = t('settings.walletBindBtn');
        }
      };
      walletCard.appendChild(bindBtn);
    } catch (e) {
      walletCard.innerHTML = '';
      walletCard.appendChild(el('p', { class: 'neg', text: t('settings.walletBindError', { message: e.message }) }));
    }
  }

  /// 绑定新钱包：选钱包 → 取 nonce → SIWE 签名 → POST /me/wallets。
  /// 后端从验签消息权威导出地址，不信任客户端传入。
  async function bindNewWallet() {
    const wallet = await openWalletPicker();
    if (!wallet) return;
    const [address] = await connect(wallet);
    const { nonce, domain, uri, chain_id, issued_at } = await walletNonce(address);
    const message = buildSiwe({
      domain,
      address,
      uri: uri || location.origin,
      chainId: chain_id,
      nonce,
      issuedAt: issued_at,
      expirationTime: new Date(Date.now() + 5 * 60 * 1000).toISOString(),
    });
    const signature = await personalSign(wallet, message, address);
    await linkWallet({ message, signature });
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

  await renderAccount();
  await renderWallets();

  return withShell(c);
}
