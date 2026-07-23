// components/nav.js · admin 分组导航。对应 docs/FRONTEND_DESIGN.md §7.1 + P1。
import { el } from './ui.js';
import { isLoggedIn, clearSession, getSession } from '../store/auth.js';
import { post } from '../api/client.js';
import { navigate, isActive } from '../router.js';

const GROUPS = [
  {
    label: '审核',
    links: [
      { href: '#/mappings', label: '市场映射' },
      { href: '#/identities', label: '身份' },
    ],
  },
  {
    label: '交易者池',
    links: [
      { href: '#/traders', label: '交易者管控' },
      { href: '#/hot-wallets', label: '热钥清单' },
      { href: '#/tag-rules', label: '标签规则' },
      { href: '#/category-mapping', label: '分类映射' },
    ],
  },
  {
    label: '影子',
    links: [
      { href: '#/audit-thresholds', label: '影子阈值' },
      { href: '#/shadow-health', label: '数据健康' },
    ],
  },
];

export function nav() {
  const bar = el('nav', { class: 'nav' });
  bar.appendChild(el('a', { class: 'brand', href: '#/mappings', text: 'Sharpside Admin' }));

  if (isLoggedIn()) {
    for (const g of GROUPS) {
      const group = el('div', { class: 'nav-group' });
      group.appendChild(el('span', { class: 'nav-group-label', text: g.label }));
      for (const l of g.links) {
        group.appendChild(el('a', {
          href: l.href,
          class: isActive(l.href.slice(1)) ? 'active' : '',
          text: l.label,
        }));
      }
      bar.appendChild(group);
    }
  }

  bar.appendChild(el('div', { class: 'spacer' }));
  if (isLoggedIn()) {
    const email = (getSession() && getSession().email) || 'admin';
    bar.appendChild(el('span', { class: 'muted', text: email }));
    bar.appendChild(el('button', {
      class: 'sm',
      onclick: async () => {
        // 安全修复 3.3：清服务端 session cookie，再清本地缓存。
        try { await post('/auth/oidc/logout'); } catch { /* best-effort */ }
        clearSession();
        navigate('/login');
      },
      text: '退出',
    }));
  }
  return bar;
}
