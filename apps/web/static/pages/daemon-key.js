// pages/daemon-key.js · daemon API key（通道B 凭证轮换）。对应 docs/FRONTEND_DESIGN.md §6.13。
import { el, skeleton } from '../components/ui.js';
import { withShell } from '../components/nav.js';
import { me, rotateDaemonKey } from '../lib/account.js';
import { openOneTimeSecretModal } from '../components/one-time-secret.js';
import { toast } from '../store/toast.js';
import { t } from '../i18n/index.js';

export async function daemonKeyPage() {
  const c = el('div', { class: 'container narrow' });
  c.appendChild(el('h1', { text: t('daemonKey.title') }));

  const card = el('div', { class: 'card' }, [skeleton(2)]);
  c.appendChild(card);

  let user = null;
  try { user = await me(); } catch (e) { /* degrade */ }
  card.innerHTML = '';

  const issued = !!user?.daemon_api_key_rotated_at;
  card.appendChild(el('p', {}, [el('strong', { text: t('daemonKey.statusLabel') }), el('span', { class: issued ? 'pos' : 'muted', text: issued ? t('daemonKey.statusIssued') : t('daemonKey.statusNotIssued') })]));
  if (issued && user.daemon_api_key_rotated_at) {
    card.appendChild(el('p', { class: 'muted', text: t('daemonKey.lastRotated', { datetime: new Date(user.daemon_api_key_rotated_at).toLocaleString() }) }));
  }
  card.appendChild(el('p', { class: 'muted', text: t('daemonKey.description') }));

  const btnRow = el('div', { class: 'row' }, [
    el('button', { class: 'primary', text: issued ? t('daemonKey.rotate') : t('daemonKey.issue'), onclick: async () => {
      const msg = issued ? t('daemonKey.rotateConfirm') : t('daemonKey.issueConfirm');
      if (!confirm(msg)) return;
      try {
        const r = await rotateDaemonKey();
        const key = r.daemon_api_key || r.api_key || r.key || JSON.stringify(r);
        openOneTimeSecretModal({ title: t('daemonKey.oneTimeTitle'), value: key, warn: t('daemonKey.oneTimeWarn') });
        daemonKeyPage().then(mount);
      } catch (e) { toast(e.message, 'error'); }
    } }),
  ]);
  card.appendChild(btnRow);

  // daemon 安装步骤（F0 文档占位，下载链接待构建产物就绪）
  c.appendChild(el('h2', { text: t('daemonKey.installTitle') }));
  const install = el('div', { class: 'card' });
  const steps = el('ol', { class: 'install-steps' });
  steps.appendChild(step(t('daemonKey.stepDownload'), [
    el('button', { class: 'sm', disabled: true, text: 'macOS 🔒' }),
    el('button', { class: 'sm', disabled: true, text: 'Linux 🔒' }),
    el('button', { class: 'sm', disabled: true, text: 'Windows 🔒' }),
    el('span', { class: 'muted', text: t('daemonKey.downloadPending') }),
  ]));
  steps.appendChild(step(t('daemonKey.stepConfigure'), [
    el('code', { class: 'one-time-value', text: 'DAEMON_API_KEY=<key above>\nCOPIER_URL=https://api.sharpside.example/copier' }),
  ]));
  steps.appendChild(step(t('daemonKey.stepRun'), [
    el('a', { href: '#/settings/daemon-key', text: t('daemonKey.docsLink') }),
  ]));
  install.appendChild(steps);
  c.appendChild(install);

  return withShell(c);
}

function step(title, body) {
  const li = el('li');
  li.appendChild(el('div', { class: 'step-title', text: title }));
  li.appendChild(el('div', { class: 'step-body' }, body));
  return li;
}
function mount(node) { const app = document.getElementById('app'); app.innerHTML = ''; app.appendChild(node); }
