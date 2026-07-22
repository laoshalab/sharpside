// store/auth.js · admin token 持久化。对应 docs/FRONTEND_DESIGN.md §7.1。
// MVP 单一共享 admin token（ADMIN_TOKEN env）；生产接 SSO/OIDC。
const TOKEN_KEY = 'sharpside_admin_token';

export function getToken() {
  try { return localStorage.getItem(TOKEN_KEY); } catch { return null; }
}

export function setToken(t) {
  try { localStorage.setItem(TOKEN_KEY, t); } catch {}
}

export function clearToken() {
  try { localStorage.removeItem(TOKEN_KEY); } catch {}
}

export function isLoggedIn() {
  return !!getToken();
}
