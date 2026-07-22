// pages/identities.js · 身份审核。对应 docs/FRONTEND_DESIGN.md §7.3。
import { el, skeleton, emptyState, tagChips } from '../components/ui.js';
import { nav } from '../components/nav.js';
import { listPendingIdentities, verifyIdentity, deleteIdentity } from '../api/admin.js';
import { toast } from '../store/toast.js';

export async function identitiesPage() {
  const root = el('div');
  root.appendChild(nav());
  const c = el('div', { class: 'container' });
  root.appendChild(c);
  c.appendChild(el('h1', { text: '身份审核' }));

  const list = el('div');
  c.appendChild(list);
  list.appendChild(skeleton(4));

  async function load() {
    list.innerHTML = '';
    list.appendChild(skeleton(3));
    try {
      const rows = await listPendingIdentities();
      list.innerHTML = '';
      if (!rows || rows.length === 0) {
        list.appendChild(emptyState({ icon: '✓', text: '无待审身份' }));
        return;
      }
      c.querySelector('h1').textContent = `身份审核（待审 ${rows.length}）`;
      for (const idn of rows) {
        list.appendChild(identityCard(idn, load));
      }
    } catch (e) {
      list.innerHTML = '';
      list.appendChild(el('p', { class: 'neg', text: '加载失败：' + e.message }));
    }
  }

  load();
  return root;
}

function identityCard(idn, reload) {
  const card = el('div', { class: 'card' });
  card.appendChild(el('div', { class: 'section-title', text: `候选身份 #${String(idn.id).slice(0, 8)}` }));
  card.appendChild(el('p', {}, [el('strong', { text: 'alias: ' }), el('span', { text: idn.alias || '—' }), el('span', { class: 'muted', text: `  confidence: ${idn.confidence ?? '—'}` })]));
  if (idn.x_username) card.appendChild(el('p', { class: 'muted', text: '@' + idn.x_username }));
  // 关联 traders（Identity 模型可能含 trader 列表或启发式依据；按字段尽力渲染）
  if (idn.heuristic || idn.heuristic_notes) {
    card.appendChild(el('p', { class: 'muted', text: '启发式依据：' + (idn.heuristic || idn.heuristic_notes) }));
  }
  if (idn.trader_platform && idn.trader_address) {
    card.appendChild(el('p', { class: 'muted', text: `关联：${idn.trader_platform}/${idn.trader_address}` }));
  }
  card.appendChild(tagChips(idn.verified ? ['verified ✓'] : ['待审']));

  const btnRow = el('div', { class: 'row', style: 'margin-top:12px' });
  card.appendChild(btnRow);
  btnRow.appendChild(el('button', { class: 'primary', text: '✓ 确认人工校对', onclick: async () => {
    try {
      await verifyIdentity(idn.id, { verified_by: 'admin' });
      toast('已确认，该身份可被用户跟随', 'success');
      reload();
    } catch (e) { toast(e.message, 'error'); }
  } }));
  btnRow.appendChild(el('button', { class: 'danger', text: '✗ 删除候选', onclick: async () => {
    if (!confirm('确认删除此身份候选？')) return;
    try {
      await deleteIdentity(idn.id);
      toast('已删除', 'success');
      reload();
    } catch (e) { toast(e.message, 'error'); }
  } }));

  card.appendChild(el('p', { class: 'muted', style: 'margin-top:8px', text: '提示：确认后该身份可被用户跟随（跟随门禁硬规则）' }));
  return card;
}
