// pages/credentials.js · Venue 凭证中心。对应 docs/FRONTEND_DESIGN.md §6.5。
import { el, skeleton, emptyState } from '../components/ui.js';
import { withShell } from '../components/nav.js';
import { listCredentials, getDelegation, provisionDepositWallet } from '../lib/account.js';
import { toast } from '../store/toast.js';
import { navigate, remount } from '../router.js';
import { t } from '../i18n/index.js';

function stepNames() {
  return [
    t('credentials.stepGenerateOwner'),
    t('credentials.stepKms'),
    t('credentials.stepCreate2'),
    t('credentials.stepRelayer'),
    t('credentials.stepL1L2'),
    'batch approve',
    t('credentials.stepBalance'),
    t('credentials.stepPersist'),
  ];
}

export async function credentialsPage() {
  const c = el('div', { class: 'container' });
  c.appendChild(el('h1', { text: t('credentials.title') }));

  // Polymarket 卡（含可展开预配状态机）
  const polyCard = el('div', { class: 'card' }, [skeleton(2)]);
  c.appendChild(polyCard);
  try {
    const [creds, d] = await Promise.all([
      listCredentials().catch(() => []),
      getDelegation().catch(() => null),
    ]);
    polyCard.innerHTML = '';
    const polyCred = creds.find(cr => cr.platform === 'polymarket');
    const kindLabel = kindDisplay(polyCred?.kind);
    polyCard.appendChild(el('div', { class: 'section-title', text: 'Polymarket' }));
    polyCard.appendChild(el('p', {}, [
      el('strong', { text: kindLabel + ' ' }),
      el('span', { class: 'muted', text: d ? (d.provision_live ? t('credentials.live') : t('credentials.offline')) : (polyCred ? t('credentials.configured') : t('credentials.notProvisioned')) }),
    ]));
    if (polyCred?.kind) {
      polyCard.appendChild(el('p', { class: 'muted', text: `kind: ${polyCred.kind}` }));
    }
    if (d) {
      polyCard.appendChild(el('p', { class: 'muted', text: `Deposit Wallet ${d.deposit_wallet_address || '—'} · Owner ${d.owner_address || '—'}` }));
      const stepper = el('details', { class: 'advanced', open: true });
      stepper.appendChild(el('summary', { text: t('credentials.stepperTitle') }));
      stepper.appendChild(provisionStepper(d.provision_steps, d.provision_live));
      polyCard.appendChild(stepper);
    }
    polyCard.appendChild(el('div', { class: 'row' }, [
      el('button', { text: t('credentials.viewDelegation'), onclick: () => navigate('/settings/delegation') }),
      el('button', { text: t('credentials.reprovision'), onclick: async () => {
        let msg = t('credentials.reprovisionConfirm');
        let needDouble = false;
        try {
          const { getWallet, getPortfolio } = await import('../lib/copier.js');
          const [w, p] = await Promise.all([
            getWallet().catch(() => null),
            getPortfolio({ period: '1m' }).catch(() => null),
          ]);
          const positions = (p && Array.isArray(p.positions)) ? p.positions : [];
          const openCount = positions.filter((x) => Math.abs(Number(x.size) || 0) > 1e-9).length;
          const openCost = positions.reduce((s, x) => s + (Number(x.cost_basis) || 0), 0);
          const addr = String((w && w.deposit_wallet_address) || '').slice(0, 10) + '…';
          if (openCount > 0) {
            msg = t('delegation.reprovisionConfirmPositions', {
              address: addr,
              count: String(openCount),
              cost: String(openCost),
            });
            needDouble = true;
          } else if (w && w.cash_balance != null && Number(w.cash_balance) > 0) {
            msg = t('delegation.reprovisionConfirmBalance', {
              address: addr,
              balance: String(w.cash_balance),
            });
          }
        } catch (_) { /* ignore */ }
        if (!confirm(msg)) return;
        if (needDouble && !confirm(t('delegation.reprovisionConfirmPositionsAgain'))) return;
        try {
          await provisionDepositWallet({ confirm_replace: true });
          toast(t('credentials.reprovisionSuccess'), 'success');
          remount();
        } catch (e) { toast(e.message, 'error'); }
      } }),
      el('button', { class: 'danger', disabled: true, text: t('credentials.revokePhase2') }),
    ]));
  } catch (e) {
    polyCard.innerHTML = '';
    polyCard.appendChild(el('p', { class: 'neg', text: t('credentials.loadError', { message: e.message }) }));
  }

  // Kalshi / Manifold 占位
  c.appendChild(phaseCard('Kalshi', t('credentials.kalshiDesc'), 'Phase 3'));
  c.appendChild(phaseCard('Manifold', t('credentials.manifoldDesc'), 'Phase 2'));

  // 通道B · daemon API key（跨 Venue）— 简要状态 + 跳转专用页
  c.appendChild(el('h2', { text: t('credentials.daemonSectionTitle') }));
  const keyCard = el('div', { class: 'card' });
  c.appendChild(keyCard);
  keyCard.appendChild(el('p', { class: 'muted', text: t('credentials.daemonSectionDesc') }));
  keyCard.appendChild(el('a', { href: '#/settings/daemon-key', text: t('credentials.daemonManage') }));

  return withShell(c);
}

function phaseCard(name, desc, phase) {
  return el('div', { class: 'card', style: 'opacity:0.7' }, [
    el('div', { class: 'section-title', text: name }),
    el('p', {}, [el('strong', { text: desc }), el('span', { class: 'muted', text: `  🔒 ${phase}` })]),
    el('button', { disabled: true, text: t('credentials.configureLocked', { phase }) }),
  ]);
}

/// kind → 用户可见文案。对应 Credential enum / CHANNEL_A_SIGNING。
function kindDisplay(kind) {
  switch (kind) {
    case 'deposit_wallet_delegated': return t('credentials.kindDelegated');
    case 'wallet': return t('credentials.kindSession');
    case 'kyc_api_key': return 'KYC + API key';
    case 'api_key': return 'API key';
    default: return kind ? t('credentials.kindFallback', { kind }) : t('credentials.kindDelegated');
  }
}

function provisionStepper(steps, live) {
  const names = stepNames();
  const wrap = el('div', { style: 'display:flex;flex-wrap:wrap;gap:8px' });
  (steps || []).forEach((st, i) => {
    const status = typeof st === 'string' ? st : 'done';
    const icon = status === 'done' ? '✅' : status === 'skipped' ? '⏭' : status === 'pending' ? '⏳' : '❌';
    wrap.appendChild(el('div', { style: `border:1px solid var(--c-border);border-radius:6px;padding:6px 10px;${status === 'failed' ? 'border-color:var(--c-down)' : ''}` }, [
      el('span', { text: `${i + 1}${icon} `, title: names[i] }),
      el('span', { class: 'muted', style: 'font-size:11px', text: names[i] }),
    ]));
  });
  void live;
  return wrap;
}
