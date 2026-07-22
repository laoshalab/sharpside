// router.js · hash 路由 + 路由守卫。对应 docs/FRONTEND_DESIGN.md §8。
// F0 用 hash 路由（#/path），无需服务端配合；F1 迁 path 路由。
import { isLoggedIn } from './store/auth.js';
import { t } from './i18n/index.js';

const routes = []; // { pattern, render, guard }

/// 注册路由。pattern 支持 :param 段，render(ctx) 返回 HTMLElement 或 Promise<HTMLElement>。
/// guard: 'auth' | 'guest' | undefined
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
  // decodeURIComponent：兼容浏览器/用户把 hash 中的 `/` 编码为 %2F 的情况，
  // 否则 `/traders/p/a` 会被当成单段路径导致 404。
  const h = decodeURIComponent(location.hash.slice(1) || '/');
  // 剥离 hash 内的 query string（?key=val），否则带参数的 URL（如
  // #/leaderboard?period=1m&sort=roi）会被当成单段路径导致路由匹配失败 404。
  const path = h.split('?')[0];
  return path.startsWith('/') ? path : '/' + path;
}

export function navigate(path) {
  if (location.hash.slice(1) === path) { render(); return; }
  location.hash = path;
}

/** 强制按当前 hash 重渲染（语言切换等）。 */
export function remount() {
  return render();
}

async function render() {
  const path = currentPath();
  const m = matchRoute(path);
  const app = document.getElementById('app');
  if (!m) {
    app.innerHTML = `<div class="container"><div class="empty"><div class="icon">404</div><p>${t('common.notFound')}</p><p><a href="#/">${t('common.backHome')}</a></p></div></div>`;
    return;
  }
  // 守卫：未登录仍停留在目标路径，展示连接入口（菜单可点）
  if (m.route.guard === 'auth' && !isLoggedIn()) {
    const [{ withShell }, { el }, { connectWalletFlow }, { toast }] = await Promise.all([
      import('./components/nav.js'),
      import('./components/ui.js'),
      import('./lib/wallet-connect.js'),
      import('./store/toast.js'),
    ]);
    const link = el('a', {
      href: '#/connect',
      class: 'auth-gate-link',
      text: t('nav.connectWallet'),
    });
    link.onclick = async (e) => {
      e.preventDefault();
      if (link.dataset.busy === '1') return;
      link.dataset.busy = '1';
      try {
        await connectWalletFlow();
        remount();
      } catch (err) {
        toast(err.message || t('common.connectFailed'), 'error');
      } finally {
        link.dataset.busy = '0';
      }
    };
    const gate = el('div', { class: 'container' }, [
      el('div', { class: 'empty auth-gate' }, [
        el('p', { text: t('common.loginRequired') }),
        el('p', { class: 'muted', text: t('common.loginRequiredHint') }),
        el('p', {}, [link]),
      ]),
    ]);
    app.innerHTML = '';
    app.appendChild(withShell(gate));
    window.scrollTo(0, 0);
    return;
  }
  if (m.route.guard === 'guest' && isLoggedIn()) {
    navigate('/');
    return;
  }
  // loading
  app.innerHTML = '<div class="container"><div class="skeleton line"></div><div class="skeleton line"></div><div class="skeleton block"></div></div>';
  try {
    const node = await m.route.render({ params: m.params, path });
    app.innerHTML = '';
    if (node) app.appendChild(node);
    window.scrollTo(0, 0);
  } catch (e) {
    app.innerHTML = `<div class="container"><div class="card"><h2>${t('common.loadFailed')}</h2><p class="neg">${e.message || e}</p><p><a href="#/">${t('common.backHome')}</a></p></div></div>`;
  }
}

export function startRouter() {
  window.addEventListener('hashchange', render);
  window.addEventListener('auth:401', () => {
    navigate('/');
    import('./lib/wallet-connect.js').then(({ connectWalletFlow }) => {
      queueMicrotask(() => connectWalletFlow().catch(() => {}));
    });
  });
  render();
}

/// 当前激活路径（用于导航高亮）。
export function isActive(path) {
  const cur = currentPath();
  if (path === '/') return cur === '/';
  return cur === path || cur.startsWith(path + '/');
}
