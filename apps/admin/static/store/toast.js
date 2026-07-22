// store/toast.js · 轻量 toast 通知。对应 docs/FRONTEND_DESIGN.md §8。
export function toast(message, type = 'info', timeout = 3500) {
  const wrap = document.getElementById('toast-wrap');
  if (!wrap) { console.warn('[toast]', message); return; }
  const el = document.createElement('div');
  el.className = 'toast ' + type;
  el.textContent = message;
  wrap.appendChild(el);
  setTimeout(() => { el.remove(); }, timeout);
}
