// store/theme.js · 白天/黑夜主题；localStorage 持久化，缺省跟系统偏好。
export const THEME_STORAGE_KEY = 'sharpside-theme';

/** @returns {'dark' | 'light'} */
export function getTheme() {
  if (typeof document === 'undefined') return 'dark';
  return document.documentElement.classList.contains('light') ? 'light' : 'dark';
}

function systemTheme() {
  try {
    return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
  } catch {
    return 'dark';
  }
}

function readStored() {
  try {
    const v = localStorage.getItem(THEME_STORAGE_KEY);
    if (v === 'light' || v === 'dark') return v;
  } catch { /* ignore */ }
  return null;
}

/** @param {'dark' | 'light'} theme */
export function applyTheme(theme) {
  const next = theme === 'light' ? 'light' : 'dark';
  const root = document.documentElement;
  root.classList.toggle('light', next === 'light');
  root.classList.toggle('dark', next === 'dark');
  root.style.colorScheme = next;
  try {
    localStorage.setItem(THEME_STORAGE_KEY, next);
  } catch { /* ignore */ }
  return next;
}

export function toggleTheme() {
  return applyTheme(getTheme() === 'dark' ? 'light' : 'dark');
}

/** 启动时应用已存偏好，否则跟系统。 */
export function initTheme() {
  return applyTheme(readStored() || systemTheme());
}
