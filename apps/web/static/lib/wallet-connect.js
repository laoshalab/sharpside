// lib/wallet-connect.js · 钱包连接弹窗 + SIWE 鉴权流程（可从导航任意页触发）。
import { el } from '../components/ui.js';
import { discoverWallets, connect, personalSign, buildSiwe } from './siwe.js';
import { me, walletNonce, walletLogin } from './account.js';
import { setToken, setUser } from '../store/auth.js';
import { toast } from '../store/toast.js';
import { navigate } from '../router.js';
import { t } from '../i18n/index.js';

let pickerOpen = false;

/// 浏览器正中钱包选择弹窗。返回选中的 wallet，或取消时 null。
export function openWalletPicker() {
  if (pickerOpen) return Promise.resolve(null);
  pickerOpen = true;

  return new Promise((resolve) => {
    let selected = null;
    let settled = false;
    const finish = (value) => {
      if (settled) return;
      settled = true;
      pickerOpen = false;
      window.removeEventListener('keydown', onKey);
      overlay.remove();
      resolve(value);
    };

    const overlay = el('div', {
      class: 'modal-backdrop wallet-picker-backdrop',
      role: 'dialog',
      'aria-modal': 'true',
      'aria-label': t('walletConnect.ariaLabel'),
    });
    const box = el('div', { class: 'modal wallet-picker' });
    box.appendChild(el('h3', { text: t('walletConnect.title') }));
    box.appendChild(el('p', { class: 'hint', text: t('walletConnect.hint') }));

    const list = el('div', { class: 'wallet-list' });
    list.appendChild(el('p', { class: 'muted wallet-loading', text: t('walletConnect.detecting') }));
    box.appendChild(list);

    const connectBtn = el('button', { class: 'primary wallet-connect-btn', text: t('walletConnect.connect'), disabled: 'disabled' });
    const cancelBtn = el('button', { class: 'ghost', text: t('walletConnect.cancel') });
    box.appendChild(el('div', { class: 'modal-actions wallet-picker-actions' }, [cancelBtn, connectBtn]));

    overlay.appendChild(box);
    overlay.addEventListener('click', (e) => { if (e.target === overlay) finish(null); });
    cancelBtn.onclick = () => finish(null);
    connectBtn.onclick = () => { if (selected) finish(selected); };
    document.body.appendChild(overlay);

    const onKey = (e) => { if (e.key === 'Escape') finish(null); };
    window.addEventListener('keydown', onKey);

    const renderList = (wallets) => {
      list.innerHTML = '';
      if (!wallets.length) {
        list.appendChild(el('div', { class: 'wallet-empty' }, [
          el('p', { text: t('walletConnect.noWallet') }),
          el('p', { class: 'hint', text: t('walletConnect.installHint') }),
        ]));
        connectBtn.disabled = true;
        return;
      }
      wallets.forEach((w) => {
        const row = el('button', { type: 'button', class: 'wallet-option' });
        if (w.info.icon) {
          const img = el('img', { class: 'wallet-icon', src: w.info.icon, alt: '' });
          img.onerror = () => { img.style.display = 'none'; };
          row.appendChild(img);
        } else {
          row.appendChild(el('span', { class: 'wallet-icon wallet-icon-fallback', text: '◈' }));
        }
        row.appendChild(el('div', { class: 'wallet-meta' }, [
          el('div', { class: 'wallet-name', text: w.info.name || t('walletConnect.unknownWallet') }),
          w.info.rdns && w.info.rdns !== 'window.ethereum'
            ? el('div', { class: 'wallet-rdns', text: w.info.rdns })
            : null,
        ]));
        row.onclick = () => {
          selected = w;
          list.querySelectorAll('.wallet-option').forEach((n) => n.classList.remove('selected'));
          row.classList.add('selected');
          connectBtn.disabled = false;
          connectBtn.focus();
        };
        row.ondblclick = () => { selected = w; finish(w); };
        list.appendChild(row);
      });
    };

    discoverWallets(400).then(renderList).catch(() => renderList([]));
  });
}

/// 弹窗选钱包 → eth_requestAccounts → SIWE → 存 JWT → 跳仪表盘。
/// 成功返回 true；取消返回 false；失败抛错。
export async function connectWalletFlow({ redirect = '/dashboard' } = {}) {
  const wallet = await openWalletPicker();
  if (!wallet) return false;

  const [address] = await connect(wallet);
  const { nonce, domain, chain_id, issued_at } = await walletNonce(address);
  const message = buildSiwe({
    domain,
    address,
    uri: location.origin,
    chainId: chain_id,
    nonce,
    issuedAt: issued_at,
    expirationTime: new Date(Date.now() + 5 * 60 * 1000).toISOString(),
  });
  const signature = await personalSign(wallet, message, address);
  const r = await walletLogin({ message, signature });
  setToken(r.token);
  try { setUser(await me()); } catch {}
  toast(t('walletConnect.connected'), 'success');
  if (redirect) navigate(redirect);
  return true;
}
