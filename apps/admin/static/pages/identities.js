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
  const idShort = String(idn.id).slice(0, 8);
  card.appendChild(el('div', { class: 'section-title', text: `候选身份 #${idShort}` }));
  card.appendChild(el('p', {}, [
    el('strong', { text: 'alias: ' }),
    el('span', { text: idn.alias || '—' }),
    el('span', { class: 'muted', text: `  confidence: ${idn.confidence ?? '—'}` }),
  ]));
  // API 返回 flatten Identity + traders[]；verified 字段实际为 manual_verified。
  const verified = !!idn.manual_verified;
  card.appendChild(tagChips(verified ? ['verified ✓'] : ['待审']));

  const traders = Array.isArray(idn.traders) ? idn.traders : [];
  if (traders.length === 0) {
    card.appendChild(el('p', { class: 'muted', text: '关联 traders：无（无法确认跨 Venue 链接）' }));
  } else {
    card.appendChild(el('p', { class: 'muted', text: `关联 traders（${traders.length}）：` }));
    const ul = el('ul', { style: 'margin:4px 0 8px 18px' });
    for (const t of traders) {
      const alias = t.alias ? ` · ${t.alias}` : '';
      const x = t.x_username ? ` · @${t.x_username}` : '';
      ul.appendChild(el('li', {
        text: `${t.platform}/${t.address}${alias}${x}`,
        style: 'font-family:monospace;font-size:12px;word-break:break-all',
      }));
    }
    card.appendChild(ul);
  }

  const btnRow = el('div', { class: 'row', style: 'margin-top:12px' });
  card.appendChild(btnRow);
  const confirmBtn = el('button', { class: 'primary', text: '✓ 确认人工校对' });
  confirmBtn.onclick = async () => {
    confirmBtn.disabled = true;
    try {
      await verifyIdentity(idn.id, { verified_by: 'admin' });
      toast('已确认，该身份可被用户跟随', 'success');
      reload();
    } catch (e) {
      toast(e.message, 'error');
      confirmBtn.disabled = false;
    }
  };
  const delBtn = el('button', { class: 'danger', text: '✗ 删除候选' });
  delBtn.onclick = async () => {
    if (!confirm('确认删除此身份候选？')) return;
    delBtn.disabled = true;
    try {
      await deleteIdentity(idn.id);
      toast('已删除', 'success');
      reload();
    } catch (e) {
      toast(e.message, 'error');
      delBtn.disabled = false;
    }
  };
  btnRow.appendChild(confirmBtn);
  btnRow.appendChild(delBtn);

  card.appendChild(el('p', { class: 'muted', style: 'margin-top:8px', text: '提示：确认后该身份可被用户跟随（跟随门禁硬规则）' }));
  return card;
}
