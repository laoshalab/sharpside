// pages/delegation.js · 委托管理。对应 docs/FRONTEND_DESIGN.md §6.4。
import { el, skeleton, emptyState, fmtUSD } from '../components/ui.js';
import { withShell } from '../components/nav.js';
import {
  getDelegation,
  listDelegationArchives,
  provisionDepositWallet,
  revokeDepositWallet,
  migrateArchiveDeposit,
  listArchiveRedeemable,
  redeemArchive,
} from '../lib/account.js';
import { getWallet, getPortfolio } from '../lib/copier.js';
import { toast } from '../store/toast.js';
import { t } from '../i18n/index.js';
import { copyText } from '../lib/clipboard.js';

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

      card.appendChild(custodyBanner(d));

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

      card.appendChild(el('h3', { text: t('delegation.provisionStatus') }));
      card.appendChild(provisionStepper(d.provision_steps, d.provision_live));
      card.appendChild(el('p', { class: 'muted', text: d.provision_live ? t('delegation.provisionLive') : t('delegation.provisionOffline') }));

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

      // 历史 Deposit Wallet（L1/L2）
      card.appendChild(el('h3', { text: t('delegation.archivesTitle') }));
      card.appendChild(el('p', { class: 'muted', text: t('delegation.archivesIntro') }));
      const archivesBox = el('div', { class: 'card', style: 'margin-top:8px' }, [skeleton(2)]);
      card.appendChild(archivesBox);
      renderArchives(archivesBox, d.deposit_wallet_address, render);

      card.appendChild(el('div', { class: 'row', style: 'margin-top:12px' }, [
        el('button', { class: 'primary', text: t('delegation.reprovision'), onclick: async () => {
          if (!(await confirmReprovision(d))) return;
          try {
            await provisionDepositWallet({ confirm_replace: true });
            toast(t('delegation.reprovisionSuccess'), 'success');
            render();
          } catch (e) {
            toast(e.message, 'error');
          }
        } }),
      ]));

      card.appendChild(el('h3', { text: t('delegation.revokeTitle') }));
      if (d.revoked_at) {
        card.appendChild(el('div', { class: 'card', style: 'border-left:4px solid var(--c-down)' }, [
          el('div', { style: 'display:flex;align-items:center;gap:8px' }, [
            el('span', { style: 'font-size:20px', text: '🔒' }),
            el('strong', { text: t('delegation.revokedBadge') }),
          ]),
          el('p', { class: 'muted', style: 'margin-top:4px', text: t('delegation.revokedNote', { at: new Date(d.revoked_at).toLocaleString() }) }),
        ]));
        card.appendChild(el('button', { disabled: true, class: 'danger', text: t('delegation.revokedBadge') }));
      } else {
        card.appendChild(el('button', { class: 'danger', disabled: !d.can_revoke, text: t('delegation.revoke'), onclick: async () => {
          if (!confirm(t('delegation.revokeConfirm'))) return;
          try { await revokeDepositWallet(); toast(t('delegation.revokeSuccess'), 'success'); render(); } catch (e) { toast(e.message, 'error'); }
        } }));
        card.appendChild(el('p', { class: 'muted', text: t('delegation.revokeNote') }));
      }

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

/** L3：有未平仓 → 双次强警告；仅有现金 → 强化确认；否则普通确认。 */
async function confirmReprovision(d) {
  let bal = null;
  let openCount = 0;
  let openCost = 0;
  try {
    const [w, p] = await Promise.all([
      getWallet().catch(() => null),
      getPortfolio({ period: '1m' }).catch(() => null),
    ]);
    bal = w && w.cash_balance != null ? Number(w.cash_balance) : null;
    const positions = (p && Array.isArray(p.positions)) ? p.positions : [];
    openCount = positions.filter((x) => Math.abs(Number(x.size) || 0) > 1e-9).length;
    openCost = positions.reduce((s, x) => s + (Number(x.cost_basis) || 0), 0);
  } catch (_) { /* ignore */ }

  const addr = String(d.deposit_wallet_address || '').slice(0, 10) + '…';

  if (openCount > 0) {
    if (!confirm(t('delegation.reprovisionConfirmPositions', {
      address: addr,
      count: String(openCount),
      cost: fmtUSD(openCost),
    }))) return false;
    return confirm(t('delegation.reprovisionConfirmPositionsAgain'));
  }
  if (bal != null && bal > 0) {
    return confirm(t('delegation.reprovisionConfirmBalance', {
      address: addr,
      balance: fmtUSD(bal),
    }));
  }
  return confirm(t('delegation.reprovisionConfirm'));
}

