// components/follow-form.js · 跟随配置表单（模态）。对应 docs/FRONTEND_DESIGN.md §6.9/§6.10。
// 复用于：我的跟随页[编辑]模态、创建跟随页（页面形式另写，字段一致）。
// props: { follow: FollowRelation, onSaved: async (body) => any }
// 返回 HTMLElement（模态根）；自行挂载到 document.body。
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

  const channelF = field(t('followForm.channel'), selectEl('channel', [
    ['tg', t('followForm.channelTg')],
    ['daemon', t('followForm.channelDaemon')],
  ], cfg.channel || follow?.channel || 'tg'));

  const venueF = field(t('followForm.executeVenue'), el('input', { id: 'execute_venue', value: cfg.execute_venue || follow?.execute_venue || 'polymarket' }));

  const sizingModeF = field('sizing mode', selectEl('sizing_mode', [
    ['fixed', t('followForm.sizingFixed')],
    ['proportional', t('followForm.sizingProportional')],
  ], sizing.mode || 'fixed'));

  const sizingValF = field(t('followForm.sizingValue'), el('input', { id: 'sizing_value', type: 'number', step: '0.01', min: '0', value: String(sizing.value?.amount ?? sizing.value?.ratio ?? sizing.value?.pct ?? 10) }));
  const sizingHint = el('p', { class: 'muted', text: t('followForm.sizingHint') });
  const sameVenueF = field('', el('label', {}, [el('input', { id: 'same_venue_only', type: 'checkbox', ...(cfg.same_venue_only ? { checked: 'checked' } : {}) }), t('followForm.sameVenueOnly')]));

  const maxOrderF = field(t('followForm.maxOrder'), el('input', { id: 'max_order', type: 'number', step: '0.1', min: '0', value: String(cfg.max_notional_per_order || 0) }));
  const dailyF = field(t('followForm.dailyMax'), el('input', { id: 'daily_max', type: 'number', step: '0.1', min: '0', value: String(cfg.daily_max_notional || 0) }));
  const maxOpenF = field(t('followForm.maxOpen'), el('input', { id: 'max_open', type: 'number', step: '1', min: '0', value: String(cfg.max_open_positions || 0) }));

  const errP = el('p', { class: 'error' });
  [channelF, venueF, sizingModeF, sizingValF, sizingHint, sameVenueF, maxOrderF, dailyF, maxOpenF, errP].forEach(n => form.appendChild(n));

  const actions = el('div', { class: 'modal-actions' });
  const cancelBtn = el('button', { text: t('common.cancel'), onclick: () => backdrop.remove() });
  const saveBtn = el('button', { class: 'primary', text: t('common.save') });
  actions.appendChild(cancelBtn); actions.appendChild(saveBtn);
  form.appendChild(actions);
  modal.appendChild(form);

  saveBtn.onclick = async (e) => {
    e.preventDefault();
    errP.textContent = '';
    const mode = document.getElementById('sizing_mode').value;
    const sv = Number(document.getElementById('sizing_value').value || 0);
    if (!(sv > 0)) { errP.textContent = t('followForm.errorSizing'); return; }
    const sizingValue = mode === 'fixed' ? { amount: sv } : mode === 'proportional' ? { ratio: sv } : { pct: sv };
    const body = {
      execute_venue: document.getElementById('execute_venue').value.trim(),
      channel: document.getElementById('channel').value,
      config: {
        sizing: { mode, value: sizingValue },
        execute_venue: document.getElementById('execute_venue').value.trim(),
        channel: document.getElementById('channel').value,
        same_venue_only: document.getElementById('same_venue_only').checked,
        max_notional_per_order: numOr0('max_order'),
        daily_max_notional: numOr0('daily_max'),
        max_open_positions: intOr0('max_open'),
      },
    };
    try { await onSaved(body); backdrop.remove(); } catch (err) { errP.textContent = err.message; }
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
