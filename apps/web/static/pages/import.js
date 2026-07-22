// pages/import.js · 导入交易者地址。对应 docs/PERFORMANCE_PIPELINE.md §8.2 ImportBox。
// 输入钱包地址 → 调 POST /venue-hub/traders/import → 触发同步回填 raw_trades。
// 独立路由 #/import 已并入观察名单页，此处仅保留 ImportBox 与兼容跳转。
import { el } from '../components/ui.js';
import { importTrader, listVenues } from '../lib/venue-hub.js';
import { navigate } from '../router.js';
import { toast } from '../store/toast.js';
import { t } from '../i18n/index.js';

/// 已知 signal_source 平台（与 venue-hub 注册一致）。listVenues 失败时回退。
const FALLBACK_PLATFORMS = [
  ['polymarket', 'Polymarket'],
  ['kalshi', 'Kalshi'],
  ['manifold', 'Manifold'],
  ['zeitgeist', 'Zeitgeist'],
  ['azuro', 'Azuro'],
];

/// 兼容旧书签 #/import → #/watchlist。
export async function importPage() {
  navigate('/watchlist');
  return el('div');
}

/// ImportBox：平台 + 地址同一行。成功后调用 onDone(res, { platform, address })。
export async function importBox({ onDone } = {}) {
  const box = el('div', { class: 'card import-box' });
  box.appendChild(el('h2', { text: t('importPage.title') }));
  box.appendChild(el('p', { class: 'muted', text: t('importPage.description') }));

  const platformSel = el('select', { id: 'platform' });
  try {
    const venues = await listVenues();
    const list = Array.isArray(venues) ? venues : (venues?.items || []);
    const signalSources = list.filter(v => (v.capabilities || []).includes('signal_source'));
    if (signalSources.length) {
      for (const v of signalSources) {
        const key = v.platform || v.id || v.name;
        const label = v.display_name || v.name || key;
        if (!key) continue;
        platformSel.appendChild(el('option', {
          value: key, text: label,
          ...(key === 'polymarket' ? { selected: 'selected' } : {}),
        }));
      }
    } else {
      fillFallback(platformSel);
    }
  } catch {
    fillFallback(platformSel);
  }

  const addrInput = el('input', {
    id: 'address', type: 'text', autocomplete: 'off', spellcheck: 'false',
    placeholder: '0x…  (Polymarket proxy wallet)',
  });

  const errP = el('p', { class: 'error' });
  const submitBtn = el('button', { class: 'primary', text: t('importPage.submit'), onclick: submit });

  const platformField = field(t('importPage.platform'), platformSel);
  platformField.style.flex = '0 0 160px';
  const addrField = field(t('importPage.walletAddress'), addrInput);
  addrField.style.flex = '1 1 240px';

  box.appendChild(el('div', { class: 'row' }, [platformField, addrField, submitBtn]));
  box.appendChild(errP);

  async function submit() {
    errP.textContent = '';
    const platform = platformSel.value.trim();
    const address = addrInput.value.trim();
    if (!platform) { errP.textContent = t('importPage.selectPlatform'); return; }
    if (!address) { errP.textContent = t('importPage.enterAddress'); return; }
    if (!/^0x[a-fA-F0-9]{40}$/.test(address)) {
      errP.textContent = t('importPage.invalidAddress');
      return;
    }
    submitBtn.disabled = true;
    submitBtn.textContent = t('importPage.submitting');
    try {
      const res = await importTrader({ platform, address });
      const n = res?.trades_backfilled ?? 0;
      toast(t('importPage.success', { count: n }), 'success');
      addrInput.value = '';
      if (typeof onDone === 'function') {
        await onDone(res, { platform, address: address.toLowerCase() });
      } else {
        navigate(`/traders/${encodeURIComponent(platform)}/${encodeURIComponent(address.toLowerCase())}`);
      }
    } catch (e) {
      errP.textContent = e.message || t('importPage.importFailed');
    } finally {
      submitBtn.disabled = false;
      submitBtn.textContent = t('importPage.submit');
    }
  }
  return box;
}

function fillFallback(sel) {
  for (const [v, l] of FALLBACK_PLATFORMS) {
    sel.appendChild(el('option', { value: v, text: l, ...(v === 'polymarket' ? { selected: 'selected' } : {}) }));
  }
}

function field(label, child) {
  const w = el('div', { class: 'field' });
  if (label) w.appendChild(el('label', { text: label }));
  w.appendChild(child);
  return w;
}
