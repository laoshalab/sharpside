// components/upgrade-form.js · watchlist → follow 升级模态。对应 Watchlist 功能规划。
import { el } from './ui.js';
import { upgradeWatchlist } from '../lib/watchlist.js';
import { t } from '../i18n/index.js';

/// 打开升级模态。watchlist 为待升级的观察项；onDone(follow) 成功回调。
export function openUpgradeModal({ watchlist, onDone }) {
  const backdrop = el('div', { class: 'modal-backdrop' });
  const modal = el('div', { class: 'modal' });
  backdrop.appendChild(modal);

  const target = watchlist.watch_identity_id
    ? t('upgradeForm.identityPrefix', { id: String(watchlist.watch_identity_id).slice(0, 8) })
    : `${watchlist.watch_platform || '—'} / ${(watchlist.watch_address || '').slice(0, 10)}…`;

  const form = el('form');
  form.appendChild(el('h2', { text: t('upgradeForm.title') }));
  form.appendChild(el('p', { class: 'muted', text: t('upgradeForm.targetDescription', { target }) }));

  const venueInput = el('input', { name: 'execute_venue', value: watchlist.watch_platform || 'polymarket' });
  const channelSel = selectEl('channel', [
    ['tg', t('followForm.channelTg')],
    ['daemon', t('followForm.channelDaemon')],
  ], 'tg');
  const modeSel = selectEl('sizing_mode', [
    ['fixed', t('followForm.sizingFixed')],
    ['proportional', t('followForm.sizingProportional')],
  ], 'fixed');
  const sizingInput = el('input', { name: 'sizing_value', type: 'number', step: '0.01', min: '0', value: '10' });
  modeSel.onchange = () => {
    const mode = modeSel.value;
    const cur = Number(sizingInput.value);
    if (mode === 'proportional' && (!(cur > 0) || cur > 1)) sizingInput.value = '0.5';
    if (mode === 'fixed' && (!(cur > 0) || cur <= 1)) sizingInput.value = '10';
  };

  const sameVenueCb = el('input', { name: 'same_venue_only', type: 'checkbox', checked: 'checked' });
  const adv = el('details', { class: 'advanced' });
  adv.appendChild(el('summary', { text: t('upgradeForm.advanced') }));
  adv.appendChild(field(t('followForm.maxOrder'), el('input', { name: 'max_order', type: 'number', step: '0.1', min: '0', value: '0' })));
  adv.appendChild(field(t('followForm.dailyMax'), el('input', { name: 'daily_max', type: 'number', step: '0.1', min: '0', value: '0' })));
  adv.appendChild(field(t('followForm.maxOpen'), el('input', { name: 'max_open', type: 'number', step: '1', min: '0', value: '0' })));

  const errP = el('p', { class: 'error' });
  [field(t('followForm.executeVenue'), venueInput),
   field(t('followForm.channel'), channelSel),
   field(t('followForm.sizingMode'), modeSel),
   field(t('followForm.sizingValue'), sizingInput),
   el('p', { class: 'muted', text: t('followForm.sizingHint') }),
   field('', el('label', {}, [sameVenueCb, t('followForm.sameVenueOnly')])),
   adv, errP].forEach(n => form.appendChild(n));

  const actions = el('div', { class: 'modal-actions' });
  const cancelBtn = el('button', { text: t('common.cancel'), onclick: () => backdrop.remove() });
  const okBtn = el('button', { class: 'primary', text: t('upgradeForm.submit') });
  actions.appendChild(cancelBtn); actions.appendChild(okBtn);
  form.appendChild(actions);
  modal.appendChild(form);

  const q = (name) => form.querySelector(`[name="${name}"]`);

  okBtn.onclick = async (e) => {
    e.preventDefault();
    errP.textContent = '';
    const venue = venueInput.value.trim();
    const channel = channelSel.value;
    const mode = modeSel.value;
    const sv = Number(sizingInput.value || 0);
    if (!(sv > 0)) { errP.textContent = t('followForm.errorSizing'); return; }
    if (mode === 'proportional' && !(sv > 0 && sv <= 1)) {
      errP.textContent = t('followForm.errorProportional');
      return;
    }
    const sizingValue = mode === 'fixed' ? { amount: sv } : mode === 'proportional' ? { ratio: sv } : { pct: sv };
    const body = {
      execute_venue: venue,
      channel,
      config: {
        sizing: { mode, value: sizingValue },
        execute_venue: venue,
        channel,
        same_venue_only: sameVenueCb.checked,
        max_notional_per_order: numOr0(q('max_order')),
        daily_max_notional: numOr0(q('daily_max')),
        max_open_positions: intOr0(q('max_open')),
      },
    };
    okBtn.disabled = true;
    try {
      const follow = await upgradeWatchlist(watchlist.id, body);
      backdrop.remove();
      if (onDone) await onDone(follow);
    } catch (err) {
      errP.textContent = err.message;
      okBtn.disabled = false;
    }
  };

  backdrop.addEventListener('click', e => { if (e.target === backdrop) backdrop.remove(); });
  document.body.appendChild(backdrop);
}

function numOr0(node) { const v = Number(node?.value || ''); return Number.isFinite(v) && v > 0 ? v : 0; }
function intOr0(node) { const v = parseInt(node?.value || '', 10); return Number.isFinite(v) && v > 0 ? v : 0; }
function field(label, child) {
  const wrap = el('div', { class: 'field' });
  if (label) wrap.appendChild(el('label', { text: label }));
  wrap.appendChild(child);
  return wrap;
}
function selectEl(name, options, val) {
  const s = el('select', { name });
  for (const [v, l] of options) s.appendChild(el('option', { value: v, text: l, ...(v === val ? { selected: 'selected' } : {}) }));
  return s;
}
