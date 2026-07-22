// api/bff.js · gateway BFF 端点封装。对应 docs/FRONTEND_DESIGN.md §6.6 仪表盘。
import { get } from './client.js';

/// GET /me/dashboard — 仪表盘聚合（JWT 鉴权，BFF）。
export const getDashboard = () => get('/me/dashboard');
