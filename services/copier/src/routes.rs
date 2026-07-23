//! 路由聚合。对应 `docs/ARCHITECTURE.md` §6.3 / `docs/FLOWS.md` §7。
//!
//! daemon 通道 B 端点（`daemon_api_key` 鉴权）：
//! - `GET /me/copy-orders?since=&channel=`：daemon 长轮询拉取待派发指令
//! - `POST /me/copy-orders/{id}/result`：daemon 上报成交 → 写 copy_execution + 更新状态

use crate::auth::{AuthUser, DaemonAuth};
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Path, Query};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sharpside_db::queries::account as acct;
use sharpside_db::queries::raw;
use sharpside_db::{CopyExecution, CopyOrderRow};
use sharpside_shared::Platform;
use sharpside_venues_core::Credential;
use uuid::Uuid;

/// Polygon 公共 RPC 兜底（copier 赎回 balanceOf 用；与 polymarket venue 的 onchain 默认一致）。
pub(crate) const POLYGON_RPC_DEFAULT_FALLBACK: &str = "https://polygon-bor.publicnode.com";
/// pUSD（collateral）合约地址。与 `wallet_batch::contracts::COLLATERAL` 一致。
pub(crate) const PUSD_CONST: &str = "0xC011a7E12a19f7B1f670d46F03B03f3342E82DFB";
/// Conditional Tokens 合约地址。与 `wallet_batch::contracts::CONDITIONAL_TOKENS` 一致。
pub(crate) const CTF_CONST: &str = "0x4D97DCd97eC945f40cF65F87097ACe5EA0476045";

/// 解析合约地址常量为 `alloy_primitives::Address`（编译期常量，启动时一次性校验）。
pub(crate) fn parse_address_const(s: &str) -> Result<alloy_primitives::Address, String> {
    s.parse().map_err(|e| format!("{s} 地址解析失败: {e}"))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(readyz))
        .route("/metrics", get(crate::metrics::metrics))
        // daemon 通道 B
        .route("/me/copy-orders", get(list_copy_orders))
        .route("/me/copy-orders/:id/result", post(report_result))
        // 用户态（JWT）
        .route("/me/copy-executions", get(list_my_executions))
        .route("/me/copy-orders/recent", get(list_my_recent_orders))
        .route("/me/portfolio", get(my_portfolio))
        // 钱包：充值（地址+余额）+ 提现 + 提现历史。对应 docs/FRONTEND_DESIGN.md §6.5。
        .route("/me/wallet", get(my_wallet))
        .route("/me/wallet/withdraw", post(withdraw))
        .route("/me/wallet/withdrawals", get(list_withdrawals))
        // 赎回：已结算市场赢仓位 CTF token → pUSD。对应 docs/CHANNEL_A_SIGNING.md §4.2。
        .route("/me/wallet/redeem", post(redeem))
        .route("/me/wallet/redeemable", get(list_redeemable))
        .route("/me/wallet/redemptions", get(list_redemptions))
}

async fn readyz(state: AppState) -> Result<Json<serde_json::Value>, ApiError> {
    sharpside_db::ping(&state.db).await?;
    Ok(Json(serde_json::json!({ "db": "ok" })))
}

