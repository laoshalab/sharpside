//! `GET /traders` / `GET /traders/{platform}/{address}` / `POST /traders/import`。
//! 对应 `docs/ARCHITECTURE.md` §6.1 与 `docs/FLOWS.md` §1（导入地址触发回填）。

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, Query};
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sharpside_db::queries::perf as perf_q;
use sharpside_db::queries::raw;
use sharpside_db::queries::traders as trader_q;
use sharpside_venues_core::LeaderboardQuery;
use std::time::Duration;

/// 导入后立即拍一条 `/value` 快照，缩短非榜地址 official_pnl（value_delta）冷启动。
async fn snap_portfolio_value(state: &AppState, platform: &str, address: &str) {
    let Ok(platform_enum) = platform.parse::<sharpside_shared::Platform>() else {
        return;
    };
    let Some(venue) = state.registry.get(platform_enum) else {
        return;
    };
    match venue.portfolio_value(address).await {
        Ok(v) => {
            if let Err(e) =
                perf_q::insert_value_snapshot(&state.db, platform, address, Utc::now(), v).await
            {
                tracing::warn!(
                    platform = %platform,
                    address = %address,
                    error = %e,
                    "import 写 /value 快照失败"
                );
            }
        }
        Err(sharpside_venues_core::VenueError::Unsupported(_)) => {}
        Err(e) => {
            tracing::warn!(
                platform = %platform,
                address = %address,
                error = %e,
                "import 拉 /value 失败"
            );
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub platform: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    /// 绩效周期：1d / 1w / 1m / 1y / ytd / all（默认 1m）。对应 `docs/FRONTEND_DESIGN.md` §6.2。
    #[serde(default = "default_period")]
    pub period: String,
    /// 站内分类：OVERALL（默认，全部成交）或某分类（perf worker 按 raw_markets.category 切片）。
    /// 空字符串视为 OVERALL。
    #[serde(default = "default_category")]
    pub category: String,
    /// 排序字段：roi / sharpe / win_rate / max_drawdown / realized_pnl / total_volume / updated_at（默认 roi）。
    #[serde(default = "default_sort")]
    pub sort: String,
    /// 排序方向（默认按字段：max_drawdown ASC，其余 DESC）。
    #[serde(default)]
    pub sort_desc: Option<bool>,
    /// 搜索：地址 / alias / @x 模糊。
    #[serde(default)]
    pub q: Option<String>,
    /// 仅热钥。
    #[serde(default)]
    pub hot_only: Option<bool>,
    /// 仅已验证。
    #[serde(default)]
    pub verified_only: Option<bool>,
    /// 是否包含被 botfilter 标记为机器人的交易者。默认 false（排除机器人）。
    /// 传 true 时返回全部（含 bot）。对应 `crates/botfilter` 产出的 `bot` 标签。
    #[serde(default)]
    pub include_bots: Option<bool>,
    /// 是否要求交易者**必须存在** `period`/`category` 对应的绩效行（多条件共同筛选）。
    ///
    /// 默认 false（向后兼容）→ 周期/分类仅决定展示哪行绩效，不缩小交易者范围。
    /// true → 没有该周期/分类绩效行的交易者被剔除，周期/分类真正参与 AND 共同筛选。
    /// 排行榜前端开启此开关以实现全维度组合过滤。
    #[serde(default)]
    pub require_perf: Option<bool>,
    /// 是否在响应里附带总数（`{rows, total}`）。默认 false → 返纯数组（向后兼容 tg-bot / home.js / BFF）。
    /// 排行榜前端传 true 以显示「显示 1-50 / 1,284」。
    #[serde(default)]
    pub with_count: Option<bool>,
}

fn default_period() -> String {
    "1m".into()
}
fn default_category() -> String {
    "OVERALL".into()
}
fn default_sort() -> String {
    "roi".into()
}

/// `list_traders` 响应。`#[serde(untagged)]`：
/// - `Rows`：纯数组（默认，向后兼容现有消费者）。
/// - `WithTotal`：`{rows, total}`（`with_count=true` 时，供排行榜分页总数）。
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum LeaderboardResponse {
    Rows(Vec<sharpside_db::LeaderboardRow>),
    WithTotal {
        rows: Vec<sharpside_db::LeaderboardRow>,
        total: i64,
    },
}

/// `GET /traders?platform=&period=&sort=&q=&hot_only=&verified_only=&limit=&offset=&with_count=`
/// — 列出可见交易者并 join 当前周期绩效 + 标签，返 `LeaderboardRow`。
///
/// 对应 `docs/FRONTEND_DESIGN.md` §6.2 与 `docs/ARCHITECTURE.md` §6.1。
pub async fn list_traders(
    state: AppState,
    Query(q): Query<ListQuery>,
) -> Result<Json<LeaderboardResponse>, ApiError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let offset = q.offset.unwrap_or(0).max(0);
    let period = if matches!(q.period.as_str(), "1d" | "1w" | "1m" | "1y" | "ytd" | "all") {
        q.period
    } else {
        "1m".into()
    };
    let category = if q.category.is_empty() {
        "OVERALL".to_string()
    } else {
        q.category
    };
    let query = trader_q::LeaderboardQuery {
        platform: q.platform.as_deref(),
        period: &period,
        category: &category,
        sort: &q.sort,
        sort_desc: q.sort_desc.unwrap_or(false),
        q: q.q.as_deref(),
        hot_only: q.hot_only.unwrap_or(false),
        verified_only: q.verified_only.unwrap_or(false),
        include_bots: q.include_bots.unwrap_or(false),
        require_perf: q.require_perf.unwrap_or(false),
        limit,
        offset,
    };
    let rows = trader_q::list_leaderboard(&state.db, query.clone()).await?;
    let resp = if q.with_count.unwrap_or(false) {
        let total = trader_q::count_leaderboard(&state.db, query).await?;
        LeaderboardResponse::WithTotal { rows, total }
    } else {
        LeaderboardResponse::Rows(rows)
    };
    Ok(Json(resp))
}

/// `GET /traders/sparklines?ids=polymarket:0xabc,polymarket:0xdef&period=1m`
/// — 批量取多个交易者的 equity 曲线（供排行榜 sparkline，消除 N+1）。
///
/// `ids`：逗号分隔的 `platform:address`（最多 100）。`period`：1d/1w/1m/1y/ytd/all（默认 1m）。
/// 响应：`{ "polymarket:0xabc": [{ts, equity}, ...], ... }`，每行已按 period 截断 + 降采样到 ≤40 点。
#[derive(Debug, Deserialize)]
pub struct SparklineQuery {
    pub ids: String,
    #[serde(default = "default_period")]
    pub period: String,
}

pub async fn list_sparklines(
    state: AppState,
    Query(q): Query<SparklineQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let period = if matches!(q.period.as_str(), "1d" | "1w" | "1m" | "1y" | "ytd" | "all") {
        q.period
    } else {
        "1m".into()
    };
    // 解析 ids → (platform, address) 对，最多 100。
    let ids: Vec<(String, String)> = q
        .ids
        .split(',')
        .filter_map(|s| {
            let s = s.trim();
            let i = s.find(':')?;
            Some((s[..i].to_string(), s[i + 1..].to_string()))
        })
        .take(100)
        .collect();
    if ids.is_empty() {
        return Ok(Json(serde_json::json!({})));
    }
    // period → since 截断点（None = 全历史）。
    let now = chrono::Utc::now();
    let since: Option<chrono::DateTime<chrono::Utc>> = match period.as_str() {
        "1d" => Some(now - chrono::Duration::days(1)),
        "1w" => Some(now - chrono::Duration::days(7)),
        "1m" => Some(now - chrono::Duration::days(30)),
        "1y" => Some(now - chrono::Duration::days(365)),
        "ytd" => ytd_start(now),
        _ => None,
    };
    // 短周期用小时级，长周期用日级（服务端降采样）。
    let granularity = if period == "1d" || period == "1w" {
        perf_q::EquityGranularity::Hour
    } else {
        perf_q::EquityGranularity::Day
    };
    let groups = perf_q::list_equity_curves_batch(&state.db, &ids, since, granularity).await?;
    // 组装 { "platform:address": [{ts, equity}, ...] }，每行降采样到 ≤40 点。
    let mut map = serde_json::Map::new();
    for (platform, address, pts) in groups {
        let sampled = downsample_pts(pts, 40);
        let arr: Vec<serde_json::Value> = sampled
            .iter()
            .map(|p| serde_json::json!({ "ts": p.ts, "equity": p.equity }))
            .collect();
        map.insert(
            format!("{platform}:{address}"),
            serde_json::Value::Array(arr),
        );
    }
    Ok(Json(serde_json::Value::Object(map)))
}

/// 年初至今起点（解析失败返回 None）。
fn ytd_start(now: chrono::DateTime<chrono::Utc>) -> Option<chrono::DateTime<chrono::Utc>> {
    let y = now.format("%Y").to_string().parse::<i32>().ok()?;
    chrono::NaiveDate::from_ymd_opt(y, 1, 1)?
        .and_hms_opt(0, 0, 0)?
        .and_utc()
        .into()
}

/// 等距降采样点到 ≤max（sparkline 用），保留首末点。
fn downsample_pts(
    pts: Vec<sharpside_db::EquityCurveBatchRow>,
    max: usize,
) -> Vec<sharpside_db::EquityCurveBatchRow> {
    let n = pts.len();
    if n <= max {
        return pts;
    }
    let stride = n as f64 / max as f64;
    let mut out = Vec::with_capacity(max + 1);
    for i in 0..max {
        out.push(pts[(i as f64 * stride) as usize].clone());
    }
    out.push(pts[n - 1].clone());
    out
}

/// `GET /traders/{platform}/{address}` — 单个交易者详情。
pub async fn get_trader(
    state: AppState,
    Path((platform, address)): Path<(String, String)>,
) -> Result<Json<sharpside_db::Trader>, ApiError> {
    let trader = trader_q::get_trader(&state.db, &platform, &address).await?;
    Ok(Json(trader))
}

/// `GET /traders/{platform}/{address}/performance` — 绩效（全周期）+ 标签 + tag_attrs（含 bot evidence）。
#[derive(Debug, Serialize)]
pub struct PerformanceOut {
    pub platform: String,
    pub address: String,
    pub performance: Vec<sharpside_db::TraderPerformance>,
    pub tags: Vec<String>,
    /// `trader_tag.tag_attrs` jsonb。结构 `{ "style": [...], "bot": BotFlags }`，
    /// 前端机器人检测面板读 `.bot` 下钻命中规则与 evidence。无标签行时为 `{}`。
    #[serde(default)]
    pub tag_attrs: serde_json::Value,
}

pub async fn get_performance(
    state: AppState,
    Path((platform, address)): Path<(String, String)>,
) -> Result<Json<PerformanceOut>, ApiError> {
    let performance = perf_q::list_trader_performance(&state.db, &platform, &address).await?;
    let tags = perf_q::get_trader_tag(&state.db, &platform, &address).await?;
    let tag_attrs = perf_q::get_trader_tag_attrs(&state.db, &platform, &address)
        .await?
        .unwrap_or_else(|| serde_json::json!({}));
    Ok(Json(PerformanceOut {
        platform,
        address,
        performance,
        tags,
        tag_attrs,
    }))
}

/// `POST /traders/import` 请求体。
#[derive(Debug, Deserialize)]
pub struct ImportBody {
    pub platform: String,
    pub address: String,
    /// 可选 alias / x_username，便于身份链接
    pub alias: Option<String>,
    pub x_username: Option<String>,
}

/// `POST /traders/import` 响应。
#[derive(Debug, Serialize)]
pub struct ImportResponse {
    pub trader: sharpside_db::Trader,
    pub trades_backfilled: usize,
}

/// 导入地址：upsert trader（source=imported）+ 从 Venue 回填 raw_trades。
///
/// 对应 `docs/FLOWS.md` §1。回填只拉该地址的成交写入 `raw_trades`，
/// 绩效由 perf worker 异步重算（不在此同步阻塞）。
pub async fn import_trader(
    state: AppState,
    auth: crate::auth::ImportCaller,
    Json(body): Json<ImportBody>,
) -> Result<Json<ImportResponse>, ApiError> {
    let platform = body.platform.clone();
    let address = body.address.clone();
    tracing::info!(
        caller = %auth.audit_label(),
        %platform,
        %address,
        "import trader"
    );

    let trader = trader_q::upsert_trader(
        &state.db,
        &platform,
        &address,
        "imported",
        body.alias.as_deref(),
        None,
        None,
        body.x_username.as_deref(),
        None,
    )
    .await?;

    // 回填 raw_trades：从已注册的 signal_source Venue 拉该地址成交。
    // 绩效由 perf worker 异步重算（不在此同步阻塞）。
    let mut trades_backfilled = 0usize;
    if let Ok(platform_enum) = platform.parse::<sharpside_shared::Platform>() {
        match crate::workers::backfill::backfill_trades_for(&state, platform_enum, &address).await {
            Ok(n) => trades_backfilled = n,
            Err(sharpside_venues_core::VenueError::Unsupported(_)) => {
                tracing::debug!(platform = %platform, "venue 不支持 trades，跳过回填");
            }
            Err(e) => {
                tracing::warn!(platform = %platform, address = %address, error = %e, "回填 trades 失败");
            }
        }
        // 导入路径同样标记已回填，避免 backfill worker 重复拉取。
        let _ = trader_q::mark_trades_backfilled(&state.db, &platform, &address).await;
        // 立即拍 /value，方便后续 official_pnl value_delta 兜底。
        snap_portfolio_value(&state, &platform, &address).await;
    }

    Ok(Json(ImportResponse {
        trader,
        trades_backfilled,
    }))
}

/// 单批最大条目数，避免单请求长时间占用 + Polymarket 限流放大。
const BATCH_IMPORT_MAX: usize = 100;
/// 批量导入地址间间隔（毫秒），与 backfill worker 一致缓解 Polymarket rate limit。
const BATCH_IMPORT_DELAY_MS: u64 = 200;

/// `POST /traders/import/batch` 请求体。`items` 复用 `ImportBody` 字段。
#[derive(Debug, Deserialize)]
pub struct BatchImportBody {
    pub items: Vec<ImportBody>,
}

/// 单条导入结果。
#[derive(Debug, Serialize)]
pub struct BatchImportItem {
    pub platform: String,
    pub address: String,
    pub ok: bool,
    pub trades_backfilled: usize,
    pub error: Option<String>,
}

/// `POST /traders/import/batch` 响应。
#[derive(Debug, Serialize)]
pub struct BatchImportResponse {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub total_trades_backfilled: usize,
    pub items: Vec<BatchImportItem>,
}

/// 批量导入地址：逐条 upsert + 回填，返回逐条结果。
///
/// 顺序执行（地址间 sleep），不并发，以缓解 Polymarket rate limit。
/// 单条失败不影响其余条目（per-item error 收集）。对应 `docs/FLOWS.md` §1。
pub async fn import_traders_batch(
    state: AppState,
    auth: crate::auth::ImportCaller,
    Json(body): Json<BatchImportBody>,
) -> Result<Json<BatchImportResponse>, ApiError> {
    tracing::info!(
        caller = %auth.audit_label(),
        count = body.items.len(),
        "import traders batch"
    );
    if body.items.is_empty() {
        return Err(ApiError::BadRequest("items 不能为空".into()));
    }
    if body.items.len() > BATCH_IMPORT_MAX {
        return Err(ApiError::BadRequest(format!(
            "单批最多 {} 条",
            BATCH_IMPORT_MAX
        )));
    }
    let total = body.items.len();
    let mut items: Vec<BatchImportItem> = Vec::with_capacity(total);
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut total_trades = 0usize;

    for (i, item) in body.items.into_iter().enumerate() {
        let platform = item.platform.clone();
        let address = item.address.clone();
        let mut trades_backfilled = 0usize;
        let mut err: Option<String> = None;

        match trader_q::upsert_trader(
            &state.db,
            &platform,
            &address,
            "imported",
            item.alias.as_deref(),
            None,
            None,
            item.x_username.as_deref(),
            None,
        )
        .await
        {
            Ok(_trader) => {
                if let Ok(platform_enum) = platform.parse::<sharpside_shared::Platform>() {
                    match crate::workers::backfill::backfill_trades_for(
                        &state,
                        platform_enum,
                        &address,
                    )
                    .await
                    {
                        Ok(n) => trades_backfilled = n,
                        Err(sharpside_venues_core::VenueError::Unsupported(_)) => {
                            tracing::debug!(platform = %platform, "venue 不支持 trades，跳过回填");
                        }
                        Err(e) => {
                            tracing::warn!(
                                platform = %platform, address = %address, error = %e,
                                "批量回填 trades 失败"
                            );
                            err = Some(e.to_string());
                        }
                    }
                    // 无论回填成功与否都标记，避免 backfill worker 重复拉取。
                    let _ = trader_q::mark_trades_backfilled(&state.db, &platform, &address).await;
                    snap_portfolio_value(&state, &platform, &address).await;
                }
            }
            Err(e) => {
                err = Some(e.to_string());
            }
        }

        let ok = err.is_none();
        if ok {
            succeeded += 1;
        } else {
            failed += 1;
        }
        total_trades += trades_backfilled;
        items.push(BatchImportItem {
            platform: platform.clone(),
            address: address.clone(),
            ok,
            trades_backfilled,
            error: err,
        });

        // 地址间限流（最后一条不 sleep）。
        if i + 1 < total {
            tokio::time::sleep(Duration::from_millis(BATCH_IMPORT_DELAY_MS)).await;
        }
    }

    tracing::info!(total, succeeded, failed, total_trades, "批量导入完成");
    Ok(Json(BatchImportResponse {
        total,
        succeeded,
        failed,
        total_trades_backfilled: total_trades,
        items,
    }))
}

/// 每页拉取条数。Polymarket Data API `limit` 过大易 404/解码失败，固定 50。
const LEADERBOARD_PAGE_SIZE: u32 = 50;

/// 单轮目标新增交易者数（已存在自动跳过，不计入）。
/// 可通过环境变量 `INGEST_LEADERBOARD_TARGET` 覆盖，默认 500。
fn leaderboard_new_target() -> usize {
    std::env::var("INGEST_LEADERBOARD_TARGET")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(500)
}

/// 供 ingest worker 复用：从 leaderboard 分页拉交易者，**仅插入不存在的**。
///
/// - 已存在 `(platform, address)` 自动过滤（`ON CONFLICT DO NOTHING`），不重复写入。
/// - `category` = None 时走 venue 默认（Polymarket=OVERALL）；Some 时按官方分类拉榜，
///   用于发现某分类下活跃的交易者，并**对榜上每位交易者**种子该 `period`×`category`
///   绩效行（否则前端点分类 + 严格匹配 → 0 人）。
/// - `period`：sharpside 周期键（`1d`/`1w`/`1m`/`1y`/`ytd`/`all`），映射到 Venue API。
/// - 分页推进直到新增满 `INGEST_LEADERBOARD_TARGET`（默认 500）或远端无更多结果。
/// - 返回值为本轮**新插入**条数（跳过的不算）。
pub(crate) async fn ingest_leaderboard(
    state: &AppState,
    platform: sharpside_shared::Platform,
    category: Option<&str>,
    period: &str,
) -> Result<usize, ApiError> {
    let venue = state.registry.get(platform).ok_or_else(|| {
        ApiError::Unsupported(format!("venue {} not registered", platform.as_str()))
    })?;

    let target = leaderboard_new_target();
    let mut inserted = 0usize;
    let mut skipped = 0usize;
    let mut category_seeded = 0usize;
    let mut offset: u32 = 0;

    while inserted < target {
        let q = LeaderboardQuery {
            category: category.map(|s| s.to_string()),
            time_period: period.into(),
            order_by: "pnl".into(),
            limit: LEADERBOARD_PAGE_SIZE,
            offset,
        };
        let traders = venue.leaderboard(q).await?;
        if traders.is_empty() {
            break;
        }
        let page_len = traders.len();
        for t in &traders {
            // 分类榜：无论新老交易者都写/刷新该 period×category 绩效种子，
            // 否则已存在地址全部 skipped 时分类筛选永远 0 人。
            if let Some(cat) = category {
                match perf_q::upsert_category_leaderboard_seed(
                    &state.db,
                    platform.as_str(),
                    &t.venue_trader_id,
                    period,
                    cat,
                    t.seed_pnl,
                    t.seed_vol,
                    "polymarket_leaderboard",
                )
                .await
                {
                    Ok(()) => category_seeded += 1,
                    Err(e) => tracing::warn!(
                        platform = platform.as_str(),
                        address = %t.venue_trader_id,
                        category = cat,
                        period,
                        error = %e,
                        "seed 分类绩效失败"
                    ),
                }
            }

            match trader_q::insert_trader_if_absent(
                &state.db,
                platform.as_str(),
                &t.venue_trader_id,
                "leaderboard",
                t.alias.as_deref(),
                None,
                t.profile_image.as_deref(),
                t.x_username.as_deref(),
                Some(t.verified),
            )
            .await
            {
                Ok(Some(_)) => {
                    // 临时展示层：首次入库时用 Polymarket 排行榜自带的 pnl/vol
                    // 填一行 trader_performance(period='all', OVERALL)，backfill + perf 跑完前先有数。
                    // ON CONFLICT DO NOTHING 永不覆盖已有真实绩效。
                    if let Err(e) = perf_q::seed_trader_performance(
                        &state.db,
                        platform.as_str(),
                        &t.venue_trader_id,
                        t.seed_pnl,
                        t.seed_vol,
                    )
                    .await
                    {
                        tracing::warn!(
                            platform = platform.as_str(),
                            address = %t.venue_trader_id,
                            error = %e,
                            "seed 临时绩效失败"
                        );
                    }
                    inserted += 1;
                    if inserted >= target {
                        break;
                    }
                }
                Ok(None) => skipped += 1,
                Err(e) => {
                    tracing::warn!(
                        platform = platform.as_str(),
                        address = %t.venue_trader_id,
                        error = %e,
                        "insert trader 失败"
                    );
                }
            }
        }
        offset = offset.saturating_add(LEADERBOARD_PAGE_SIZE);
        // 末页不足一页 → 远端已无更多
        if (page_len as u32) < LEADERBOARD_PAGE_SIZE {
            break;
        }
        // 分类路径：种子写满 target 即可停（不必为「找新地址」空扫 2500）。
        if category.is_some() && category_seeded >= target {
            break;
        }
        // 防护：极端情况下 offset 过大仍不停，最多扫 50 页（2500 条远端）
        if offset >= 2500 {
            break;
        }
    }

    tracing::info!(
        platform = platform.as_str(),
        category = category.unwrap_or("OVERALL"),
        period,
        inserted,
        skipped,
        category_seeded,
        target,
        "ingest leaderboard 去重完成"
    );
    Ok(inserted)
}

/// `GET /traders/{platform}/{address}/equity-curve?granularity=hour|day|auto`
/// — 权益曲线（按粒度降采样，前端按 period 截取）。对应 `docs/FRONTEND_DESIGN.md` §6.1。
///
/// `granularity`：
/// - `hour`：全历史小时级（长历史 trader 点数多）。
/// - `day`：全历史日级。
/// - `auto`（默认）：近 30 天小时级 + 30 天前日级，平衡平滑度与规模。
#[derive(Debug, Deserialize)]
pub struct EquityCurveQuery {
    #[serde(default = "default_granularity")]
    pub granularity: String,
}

fn default_granularity() -> String {
    "auto".into()
}

pub async fn get_equity_curve(
    state: AppState,
    Path((platform, address)): Path<(String, String)>,
    Query(q): Query<EquityCurveQuery>,
) -> Result<Json<Vec<sharpside_db::EquityCurvePoint>>, ApiError> {
    let granularity = perf_q::EquityGranularity::parse(&q.granularity);
    let rows = perf_q::list_equity_curve(&state.db, &platform, &address, granularity).await?;
    Ok(Json(rows))
}

/// `GET /traders/{platform}/{address}/positions` — 仓位时间线（含已平仓，前端过滤当前持仓）。
/// 对应 `docs/FRONTEND_DESIGN.md` §6.1。
///
/// 附带市场元数据（title / slug / outcome）：优先 Data API 实时持仓，回退 `raw_markets` 缓存。
#[derive(Debug, Serialize)]
pub struct PositionOut {
    #[serde(flatten)]
    pub row: sharpside_db::PositionRow,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}

pub async fn get_positions(
    state: AppState,
    Path((platform, address)): Path<(String, String)>,
) -> Result<Json<Vec<PositionOut>>, ApiError> {
    let rows = perf_q::list_position_timeline(&state.db, &platform, &address).await?;

    // raw_markets 缓存：condition_id → (title, slug)
    let condition_ids: Vec<String> = rows
        .iter()
        .filter_map(|r| r.condition_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let db_meta = raw::map_market_meta(&state.db, &platform, &condition_ids)
        .await
        .unwrap_or_default();

    // 实时持仓元数据：token_id → (title, slug, event_slug, outcome)
    let mut live_meta: std::collections::HashMap<
        String,
        (Option<String>, Option<String>, Option<String>, Option<String>),
    > = std::collections::HashMap::new();
    if let Ok(platform_enum) = platform.parse::<sharpside_shared::Platform>() {
        if let Some(venue) = state.registry.get(platform_enum) {
            match venue.positions(&address).await {
                Ok(live) => {
                    for p in live {
                        live_meta.insert(
                            p.token_id,
                            (p.title, p.slug, p.event_slug, p.outcome),
                        );
                    }
                }
                Err(sharpside_venues_core::VenueError::Unsupported(_)) => {}
                Err(e) => {
                    tracing::debug!(
                        platform = %platform,
                        address = %address,
                        error = %e,
                        "positions 实时元数据回源失败，回退 raw_markets"
                    );
                }
            }
        }
    }

    let out = rows
        .into_iter()
        .map(|row| {
            let live = live_meta.get(&row.token_id);
            let (db_title, db_slug) = row
                .condition_id
                .as_ref()
                .and_then(|id| db_meta.get(id))
                .cloned()
                .map(|(t, s)| (Some(t), s))
                .unwrap_or((None, None));
            PositionOut {
                market_title: live
                    .and_then(|(t, _, _, _)| t.clone())
                    .or(db_title),
                market_slug: live
                    .and_then(|(_, s, _, _)| s.clone())
                    .or(db_slug),
                event_slug: live.and_then(|(_, _, e, _)| e.clone()),
                outcome: live.and_then(|(_, _, _, o)| o.clone()),
                row,
            }
        })
        .collect();
    Ok(Json(out))
}

/// `GET /traders/{platform}/{address}/trades?limit=&offset=` — 近期原始成交。
/// 对应 `docs/FRONTEND_DESIGN.md` §6.1。复用 `list_raw_trades_for_trader`，按时间降序截取。
#[derive(Debug, Deserialize)]
pub struct TradesQuery {
    #[serde(default = "default_trades_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_trades_limit() -> i64 {
    20
}

pub async fn get_trades(
    state: AppState,
    Path((platform, address)): Path<(String, String)>,
    Query(q): Query<TradesQuery>,
) -> Result<Json<Vec<sharpside_db::RawTrade>>, ApiError> {
    let limit = q.limit.clamp(1, 500);
    let offset = q.offset.max(0);
    let mut rows = raw::list_raw_trades_for_trader(&state.db, &platform, &address).await?;
    // list_raw_trades_for_trader 返回升序；前端要"近期"→降序后截取。
    rows.reverse();
    let end = (offset + limit) as usize;
    let start = offset as usize;
    let sliced: Vec<_> = rows.into_iter().skip(start).take(end - start).collect();
    Ok(Json(sliced))
}
