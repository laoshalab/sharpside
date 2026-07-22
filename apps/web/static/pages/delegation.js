// pages/delegation.js · 委托管理。对应 docs/FRONTEND_DESIGN.md §6.4。
import { el, skeleton, emptyState } from '../components/ui.js';
import { withShell } from '../components/nav.js';
import { getDelegation, provisionDepositWallet } from '../lib/account.js';
import { toast } from '../store/toast.js';
import { navigate } from '../router.js';
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

export async function delegationPage() {
  const c = el('div', { class: 'container' });
  c.appendChild(el('h1', { text: t('delegation.title') }));

  const card = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(card);

  async function render() {
    card.innerHTML = '';
    card.appendChild(skeleton(3));
    try {
      const d = await getDelegation();
      card.innerHTML = '';

      // 托管等级横幅
      card.appendChild(custodyBanner(d));

      // 资产权 / 交易权双卡
      card.appendChild(el('div', { class: 'row' }, [
        permissionCard(t('delegation.assetRights'), 'Deposit Wallet (ERC-1967)', d.deposit_wallet_address, [
          { ok: true, text: t('delegation.exportOwner') },
          { ok: false, text: t('delegation.assetStorage') },
        ]),
        permissionCard(t('delegation.tradingRights'), t('delegation.platformKms'), d.owner_address, [
          { ok: false, text: t('delegation.platformTransfer') },
          { ok: true, text: t('delegation.platformOrders') },
        ]),
      ]));

      // 预配状态机
      card.appendChild(el('h3', { text: t('delegation.provisionStatus') }));
      card.appendChild(provisionStepper(d.provision_steps, d.provision_live));
      card.appendChild(el('p', { class: 'muted', text: d.provision_live ? t('delegation.provisionLive') : t('delegation.provisionOffline') }));

      // 凭证详情
      card.appendChild(el('h3', { text: t('delegation.detailsTitle') }));
      card.appendChild(detailGrid([
        [t('delegation.labelPlatform'), d.platform],
        ['Deposit Wallet', d.deposit_wallet_address],
        ['Owner EOA', d.owner_address],
        ['L2 API Key', d.l2_api_key],
        ['Builder Code', d.builder_code],
        [t('delegation.labelMode'), d.provision_live ? t('delegation.modeOnline') : t('delegation.modeOffline')],
        ['KMS Key ID', d.kms_key_id],
      ]));
      card.appendChild(el('p', { class: 'muted', style: 'margin-top:8px', text: t('delegation.serverKeyNote') }));

      // 重新预配
      card.appendChild(el('div', { class: 'row' }, [
        el('button', { class: 'primary', text: t('delegation.reprovision'), onclick: async () => {
          if (!confirm(t('delegation.reprovisionConfirm'))) return;
          try { await provisionDepositWallet(); toast(t('delegation.reprovisionSuccess'), 'success'); render(); } catch (e) { toast(e.message, 'error'); }
        } }),
      ]));

      // 撤销委托（Phase 2 锁定）
      card.appendChild(el('h3', { text: t('delegation.revokeTitle') }));
      card.appendChild(el('button', { disabled: !d.can_revoke, class: d.can_revoke ? 'danger' : '', text: d.can_revoke ? t('delegation.revoke') : t('delegation.revokePhase2') }));
      card.appendChild(el('p', { class: 'muted', text: t('delegation.revokePhase2Note') }));

      // 升级非托管
      card.appendChild(el('h3', { text: t('delegation.selfCustodyTitle') }));
      card.appendChild(el('p', { class: 'muted', text: t('delegation.selfCustodyDesc') }));
      card.appendChild(el('button', { text: t('delegation.notifyMigration'), onclick: () => toast(t('delegation.notifySuccess'), 'success') }));
    } catch (e) {
      card.innerHTML = '';
      if (e.status === 404) {
        card.appendChild(emptyState({ icon: '🔑', text: t('delegation.notProvisioned'), action: el('button', { class: 'primary', text: t('delegation.provisionNow'), onclick: async () => {
          try { await provisionDepositWallet(); toast(t('delegation.provisionSuccess'), 'success'); render(); } catch (err) { toast(err.message, 'error'); }
        } }) }));
      } else {
        card.appendChild(el('p', { class: 'neg', text: t('delegation.loadError', { message: e.message }) }));
      }
    }
  }

  render();
  return withShell(c);
}

function custodyBanner(d) {
  const warn = !d.provision_live;
  return el('div', { class: 'card', style: `border-left:4px solid ${warn ? 'var(--c-warn)' : 'var(--c-up)'}` }, [
    el('div', { style: 'display:flex;align-items:center;gap:8px' }, [
      el('span', { style: 'font-size:20px', text: warn ? '⚠' : '✓' }),
      el('strong', { text: d.custody_label }),
      el('span', { class: 'muted', text: t('delegation.whatIsCustody') }),
    ]),
    el('p', { class: 'muted', style: 'margin-top:4px', text: t('delegation.channelADesc') }),
  ]);
}

function permissionCard(title, subtitle, address, perms) {
  return el('div', { class: 'card', style: 'flex:1;min-width:240px' }, [
    el('div', { class: 'section-title', text: title }),
    el('strong', { text: subtitle }),
    address ? el('div', { style: 'margin:8px 0' }, [copyField(address)]) : null,
    ...perms.map(p => el('div', { style: 'display:flex;gap:6px;align-items:center;padding:2px 0' }, [
      el('span', { text: p.ok ? '✓' : '✗', style: `color:${p.ok ? 'var(--c-up)' : 'var(--c-down)'}` }),
      el('span', { text: p.text }),
    ])),
  ]);
}

function provisionStepper(steps, live) {
  const names = stepNames();
  const wrap = el('div', { style: 'display:flex;flex-wrap:wrap;gap:8px' });
  steps.forEach((st, i) => {
    const status = typeof st === 'string' ? st : 'done';
    const color = status === 'done' ? 'var(--c-up)' : status === 'skipped' ? 'var(--c-muted)' : status === 'pending' ? 'var(--c-warn)' : 'var(--c-down)';
    const icon = status === 'done' ? '✅' : status === 'skipped' ? '⏭' : status === 'pending' ? '⏳' : '❌';
    wrap.appendChild(el('div', { style: `border:1px solid var(--c-border);border-radius:6px;padding:6px 10px;${status === 'failed' ? 'border-color:var(--c-down)' : ''}` }, [
      el('span', { text: `${i + 1}${icon} `, title: names[i] }),
      el('span', { class: 'muted', style: 'font-size:11px', text: names[i] }),
    ]));
    void color; void live;
  });
  return wrap;
}

function detailGrid(rows) {
  const grid = el('div', { style: 'display:grid;grid-template-columns:140px 1fr;gap:4px 12px' });
  for (const [k, v] of rows) {
    grid.appendChild(el('div', { class: 'muted', text: k }));
    grid.appendChild(el('div', { text: v == null ? '—' : String(v) }));
  }
  return grid;
}

function copyField(text) {
  const wrap = el('div', { style: 'display:flex;gap:6px;align-items:center' });
  wrap.appendChild(el('code', { style: 'word-break:break-all', text: String(text).slice(0, 14) + '…' + String(text).slice(-4) }));
  wrap.appendChild(el('button', { class: 'sm', text: t('common.copy'), onclick: () => { navigator.clipboard.writeText(String(text)); toast(t('common.copied'), 'success'); } }));
  return wrap;
}
