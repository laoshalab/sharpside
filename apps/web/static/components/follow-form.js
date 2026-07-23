// components/follow-form.js · 跟随配置表单（模态）。对应 docs/FRONTEND_DESIGN.md §6.9/§6.10。
// 复用于：我的跟随页[编辑]模态、创建跟随页（页面形式另写，字段一致）。
// props: { follow: FollowRelation, onSaved: async (body) => any }
import { el } from './ui.js';
import { t } from '../i18n/index.js';

/// 打开 follow-form 模态。follow 为现有跟随关系（编辑）；onSaved(body) 提交。
export function openFollowFormModal({ follow, onSaved }) {
  const backdrop = el('div', { class: 'modal-backdrop' });
  const modal = el('div', { class: 'modal' });
  backdrop.appendChild(modal);
  const cfg = (follow && follow.config) || {};
  const sizing = cfg.sizing || { mode: 'fixed', value: { amount: 10 } };
  const form = el('form');
  form.appendChild(el('h2', { text: t('followForm.title') }));

  const channelSel = selectEl('channel', [
    ['tg', t('followForm.channelTg')],
    ['daemon', t('followForm.channelDaemon')],
  ], cfg.channel || follow?.channel || 'tg');
  const venueInput = el('input', { name: 'execute_venue', value: cfg.execute_venue || follow?.execute_venue || 'polymarket' });
  const modeSel = selectEl('sizing_mode', [
    ['fixed', t('followForm.sizingFixed')],
    ['proportional', t('followForm.sizingProportional')],
  ], sizing.mode || 'fixed');
  const initialMode = sizing.mode || 'fixed';
  const initialVal = initialMode === 'proportional'
    ? (sizing.value?.ratio ?? 0.5)
    : (sizing.value?.amount ?? sizing.value?.pct ?? 10);
  const sizingInput = el('input', { name: 'sizing_value', type: 'number', step: '0.01', min: '0', value: String(initialVal) });
  const sizingHint = el('p', { class: 'muted', text: t('followForm.sizingHint') });
  const sameVenueCb = el('input', { name: 'same_venue_only', type: 'checkbox', ...(cfg.same_venue_only ? { checked: 'checked' } : {}) });

  modeSel.onchange = () => {
    const mode = modeSel.value;
    const cur = Number(sizingInput.value);
    if (mode === 'proportional' && (!(cur > 0) || cur > 1)) sizingInput.value = '0.5';
    if (mode === 'fixed' && (!(cur > 0) || cur <= 1)) sizingInput.value = '10';
  };

  [field(t('followForm.channel'), channelSel),
   field(t('followForm.executeVenue'), venueInput),
   field(t('followForm.sizingMode'), modeSel),
   field(t('followForm.sizingValue'), sizingInput),
   sizingHint,
   field('', el('label', {}, [sameVenueCb, t('followForm.sameVenueOnly')])),
   field(t('followForm.maxOrder'), el('input', { name: 'max_order', type: 'number', step: '0.1', min: '0', value: String(cfg.max_notional_per_order || 0) })),
   field(t('followForm.dailyMax'), el('input', { name: 'daily_max', type: 'number', step: '0.1', min: '0', value: String(cfg.daily_max_notional || 0) })),
   field(t('followForm.maxOpen'), el('input', { name: 'max_open', type: 'number', step: '1', min: '0', value: String(cfg.max_open_positions || 0) })),
  ].forEach(n => form.appendChild(n));

  const errP = el('p', { class: 'error' });
  form.appendChild(errP);

  const actions = el('div', { class: 'modal-actions' });
  const cancelBtn = el('button', { text: t('common.cancel'), onclick: () => backdrop.remove() });
  const saveBtn = el('button', { class: 'primary', text: t('common.save') });
  actions.appendChild(cancelBtn); actions.appendChild(saveBtn);
  form.appendChild(actions);
  modal.appendChild(form);

  const q = (name) => form.querySelector(`[name="${name}"]`);

  saveBtn.onclick = async (e) => {
    e.preventDefault();
    errP.textContent = '';
    const mode = modeSel.value;
    const sv = Number(sizingInput.value || 0);
    if (!(sv > 0)) { errP.textContent = t('followForm.errorSizing'); return; }
    if (mode === 'proportional' && !(sv > 0 && sv <= 1)) {
      errP.textContent = t('followForm.errorProportional');
      return;
    }
    const sizingValue = mode === 'fixed' ? { amount: sv } : mode === 'proportional' ? { ratio: sv } : { pct: sv };
    const body = {
      execute_venue: venueInput.value.trim(),
      channel: channelSel.value,
      config: {
        sizing: { mode, value: sizingValue },
        execute_venue: venueInput.value.trim(),
        channel: channelSel.value,
        same_venue_only: sameVenueCb.checked,
        max_notional_per_order: numOr0(q('max_order')),
        daily_max_notional: numOr0(q('daily_max')),
        max_open_positions: intOr0(q('max_open')),
      },
    };
    try { await onSaved(body); backdrop.remove(); } catch (err) { errP.textContent = err.message; }
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
