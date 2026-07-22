// components/upgrade-form.js · watchlist → follow 升级模态。对应 Watchlist 功能规划。
// 复用 follow-form 的字段口径（execute_venue / channel / sizing / 风控），
// 但提交走 POST /follow/watchlists/{id}/upgrade（事务内删 watchlist + 建 follow）。
// props: { watchlist: Watchlist, onDone: async (follow) => any }
//   onDone：升级成功后回调（通常 toast + 跳转 /follows）。
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

  const venueF = field(t('followForm.executeVenue'), el('input', { id: 'execute_venue', value: watchlist.watch_platform || 'polymarket' }));
  const channelF = field(t('followForm.channel'), selectEl('channel', [
    ['tg', t('followForm.channelTg')],
    ['daemon', t('followForm.channelDaemon')],
  ], 'tg'));
  const sizingModeF = field('sizing mode', selectEl('sizing_mode', [
    ['fixed', t('followForm.sizingFixed')],
    ['proportional', t('followForm.sizingProportional')],
  ], 'fixed'));
  const sizingValF = field(t('followForm.sizingValue'), el('input', { id: 'sizing_value', type: 'number', step: '0.01', min: '0', value: '10' }));
  const sizingHint = el('p', { class: 'muted', text: t('followForm.sizingHint') });
  const sameVenueF = field('', el('label', {}, [el('input', { id: 'same_venue_only', type: 'checkbox', checked: 'checked' }), t('followForm.sameVenueOnly')]));

  const adv = el('details', { class: 'advanced' });
  adv.appendChild(el('summary', { text: t('upgradeForm.advanced') }));
  adv.appendChild(field(t('followForm.maxOrder'), el('input', { id: 'max_order', type: 'number', step: '0.1', min: '0', value: '0' })));
  adv.appendChild(field(t('followForm.dailyMax'), el('input', { id: 'daily_max', type: 'number', step: '0.1', min: '0', value: '0' })));
  adv.appendChild(field(t('followForm.maxOpen'), el('input', { id: 'max_open', type: 'number', step: '1', min: '0', value: '0' })));

  const errP = el('p', { class: 'error' });
  [venueF, channelF, sizingModeF, sizingValF, sizingHint, sameVenueF, adv, errP].forEach(n => form.appendChild(n));

  const actions = el('div', { class: 'modal-actions' });
  const cancelBtn = el('button', { text: t('common.cancel'), onclick: () => backdrop.remove() });
  const okBtn = el('button', { class: 'primary', text: t('upgradeForm.submit') });
  actions.appendChild(cancelBtn); actions.appendChild(okBtn);
  form.appendChild(actions);
  modal.appendChild(form);

  okBtn.onclick = async (e) => {
    e.preventDefault();
    errP.textContent = '';
    const venue = document.getElementById('execute_venue').value.trim();
    const channel = document.getElementById('channel').value;
    const mode = document.getElementById('sizing_mode').value;
    const sv = Number(document.getElementById('sizing_value').value || 0);
    if (!(sv > 0)) { errP.textContent = t('followForm.errorSizing'); return; }
    const sizingValue = mode === 'fixed' ? { amount: sv } : mode === 'proportional' ? { ratio: sv } : { pct: sv };
    const body = {
      execute_venue: venue,
      channel,
      config: {
        sizing: { mode, value: sizingValue },
        execute_venue: venue,
        channel,
        same_venue_only: document.getElementById('same_venue_only').checked,
        max_notional_per_order: numOr0('max_order'),
        daily_max_notional: numOr0('daily_max'),
        max_open_positions: intOr0('max_open'),
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

function numOr0(id) { const v = Number(document.getElementById(id)?.value || ''); return Number.isFinite(v) && v > 0 ? v : 0; }
function intOr0(id) { const v = parseInt(document.getElementById(id)?.value || '', 10); return Number.isFinite(v) && v > 0 ? v : 0; }
function field(label, child) {
  const wrap = el('div', { class: 'field' });
  if (label) wrap.appendChild(el('label', { text: label }));
  wrap.appendChild(child);
  return wrap;
}
function selectEl(name, options, val) {
  const s = el('select', { id: name });
  for (const [v, l] of options) s.appendChild(el('option', { value: v, text: l, ...(v === val ? { selected: 'selected' } : {}) }));
  return s;
}
