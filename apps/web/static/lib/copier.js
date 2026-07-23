// api/copier.js · copier 服务端点封装。对应 docs/FRONTEND_DESIGN.md §6.11/§6.3。
import { get, post, qs } from './client.js';

/// GET /copier/me/copy-executions?since=&limit=&offset=&follow_id=&venue=&status=
/// 用户成交历史（服务端过滤）。对应 §6.11。
export const listCopyExecutions = (params = {}) =>
  get('/copier/me/copy-executions' + qs({
    since: params.since,
    limit: params.limit,
    offset: params.offset,
    follow_id: params.follow_id,
    venue: params.venue,
    status: params.status,
  }));

/// GET /copier/me/copy-orders/recent?limit= — 用户近期跟单指令（所有状态，含 skip_reason）。
/// 用于展示失败/跳过原因（余额不足/股数不够/滑点超限/Polymarket 拒单等）。
export const listRecentOrders = (params = {}) =>
  get('/copier/me/copy-orders/recent' + qs({ limit: params.limit }));

/// GET /copier/me/portfolio?period= — 投资组合聚合（后端补点，未就绪返 404 降级）。
export const getPortfolio = (params = {}) =>
  get('/copier/me/portfolio' + qs({ period: params.period }));

/// GET /copier/me/wallet — 钱包视图（deposit wallet 地址 + 实时 pUSD 余额 + 预配状态）。充值页用。
export const getWallet = () => get('/copier/me/wallet');

/// POST /copier/me/wallet/withdraw — 提现 pUSD 到用户绑定钱包。body: { to, amount }
/// 风控：目标须为绑定钱包、金额上下限、余额校验、日累计上限。后端落库审计。
export const withdraw = (body) => post('/copier/me/wallet/withdraw', body);

/// GET /copier/me/wallet/withdrawals?limit=&offset= — 提现历史（最近优先）。
export const listWithdrawals = (params = {}) =>
  get('/copier/me/wallet/withdrawals' + qs({ limit: params.limit, offset: params.offset }));

/// GET /copier/me/wallet/redeemable — 可赎回列表（已结算市场的赢仓位，链上 balanceOf > 0）。
export const listRedeemable = () => get('/copier/me/wallet/redeemable');

/// POST /copier/me/wallet/redeem — 手动赎回单市场赢仓位。body: { condition_id }
/// 把已结算市场赢仓位 CTF token 换回 pUSD（转入 deposit wallet，纯收益，无金额风控）。
export const redeem = (body) => post('/copier/me/wallet/redeem', body);

/// GET /copier/me/wallet/redemptions?limit=&offset= — 赎回历史（最近优先，含 auto/manual 来源）。
export const listRedemptions = (params = {}) =>
  get('/copier/me/wallet/redemptions' + qs({ limit: params.limit, offset: params.offset }));
