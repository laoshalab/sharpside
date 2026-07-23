//! 通道 A 执行 worker（TG，平台代签）。对应 `docs/FLOWS.md` §6。
//!
//! 轮询 `account.copy_order WHERE channel='tg' AND status='pending'`，逐笔：
//! 管辖域过滤 → 跨 Venue 映射 → 单位换算 → 风控 → 下单（或 dry_run 合成）→ 写 copy_execution + 更新状态。

use crate::risk::{check_risk, RiskContext};
use crate::state::AppState;
use chrono::{Duration, Utc};
use rust_decimal::prelude::ToPrimitive;
use sharpside_db::queries::account as acct;
use sharpside_db::queries::mappings as map_q;
use sharpside_db::CopyOrderRow;
use sharpside_mapping::types::ExecParams;
use sharpside_mapping::unit::{convert_price, convert_size};
use sharpside_shared::{allowed_execute_venues, Channel, Platform, Side};
use sharpside_venues_core::{Credential, Order, VenueError};
use tracing::{error, info, warn};

const TG_BATCH: i64 = 50;

pub async fn run(state: AppState) {
    loop {
        if let Err(e) = tick(&state).await {
            error!(error = %e, "copier exec tick 失败");
        }
        tokio::time::sleep(std::time::Duration::from_secs(
            state.config.worker_exec_secs,
        ))
        .await;
    }
}

async fn tick(state: &AppState) -> Result<(), anyhow::Error> {
    let pending = acct::list_pending_copy_orders(&state.db, "tg", TG_BATCH).await?;
    if pending.is_empty() {
        return Ok(());
    }
    info!(n = pending.len(), "copier 处理 tg 待派发指令");
    for order in pending {
        if let Err(e) = process_one(state, &order).await {
            error!(order_id = %order.id, error = %e, "处理指令异常");
        }
    }
    Ok(())
}

async fn process_one(state: &AppState, order: &CopyOrderRow) -> Result<(), anyhow::Error> {
    let user = acct::get_user(&state.db, order.user_id).await?;
    // P0-3：DB 字段解析失败绝静默回退（side 误当 BUY = 真钱反向；price=0 = 0 价挂单）。
    // 任一字段非法即 skip + 告警，不继续。
    let (source_venue, execute_venue, side, price, size) = match parse_order_fields(order) {
        Ok(v) => v,
        Err(reason) => return skip(state, order.id, &reason).await,
    };

    // 1. 管辖域过滤
    let allowed = allowed_execute_venues(&user.jurisdiction);
    if !allowed.contains(&execute_venue) {
        return skip(
            state,
            order.id,
            &format!(
                "jurisdiction {} 不允许 execute_venue {}",
                user.jurisdiction, execute_venue
            ),
        )
        .await;
    }

    // 2. 跨 Venue 映射 + 单位换算
    let (exec_market_id, exec_token_id, exec_side, min_notional) = if source_venue == execute_venue
    {
        (
            order.source_market_id.clone(),
            order.source_token_id.clone(),
            side,
            1.0,
        )
    } else {
        let m = match map_q::resolve_mapping(
            &state.db,
            order.source_venue.as_str(),
            &order.source_market_id,
            order.execute_venue.as_str(),
        )
        .await
        {
            Ok(m) => m,
            Err(_) => {
                return skip(state, order.id, "无 verified 跨 Venue 映射").await;
            }
        };
        let exec_token = order.source_token_id.clone(); // token 翻译简化：同 id（Phase 1b 完善）
        let min = m.min_notional.and_then(|d| d.to_f64()).unwrap_or(1.0);
        let s = if m.direction_flip { side.flip() } else { side };
        (m.to_market_id, exec_token, s, min)
    };

    // 单位换算
    let (exec_price, exec_size) = convert_units(state, source_venue, execute_venue, price, size);

    // 3. 执行参数（min_size 默认 0；live 路径下从 Venue 元数据填充）
    let mut exec_params = exec_params_for(execute_venue, min_notional);

    // 3b. 非 dry_run：提前查 Venue，拉取市场最小下单股数（min_size）注入执行参数。
    //     Polymarket 每市场 minimum_order_size 不同（5/10/50/100…），服务端强制；
    //     下单前校验避免撞服务端 400。dry_run 无网络，min_size 保持 0（不校验）。
    let venue_opt = if !state.config.dry_run {
        let Some(venue) = state.registry.get(execute_venue) else {
            return skip(state, order.id, &format!("venue {execute_venue} 未注册")).await;
        };
        match venue.market_min_size(&exec_market_id).await {
            Ok(ms) if ms > 0.0 => {
                exec_params.min_size = ms;
            }
            Ok(_) => {}
            Err(e) => {
                warn!(order_id = %order.id, error = %e, "拉取 minimum_order_size 失败，min_size 保持 0");
            }
        }
        // 3c. 市场可交易性校验：已结算/下架的市场 active/accepting_orders=false → 早拒 skip，
        //     避免对已关闭市场下单（撞服务端 400 或挂死单）。
        //     venue 未实现（Unsupported）/拉取失败 → fail-closed skip + error 告警（M2 修复）：
        //     市场状态未知时不再放行（避免对已下架市场挂死单）。代价是 Venue API 瞬态故障会
        //     skip 该笔（置 skipped，不重试）；该 trader 下次仓位变化会重新派生新 copy_order，
        //     故非永久丢失仓位变化。market_min_size 仍 fail-open（仅 hint，place_order 兜底）。
        match venue.market_tradable(&exec_market_id).await {
            Ok(false) => {
                return skip(
                    state,
                    order.id,
                    "市场不可交易（active/accepting_orders=false，已结算或下架）",
                )
                .await;
            }
            Ok(true) => {}
            Err(e) => {
                error!(order_id = %order.id, error = %e, "market_tradable 拉取失败，fail-closed skip");
                return skip(
                    state,
                    order.id,
                    &format!("market_tradable 拉取失败，市场状态未知，fail-closed skip: {e}"),
                )
                .await;
            }
        }
        Some(venue)
    } else {
        None
    };

    // 4. 风控（三级覆盖：全局 × 档位 → 用户覆盖 → Venue 执行参数）
    let now = Utc::now();
    // 安全修复 1.5：风控查询 fail-closed —— 任一查询失败则 skip 不下单，
    // 绝不 `unwrap_or(0)`（那会绕过风控 = fail-open，真钱风险）。
    let risk_inputs = async {
        let daily_used =
            acct::sum_daily_notional(&state.db, order.user_id, now - Duration::hours(24)).await?;
        let open_positions = acct::count_active_copy_orders(&state.db, order.user_id).await?;
        let recent = acct::count_recent_copy_orders(
            &state.db,
            order.user_id,
            now - Duration::seconds(state.config.rapid_flip_window_secs),
        )
        .await?;
        let recent_statuses = acct::recent_copy_order_statuses(&state.db, order.user_id, 20).await?;
        Ok::<_, sharpside_db::DbError>((daily_used, open_positions, recent, recent_statuses))
    }
    .await;
    let (daily_used, open_positions, recent, recent_statuses) = match risk_inputs {
        Ok(v) => v,
        Err(e) => {
            return skip(
                state,
                order.id,
                &format!("风控查询失败（fail-closed 不下单）: {e}"),
            )
            .await
        }
    };
    let consecutive_failures = crate::risk::count_trailing_failures(&recent_statuses);

    let overrides: crate::risk::UserRiskOverrides =
        serde_json::from_value(user.risk_overrides.clone()).unwrap_or_default();
    let exec_limits = crate::risk::ExecLimits {
        min_notional: exec_params.min_notional,
        max_slippage_bps: exec_params.max_slippage_bps,
        min_size: exec_params.min_size,
    };
    let limits = crate::risk::effective_limits(
        &state.config,
        &user.subscription_tier,
        &overrides,
        &exec_limits,
    );

    let ctx = RiskContext {
        daily_notional_used: daily_used,
        open_positions,
        recent_orders_in_window: recent,
        consecutive_failures,
    };
    let notional = exec_price * exec_size;
    if let Err(reason) = check_risk(ctx, notional, exec_size, &limits) {
        return skip(state, order.id, &reason).await;
    }

    // 4b. per-follow 风控：FollowConfig.daily_max_notional / max_open_positions 独立约束
    //     该跟随关系的日累计与持仓数（UI 可填，此前未强制）。
    if let Some(follow_cfg) = load_follow_limits(state, order.follow_relation_id).await {
        // 安全修复 1.5：per-follow 风控查询同样 fail-closed。
        let f_inputs = async {
            let f_daily_used = acct::sum_daily_notional_for_follow(
                &state.db,
                order.follow_relation_id,
                now - Duration::hours(24),
            )
            .await?;
            let f_open =
                acct::count_active_copy_orders_for_follow(&state.db, order.follow_relation_id).await?;
            Ok::<_, sharpside_db::DbError>((f_daily_used, f_open))
        }
        .await;
        let (f_daily_used, f_open) = match f_inputs {
            Ok(v) => v,
            Err(e) => {
                return skip(
                    state,
                    order.id,
                    &format!("per-follow 风控查询失败（fail-closed 不下单）: {e}"),
                )
                .await
            }
        };
        let fctx = crate::risk::FollowRiskContext {
            daily_notional_used: f_daily_used,
            open_positions: f_open,
        };
        if let Err(reason) =
            crate::risk::check_follow_risk(fctx, notional, &follow_cfg)
        {
            return skip(state, order.id, &reason).await;
        }
    }

    // 5. 下单
    let _ = exec_side; // 已用于 Order
    if state.config.dry_run {
        let fill = sharpside_venues_core::Fill {
            order_id: format!("dry-run-{}", order.id),
            filled_size: exec_size,
            filled_price: exec_price,
            tx_hash: None,
            fee: exec_size * exec_price * exec_params.taker_fee_bps / 10_000.0,
            dry: true,
        };
        record_fill_with(state, order, &execute_venue, &exec_market_id, &exec_token_id, &fill)
            .await?;
        info!(order_id = %order.id, "dry-run 合成成交");
        return Ok(());
    }

    // 非 dry_run：venue 已在 3b 步查到（venue_opt）；凭证 + 滑点 + 余额校验后下单
    let Some(venue) = venue_opt else {
        return skip(state, order.id, "dry_run 关闭但 venue 未注册（不应发生）").await;
    };
    let cred = match load_credential(state, order.user_id, execute_venue, &order.channel).await {
        Ok(c) => c,
        Err(e) => {
            warn!(order_id = %order.id, error = %e, "无凭证，跳过");
            return skip(state, order.id, &format!("无 {execute_venue} 凭证: {e}")).await;
        }
    };

    // P0-1：滑点保护。book() 拉取失败时绝不下单（保守）——网络抖动/限流不应绕过滑点保护，
    // 否则真钱路径会以无保护价下单。失败即 skip。
    let book = match venue.book(&exec_market_id, &exec_token_id).await {
        Ok(b) => b,
        Err(e) => {
            return skip(
                state,
                order.id,
                &format!("盘口拉取失败，滑点保护无法校验: {e}"),
            )
            .await;
        }
    };
    let best_bid = book.bids.first().map(|l| l.price).unwrap_or(0.0);
    let best_ask = book.asks.first().map(|l| l.price).unwrap_or(0.0);

    // 第 2 层：激进定价 + FAK，让跟单真正"跟上"成交而非挂死单。
    // 信号价（exec_price）作为风控基准与钳制锚点；order_price 为实际下单价（见 aggressive_price）。
    // 滑点保护改为校验 order_price vs mid（实际付出价 vs 盘口中位）。
    let signal_price = exec_price;
    let order_price = match aggressive_price(
        exec_side,
        signal_price,
        best_bid,
        best_ask,
        limits.max_slippage_bps,
        state.config.aggressive_pricing,
    ) {
        Ok(p) => p,
        Err(reason) => return skip(state, order.id, &reason).await,
    };
    if let Err(reason) =
        crate::risk::check_slippage(order_price, best_bid, best_ask, limits.max_slippage_bps)
    {
        return skip(state, order.id, &reason).await;
    }

    // 最低余额校验：DW pUSD 余额 < min_dw_balance 则 skip（防充值不足下单被拒）。
    // RISK_MIN_DW_BALANCE 默认 50 USDC。balance() 拉取失败保守 skip（不下单）。
    if state.config.min_dw_balance > 0.0 {
        match venue.balance(&cred).await {
            Ok(bal) => {
                if bal.cash < state.config.min_dw_balance {
                    return skip(
                        state,
                        order.id,
                        &format!(
                            "DW 余额 {bal:.2} USDC 低于最低 {min:.0} USDC，请充值",
                            bal = bal.cash,
                            min = state.config.min_dw_balance
                        ),
                    )
                    .await;
                }
            }
            Err(e) => {
                warn!(order_id = %order.id, error = %e, "余额拉取失败，保守跳过");
                return skip(
                    state,
                    order.id,
                    &format!("DW 余额拉取失败，无法校验最低余额: {e}"),
                )
                .await;
            }
        }
    }

    // P0-2：place_order 前原子抢占 pending → dispatched。多 worker 并发时，风控通过后
    // 用 `UPDATE ... WHERE id=$1 AND status='pending' RETURNING *` CAS 抢占：只有一个
    // worker 能拿到行，其余拿到 None 即放弃（风控白做但绝不重复下单）。同时回写
    // execute_market_id/execute_token_id（映射后的真实执行目标，供 copy_execution 与赎回对账）。
    //
    // P1 订单级幂等键（Phase 2e/H5）：claim 时一次性生成并持久化 idempotency_salt（按
    // copy_order.id 确定性派生）+ order_timestamp_ms + 已换算 exec_price/exec_size。
    // place_order 复用 salt+timestamp → 重试发逐字节相同已签订单 → 相同 orderID → Polymarket
    // 判重而非重复下单。reclaim worker 据此可安全重试 place_order（见 reclaim_worker.rs）。
    let idempotency_salt = derive_idempotency_salt(order.id);
    let order_timestamp_ms = now_ms();
    let claimed = acct::claim_copy_order(
        &state.db,
        order.id,
        Some(&exec_market_id),
        Some(&exec_token_id),
        idempotency_salt as i64,
        order_timestamp_ms as i64,
        // 持久化实际下单价（激进定价后的 order_price），reclaim 重试复用同一价 → 相同 orderID 幂等。
        order_price,
        exec_size,
    )
    .await?;
    if claimed.is_none() {
        info!(order_id = %order.id, "已被其他 worker 抢占或已终态，放弃");
        return Ok(());
    }

    let order_req = Order {
        market_id: exec_market_id.clone(),
        token_id: exec_token_id.clone(),
        side: exec_side,
        price: order_price,
        size: exec_size,
        idempotency_salt: Some(idempotency_salt),
        order_timestamp_ms: Some(order_timestamp_ms),
        // 第 2 层：默认 FAK（立即成交能成交的部分，剩余取消），避免 GTC 挂单等行情、120s 超时撤单。
        // 可经 COPIER_ORDER_TYPE 回退 GTC/FOK/GTD。
        order_type: state.config.copy_order_type,
        expiration: None,
        post_only: false,
    };
    match venue.place_order(&cred, order_req).await {
        Ok(fill) => {
            // 安全修复 1.3：dry-sign（POLYMARKET_CLOB_POST≠1）仅签名未提交 CLOB，
            // 返回的 Fill 是合成假成交（dry=true）。**不得**置 submitted（否则 reconcile
            // 用假 order_id 永远查不到 → 订单卡 submitted + 日志刷屏），也不得记 copy_execution。
            // 改置 skipped（终态、诚实）：订单已签名但未提交，待启用 POLYMARKET_CLOB_POST=1 后重发。
            if fill.dry {
                info!(order_id = %order.id, "dry-sign 未提交 CLOB，置 skipped（设 POLYMARKET_CLOB_POST=1 提交）");
                return skip(
                    state,
                    order.id,
                    "dry-sign 未提交 CLOB（设 POLYMARKET_CLOB_POST=1 后重发）",
                )
                .await;
            }
            // P0 成交对账：place_order 返回 orderID 仅代表"订单被 Venue 接受"，非成交。
            // 限价单可能挂单未成交 / 部分成交。故置 submitted（不记成交），交 reconcile worker
            // 轮询 Venue::order_state 回写真实 filled_size/filled_price 后才置 filled。
            // fill.order_id 即 Venue 返回的订单 ID（live 为真实 CLOB orderID）。
            if let Err(e) =
                acct::mark_copy_order_submitted(&state.db, order.id, &fill.order_id).await
            {
                // 状态回写失败但订单已提交 Venue：置 failed 交人工核对（避免静默卡死）。
                error!(order_id = %order.id, error = %e, "mark_submitted 失败，订单已提交 Venue，置 failed 交人工核对");
                return fail(
                    state,
                    order.id,
                    &format!("订单已提交 Venue({}) 但 mark_submitted 失败: {e}", fill.order_id),
                )
                .await;
            }
            info!(order_id = %order.id, venue_order_id = %fill.order_id, "通道 A 已提交 Venue，置 submitted 待对账");
            // TODO(L1): 成交后异步通知 tg-bot（由 reconcile worker 在确认 filled 后触发更准确）。
        }
        Err(e) => {
            if matches!(e, VenueError::RateLimited) {
                crate::metrics::inc_clob_429();
            }
            fail(state, order.id, &format!("place_order 失败: {e}")).await?;
        }
    }
    Ok(())
}

