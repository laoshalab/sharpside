// api/venue-hub.js · venue-hub 端点封装。对应 docs/FRONTEND_DESIGN.md §6.1/§6.2/§6.7。
import { get, post, qs } from './client.js';

/// GET /venue-hub/venues — 已接入 Venue 总览。
export const listVenues = () => get('/venue-hub/venues');

/// GET /venue-hub/traders?platform=&period=&category=&sort=&sort_desc=&q=&hot_only=&verified_only=&include_bots=&require_perf=&limit=&offset=&with_count=
/// 排行榜（join 绩效+标签）。对应 §6.2。category=OVERALL（默认）或某站内分类。
/// include_bots=false（默认）→ 排除被 botfilter 标记为机器人的交易者；true → 返回全部。
/// require_perf=true → 周期/分类参与 AND 共同筛选（剔除无该周期/分类绩效行的交易者）。
/// with_count=true → 响应 {rows, total}；否则纯数组（向后兼容 home.js / tg-bot / BFF）。
export const listTraders = (params = {}) =>
  get('/venue-hub/traders' + qs({
    platform: params.platform,
    period: params.period,
    category: params.category,
    sort: params.sort,
    sort_desc: params.sort_desc,
    q: params.q,
    hot_only: params.hot_only,
    verified_only: params.verified_only,
    include_bots: params.include_bots,
    require_perf: params.require_perf,
    limit: params.limit,
    offset: params.offset,
    with_count: params.with_count,
  }));

/// GET /venue-hub/traders/{platform}/{address} — 单个交易者详情。
export const getTrader = (platform, address) =>
  get(`/venue-hub/traders/${encodeURIComponent(platform)}/${encodeURIComponent(address)}`);

/// GET /venue-hub/traders/{platform}/{address}/performance — 绩效（全周期）+ 标签。
export const getPerformance = (platform, address) =>
  get(`/venue-hub/traders/${encodeURIComponent(platform)}/${encodeURIComponent(address)}/performance`);

/// GET /venue-hub/traders/{platform}/{address}/equity-curve?granularity=hour|day|auto
/// — 权益曲线（按粒度降采样）。`granularity` 默认 `auto`（近 30 天小时级 + 30 天前日级）。
export const getEquityCurve = (platform, address, params = {}) =>
  get(`/venue-hub/traders/${encodeURIComponent(platform)}/${encodeURIComponent(address)}/equity-curve` + qs({
    granularity: params.granularity,
  }));

/// GET /venue-hub/traders/sparklines?ids=p:a,p:b&period=1m
/// — 批量 equity 曲线（供排行榜 sparkline，消除 N+1）。
/// `ids`：`platform:address` 数组。响应 `{ "p:a": [{ts, equity}, ...], ... }`，服务端已按 period 截断 + 降采样到 ≤40 点。
export const getSparklines = (ids, period) =>
  get('/venue-hub/traders/sparklines' + qs({ ids: ids && ids.length ? ids.join(',') : undefined, period }));

/// GET /venue-hub/traders/{platform}/{address}/positions — 仓位时间线。
export const getPositions = (platform, address) =>
  get(`/venue-hub/traders/${encodeURIComponent(platform)}/${encodeURIComponent(address)}/positions`);

/// GET /venue-hub/traders/{platform}/{address}/trades?limit=&offset= — 近期成交。
export const getTrades = (platform, address, params = {}) =>
  get(`/venue-hub/traders/${encodeURIComponent(platform)}/${encodeURIComponent(address)}/trades` + qs({
    limit: params.limit,
    offset: params.offset,
  }));

/// POST /venue-hub/traders/import — 导入地址触发回填。
export const importTrader = (body) => post('/venue-hub/traders/import', body);

/// POST /venue-hub/traders/import/batch — 批量导入地址（逐条回填，最多 100 条/批）。
/// body: { items: [{ platform, address, alias?, x_username? }, ...] }
export const importTradersBatch = (body) => post('/venue-hub/traders/import/batch', body);

/// GET /venue-hub/identities — 已人工校对身份列表（manual_verified=true）。对应 §6.10。
export const listIdentities = () => get('/venue-hub/identities');

/// GET /venue-hub/identities/{id} — 跨平台身份详情。
export const getIdentity = (id) => get(`/venue-hub/identities/${encodeURIComponent(id)}`);

/// GET /venue-hub/markets?platform=&q= — 市场搜索（Phase 2）。
export const listMarkets = (params = {}) =>
  get('/venue-hub/markets' + qs({ platform: params.platform, q: params.q }));
