//! perf worker — 仓位重建 + 绩效物化。对应 `docs/ARCHITECTURE.md` §6.1 / `docs/PERFORMANCE_PIPELINE.md`。
//!
//! 每个 tick：对每个可见 trader，
//!   1) 读 raw_trades → `perf::TradeInput`
//!   2) 按周期（1d/1w/1m/1y/ytd/all）过滤 trades → `reconstruct_position_timeline`
//!   3) `compute_equity_curve`（marks 暂空，仅 realized PnL；mark-to-market 待 raw_prices 接入）
//!   4) `compute_performance`（current_price 取该 token 最后一笔成交价作为 mark）
//!   5) 覆盖写 `position_timeline` / `trader_equity_curve` / `trader_performance` / `trader_tag`
//!
//! 绩效延迟可接受：展示读旧快照，下轮覆盖。

use crate::state::AppState;
use chrono::{DateTime, Utc};
use sharpside_db::queries::ops;
use sharpside_db::queries::perf::TimelineRow;
use sharpside_db::queries::{perf as perf_q, raw, traders as trader_q};
use sharpside_perf::types::{TagThresholds, TradeInput};
use sharpside_shared::{PerformancePeriod, Side};
use std::collections::HashMap;
use std::time::Duration;

/// `raw_trades` 行 → `perf::TradeInput`。
fn to_trade_input(t: &sharpside_db::RawTrade) -> Option<TradeInput> {
    let platform = t.platform.parse().ok()?;
    let side = match t.side.as_str() {
        "BUY" => Side::Buy,
        "SELL" => Side::Sell,
        _ => return None,
    };
    Some(TradeInput {
        platform,
        address: t.address.clone(),
        token_id: t.token_id.clone(),
        condition_id: t.condition_id.clone(),
        side,
        price: t.price.to_string().parse().unwrap_or(0.0),
        size: t.size.to_string().parse().unwrap_or(0.0),
        ts: t.ts,
    })
}

/// 中位数（holding_seconds 用）。
fn median_i64(xs: &[i64]) -> Option<i64> {
    if xs.is_empty() {
        return None;
    }
    let mut v = xs.to_vec();
    v.sort_unstable();
    Some(v[v.len() / 2])
}

/// 从已加载的 raw_trades + position_timeline + 绩效聚合 [`sharpside_botfilter::AggregatedStats`]。
///
/// 纯内存计算，无额外 DB 往返（perf worker 已加载全部数据）。字段口径见
/// `crates/botfilter/src/lib.rs` `AggregatedStats` 文档：
/// - `self_trade_count`：同 `(tx_hash, token_id)` 同时出现 BUY/SELL 的组数（wash proxy）。
/// - `round_trips`：已平仓且 `holding_seconds ≤ 3600`（1h）的配对数。
/// - `median_hold_secs`：全部配对 hold 中位；无配对 → -1（未知，走弱命中）。
/// - `large_trade_count`：单笔 notional = `size * price ≥ large_notional`（USDC）。
fn aggregate_bot_stats(
    raw_rows: &[sharpside_db::RawTrade],
    timelines: &[sharpside_perf::PositionTimeline],
    wins: i64,
    losses: i64,
    large_notional: f64,
) -> sharpside_botfilter::AggregatedStats {
    use std::collections::{HashMap, HashSet};

    let n_trades = raw_rows.len() as u64;
    let n_buys = raw_rows.iter().filter(|t| t.side == "BUY").count() as u64;
    let n_sells = raw_rows.iter().filter(|t| t.side == "SELL").count() as u64;
    let symmetric_ratio = if n_trades > 0 {
        1.0 - ((n_buys as f64 - n_sells as f64).abs() / n_trades as f64)
    } else {
        0.0
    };

    // self_trade_count：同 (tx_hash, token_id) 同时出现 BUY 和 SELL 的组数。
    let mut tx_token_sides: HashMap<(String, String), HashSet<String>> = HashMap::new();
    for t in raw_rows {
        if let Some(tx) = &t.tx_hash {
            tx_token_sides
                .entry((tx.clone(), t.token_id.clone()))
                .or_default()
                .insert(t.side.clone());
        }
    }
    let self_trade_count = tx_token_sides
        .values()
        .filter(|s| s.contains("BUY") && s.contains("SELL"))
        .count() as u64;

    // round_trips：已平仓且 holding_seconds ≤ 1h 的配对数。
    let round_trips = timelines
        .iter()
        .filter(|t| t.is_closed)
        .filter(|t| t.holding_seconds.map(|h| h <= 3600).unwrap_or(false))
        .count() as u64;

    // median_hold_secs：全部配对 hold 的中位；无配对 → -1（未知）。
    let holds: Vec<i64> = timelines.iter().filter_map(|t| t.holding_seconds).collect();
    let median_hold_secs = median_i64(&holds).unwrap_or(-1);

    // unique_conditions：distinct condition_id（非空）。
    let unique_conditions = raw_rows
        .iter()
        .filter_map(|t| t.condition_id.clone())
        .filter(|c| !c.is_empty())
        .collect::<HashSet<_>>()
        .len() as u64;

    // large_trade_count：单笔 notional = size * price ≥ large_notional。
    let large_trade_count = raw_rows
        .iter()
        .filter(|t| {
            let size: f64 = t.size.to_string().parse().unwrap_or(0.0);
            let price: f64 = t.price.to_string().parse().unwrap_or(0.0);
            size * price >= large_notional
        })
        .count() as u64;

    let n_resolved = (wins + losses).max(0) as u64;
    let n_resolved_wins = wins.max(0) as u64;

    sharpside_botfilter::AggregatedStats {
        n_trades,
        n_buys,
        n_sells,
        symmetric_ratio,
        self_trade_count,
        round_trips,
        median_hold_secs,
        unique_conditions,
        large_trade_count,
        n_resolved,
        n_resolved_wins,
    }
}

