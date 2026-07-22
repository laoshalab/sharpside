// api/watchlist.js · follow 服务 watchlist 端点封装。对应 Watchlist 功能规划。
// 观察名单（纯收藏，不进执行路径）：trader / identity 二选一。
// 一键升级为 Follow 见 upgradeWatchlist（消费式升级，事务内删 watchlist + 建 follow）。
import { get, post, del } from './client.js';

/// POST /follow/watchlists — 创建收藏（trader 或 identity）。
/// body: { watch_platform, watch_address } | { watch_identity_id }
export const createWatchlist = (body) => post('/follow/watchlists', body);

/// GET /follow/me/watchlists — 我的观察名单。
export const listMyWatchlists = () => get('/follow/me/watchlists');

/// GET /follow/me/watchlists/{id} — 单条。
export const getMyWatchlist = (id) => get(`/follow/me/watchlists/${encodeURIComponent(id)}`);

/// DELETE /follow/watchlists/{id} — 删除收藏。
export const deleteWatchlist = (id) => del(`/follow/watchlists/${encodeURIComponent(id)}`);

/// POST /follow/watchlists/{id}/upgrade — 一键升级为 Follow。
/// body: { execute_venue, channel, config: FollowConfig }
/// 返回 { watchlist_id, ...FollowRelation 字段 }；升级后 watchlist 被删除。
export const upgradeWatchlist = (id, body) =>
  post(`/follow/watchlists/${encodeURIComponent(id)}/upgrade`, body);
