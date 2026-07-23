// pages/category-mapping.js · 官方 category → 站内分类映射。
// 对应 docs/VENUEHUB_STORAGE.md §8 / PERFORMANCE_PIPELINE.md。
import { el, dataTable, skeleton, emptyState } from '../components/ui.js';
import { nav } from '../components/nav.js';
import { listCategoryMappings, upsertCategoryMapping, deleteCategoryMapping } from '../api/admin.js';
import { toast } from '../store/toast.js';

const PLATFORMS = ['', 'polymarket', 'kalshi', 'manifold', 'zeitgeist', 'azuro'];

export async function categoryMappingPage() {
  const state = { platform: 'polymarket' };
  const root = el('div');
  root.appendChild(nav());
  const c = el('div', { class: 'container' });
  root.appendChild(c);
  c.appendChild(el('h1', { text: '分类映射' }));
  c.appendChild(el('p', {
    class: 'muted',
    text: 'Venue 官方 category → 站内分类；影响排行榜分类切片与 perf 物化。',
  }));

  const bar = el('div', { class: 'row' });
  bar.appendChild(field('Venue', selectEl(state.platform, PLATFORMS, v => { state.platform = v; load(); })));
  bar.appendChild(el('div', { class: 'field' }, [
    el('label', { text: ' ' }),
    el('button', { class: 'primary', text: '+ 添加', onclick: () => openModal(null, state.platform || 'polymarket', load) }),
  ]));
  c.appendChild(bar);

  const card = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(card);

  async function load() {
    card.innerHTML = '';
    card.appendChild(skeleton(3));
    try {
      const rows = await listCategoryMappings(state.platform || undefined);
      card.innerHTML = '';
      if (!rows || rows.length === 0) {
        card.appendChild(emptyState({ text: '无分类映射' }));
        return;
      }
      card.appendChild(dataTable({
        columns: [
          { key: 'platform', label: '平台' },
          { key: 'official_category', label: '官方 category' },
          { key: 'site_category', label: '站内分类' },
          { key: 'display_name', label: '显示名' },
          {
            key: 'actions',
            label: '操作',
            render: () => `<button class="sm" data-act="edit">编辑</button> <button class="sm danger" data-act="del">删除</button>`,
          },
        ],
        rows,
      }));
      card.querySelectorAll('button[data-act]').forEach((btn, i) => {
        const row = rows[Math.floor(i / 2)];
        btn.onclick = () => {
          if (btn.dataset.act === 'edit') openModal(row, state.platform || row.platform, load);
          else if (btn.dataset.act === 'del') (async () => {
            if (!confirm(`确认删除 ${row.platform}/${row.official_category}？`)) return;
            try {
              await deleteCategoryMapping(row.platform, row.official_category);
              toast('已删除', 'success');
              load();
            } catch (e) { toast(e.message, 'error'); }
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
  modal.appendChild(el('h2', { text: existing ? '编辑分类映射' : '添加分类映射' }));
  const plat = input('platform', existing?.platform || platform);
  const official = input('official_category', existing?.official_category || '');
  if (existing) {
    plat.disabled = true;
    official.disabled = true;
  }
  modal.appendChild(field('平台', plat));
  modal.appendChild(field('官方 category', official));
  modal.appendChild(field('站内分类', input('site_category', existing?.site_category || '')));
  modal.appendChild(field('显示名（可选）', input('display_name', existing?.display_name || '')));
  const errP = el('p', { class: 'error' });
  modal.appendChild(errP);
  modal.appendChild(el('button', {
    class: 'primary',
    text: '保存',
    onclick: async () => {
      errP.textContent = '';
      const body = {
        platform: val('platform'),
        official_category: val('official_category'),
        site_category: val('site_category'),
        display_name: val('display_name') || null,
      };
      if (!body.platform || !body.official_category || !body.site_category) {
        errP.textContent = '请填写平台、官方 category、站内分类';
        return;
      }
      try {
        await upsertCategoryMapping(body);
        toast('已保存', 'success');
        backdrop.remove();
        onSaved();
      } catch (e) { errP.textContent = e.message; }
    },
  }));
  modal.appendChild(el('button', { text: '取消', onclick: () => backdrop.remove() }));
  backdrop.appendChild(modal);
  backdrop.addEventListener('click', e => { if (e.target === backdrop) backdrop.remove(); });
  document.body.appendChild(backdrop);
}

function field(label, child) { return el('div', { class: 'field' }, [el('label', { text: label }), child]); }
function input(id, v) { return el('input', { id, value: v ?? '' }); }
function val(id) { return document.getElementById(id).value.trim(); }
function selectEl(val, options, onChange) {
  const s = el('select', { onchange: e => onChange(e.target.value) });
  for (const o of options) {
    s.appendChild(el('option', {
      value: o,
      text: o || '全部',
      ...(o === val ? { selected: 'selected' } : {}),
    }));
  }
  return s;
}
