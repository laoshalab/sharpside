// api/account.js · account 服务端点封装。对应 docs/FRONTEND_DESIGN.md §6.8/§6.12/§6.13/§6.5。
// 身份方式：钱包登录（SIWE）或 TG 登录。邮箱认证已移除。
import { get, post, del, qs } from './client.js';

/// GET /account/auth/wallet/nonce?address=0x... — 钱包登录：签发一次性 nonce。
export const walletNonce = (address) =>
  get(`/account/auth/wallet/nonce${qs({ address })}`);

/// POST /account/auth/wallet — 钱包登录：SIWE 验签。body: { message, signature }
export const walletLogin = (body) => post('/account/auth/wallet', body);

/// POST /account/auth/logout — 吊销当前 JWT（写 jti 入 denylist）。
/// 调用后该 token 在所有服务立即失效。前端登出应先调此再 clearToken。
export const logout = () => post('/account/auth/logout');

/// GET /account/me — 当前用户信息。
export const me = () => get('/account/me');

/// POST /account/me/subscription — 更新订阅。body: { tier, until? }
/// 生产环境禁止自助升 pro_plus；取消（free）仍可用。付费升档走 billing。
export const updateSubscription = (body) => post('/account/me/subscription', body);

/// POST /account/me/billing/invoices — 创建或返回活跃 pending 发票。body: { period_days?: 30|90 }
export const createBillingInvoice = (body = {}) =>
  post('/account/me/billing/invoices', body);

/// GET /account/me/billing/invoices/active — 当前 pending 发票（无则 null）。
export const getActiveBillingInvoice = () => get('/account/me/billing/invoices/active');

/// POST /account/me/billing/invoices/:id/submit-tx — 可选：粘贴 tx 加速确认。
export const submitBillingTx = (invoiceId, txHash) =>
  post(`/account/me/billing/invoices/${encodeURIComponent(invoiceId)}/submit-tx`, {
    tx_hash: txHash,
  });

/// GET /account/me/billing/history — 发票与支付历史。
export const getBillingHistory = () => get('/account/me/billing/history');

/// GET /account/me/venue-credentials — 凭证列表（blob 被 skip，仅返 platform/proxy_address）。
export const listCredentials = () => get('/account/me/venue-credentials');

/// GET /account/me/delegation — 委托管理安全视图（非密字段）。对应 §6.4。
export const getDelegation = () => get('/account/me/delegation');

/// GET /account/me/delegation/archives — 历史 Deposit Wallet（重新预配归档，含链上余额）。
export const listDelegationArchives = () => get('/account/me/delegation/archives');

/// POST /account/me/daemon-api-key — 轮换 daemon key（明文仅返一次）。
export const rotateDaemonKey = () => post('/account/me/daemon-api-key');

/// POST /account/me/deposit-wallet/provision — 预配 Polymarket deposit wallet。
/// 已有活跃凭证时须 `confirm_replace: true`（旧密文归档）。
export const provisionDepositWallet = (body = {}) =>
  post('/account/me/deposit-wallet/provision', { builder_code: 'sharpside-builder', ...body });

/// POST /account/me/deposit-wallet/revoke — 撤销委托凭证（安全修复 2.2，不可逆）。
/// 调用后 copier 不再为该凭证下单；重新预配（provision）才会生成新凭证并重置撤销态。
export const revokeDepositWallet = () => post('/account/me/deposit-wallet/revoke');

/// POST /account/me/deposit-wallet/migrate-archive — 将归档 DW 全部 pUSD 迁到当前活跃 DW。
export const migrateArchiveDeposit = (archiveId) =>
  post('/account/me/deposit-wallet/migrate-archive', { archive_id: archiveId });

/// GET /account/me/deposit-wallet/archives/:id/redeemable — 归档旧 DW 可赎回列表。
export const listArchiveRedeemable = (archiveId) =>
  get(`/account/me/deposit-wallet/archives/${encodeURIComponent(archiveId)}/redeemable`);

/// POST /account/me/deposit-wallet/archives/:id/redeem — 在归档旧 DW 上赎回。
export const redeemArchive = (archiveId, conditionId) =>
  post(`/account/me/deposit-wallet/archives/${encodeURIComponent(archiveId)}/redeem`, {
    condition_id: conditionId,
  });

// ── 已登录用户多钱包管理（恢复因子）──

/// GET /account/me/wallets — 列出当前用户所有钱包。
export const listWallets = () => get('/account/me/wallets');

/// POST /account/me/wallets — 绑定第二个钱包（须 SIWE 验签证明所有权）。
/// 流程：openWalletPicker 选钱包 → walletNonce(address) 取 nonce → buildSiwe → personalSign
///       → linkWallet({ message, signature, label? })。地址由后端从验签消息权威导出。
export const linkWallet = (body) => post('/account/me/wallets', body);

/// DELETE /account/me/wallets/:address — 解绑钱包。
export const unlinkWallet = (address) =>
  del(`/account/me/wallets/${encodeURIComponent(address)}`);
