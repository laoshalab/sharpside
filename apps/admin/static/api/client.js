// api/client.js · admin fetch 封装：/api 前缀 + cookie 会话 + 401 全局事件。
// 对应 docs/FRONTEND_DESIGN.md §7.1 / 安全修复 3.3。

import { getDevToken, clearSession } from '../store/auth.js';
import { toast } from '../store/toast.js';

const BASE = '/api';

export class ApiError extends Error {
  constructor(message, status, body) {
    super(message);
    this.status = status;
    this.body = body;
  }
}

export async function request(path, { method = 'GET', body, headers = {} } = {}) {
  const url = BASE + path;
  const h = { ...headers };
  // 安全修复 3.3：默认靠 HttpOnly session cookie（credentials: same-origin）。
  // Dev 无 OIDC 时才附带 Bearer ADMIN_TOKEN。
  const token = getDevToken();
  if (token) h['Authorization'] = 'Bearer ' + token;
  if (body !== undefined && !h['Content-Type']) h['Content-Type'] = 'application/json';
  const opt = { method, headers: h, credentials: 'same-origin' };
  if (body !== undefined) opt.body = JSON.stringify(body);

  let resp;
  try {
    resp = await fetch(url, opt);
  } catch (e) {
    throw new ApiError('网络错误：' + e.message, 0);
  }

  if (resp.status === 401) {
    clearSession();
    window.dispatchEvent(new CustomEvent('auth:401'));
    throw new ApiError('admin session 无效或已过期', 401);
  }

  const text = await resp.text();
  let parsed;
  try { parsed = text ? JSON.parse(text) : null; } catch { parsed = text; }

  if (!resp.ok) {
    const msg = (parsed && (parsed.error || parsed.message)) || `HTTP ${resp.status}`;
    if (resp.status >= 500) toast('服务暂不可用，请稍后重试', 'error');
    throw new ApiError(msg, resp.status, parsed);
  }
  return parsed;
}

export const get = (p, opt) => request(p, { ...opt, method: 'GET' });
export const post = (p, body, opt) => request(p, { ...opt, method: 'POST', body });
export const put = (p, body, opt) => request(p, { ...opt, method: 'PUT', body });
export const patch = (p, body, opt) => request(p, { ...opt, method: 'PATCH', body });
export const del = (p, opt) => request(p, { ...opt, method: 'DELETE' });

export function qs(params) {
  const s = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined || v === null || v === '') continue;
    s.set(k, v);
  }
  const str = s.toString();
  return str ? '?' + str : '';
}
