//! backfill worker — 异步回填 raw_trades。对应 `docs/FLOWS.md` §1。
//!
//! ingest worker 只存身份（leaderboard 来源），不拉成交；
//! 本 worker 每个 tick 取一批「从未回填 / 超过 refresh 窗口」的可见交易者，
//! 从 Venue 拉 trades 写 raw_trades，并标记 `trades_backfilled_at`。
//! 绩效由 perf worker 下一轮重算（不在此阻塞）。
//!
//! 幂等：`raw_trades` `ON CONFLICT DO NOTHING`；零成交也标记，避免每轮重试空账户。
//! 限速：地址间拉取 sleep，配合 Polymarket rate limit。
//! 失败策略：拉取成功 / Unsupported / 零成交 → 标记；瞬时错误 → 不标记，下一轮重试。

use crate::state::AppState;
use sharpside_db::queries::{raw, traders as trader_q};
use sharpside_shared::Platform;
use sharpside_venues_core::{Pagination, VenueCapabilities, VenueError};
use std::time::Duration;

/// 每地址拉取的成交上限（与 `POST /traders/import` 一致）。
const TRADES_PAGE_LIMIT: u32 = 500;
/// 地址间拉取的间隔（毫秒），缓解 Polymarket rate limit。
const INTER_FETCH_DELAY_MS: u64 = 200;

/// 从已注册 signal_source Venue 拉某地址成交，写入 `raw_trades`。
///
/// 幂等（`ON CONFLICT DO NOTHING`）。返回成功写入的笔数。
/// 供 backfill worker 与 `POST /traders/import` 共用，避免逻辑重复。
pub async fn backfill_trades_for(
    state: &AppState,
    platform: Platform,
    address: &str,
) -> Result<usize, VenueError> {
    let venue = state
        .registry
        .get(platform)
        .ok_or(VenueError::Unsupported("venue 未注册"))?;
    let trades = venue
        .trades(
            address,
            Pagination {
                limit: TRADES_PAGE_LIMIT,
                offset: 0,
            },
        )
        .await?;
    let mut written = 0usize;
    for t in &trades {
        let side = match t.side {
            sharpside_shared::Side::Buy => "BUY",
            sharpside_shared::Side::Sell => "SELL",
        };
        let price = rust_decimal::Decimal::try_from(t.price).unwrap_or_default();
        let size = rust_decimal::Decimal::try_from(t.size).unwrap_or_default();
        let res = raw::insert_raw_trade(
            &state.db,
            platform.as_str(),
            address,
            &t.token_id,
            Some(&t.market_id),
            side,
            price,
            size,
            t.ts,
            t.tx_hash.as_deref(),
            None,
        )
        .await;
        if res.is_ok() {
            written += 1;
        }
    }
    Ok(written)
}

pub async fn run(state: AppState) {
    let interval = state.config.workers.backfill_secs.max(1);
    let batch = state.config.workers.backfill_batch.max(1) as i64;
    let refresh_days = state.config.workers.backfill_refresh_days.max(1) as i64;
    let mut ticker = tokio::time::interval(Duration::from_secs(interval));
    loop {
        ticker.tick().await;
        // refresh 窗口截止：超过此时间未回填的，增量重拉新成交。
        let cutoff = chrono::Utc::now() - chrono::Duration::days(refresh_days);
        let traders = match trader_q::list_traders_needing_backfill(
            &state.db,
            Some(cutoff),
            batch,
            0,
        )
        .await
        {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "backfill 读待回填清单失败");
                continue;
            }
        };
        if traders.is_empty() {
            continue;
        }
        let mut done = 0usize;
        let mut total_trades = 0usize;
        for t in &traders {
            let Ok(platform) = t.platform.parse::<Platform>() else {
                // 未知 platform：标记跳过，避免每轮重试。
                let _ = trader_q::mark_trades_backfilled(&state.db, &t.platform, &t.address).await;
                continue;
            };
            // 仅 signal_source Venue 支持 trades。
            let supports_trades = state
                .registry
                .get(platform)
                .map(|v| {
                    v.info()
                        .capabilities
                        .contains(VenueCapabilities::SIGNAL_SOURCE)
                })
                .unwrap_or(false);

            let mut should_mark = true;
            if supports_trades {
                match backfill_trades_for(&state, platform, &t.address).await {
                    Ok(n) => total_trades += n,
                    Err(VenueError::Unsupported(_)) => {}
                    Err(e) => {
                        // 瞬时错误：不标记，下一轮重试。
                        tracing::warn!(
                            platform = %t.platform, address = %t.address, error = %e,
                            "backfill 拉 trades 失败，本轮不标记，下轮重试"
                        );
                        should_mark = false;
                    }
                }
            }
            if should_mark {
                if let Err(e) =
                    trader_q::mark_trades_backfilled(&state.db, &t.platform, &t.address).await
                {
                    tracing::warn!(
                        platform = %t.platform, address = %t.address, error = %e,
                        "标记 trades_backfilled_at 失败"
                    );
                } else {
                    done += 1;
                }
            }
            tokio::time::sleep(Duration::from_millis(INTER_FETCH_DELAY_MS)).await;
        }
        tracing::info!(traders = done, trades = total_trades, "backfill 本轮完成");
    }
}
