// api/account.js · account 服务端点封装。对应 docs/FRONTEND_DESIGN.md §6.8/§6.12/§6.13/§6.5。
// 身份方式：钱包登录（SIWE）或 TG 登录。邮箱认证已移除。
import { get, post, del, qs } from './client.js';

/// GET /account/auth/wallet/nonce?address=0x... — 钱包登录：签发一次性 nonce。
export const walletNonce = (address) =>
  get(`/account/auth/wallet/nonce${qs({ address })}`);

/// POST /account/auth/wallet — 钱包登录：SIWE 验签。body: { message, signature }
export const walletLogin = (body) => post('/account/auth/wallet', body);

/// GET /account/me — 当前用户信息。
export const me = () => get('/account/me');

/// POST /account/me/subscription — 更新订阅。body: { tier, until? }
export const updateSubscription = (body) => post('/account/me/subscription', body);

/// GET /account/me/venue-credentials — 凭证列表（blob 被 skip，仅返 platform/proxy_address）。
export const listCredentials = () => get('/account/me/venue-credentials');

/// GET /account/me/delegation — 委托管理安全视图（非密字段）。对应 §6.4。
export const getDelegation = () => get('/account/me/delegation');

/// POST /account/me/venue-credentials/{platform} — upsert 凭证（加密 blob 由调用方构造）。
export const upsertCredential = (platform, body) =>
  post(`/account/me/venue-credentials/${encodeURIComponent(platform)}`, body);

/// POST /account/me/daemon-api-key — 轮换 daemon key（明文仅返一次）。
export const rotateDaemonKey = () => post('/account/me/daemon-api-key');

/// POST /account/me/deposit-wallet/provision — 预配 Polymarket deposit wallet。
export const provisionDepositWallet = (body = {}) =>
  post('/account/me/deposit-wallet/provision', { builder_code: 'sharpside-builder', ...body });

// ── 已登录用户多钱包管理（恢复因子）──

/// GET /account/me/wallets — 列出当前用户所有钱包。
export const listWallets = () => get('/account/me/wallets');

/// POST /account/me/wallets — 绑定第二个钱包。body: { address, label? }
export const linkWallet = (body) => post('/account/me/wallets', body);

/// DELETE /account/me/wallets/:address — 解绑钱包。
export const unlinkWallet = (address) =>
  del(`/account/me/wallets/${encodeURIComponent(address)}`);
