// pages/identity.js · 跨平台身份详情。对应 docs/FRONTEND_DESIGN.md #/identities/{id}。
import { el, skeleton, emptyState, traderLabel, tagChips } from '../components/ui.js';
import { withShell } from '../components/nav.js';
import { getIdentity } from '../lib/venue-hub.js';
import { navigate } from '../router.js';
import { t } from '../i18n/index.js';

export async function identityPage({ params }) {
  const id = params.id;
  const c = el('div', { class: 'container' });
  c.appendChild(el('a', { href: '#/leaderboard', class: 'muted', text: t('identity.backDiscover') }));
  c.appendChild(el('h1', { text: t('identity.title') }));

  const card = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(card);

  try {
    const data = await getIdentity(id);
    const idn = data.identity || data;
    const traders = data.traders || [];
    card.innerHTML = '';
    card.appendChild(el('div', { class: 'section-title', text: idn.alias || t('identity.unnamed') }));
    card.appendChild(el('p', { class: 'muted', text: `ID ${String(idn.id).slice(0, 8)}… · confidence ${idn.confidence ?? '—'}` }));
    card.appendChild(tagChips(idn.manual_verified ? [t('identity.verified')] : [t('identity.unverified')]));

    c.appendChild(el('h2', { text: t('identity.linkedTraders') }));
    const list = el('div', { class: 'card' });
    c.appendChild(list);
    if (!traders.length) {
      list.appendChild(emptyState({ text: t('identity.noTraders') }));
    } else {
      for (const tr of traders) {
        const href = `#/traders/${encodeURIComponent(tr.platform)}/${encodeURIComponent(tr.address)}`;
        list.appendChild(el('div', { class: 'follow-card', style: 'margin-bottom:8px' }, [
          el('div', { class: 'fc-head' }, [
            el('a', { href, text: `${traderLabel(tr)} · ${tr.platform}` }),
          ]),
          el('div', { class: 'fc-meta' }, [
            el('span', { class: 'muted', text: tr.address }),
          ]),
          el('div', { class: 'fc-actions' }, [
            el('button', {
              class: 'sm primary',
              text: t('identity.follow'),
              onclick: () => navigate(`/follows/new?platform=${encodeURIComponent(tr.platform)}&address=${encodeURIComponent(tr.address)}`),
            }),
          ]),
        ]));
      }
    }

    if (idn.manual_verified) {
      c.appendChild(el('div', { class: 'row' }, [
        el('button', {
          class: 'primary',
          text: t('identity.followIdentity'),
          onclick: () => navigate(`/follows/new?identity_id=${encodeURIComponent(idn.id)}`),
        }),
      ]));
    } else {
      c.appendChild(el('p', { class: 'muted', text: t('identity.followLocked') }));
    }
  } catch (e) {
    card.innerHTML = '';
    card.appendChild(el('p', { class: 'neg', text: t('identity.loadError', { message: e.message }) }));
  }

  return withShell(c);
}
