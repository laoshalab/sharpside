// pages/audit-thresholds.js · 影子阈值。对应 docs/FRONTEND_DESIGN.md §7.7。
import { el, dataTable, skeleton, emptyState } from '../components/ui.js';
import { nav } from '../components/nav.js';
import { listAuditThresholds, upsertAuditThreshold } from '../api/admin.js';
import { toast } from '../store/toast.js';

export async function auditThresholdsPage() {
  const root = el('div');
  root.appendChild(nav());
  const c = el('div', { class: 'container' });
  root.appendChild(c);
  c.appendChild(el('h1', { text: '影子阈值（交叉校验告警）' }));
  c.appendChild(el('p', { class: 'muted', text: '影子模式与第三方数据交叉校验：超 warn 记录，超 alert 告警；不影响主路径展示。' }));

  const card = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(card);

  async function load() {
    card.innerHTML = '';
    card.appendChild(skeleton(3));
    try {
      const rows = await listAuditThresholds();
      card.innerHTML = '';
      if (!rows || rows.length === 0) {
        card.appendChild(emptyState({ text: '无阈值配置' }));
        return;
      }
      card.appendChild(dataTable({
        columns: [
          { key: 'metric_name', label: 'metric' },
          { key: 'warn_pct', label: 'warn_pct', render: r => (r.warn_pct ?? 0) + '%' },
          { key: 'warn_abs', label: 'warn_abs', render: r => r.warn_abs ?? '—' },
          { key: 'alert_pct', label: 'alert_pct', render: r => (r.alert_pct ?? 0) + '%' },
          { key: 'alert_abs', label: 'alert_abs', render: r => r.alert_abs ?? '—' },
          { key: 'actions', label: '操作', render: () => `<button class="sm" data-act="edit">编辑</button>` },
        ],
        rows,
      }));
      card.querySelectorAll('button[data-act]').forEach((btn, i) => {
        btn.onclick = () => openModal(rows[i], load);
      });
    } catch (e) {
      card.innerHTML = '';
      card.appendChild(el('p', { class: 'neg', text: '加载失败：' + e.message }));
    }
  }

  load();
  return root;
}

function openModal(thr, onSaved) {
  const backdrop = el('div', { class: 'modal-backdrop' });
  const modal = el('div', { class: 'modal' });
  modal.appendChild(el('h2', { text: '编辑阈值' }));
  modal.appendChild(el('p', { class: 'muted', text: 'metric: ' + thr.metric_name + '（只读）' }));
  modal.appendChild(el('div', { class: 'row' }, [
    field('warn_pct', input('warn_pct', thr.warn_pct ?? 0)),
    field('warn_abs', input('warn_abs', thr.warn_abs ?? 0)),
    field('alert_pct', input('alert_pct', thr.alert_pct ?? 0)),
    field('alert_abs', input('alert_abs', thr.alert_abs ?? 0)),
  ]));
  const errP = el('p', { class: 'error' });
  modal.appendChild(errP);
  modal.appendChild(el('button', { class: 'primary', text: '保存', onclick: async () => {
    errP.textContent = '';
    const body = {
      warn_pct: Number(val('warn_pct')) || 0,
      warn_abs: Number(val('warn_abs')) || 0,
      alert_pct: Number(val('alert_pct')) || 0,
      alert_abs: Number(val('alert_abs')) || 0,
    };
    try {
      await upsertAuditThreshold(thr.metric_name, body);
      toast('已保存', 'success');
      backdrop.remove();
      onSaved();
    } catch (e) { errP.textContent = e.message; }
  } }));
  modal.appendChild(el('button', { text: '取消', onclick: () => backdrop.remove() }));
  backdrop.appendChild(modal);
  backdrop.addEventListener('click', e => { if (e.target === backdrop) backdrop.remove(); });
  document.body.appendChild(backdrop);
}

function field(label, child) { return el('div', { class: 'field' }, [el('label', { text: label }), child]); }
function input(id, v) { return el('input', { id, type: 'number', step: 'any', value: v }); }
function val(id) { return document.getElementById(id).value; }
