// components/nav.js · 分组顶部导航。对应 docs/FRONTEND_DESIGN.md §4 IA。
// 桌面顶栏：发现 / 跟单 / 组合 / 设置（下拉）；移动端底栏 4 主入口。
import { el } from './ui.js';
import { isLoggedIn, clearToken } from '../store/auth.js';
import { navigate, isActive } from '../router.js';
import { listWallets } from '../lib/account.js';
import { connectWalletFlow } from '../lib/wallet-connect.js';
import { toast } from '../store/toast.js';
import { t } from '../i18n/index.js';
import { languageSwitcher } from './language-switcher.js';
import { themeSwitcher } from './theme-switcher.js';

/** 信息架构：顶层分组 + 子页（未登录也展示；进页由路由提示连接钱包） */
function navGroups() {
  return [
    {
      id: 'discover',
      label: t('nav.discover'),
      items: [
        { href: '#/', label: t('nav.home'), match: '/' },
        { href: '#/leaderboard', label: t('nav.leaderboard'), match: '/leaderboard', also: ['/traders'] },
        { href: '#/watchlist', label: t('nav.watchlist'), match: '/watchlist' },
      ],
    },
    {
      id: 'copy',
      label: t('nav.copy'),
      items: [
        { href: '#/follows', label: t('nav.follows'), match: '/follows', also: ['/copy-history'] },
      ],
    },
    {
      id: 'portfolio',
      label: t('nav.portfolio'),
      items: [
        { href: '#/dashboard', label: t('nav.dashboard'), match: '/dashboard' },
        { href: '#/portfolio', label: t('nav.portfolioPage'), match: '/portfolio' },
        { href: '#/wallet', label: t('nav.wallet'), match: '/wallet' },
      ],
    },
    {
      id: 'account',
      label: t('nav.account'),
      items: [
        { href: '#/settings', label: t('nav.settings'), match: '/settings' },
      ],
    },
  ];
}

/** 移动端底栏主入口（未登录也显示全部） */
function bottomTabs() {
  return [
    { href: '#/leaderboard', label: t('nav.discover'), match: '/leaderboard', also: ['/', '/watchlist', '/traders'] },
    { href: '#/follows', label: t('nav.copy'), match: '/follows', also: ['/copy-history'] },
    { href: '#/dashboard', label: t('nav.portfolio'), match: '/dashboard', also: ['/portfolio', '/wallet'] },
    { href: '#/settings', label: t('nav.account'), match: '/settings' },
  ];
}

function pathActive(match, also = []) {
  if (match === '/') return isActive('/');
  if (isActive(match)) return true;
  return also.some(p => isActive(p));
}

function groupActive(group) {
  return group.items.some(item => pathActive(item.match, item.also || []));
}

function primaryHref(group) {
  const hit = group.items.find(item => pathActive(item.match, item.also || []));
  return (hit || group.items[0]).href;
}

function linkEl(item) {
  const active = pathActive(item.match, item.also || []);
  return el('a', {
    href: item.href,
    class: 'nav-dd-link' + (active ? ' active' : ''),
    text: item.label,
  });
}

function buildTopNav() {
  const bar = el('header', { class: 'top-nav', 'aria-label': t('nav.ariaMain') });
  const inner = el('div', { class: 'container top-nav-inner' });

  inner.appendChild(el('a', {
    class: 'brand',
    href: '#/',
    html: '<span class="brand-mark">◈</span><span class="brand-text">Sharpside</span>',
  }));

  const menu = el('nav', { class: 'top-nav-menu' });
  for (const group of navGroups()) {
    // 单入口分组：直接链接，不下拉
    if (group.items.length === 1) {
      const only = group.items[0];
      menu.appendChild(el('a', {
        href: only.href,
        class: 'top-nav-item' + (pathActive(only.match, only.also || []) ? ' active' : ''),
        text: group.label,
      }));
      continue;
    }
    const drop = el('div', { class: 'nav-drop' + (groupActive(group) ? ' active' : '') });
    const trigger = el('a', {
      href: primaryHref(group),
      class: 'top-nav-item' + (groupActive(group) ? ' active' : ''),
      html: `${group.label}<span class="nav-caret" aria-hidden="true"></span>`,
    });
    const panel = el('div', { class: 'nav-dd', role: 'menu' });
    for (const item of group.items) panel.appendChild(linkEl(item));
    drop.appendChild(trigger);
    drop.appendChild(panel);
    menu.appendChild(drop);
  }
  inner.appendChild(menu);
  inner.appendChild(el('div', { class: 'spacer' }));

  const actions = el('div', { class: 'top-nav-actions' });
  actions.appendChild(themeSwitcher());
  actions.appendChild(languageSwitcher());
  if (isLoggedIn()) {
    const tag = el('span', { class: 'wallet-chip', text: t('nav.connected') });
    actions.appendChild(tag);
    listWallets().then(ws => {
      const primary = (ws || []).find(w => w.is_primary) || (ws || [])[0];
      if (primary) {
        const a = primary.address || '';
        tag.textContent = a.slice(0, 6) + '…' + a.slice(-4);
      }
    }).catch(() => {});
    actions.appendChild(el('button', {
      class: 'sm ghost',
      text: t('nav.disconnect'),
      onclick: () => { clearToken(); navigate('/'); },
    }));
  } else {
    const btn = el('button', { class: 'sm primary nav-connect', text: t('nav.connectWallet') });
    btn.onclick = async () => {
      btn.disabled = true;
      try {
        await connectWalletFlow();
      } catch (e) {
        toast(e.message || t('common.connectFailed'), 'error');
      } finally {
        btn.disabled = false;
      }
    };
    actions.appendChild(btn);
  }
  inner.appendChild(actions);
  bar.appendChild(inner);
  return bar;
}

