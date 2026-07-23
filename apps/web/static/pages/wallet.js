// pages/wallet.js · 钱包：充值 + 提现 + 提现历史。对应 docs/FRONTEND_DESIGN.md §6.5。
//
// 充值：本质是用户从外部钱包向 deposit wallet 地址转 pUSD（平台无法代发起），
//       故"充值"= 展示地址 + 复制 + 实时余额 + 刷新 + 充值指引。
// 提现：owner EOA（平台 KMS 代签）签 WALLET batch 调 pUSD.transfer(to, amount)，relayer gasless 提交。
//       高敏——目标限用户绑定钱包、二次确认弹窗、后端单笔/日上限校验。
import { el, skeleton, emptyState, dataTable, fmtUSD } from '../components/ui.js';
import { withShell } from '../components/nav.js';
import { getWallet, withdraw, listWithdrawals, listRedeemable, redeem, listRedemptions } from '../lib/copier.js';
import { listWallets } from '../lib/account.js';
import { toast } from '../store/toast.js';
import { t } from '../i18n/index.js';

// pUSD（collateral）合约地址，Polygon 主网。与 wallet_batch::contracts::COLLATERAL / PUSD_CONST 一致。
const PUSD_CONTRACT = '0xC011a7E12a19f7B1f670d46F03B03f3342E82DFB';

