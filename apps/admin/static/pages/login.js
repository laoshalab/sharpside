// pages/login.js · admin 登录（admin token）。对应 docs/FRONTEND_DESIGN.md §7.1。
import { el } from '../components/ui.js';
import { setToken } from '../store/auth.js';
import { toast } from '../store/toast.js';
import { navigate } from '../router.js';

export async function loginPage() {
  const root = el('div');
  const c = el('div', { class: 'container narrow' });
  root.appendChild(c);
  c.appendChild(el('h1', { text: 'Sharpside Admin' }));
  c.appendChild(el('p', { class: 'muted', text: '运营后台 · 请输入 admin token' }));

  const errP = el('p', { class: 'error' });
  const tokenF = el('div', { class: 'field' }, [el('label', { text: 'Admin Token' }), el('input', { type: 'password', id: 'token', placeholder: 'dev-admin-token' })]);
  c.appendChild(tokenF);
  c.appendChild(errP);
  c.appendChild(el('button', { class: 'primary', text: '登录', onclick: submit }));

  async function submit() {
    errP.textContent = '';
    const t = document.getElementById('token').value.trim();
    if (!t) { errP.textContent = '请输入 admin token'; return; }
    setToken(t);
    // 验证：拉一个需鉴权端点
    try {
      const { listTagRules } = await import('../api/admin.js');
      await listTagRules();
      toast('登录成功', 'success');
      navigate('/mappings');
    } catch (e) {
      errP.textContent = 'token 无效：' + e.message;
      setToken('');
    }
  }

  return root;
}
