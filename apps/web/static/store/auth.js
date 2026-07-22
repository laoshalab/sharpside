// store/auth.js · JWT 持久化 + 当前用户缓存。对应 docs/FRONTEND_DESIGN.md §8。
const TOKEN_KEY = 'sharpside_token';
const USER_KEY = 'sharpside_user';

export function getToken() {
  try { return localStorage.getItem(TOKEN_KEY); } catch { return null; }
}

export function setToken(t) {
  try { localStorage.setItem(TOKEN_KEY, t); } catch {}
}

export function clearToken() {
  try { localStorage.removeItem(TOKEN_KEY); localStorage.removeItem(USER_KEY); } catch {}
}

export function getUser() {
  try { return JSON.parse(localStorage.getItem(USER_KEY) || 'null'); } catch { return null; }
}

export function setUser(u) {
  try { localStorage.setItem(USER_KEY, JSON.stringify(u)); } catch {}
}

export function isLoggedIn() {
  return !!getToken();
}
