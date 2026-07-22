// components/language-switcher.js · 顶栏 10 语切换。
import { el } from './ui.js';
import { getLocale, setLocale, locales, localeNames, t } from '../i18n/index.js';

export function languageSwitcher() {
  const wrap = el('div', { class: 'lang-switch' });
  const btn = el('button', {
    type: 'button',
    class: 'sm ghost lang-switch-btn',
    'aria-label': t('nav.language'),
    'aria-haspopup': 'listbox',
    'aria-expanded': 'false',
    html: `<span class="lang-switch-icon" aria-hidden="true">🌐</span><span class="lang-switch-label">${localeNames[getLocale()]}</span>`,
  });
  const panel = el('div', {
    class: 'lang-switch-panel',
    role: 'listbox',
    'aria-label': t('nav.language'),
  });

  for (const loc of locales) {
    const opt = el('button', {
      type: 'button',
      class: 'lang-switch-option' + (loc === getLocale() ? ' active' : ''),
      role: 'option',
      'aria-selected': loc === getLocale() ? 'true' : 'false',
      text: localeNames[loc],
      onclick: (e) => {
        e.stopPropagation();
        setLocale(loc);
        close();
      },
    });
    panel.appendChild(opt);
  }

  function close() {
    wrap.classList.remove('open');
    btn.setAttribute('aria-expanded', 'false');
  }

  function toggle() {
    const open = wrap.classList.toggle('open');
    btn.setAttribute('aria-expanded', open ? 'true' : 'false');
  }

  btn.onclick = (e) => {
    e.stopPropagation();
    toggle();
  };

  const onDoc = (e) => {
    if (!wrap.contains(e.target)) close();
  };
  document.addEventListener('mousedown', onDoc);
  // 页面 remount 时旧节点会脱离 DOM；一次性清理监听避免泄漏。
  const obs = new MutationObserver(() => {
    if (!document.body.contains(wrap)) {
      document.removeEventListener('mousedown', onDoc);
      obs.disconnect();
    }
  });
  obs.observe(document.body, { childList: true, subtree: true });

  wrap.appendChild(btn);
  wrap.appendChild(panel);
  return wrap;
}