function buildBottomNav() {
  const bar = el('nav', { class: 'bottom-nav', 'aria-label': t('nav.ariaMobile') });
  for (const tab of bottomTabs()) {
    const active = pathActive(tab.match, tab.also || []);
    bar.appendChild(el('a', {
      href: tab.href,
      class: 'bottom-tab' + (active ? ' active' : ''),
      html: `<span class="bottom-tab-label">${tab.label}</span>`,
    }));
  }
  if (!isLoggedIn()) {
    const btn = el('button', { class: 'bottom-tab bottom-tab-action', text: t('nav.connectShort') });
    btn.onclick = async () => {
      try {
        await connectWalletFlow();
      } catch (e) {
        toast(e.message || t('common.connectFailed'), 'error');
      }
    };
    bar.appendChild(btn);
  }
  return bar;
}

/** 页脚「联系我们」外链；上线前换成真实账号。 */
const CONTACT_LINKS = [
  { href: 'https://t.me/laoshalab', label: 'TG' },
  { href: 'https://x.com/laoshalab', label: 'X' },
  { href: 'https://discord.gg/sharpside', label: 'Discord' },
];

function buildFooter() {
  const year = new Date().getFullYear();
  const links = [
    { href: '#/', label: t('nav.home') },
    { href: '#/leaderboard', label: t('nav.leaderboard') },
    { href: '#/follows', label: t('nav.follows') },
    { href: '#/dashboard', label: t('nav.dashboard') },
    { href: '#/settings', label: t('nav.settings') },
  ];

  const nav = el('nav', { class: 'app-footer-nav', 'aria-label': t('footer.ariaNav') });
  for (const item of links) {
    nav.appendChild(el('a', { href: item.href, text: item.label }));
  }

  const contact = el('nav', { class: 'app-footer-contact', 'aria-label': t('footer.ariaContact') }, [
    el('p', { class: 'app-footer-contact-title', text: t('footer.contact') }),
    el('div', { class: 'app-footer-contact-links' }, CONTACT_LINKS.map((item) =>
      el('a', {
        href: item.href,
        text: item.label,
        target: '_blank',
        rel: 'noopener noreferrer',
      }),
    )),
  ]);

  return el('footer', { class: 'app-footer' }, [
    el('div', { class: 'container app-footer-inner' }, [
      el('div', { class: 'app-footer-brand' }, [
        el('a', {
          class: 'app-footer-logo',
          href: '#/',
          html: '<span class="brand-mark">◈</span><span class="brand-text">Sharpside</span>',
        }),
        el('p', { class: 'muted app-footer-tagline', text: t('footer.tagline') }),
      ]),
      nav,
      contact,
      el('div', { class: 'app-footer-meta' }, [
        el('p', {
          class: 'muted app-footer-note',
          text: t('footer.note'),
        }),
        el('p', { class: 'muted app-footer-copy', text: `© ${year} Sharpside` }),
      ]),
    ]),
  ]);
}

/**
 * 页面壳：顶部导航 + 主区 + 页脚 + 移动底栏。
 * 用法：构建 container 后 `return withShell(c)`。
 */
export function withShell(content) {
  const shell = el('div', { class: 'app-shell' });
  shell.appendChild(buildTopNav());
  const main = el('main', { class: 'app-main' });
  if (content) main.appendChild(content);
  shell.appendChild(main);
  shell.appendChild(buildFooter());
  shell.appendChild(buildBottomNav());
  return shell;
}
