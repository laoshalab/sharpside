// store/auth.js · admin 会话状态。对应 docs/FRONTEND_DESIGN.md §7.1。
//
// 安全修复 3.3：生产走 OIDC session cookie（HttpOnly，JS 不可读）；
// 此处仅缓存「是否已登录」标志 + 可选 email（非密），不存任何 token。
// Dev 无 OIDC 时仍可用 ADMIN_TOKEN 填入 localStorage（Bearer 回退）。

const SESSION_KEY = 'sharpside_admin_session';
const DEV_TOKEN_KEY = 'sharpside_admin_token';

export function getSession() {
  try { return JSON.parse(sessionStorage.getItem(SESSION_KEY) || 'null'); } catch { return null; }
}

export function setSession(s) {
  try { sessionStorage.setItem(SESSION_KEY, JSON.stringify(s)); } catch {}
}

export function clearSession() {
  try { sessionStorage.removeItem(SESSION_KEY); } catch {}
  try { localStorage.removeItem(DEV_TOKEN_KEY); } catch {}
}

/// Dev 回退：仅非 OIDC 时使用。
export function getDevToken() {
  try { return localStorage.getItem(DEV_TOKEN_KEY); } catch { return null; }
}

export function setDevToken(t) {
  try {
    if (t) localStorage.setItem(DEV_TOKEN_KEY, t);
    else localStorage.removeItem(DEV_TOKEN_KEY);
  } catch {}
}

export function isLoggedIn() {
  return !!getSession() || !!getDevToken();
}

// 向后兼容
export const clearToken = clearSession;
export const getToken = getDevToken;
export const setToken = setDevToken;