async function renderArchives(box, currentDw, onMigrated) {
  try {
    const rows = await listDelegationArchives();
    box.innerHTML = '';
    if (!rows || !rows.length) {
      box.appendChild(el('p', { class: 'muted', text: t('delegation.archivesEmpty') }));
      return;
    }
    for (const a of rows) {
      const dw = a.deposit_wallet_address || '—';
      const bal = a.onchain_balance;
      const hasBal = bal != null && Number(bal) > 0;
      const balText = bal == null
        ? shortenBalanceNote(a.balance_note) || t('delegation.archiveBalanceUnknown')
        : (hasBal ? fmtUSD(bal) + ' pUSD' : t('delegation.archiveMigratedZero'));
      const mode = a.provision_live === true
        ? t('delegation.archiveOnline')
        : a.provision_live === false
          ? t('delegation.archiveOffline')
          : '—';
      const row = el('div', {
        style: 'padding:10px 0;border-bottom:1px solid var(--c-border)',
      });
      const head = el('div', {
        style: 'display:flex;flex-wrap:wrap;gap:10px;align-items:flex-start;justify-content:space-between',
      });
      head.appendChild(el('div', { style: 'flex:1;min-width:220px' }, [
        copyField(dw),
        el('div', { class: 'muted', style: 'margin-top:4px;font-size:12px' }, [
          el('span', { text: `${t('delegation.archiveBalance')}: ${balText}` }),
          el('span', { text: ` · ${mode}` }),
          el('span', { text: ' · ' + t('delegation.archiveAt', { at: new Date(a.archived_at).toLocaleString() }) }),
        ]),
      ]));
      const actions = el('div', { style: 'display:flex;flex-wrap:wrap;gap:6px' });
      if (hasBal && currentDw) {
        actions.appendChild(el('button', {
          class: 'primary sm',
          text: t('delegation.migrateToCurrent'),
          onclick: async () => {
            const msg = t('delegation.migrateConfirm', {
              amount: fmtUSD(bal),
              from: shortAddr(dw),
              to: shortAddr(currentDw),
            });
            if (!confirm(msg)) return;
            try {
              const r = await migrateArchiveDeposit(a.id);
              toast(t('delegation.migrateSuccess', { amount: fmtUSD(r.amount) }), 'success');
              onMigrated();
            } catch (e) {
              toast(t('delegation.migrateError', { message: e.message }), 'error');
            }
          },
        }));
      }
      head.appendChild(actions);
      row.appendChild(head);

      // 可赎回仓位（懒加载）
      const redeemBox = el('div', { style: 'margin-top:8px;padding-left:4px' });
      row.appendChild(redeemBox);
      redeemBox.appendChild(el('span', { class: 'muted', style: 'font-size:12px', text: t('delegation.archiveRedeemable') + '…' }));
      listArchiveRedeemable(a.id).then((items) => {
        redeemBox.innerHTML = '';
        if (!items || !items.length) {
          redeemBox.appendChild(el('p', { class: 'muted', style: 'font-size:12px;margin:0', text: t('delegation.archiveRedeemableEmpty') }));
          return;
        }
        for (const it of items) {
          const line = el('div', {
            style: 'display:flex;flex-wrap:wrap;gap:8px;align-items:center;margin:4px 0;font-size:13px',
          });
          line.appendChild(el('span', {
            text: `${it.title || it.condition_id.slice(0, 10)} · ${it.outcome} · ${fmtUSD(it.amount)} pUSD`,
          }));
          if (it.already_redeemed) {
            line.appendChild(el('span', { class: 'muted', text: t('wallet.redeemInProgress') }));
          } else {
            line.appendChild(el('button', {
              class: 'sm primary',
              text: t('delegation.archiveRedeem'),
              onclick: async (ev) => {
                const btn = ev.currentTarget;
                if (!confirm(t('delegation.archiveRedeemConfirm', {
                  title: it.title || it.condition_id.slice(0, 12),
                  amount: fmtUSD(it.amount),
                }))) return;
                btn.disabled = true;
                btn.textContent = t('delegation.archiveRedeeming');
                try {
                  const r = await redeemArchive(a.id, it.condition_id);
                  toast(t('delegation.archiveRedeemSuccess', { amount: fmtUSD(r.amount) }), 'success');
                  onMigrated();
                } catch (e) {
                  toast(t('delegation.archiveRedeemError', { message: e.message }), 'error');
                  btn.disabled = false;
                  btn.textContent = t('delegation.archiveRedeem');
                }
              },
            }));
          }
          redeemBox.appendChild(line);
        }
      }).catch((e) => {
        redeemBox.innerHTML = '';
        redeemBox.appendChild(el('p', {
          class: 'neg',
          style: 'font-size:12px;margin:0',
          text: t('delegation.archiveRedeemLoadError', { message: e.message }),
        }));
      });

      box.appendChild(row);
    }
  } catch (e) {
    box.innerHTML = '';
    box.appendChild(el('p', { class: 'neg', text: t('delegation.archivesLoadError', { message: e.message }) }));
  }
}

function shortAddr(a) {
  const s = String(a || '');
  if (s.length < 12) return s;
  return s.slice(0, 8) + '…' + s.slice(-4);
}

function shortenBalanceNote(note) {
  if (!note) return '';
  const s = String(note);
  if (/RPC|eth_call|请求失败|网络/i.test(s)) {
    return t('delegation.archiveBalanceRpcHint');
  }
  return s.length > 80 ? s.slice(0, 80) + '…' : s;
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
  (steps || []).forEach((st, i) => {
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
  wrap.appendChild(el('button', { class: 'sm', text: t('common.copy'), onclick: () => copyText(String(text)) }));
  return wrap;
}
