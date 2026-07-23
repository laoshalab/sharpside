//! trade_watch worker — 第 3 层（方案 A）主信号源：逐笔成交信号。
//!
//! 对每个被跟随/热钥地址增量轮询 Venue `/trades`，每条新成交 → 写 `raw_trades`（去重账）+
//! 构造 `SignalPayload`（携带成交 ID 作 `source_id`）→ `emit_signals` → follow 派生 `copy_order`。
//!
//! 与 hot（positions diff）的关系：trades 是**权威主源**，逐笔不吞往返、价格真实、不误判赎回；
//! hot 降级为**对账补漏**（见 hot.rs 的覆盖检查），只补 trades 漏的残差，跨源去重靠 raw_trades 账。
//!
//! 速率：Polymarket `/trades` ~200 req/10s，由 `PolymarketClient` governor 令牌桶节流；
//! 被跟随地址多时每 tick 全量轮询，governor 排队，故 tick 间隔不必过小（默认 3s）。
//! Data API 成交可见有数秒~十几秒延迟，是源检测的物理下限，WS 也救不了（WS 不给他人成交）。

use crate::registry::enabled_signal_sources;
use crate::state::AppState;
use crate::workers::signal_emit::{emit_signals, SignalPayload};
use rust_decimal::Decimal;
use sharpside_db::queries::{monitor, raw};
use sharpside_shared::Platform;
use sharpside_venues_core::{Pagination, VenueCapabilities, VenueError};
use std::time::Duration;

/// 单次轮询每地址拉取的成交上限（最新优先）。Polymarket `/trades` 单次分页 ~3500，
/// 取 200 足够覆盖一轮 tick 的新增；落后过多时由 hot 对账补漏 + 人工介入。
const TRADE_PAGE_LIMIT: u32 = 200;

pub async fn run(state: AppState) {
    let tick = state.config.workers.trade_watch_secs.max(1);
    let mut ticker = tokio::time::interval(Duration::from_secs(tick));
    loop {
        ticker.tick().await;
        if let Err(e) = tick_once(&state).await {
            tracing::error!(error = %e, "trade_watch tick 失败");
        }
        state.worker_ticks.touch_trade_watch();
    }
}