async fn skip(state: &AppState, id: uuid::Uuid, reason: &str) -> Result<(), anyhow::Error> {
    acct::update_copy_order_status(&state.db, id, "skipped", Some(reason)).await?;
    info!(order_id = %id, reason, "跳过");
    Ok(())
}

/// 加载某跟随关系的 per-follow 风控限额（来自 FollowConfig）。
/// 跟随关系不存在或 config 解析失败时返回 None（跳过 per-follow 校验，仅走全局风控）。
async fn load_follow_limits(
    state: &AppState,
    follow_relation_id: uuid::Uuid,
) -> Option<crate::risk::FollowRiskLimits> {
    let rel = acct::get_follow(&state.db, follow_relation_id).await.ok()?;
    let cfg: sharpside_shared::FollowConfig = serde_json::from_value(rel.config).ok()?;
    Some(crate::risk::FollowRiskLimits {
        daily_max_notional: cfg.daily_max_notional,
        max_open_positions: cfg.max_open_positions as i64,
    })
}

async fn fail(state: &AppState, id: uuid::Uuid, reason: &str) -> Result<(), anyhow::Error> {
    acct::update_copy_order_status(&state.db, id, "failed", Some(reason)).await?;
    warn!(order_id = %id, reason, "失败");
    Ok(())
}

/// 显式传入执行目标的成交回写（channel A claim 后用，避免依赖 in-memory NULL）。
async fn record_fill_with(
    state: &AppState,
    order: &CopyOrderRow,
    venue: &Platform,
    exec_market_id: &str,
    exec_token_id: &str,
    fill: &sharpside_venues_core::Fill,
) -> Result<(), anyhow::Error> {
    acct::insert_copy_execution(
        &state.db,
        order.id,
        order.user_id,
        venue.as_str(),
        exec_market_id,
        exec_token_id,
        Some(fill.order_id.as_str()),
        order.side.as_str(),
        fill.filled_size,
        fill.filled_price,
        fill.fee,
        fill.tx_hash.as_deref(),
    )
    .await?;
    acct::update_copy_order_status(&state.db, order.id, "filled", None).await?;
    Ok(())
}

/// P0-3：纯函数解析 copy_order 的 source_venue/execute_venue/side/price/size。
/// 任一字段非法返回 `Err(reason)`（调用方 skip + 告警），绝不静默回退默认值——
/// side 误当 BUY = 真钱反向；price=0 = 0 价挂单；size<=0 = 空单。
/// 订单级幂等键：按 copy_order.id 确定性派生 Polymarket CLOB salt（≤2^53，JSON 整数安全）。
/// 取 UUID 前 8 字节 → u64 → 掩码低 53 位 → OR 1 保证非零。同一 copy_order 永远派生同一 salt，
/// 故 place_order / reclaim 重试复用此 salt + 持久化 timestamp → 发逐字节相同已签订单 → 相同 orderID。
fn derive_idempotency_salt(id: uuid::Uuid) -> u64 {
    let bytes = id.as_bytes();
    let raw = u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]);
    (raw & ((1u64 << 53) - 1)) | 1
}

/// 第 2 层：激进定价。返回实际下单价（`order_price`）。
///
/// - `aggressive=true`：买取 `best_ask`、卖取 `best_bid`（吃对手盘立即成交），再以信号价
///   ± `max_slippage_bps` 钳制，避免宽幅/快速行情中付出远超源钱包成交价。
/// - `aggressive=false`：返回 `signal_price`（FAK 仍 IOC，但不主动跨越盘口）。
///
/// 返回 `Err(reason)` 表示无法给出安全价（盘口为空 / 钳制后非正），调用方应 skip。
fn aggressive_price(
    side: Side,
    signal_price: f64,
    best_bid: f64,
    best_ask: f64,
    max_slippage_bps: f64,
    aggressive: bool,
) -> Result<f64, String> {
    if !aggressive {
        return Ok(signal_price);
    }
    let slip = max_slippage_bps / 10_000.0;
    let candidate = match side {
        Side::Buy => best_ask,
        Side::Sell => best_bid,
    };
    if candidate <= 0.0 {
        return Err("盘口无对手价，激进定价无法成交（盘口可能为空）".into());
    }
    let p = match side {
        // 买：付出不高于 信号价*(1+slip)；best_ask 更低则吃更低价。
        Side::Buy => candidate.min(signal_price * (1.0 + slip)),
        // 卖：卖出不低于 信号价*(1-slip)；best_bid 更高则吃更高价。
        Side::Sell => candidate.max(signal_price * (1.0 - slip)),
    };
    if p <= 0.0 {
        return Err(format!(
            "激进定价钳制后价 <= 0（candidate={candidate}, signal={signal_price}, slip={slip}）"
        ));
    }
    Ok(p)
}

