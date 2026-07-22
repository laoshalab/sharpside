// components/theme-switcher.js · 顶栏白天/黑夜切换。
import { el } from './ui.js';
import { getTheme, toggleTheme } from '../store/theme.js';
import { t } from '../i18n/index.js';

const ICON_SUN = '<svg class="theme-switch-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41"/></svg>';
const ICON_MOON = '<svg class="theme-switch-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 14.5A8.5 8.5 0 1 1 9.5 3a7 7 0 0 0 11.5 11.5z"/></svg>';

function iconFor(theme) {
  // 暗色时显示太阳（点一下切白天）；亮色时显示月亮
  return theme === 'dark' ? ICON_SUN : ICON_MOON;
}

export function themeSwitcher() {
  const btn = el('button', {
    type: 'button',
    class: 'sm ghost theme-switch-btn',
    'aria-label': t('nav.toggleTheme'),
    title: t('nav.toggleTheme'),
    html: iconFor(getTheme()),
  });
  btn.onclick = () => {
    const next = toggleTheme();
    btn.innerHTML = iconFor(next);
  };
  return btn;
}