async fn tick_once(state: &AppState) -> Result<(), anyhow::Error> {
    for platform in enabled_signal_sources(&state.config.venues) {
        let Some(venue) = state.registry.get(platform) else {
            continue;
        };
        if !venue
            .info()
            .capabilities
            .contains(VenueCapabilities::SIGNAL_SOURCE)
        {
            continue;
        }
        let targets = match monitor::list_all_signal_targets(&state.db, platform.as_str()).await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(platform = platform.as_str(), error = %e, "trade_watch 读目标清单失败");
                continue;
            }
        };
        for w in &targets {
            // 增量游标：(ts, trade_id)。None = 从未轮询过（bootstrap）。
            // 复合游标消除同秒多笔漏检（旧 ts > MAX(ts) 会跳过同秒后续笔）。
            let cursor = match raw::latest_trade_cursor(&state.db, platform.as_str(), &w.address).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(
                        platform = platform.as_str(), address = %w.address, error = %e,
                        "读 latest_trade_cursor 失败，跳过该地址本轮",
                    );
                    continue;
                }
            };

            // 翻页拉取：最新优先，按 offset 翻页直到无新成交或达页数上限。
            // 爆发成交（一轮 >200 笔）时单页截断会永久丢失，翻页追上为止。
            const MAX_PAGES: u32 = 5;
            let mut all_new_trades: Vec<sharpside_venues_core::Trade> = Vec::new();
            let mut offset = 0u32;
            let mut exhausted = false;
            for _ in 0..MAX_PAGES {
                let trades = match venue
                    .trades(&w.address, Pagination { limit: TRADE_PAGE_LIMIT, offset })
                    .await
                {
                    Ok(t) => t,
                    Err(VenueError::Unsupported(_)) => break,
                    Err(e) => {
                        tracing::warn!(
                            platform = platform.as_str(), address = %w.address, error = %e,
                            "trade_watch 拉 trades 失败（offset={}）", offset,
                        );
                        break;
                    }
                };
                if trades.is_empty() {
                    exhausted = true;
                    break;
                }
                // bootstrap：cursor 为空时只记基线（最新一条），不 emit。
                if cursor.is_none() {
                    if let Some(latest) = trades.first() {
                        if let Err(e) = upsert_raw_trade(state, platform, &w.address, latest).await {
                            tracing::warn!(address = %w.address, error = %e, "bootstrap 写基线 raw_trade 失败");
                        }
                    }
                    tracing::info!(
                        platform = platform.as_str(), address = %w.address,
                        "trade_watch bootstrap：仅记基线，本轮不 emit",
                    );
                    exhausted = true;
                    break;
                }
                let (cursor_ts, cursor_id) = cursor.as_ref().unwrap();
                // 复合游标过滤：ts > cursor_ts，或同秒且 id > cursor_id（消除同秒漏检）。
                // /trades 最新优先，故越往后越旧；一旦遇到 ts < cursor_ts 的，后续全是旧的，可提前终止。
                let mut page_new = 0u32;
                let mut hit_old = false;
                for t in &trades {
                    let id = t.tx_hash.as_deref();
                    let is_new = match id {
                        Some(id_str) if Some(id_str) == cursor_id.as_deref() => false,
                        _ => t.ts > *cursor_ts
                            || (t.ts == *cursor_ts && id.is_some() && id > cursor_id.as_deref()),
                    };
                    if is_new {
                        all_new_trades.push(t.clone());
                        page_new += 1;
                    } else if t.ts < *cursor_ts {
                        hit_old = true;
                    }
                }
                // 本页无新成交，或已遇到旧成交（后续更旧），翻页终止。
                if page_new == 0 || hit_old || trades.len() < TRADE_PAGE_LIMIT as usize {
                    exhausted = true;
                    break;
                }
                offset += TRADE_PAGE_LIMIT;
            }
            if !exhausted && all_new_trades.len() as u32 >= MAX_PAGES * TRADE_PAGE_LIMIT {
                tracing::warn!(
                    platform = platform.as_str(), address = %w.address,
                    n = all_new_trades.len(),
                    "trade_watch 翻 {} 页仍追不上（爆发成交），剩余由 hot 对账补漏", MAX_PAGES,
                );
            }
            if all_new_trades.is_empty() {
                continue;
            }
            // /trades 最新优先；按 ts 升序处理，保证同窗口覆盖检查时序正确。
            all_new_trades.sort_by_key(|t| t.ts);
            let new_trades = all_new_trades;

            // 先写信号账，写成功的才 emit。写失败则跳过该笔（不 emit）：
            // 游标取 raw_trades 的 MAX(ts)，写失败即游标未前进，下一轮会重拉该笔重试。
            // 关键：若写失败仍 emit，hot 对账会因 covered 少计而补发 diff（不同 signal_id）→ 双发。
            let trader_id = platform.normalize_trader_id(&w.address);
            let mut signals = Vec::with_capacity(new_trades.len());
            for t in &new_trades {
                if let Err(e) = upsert_raw_trade(state, platform, &w.address, t).await {
                    tracing::warn!(
                        address = %w.address, ts = ?t.ts, error = %e,
                        "写 raw_trade 失败，跳过该笔 emit（下轮游标未前进会重试）；不 emit 以防 hot 覆盖少计双发",
                    );
                    continue;
                }
                // source_id = 成交 ID（trade.tx_hash = d.id.or(transaction_hash)），防同秒同 token 撞键。
                signals.push(SignalPayload {
                    platform,
                    trader_id: trader_id.clone(),
                    token_id: t.token_id.clone(),
                    market_id: t.market_id.clone(),
                    side: t.side,
                    price: t.price,
                    size: t.size,
                    ts: t.ts,
                    identity_id: w.identity_id,
                    source_id: t.tx_hash.clone(),
                });
            }
            if !signals.is_empty() {
                emit_signals(state, signals).await;
            }
        }
    }
    Ok(())
}