/// 从 `tag_rules` 表读 `rule_id='botfilter'` 行，反序列化为 [`sharpside_botfilter::BotFilterConfig`]。
///
/// 回退策略（任一即用默认）：
/// - 行不存在（首次部署或被删）→ default()
/// - `enabled=false`（运营临时关闭 botfilter）→ default()（注：default 仍会跑规则，只是阈值回到默认；
///   若要完全关闭需把 `bot_threshold` 调到 1.0 以上）
/// - `params` 反序列化失败（运营改坏 JSON）→ default()，并 warn 日志
///
/// 对应 `docs/BOTFILTER_RULES.md` §5（阈值可调可审计）。
async fn load_bot_filter_config(state: &AppState) -> sharpside_botfilter::BotFilterConfig {
    match ops::get_tag_rule(&state.db, "botfilter").await {
        Ok(Some(rule)) if rule.enabled => serde_json::from_value(rule.params).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "botfilter tag_rules params 解析失败，回退默认阈值");
            sharpside_botfilter::BotFilterConfig::default()
        }),
        Ok(Some(_)) => {
            // enabled=false：运营显式关闭 → 用默认（仍跑规则但默认阈值）。
            sharpside_botfilter::BotFilterConfig::default()
        }
        Ok(None) => sharpside_botfilter::BotFilterConfig::default(),
        Err(e) => {
            tracing::warn!(error = %e, "读 botfilter tag_rules 失败，回退默认阈值");
            sharpside_botfilter::BotFilterConfig::default()
        }
    }
}

/// 周期截止时间。`all` 不截；`ytd` 截到当年 1 月 1 日。
fn cutoff(period: PerformancePeriod, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    match period {
        PerformancePeriod::OneDay => Some(now - chrono::Duration::days(1)),
        PerformancePeriod::OneWeek => Some(now - chrono::Duration::days(7)),
        PerformancePeriod::OneMonth => Some(now - chrono::Duration::days(30)),
        PerformancePeriod::OneYear => Some(now - chrono::Duration::days(365)),
        PerformancePeriod::Ytd => {
            let year = now.format("%Y").to_string().parse::<i32>().ok()?;
            let jan1 = chrono::NaiveDate::from_ymd_opt(year, 1, 1)?
                .and_hms_opt(0, 0, 0)?
                .and_utc();
            Some(jan1)
        }
        PerformancePeriod::All => None,
    }
}

fn period_str(period: PerformancePeriod) -> &'static str {
    period.as_str()
}