#[derive(Debug, Deserialize)]
pub struct CopyOrderQuery {
    /// ISO8601 时间戳，返回 enqueued_at >= since 的指令；缺省=now-24h
    #[serde(default)]
    pub since: Option<DateTime<Utc>>,
    /// 通道过滤：tg / daemon；缺省=daemon
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct CopyOrderOut {
    #[serde(flatten)]
    pub order: CopyOrderRow,
}

async fn list_copy_orders(
    state: AppState,
    auth: DaemonAuth,
    Query(q): Query<CopyOrderQuery>,
) -> Result<Json<Vec<CopyOrderRow>>, ApiError> {
    let channel = q.channel.as_deref().unwrap_or("daemon");
    let since = q
        .since
        .unwrap_or_else(|| Utc::now() - chrono::Duration::hours(24));
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let rows = acct::list_copy_orders_since(&state.db, auth.user_id, channel, since, limit).await?;
    // M3 修复：通道 B（daemon）服务端 jurisdiction 兜底过滤。daemon 是用户本地进程，
    // 自身只做 local_max_notional 风控，不复查 jurisdiction。若用户创建跟随后改了 jurisdiction，
    // 旧 follow 派生的 copy_order 可能指向已不被允许的 execute_venue——此处过滤掉，
    // 与通道 A worker（exec.rs）的 jurisdiction 复查对齐（防御纵深）。被过滤的指令留在 pending，
    // 由用户在 UI 调整 execute_venue 或暂停跟随。
    let user = acct::get_user(&state.db, auth.user_id).await?;
    let allowed: std::collections::HashSet<Platform> =
        sharpside_shared::allowed_execute_venues(&user.jurisdiction)
            .into_iter()
            .collect();
    let filtered: Vec<CopyOrderRow> = rows
        .into_iter()
        .filter(|r| {
            r.execute_venue
                .parse::<Platform>()
                .map(|p| allowed.contains(&p))
                .unwrap_or(false)
        })
        .collect();
    Ok(Json(filtered))
}

#[derive(Debug, Deserialize)]
pub struct ResultBody {
    pub status: String, // filled / failed / skipped
    #[serde(default)]
    pub filled_size: Option<f64>,
    #[serde(default)]
    pub filled_price: Option<f64>,
    #[serde(default)]
    pub fee: Option<f64>,
    #[serde(default)]
    pub tx_hash: Option<String>,
    #[serde(default)]
    pub venue_order_id: Option<String>,
    #[serde(default)]
    pub skip_reason: Option<String>,
    /// daemon 回写映射后的执行市场/token（跨 Venue 跟单对账用；同 Venue 可不传，回退 source）。
    #[serde(default)]
    pub execute_market_id: Option<String>,
    #[serde(default)]
    pub execute_token_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ResultAck {
    pub id: Uuid,
    pub status: String,
}

async fn report_result(
    state: AppState,
    auth: DaemonAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<ResultBody>,
) -> Result<Json<ResultAck>, ApiError> {
    let order = acct::get_copy_order(&state.db, id).await?;
    if order.user_id != auth.user_id {
        return Err(ApiError::Unauthorized("无权上报他人指令".into()));
    }
    // 回写 daemon 上报的执行目标（若提供且 DB 仍为 NULL）。
    if body.execute_market_id.is_some() || body.execute_token_id.is_some() {
        acct::set_copy_order_exec_targets(
            &state.db,
            id,
            body.execute_market_id.as_deref(),
            body.execute_token_id.as_deref(),
        )
        .await?;
    }
    match body.status.as_str() {
        "filled" => {
            // 安全修复 1.4：CAS 抢占 pending/dispatched → filled。已终态 → None → 幂等返回 200，
            // 不重复 insert copy_execution（堵死 daemon 重复上报 / 多实例竞争的重复入账）。
            let claimed = acct::claim_copy_order_status(&state.db, id, "filled", None).await?;
            if claimed.is_none() {
                return Ok(Json(ResultAck { id, status: body.status }));
            }
            let filled_size = body
                .filled_size
                .unwrap_or_else(|| order.size.try_into().unwrap_or(0.0f64));
            let filled_price = body
                .filled_price
                .unwrap_or_else(|| order.price.try_into().unwrap_or(0.0f64));
            let fee = body.fee.unwrap_or(0.0);
            let exec_market_id = order
                .execute_market_id
                .clone()
                .unwrap_or_else(|| order.source_market_id.clone());
            let exec_token_id = order
                .execute_token_id
                .clone()
                .unwrap_or_else(|| order.source_token_id.clone());
            // CAS 已置 filled → 记成交（ON CONFLICT DO NOTHING 兜底跨通道竞争）。
            // 若成交行写入真 DB 故障（非冲突），回退 failed 交人工核对，避免 filled 但无 copy_execution 的账实不符。
            if let Err(e) = acct::insert_copy_execution(
                &state.db,
                order.id,
                order.user_id,
                order.execute_venue.as_str(),
                &exec_market_id,
                &exec_token_id,
                body.venue_order_id.as_deref(),
                order.side.as_str(),
                filled_size,
                filled_price,
                fee,
                body.tx_hash.as_deref(),
            )
            .await
            {
                acct::update_copy_order_status(
                    &state.db,
                    id,
                    "failed",
                    Some(&format!("insert_copy_execution 失败: {e}")),
                )
                .await
                .ok();
                return Err(e.into());
            }
        }
        "failed" | "skipped" => {
            // CAS 抢占：已终态 → 幂等返回 200（不重复改状态 / 不回退）。
            let _ = acct::claim_copy_order_status(
                &state.db,
                id,
                &body.status,
                body.skip_reason.as_deref(),
            )
            .await?;
        }
        other => return Err(ApiError::BadRequest(format!("未知 status: {other}"))),
    }
    Ok(Json(ResultAck {
        id,
        status: body.status,
    }))
}

// ── 用户态端点（JWT 鉴权）──

/// `GET /me/copy-executions?since=&limit=&offset=&follow_id=&venue=&status=` — 用户成交历史。
/// 对应 `docs/FRONTEND_DESIGN.md` §6.3 / §6.6 / §6.11。
/// 过滤参数：since（ISO8601，executed_at >= since）/ follow_id / venue / status（copy_order.status）。
#[derive(Debug, Deserialize)]
pub struct ExecutionsQuery {
    #[serde(default = "default_exec_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    #[serde(default)]
    pub since: Option<DateTime<Utc>>,
    #[serde(default)]
    pub follow_id: Option<Uuid>,
    #[serde(default)]
    pub venue: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

fn default_exec_limit() -> i64 {
    50
}

async fn list_my_executions(
    state: AppState,
    auth: AuthUser,
    Query(q): Query<ExecutionsQuery>,
) -> Result<Json<Vec<acct::CopyExecutionOut>>, ApiError> {
    let limit = q.limit.clamp(1, 10000);
    let offset = q.offset.max(0);
    let rows = acct::list_copy_executions_filtered(
        &state.db,
        auth.user_id,
        q.since,
        q.follow_id,
        q.venue.as_deref(),
        q.status.as_deref(),
        limit,
        offset,
    )
    .await?;
    Ok(Json(rows))
}

/// `GET /me/copy-orders/recent?limit=` — 用户近期跟单指令（所有状态）。
///
/// 与 daemon 专用的 `GET /me/copy-orders`（只返 pending）不同：本端点返回所有状态
/// （filled/failed/skipped/dispatched/pending），含 `skip_reason`，供前端展示
/// 「近期跟单指令」及失败/跳过原因（余额不足/股数不够/滑点超限/Polymarket 拒单等）。
#[derive(Debug, Deserialize)]
pub struct RecentOrdersQuery {
    #[serde(default = "default_exec_limit")]
    pub limit: i64,
}

async fn list_my_recent_orders(
    state: AppState,
    auth: AuthUser,
    Query(q): Query<RecentOrdersQuery>,
) -> Result<Json<Vec<CopyOrderRow>>, ApiError> {
    let limit = q.limit.clamp(1, 500);
    let rows = acct::list_recent_copy_orders(&state.db, auth.user_id, limit).await?;
    Ok(Json(rows))
}

/// `GET /me/portfolio?period=` — 用户投资组合聚合。
/// 对应 `docs/FRONTEND_DESIGN.md` §6.3。FIFO 仓位重建 + P&L + 权益曲线 + per_follow/per_venue + 延迟统计。
#[derive(Debug, Deserialize)]
pub struct PortfolioQuery {
    #[serde(default = "default_period")]
    pub period: String,
}

fn default_period() -> String {
    "1m".into()
}

#[derive(Debug, Serialize)]
pub struct Portfolio {
    pub period: String,
    pub kpi: PortfolioKpi,
    pub equity_curve: Vec<EquityPoint>,
    pub per_follow: Vec<PerFollowPnl>,
    pub per_venue: Vec<PerVenuePnl>,
    pub latency: LatencyStats,
    pub recent_executions: Vec<CopyExecution>,
    /// 当前未平仓明细（FIFO 重建后剩余 open lots，按 venue/market/token 聚合）。
    /// 无 mark price，故只有成本口径；未实现 PnL 留 Phase 2 接行情源后补。
    pub positions: Vec<Position>,
    /// 钱包视图（EOA + Deposit Wallet + pUSD 可用余额）。None = 无 polymarket 凭证。
    /// 余额为实时拉取（CLOB /balance-allowance），失败/离线时 cash_balance=None 并附 note。
    pub wallet: Option<WalletView>,
}

/// 钱包与可用资金视图。对应 `docs/FRONTEND_DESIGN.md` §6.3（portfolio 补 EOA + 可用资金）。
///
/// 口径（对齐 §6.4 资产权/交易权双卡）：
/// - `deposit_wallet_address`：资产权（ERC-1967 proxy），pUSD 存放处，"剩余可用资金" 由此地址余额衡量。
/// - `owner_address`：交易权（平台 KMS 代签的 EOA），下单签名用，relayer gasless 模式下不存 gas。
/// - `cash_balance`：Deposit Wallet 的 pUSD collateral 余额（实时，CLOB /balance-allowance，
///   signature_type=3 映射 owner→deposit wallet）。**仅含可用现金，不含已锁仓的持仓市值**。
///   None = 未预配 / 离线预配 / 拉取失败（见 `balance_note`）。
#[derive(Debug, Serialize)]
pub struct WalletView {
    pub venue: String,
    pub owner_address: Option<String>,
    pub deposit_wallet_address: Option<String>,
    pub cash_balance: Option<f64>,
    /// 是否完成在线全流程（false = 离线模式，余额不可查）。
    pub provision_live: bool,
    /// 余额不可查时的降级原因（前端展示）。
    pub balance_note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Position {
    pub venue: String,
    pub market_id: String,
    pub token_id: String,
    /// 净持仓股数（剩余 open lots 的 size 之和）。
    pub size: f64,
    /// 加权平均成本（cost_basis / size）。
    pub avg_cost: f64,
    /// 成本基数 = sum(size_i * cost_i)，即持仓市值（成本口径）。
    pub cost_basis: f64,
    /// 最近一笔建仓时间（open lots 中最新 executed_at）。
    pub opened_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct PortfolioKpi {
    pub total_pnl: f64,
    pub total_roi: f64,
    pub open_market_value: f64,
    pub win_rate: f64,
    pub trade_count: i64,
    pub unrealized_pnl: f64,
}

#[derive(Debug, Serialize)]
pub struct EquityPoint {
    pub date: NaiveDate,
    pub equity: f64,
    pub daily_pnl: f64,
}

#[derive(Debug, Serialize)]
pub struct PerFollowPnl {
    pub follow_relation_id: String,
    pub pnl: f64,
    pub share: f64,
}

#[derive(Debug, Serialize)]
pub struct PerVenuePnl {
    pub venue: String,
    pub pnl: f64,
    pub share: f64,
}

#[derive(Debug, Serialize)]
pub struct LatencyStats {
    pub median_ms: f64,
    pub p95_ms: f64,
    pub block0_hit_rate: f64,
    pub block0_enabled: bool,
    pub buckets: [i64; 5],
}

async fn my_portfolio(
    state: AppState,
    auth: AuthUser,
    Query(q): Query<PortfolioQuery>,
) -> Result<Json<Portfolio>, ApiError> {
    let period = if matches!(q.period.as_str(), "1d" | "1w" | "1m" | "1y" | "ytd" | "all") {
        q.period.clone()
    } else {
        "1m".into()
    };
    let since = period_cutoff(&period);

    let rows = acct::list_copy_executions_with_signal(&state.db, auth.user_id).await?;
    // period 过滤
    let in_range: Vec<&acct::CopyExecutionWithSignal> = rows
        .iter()
        .filter(|r| since.map(|s| r.exec.executed_at >= s).unwrap_or(true))
        .collect();

    // FIFO 仓位重建（per (venue, market_id, token_id)）求 realized PnL。
    // 简化：BUY 累加成本与数量，SELL 按 FIFO 减仓并实现 PnL = (sell_price - avg_cost) * size。
    use std::collections::HashMap;
    #[derive(Default)]
    struct Lot {
        size: f64,
        cost: f64,
        opened_at: Option<DateTime<Utc>>,
    }
    let mut lots: HashMap<(String, String, String), Vec<Lot>> = HashMap::new();
    let mut realized: f64 = 0.0;
    let mut per_follow_pnl: HashMap<String, f64> = HashMap::new();
    let mut per_venue_pnl: HashMap<String, f64> = HashMap::new();
    let mut wins = 0i64;
    let mut closed_trades = 0i64;
    let mut latencies: Vec<f64> = Vec::new();

    for r in &in_range {
        let size = to_f64(r.exec.filled_size);
        let price = to_f64(r.exec.filled_price);
        let key = (
            r.exec.venue.clone(),
            r.exec.market_id.clone(),
            r.exec.token_id.clone(),
        );
        if r.exec.side == "BUY" {
            lots.entry(key.clone()).or_default().push(Lot {
                size,
                cost: price,
                opened_at: Some(r.exec.executed_at),
            });
        } else {
            // SELL：FIFO 消减
            let mut remain = size;
            let queue = lots.entry(key.clone()).or_default();
            let mut avg_cost = 0.0;
            while remain > 0.0 && !queue.is_empty() {
                let lot = queue.first_mut().unwrap();
                let take = remain.min(lot.size);
                avg_cost += lot.cost * take;
                lot.size -= take;
                remain -= take;
                if lot.size <= 0.0 {
                    queue.remove(0);
                }
            }
            let cost_basis = avg_cost; // = sum(cost*take)
            let pnl = (price * size) - cost_basis - to_f64(r.exec.fee);
            realized += pnl;
            closed_trades += 1;
            if pnl > 0.0 {
                wins += 1;
            }
            if let Some(fid) = r.follow_relation_id {
                *per_follow_pnl.entry(fid.to_string()).or_insert(0.0) += pnl;
            }
            *per_venue_pnl.entry(r.exec.venue.clone()).or_insert(0.0) += pnl;
        }
        if let (Some(sig), exec) = (r.signal_at, r.exec.executed_at) {
            let ms = (exec - sig).num_milliseconds().max(0) as f64;
            latencies.push(ms);
        }
    }

    // 未实现 PnL：剩余 open lots 按最后成本估算（无 mark price，用 0 占位，诚实口径）。
    let open_cost: f64 = lots.values().flatten().map(|l| l.size * l.cost).sum();
    let unrealized = 0.0; // 无 mark price；前端显 "需行情源（Phase 2）"

    // 持仓明细：按 (venue, market_id, token_id) 聚合剩余 open lots。
    let mut positions: Vec<Position> = lots
        .iter()
        .filter_map(|((venue, market_id, token_id), queue)| {
            let size: f64 = queue.iter().map(|l| l.size).sum();
            if size.abs() < 1e-9 {
                return None;
            }
            let cost_basis: f64 = queue.iter().map(|l| l.size * l.cost).sum();
            let avg_cost = if size > 0.0 { cost_basis / size } else { 0.0 };
            let opened_at = queue.iter().filter_map(|l| l.opened_at).max();
            Some(Position {
                venue: venue.clone(),
                market_id: market_id.clone(),
                token_id: token_id.clone(),
                size,
                avg_cost,
                cost_basis,
                opened_at,
            })
        })
        .collect();
    positions.sort_by(|a, b| {
        b.cost_basis
            .partial_cmp(&a.cost_basis)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // 权益曲线：按 executed_at 日期累积 realized PnL（FIFO 二次遍历按日分桶）。
    let mut day_pnl: HashMap<NaiveDate, f64> = HashMap::new();
    {
        let mut lots2: HashMap<(String, String, String), Vec<Lot>> = HashMap::new();
        for r in &in_range {
            let size = to_f64(r.exec.filled_size);
            let price = to_f64(r.exec.filled_price);
            let key = (
                r.exec.venue.clone(),
                r.exec.market_id.clone(),
                r.exec.token_id.clone(),
            );
            if r.exec.side == "BUY" {
                lots2.entry(key).or_default().push(Lot {
                    size,
                    cost: price,
                    opened_at: Some(r.exec.executed_at),
                });
            } else {
                let mut remain = size;
                let queue = lots2.entry(key).or_default();
                let mut cost_basis = 0.0;
                while remain > 0.0 && !queue.is_empty() {
                    let lot = queue.first_mut().unwrap();
                    let take = remain.min(lot.size);
                    cost_basis += lot.cost * take;
                    lot.size -= take;
                    remain -= take;
                    if lot.size <= 0.0 {
                        queue.remove(0);
                    }
                }
                let pnl = (price * size) - cost_basis - to_f64(r.exec.fee);
                *day_pnl
                    .entry(r.exec.executed_at.date_naive())
                    .or_insert(0.0) += pnl;
            }
        }
    }
    let mut days: Vec<NaiveDate> = day_pnl.keys().copied().collect();
    days.sort();
    let mut equity_curve = Vec::new();
    let mut cum = 0.0_f64;
    for d in &days {
        let dp = day_pnl[d];
        cum += dp;
        equity_curve.push(EquityPoint {
            date: *d,
            equity: cum,
            daily_pnl: dp,
        });
    }

    // 延迟统计
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let (median_ms, p95_ms) = if latencies.is_empty() {
        (0.0, 0.0)
    } else {
        let pick = |p: f64| -> f64 {
            let idx = ((latencies.len() as f64) * p).floor() as usize;
            latencies[idx.min(latencies.len() - 1)]
        };
        (pick(0.5), pick(0.95))
    };
    let mut buckets = [0i64; 5];
    for ms in &latencies {
        let b = if *ms < 1000.0 {
            0
        } else if *ms < 2000.0 {
            1
        } else if *ms < 3000.0 {
            2
        } else if *ms < 5000.0 {
            3
        } else {
            4
        };
        buckets[b] += 1;
    }

    let total_pnl = realized;
    let win_rate = if closed_trades > 0 {
        wins as f64 / closed_trades as f64
    } else {
        0.0
    };
    // ROI 分母 = SELL 笔的成交额（已实现部分的成本基数近似）。
    let sell_notional: f64 = in_range
        .iter()
        .filter(|r| r.exec.side == "SELL")
        .map(|r| to_f64(r.exec.filled_size) * to_f64(r.exec.filled_price))
        .sum();
    let total_roi = if sell_notional > 0.0 {
        total_pnl / sell_notional
    } else {
        0.0
    };

    let per_follow: Vec<PerFollowPnl> = {
        let total: f64 = per_follow_pnl.values().sum();
        let mut v: Vec<PerFollowPnl> = per_follow_pnl
            .iter()
            .map(|(k, pnl)| PerFollowPnl {
                follow_relation_id: k.clone(),
                pnl: *pnl,
                share: if total.abs() > 0.0 { pnl / total } else { 0.0 },
            })
            .collect();
        v.sort_by(|a, b| {
            b.pnl
                .partial_cmp(&a.pnl)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        v
    };
    let per_venue: Vec<PerVenuePnl> = {
        let total: f64 = per_venue_pnl.values().sum();
        let mut v: Vec<PerVenuePnl> = per_venue_pnl
            .iter()
            .map(|(k, pnl)| PerVenuePnl {
                venue: k.clone(),
                pnl: *pnl,
                share: if total.abs() > 0.0 { pnl / total } else { 0.0 },
            })
            .collect();
        v.sort_by(|a, b| {
            b.pnl
                .partial_cmp(&a.pnl)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        v
    };

    let recent: Vec<CopyExecution> =
        acct::list_copy_executions_for_user(&state.db, auth.user_id, 20, 0).await?;

    let wallet = build_wallet_view(&state, auth.user_id).await;

    Ok(Json(Portfolio {
        period,
        kpi: PortfolioKpi {
            total_pnl,
            total_roi,
            open_market_value: open_cost,
            win_rate,
            trade_count: closed_trades,
            unrealized_pnl: unrealized,
        },
        equity_curve,
        per_follow,
        per_venue,
        latency: LatencyStats {
            median_ms,
            p95_ms,
            block0_hit_rate: 0.0,
            block0_enabled: false,
            buckets,
        },
        recent_executions: recent,
        positions,
        wallet,
    }))
}

/// 构建 portfolio 的钱包视图：解析 polymarket 凭证 → 取 owner/deposit wallet 地址 →
/// 在线预配时实时拉取 pUSD 可用余额（CLOB /balance-allowance），离线/失败时降级。
///
/// 设计要点：
/// - **绝不让余额拉取失败拖垮整个 portfolio**：任何错误都降级为 `cash_balance=None` + note。
/// - **离线预配（provision_live=false）走链上兜底**：dev 凭证是占位，CLOB 必 404/超时，
///   但 `deposit_wallet_address` 已由 CREATE2 派生，可直接 Polygon RPC `eth_call` 读 pUSD
///   `balanceOf(deposit_wallet)` 展示链上持有量（标注「链上余额，非 CLOB 可用资金」）。
/// - **CLOB 失败/超时也走链上兜底**：在线但 CLOB 异常时同样回退链上读取，尽力展示余额。
/// - **超时保护**：CLOB 与链上兜底各包 5s 超时，防挂起阻塞页面。
/// - **口径**：cash = Deposit Wallet 的 pUSD collateral（可用现金），不含持仓锁仓部分。
///   链上兜底口径为 ERC-20 原始余额，note 标注区别。
async fn build_wallet_view(state: &AppState, user_id: Uuid) -> Option<WalletView> {
    let rows = acct::list_credentials(&state.db, user_id).await.ok()?;
    let row = rows.into_iter().find(|c| c.platform == "polymarket")?;
    let blob = &row.encrypted_blob;

    // 凭证反序列化取地址字段；失败仍可展示地址（从 blob 兜底）。
    let (owner_address, deposit_wallet_address, provision_live) =
        match serde_json::from_value::<Credential>(blob.clone()) {
            Ok(Credential::DepositWalletDelegated {
                owner_address,
                deposit_wallet_address,
                ..
            }) => {
                let live = blob
                    .get("provision_live")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                (Some(owner_address), Some(deposit_wallet_address), live)
            }
            _ => {
                // 旧 Wallet 凭证或反序列化失败：从 blob 兜底读地址。
                let owner = blob
                    .get("owner_address")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let dw = blob
                    .get("deposit_wallet_address")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let live = blob
                    .get("provision_live")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                (owner, dw, live)
            }
        };

    let venue = "polymarket".to_string();
    let venue_impl = state.registry.get(Platform::Polymarket);

    /// 链上余额兜底：Polygon RPC `eth_call` 读 pUSD `balanceOf(deposit_wallet)`。
    /// 离线预配或 CLOB 拉取失败时调用，返回 (cash_balance, note)。
    /// 成功 → Some(余额) + 链上兜底 note；失败 → None + 降级 note。
    async fn onchain_fallback(
        venue_impl: Option<&std::sync::Arc<dyn sharpside_venues_core::Venue>>,
        deposit_wallet_address: &Option<String>,
        offline: bool,
    ) -> (Option<f64>, Option<String>) {
        let Some(venue) = venue_impl else {
            return (None, Some("polymarket venue 未注册".into()));
        };
        let Some(dw) = deposit_wallet_address.as_deref() else {
            return (None, Some("无 deposit_wallet_address，余额不可查".into()));
        };
        match tokio::time::timeout(std::time::Duration::from_secs(5), venue.balance_onchain(dw))
            .await
        {
            Ok(Ok(cash)) => (
                Some(cash),
                Some("链上余额（RPC eth_call 兜底，非 CLOB 实时可用资金）".into()),
            ),
            Ok(Err(e)) => (
                None,
                Some(format!(
                    "链上余额兜底失败: {e}{}",
                    if offline {
                        "（离线预配，需在线预配后充值 pUSD）"
                    } else {
                        ""
                    }
                )),
            ),
            Err(_) => (None, Some("链上余额兜底超时（5s）".into())),
        }
    }

    // 离线预配：dev 凭证是占位，CLOB 必失败，直接走链上余额兜底（不发 CLOB 请求）。
    if !provision_live {
        let (cash, note) = onchain_fallback(venue_impl, &deposit_wallet_address, true).await;
        // 兜底也失败时回退到原「离线预配」文案，让用户知道需在线预配。
        let note = note.unwrap_or_else(|| "离线预配，余额不可查（需在线预配后充值 pUSD）".into());
        return Some(WalletView {
            venue,
            owner_address,
            deposit_wallet_address,
            cash_balance: cash,
            provision_live,
            balance_note: Some(note),
        });
    }

    // 在线预配：实时拉取 pUSD 余额。venue 未注册或拉取失败均降级（含链上兜底）。
    let Some(venue_impl) = venue_impl else {
        return Some(WalletView {
            venue,
            owner_address,
            deposit_wallet_address,
            cash_balance: None,
            provision_live,
            balance_note: Some("polymarket venue 未注册".into()),
        });
    };

    let cred = match serde_json::from_value::<Credential>(blob.clone()) {
        Ok(c) => c,
        Err(e) => {
            return Some(WalletView {
                venue,
                owner_address,
                deposit_wallet_address,
                cash_balance: None,
                provision_live,
                balance_note: Some(format!("凭证反序列化失败: {e}")),
            });
        }
    };

    // 5s 超时保护，防 CLOB 挂起阻塞 portfolio。
    let balance =
        tokio::time::timeout(std::time::Duration::from_secs(5), venue_impl.balance(&cred)).await;

    match balance {
        Ok(Ok(bal)) => Some(WalletView {
            venue,
            owner_address,
            deposit_wallet_address,
            cash_balance: Some(bal.cash),
            provision_live,
            balance_note: None,
        }),
        Ok(Err(e)) => {
            // CLOB 失败 → 链上兜底，note 标注 CLOB 失败原因。
            let (cash, note) =
                onchain_fallback(Some(venue_impl), &deposit_wallet_address, false).await;
            let note = note.unwrap_or_else(|| format!("余额拉取失败: {e}"));
            Some(WalletView {
                venue,
                owner_address,
                deposit_wallet_address,
                cash_balance: cash,
                provision_live,
                balance_note: Some(if cash.is_some() {
                    format!("{note}（CLOB 失败: {e}）")
                } else {
                    format!("余额拉取失败: {e}")
                }),
            })
        }
        Err(_) => {
            // CLOB 超时 → 链上兜底。
            let (cash, note) =
                onchain_fallback(Some(venue_impl), &deposit_wallet_address, false).await;
            Some(WalletView {
                venue,
                owner_address,
                deposit_wallet_address,
                cash_balance: cash,
                provision_live,
                balance_note: Some(if cash.is_some() {
                    note.unwrap_or_else(|| "链上余额（CLOB 超时兜底）".into())
                } else {
                    "余额拉取超时（5s）".into()
                }),
            })
        }
    }
}

fn to_f64(d: rust_decimal::Decimal) -> f64 {
    use rust_decimal::prelude::ToPrimitive;
    d.to_f64().unwrap_or(0.0)
}

fn period_cutoff(period: &str) -> Option<DateTime<Utc>> {
    let now = Utc::now();
    match period {
        "1d" => Some(now - chrono::Duration::days(1)),
        "1w" => Some(now - chrono::Duration::days(7)),
        "1m" => Some(now - chrono::Duration::days(30)),
        "1y" => Some(now - chrono::Duration::days(365)),
        "ytd" => {
            let year: i32 = now.format("%Y").to_string().parse().unwrap_or(1970);
            chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, year, 1, 1, 0, 0, 0)
                .single()
                .map(|dt| dt.with_timezone(&Utc))
        }
        _ => None, // all
    }
}

// ── 钱包：充值 + 提现 + 提现历史 ──
// 对应 docs/FRONTEND_DESIGN.md §6.5 与 docs/CHANNEL_A_SIGNING.md §4.1。
//
// 充值：本质是用户从外部钱包向 deposit wallet 地址转 pUSD（平台无法代发起），
//       故"充值按钮"= 展示地址 + 复制 + 实时余额 + 刷新。
// 提现：owner EOA（平台 KMS 代签）签 WALLET batch 调 pUSD.transfer(to, amount)，
//       relayer gasless 提交。高敏操作——目标地址限用户绑定钱包、单笔/日上限、二次确认。

/// `GET /me/wallet` — 钱包视图（地址 + 实时 pUSD 余额 + 预配状态）。充值页用。
async fn my_wallet(state: AppState, auth: AuthUser) -> Result<Json<WalletView>, ApiError> {
    let w = build_wallet_view(&state, auth.user_id)
        .await
        .ok_or_else(|| ApiError::NotFound("polymarket 凭证未预配".into()))?;
    Ok(Json(w))
}

/// `POST /me/wallet/withdraw` — 提现 pUSD 到用户绑定的钱包地址。
///
/// body: `{ to: "0x...", amount: 7.0 }`
///
/// 风控链路：
/// 1. 目标地址须为用户已绑定钱包（`account.user_wallets`）之一。
/// 2. 金额 ∈ [WITHDRAW_MIN_AMOUNT, WITHDRAW_MAX_AMOUNT]。
/// 3. 实时余额 ≥ 金额（CLOB /balance-allowance，5s 超时）。
/// 4. 当日累计提现 + 金额 ≤ WITHDRAW_DAILY_MAX。
/// 5. 落库审计（pending）→ venue.withdraw（owner 签 WALLET batch → relayer）→ 更新状态。
#[derive(Debug, Deserialize)]
pub struct WithdrawBody {
    /// 提现目标地址（0x hex，须为用户绑定钱包之一）。
    pub to: String,
    /// 提现金额（pUSD 人类单位，如 7.0）。
    pub amount: f64,
}

#[derive(Debug, Serialize)]
pub struct WithdrawResponse {
    pub id: Uuid,
    pub status: String,
    pub to: String,
    pub amount: f64,
    pub tx_hash: Option<String>,
    pub relayer_tx_id: Option<String>,
    /// 失败/降级原因（status != mined 时填充）。
    pub note: Option<String>,
}

async fn withdraw(
    state: AppState,
    auth: AuthUser,
    Json(body): Json<WithdrawBody>,
) -> Result<Json<WithdrawResponse>, ApiError> {
    // 1. 目标地址规范化 + 校验属于用户绑定钱包。
    let to = normalize_address(&body.to).map_err(ApiError::BadRequest)?;
    let wallets = acct::list_wallets(&state.db, auth.user_id).await?;
    if !wallets.iter().any(|w| w.address == to) {
        return Err(ApiError::BadRequest(
            "提现目标地址须为已绑定钱包之一（在 设置 → 钱包 绑定后再提现）".into(),
        ));
    }

    // 2. 金额上下限校验。
    let amount = body.amount;
    if amount <= 0.0 {
        return Err(ApiError::BadRequest("提现金额须大于 0".into()));
    }
    let cfg = &state.config;
    if cfg.withdraw_min_amount > 0.0 && amount < cfg.withdraw_min_amount {
        return Err(ApiError::BadRequest(format!(
            "提现金额低于单笔下限 {} pUSD",
            cfg.withdraw_min_amount
        )));
    }
    if cfg.withdraw_max_amount > 0.0 && amount > cfg.withdraw_max_amount {
        return Err(ApiError::BadRequest(format!(
            "提现金额超过单笔上限 {} pUSD",
            cfg.withdraw_max_amount
        )));
    }

    // 3. 加载 polymarket 凭证 + 实时余额校验。
    let creds = acct::list_credentials(&state.db, auth.user_id).await?;
    let cred_row = creds
        .into_iter()
        .find(|c| c.platform == "polymarket")
        .ok_or_else(|| ApiError::NotFound("polymarket 凭证未预配".into()))?;
    let blob = &cred_row.encrypted_blob;
    let provision_live = blob
        .get("provision_live")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !provision_live {
        return Err(ApiError::BadRequest(
            "离线预配，无法提现（需在线预配后充值 pUSD 再提现）".into(),
        ));
    }
    let cred: Credential = serde_json::from_value(blob.clone())
        .map_err(|e| ApiError::Internal(format!("凭证反序列化失败: {e}")))?;
    let venue_impl = state
        .registry
        .get(Platform::Polymarket)
        .ok_or_else(|| ApiError::Internal("polymarket venue 未注册".into()))?
        .clone();

    // 实时余额（5s 超时保护，复用 portfolio 的口径）。
    let balance =
        tokio::time::timeout(std::time::Duration::from_secs(5), venue_impl.balance(&cred)).await;
    let cash = match balance {
        Ok(Ok(bal)) => bal.cash,
        Ok(Err(e)) => {
            return Err(ApiError::Internal(format!(
                "余额拉取失败，无法校验提现金额: {e}"
            )));
        }
        Err(_) => {
            return Err(ApiError::Internal(
                "余额拉取超时（5s），无法校验提现金额".into(),
            ));
        }
    };
    if amount > cash {
        return Err(ApiError::BadRequest(format!(
            "提现金额 {amount} 超过可用余额 {cash} pUSD"
        )));
    }

    // 4. 日累计上限校验。
    if cfg.withdraw_daily_max > 0.0 {
        let today = acct::daily_withdrawal_total(&state.db, auth.user_id, "polymarket").await?;
        let used: f64 = today
            .and_then(|d| {
                use rust_decimal::prelude::ToPrimitive;
                d.to_f64()
            })
            .unwrap_or(0.0);
        if used + amount > cfg.withdraw_daily_max {
            return Err(ApiError::BadRequest(format!(
                "今日已提现 {used} pUSD，加本次 {amount} 超过日上限 {} pUSD",
                cfg.withdraw_daily_max
            )));
        }
    }

    // 5. 落库审计（pending）——先记录，再发起链上交易，确保任何失败都有据可查。
    let amount_dec = rust_decimal::Decimal::from_f64_retain(amount)
        .ok_or_else(|| ApiError::BadRequest("提现金额精度异常".into()))?;
    let pending = acct::insert_withdrawal(
        &state.db,
        auth.user_id,
        "polymarket",
        "pUSD",
        amount_dec,
        &to,
        None,
    )
    .await?;

    // 6. 发起提现：owner 签 WALLET batch → relayer 提交 → 轮询确认。
    let result = venue_impl.withdraw(&cred, &to, amount).await;
    match result {
        Ok(r) => {
            // 轮询成功（有 tx_hash）→ mined；轮询超时但仍提交了 → 保留 pending。
            let status = if r.tx_hash.is_some() {
                "mined"
            } else {
                "pending"
            };
            let row = acct::update_withdrawal_status(
                &state.db,
                pending.id,
                status,
                r.tx_hash.as_deref(),
                None,
            )
            .await?;
            Ok(Json(WithdrawResponse {
                id: row.id,
                status: row.status,
                to: row.to_address,
                amount,
                tx_hash: row.tx_hash,
                relayer_tx_id: row.relayer_tx_id.or(r.relayer_tx_id),
                note: row.note,
            }))
        }
        Err(e) => {
            // 链上失败/签名失败 → 标记 failed，保留原因。
            let note = format!("{e}");
            let row =
                acct::update_withdrawal_status(&state.db, pending.id, "failed", None, Some(&note))
                    .await?;
            Ok(Json(WithdrawResponse {
                id: row.id,
                status: row.status,
                to: row.to_address,
                amount,
                tx_hash: row.tx_hash,
                relayer_tx_id: row.relayer_tx_id,
                note: row.note,
            }))
        }
    }
}

/// `GET /me/wallet/withdrawals?limit=&offset=` — 提现历史（最近优先）。
#[derive(Debug, Deserialize)]
pub struct WithdrawalsQuery {
    #[serde(default = "default_withdrawals_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_withdrawals_limit() -> i64 {
    50
}

async fn list_withdrawals(
    state: AppState,
    auth: AuthUser,
    Query(q): Query<WithdrawalsQuery>,
) -> Result<Json<Vec<sharpside_db::Withdrawal>>, ApiError> {
    let limit = q.limit.clamp(1, 200);
    let offset = q.offset.max(0);
    let rows = acct::list_withdrawals(&state.db, auth.user_id, limit, offset).await?;
    Ok(Json(rows))
}

/// 校验地址格式（0x + 40 hex）并规范化为小写。
fn normalize_address(addr: &str) -> Result<String, String> {
    let a = addr.trim();
    if !a.starts_with("0x") || a.len() != 42 {
        return Err("address 须为 0x + 40 hex".into());
    }
    if !a[2..].chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("address 含非 hex 字符".into());
    }
    Ok(a.to_lowercase())
}

// ── 赎回：已结算市场赢仓位 CTF token → pUSD ──
// 对应 docs/CHANNEL_A_SIGNING.md §4.2 与 migration 0025。
//
// 与提现的区别：提现转出 pUSD（高敏，金额风控）；赎回把赢仓位换 pUSD（纯收益转入 deposit wallet，
// 无金额风控，但防重复）。链路同 owner 签 WALLET batch → relayer gasless，calldata 换成
// CTF.redeemPositions(pUSD, 0, conditionId, [1,2])。
//
// 仓位来源以链上 CTF balanceOf 为准（用户可能在 Polymarket 官网手动交易，copy_execution 不全）。

/// `POST /me/wallet/redeem` body：手动赎回单市场赢仓位。
#[derive(Debug, Deserialize)]
pub struct RedeemBody {
    /// 市场 conditionId（0x hex，bytes32）。
    pub condition_id: String,
}

/// `POST /me/wallet/redeem` 响应。
#[derive(Debug, Serialize)]
pub struct RedeemResponse {
    pub id: Uuid,
    pub status: String,
    pub condition_id: String,
    /// 赢方 outcome：YES / NO。
    pub outcome: String,
    /// 赎回的赢方 token 数量（人类单位）。
    pub amount: f64,
    pub tx_hash: Option<String>,
    pub relayer_tx_id: Option<String>,
    pub note: Option<String>,
}

/// `GET /me/wallet/redeemable` 单条：用户在某已结算市场的可赎回仓位。
#[derive(Debug, Serialize)]
pub struct RedeemableItem {
    pub condition_id: String,
    pub title: String,
    /// 赢方 outcome：YES / NO。
    pub outcome: String,
    /// 赢方 token 的 ERC-1155 id（十进制字符串，前端展示用）。
    pub token_id: String,
    /// 链上可赎回数量（人类单位，CTF token 1:1 pUSD）。
    pub amount: f64,
    /// 预计可得 pUSD（= amount，1:1）。
    pub estimated_pusd: f64,
    /// 是否已有 pending/mined 赎回（前端据此禁用按钮）。
    pub already_redeemed: bool,
}

/// `POST /me/wallet/redeem` — 手动赎回单市场赢仓位。
///
/// body: `{ condition_id: "0x..." }`
///
/// 链路：校验市场已结算 → 链上 balanceOf 确认有赢仓位 → 落库 redemptions(pending, manual)
/// → venue.redeem（owner 签 WALLET batch 调 CTF.redeemPositions）→ 更新状态。
async fn redeem(
    state: AppState,
    auth: AuthUser,
    Json(body): Json<RedeemBody>,
) -> Result<Json<RedeemResponse>, ApiError> {
    let condition_id = body.condition_id.trim().to_lowercase();
    let cond_hex = condition_id.trim_start_matches("0x");
    if cond_hex.len() != 64 || !cond_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError::BadRequest(
            "condition_id 须为 0x 前缀的 32 字节 hex（bytes32）".into(),
        ));
    }

    // 1. 加载 polymarket 凭证（需 DepositWalletDelegated + 在线预配）。
    let (cred, deposit_wallet_address) = load_polymarket_cred(&state, auth.user_id).await?;

    // 2. 查市场是否已结算 + 判定赢方。
    let market = raw::list_raw_markets(&state.db, "polymarket")
        .await?
        .into_iter()
        .find(|m| m.venue_market_id.eq_ignore_ascii_case(&condition_id))
        .ok_or_else(|| ApiError::NotFound("市场未在缓存中（condition_id 不匹配）".into()))?;
    if !market.closed {
        return Err(ApiError::BadRequest("市场尚未结算，无法赎回".into()));
    }
    let (outcome, index_set) = match (
        market
            .outcome_yes
            .and_then(|d| d.to_string().parse::<f64>().ok()),
        market
            .outcome_no
            .and_then(|d| d.to_string().parse::<f64>().ok()),
    ) {
        (Some(1.0), _) => ("YES", 2u64),
        (_, Some(1.0)) => ("NO", 1u64),
        _ => {
            return Err(ApiError::BadRequest(
                "市场已结算但赢方 outcome 不明确（outcome_yes/no 非 1.0）".into(),
            ));
        }
    };

    // 3. 防重复：已有 pending/mined 赎回则拒绝。
    if acct::redemption_exists_active(
        &state.db,
        auth.user_id,
        &condition_id,
        outcome,
        &deposit_wallet_address,
    )
    .await?
    {
        return Err(ApiError::BadRequest(
            "该市场赢仓位已有进行中/完成的赎回".into(),
        ));
    }

    // 4. 链上 balanceOf 确认有可赎回量。
    let venue_impl = state
        .registry
        .get(Platform::Polymarket)
        .ok_or_else(|| ApiError::Internal("polymarket venue 未注册".into()))?
        .clone();
    let rpc_url = std::env::var("POLYGON_RPC_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| crate::routes::POLYGON_RPC_DEFAULT_FALLBACK.to_string());
    let pusd = crate::routes::parse_address_const(crate::routes::PUSD_CONST)
        .map_err(ApiError::Internal)?;
    let ctf =
        crate::routes::parse_address_const(crate::routes::CTF_CONST).map_err(ApiError::Internal)?;
    let dw: alloy_primitives::Address = deposit_wallet_address
        .parse()
        .map_err(|e| ApiError::BadRequest(format!("deposit_wallet_address 解析失败: {e}")))?;
    let position_id =
        sharpside_venues_polymarket::onchain::ctf_position_id(pusd, &condition_id, index_set);
    let balance =
        sharpside_venues_polymarket::onchain::ctf_balance_of(&rpc_url, ctf, dw, position_id)
            .await
            .map_err(ApiError::Internal)?;
    if balance <= 0.0 {
        return Err(ApiError::BadRequest(
            "链上无可赎回仓位（balanceOf=0，可能已赎回或未持有赢方 token）".into(),
        ));
    }

    // 5. 落库审计（pending, manual）。
    let amount_dec = rust_decimal::Decimal::from_f64_retain(balance)
        .ok_or_else(|| ApiError::BadRequest("赎回数量精度异常".into()))?;
    let token_id_str = position_id.to_string();
    let pending = acct::insert_redemption(
        &state.db,
        auth.user_id,
        "polymarket",
        &condition_id,
        outcome,
        &token_id_str,
        amount_dec,
        "manual",
        &deposit_wallet_address,
    )
    .await
    .map_err(|e| match e {
        sharpside_db::DbError::Conflict(msg) => ApiError::BadRequest(msg),
        other => ApiError::Internal(other.to_string()),
    })?;

    // 6. 发起赎回：owner 签 WALLET batch → relayer 提交 → 轮询确认。
    let result = venue_impl.redeem(&cred, &condition_id, balance).await;
    let row = match result {
        Ok(r) => {
            let status = if r.tx_hash.is_some() {
                "mined"
            } else {
                "pending"
            };
            acct::update_redemption_status(
                &state.db,
                pending.id,
                status,
                r.tx_hash.as_deref(),
                None,
            )
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
        }
        Err(e) => {
            let note = format!("{e}");
            acct::update_redemption_status(&state.db, pending.id, "failed", None, Some(&note))
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?
        }
    };

    Ok(Json(RedeemResponse {
        id: row.id,
        status: row.status,
        condition_id: row.condition_id,
        outcome: row.outcome,
        amount: to_f64(row.amount),
        tx_hash: row.tx_hash,
        relayer_tx_id: row.relayer_tx_id,
        note: row.note,
    }))
}

/// `GET /me/wallet/redeemable` — 列出用户在已结算市场的可赎回仓位。
///
/// 链路：查 raw_markets closed=true → 对每个算赢方 → 链上 balanceOf > 0 → 返回。
/// balanceOf 查询逐个 RPC 调用（已结算市场数量有限，可接受）；失败的市场跳过（note 不阻塞列表）。
async fn list_redeemable(
    state: AppState,
    auth: AuthUser,
) -> Result<Json<Vec<RedeemableItem>>, ApiError> {
    // 加载凭证取 deposit wallet 地址（无凭证则空列表）。
    let deposit_wallet_address = match load_polymarket_cred(&state, auth.user_id).await {
        Ok((_, dw)) => dw,
        Err(_) => return Ok(Json(vec![])),
    };

    let markets = raw::list_resolved_markets(&state.db, "polymarket", None).await?;
    let rpc_url = std::env::var("POLYGON_RPC_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| crate::routes::POLYGON_RPC_DEFAULT_FALLBACK.to_string());
    let pusd = crate::routes::parse_address_const(crate::routes::PUSD_CONST)
        .map_err(ApiError::Internal)?;
    let ctf =
        crate::routes::parse_address_const(crate::routes::CTF_CONST).map_err(ApiError::Internal)?;
    let dw: alloy_primitives::Address = deposit_wallet_address
        .parse()
        .map_err(|e| ApiError::BadRequest(format!("deposit_wallet_address 解析失败: {e}")))?;

    let mut items = Vec::new();
    for m in markets {
        let (outcome, index_set) = match (
            m.outcome_yes
                .and_then(|d| d.to_string().parse::<f64>().ok()),
            m.outcome_no.and_then(|d| d.to_string().parse::<f64>().ok()),
        ) {
            (Some(1.0), _) => ("YES", 2u64),
            (_, Some(1.0)) => ("NO", 1u64),
            // 已结算但 outcome 不明确，跳过（不阻塞列表）。
            _ => continue,
        };
        let position_id = sharpside_venues_polymarket::onchain::ctf_position_id(
            pusd,
            &m.venue_market_id,
            index_set,
        );
        // 链上 balanceOf；失败跳过该市场（不阻塞列表）。
        let balance = match sharpside_venues_polymarket::onchain::ctf_balance_of(
            &rpc_url,
            ctf,
            dw,
            position_id,
        )
        .await
        {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    condition_id = %m.venue_market_id,
                    error = %e,
                    "可赎回列表：balanceOf 查询失败，跳过"
                );
                continue;
            }
        };
        if balance <= 0.0 {
            continue;
        }
        let already_redeemed = acct::redemption_exists_active(
            &state.db,
            auth.user_id,
            &m.venue_market_id,
            outcome,
            &deposit_wallet_address,
        )
        .await
        .unwrap_or(false);
        items.push(RedeemableItem {
            condition_id: m.venue_market_id,
            title: m.title,
            outcome: outcome.to_string(),
            token_id: position_id.to_string(),
            amount: balance,
            estimated_pusd: balance,
            already_redeemed,
        });
    }
    Ok(Json(items))
}

/// `GET /me/wallet/redemptions?limit=&offset=` — 赎回历史（最近优先）。
#[derive(Debug, Deserialize)]
pub struct RedemptionsQuery {
    #[serde(default = "default_redemptions_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_redemptions_limit() -> i64 {
    50
}

async fn list_redemptions(
    state: AppState,
    auth: AuthUser,
    Query(q): Query<RedemptionsQuery>,
) -> Result<Json<Vec<sharpside_db::Redemption>>, ApiError> {
    let limit = q.limit.clamp(1, 200);
    let offset = q.offset.max(0);
    let rows = acct::list_redemptions(&state.db, auth.user_id, limit, offset).await?;
    Ok(Json(rows))
}

/// 加载用户 polymarket DepositWalletDelegated 凭证 + deposit wallet 地址。
/// 复用提现路径的凭证加载逻辑。返回 (Credential, deposit_wallet_address)。
async fn load_polymarket_cred(
    state: &AppState,
    user_id: Uuid,
) -> Result<(Credential, String), ApiError> {
    let creds = acct::list_credentials(&state.db, user_id).await?;
    let cred_row = creds
        .into_iter()
        .find(|c| c.platform == "polymarket")
        .ok_or_else(|| ApiError::NotFound("polymarket 凭证未预配".into()))?;
    let blob = &cred_row.encrypted_blob;
    let provision_live = blob
        .get("provision_live")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !provision_live {
        return Err(ApiError::BadRequest(
            "离线预配，无法赎回（需在线预配后才能赎回链上仓位）".into(),
        ));
    }
    let cred: Credential = serde_json::from_value(blob.clone())
        .map_err(|e| ApiError::Internal(format!("凭证反序列化失败: {e}")))?;
    let deposit_wallet_address = match &cred {
        Credential::DepositWalletDelegated {
            deposit_wallet_address,
            ..
        } => deposit_wallet_address.clone(),
        _ => {
            return Err(ApiError::BadRequest(
                "赎回仅支持 DepositWalletDelegated 凭证（旧 Wallet 凭证无 deposit wallet）".into(),
            ));
        }
    };
    Ok((cred, deposit_wallet_address))
}
