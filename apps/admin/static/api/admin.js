// api/admin.js · admin 端点组封装。对应 docs/FRONTEND_DESIGN.md §7 + P1 扩展。
import { get, post, put, patch, del, qs } from './client.js';

// ── 市场映射审核 §7.2 ──
export const listPendingMappings = () => get('/mappings/pending');
export const verifyMapping = (body) => post('/mappings/verify', body);
export const retireMapping = (body) => post('/mappings/retire', body);

// ── 身份审核 §7.3 ──
export const listPendingIdentities = () => get('/identities/pending');
export const verifyIdentity = (id, body) => post(`/identities/${encodeURIComponent(id)}/verify`, body);
export const deleteIdentity = (id) => del(`/identities/${encodeURIComponent(id)}`);

// ── 热钥管理 §7.4 ──
export const listHotWallets = (platform) => get('/hot-wallets' + qs({ platform }));
export const upsertHotWallet = (body) => post('/hot-wallets', body);
export const deleteHotWallet = (platform, address) =>
  del(`/hot-wallets/${encodeURIComponent(platform)}/${encodeURIComponent(address)}`);

// ── 标签阈值 §7.5 ──
export const listTagRules = () => get('/tag-rules');
export const upsertTagRule = (ruleId, body) => put(`/tag-rules/${encodeURIComponent(ruleId)}`, body);

// ── 分类映射 P1 ──
export const listCategoryMappings = (platform) =>
  get('/category-mappings' + qs({ platform }));
export const upsertCategoryMapping = (body) => put('/category-mappings', body);
export const deleteCategoryMapping = (platform, officialCategory) =>
  del(`/category-mappings/${encodeURIComponent(platform)}/${encodeURIComponent(officialCategory)}`);

// ── 交易者管控 §7.6 + is_hot / alias ──
export const listTraders = (params = {}) =>
  get('/traders' + qs({ platform: params.platform, q: params.q, limit: params.limit, offset: params.offset }));
export const setVisibility = (platform, address, visibility) =>
  patch(`/traders/${encodeURIComponent(platform)}/${encodeURIComponent(address)}/visibility`, { visibility });
export const setHot = (platform, address, is_hot) =>
  patch(`/traders/${encodeURIComponent(platform)}/${encodeURIComponent(address)}/hot`, { is_hot });
export const setAlias = (platform, address, alias) =>
  patch(`/traders/${encodeURIComponent(platform)}/${encodeURIComponent(address)}/alias`, { alias });

// ── 影子阈值 §7.7 ──
export const listAuditThresholds = () => get('/audit-thresholds');
export const upsertAuditThreshold = (metric, body) =>
  put(`/audit-thresholds/${encodeURIComponent(metric)}`, body);

// ── 数据健康 P1（SHADOW_MODE §8）──
export const shadowSummary = (hours = 24) => get('/shadow-health/summary' + qs({ hours }));
export const shadowHeatmap = (hours = 24) => get('/shadow-health/heatmap' + qs({ hours }));
export const shadowTopDiffs = (params = {}) =>
  get('/shadow-health/top-diffs' + qs({ hours: params.hours ?? 24, status: params.status, limit: params.limit }));
export const shadowAudits = (params = {}) =>
  get('/shadow-health/audits' + qs({
    platform: params.platform,
    address: params.address,
    metric: params.metric,
    status: params.status,
    hours: params.hours,
    limit: params.limit,
    offset: params.offset,
  }));