async fn compute_one_trader(
    state: &AppState,
    platform: &str,
    address: &str,
    now: DateTime<Utc>,
    bot_cfg: &sharpside_botfilter::BotFilterConfig,
) -> Result<(), sharpside_db::DbError> {
    let raw_rows = raw::list_raw_trades_for_trader(&state.db, platform, address).await?;
    let all_trades: Vec<TradeInput> = raw_rows.iter().filter_map(to_trade_input).collect();
    if all_trades.is_empty() {
        return Ok(());
    }

    // current_price：每个 token 取最后一笔成交价作为 mark（MVP；后续接 raw_prices / book）。
    let mut current_prices: HashMap<String, f64> = HashMap::new();
    for t in &all_trades {
        current_prices.insert(t.token_id.clone(), t.price);
    }

    // 小时级权益曲线（全历史，供前端展示 + 按 period 切片）：只算一次、覆盖写一次。
    let all_timelines = sharpside_perf::reconstruct_position_timeline(&all_trades, &[]);
    if !all_timelines.is_empty() {
        let hour = chrono::Duration::hours(1);
        let start_h = all_trades.iter().map(|t| t.ts).min().unwrap_or(now);
        let end_h = now;
        let hourly =
            sharpside_perf::compute_equity_curve(&all_timelines, &[], start_h, end_h, hour);
        let equity_rows: Vec<(chrono::DateTime<chrono::Utc>, f64, f64, f64)> = hourly
            .iter()
            .map(|p| (p.ts, p.equity, p.daily_pnl, p.drawdown_pct))
            .collect();
        let _ = perf_q::replace_equity_curve(&state.db, platform, address, &equity_rows).await;
    }

    // 按 (period × category) 切片重算绩效。
    //   - OVERALL = 全部成交（兼容旧行为）；
    //   - 其余 = 该 trader 成交涉及的站内分类（condition_id → raw_markets.category 归一）。
    // position_timeline / equity_curve / tag 仍按 OVERALL 物化（当前持仓/曲线视图不切分类）。
    let condition_ids: Vec<String> = all_trades
        .iter()
        .filter_map(|t| t.condition_id.clone())
        .collect();
    let cat_map = raw::map_market_categories(&state.db, platform, &condition_ids)
        .await
        .unwrap_or_default();
    let categories: Vec<String> = {
        let mut set = std::collections::BTreeSet::new();
        for t in &all_trades {
            if let Some(cid) = &t.condition_id {
                if let Some(Some(c)) = cat_map.get(cid) {
                    if !c.is_empty() {
                        set.insert(c.clone());
                    }
                }
            }
        }
        set.into_iter().collect()
    };

    for period in [
        PerformancePeriod::OneDay,
        PerformancePeriod::OneWeek,
        PerformancePeriod::OneMonth,
        PerformancePeriod::OneYear,
        PerformancePeriod::Ytd,
        PerformancePeriod::All,
    ] {
        let trades: Vec<TradeInput> = match cutoff(period, now) {
            Some(c) => all_trades.iter().filter(|t| t.ts >= c).cloned().collect(),
            None => all_trades.clone(),
        };
        if trades.is_empty() {
            continue;
        }
        let timelines = sharpside_perf::reconstruct_position_timeline(&trades, &[]);
        if timelines.is_empty() {
            continue;
        }

        // 日级权益曲线（仅用于算 Sharpe/回撤的 daily_pnls 序列；按日年化口径不变）
        let start = trades.iter().map(|t| t.ts).min().unwrap_or(now);
        let end = now;
        let daily_curve = sharpside_perf::compute_equity_curve(
            &timelines,
            &[],
            start,
            end,
            chrono::Duration::days(1),
        );
        let daily_pnls = sharpside_perf::daily_pnls(&daily_curve);

        // 绩效指标（OVERALL）
        let perf =
            sharpside_perf::compute_performance(&timelines, &current_prices, &daily_pnls, period);

        // 落库：position_timeline（OVERALL，整组替换）
        let timeline_rows: Vec<TimelineRow> = timelines
            .iter()
            .map(|t| TimelineRow {
                token_id: t.token_id.clone(),
                condition_id: t.condition_id.clone(),
                opened_at: t.opened_at,
                closed_at: t.closed_at,
                total_bought_size: t.total_bought_size,
                total_sold_size: t.total_sold_size,
                avg_cost: t.avg_cost,
                realized_pnl: t.realized_pnl,
                final_open_size: t.final_open_size,
                is_closed: t.is_closed,
                holding_seconds: t.holding_seconds,
            })
            .collect();
        let _ =
            perf_q::replace_position_timelines(&state.db, platform, address, &timeline_rows).await;

        // 落库：trader_performance（OVERALL）
        let _ = perf_q::upsert_trader_performance(
            &state.db,
            platform,
            address,
            period_str(period),
            "OVERALL",
            perf.roi,
            perf.sharpe,
            perf.sortino,
            perf.win_rate,
            perf.max_drawdown,
            perf.realized_pnl,
            perf.unrealized_pnl,
            perf.gross_profit,
            perf.gross_loss,
            perf.profit_factor,
            perf.wins.try_into().unwrap_or(0),
            perf.losses.try_into().unwrap_or(0),
            perf.position_count.try_into().unwrap_or(0),
            perf.open_positions.try_into().unwrap_or(0),
            perf.total_volume,
            perf.cost_basis,
        )
        .await;

        // 标签（仅 all 周期写一次，避免 1d/1w/... 覆盖）
        if matches!(period, PerformancePeriod::All) {
            let holdings: Vec<i64> = timelines.iter().filter_map(|t| t.holding_seconds).collect();
            let med = median_i64(&holdings);
            let tags = sharpside_perf::compute_tags(
                &perf,
                med,
                &sharpside_perf::types::FillStats::default(),
                &TagThresholds::default(),
            );

            // botfilter：从已加载的 raw_trades + timelines 聚合输入，跑 6 条规则。
            // 阈值来自 tag_rules 表（rule_id='botfilter'），由 run() 每 tick 加载一次传入。
            let bot_stats = aggregate_bot_stats(
                &raw_rows,
                &timelines,
                perf.wins,
                perf.losses,
                bot_cfg.sc_large_notional,
            );
            let bot_flags = sharpside_botfilter::detect_with(&bot_stats, bot_cfg);

            // 合并标签：style tags + bot:* 命中规则 + 顶层 bot（is_bot 时）。
            let mut tag_names: Vec<String> =
                tags.iter().map(|t| t.kind.as_str().to_string()).collect();
            for h in &bot_flags.hit_rules {
                tag_names.push(format!("bot:{}", h.rule.as_snake_str()));
            }
            if bot_flags.is_bot {
                tag_names.push("bot".into());
            }
            // tag_attrs：style 证据（原数组）+ bot 判定（is_bot/confidence/hit_rules）。
            // 原 tag_attrs 是数组，现改为对象——目前无消费方读它，安全重构。
            let attrs = serde_json::json!({
                "style": tags.iter().map(|t| t.attrs.clone().unwrap_or_default()).collect::<Vec<_>>(),
                "bot": &bot_flags,
            });
            let _ =
                perf_q::upsert_trader_tag(&state.db, platform, address, &tag_names, &attrs).await;
        }

        // 落库：trader_performance（per category）
        // 只对该 trader 成交涉及的分类切片；未分类成交只计入 OVERALL。
        for cat in &categories {
            let ct: Vec<TradeInput> = trades
                .iter()
                .filter(|t| {
                    t.condition_id
                        .as_ref()
                        .and_then(|cid| cat_map.get(cid).cloned().flatten())
                        .as_deref()
                        == Some(cat.as_str())
                })
                .cloned()
                .collect();
            if ct.is_empty() {
                continue;
            }
            let tl = sharpside_perf::reconstruct_position_timeline(&ct, &[]);
            if tl.is_empty() {
                continue;
            }
            let cstart = ct.iter().map(|t| t.ts).min().unwrap_or(now);
            let ccurve = sharpside_perf::compute_equity_curve(
                &tl,
                &[],
                cstart,
                end,
                chrono::Duration::days(1),
            );
            let cdpnls = sharpside_perf::daily_pnls(&ccurve);
            let cperf = sharpside_perf::compute_performance(&tl, &current_prices, &cdpnls, period);
            let _ = perf_q::upsert_trader_performance(
                &state.db,
                platform,
                address,
                period_str(period),
                cat,
                cperf.roi,
                cperf.sharpe,
                cperf.sortino,
                cperf.win_rate,
                cperf.max_drawdown,
                cperf.realized_pnl,
                cperf.unrealized_pnl,
                cperf.gross_profit,
                cperf.gross_loss,
                cperf.profit_factor,
                cperf.wins.try_into().unwrap_or(0),
                cperf.losses.try_into().unwrap_or(0),
                cperf.position_count.try_into().unwrap_or(0),
                cperf.open_positions.try_into().unwrap_or(0),
                cperf.total_volume,
                cperf.cost_basis,
            )
            .await;
        }
    }
    Ok(())
}

pub async fn run(state: AppState) {
    let interval = state.config.workers.perf_secs.max(1);
    let mut ticker = tokio::time::interval(Duration::from_secs(interval));
    loop {
        ticker.tick().await;
        let traders = match trader_q::list_all_visible_traders(&state.db, 2000, 0).await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "perf 读 traders 失败");
                continue;
            }
        };
        // 每 tick 加载一次 botfilter 阈值（tag_rules 表，运营可调），避免逐 trader 查 DB。
        let bot_cfg = load_bot_filter_config(&state).await;
        let now = Utc::now();
        let mut done = 0usize;
        for t in &traders {
            if let Err(e) = compute_one_trader(&state, &t.platform, &t.address, now, &bot_cfg).await
            {
                tracing::warn!(platform = %t.platform, address = %t.address, error = %e, "perf 计算失败");
            } else {
                done += 1;
            }
        }
        tracing::info!(traders = done, "perf 本轮物化完成");
    }
}