/// 当前毫秒时间戳（签名用 timestamp）。
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn parse_order_fields(
    order: &CopyOrderRow,
) -> Result<(Platform, Platform, Side, f64, f64), String> {
    use rust_decimal::prelude::ToPrimitive;
    let source_venue: Platform = order
        .source_venue
        .parse()
        .map_err(|_| format!("source_venue 解析失败: {}", order.source_venue))?;
    let execute_venue: Platform = order
        .execute_venue
        .parse()
        .map_err(|_| format!("execute_venue 解析失败: {}", order.execute_venue))?;
    let side: Side = order
        .side
        .parse()
        .map_err(|_| format!("side 解析失败: {}", order.side))?;
    let price = order
        .price
        .to_f64()
        .ok_or_else(|| format!("price 解析失败: {}", order.price))?;
    let size = order
        .size
        .to_f64()
        .ok_or_else(|| format!("size 解析失败: {}", order.size))?;
    if price <= 0.0 || size <= 0.0 {
        return Err(format!("非法 price/size: price={price} size={size}"));
    }
    Ok((source_venue, execute_venue, side, price, size))
}

fn convert_units(
    state: &AppState,
    source: Platform,
    execute: Platform,
    price: f64,
    size: f64,
) -> (f64, f64) {
    use sharpside_venues_core::Unit;
    let from = state
        .registry
        .get(source)
        .map(|v| v.info().unit)
        .unwrap_or(Unit::UsdcCtf);
    let to = state
        .registry
        .get(execute)
        .map(|v| v.info().unit)
        .unwrap_or(Unit::UsdcCtf);
    let p = convert_price(from, to, price);
    let s = convert_size(from, to, size, price);
    (p, s)
}

fn exec_params_for(venue: Platform, min_notional_override: f64) -> ExecParams {
    let mut p = match venue {
        Platform::Kalshi => ExecParams::kalshi_default(),
        _ => ExecParams::polymarket_default(),
    };
    if min_notional_override > 0.0 {
        p.min_notional = min_notional_override;
    }
    p
}

/// 装载并「解密」用户 per-Venue 凭证。
///
/// 从 `account.user_venue_credentials` 读加密 blob → 反序列化为 [`Credential`]。
/// 密文（`encrypted_owner_key` / `encrypted_l2_secret`）原样回填，由 `PolymarketVenue`
/// 在注入的 KMS（`main.rs` 启动时 `with_kms`）内解密。dev 路径用 `DevKms`（明文透传）。
///
/// 通道 A（`channel=tg`，平台代签）要求 `DepositWalletDelegated` 凭证（POLY_1271 委托签名）；
/// 旧 `Wallet` 凭证（EOA 直签）仅兼容历史用户，对 `tg` 通道会被拒绝以避免误用未委托的 EOA。
pub(crate) async fn load_credential(
    state: &AppState,
    user_id: uuid::Uuid,
    venue: Platform,
    channel: &str,
) -> Result<Credential, anyhow::Error> {
    let rows = acct::list_credentials(&state.db, user_id).await?;
    let row = rows
        .into_iter()
        .find(|c| c.platform == venue.as_str())
        .ok_or_else(|| anyhow::anyhow!("无 {venue} 凭证"))?;
    // 安全修复 2.2：已撤销凭证拒下单（pull-based 停派发，无需显式通知）。
    if let Some(revoked_at) = row.revoked_at {
        return Err(anyhow::anyhow!(
            "{venue} 凭证已于 {revoked_at} 撤销（不可逆），copier 停止派发"
        ));
    }
    let cred: Credential = serde_json::from_value(row.encrypted_blob)
        .map_err(|e| anyhow::anyhow!("凭证反序列化失败: {e}"))?;
    // 通道 A（平台代签）必须用 DepositWalletDelegated；旧 Wallet 凭证拒绝。
    if channel == "tg" && !matches!(cred, Credential::DepositWalletDelegated { .. }) {
        return Err(anyhow::anyhow!(
            "通道 A(tg) 要求 DepositWalletDelegated 凭证（POLY_1271 委托签名），当前为 {:?}",
            std::mem::discriminant(&cred)
        ));
    }
    Ok(cred)
}

// 避免未使用警告（Channel 在 routes 也会用到，此处保留导入以供未来扩展）
const _: fn(Channel) -> () = |_| {};

#[cfg(test)]
mod tests {
    use super::*;
    use sharpside_db::CopyOrderRow;