/// 写一条 raw_trade（ON CONFLICT DO NOTHING 幂等）。size/price 转 Decimal。
async fn upsert_raw_trade(
    state: &AppState,
    platform: Platform,
    address: &str,
    t: &sharpside_venues_core::Trade,
) -> Result<(), anyhow::Error> {
    let price = Decimal::try_from(t.price).unwrap_or_default();
    let size = Decimal::try_from(t.size).unwrap_or_default();
    let side_str = match t.side {
        sharpside_shared::Side::Buy => "BUY",
        sharpside_shared::Side::Sell => "SELL",
    };
    // trade_id 与 tx_hash：Polymarket map_trade 把 id/transactionHash 都塞进 tx_hash；
    // 这里 trade_id 取同值（玩钱 Venue 用），tx_hash 亦取同值（链上 Venue 用）。二者至少一非空。
    let id = t.tx_hash.as_deref();
    raw::insert_raw_trade(
        &state.db,
        platform.as_str(),
        address,
        &t.token_id,
        Some(&t.market_id),
        side_str,
        price,
        size,
        t.ts,
        id,
        id,
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use rust_decimal::prelude::ToPrimitive;
    use rust_decimal::Decimal;
    use sharpside_db::queries::{monitor, raw};
    use sharpside_shared::signal_id;

    #[test]
    fn page_limit_is_reasonable() {
        // 200 远小于 /trades 单次分页上限 ~3500，且覆盖一轮 tick（3s）的新增绰绰有余。
        assert!(TRADE_PAGE_LIMIT >= 50 && TRADE_PAGE_LIMIT <= 3500);
    }

    /// signal_id 带 source_id 时同秒同 token 不撞键（跨源去重的语义基础）。
    #[test]
    fn signal_id_source_id_disambiguates() {
        let ts = Utc::now();
        let a = signal_id("polymarket", "0xabc", "tok", ts, Some("trade-1"));
        let b = signal_id("polymarket", "0xabc", "tok", ts, Some("trade-2"));
        let none = signal_id("polymarket", "0xabc", "tok", ts, None);
        assert_ne!(a, b);
        assert_ne!(a, none);
        // 空 source_id 退化为无 source_id（diff 补漏信号走 None 分支）。
        assert_eq!(none, signal_id("polymarket", "0xabc", "tok", ts, Some("")));
    }

    /// 第 3 层 DB 层 dry-run：验证新查询（`sum_signed_trade_size` / `latest_trade_ts` /
    /// `latest_snapshots_before`）+ reconcile 残差数学在真实 PG（含迁移 0042/0043）上成立。
    ///
    /// 三个场景：
    ///   A. trades 全覆盖 Δ → 残差 0（diff 不双计）
    ///   B. trades 漏 Δ → 残差 = Δ（diff 补漏，期望降级行为）
    ///   C. 往返 BUY+SELL 净 0 → Δ=0 且 covered=0（positions-diff 旧法会误判，新法不误发）
    ///
    /// 跑法：
    /// ```bash
    /// docker compose -f infra/docker-compose.yml up -d postgres
    /// DATABASE_URL='postgres://sharpside:sharpside_dev@127.0.0.1:5432/sharpside' \
    ///   cargo test -p sharpside-venue-hub --bins tier3_reconcile -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore]
    async fn tier3_reconcile_residual_math_against_pg() {
        let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://sharpside:sharpside_dev@127.0.0.1:5432/sharpside".to_string()
        });
        let db = sharpside_db::connect(&db_url, 5).await.expect("连 DB 失败");
        sharpside_db::migrate(&db).await.expect("迁移失败（含 0042/0043）");

        // 每次跑用唯一地址，避免与历史数据 / 去重索引冲突。
        let stamp = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let addr = format!("0xtier3{stamp}");
        let platform = "polymarket";
        let t_prev_prev = Utc::now() - Duration::minutes(20);
        let t_prev = Utc::now() - Duration::minutes(10);

        // ── 场景 A：tokA，prev_prev=0 → prev=10，trades BUY 10 全覆盖 ──
        monitor::insert_position_snapshot(
            &db, platform, &addr, "tokA", Some("condA"),
            0.0, 0.0, 0.0, 0.0, t_prev_prev,
        ).await.unwrap();
        monitor::insert_position_snapshot(
            &db, platform, &addr, "tokA", Some("condA"),
            10.0, 0.5, 0.5, 0.0, t_prev,
        ).await.unwrap();
        raw::insert_raw_trade(
            &db, platform, &addr, "tokA", Some("condA"), "BUY",
            Decimal::try_from(0.5).unwrap(), Decimal::try_from(10.0).unwrap(),
            t_prev - Duration::minutes(5), Some(&format!("txA{stamp}")), Some(&format!("txA{stamp}")),
        ).await.unwrap();

        let prev = monitor::latest_snapshots(&db, platform, &addr).await.unwrap();
        let t_prev_a = prev.iter().map(|s| s.captured_at).max().unwrap();
        let prev_prev = monitor::latest_snapshots_before(&db, platform, &addr, t_prev_a).await.unwrap();
        assert!(!prev.is_empty() && !prev_prev.is_empty(), "两轮快照应就位");
        let t_prev_prev_a = prev_prev.iter().map(|s| s.captured_at).max().unwrap();

        let prev_a = prev.iter().find(|s| s.token_id == "tokA").unwrap();
        let prev_prev_a = prev_prev.iter().find(|s| s.token_id == "tokA").unwrap();
        let delta_a = prev_a.size.to_f64().unwrap_or(0.0) - prev_prev_a.size.to_f64().unwrap_or(0.0);
        let covered_a = raw::sum_signed_trade_size(
            &db, platform, &addr, "tokA", t_prev_prev_a, t_prev_a,
        ).await.unwrap();
        let residual_a = delta_a - covered_a;
        assert!((delta_a - 10.0).abs() < 1e-6, "tokA Δ 应为 10，实际 {delta_a}");
        assert!((covered_a - 10.0).abs() < 1e-6, "tokA covered 应为 10，实际 {covered_a}");
        assert!(residual_a.abs() <= 1e-6, "场景A：trades 全覆盖 → 残差应为 0，实际 {residual_a}");

        // ── 场景 B：tokB，prev_prev=0 → prev=8，无 trades → 残差 8（diff 补漏）──
        monitor::insert_position_snapshot(
            &db, platform, &addr, "tokB", Some("condB"),
            0.0, 0.0, 0.0, 0.0, t_prev_prev,
        ).await.unwrap();
        monitor::insert_position_snapshot(
            &db, platform, &addr, "tokB", Some("condB"),
            8.0, 0.4, 0.4, 0.0, t_prev,
        ).await.unwrap();
        let prev_b = monitor::latest_snapshots(&db, platform, &addr).await.unwrap()
            .into_iter().find(|s| s.token_id == "tokB").expect("tokB prev 快照应存在");
        let prev_prev_b = monitor::latest_snapshots_before(&db, platform, &addr, prev_b.captured_at).await.unwrap()
            .into_iter().find(|s| s.token_id == "tokB").expect("tokB prev_prev 快照应存在");
        let delta_b = prev_b.size.to_f64().unwrap_or(0.0) - prev_prev_b.size.to_f64().unwrap_or(0.0);
        let covered_b = raw::sum_signed_trade_size(
            &db, platform, &addr, "tokB", prev_prev_b.captured_at, prev_b.captured_at,
        ).await.unwrap();
        let residual_b = delta_b - covered_b;
        assert!((delta_b - 8.0).abs() < 1e-6, "tokB Δ 应为 8，实际 {delta_b}");
        assert!((covered_b - 0.0).abs() < 1e-6, "tokB covered 应为 0，实际 {covered_b}");
        assert!((residual_b - 8.0).abs() < 1e-6, "场景B：trades 漏 → 残差应 = Δ = 8，实际 {residual_b}");

        // ── 场景 C：tokC，prev_prev=0 → prev=0（往返净 0），trades BUY 12 + SELL 12 ──
        monitor::insert_position_snapshot(
            &db, platform, &addr, "tokC", Some("condC"),
            0.0, 0.0, 0.0, 0.0, t_prev_prev,
        ).await.unwrap();
        monitor::insert_position_snapshot(
            &db, platform, &addr, "tokC", Some("condC"),
            0.0, 0.0, 0.0, 0.0, t_prev,
        ).await.unwrap();
        raw::insert_raw_trade(
            &db, platform, &addr, "tokC", Some("condC"), "BUY",
            Decimal::try_from(0.5).unwrap(), Decimal::try_from(12.0).unwrap(),
            t_prev - Duration::minutes(5), Some(&format!("txC1{stamp}")), Some(&format!("txC1{stamp}")),
        ).await.unwrap();
        raw::insert_raw_trade(
            &db, platform, &addr, "tokC", Some("condC"), "SELL",
            Decimal::try_from(0.5).unwrap(), Decimal::try_from(12.0).unwrap(),
            t_prev - Duration::minutes(2), Some(&format!("txC2{stamp}")), Some(&format!("txC2{stamp}")),
        ).await.unwrap();
        let prev_c = monitor::latest_snapshots(&db, platform, &addr).await.unwrap()
            .into_iter().find(|s| s.token_id == "tokC").unwrap();
        let prev_prev_c = monitor::latest_snapshots_before(&db, platform, &addr, prev_c.captured_at).await.unwrap()
            .into_iter().find(|s| s.token_id == "tokC").unwrap();
        let delta_c = prev_c.size.to_f64().unwrap_or(0.0) - prev_prev_c.size.to_f64().unwrap_or(0.0);
        let covered_c = raw::sum_signed_trade_size(
            &db, platform, &addr, "tokC", prev_prev_c.captured_at, prev_c.captured_at,
        ).await.unwrap();
        let residual_c = delta_c - covered_c;
        assert!(delta_c.abs() < 1e-6, "tokC Δ 应为 0（往返净 0），实际 {delta_c}");
        assert!(covered_c.abs() < 1e-6, "tokC covered 应为 0（BUY+SELL 抵消），实际 {covered_c}");
        assert!(residual_c.abs() <= 1e-6, "场景C：往返净 0 → 不误发信号，残差 {residual_c}");

        // ── latest_trade_ts：返回该地址 raw_trades 的 max(ts) ──
        let latest = raw::latest_trade_ts(&db, platform, &addr).await.unwrap();
        assert!(latest.is_some(), "latest_trade_ts 应非空");
        // tokC 的 SELL 是最晚一笔（t_prev-2min，落在闭合窗口内），应即 max。
        // 容差 1s：PG timestamptz 微秒精度，Rust DateTime 纳秒精度，直接 >= 会因截断少几纳秒误判。
        let expected_max = t_prev - Duration::minutes(2);
        let diff = latest.unwrap().signed_duration_since(expected_max).abs();
        assert!(diff < Duration::seconds(1), "latest_trade_ts 应 ≈ t_prev-2min，实际差 {diff:?}");

        // 清理本轮测试数据（按地址），避免污染后续运行。
        sqlx::query("DELETE FROM trader_hub.raw_trades WHERE address = $1").bind(&addr).execute(&db).await.unwrap();
        sqlx::query("DELETE FROM trader_hub.trader_positions_snapshot WHERE address = $1").bind(&addr).execute(&db).await.unwrap();
    }
}
