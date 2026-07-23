// store/auth.js · 会话状态缓存。对应 docs/FRONTEND_DESIGN.md §8。
//
// 安全修复 3.1：JWT 不再存 localStorage（XSS 可窃），改由 HttpOnly cookie 携带（JS 不可读）。
// 此处仅缓存**非密**的用户对象（user profile）用于 UI 状态（登录态展示 / 路由 guard），
// 不存任何 token。登录态以 cookie 为准：401 时清 user 缓存并触发重登。
const USER_KEY = 'sharpside_user';

export function getUser() {
  try { return JSON.parse(localStorage.getItem(USER_KEY) || 'null'); } catch { return null; }
}

export function setUser(u) {
  try { localStorage.setItem(USER_KEY, JSON.stringify(u)); } catch {}
}

export function clearUser() {
  try { localStorage.removeItem(USER_KEY); } catch {}
}

/// 是否已登录（依据本地 user 缓存；真权威是 cookie，401 会清缓存）。
export function isLoggedIn() {
  return !!getUser();
}

// ── 向后兼容别名（调用方仍可用旧名，语义改为操作 user 缓存）──
export const clearToken = clearUser;
