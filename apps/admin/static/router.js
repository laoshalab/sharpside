// router.js · admin hash 路由 + 路由守卫。复用 web 模式。
import { isLoggedIn } from './store/auth.js';

const routes = [];

export function route(pattern, render, guard) {
  routes.push({ pattern: pattern.split('/').filter(Boolean), render, guard });
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
  const path = currentPath();
  const m = matchRoute(path);
  const app = document.getElementById('app');
  if (!m) {
    app.innerHTML = '<div class="container"><div class="empty"><div class="icon">404</div><p>页面不存在</p><p><a href="#/">返回首页</a></p></div></div>';
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
  app.innerHTML = '<div class="container"><div class="skeleton line"></div><div class="skeleton line"></div><div class="skeleton block"></div></div>';
  try {
    const node = await m.route.render({ params: m.params, path });
    app.innerHTML = '';
    if (node) app.appendChild(node);
    window.scrollTo(0, 0);
  } catch (e) {
    app.innerHTML = `<div class="container"><div class="card"><h2>加载失败</h2><p class="neg">${e.message || e}</p><p><a href="#/">返回首页</a></p></div></div>`;
  }
}

export function startRouter() {
  window.addEventListener('hashchange', render);
  window.addEventListener('auth:401', () => navigate('/login'));
  render();
}

export function isActive(path) {
  const cur = currentPath();
  if (path === '/') return cur === '/';
  return cur === path || cur.startsWith(path + '/');
}
