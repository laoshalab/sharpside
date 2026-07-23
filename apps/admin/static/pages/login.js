// pages/login.js · admin 登录。安全修复 3.3：优先 OIDC SSO；dev 无 OIDC 时回退 ADMIN_TOKEN。
import { el } from '../components/ui.js';
import { setDevToken, setSession, clearSession, isLoggedIn } from '../store/auth.js';
import { get } from '../api/client.js';
import { toast } from '../store/toast.js';
import { navigate } from '../router.js';

export async function loginPage() {
  // 已有 session cookie / dev token → 校验后进后台。
  if (isLoggedIn()) {
    try {
      const me = await get('/auth/me');
      setSession({ email: me.email || 'admin' });
      navigate('/mappings');
      return el('div');
    } catch {
      clearSession();
    }
  }

  const root = el('div');
  const c = el('div', { class: 'container narrow' });
  root.appendChild(c);
  c.appendChild(el('h1', { text: 'Sharpside Admin' }));
  c.appendChild(el('p', { class: 'muted', text: '运营后台 · SSO / OIDC 登录' }));

  // 主路径：OIDC（生产必配；未配时按钮会 5xx，下方有 dev 回退）。
  c.appendChild(el('a', {
    class: 'button primary',
    href: '/api/auth/oidc/login',
    text: '使用 SSO 登录',
    style: 'display:inline-block;text-decoration:none;padding:10px 16px;margin:12px 0',
  }));
  c.appendChild(el('p', { class: 'muted', text: '生产环境须走 OIDC（邮箱白名单）。' }));

  // Dev 回退：仅本地未配 OIDC 时使用共享 ADMIN_TOKEN。
  const errP = el('p', { class: 'error' });
  const details = el('details', { style: 'margin-top:24px' });
  details.appendChild(el('summary', { class: 'muted', text: '开发：Admin Token 登录（无 OIDC 时）' }));
  const body = el('div', { style: 'margin-top:8px' });
  body.appendChild(el('div', { class: 'field' }, [
    el('label', { text: 'Admin Token' }),
    el('input', { type: 'password', id: 'token', placeholder: 'dev-admin-token' }),
  ]));
  body.appendChild(errP);
  body.appendChild(el('button', { class: 'sm', text: '用 Token 登录', onclick: submitDev }));
  details.appendChild(body);
  c.appendChild(details);

  async function submitDev() {
    errP.textContent = '';
    const t = document.getElementById('token').value.trim();
    if (!t) { errP.textContent = '请输入 admin token'; return; }
    setDevToken(t);
    try {
      const me = await get('/auth/me');
      setSession({ email: me.email || 'dev-admin' });
      toast('登录成功', 'success');
      navigate('/mappings');
    } catch (e) {
      errP.textContent = 'token 无效：' + e.message;
      clearSession();
    }
  }

  return root;
}
