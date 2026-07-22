// i18n/index.js · 轻量字典 i18n（cookie/localStorage + 10 语）。
import {
  locales,
  defaultLocale,
  STORAGE_KEY,
  localeNames,
  rtlLocales,
} from './config.js';
import zh from './messages/zh.js';
import en from './messages/en.js';
import ja from './messages/ja.js';
import ko from './messages/ko.js';
import es from './messages/es.js';
import fr from './messages/fr.js';
import de from './messages/de.js';
import pt from './messages/pt.js';
import ru from './messages/ru.js';
import ar from './messages/ar.js';

const catalogs = { zh, en, ja, ko, es, fr, de, pt, ru, ar };
const listeners = new Set();
let current = defaultLocale;

function readStored() {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v && locales.includes(v)) return v;
  } catch { /* ignore */ }
  return defaultLocale;
}

function lookup(catalog, key) {
  const parts = key.split('.');
  let cur = catalog;
  for (const p of parts) {
    if (cur == null || typeof cur !== 'object') return undefined;
    cur = cur[p];
  }
  return typeof cur === 'string' ? cur : undefined;
}

export function getLocale() {
  return current;
}

export function t(key, params) {
  let s = lookup(catalogs[current], key);
  // 缺键时：当前语 → en → 默认 zh，避免半翻页面回落成中文。
  if (s == null && current !== 'en') s = lookup(catalogs.en, key);
  if (s == null) s = lookup(catalogs[defaultLocale], key);
  if (s == null) s = key;
  if (params) {
    s = s.replace(/\{(\w+)\}/g, (_, k) => (params[k] != null ? String(params[k]) : ''));
  }
  return s;
}

export function applyDocumentLocale(locale = current) {
  if (typeof document === 'undefined') return;
  document.documentElement.lang = locale;
  document.documentElement.dir = rtlLocales.includes(locale) ? 'rtl' : 'ltr';
  document.title = t('meta.title');
}

export function setLocale(locale) {
  if (!locales.includes(locale) || locale === current) return;
  current = locale;
  try {
    localStorage.setItem(STORAGE_KEY, locale);
  } catch { /* ignore */ }
  applyDocumentLocale(locale);
  for (const fn of listeners) {
    try { fn(locale); } catch { /* ignore */ }
  }
}

export function onLocaleChange(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function initI18n() {
  current = readStored();
  applyDocumentLocale(current);
  return current;
}

export { locales, localeNames, defaultLocale, rtlLocales };
