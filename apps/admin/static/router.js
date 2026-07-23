// router.js · admin hash 路由 + 路由守卫。复用 web 模式。
// 安全修复 3.3：OIDC 回调只写 HttpOnly cookie，需 bootstrap 拉 /auth/me 填充 session 缓存。
import { isLoggedIn, setSession, clearSession } from './store/auth.js';

const routes = [];
let sessionReady = false;

export function route(pattern, render, guard) {
  routes.push({ pattern: pattern.split('/').filter(Boolean), render, guard });
}

/// 用 cookie session 探测登录态（OIDC 回调后 sessionStorage 为空）。
/// 故意不用 request()：未登录时 401 不应触发全局跳转。
async function bootstrapSession() {
  if (sessionReady) return;
  sessionReady = true;
  try {
    const resp = await fetch('/api/auth/me', { credentials: 'same-origin' });
    if (resp.ok) {
      const me = await resp.json();
      if (me && me.email) setSession({ email: me.email });
      return;
    }
  } catch {
    /* ignore */
  }
  // 无 cookie session 时保留 dev token 登录态
}

function matchRoute(path) {
  const parts = path.split('/').filter(Boolean);
  for (const r of routes) {
    if (r.pattern.length !== parts.length) continue;
    const params = {};
    let ok = true;
    for (let i = 0; i < r.pattern.length; i++) {
      if (r.pattern[i].startsWith(':')) {
        params[r.pattern[i].slice(1)] = decodeURIComponent(parts[i]);
      } else if (r.pattern[i] !== parts[i]) {
        ok = false; break;
      }
    }
    if (ok) return { route: r, params };
  }
  return null;
}

function currentPath() {
  const h = location.hash.slice(1) || '/';
  return h.startsWith('/') ? h : '/' + h;
}

export function navigate(path) {
  if (location.hash.slice(1) === path) { render(); return; }
  location.hash = path;
}

async function render() {
  await bootstrapSession();
  const path = currentPath();
  const m = matchRoute(path);
  const app = document.getElementById('app');
  if (!m) {
    app.textContent = '';
    const wrap = document.createElement('div');
    wrap.className = 'container';
    wrap.innerHTML = '<div class="empty"><div class="icon">404</div><p>页面不存在</p><p><a href="#/">返回首页</a></p></div>';
    app.appendChild(wrap);
    return;
  }
  if (m.route.guard === 'auth' && !isLoggedIn()) {
    navigate('/login');
    return;
  }
  if (m.route.guard === 'guest' && isLoggedIn()) {
    navigate('/');
    return;
  }
  app.textContent = '';
  const sk = document.createElement('div');
  sk.className = 'container';
  sk.innerHTML = '<div class="skeleton line"></div><div class="skeleton line"></div><div class="skeleton block"></div>';
  app.appendChild(sk);
  try {
    const node = await m.route.render({ params: m.params, path });
    app.textContent = '';
    if (node) app.appendChild(node);
    window.scrollTo(0, 0);
  } catch (e) {
    app.textContent = '';
    const card = document.createElement('div');
    card.className = 'container';
    const inner = document.createElement('div');
    inner.className = 'card';
    const h2 = document.createElement('h2');
    h2.textContent = '加载失败';
    const p = document.createElement('p');
    p.className = 'neg';
    p.textContent = String(e.message || e);
    const back = document.createElement('p');
    back.innerHTML = '<a href="#/">返回首页</a>';
    inner.appendChild(h2);
    inner.appendChild(p);
    inner.appendChild(back);
    card.appendChild(inner);
    app.appendChild(card);
  }
}

export function startRouter() {
  window.addEventListener('hashchange', render);
  window.addEventListener('auth:401', () => {
    clearSession();
    navigate('/login');
  });
  render();
}

export function isActive(path) {
  const cur = currentPath();
  if (path === '/') return cur === '/';
  return cur === path || cur.startsWith(path + '/');
}
