// pages/tag-rules.js · 标签阈值。对应 docs/FRONTEND_DESIGN.md §7.5。
import { el, dataTable, skeleton, emptyState } from '../components/ui.js';
import { nav } from '../components/nav.js';
import { listTagRules, upsertTagRule } from '../api/admin.js';
import { toast } from '../store/toast.js';

export async function tagRulesPage() {
  const root = el('div');
  root.appendChild(nav());
  const c = el('div', { class: 'container' });
  root.appendChild(c);
  c.appendChild(el('h1', { text: '标签阈值规则' }));

  const card = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(card);

  async function load() {
    card.innerHTML = '';
    card.appendChild(skeleton(3));
    try {
      const rows = await listTagRules();
      card.innerHTML = '';
      if (!rows || rows.length === 0) {
        card.appendChild(emptyState({ text: '无规则' }));
        return;
      }
      card.appendChild(dataTable({
        columns: [
          { key: 'rule_id', label: 'rule_id' },
          { key: 'enabled', label: '启用', render: r => r.enabled ? '✅' : '☐' },
          { key: 'updated_by', label: 'updated_by' },
          { key: 'updated_at', label: '更新于', render: r => r.updated_at ? new Date(r.updated_at).toLocaleDateString() : '—' },
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

function openModal(rule, onSaved) {
  const backdrop = el('div', { class: 'modal-backdrop' });
  const modal = el('div', { class: 'modal' });
  modal.appendChild(el('h2', { text: '编辑规则' }));
  modal.appendChild(el('p', { class: 'muted', text: 'rule_id: ' + rule.rule_id + '（只读）' }));
  const paramsStr = typeof rule.params === 'string' ? rule.params : JSON.stringify(rule.params ?? {}, null, 2);
  const ta = el('textarea', { style: 'width:100%;min-height:160px;font-family:monospace' });
  ta.value = paramsStr;
  modal.appendChild(el('div', { class: 'field' }, [el('label', { text: 'params (JSON)' }), ta]));
  const enCb = el('input', { type: 'checkbox', ...(rule.enabled !== false ? { checked: 'checked' } : {}) });
  modal.appendChild(el('label', { style: 'display:flex;align-items:center;gap:6px;margin:8px 0' }, [enCb, document.createTextNode('enabled')]));
  const errP = el('p', { class: 'error' });
  modal.appendChild(errP);
  modal.appendChild(el('button', { class: 'primary', text: '保存', onclick: async () => {
    errP.textContent = '';
    let parsed;
    try { parsed = JSON.parse(ta.value); } catch (e) { errP.textContent = 'JSON 语法错误：' + e.message; return; }
    try {
      await upsertTagRule(rule.rule_id, { params: parsed, enabled: enCb.checked, updated_by: 'admin' });
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
