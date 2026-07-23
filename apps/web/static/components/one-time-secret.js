// components/one-time-secret.js · 明文一次性弹窗（强制确认已保存）。对应 docs/FRONTEND_DESIGN.md §8。
import { el } from './ui.js';
import { t } from '../i18n/index.js';
import { copyText } from '../lib/clipboard.js';

/// openOneTimeSecretModal({ title, value, warn })
export function openOneTimeSecretModal({ title = t('oneTimeSecret.defaultTitle'), value, warn } = {}) {
  const backdrop = el('div', { class: 'modal-backdrop' });
  const modal = el('div', { class: 'modal' });
  modal.appendChild(el('h2', { text: title }));
  modal.appendChild(el('p', { class: 'neg', text: warn || t('oneTimeSecret.defaultWarn') }));
  const code = el('code', { class: 'one-time-value', text: String(value) });
  modal.appendChild(code);
  modal.appendChild(el('button', {
    class: 'sm',
    style: 'margin-top:8px',
    text: t('common.copy'),
    onclick: () => copyText(String(value)),
  }));
  const cb = el('input', { type: 'checkbox' });
  const cbLabel = el('label', { class: 'ots-confirm' }, [cb, document.createTextNode(t('oneTimeSecret.confirmSaved'))]);
  modal.appendChild(cbLabel);
  const closeBtn = el('button', { class: 'primary', disabled: true, text: t('oneTimeSecret.close'), onclick: () => { if (cb.checked) backdrop.remove(); } });
  cb.addEventListener('change', () => { closeBtn.disabled = !cb.checked; });
  modal.appendChild(closeBtn);
  backdrop.appendChild(modal);
  document.body.appendChild(backdrop);
}
