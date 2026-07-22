// components/one-time-secret.js · 明文一次性弹窗（强制确认已保存）。对应 docs/FRONTEND_DESIGN.md §8。
// 安全关键交互：显示明文 + [复制] + 强制勾选 [我已妥善保存] 才能 [关闭]；不允许点背景关闭。
import { el } from './ui.js';
import { toast } from '../store/toast.js';
import { t } from '../i18n/index.js';

/// openOneTimeSecretModal({ title, value, warn })
/// title: 弹窗标题；value: 明文；warn: 红字警示语（可选，默认"请立即妥善保存，关闭后无法再次查看"）。
export function openOneTimeSecretModal({ title = t('oneTimeSecret.defaultTitle'), value, warn } = {}) {
  const backdrop = el('div', { class: 'modal-backdrop' });
  const modal = el('div', { class: 'modal' });
  modal.appendChild(el('h2', { text: title }));
  modal.appendChild(el('p', { class: 'neg', text: warn || t('oneTimeSecret.defaultWarn') }));
  const code = el('code', { class: 'one-time-value', text: String(value) });
  modal.appendChild(code);
  modal.appendChild(el('button', { class: 'sm', style: 'margin-top:8px', text: t('common.copy'), onclick: () => {
    navigator.clipboard?.writeText(String(value)); toast(t('common.copied'), 'success');
  } }));
  const cb = el('input', { type: 'checkbox' });
  const cbLabel = el('label', { class: 'ots-confirm' }, [cb, document.createTextNode(t('oneTimeSecret.confirmSaved'))]);
  modal.appendChild(cbLabel);
  const closeBtn = el('button', { class: 'primary', disabled: true, text: t('oneTimeSecret.close'), onclick: () => { if (cb.checked) backdrop.remove(); } });
  cb.addEventListener('change', () => { closeBtn.disabled = !cb.checked; });
  modal.appendChild(closeBtn);
  backdrop.appendChild(modal);
  // 不允许点背景关闭（强制勾选）
  document.body.appendChild(backdrop);
}