    /// 安全修复 1.4 DB 级幂等：仅需 PG（无需 Polymarket 网络）。
    /// 验证 `claim_copy_order_status` CAS + `insert_copy_execution` ON CONFLICT：
    /// 同一 copy_order 重复上报 → 只一条 copy_execution，第二次 CAS 返回 None（幂等）。
    ///
    /// 跑法：
    /// ```bash
    /// DATABASE_URL='postgres://sharpside:sharpside_dev@127.0.0.1:5432/sharpside' \
    ///   cargo test -p sharpside-copier --bins cas_idempotent -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore]
    async fn cas_idempotent_duplicate_result_writes_one_execution() {
        let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://sharpside:sharpside_dev@127.0.0.1:5432/sharpside".to_string()
        });
        let db = sharpside_db::connect(&db_url, 5).await.expect("连 DB 失败");
        sharpside_db::migrate(&db).await.expect("迁移失败");

        let tg_id: i64 = 9_999_200_000 + (chrono::Utc::now().timestamp() % 1_000_000);
        let user = acct::upsert_tg_user(&db, tg_id).await.expect("建用户失败");
        let user_id = user.id;
        let follow = sqlx::query(
            r#"INSERT INTO account.follow_relation
               (user_id, follow_platform, follow_address, execute_venue, channel, config, same_venue_only, active)
               VALUES ($1,'polymarket',$2,'polymarket','tg','{}'::jsonb, false, true)
               RETURNING id"#,
        )
        .bind(user_id)
        .bind(format!("0xcas{tg_id}"))
        .fetch_one(&db)
        .await
        .expect("建 follow_relation 失败");
        let follow_id: uuid::Uuid = sqlx::Row::get(&follow, "id");

        let order_id = uuid::Uuid::new_v4();
        acct::enqueue_copy_order(
            &db, order_id, follow_id, user_id, "polymarket", "polymarket",
            "m", "t", "buy", 0.5, 10.0, "tg", chrono::Utc::now(), None, "pending", None,
        ).await.expect("插 copy_order 失败");

        // 第一次 CAS：pending → filled，返回 Some
        let first = acct::claim_copy_order_status(&db, order_id, "filled", None).await.unwrap();
        assert!(first.is_some(), "首次 CAS 应抢占成功");

        // 第一次 insert：写入成交行
        let r1 = acct::insert_copy_execution(
            &db, order_id, user_id, "polymarket", "m", "t", Some("0xorder1"),
            "buy", 10.0, 0.5, 0.0, None,
        ).await.unwrap();
        assert!(r1.is_some(), "首次 insert 应写入成交行");

        // 第二次 CAS：已 filled → 返回 None（幂等，不重复入账）
        let second = acct::claim_copy_order_status(&db, order_id, "filled", None).await.unwrap();
        assert!(second.is_none(), "重复 CAS 应返回 None（已终态，幂等）");

        // 第二次 insert：ON CONFLICT DO NOTHING → None（兜底，即便绕过 CAS 也只一条）
        let r2 = acct::insert_copy_execution(
            &db, order_id, user_id, "polymarket", "m", "t", Some("0xorder2"),
            "buy", 10.0, 0.5, 0.0, None,
        ).await.unwrap();
        assert!(r2.is_none(), "重复 insert 应 ON CONFLICT 返回 None");

        // 验收：copy_execution 恰好 1 行
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM account.copy_execution WHERE copy_order_id = $1",
        )
        .bind(order_id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(n, 1, "重复上报应只产生 1 条 copy_execution，实际 {n}");

        // 清理
        let _ = sqlx::query("DELETE FROM account.copy_order WHERE user_id = $1").bind(user_id).execute(&db).await;
        let _ = sqlx::query("DELETE FROM account.follow_relation WHERE user_id = $1").bind(user_id).execute(&db).await;
        let _ = sqlx::query("DELETE FROM account.users WHERE id = $1").bind(user_id).execute(&db).await;
        eprintln!("CAS_IDEMPOTENT ✅ 重复上报只一条 copy_execution");
    }

    /// 安全修复 1.5 DB 级：验证 `count_active_copy_orders` 净持仓语义。
    /// buy 10 tokenA → 1 开仓；再 sell 10 tokenA（往返）→ 0 开仓（不增）。
    /// 仅需 PG。
    #[tokio::test]
    #[ignore]
    async fn net_position_round_trip_does_not_accumulate() {
        let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://sharpside:sharpside_dev@127.0.0.1:5432/sharpside".to_string()
        });
        let db = sharpside_db::connect(&db_url, 5).await.expect("连 DB 失败");
        sharpside_db::migrate(&db).await.expect("迁移失败");

        let tg_id: i64 = 9_999_300_000 + (chrono::Utc::now().timestamp() % 1_000_000);
        let user = acct::upsert_tg_user(&db, tg_id).await.expect("建用户失败");
        let user_id = user.id;
        let follow = sqlx::query(
            r#"INSERT INTO account.follow_relation
               (user_id, follow_platform, follow_address, execute_venue, channel, config, same_venue_only, active)
               VALUES ($1,'polymarket',$2,'polymarket','tg','{}'::jsonb, false, true)
               RETURNING id"#,
        )
        .bind(user_id)
        .bind(format!("0xnet{tg_id}"))
        .fetch_one(&db)
        .await
        .expect("建 follow_relation 失败");
        let follow_id: uuid::Uuid = sqlx::Row::get(&follow, "id");

        // 1. buy 10 tokenA → filled + 成交行
        let buy_id = uuid::Uuid::new_v4();
        acct::enqueue_copy_order(&db, buy_id, follow_id, user_id, "polymarket", "polymarket", "m", "tokenA", "buy", 0.5, 10.0, "tg", chrono::Utc::now(), None, "pending", None).await.unwrap();
        acct::claim_copy_order_status(&db, buy_id, "filled", None).await.unwrap();
        acct::insert_copy_execution(&db, buy_id, user_id, "polymarket", "m", "tokenA", Some("0xbuy"), "buy", 10.0, 0.5, 0.0, None).await.unwrap();
        let open1 = acct::count_active_copy_orders(&db, user_id).await.unwrap();
        assert_eq!(open1, 1, "buy 10 tokenA 后应 1 开仓，实际 {open1}");

        // 2. sell 10 tokenA（往返）→ filled + 成交行
        let sell_id = uuid::Uuid::new_v4();
        acct::enqueue_copy_order(&db, sell_id, follow_id, user_id, "polymarket", "polymarket", "m", "tokenA", "sell", 0.5, 10.0, "tg", chrono::Utc::now(), None, "pending", None).await.unwrap();
        acct::claim_copy_order_status(&db, sell_id, "filled", None).await.unwrap();
        acct::insert_copy_execution(&db, sell_id, user_id, "polymarket", "m", "tokenA", Some("0xsell"), "sell", 10.0, 0.5, 0.0, None).await.unwrap();
        let open2 = acct::count_active_copy_orders(&db, user_id).await.unwrap();
        assert_eq!(open2, 0, "往返（buy+sell 同 token）后应 0 开仓（不增），实际 {open2}");

        // 3. per-follow 同口径
        let f_open = acct::count_active_copy_orders_for_follow(&db, follow_id).await.unwrap();
        assert_eq!(f_open, 0, "per-follow 往返后应 0 开仓，实际 {f_open}");

        // 清理
        let _ = sqlx::query("DELETE FROM account.copy_order WHERE user_id = $1").bind(user_id).execute(&db).await;
        let _ = sqlx::query("DELETE FROM account.follow_relation WHERE user_id = $1").bind(user_id).execute(&db).await;
        let _ = sqlx::query("DELETE FROM account.users WHERE id = $1").bind(user_id).execute(&db).await;
        eprintln!("NET_POSITION ✅ 往返后 open_positions 不增");
    }

    /// 安全修复 2.2：revoke 后 load_credential 拒下单；重新预配（upsert）重置撤销态。
    /// 仅需 PG。
    #[tokio::test]
    #[ignore]
    async fn revoke_blocks_load_credential_and_reprovision_reactivates() {
        let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://sharpside:sharpside_dev@127.0.0.1:5432/sharpside".to_string()
        });
        let db = sharpside_db::connect(&db_url, 5).await.expect("连 DB 失败");
        sharpside_db::migrate(&db).await.expect("迁移失败");

        let tg_id: i64 = 9_999_400_000 + (chrono::Utc::now().timestamp() % 1_000_000);
        let user = acct::upsert_tg_user(&db, tg_id).await.expect("建用户失败");
        let user_id = user.id;

        // 注入一条 polymarket 凭证（DevKms 风格明文 blob，load_credential 不解密即可反序列化）。
        let blob = serde_json::json!({
            "kind": "deposit_wallet_delegated",
            "deposit_wallet_address": "0x000000000000000000000000000000000000dEaD",
            "owner_address": "0x0000000000000000000000000000000000000bAd",
            "encrypted_owner_key": "x",
            "l2_api_key": "k",
            "encrypted_l2_secret": "s",
            "l2_passphrase": "p",
            "builder_code": "bc",
        });
        acct::upsert_credential_with_proxy(&db, user_id, "polymarket", &blob, Some("0xdead"))
            .await
            .expect("写凭证失败");

        // 1. revoke 前：load_credential 成功（构造一个最小 state 仅取 db）。
        //    直接验证 revoke_credential 行为 + 列状态，避免依赖完整 AppState。
        let row = acct::revoke_credential(&db, user_id, "polymarket")
            .await
            .expect("revoke 失败")
            .expect("应有凭证行");
        assert!(row.revoked_at.is_some(), "revoked_at 应已置位");
        assert_eq!(row.revoked_by, Some(user_id));

        // 2. 已撤销：再次 revoke 幂等返回 None（无活跃行可更新）。
        let again = acct::revoke_credential(&db, user_id, "polymarket").await.unwrap();
        assert!(again.is_none(), "已撤销凭证再次 revoke 应幂等返回 None");

        // 3. list_credentials 反映 revoked_at。
        let listed = acct::list_credentials(&db, user_id).await.unwrap();
        let poly = listed.iter().find(|c| c.platform == "polymarket").unwrap();
        assert!(poly.revoked_at.is_some(), "list_credentials 应见 revoked_at");

        // 4. 重新预配（upsert）应重置撤销态 → 新凭证活跃。
        acct::upsert_credential_with_proxy(&db, user_id, "polymarket", &blob, Some("0xdead"))
            .await
            .expect("重新预配失败");
        let listed2 = acct::list_credentials(&db, user_id).await.unwrap();
        let poly2 = listed2.iter().find(|c| c.platform == "polymarket").unwrap();
        assert!(poly2.revoked_at.is_none(), "重新预配后 revoked_at 应重置为 None");

        // 清理
        let _ = sqlx::query("DELETE FROM account.user_venue_credentials WHERE user_id = $1").bind(user_id).execute(&db).await;
        let _ = sqlx::query("DELETE FROM account.users WHERE id = $1").bind(user_id).execute(&db).await;
        eprintln!("REVOKE ✅ revoke 后凭证标记撤销，重新预配重置");
    }

    #[test]
    fn exec_params_override_min_notional() {
        let p = exec_params_for(Platform::Polymarket, 5.0);
        assert_eq!(p.min_notional, 5.0);
        let p2 = exec_params_for(Platform::Polymarket, 0.0);
        assert_eq!(p2.min_notional, 1.0); // polymarket_default
    }

    fn row(venue: &str, exec: &str, side: &str, price: &str, size: &str) -> CopyOrderRow {
        CopyOrderRow {
            id: uuid::Uuid::nil(),
            follow_relation_id: uuid::Uuid::nil(),
            user_id: uuid::Uuid::nil(),
            source_venue: venue.into(),
            execute_venue: exec.into(),
            source_market_id: "m".into(),
            source_token_id: "t".into(),
            execute_market_id: None,
            execute_token_id: None,
            side: side.into(),
            price: price.parse().unwrap_or_default(),
            size: size.parse().unwrap_or_default(),
            channel: "tg".into(),
            signal_at: chrono::Utc::now(),
            skip_reason: None,
            status: "pending".into(),
            enqueued_at: chrono::Utc::now(),
            dispatched_at: None,
            venue_order_id: None,
            submitted_at: None,
            idempotency_salt: None,
            order_timestamp_ms: None,
            exec_price: None,
            exec_size: None,
            signal_id: None,
        }
    }

    #[test]
    fn parse_ok_polymarket_buy() {
        let r = row("polymarket", "polymarket", "buy", "0.5", "10");
        let (sv, ev, s, p, sz) = parse_order_fields(&r).unwrap();
        assert_eq!(sv, Platform::Polymarket);
        assert_eq!(ev, Platform::Polymarket);
        assert_eq!(s, Side::Buy);
        assert!((p - 0.5).abs() < 1e-9);
        assert!((sz - 10.0).abs() < 1e-9);
    }

    #[test]
    fn parse_bad_side_errors_not_silent_buy() {
        // side="xyz" 绝不静默回退为 Buy（真钱反向风险）
        let r = row("polymarket", "polymarket", "xyz", "0.5", "10");
        let err = parse_order_fields(&r).unwrap_err();
        assert!(err.contains("side 解析失败"));
    }

    #[test]
    fn aggressive_buy_takes_best_ask_clamped_to_signal_plus_slip() {
        // 信号 0.50，best_ask 0.51，slip 200bps(2%) → cap 0.51；min(0.51, 0.51)=0.51
        let p = aggressive_price(Side::Buy, 0.50, 0.49, 0.51, 200.0, true).unwrap();
        assert!((p - 0.51).abs() < 1e-9, "got {p}");
    }

    #[test]
    fn aggressive_buy_clamps_when_ask_exceeds_signal_plus_slip() {
        // 信号 0.50，best_ask 0.60，slip 200bps → cap 0.51；min(0.60, 0.51)=0.51（不追高）
        let p = aggressive_price(Side::Buy, 0.50, 0.49, 0.60, 200.0, true).unwrap();
        assert!((p - 0.51).abs() < 1e-9, "got {p}");
    }

    #[test]
    fn aggressive_sell_takes_best_bid_clamped_to_signal_minus_slip() {
        // 信号 0.50，best_bid 0.49，slip 200bps → floor 0.49；max(0.49, 0.49)=0.49
        let p = aggressive_price(Side::Sell, 0.50, 0.49, 0.51, 200.0, true).unwrap();
        assert!((p - 0.49).abs() < 1e-9, "got {p}");
    }

    #[test]
    fn aggressive_sell_clamps_when_bid_below_signal_minus_slip() {
        // 信号 0.50，best_bid 0.40，slip 200bps → floor 0.49；max(0.40, 0.49)=0.49（不杀跌）
        let p = aggressive_price(Side::Sell, 0.50, 0.40, 0.51, 200.0, true).unwrap();
        assert!((p - 0.49).abs() < 1e-9, "got {p}");
    }

    #[test]
    fn aggressive_empty_book_errors() {
        // best_ask=0 → 盘口为空，无法激进定价
        let err = aggressive_price(Side::Buy, 0.50, 0.0, 0.0, 200.0, true).unwrap_err();
        assert!(err.contains("盘口无对手价"));
    }

    #[test]
    fn non_aggressive_returns_signal_price() {
        let p = aggressive_price(Side::Buy, 0.50, 0.49, 0.51, 200.0, false).unwrap();
        assert!((p - 0.50).abs() < 1e-9, "got {p}");
    }

    #[test]
    fn parse_bad_execute_venue_errors() {
        let r = row("polymarket", "not-a-venue", "sell", "0.5", "10");
        let err = parse_order_fields(&r).unwrap_err();
        assert!(err.contains("execute_venue 解析失败"));
    }

    #[test]
    fn parse_zero_price_errors() {
        // price=0 绝不下单（0 价挂单风险）
        let r = row("polymarket", "polymarket", "buy", "0", "10");
        let err = parse_order_fields(&r).unwrap_err();
        assert!(err.contains("非法 price/size"));
    }

    #[test]
    fn parse_zero_size_errors() {
        let r = row("polymarket", "polymarket", "buy", "0.5", "0");
        let err = parse_order_fields(&r).unwrap_err();
        assert!(err.contains("非法 price/size"));
    }

    #[test]
    fn parse_negative_price_errors() {
        let r = row("polymarket", "polymarket", "buy", "-0.5", "10");
        let err = parse_order_fields(&r).unwrap_err();
        assert!(err.contains("非法 price/size"));
    }

    /// 阶段2 · 真钱：copier worker 真打 Polymarket `/order`（`#[ignore]`，花真钱，需 full_network + 代理）。
    ///
    /// 用 .env.local 的 funded deposit wallet（已部署 + approved + 充 pUSD）注入凭证 → 插 pending tg 单 →
    /// 调 `process_one`（COPIER_DRY_RUN=false + POLYMARKET_CLOB_POST=1）→ 真打 `/order` 返回真实 orderID →
    /// 立即撤单（撤回锁定的 USDC）。挂单价 = best_bid（不立即成交），size 取最小过 min_notional=1.0。
    ///
    /// 跑法（需代理 + full_network + .env.local 已 source）：
    /// ```bash
    /// set -a; source .env.local; set +a
    /// DATABASE_URL='postgres://sharpside:sharpside_dev@127.0.0.1:5432/sharpside' \
    /// SHARPSIDE_KMS_DEV_PLAINTEXT=1 POLYMARKET_CLOB_POST=1 \
    ///   cargo test -p sharpside-copier --offline --test '*' stage2_real_order -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore]
    async fn stage2_real_order_places_and_cancels() {
        use sharpside_kms::Kms;
        // 前置 env
        let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://sharpside:sharpside_dev@127.0.0.1:5432/sharpside".to_string()
        });
        let owner_pk = std::env::var("POLYMARKET_TEST_OWNER_PK")
            .expect("需设 POLYMARKET_TEST_OWNER_PK（.env.local）");
        let owner_addr_str = std::env::var("POLYMARKET_TEST_OWNER_ADDRESS")
            .expect("需设 POLYMARKET_TEST_OWNER_ADDRESS");
        let dw_addr_str = std::env::var("POLYMARKET_TEST_DEPOSIT_WALLET")
            .expect("需设 POLYMARKET_TEST_DEPOSIT_WALLET");
        let builder_code = std::env::var("POLYMARKET_BUILDER_CODE")
            .unwrap_or_else(|_| "019f6e85-dce2-7a7a-aa72-cadb8d498bbe".into());
        std::env::set_var("POLYMARKET_CLOB_POST", "1");

        // 1. 连 DB + 迁移
        let db = sharpside_db::connect(&db_url, 5).await.expect("连 DB 失败");
        sharpside_db::migrate(&db).await.expect("迁移失败");

        // 2. 建测试用户 + follow_relation（copy_order.follow_relation_id NOT NULL FK）
        let tg_id: i64 = 9_999_000_000 + (chrono::Utc::now().timestamp() % 1_000_000);
        let user = acct::upsert_tg_user(&db, tg_id).await.expect("建用户失败");
        let user_id = user.id;
        let follow = sqlx::query(
            r#"INSERT INTO account.follow_relation
               (user_id, follow_platform, follow_address, execute_venue, channel, config, same_venue_only, active)
               VALUES ($1,'polymarket',$2,'polymarket','tg','{}'::jsonb, false, true)
               RETURNING id"#,
        )
        .bind(user_id)
        .bind(format!("0xstage2{tg_id}"))
        .fetch_one(&db)
        .await
        .expect("建 follow_relation 失败");
        let follow_id: uuid::Uuid = sqlx::Row::get(&follow, "id");

        // 3. owner signer + 派生 L2（L1 deriveApiKey，幂等）
        let owner_signer = sharpside_venues_polymarket::clob::signer_from_hex(&owner_pk)
            .expect("owner PK 解析失败");
        let owner_address: alloy_primitives::Address =
            owner_addr_str.parse().expect("owner addr 解析失败");
        assert_eq!(owner_signer.address(), owner_address, "PK 与地址不一致");
        let _dw_address: alloy_primitives::Address = dw_addr_str.parse().expect("DW addr 解析失败");
        let client = sharpside_venues_polymarket::PolymarketClient::new();
        let ts = chrono::Utc::now().timestamp();
        let auth_sig =
            sharpside_venues_polymarket::clob::build_l1_auth_signature(&owner_signer, ts)
                .expect("L1 签名失败");
        let l2 = client
            .derive_api_key_l1(owner_address, &auth_sig, ts)
            .await
            .expect("L1 deriveApiKey 失败（代理/网络）");
        eprintln!("step3 L2 派生 ok: api_key={}", l2.api_key);

        // 4. DevKms 加密 owner_key + l2.secret
        let kms = sharpside_kms::DevKms::enabled_for_test();
        let enc_owner = kms.encrypt(&owner_pk).unwrap();
        let enc_l2 = kms.encrypt(&l2.secret).unwrap();

        // 5. 注入 funded 凭证
        let blob = serde_json::json!({
            "kind": "deposit_wallet_delegated",
            "deposit_wallet_address": dw_addr_str,
            "owner_address": owner_addr_str,
            "encrypted_owner_key": enc_owner,
            "l2_api_key": l2.api_key,
            "encrypted_l2_secret": enc_l2,
            "l2_passphrase": l2.passphrase,
            "builder_code": builder_code,
        });
        acct::upsert_credential_with_proxy(&db, user_id, "polymarket", &blob, Some(&dw_addr_str))
            .await
            .expect("写凭证失败");
        eprintln!("step5 funded 凭证已注入 user={user_id} dw={dw_addr_str}");

        // 6. Gamma 取活跃 token + CLOB /book。挂单价 = mid 对齐 tick 向下取整（slip≈0 必过滑点，
        //    且 price ≤ mid < ask → 作为 bid 挂着不立即成交）。只需 bid<ask 的真实盘口即可。
        let mkt_url = format!(
            "{}/markets?limit=50&active=true&closed=false&order=volume24hr&ascending=false",
            client.gamma_api()
        );
        let mkt: serde_json::Value = client
            .http_get_json(&mkt_url)
            .await
            .expect("Gamma /markets 失败");
        let arr = mkt.as_array().expect("/markets 返回数组");
        let mut token_id = String::new();
        let mut condition_id = String::new();
        let mut price = 0.0_f64;
        for pick in arr {
            let Some(ids_str) = pick.get("clobTokenIds").and_then(|v| v.as_str()) else {
                continue;
            };
            if ids_str.is_empty() || ids_str == "[]" {
                continue;
            }
            let Ok(mut ids) = serde_json::from_str::<Vec<String>>(ids_str) else {
                continue;
            };
            if ids.is_empty() {
                continue;
            }
            let tid = ids.remove(0);
            let cid = pick
                .get("conditionId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if cid.is_empty() {
                continue;
            }
            let Ok(b) = client.book(&cid, &tid).await else {
                continue;
            };
            let Some(bb) = b.bids.first() else { continue };
            let Some(ba) = b.asks.first() else { continue };
            let Ok(bb_p): Result<f64, _> = bb.price.as_deref().unwrap_or("0").parse() else {
                continue;
            };
            let Ok(ba_p): Result<f64, _> = ba.price.as_deref().unwrap_or("0").parse() else {
                continue;
            };
            if bb_p <= 0.0 || ba_p <= 0.0 || bb_p >= ba_p {
                continue;
            }
            let mid = (bb_p + ba_p) / 2.0;
            let tick: f64 = pick
                .get("minimumTickSize")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .or_else(|| pick.get("minimumTickSize").and_then(|v| v.as_f64()))
                .unwrap_or(0.01);
            // 挂单价 = mid 向下对齐 tick（确保 ≤ mid < ask，作为 bid 不立即成交；slip≈0）
            let p = (mid / tick).floor() * tick;
            if p <= 0.0 {
                continue;
            }
            token_id = tid;
            condition_id = cid;
            price = p;
            eprintln!(
                "step6 选中 market token={token_id} bid={bb_p} ask={ba_p} mid={mid} tick={tick} 挂单价={price} q={:?}",
                pick.get("question").and_then(|v| v.as_str())
            );
            break;
        }
        assert!(price > 0.0, "无可用活跃市场（盘口为空/反向）");

        // 7. size 取最小过 min_notional=1.0（polymarket_default）+ 市场 min size（实测部分市场 ≥5）
        let size = ((1.0_f64 / price).ceil()).max(5.0);
        let notional = price * size;
        eprintln!("step7 price={price} size={size} notional={notional}（makerAmount≈{:.2} USDC 锁定，撤单返还）", price * size);
        assert!(notional >= 1.0, "notional 低于 min_notional");

        // 8. 插 pending tg copy_order
        let order_id = uuid::Uuid::new_v4();
        acct::enqueue_copy_order(
            &db,
            order_id,
            follow_id,
            user_id,
            "polymarket",
            "polymarket",
            &condition_id,
            &token_id,
            "buy",
            price,
            size,
            "tg",
            chrono::Utc::now(),
            None,
            "pending",
            None,
        )
        .await
        .expect("插 copy_order 失败");
        let order = acct::list_pending_copy_orders(&db, "tg", 50)
            .await
            .unwrap()
            .into_iter()
            .find(|o| o.id == order_id)
            .expect("找不到刚插的 order");

        // 9. 构造 AppState（dry_run=false）+ PolymarketVenue + DevKms
        let config = crate::config::Config {
            listen_addr: "0.0.0.0:0".into(),
            database_url: db_url,
            db_max_connections: 5,
            worker_exec_secs: 5,
            dry_run: false,
            daily_max_notional: 100_000.0,
            max_open_positions: 100,
            rapid_flip_window_secs: 60,
            rapid_flip_max_count: 100,
            consecutive_loss_limit: 100,
            min_dw_balance: 0.0, // stage2 不校验余额（测下单链路，非余额风控）
            withdraw_min_amount: 1.0,
            withdraw_max_amount: 10000.0,
            withdraw_daily_max: 10000.0,
            worker_redeem_secs: 300,
            redeem_worker_enabled: true,
            worker_reclaim_secs: 60,
            dispatched_timeout_secs: 600,
            reclaim_worker_enabled: true,
            worker_reconcile_secs: 15,
            reconcile_timeout_secs: 120,
            reconcile_worker_enabled: true,
            copy_order_type: sharpside_venues_core::OrderType::Fak,
            aggressive_pricing: true,
            jwt_secret: "stage2".into(),
        };
        let mut venue =
            sharpside_venues_polymarket::PolymarketVenue::new().with_kms(std::sync::Arc::new(kms));
        let _ = &mut venue;
        let mut registry = sharpside_venues_core::VenueRegistry::new();
        registry.register(std::sync::Arc::new(venue));
        let state = crate::state::AppState::new(config, db.clone(), registry);

        // 10. 真打：process_one → place_order → POST /order
        process_one(&state, &order).await.expect("process_one 异常");

        // 11. 校验 filled + 真实 orderID
        let updated = acct::get_copy_order(&db, order_id).await.unwrap();
        eprintln!("step11 copy_order.status={}", updated.status);
        assert_eq!(updated.status, "filled", "未 filled（查滑点/余额/签名）");
        let exec_row = sqlx::query(
            r#"SELECT venue_order_id, tx_hash FROM account.copy_execution
               WHERE copy_order_id = $1 ORDER BY id DESC LIMIT 1"#,
        )
        .bind(order_id)
        .fetch_one(&db)
        .await
        .unwrap();
        let venue_order_id: Option<String> = sqlx::Row::get(&exec_row, "venue_order_id");
        let tx_hash: Option<String> = sqlx::Row::get(&exec_row, "tx_hash");
        let oid = venue_order_id.as_deref().unwrap_or("");
        eprintln!(
            "step11 真实 orderID={oid} sig_len={}",
            tx_hash.as_deref().map(|s| s.len()).unwrap_or(0)
        );
        assert!(!oid.is_empty(), "venue_order_id 为空");
        assert!(
            !oid.starts_with("dry-"),
            "仍是 dry-sign 合成 orderID: {oid}"
        );
        assert!(
            oid.starts_with("0x") || oid.len() >= 8,
            "不像真实 orderID: {oid}"
        );

        // 12. 立即撤单（撤回锁定的 USDC）
        match client
            .cancel_order_l2(oid, &l2.api_key, &l2.secret, &l2.passphrase, owner_address)
            .await
        {
            Ok(v) => eprintln!("step12 撤单 ok: {v}"),
            Err(e) => {
                eprintln!("step12 撤单失败（订单可能已成交/已撤，人工核对 Polymarket 端）: {e}")
            }
        }

        // 清理测试数据
        let _ = sqlx::query("DELETE FROM account.copy_order WHERE user_id = $1")
            .bind(user_id)
            .execute(&db)
            .await;
        let _ = sqlx::query("DELETE FROM account.follow_relation WHERE user_id = $1")
            .bind(user_id)
            .execute(&db)
            .await;
        let _ = sqlx::query("DELETE FROM account.users WHERE id = $1")
            .bind(user_id)
            .execute(&db)
            .await;
        eprintln!("STAGE2_RESULT=REAL_ORDER_PLACED_AND_CANCELLED ✅ copier worker 真打 /order 路径验证通过");
    }

    /// 安全修复 1.3 e2e：`COPIER_DRY_RUN=false` + `POLYMARKET_CLOB_POST≠1` → dry-sign。
    /// 验收：copy_order 留 `skipped`（skip_reason 含 dry-sign），**无** copy_execution 行。
    ///
    /// 前置同 stage2（PG + 代理可达 Polymarket + .env.local funded 凭证）：
    /// ```bash
    /// set -a; source .env.local; set +a
    /// DATABASE_URL='postgres://sharpside:sharpside_dev@127.0.0.1:5432/sharpside' \
    /// SHARPSIDE_KMS_DEV_PLAINTEXT=1 \
    ///   cargo test -p sharpside-copier --test '*' stage2_dry_sign -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore]
    async fn stage2_dry_sign_skips_without_fill() {
        use sharpside_kms::Kms;
        let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://sharpside:sharpside_dev@127.0.0.1:5432/sharpside".to_string()
        });
        let owner_pk = std::env::var("POLYMARKET_TEST_OWNER_PK").expect("需设 POLYMARKET_TEST_OWNER_PK");
        let owner_addr_str = std::env::var("POLYMARKET_TEST_OWNER_ADDRESS").expect("需设 POLYMARKET_TEST_OWNER_ADDRESS");
        let dw_addr_str = std::env::var("POLYMARKET_TEST_DEPOSIT_WALLET").expect("需设 POLYMARKET_TEST_DEPOSIT_WALLET");
        let builder_code = std::env::var("POLYMARKET_BUILDER_CODE").unwrap_or_else(|_| "019f6e85-dce2-7a7a-aa72-cadb8d498bbe".into());
        // dry-sign：显式移除 POLYMARKET_CLOB_POST（≠1 → dry-sign 分支）
        std::env::remove_var("POLYMARKET_CLOB_POST");

        let db = sharpside_db::connect(&db_url, 5).await.expect("连 DB 失败");
        sharpside_db::migrate(&db).await.expect("迁移失败");

        let tg_id: i64 = 9_999_100_000 + (chrono::Utc::now().timestamp() % 1_000_000);
        let user = acct::upsert_tg_user(&db, tg_id).await.expect("建用户失败");
        let user_id = user.id;
        let follow = sqlx::query(
            r#"INSERT INTO account.follow_relation
               (user_id, follow_platform, follow_address, execute_venue, channel, config, same_venue_only, active)
               VALUES ($1,'polymarket',$2,'polymarket','tg','{}'::jsonb, false, true)
               RETURNING id"#,
        )
        .bind(user_id)
        .bind(format!("0xdrysign{tg_id}"))
        .fetch_one(&db)
        .await
        .expect("建 follow_relation 失败");
        let follow_id: uuid::Uuid = sqlx::Row::get(&follow, "id");

        let owner_signer = sharpside_venues_polymarket::clob::signer_from_hex(&owner_pk).expect("owner PK 解析失败");
        let owner_address: alloy_primitives::Address = owner_addr_str.parse().expect("owner addr 解析失败");
        let client = sharpside_venues_polymarket::PolymarketClient::new();
        let ts = chrono::Utc::now().timestamp();
        let auth_sig = sharpside_venues_polymarket::clob::build_l1_auth_signature(&owner_signer, ts).expect("L1 签名失败");
        let l2 = client.derive_api_key_l1(owner_address, &auth_sig, ts).await.expect("L1 deriveApiKey 失败（代理/网络）");

        let kms = sharpside_kms::DevKms::enabled_for_test();
        let enc_owner = kms.encrypt(&owner_pk).unwrap();
        let enc_l2 = kms.encrypt(&l2.secret).unwrap();
        let blob = serde_json::json!({
            "kind": "deposit_wallet_delegated",
            "deposit_wallet_address": dw_addr_str,
            "owner_address": owner_addr_str,
            "encrypted_owner_key": enc_owner,
            "l2_api_key": l2.api_key,
            "encrypted_l2_secret": enc_l2,
            "l2_passphrase": l2.passphrase,
            "builder_code": builder_code,
        });
        acct::upsert_credential_with_proxy(&db, user_id, "polymarket", &blob, Some(&dw_addr_str)).await.expect("写凭证失败");

        // 取一个有 bid<ask 的活跃 token + 挂单价（需网络拉 Gamma + /book）
        let mkt_url = format!("{}/markets?limit=50&active=true&closed=false&order=volume24hr&ascending=false", client.gamma_api());
        let mkt: serde_json::Value = client.http_get_json(&mkt_url).await.expect("Gamma /markets 失败");
        let arr = mkt.as_array().expect("/markets 返回数组");
        let mut token_id = String::new();
        let mut condition_id = String::new();
        let mut price = 0.0_f64;
        for pick in arr {
            let Some(ids_str) = pick.get("clobTokenIds").and_then(|v| v.as_str()) else { continue };
            let Ok(mut ids) = serde_json::from_str::<Vec<String>>(ids_str) else { continue };
            if ids.is_empty() { continue; }
            let tid = ids.remove(0);
            let cid = pick.get("conditionId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if cid.is_empty() { continue; }
            let Ok(b) = client.book(&cid, &tid).await else { continue };
            let (Some(bb), Some(ba)) = (b.bids.first(), b.asks.first()) else { continue };
            let (Ok(bb_p), Ok(ba_p)) = (bb.price.as_deref().unwrap_or("0").parse::<f64>(), ba.price.as_deref().unwrap_or("0").parse::<f64>()) else { continue };
            if !(bb_p > 0.0 && ba_p > 0.0 && bb_p < ba_p) { continue; }
            let mid = (bb_p + ba_p) / 2.0;
            let tick: f64 = pick.get("minimumTickSize").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).or_else(|| pick.get("minimumTickSize").and_then(|v| v.as_f64())).unwrap_or(0.01);
            let p = (mid / tick).floor() * tick;
            if p <= 0.0 { continue; }
            token_id = tid; condition_id = cid; price = p;
            break;
        }
        assert!(price > 0.0, "无可用活跃市场");
        let size = ((1.0_f64 / price).ceil()).max(5.0);

        let order_id = uuid::Uuid::new_v4();
        acct::enqueue_copy_order(&db, order_id, follow_id, user_id, "polymarket", "polymarket", &condition_id, &token_id, "buy", price, size, "tg", chrono::Utc::now(), None, "pending", None).await.expect("插 copy_order 失败");
        let order = acct::list_pending_copy_orders(&db, "tg", 50).await.unwrap().into_iter().find(|o| o.id == order_id).expect("找不到刚插的 order");

        let config = crate::config::Config {
            listen_addr: "0.0.0.0:0".into(),
            database_url: db_url,
            db_max_connections: 5,
            worker_exec_secs: 5,
            dry_run: false, // 关键：非 dry_run，走真实 place_order → dry-sign 分支
            daily_max_notional: 100_000.0,
            max_open_positions: 100,
            rapid_flip_window_secs: 60,
            rapid_flip_max_count: 100,
            consecutive_loss_limit: 100,
            min_dw_balance: 0.0,
            withdraw_min_amount: 1.0,
            withdraw_max_amount: 10000.0,
            withdraw_daily_max: 10000.0,
            worker_redeem_secs: 300,
            redeem_worker_enabled: true,
            worker_reclaim_secs: 60,
            dispatched_timeout_secs: 600,
            reclaim_worker_enabled: true,
            worker_reconcile_secs: 15,
            reconcile_timeout_secs: 120,
            reconcile_worker_enabled: true,
            copy_order_type: sharpside_venues_core::OrderType::Fak,
            aggressive_pricing: true,
            jwt_secret: "stage2".into(),
        };
        let venue = sharpside_venues_polymarket::PolymarketVenue::new().with_kms(std::sync::Arc::new(kms));
        let mut registry = sharpside_venues_core::VenueRegistry::new();
        registry.register(std::sync::Arc::new(venue));
        let state = crate::state::AppState::new(config, db.clone(), registry);

        process_one(&state, &order).await.expect("process_one 异常");

        // 验收 1：copy_order 留 skipped，skip_reason 含 dry-sign
        let updated = acct::get_copy_order(&db, order_id).await.unwrap();
        eprintln!("dry-sign copy_order.status={} reason={:?}", updated.status, updated.skip_reason);
        assert_eq!(updated.status, "skipped", "dry-sign 应置 skipped，而非 submitted/filled");
        let reason = updated.skip_reason.unwrap_or_default();
        assert!(reason.contains("dry-sign"), "skip_reason 须含 dry-sign，got: {reason}");

        // 验收 2：无 copy_execution 行（不得记假成交）
        let exec_count: i64 = sqlx::query_scalar("SELECT count(*) FROM account.copy_execution WHERE copy_order_id = $1")
            .bind(order_id)
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(exec_count, 0, "dry-sign 不得写 copy_execution（假成交），但查出 {exec_count} 行");

        // 清理
        let _ = sqlx::query("DELETE FROM account.copy_order WHERE user_id = $1").bind(user_id).execute(&db).await;
        let _ = sqlx::query("DELETE FROM account.follow_relation WHERE user_id = $1").bind(user_id).execute(&db).await;
        let _ = sqlx::query("DELETE FROM account.users WHERE id = $1").bind(user_id).execute(&db).await;
        eprintln!("STAGE2_DRY_SIGN_RESULT=SKIPPED_NO_FILL ✅ dry-sign 未记成交、置 skipped");
    }

    /// 真实跟单 e2e：信号 → follow /internal/signals → 派生 copy_order → copier worker 真打 /order → 撤单。
    ///
    /// 与 stage2 的区别：stage2 直接调 `process_one`（跳过信号派生）；本测试走真实 HTTP 信号入口，
    /// 由运行中的 follow 服务派生 copy_order、运行中的 copier worker 拾取并下单——完整跟单链路。
    ///
    /// 前置（由 infra/e2e_real_trade.sh 编排）：
    ///   - PG + account + follow + copier 服务已起（copier: COPIER_DRY_RUN=false + POLYMARKET_CLOB_POST=1）
    ///   - 代理可达 Polymarket
    ///   - .env.local 已 source（funded owner PK / DW / builder code）
    ///
    /// 跑法：
    /// ```bash
    /// bash infra/e2e_real_trade.sh
    /// ```
    #[tokio::test]
    #[ignore]
    async fn real_copy_trade_e2e() {
        use sharpside_kms::Kms;

        let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://sharpside:sharpside_dev@127.0.0.1:5432/sharpside".to_string()
        });
        let follow_url =
            std::env::var("FOLLOW_URL").unwrap_or_else(|_| "http://127.0.0.1:8082".to_string());
        let owner_pk = std::env::var("POLYMARKET_TEST_OWNER_PK")
            .expect("需设 POLYMARKET_TEST_OWNER_PK（.env.local）");
        let owner_addr_str = std::env::var("POLYMARKET_TEST_OWNER_ADDRESS")
            .expect("需设 POLYMARKET_TEST_OWNER_ADDRESS");
        let dw_addr_str = std::env::var("POLYMARKET_TEST_DEPOSIT_WALLET")
            .expect("需设 POLYMARKET_TEST_DEPOSIT_WALLET");
        let builder_code = std::env::var("POLYMARKET_BUILDER_CODE")
            .unwrap_or_else(|_| "019f6e85-dce2-7a7a-aa72-cadb8d498bbe".into());

        // 1. 连 DB + 迁移
        let db = sharpside_db::connect(&db_url, 5).await.expect("连 DB 失败");
        sharpside_db::migrate(&db).await.expect("迁移失败");

        // 2. 建测试用户（TG 渠道）
        let tg_id: i64 = 8_888_000_000 + (chrono::Utc::now().timestamp() % 1_000_000);
        let user = acct::upsert_tg_user(&db, tg_id).await.expect("建用户失败");
        let user_id = user.id;
        eprintln!("step1 建用户 user_id={user_id} tg_id={tg_id}");

        // 3. owner signer + 派生 L2
        let owner_signer = sharpside_venues_polymarket::clob::signer_from_hex(&owner_pk)
            .expect("owner PK 解析失败");
        let owner_address: alloy_primitives::Address =
            owner_addr_str.parse().expect("owner addr 解析失败");
        assert_eq!(owner_signer.address(), owner_address, "PK 与地址不一致");
        let client = sharpside_venues_polymarket::PolymarketClient::new();
        let ts = chrono::Utc::now().timestamp();
        let auth_sig =
            sharpside_venues_polymarket::clob::build_l1_auth_signature(&owner_signer, ts)
                .expect("L1 签名失败");
        let l2 = client
            .derive_api_key_l1(owner_address, &auth_sig, ts)
            .await
            .expect("L1 deriveApiKey 失败（代理/网络）");
        eprintln!("step3 L2 派生 ok: api_key={}", l2.api_key);

        // 4. DevKms 加密 + 注入 funded 凭证
        let kms = sharpside_kms::DevKms::enabled_for_test();
        let enc_owner = kms.encrypt(&owner_pk).unwrap();
        let enc_l2 = kms.encrypt(&l2.secret).unwrap();
        let blob = serde_json::json!({
            "kind": "deposit_wallet_delegated",
            "deposit_wallet_address": dw_addr_str,
            "owner_address": owner_addr_str,
            "encrypted_owner_key": enc_owner,
            "l2_api_key": l2.api_key,
            "encrypted_l2_secret": enc_l2,
            "l2_passphrase": l2.passphrase,
            "builder_code": builder_code,
        });
        acct::upsert_credential_with_proxy(&db, user_id, "polymarket", &blob, Some(&dw_addr_str))
            .await
            .expect("写凭证失败");
        eprintln!("step4 funded 凭证已注入 dw={dw_addr_str}");

        // 5. Gamma 取活跃 market + CLOB /book + /markets 元数据（拿 minimum_order_size）
        let mkt_url = format!(
            "{}/markets?limit=50&active=true&closed=false&order=volume24hr&ascending=false",
            client.gamma_api()
        );
        let mkt: serde_json::Value = client
            .http_get_json(&mkt_url)
            .await
            .expect("Gamma /markets 失败");
        let arr = mkt.as_array().expect("/markets 返回数组");
        let mut token_id = String::new();
        let mut condition_id = String::new();
        let mut price = 0.0_f64;
        let mut min_size = 5.0_f64;
        for pick in arr {
            let Some(ids_str) = pick.get("clobTokenIds").and_then(|v| v.as_str()) else {
                continue;
            };
            if ids_str.is_empty() || ids_str == "[]" {
                continue;
            }
            let Ok(mut ids) = serde_json::from_str::<Vec<String>>(ids_str) else {
                continue;
            };
            if ids.is_empty() {
                continue;
            }
            let tid = ids.remove(0);
            let cid = pick
                .get("conditionId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if cid.is_empty() {
                continue;
            }
            let Ok(b) = client.book(&cid, &tid).await else {
                continue;
            };
            let Some(bb) = b.bids.first() else { continue };
            let Some(ba) = b.asks.first() else { continue };
            let Ok(bb_p): Result<f64, _> = bb.price.as_deref().unwrap_or("0").parse() else {
                continue;
            };
            let Ok(ba_p): Result<f64, _> = ba.price.as_deref().unwrap_or("0").parse() else {
                continue;
            };
            if bb_p <= 0.0 || ba_p <= 0.0 || bb_p >= ba_p {
                continue;
            }
            let mid = (bb_p + ba_p) / 2.0;
            let tick: f64 = pick
                .get("minimumTickSize")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .or_else(|| pick.get("minimumTickSize").and_then(|v| v.as_f64()))
                .unwrap_or(0.01);
            let p = (mid / tick).floor() * tick;
            if p <= 0.0 {
                continue;
            }
            // 窄价差过滤：spread/mid ≤ 4%（aggressive 买取 best_ask 时相对 mid 滑点 ≤200bps，
            // 必过 check_slippage 200bps 门禁）。避免选中 0.001/0.999 这类单边极端价差市场被风控跳过。
            let spread = ba_p - bb_p;
            if spread / mid > 0.04 {
                continue;
            }
            // 价位过滤：mid ∈ [0.05, 0.95] 且 best_ask < 0.99，避开近决断市场（买在 ≈1.0 无意义且
            // Polymarket 拒 price≥1 的买单），选 mid 适中的正常市场确保真成交有意义。
            if !(0.05..=0.95).contains(&mid) || ba_p >= 0.99 {
                continue;
            }
            // 拉取市场真实 minimum_order_size（新 min_size 风控用）
            let ms = client
                .clob_market(&cid)
                .await
                .ok()
                .and_then(|m| m.minimum_order_size)
                .unwrap_or(5.0)
                .max(5.0);
            token_id = tid;
            condition_id = cid;
            // 信号价取 best_ask（源钱包"买"吃 ask 的真实成交价）：aggressive 跟随再吃当前 ask，
            // 必立即成交；floor(mid) 作信号价会被 200bps cap 钳到 ask 下方挂死不成交。
            price = ba_p;
            min_size = ms;
            eprintln!(
                "step5 选中 market q={:?} bid={bb_p} ask={ba_p} mid={mid} tick={tick} 挂单价={price} min_size={min_size}",
                pick.get("question").and_then(|v| v.as_str())
            );
            break;
        }
        assert!(price > 0.0, "无可用活跃市场（盘口为空/反向）");

        // 6. 创建 follow_relation（Fixed sizing，amount 使 size = min_size+1 必过股数下限 + min_notional）
        let trader_id = format!("0xrealtrade{tg_id}");
        let target_size = min_size + 1.0;
        let amount = (target_size * price).max(1.5); // notional >= 1.0 且 size >= min_size
        let config_json = serde_json::json!({
            "sizing": {"mode": "fixed", "value": {"amount": amount}},
            "execute_venue": "polymarket",
            "channel": "tg",
            "same_venue_only": false,
        });
        let follow = sqlx::query(
            r#"INSERT INTO account.follow_relation
               (user_id, follow_platform, follow_address, execute_venue, channel, config, same_venue_only, active)
               VALUES ($1,'polymarket',$2,'polymarket','tg',$3::jsonb, false, true)
               RETURNING id"#,
        )
        .bind(user_id)
        .bind(&trader_id)
        .bind(config_json.to_string())
        .fetch_one(&db)
        .await
        .expect("建 follow_relation 失败");
        let follow_id: uuid::Uuid = sqlx::Row::get(&follow, "id");
        let expect_size = amount / price;
        eprintln!("step6 建跟随 follow_id={follow_id} trader={trader_id} amount={amount} → 预期 size={expect_size:.2}（min_size={min_size}）");

        // 7. POST 信号到 follow /internal/signals（真实信号入口，非直接插 copy_order）
        let sig_body = serde_json::json!({
            "platform": "polymarket",
            "trader_id": trader_id,
            "token_id": token_id,
            "market_id": condition_id,
            "side": "buy",
            "price": price,
            "size": 100.0, // Fixed sizing 下 signal.size 不参与计算（size=amount/price）
            "ts": chrono::Utc::now().to_rfc3339(),
        });
        let http = reqwest::Client::new();
        // follow /internal/signals 现强制要求 X-Internal-Secret（与 follow 服务的
        // INTERNAL_SIGNAL_SECRET 一致）；e2e 脚本统一用 e2e-internal-secret 启动 follow。
        let signal_secret =
            std::env::var("INTERNAL_SIGNAL_SECRET").unwrap_or_else(|_| "e2e-internal-secret".into());
        let sig_resp = http
            .post(format!("{follow_url}/internal/signals"))
            .header("X-Internal-Secret", signal_secret)
            .json(&sig_body)
            .send()
            .await
            .expect("POST /internal/signals 失败（follow 服务未起？）");
        let status = sig_resp.status();
        let sig_text = sig_resp.text().await.unwrap_or_default();
        assert!(status.is_success(), "信号派生 HTTP {status}: {sig_text}");
        let sig_v: serde_json::Value = serde_json::from_str(&sig_text).expect("信号响应非 JSON");
        let enqueued = sig_v.get("enqueued").and_then(|v| v.as_u64()).unwrap_or(0);
        assert!(enqueued >= 1, "信号未派生出 copy_order: {sig_text}");
        eprintln!("step7 信号已派生 enqueued={enqueued}: {sig_text}");

        // 8. 轮询 DB 等 copier worker 处理（copier 服务单独运行，WORKER_EXEC_SECS=2 轮询）
        let mut st = String::new();
        let mut order_id = uuid::Uuid::nil();
        for _ in 0..60 {
            let row = sqlx::query(
                r#"SELECT id, status, skip_reason FROM account.copy_order
                   WHERE user_id = $1 AND channel='tg'
                   ORDER BY enqueued_at DESC LIMIT 1"#,
            )
            .bind(user_id)
            .fetch_optional(&db)
            .await
            .unwrap();
            if let Some(r) = row {
                order_id = sqlx::Row::get(&r, "id");
                st = sqlx::Row::get::<Option<String>, _>(&r, "status")
                    .unwrap_or_default()
                    .clone();
                if st == "filled" || st == "failed" || st == "skipped" {
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        eprintln!("step8 copy_order.status={st} id={order_id}");
        if st == "skipped" || st == "failed" {
            let reason: Option<String> =
                sqlx::query("SELECT skip_reason FROM account.copy_order WHERE id=$1")
                    .bind(order_id)
                    .fetch_one(&db)
                    .await
                    .ok()
                    .and_then(|r| sqlx::Row::get(&r, "skip_reason"));
            panic!("跟单未成交 status={st} 原因={reason:?}（查滑点/余额/min_size/签名）");
        }
        assert_eq!(st, "filled", "未 filled（status={st}）");

        // 9. 校验真实 orderID（非 dry-sign 合成）
        let exec_row = sqlx::query(
            r#"SELECT venue_order_id, tx_hash FROM account.copy_execution
               WHERE copy_order_id = $1 ORDER BY id DESC LIMIT 1"#,
        )
        .bind(order_id)
        .fetch_one(&db)
        .await
        .unwrap();
        let venue_order_id: Option<String> = sqlx::Row::get(&exec_row, "venue_order_id");
        let tx_hash: Option<String> = sqlx::Row::get(&exec_row, "tx_hash");
        let oid = venue_order_id.as_deref().unwrap_or("");
        eprintln!(
            "step9 真实成交 orderID={oid} sig_len={}",
            tx_hash.as_deref().map(|s| s.len()).unwrap_or(0)
        );
        assert!(!oid.is_empty(), "venue_order_id 为空");
        assert!(
            !oid.starts_with("dry-"),
            "仍是 dry-sign 合成 orderID: {oid}"
        );

        // 10. 立即撤单（撤回锁定的 USDC）
        match client
            .cancel_order_l2(oid, &l2.api_key, &l2.secret, &l2.passphrase, owner_address)
            .await
        {
            Ok(v) => eprintln!("step10 撤单 ok: {v}"),
            Err(e) => {
                eprintln!("step10 撤单失败（订单可能已成交/已撤，人工核对 Polymarket 端）: {e}")
            }
        }

        // 11. 清理测试数据
        let _ = sqlx::query("DELETE FROM account.copy_execution WHERE copy_order_id IN (SELECT id FROM account.copy_order WHERE user_id=$1)").bind(user_id).execute(&db).await;
        let _ = sqlx::query("DELETE FROM account.copy_order WHERE user_id = $1")
            .bind(user_id)
            .execute(&db)
            .await;
        let _ = sqlx::query("DELETE FROM account.follow_relation WHERE user_id = $1")
            .bind(user_id)
            .execute(&db)
            .await;
        let _ = sqlx::query("DELETE FROM account.users WHERE id = $1")
            .bind(user_id)
            .execute(&db)
            .await;
        eprintln!("REAL_COPY_TRADE=OK ✅ 真实跟单链路（信号→派生→copier→真打→撤单）验证通过");
    }

    /// 真钱 B：验证 `/me/portfolio` 的 `wallet.cash_balance` 对 live CLOB 真实返回。
    ///
    /// 复用 real_copy_trade_e2e 的 funded 凭证注入，但：
    /// - blob 显式带 `provision_live: true`（否则 build_wallet_view 走离线降级，不查余额）；
    /// - 不下单、不撤单——只读余额，零资金风险；
    /// - 自签 JWT（HS256，secret=JWT_SECRET 默认 dev-secret-change-me）→ HTTP GET copier /me/portfolio
    ///   → 断言 `wallet.cash_balance` 为正数（funded DW 充过 pUSD）。
    ///
    /// 前置：copier 服务单独运行（COPIER_LISTEN_ADDR + POLYMARKET_HTTP_PROXY + SHARPSIDE_KMS_DEV_PLAINTEXT=1），
    /// PG 可达，代理可达 Polymarket。`#[ignore]`，不进常规 CI。
    #[tokio::test]
    #[ignore]
    async fn real_portfolio_balance_e2e() {
        use jsonwebtoken::{encode, EncodingKey, Header};
        use sharpside_kms::Kms;

        let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://sharpside:sharpside_dev@127.0.0.1:5432/sharpside".to_string()
        });
        let copier_url =
            std::env::var("COPIER_URL").unwrap_or_else(|_| "http://127.0.0.1:8083".to_string());
        let jwt_secret =
            std::env::var("JWT_SECRET").unwrap_or_else(|_| "dev-secret-change-me".to_string());
        let owner_pk = std::env::var("POLYMARKET_TEST_OWNER_PK")
            .expect("需设 POLYMARKET_TEST_OWNER_PK（.env.local）");
        let owner_addr_str = std::env::var("POLYMARKET_TEST_OWNER_ADDRESS")
            .expect("需设 POLYMARKET_TEST_OWNER_ADDRESS");
        let dw_addr_str = std::env::var("POLYMARKET_TEST_DEPOSIT_WALLET")
            .expect("需设 POLYMARKET_TEST_DEPOSIT_WALLET");
        let builder_code = std::env::var("POLYMARKET_BUILDER_CODE")
            .unwrap_or_else(|_| "019f6e85-dce2-7a7a-aa72-cadb8d498bbe".into());

        // 1. 连 DB + 迁移
        let db = sharpside_db::connect(&db_url, 5).await.expect("连 DB 失败");
        sharpside_db::migrate(&db).await.expect("迁移失败");

        // 2. 建测试用户（TG 渠道）
        let tg_id: i64 = 8_888_000_000 + (chrono::Utc::now().timestamp() % 1_000_000);
        let user = acct::upsert_tg_user(&db, tg_id).await.expect("建用户失败");
        let user_id = user.id;
        eprintln!("B.step1 建用户 user_id={user_id}");

        // 3. owner signer + 派生 L2（仅用于注入凭证；不下单）
        let owner_signer = sharpside_venues_polymarket::clob::signer_from_hex(&owner_pk)
            .expect("owner PK 解析失败");
        let owner_address: alloy_primitives::Address =
            owner_addr_str.parse().expect("owner addr 解析失败");
        assert_eq!(owner_signer.address(), owner_address, "PK 与地址不一致");
        let client = sharpside_venues_polymarket::PolymarketClient::new();
        let ts = chrono::Utc::now().timestamp();
        let auth_sig =
            sharpside_venues_polymarket::clob::build_l1_auth_signature(&owner_signer, ts)
                .expect("L1 签名失败");
        let l2 = client
            .derive_api_key_l1(owner_address, &auth_sig, ts)
            .await
            .expect("L1 deriveApiKey 失败（代理/网络）");
        eprintln!("B.step3 L2 派生 ok: api_key={}", l2.api_key);

        // 4. DevKms 加密 + 注入 funded 凭证（provision_live=true，让 build_wallet_view 查余额）
        let kms = sharpside_kms::DevKms::enabled_for_test();
        let enc_owner = kms.encrypt(&owner_pk).unwrap();
        let enc_l2 = kms.encrypt(&l2.secret).unwrap();
        let blob = serde_json::json!({
            "kind": "deposit_wallet_delegated",
            "deposit_wallet_address": dw_addr_str,
            "owner_address": owner_addr_str,
            "encrypted_owner_key": enc_owner,
            "l2_api_key": l2.api_key,
            "encrypted_l2_secret": enc_l2,
            "l2_passphrase": l2.passphrase,
            "builder_code": builder_code,
            "provision_live": true,
        });
        acct::upsert_credential_with_proxy(&db, user_id, "polymarket", &blob, Some(&dw_addr_str))
            .await
            .expect("写凭证失败");
        eprintln!("B.step4 funded 凭证已注入(provision_live=true) dw={dw_addr_str}");

        // 5. 自签 JWT（HS256，与 copier AuthUser 校验同口径：sub=user_id，exp=now+3600，jti 必填）
        let exp = (chrono::Utc::now().timestamp() + 3600) as usize;
        let claims = serde_json::json!({
            "sub": user_id.to_string(),
            "exp": exp,
            "jti": uuid::Uuid::new_v4().to_string(),
            "iat": chrono::Utc::now().timestamp() as usize,
        });
        let token = encode(
            &Header::new(jsonwebtoken::Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(jwt_secret.as_bytes()),
        )
        .expect("签 JWT 失败");
        eprintln!("B.step5 JWT 已签");

        // 6. HTTP GET copier /me/portfolio?period=1m
        let http = reqwest::Client::new();
        let resp = http
            .get(format!("{copier_url}/me/portfolio?period=1m"))
            .bearer_auth(&token)
            .send()
            .await
            .expect("GET /me/portfolio 失败（copier 服务未起？）");
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        assert!(
            status.is_success(),
            "GET /me/portfolio HTTP {status}: {body}"
        );
        let v: serde_json::Value = serde_json::from_str(&body).expect("响应非 JSON");
        eprintln!("B.step6 /me/portfolio 200");

        // 7. 断言 wallet 字段：owner/deposit 地址 + cash_balance 为正数
        let wallet = v.get("wallet").expect("响应缺 wallet 字段");
        assert!(wallet.is_object(), "wallet 非 object: {wallet}");
        let cb = wallet
            .get("cash_balance")
            .and_then(|x| x.as_f64())
            .expect("wallet.cash_balance 缺失或非数（应为实时 pUSD 余额）");
        eprintln!(
            "B.step7 wallet: owner={} deposit={} provision_live={} cash_balance={cb}",
            wallet
                .get("owner_address")
                .and_then(|x| x.as_str())
                .unwrap_or("?"),
            wallet
                .get("deposit_wallet_address")
                .and_then(|x| x.as_str())
                .unwrap_or("?"),
            wallet
                .get("provision_live")
                .and_then(|x| x.as_bool())
                .unwrap_or(false),
        );
        assert!(
            wallet
                .get("provision_live")
                .and_then(|x| x.as_bool())
                .unwrap_or(false),
            "provision_live 应为 true"
        );
        assert!(
            cb > 0.0,
            "cash_balance 应 > 0（funded DW 充过 pUSD），实得 {cb}"
        );

        // 8. 清理测试数据
        let _ = sqlx::query("DELETE FROM account.user_venue_credentials WHERE user_id = $1")
            .bind(user_id)
            .execute(&db)
            .await;
        let _ = sqlx::query("DELETE FROM account.users WHERE id = $1")
            .bind(user_id)
            .execute(&db)
            .await;
        eprintln!(
            "REAL_PORTFOLIO_BALANCE=OK ✅ /me/portfolio.wallet.cash_balance={cb}（实时 pUSD 余额）"
        );
    }
}
