// pages/hot-wallets.js · 热钥管理。对应 docs/FRONTEND_DESIGN.md §7.4。
import { el, dataTable, skeleton, emptyState } from '../components/ui.js';
import { nav } from '../components/nav.js';
import { listHotWallets, upsertHotWallet, deleteHotWallet } from '../api/admin.js';
import { toast } from '../store/toast.js';

const PLATFORMS = ['polymarket', 'kalshi', 'manifold', 'zeitgeist', 'azuro'];

export async function hotWalletsPage() {
  const state = { platform: 'polymarket' };
  const root = el('div');
  root.appendChild(nav());
  const c = el('div', { class: 'container' });
  root.appendChild(c);
  c.appendChild(el('h1', { text: '热钥管理' }));

  const bar = el('div', { class: 'row' });
  bar.appendChild(field('Venue', selectEl(state.platform, PLATFORMS, v => { state.platform = v; load(); })));
  bar.appendChild(el('div', { class: 'field' }, [el('label', { text: ' ' }), el('button', { class: 'primary', text: '+ 添加', onclick: () => openModal(null, state.platform, load) })]));
  c.appendChild(bar);

  const card = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(card);

  async function load() {
    card.innerHTML = '';
    card.appendChild(skeleton(3));
    try {
      const rows = await listHotWallets(state.platform);
      card.innerHTML = '';
      if (!rows || rows.length === 0) {
        card.appendChild(emptyState({ icon: '∅', text: '该 Venue 无热钥' }));
        return;
      }
      card.appendChild(dataTable({
        columns: [
          { key: 'address', label: '地址', render: r => `<code>${String(r.address).slice(0, 10)}…${String(r.address).slice(-4)}</code>` },
          { key: 'priority', label: '优先级' },
          { key: 'scan_interval_secs', label: '抓取频率(秒)' },
          { key: 'enabled', label: '启用', render: r => r.enabled ? '✅' : '☐' },
          { key: 'added_by', label: '添加人' },
          { key: 'actions', label: '操作', render: r => `<button class="sm" data-act="edit">编辑</button> <button class="sm danger" data-act="del">删除</button>` },
        ],
        rows,
      }));
      card.querySelectorAll('button[data-act]').forEach((btn, i) => {
        const row = rows[Math.floor(i / 2)];
        btn.onclick = () => {
          if (btn.dataset.act === 'edit') openModal(row, state.platform, load);
          else if (btn.dataset.act === 'del') (async () => {
            if (!confirm(`确认删除 ${row.platform}/${row.address}？`)) return;
            try { await deleteHotWallet(row.platform, row.address); toast('已删除', 'success'); load(); } catch (e) { toast(e.message, 'error'); }
          })();
        };
      });
    } catch (e) {
      card.innerHTML = '';
      card.appendChild(el('p', { class: 'neg', text: '加载失败：' + e.message }));
    }
  }

  load();
  return root;
}

function openModal(existing, platform, onSaved) {
  const backdrop = el('div', { class: 'modal-backdrop' });
  const modal = el('div', { class: 'modal' });
  modal.appendChild(el('h2', { text: existing ? '编辑热钥' : '添加热钥' }));
  const platF = field('平台', input('platform', existing?.platform || platform));
  const addrF = field('地址', input('address', existing?.address || '', '0x…'));
  const priF = field('优先级（数字小=高优先）', input('priority', existing?.priority ?? 100, '100'));
  const intF = field('抓取频率(秒)', input('scan_interval_secs', existing?.scan_interval_secs ?? 30, '30'));
  const enCb = el('input', { type: 'checkbox', ...(existing?.enabled !== false ? { checked: 'checked' } : {}) });
  const enF = el('div', { class: 'field' }, [el('label', { text: '启用' }), el('label', { style: 'display:flex;align-items:center;gap:6px' }, [enCb, document.createTextNode('启用')])]);
  modal.appendChild(platF); modal.appendChild(addrF); modal.appendChild(priF); modal.appendChild(intF); modal.appendChild(enF);
  const errP = el('p', { class: 'error' });
  modal.appendChild(errP);
  modal.appendChild(el('button', { class: 'primary', text: '保存', onclick: async () => {
    errP.textContent = '';
    const body = {
      platform: val('platform'), address: val('address'),
      priority: Number(val('priority')) || 100,
      scan_interval_secs: Number(val('scan_interval_secs')) || 30,
      enabled: enCb.checked, added_by: 'admin',
    };
    if (!body.platform || !body.address) { errP.textContent = '请填写平台与地址'; return; }
    try {
      await upsertHotWallet(body);
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
function input(id, val, placeholder) { return el('input', { id, value: val ?? '', placeholder: placeholder || '' }); }
function val(id) { return document.getElementById(id).value.trim(); }
function selectEl(val, options, onChange) {
  const s = el('select', { onchange: e => onChange(e.target.value) });
  for (const o of options) s.appendChild(el('option', { value: o, text: o, ...(o === val ? { selected: 'selected' } : {}) }));
  return s;
}