export async function walletPage() {
  const c = el('div', { class: 'container' });
  c.appendChild(el('h1', { text: t('wallet.pageTitle') }));

  // 充值卡
  const rechargeCard = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(el('h2', { text: t('wallet.rechargeTitle') }));
  c.appendChild(rechargeCard);

  // 提现卡
  c.appendChild(el('h2', { text: t('wallet.withdrawTitle') }));
  const withdrawCard = el('div', { class: 'card' });
  c.appendChild(withdrawCard);

  // 提现历史
  c.appendChild(el('h2', { text: t('wallet.withdrawHistoryTitle') }));
  const histCard = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(histCard);

  // 赎回卡（已结算市场赢仓位 → pUSD）
  c.appendChild(el('h2', { text: t('wallet.redeemTitle') }));
  const redeemCard = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(redeemCard);

  // 赎回历史
  c.appendChild(el('h2', { text: t('wallet.redeemHistoryTitle') }));
  const redeemHistCard = el('div', { class: 'card' }, [skeleton(3)]);
  c.appendChild(redeemHistCard);

  let currentWallet = null;

  async function renderRecharge() {
    rechargeCard.innerHTML = '';
    rechargeCard.appendChild(skeleton(2));
    try {
      const w = await getWallet();
      currentWallet = w;
      rechargeCard.innerHTML = '';

      // 余额（刷新按钮在金额右侧）
      const bal = w.cash_balance;
      const balText = bal == null ? '—' : fmtUSD(bal);
      const balCls = bal == null ? 'muted' : '';
      rechargeCard.appendChild(el('div', { class: 'kpi-grid' }, [
        el('div', { class: 'kpi' }, [
          el('div', { class: 'label', text: t('wallet.balanceLabel') }),
          el('div', { style: 'display:flex;align-items:center;gap:var(--sp-3);flex-wrap:wrap;margin-top:var(--sp-1)' }, [
            el('div', { class: 'value ' + balCls, style: 'margin-top:0', text: balText }),
            el('button', {
              class: 'sm',
              text: t('wallet.refreshBalance'),
              onclick: async () => {
                toast(t('wallet.refreshing'), 'info');
                await renderRecharge();
              },
            }),
          ]),
          el('div', { class: 'sub', text: w.balance_note || (w.provision_live ? t('wallet.balanceLive') : t('wallet.balanceOffline')) }),
        ]),
      ]));

      // 充值地址
      rechargeCard.appendChild(el('h3', { text: t('wallet.depositAddressTitle') }));
      if (w.deposit_wallet_address) {
        rechargeCard.appendChild(copyField(w.deposit_wallet_address, t('wallet.copyDeposit')));
        rechargeCard.appendChild(el('p', { class: 'muted', style: 'margin-top:8px', text: t('wallet.depositHint') }));
      } else {
        rechargeCard.appendChild(el('p', { class: 'muted', text: t('wallet.noDepositWallet') }));
      }

      // pUSD 合约地址（Polygon）：外部钱包添加自定义代币 / 核对转账用
      rechargeCard.appendChild(el('h3', { style: 'margin-top:16px', text: t('wallet.contractTitle') }));
      rechargeCard.appendChild(copyField(PUSD_CONTRACT, t('wallet.copyContract')));
      rechargeCard.appendChild(el('p', { class: 'muted', style: 'margin-top:8px', text: t('wallet.contractHint') }));
    } catch (e) {
      rechargeCard.innerHTML = '';
      if (e.status === 404) {
        rechargeCard.appendChild(emptyState({ icon: '🔑', text: t('wallet.noCredential'), action: el('a', { href: '#/settings/delegation', class: 'primary', text: t('wallet.gotoDelegation') }) }));
      } else {
        rechargeCard.appendChild(el('p', { class: 'neg', text: t('wallet.loadError', { message: e.message }) }));
      }
    }
  }

  function renderWithdrawForm() {
    withdrawCard.innerHTML = '';
    const live = currentWallet && currentWallet.provision_live;
    const bal = currentWallet ? currentWallet.cash_balance : null;

    withdrawCard.appendChild(el('p', { class: 'muted', text: t('wallet.withdrawIntro') }));

    const form = el('div', { class: 'field' });
    // 目标地址下拉（绑定钱包）
    const addrSel = el('select', { id: 'withdraw-to' });
    addrSel.appendChild(el('option', { value: '', text: t('wallet.selectWallet') }));
    form.appendChild(el('label', { text: t('wallet.withdrawTo') }));
    form.appendChild(addrSel);

    const amountInput = el('input', { type: 'number', id: 'withdraw-amount', placeholder: t('wallet.amountPlaceholder'), step: '0.01', min: '0' });
    form.appendChild(el('label', { text: t('wallet.amountLabel'), style: 'margin-top:8px' }));
    form.appendChild(amountInput);

    withdrawCard.appendChild(form);

    // 余额提示
    withdrawCard.appendChild(el('p', { class: 'muted', style: 'margin-top:6px', text: bal == null ? t('wallet.balanceUnknown') : t('wallet.availableBalance', { balance: fmtUSD(bal) }) }));

    const submitBtn = el('button', { class: 'primary', style: 'margin-top:10px', text: t('wallet.withdrawTitle'), disabled: !live });
    withdrawCard.appendChild(submitBtn);

    if (!live) {
      withdrawCard.appendChild(el('p', { class: 'muted', style: 'margin-top:6px', text: t('wallet.offlineWarning') }));
    }

    // 加载绑定钱包地址
    listWallets().then(ws => {
      for (const w of (ws || [])) {
        const label = (w.label ? w.label + ' · ' : '') + w.address.slice(0, 8) + '…' + w.address.slice(-4);
        addrSel.appendChild(el('option', { value: w.address, text: label }));
      }
    }).catch(() => {});

    submitBtn.onclick = async () => {
      const to = addrSel.value;
      const amount = parseFloat(amountInput.value);
      if (!to) { toast(t('wallet.toastSelectAddress'), 'error'); return; }
      if (!amount || amount <= 0) { toast(t('wallet.toastInvalidAmount'), 'error'); return; }
      if (bal != null && amount > bal) { toast(t('wallet.toastExceedsBalance', { balance: fmtUSD(bal) }), 'error'); return; }
      // 二次确认弹窗
      if (!confirm(t('wallet.confirmWithdraw', {
        amount: fmtUSD(amount),
        addressPrefix: to.slice(0, 10),
        addressSuffix: to.slice(-6),
      }))) return;
      submitBtn.disabled = true;
      submitBtn.textContent = t('common.submitting');
      try {
        const r = await withdraw({ to, amount });
        const ok = r.status === 'mined';
        const pending = r.status === 'pending';
        toast(
          ok ? t('wallet.toastWithdrawSuccess', { txHash: r.tx_hash ? r.tx_hash.slice(0, 10) + '…' : '—' }) :
          pending ? t('wallet.toastWithdrawPending') :
          t('wallet.toastWithdrawFailed', { reason: r.note || t('wallet.unknownReason') }),
          ok ? 'success' : pending ? 'info' : 'error'
        );
        amountInput.value = '';
        await renderRecharge();
        await renderHistory();
      } catch (e) {
        toast(e.message || t('wallet.toastWithdrawError'), 'error');
      } finally {
        submitBtn.disabled = !live;
        submitBtn.textContent = t('wallet.withdrawTitle');
      }
    };
  }

  async function renderHistory() {
    histCard.innerHTML = '';
    histCard.appendChild(skeleton(2));
    try {
      const rows = await listWithdrawals({ limit: 50 });
      histCard.innerHTML = '';
      if (!rows || rows.length === 0) {
        histCard.appendChild(emptyState({ text: t('wallet.withdrawEmpty') }));
        return;
      }
      histCard.appendChild(dataTable({
        columns: [
          { key: 'created_at', label: t('wallet.colTime'), render: r => r.created_at ? new Date(r.created_at).toLocaleString() : '—' },
          { key: 'to_address', label: t('wallet.colToAddress'), render: r => r.to_address ? `${escapeText(r.to_address.slice(0, 10))}…${escapeText(r.to_address.slice(-4))}` : '—' },
          { key: 'amount', label: t('wallet.colAmount'), render: r => fmtUSD(Number(r.amount)) },
          { key: 'status', label: t('wallet.colStatus'), render: r => {
              const s = r.status || '—';
              const cls = s === 'mined' ? 'pos' : s === 'failed' ? 'neg' : 'muted';
              const icon = s === 'mined' ? '✅' : s === 'failed' ? '❌' : '⏳';
              return `<span class="${cls}">${icon} ${s}</span>`;
            }
          },
          { key: 'tx_hash', label: t('wallet.colTxHash'), render: r => r.tx_hash ? `<span title="${escapeAttr(r.tx_hash)}">${escapeText(r.tx_hash.slice(0, 10))}…</span>` : '—' },
          { key: 'note', label: t('wallet.colNote'), render: r => r.note ? `<span class="neg" title="${escapeAttr(r.note)}">${escapeText(r.note).slice(0, 30)}${r.note.length > 30 ? '…' : ''}</span>` : '—' },
        ],
        rows,
      }));
    } catch (e) {
      histCard.innerHTML = '';
      histCard.appendChild(el('p', { class: 'neg', text: t('wallet.withdrawHistoryError', { message: e.message }) }));
    }
  }

  // ── 赎回：已结算市场赢仓位 CTF token → pUSD ──
  async function renderRedeem() {
    redeemCard.innerHTML = '';
    redeemCard.appendChild(skeleton(2));
    try {
      const items = await listRedeemable();
      redeemCard.innerHTML = '';
      if (!items || items.length === 0) {
        redeemCard.appendChild(emptyState({ icon: '✓', text: t('wallet.redeemEmpty') }));
        return;
      }
      redeemCard.appendChild(el('p', { class: 'muted', text: t('wallet.redeemIntro') }));
      redeemCard.appendChild(dataTable({
        columns: [
          { key: 'title', label: t('wallet.colMarket'), render: r => escapeText(r.title || r.condition_id.slice(0, 10) + '…') },
          { key: 'outcome', label: t('wallet.colOutcome'), render: r => `<span class="pos">${escapeText(r.outcome)}</span>` },
          { key: 'amount', label: t('wallet.colRedeemable'), render: r => fmtUSD(r.amount) + ' token' },
          { key: 'estimated_pusd', label: t('wallet.colEstimated'), render: r => fmtUSD(r.estimated_pusd) + ' pUSD' },
          { key: 'action', label: t('wallet.colAction'), render: r => {
              if (r.already_redeemed) return `<span class="muted">${t('wallet.redeemInProgress')}</span>`;
              return `<button class="sm primary" data-redeem="${escapeAttr(r.condition_id)}">${t('wallet.redeem')}</button>`;
            }
          },
        ],
        rows: items,
      }));
      redeemCard.querySelectorAll('[data-redeem]').forEach(btn => {
        btn.onclick = async () => {
          const condition_id = btn.getAttribute('data-redeem');
          if (!confirm(t('wallet.confirmRedeem'))) return;
          btn.disabled = true;
          btn.textContent = t('wallet.redeeming');
          try {
            const r = await redeem({ condition_id });
            const ok = r.status === 'mined';
            const pending = r.status === 'pending';
            toast(
              ok ? t('wallet.toastRedeemSuccess', { amount: fmtUSD(r.amount), txHash: r.tx_hash ? r.tx_hash.slice(0, 10) + '…' : '—' }) :
              pending ? t('wallet.toastRedeemPending') :
              t('wallet.toastRedeemFailed', { reason: r.note || t('wallet.unknownReason') }),
              ok ? 'success' : pending ? 'info' : 'error'
            );
            await renderRedeem();
            await renderRedeemHistory();
            await renderRecharge();
          } catch (e) {
            toast(e.message || t('wallet.toastRedeemError'), 'error');
            btn.disabled = false;
            btn.textContent = t('wallet.redeem');
          }
        };
      });
    } catch (e) {
      redeemCard.innerHTML = '';
      if (e.status === 404) {
        redeemCard.appendChild(emptyState({ icon: '🔑', text: t('wallet.redeemNoCredential') }));
      } else {
        redeemCard.appendChild(el('p', { class: 'neg', text: t('wallet.redeemLoadError', { message: e.message }) }));
      }
    }
  }

  async function renderRedeemHistory() {
    redeemHistCard.innerHTML = '';
    redeemHistCard.appendChild(skeleton(2));
    try {
      const rows = await listRedemptions({ limit: 50 });
      redeemHistCard.innerHTML = '';
      if (!rows || rows.length === 0) {
        redeemHistCard.appendChild(emptyState({ text: t('wallet.redeemHistoryEmpty') }));
        return;
      }
      redeemHistCard.appendChild(dataTable({
        columns: [
          { key: 'created_at', label: t('wallet.colTime'), render: r => r.created_at ? new Date(r.created_at).toLocaleString() : '—' },
          { key: 'condition_id', label: t('wallet.colMarket'), render: r => r.condition_id ? `<span title="${escapeAttr(r.condition_id)}">${escapeText(r.condition_id.slice(0, 10))}…</span>` : '—' },
          { key: 'outcome', label: t('wallet.colOutcome'), render: r => `<span class="pos">${escapeText(r.outcome)}</span>` },
          { key: 'amount', label: t('wallet.colSize'), render: r => fmtUSD(Number(r.amount)) },
          { key: 'source', label: t('wallet.colSource'), render: r => r.source === 'auto' ? `<span class="muted">${t('wallet.sourceAuto')}</span>` : t('wallet.sourceManual') },
          { key: 'status', label: t('wallet.colStatus'), render: r => {
              const s = r.status || '—';
              const cls = s === 'mined' ? 'pos' : s === 'failed' ? 'neg' : 'muted';
              const icon = s === 'mined' ? '✅' : s === 'failed' ? '❌' : '⏳';
              return `<span class="${cls}">${icon} ${s}</span>`;
            }
          },
          { key: 'tx_hash', label: t('wallet.colTxHash'), render: r => r.tx_hash ? `<span title="${escapeAttr(r.tx_hash)}">${escapeText(r.tx_hash.slice(0, 10))}…</span>` : '—' },
          { key: 'note', label: t('wallet.colNote'), render: r => r.note ? `<span class="neg" title="${escapeAttr(r.note)}">${escapeText(r.note).slice(0, 30)}${r.note.length > 30 ? '…' : ''}</span>` : '—' },
        ],
        rows,
      }));
    } catch (e) {
      redeemHistCard.innerHTML = '';
      redeemHistCard.appendChild(el('p', { class: 'neg', text: t('wallet.redeemHistoryError', { message: e.message }) }));
    }
  }

  // 先加载充值（拿到 wallet 状态），再渲染提现表单（依赖 currentWallet）+ 历史 + 赎回
  await renderRecharge();
  renderWithdrawForm();
  renderHistory();
  renderRedeem();
  renderRedeemHistory();

  return withShell(c);
}

function copyField(text, btnText = t('common.copy')) {
  const wrap = el('div', { style: 'display:flex;gap:6px;align-items:center;flex-wrap:wrap' });
  wrap.appendChild(el('code', { style: 'word-break:break-all', text: String(text) }));
  wrap.appendChild(el('button', { class: 'sm', text: btnText, onclick: () => { navigator.clipboard.writeText(String(text)); toast(t('common.copied'), 'success'); } }));
  return wrap;
}

function escapeText(s) {
  return String(s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}
function escapeAttr(s) {
  return escapeText(s).replace(/`/g, '&#96;');
}
