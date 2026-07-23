// pages/mappings.js · 市场映射审核。对应 docs/FRONTEND_DESIGN.md §7.2。
import { el, skeleton, emptyState } from '../components/ui.js';
import { nav } from '../components/nav.js';
import { listPendingMappings, verifyMapping, retireMapping } from '../api/admin.js';
import { toast } from '../store/toast.js';

export async function mappingsPage() {
  const root = el('div');
  root.appendChild(nav());
  const c = el('div', { class: 'container' });
  root.appendChild(c);
  c.appendChild(el('h1', { text: '市场映射审核' }));

  const list = el('div');
  c.appendChild(list);
  list.appendChild(skeleton(4));

  async function load() {
    list.innerHTML = '';
    list.appendChild(skeleton(3));
    try {
      const rows = await listPendingMappings();
      list.innerHTML = '';
      if (!rows || rows.length === 0) {
        list.appendChild(emptyState({ icon: '✓', text: '无待审映射' }));
        return;
      }
      c.querySelector('h1').textContent = `市场映射审核（待审 ${rows.length}）`;
      for (const m of rows) {
        list.appendChild(mappingCard(m, load));
      }
    } catch (e) {
      list.innerHTML = '';
      list.appendChild(el('p', { class: 'neg', text: '加载失败：' + e.message }));
    }
  }

  load();
  return root;
}

function mappingCard(m, reload) {
  const card = el('div', { class: 'card' });
  card.appendChild(el('div', { class: 'section-title', text: '候选映射' }));
  card.appendChild(el('p', {}, [el('strong', { text: 'from: ' }), el('span', { text: `${m.from_platform}/${m.from_market_id}` })]));
  card.appendChild(el('p', {}, [el('strong', { text: 'to: ' }), el('span', { text: `${m.to_platform}/${m.to_market_id}` })]));
  card.appendChild(el('p', { class: 'muted', text: `confidence: ${m.confidence ?? '—'}` }));

  // 预填服务端已有建议，避免误确认翻转映射。
  const flipCb = el('input', { type: 'checkbox', ...(m.direction_flip ? { checked: 'checked' } : {}) });
  const flipWrap = el('label', { style: 'display:flex;align-items:center;gap:6px;margin:8px 0', title: 'Polymarket YES 可能对应 Kalshi No 合约（跟反方向会亏光）' }, [
    flipCb, el('span', { text: 'direction_flip (YES↔NO 翻转)' }),
  ]);
  card.appendChild(flipWrap);

  const notes = el('textarea', { placeholder: 'resolution_notes（同事件，结算规则一致…）', style: 'width:100%;min-height:60px;margin:8px 0' });
  if (m.resolution_notes) notes.value = m.resolution_notes;
  card.appendChild(notes);

  const minNotional = el('input', { type: 'number', placeholder: 'min_notional（可选）', style: 'width:200px' });
  if (m.min_notional != null && m.min_notional !== '') minNotional.value = String(m.min_notional);
  card.appendChild(minNotional);

  const btnRow = el('div', { class: 'row', style: 'margin-top:12px' });
  card.appendChild(btnRow);
  btnRow.appendChild(el('button', { class: 'primary', text: '✓ 确认验证', onclick: async () => {
    try {
      await verifyMapping({
        from_platform: m.from_platform, from_market_id: m.from_market_id,
        to_platform: m.to_platform, to_market_id: m.to_market_id,
        direction_flip: flipCb.checked, resolution_notes: notes.value || null,
        min_notional: minNotional.value ? Number(minNotional.value) : null,
        verified_by: 'admin',
      });
      toast('已验证，映射进入跟单路径', 'success');
      reload();
    } catch (e) { toast(e.message, 'error'); }
  } }));
  btnRow.appendChild(el('button', { class: 'danger', text: '✗ 撤销(retire)', onclick: async () => {
    if (!confirm('确认撤销此映射？')) return;
    try {
      await retireMapping({ from_platform: m.from_platform, from_market_id: m.from_market_id, to_platform: m.to_platform, to_market_id: m.to_market_id });
      toast('已撤销', 'success');
      reload();
    } catch (e) { toast(e.message, 'error'); }
  } }));

  card.appendChild(el('p', { class: 'muted', style: 'margin-top:8px', text: '提示：确认即生效进跟单路径（manual_verified + active）' }));
  return card;
}
