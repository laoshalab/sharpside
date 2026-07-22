// api/follow.js · follow 服务端点封装。对应 docs/FRONTEND_DESIGN.md §6.9/§6.10。
import { get, post, patch, del } from './client.js';

/// POST /follow/follows — 创建跟随（trader 或 identity）。
/// body: { follow_platform, follow_address } | { follow_identity_id }, execute_venue, channel, config
export const createFollow = (body) => post('/follow/follows', body);

/// GET /follow/follows — 我的跟随列表。
/// 注：设计文档 §6.9 写的是 `/follow/me/follows`，但 follow 服务实际暴露 `GET /follows`（AuthUser 提取 user_id）。
/// 此处对齐后端实际路由；后端补 `/me/follows` 别名后可切回。
export const listMyFollows = () => get('/follow/follows');

/// PATCH /follow/follows/{id} — 更新跟随（active/config/execute_venue/channel）。
export const updateFollow = (id, body) => patch(`/follow/follows/${encodeURIComponent(id)}`, body);

/// DELETE /follow/follows/{id} — 删除跟随。
export const deleteFollow = (id) => del(`/follow/follows/${encodeURIComponent(id)}`);
