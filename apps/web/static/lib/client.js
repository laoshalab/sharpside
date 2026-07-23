// api/client.js · fetch 封装：/api 前缀 + 401 全局事件 + 错误归一化。
// 对应 docs/FRONTEND_DESIGN.md §8 状态管理与鉴权。
//
// 安全修复 3.1：JWT 由 HttpOnly cookie 携带，不再注入 Authorization 头。
// 同源请求默认带 cookie；显式 credentials:'same-origin' 以防未来跨域变动丢 cookie。

import { clearUser } from '../store/auth.js';
import { toast } from '../store/toast.js';
import { t } from '../i18n/index.js';

const BASE = '/api';

export class ApiError extends Error {
  constructor(message, status, body) {
    super(message);
    this.status = status;
    this.body = body;
  }
}

/// 通用请求：归一化错误，401 触发全局事件。
/// path 不含 /api 前缀（如 '/venue-hub/traders'）。
export async function request(path, { method = 'GET', body, headers = {}, raw = false } = {}) {
  const url = BASE + path;
  const h = { ...headers };
  if (body !== undefined && !(body instanceof FormData) && !h['Content-Type']) {
    h['Content-Type'] = 'application/json';
  }
  const opt = { method, headers: h, credentials: 'same-origin' };
  if (body !== undefined) opt.body = body instanceof FormData ? body : JSON.stringify(body);

  let resp;
  try {
    resp = await fetch(url, opt);
  } catch (e) {
    throw new ApiError(t('errors.network', { message: e.message }), 0);
  }

  if (resp.status === 401) {
    clearUser();
    window.dispatchEvent(new CustomEvent('auth:401'));
    throw new ApiError(t('errors.sessionExpired'), 401);
  }

  const text = await resp.text();
  let parsed;
  try { parsed = text ? JSON.parse(text) : null; } catch { parsed = text; }

  if (!resp.ok) {
    const msg = (parsed && (parsed.error || parsed.message)) || `HTTP ${resp.status}`;
    // 仅对真实内部错误弹全局 toast。502/503/504 多为上游短暂不可用，
    // 页面已有降级文案，避免重复惊吓。
    if (resp.status === 500) toast(t('errors.serviceUnavailable'), 'error');
    throw new ApiError(msg, resp.status, parsed);
  }
  return raw ? resp : parsed;
}

export const get = (p, opt) => request(p, { ...opt, method: 'GET' });
export const post = (p, body, opt) => request(p, { ...opt, method: 'POST', body });
export const patch = (p, body, opt) => request(p, { ...opt, method: 'PATCH', body });
export const del = (p, opt) => request(p, { ...opt, method: 'DELETE' });

/// 拼接 query string。skip undefined/null。
export function qs(params) {
  const s = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined || v === null || v === '') continue;
    s.set(k, v);
  }
  const str = s.toString();
  return str ? '?' + str : '';
}
