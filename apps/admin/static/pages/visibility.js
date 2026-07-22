// pages/visibility.js · 交易者管控（可见性 / is_hot / alias）。
// 对应 docs/FRONTEND_DESIGN.md §7.6 + TRADERS_TABLE.md admin 编辑。
import { el, dataTable, skeleton, emptyState } from '../components/ui.js';
import { nav } from '../components/nav.js';
import { listTraders, setVisibility, setHot, setAlias } from '../api/admin.js';
import { toast } from '../store/toast.js';

const PLATFORMS = ['', 'polymarket', 'kalshi', 'manifold', 'zeitgeist', 'azuro'];
const VIS = ['visible', 'hidden', 'blocked'];

export async function visibilityPage() {
  const state = { platform: '', q: '', limit: 100, offset: 0 };
  const root = el('div');
  root.appendChild(nav());
  const c = el('div', { class: 'container' });
  root.appendChild(c);
  c.appendChild(el('h1', { text: '交易者管控' }));
  c.appendChild(el('p', {
    class: 'muted',
    text: '可见性三态 · is_hot（浮仓抓取）· 站内 alias。热钥清单（抓取频率）见「热钥清单」页。',
  }));

  const bar = el('div', { class: 'row' });
  bar.appendChild(field('Venue', selectEl(state.platform, PLATFORMS, v => { state.platform = v; load(); })));
  bar.appendChild(field('搜索', inputEl(state.q, v => { state.q = v; load(); })));
  c.appendChild(bar);

  const card = el('div', { class: 'card' }, [skeleton(4)]);
  c.appendChild(card);

  let debounce;
  async function load() {
    clearTimeout(debounce);
    debounce = setTimeout(async () => {
      card.innerHTML = '';
      card.appendChild(skeleton(3));
      try {
        const rows = await listTraders({
          platform: state.platform || undefined,
          q: state.q || undefined,
          limit: state.limit,
          offset: state.offset,
        });
        card.innerHTML = '';
        if (!rows || rows.length === 0) {
          card.appendChild(emptyState({ icon: '🔍', text: '无匹配交易者' }));
          return;
        }
        card.appendChild(dataTable({
          columns: [
            {
              key: 'name',
              label: '交易者',
              render: r => escHtml(r.alias || (r.x_username ? '@' + r.x_username : '') || String(r.address).slice(0, 10) + '…'),
            },
            { key: 'platform', label: '平台' },
            {
              key: 'address',
              label: '地址',
              render: r => `<code>${escHtml(String(r.address).slice(0, 10))}…</code>`,
            },
            {
              key: 'alias',
              label: 'alias',
              render: r => `<input class="inline" data-role="alias" value="${escAttr(r.alias || '')}" placeholder="站内名" /> <button class="sm" data-role="save-alias">保存</button>`,
            },
            {
              key: 'is_hot',
              label: 'is_hot',
              render: r => `<label class="inline-check"><input type="checkbox" data-role="hot" ${r.is_hot ? 'checked' : ''}/> 热钥</label>`,
            },
            {
              key: 'visibility',
              label: '可见性',
              render: r => `<span class="${r.visibility === 'blocked' ? 'neg' : r.visibility === 'hidden' ? 'neutral' : 'pos'}">${escHtml(r.visibility)}</span>`,
            },
            {
              key: 'switch',
              label: '切换可见性',
              render: r => `<select data-role="vis">${VIS.map(v => `<option value="${v}" ${v === r.visibility ? 'selected' : ''}>${v}</option>`).join('')}</select> <button class="sm" data-role="apply-vis">应用</button>`,
            },
          ],
          rows,
        }));

        // dataTable 的 render 只传 row；用闭包按行绑事件
        const table = card.querySelector('table');
        const trs = table ? Array.from(table.querySelectorAll('tbody tr')) : [];
        trs.forEach((tr, i) => {
          const row = rows[i];
          if (!row) return;
          const applyBtn = tr.querySelector('[data-role="apply-vis"]');
          const visSel = tr.querySelector('[data-role="vis"]');
          if (applyBtn && visSel) {
            applyBtn.onclick = async () => {
              if (!confirm(`确认将 ${row.platform}/${row.address} 切换为 ${visSel.value}？\nblocked 用户将无法被搜索/跟随。`)) return;
              try {
                await setVisibility(row.platform, row.address, visSel.value);
                toast('可见性已应用', 'success');
                load();
              } catch (e) { toast(e.message, 'error'); }
            };
          }
          const hotCb = tr.querySelector('[data-role="hot"]');
          if (hotCb) {
            hotCb.onchange = async () => {
              try {
                await setHot(row.platform, row.address, hotCb.checked);
                toast(hotCb.checked ? '已标为热钥' : '已取消热钥', 'success');
              } catch (e) {
                hotCb.checked = !hotCb.checked;
                toast(e.message, 'error');
              }
            };
          }
          const saveAlias = tr.querySelector('[data-role="save-alias"]');
          const aliasInp = tr.querySelector('[data-role="alias"]');
          if (saveAlias && aliasInp) {
            saveAlias.onclick = async () => {
              try {
                await setAlias(row.platform, row.address, aliasInp.value.trim() || null);
                toast('alias 已保存', 'success');
                load();
              } catch (e) { toast(e.message, 'error'); }
            };
          }
        });
      } catch (e) {
        card.innerHTML = '';
        card.appendChild(el('p', { class: 'neg', text: '加载失败：' + e.message }));
      }
    }, 250);
  }

  load();
  return root;
}

function escHtml(s) {
  return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}
function escAttr(s) {
  return String(s).replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;');
}
function field(label, child) { return el('div', { class: 'field' }, [el('label', { text: label }), child]); }
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
function inputEl(val, onChange) {
  return el('input', {
    type: 'text',
    value: val || '',
    placeholder: '地址 / alias / @x',
    oninput: e => onChange(e.target.value),
  });
}
