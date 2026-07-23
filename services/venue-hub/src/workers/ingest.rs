//! ingest worker — 各 Venue 采集，写 raw_*。对应 `docs/ARCHITECTURE.md` §6.1。
//!
//! 每个 tick：对每个已注册 signal_source Venue，
//!   1) 拉 leaderboard → upsert traders（source=leaderboard）
//!   2) 拉 markets → upsert raw_markets 缓存（供 mapping worker 与 `/markets` 用）
//!
//! 该 Venue 信号暂停时其他 Venue 不受影响（逐 Venue try/日志）。

use crate::registry::enabled_signal_sources;
use crate::routes::markets::cache_markets;
use crate::routes::traders::ingest_leaderboard;
use crate::state::AppState;
use sharpside_venues_core::{MarketQuery, VenueCapabilities};
use std::time::Duration;

/// Polymarket 排行榜官方分类：ingest 时逐分类拉榜，seed 各分类下活跃交易者。
/// 与 Data API `/v1/leaderboard` category 枚举 + `category_mapping` 种子一致；
/// OVERALL 已含在 None 路径之外，这里补非 OVERALL 分类。
const POLYMARKET_INGEST_CATEGORIES: &[&str] = &[
    "POLITICS",
    "SPORTS",
    "ESPORTS",
    "CRYPTO",
    "CULTURE",
    "MENTIONS",
    "WEATHER",
    "ECONOMICS",
    "TECH",
    "FINANCE",
];

pub async fn run(state: AppState) {
    let interval = state.config.workers.ingest_secs.max(1);
    let mut ticker = tokio::time::interval(Duration::from_secs(interval));
    loop {
        ticker.tick().await;
        for platform in enabled_signal_sources(&state.config.venues) {
            if let Some(venue) = state.registry.get(platform) {
                if !venue
                    .info()
                    .capabilities
                    .contains(VenueCapabilities::SIGNAL_SOURCE)
                {
                    continue;
                }
                // 1) leaderboard → upsert traders
                //    Polymarket：按官方分类 × 常用周期拉榜，种子分类绩效（否则点分类 → 0 人）；
                //    绩效切片仍由 perf worker 在 raw_markets.category 就绪后覆盖。
                //    其余 venue：仅 OVERALL（category=None）。
                if platform == sharpside_shared::Platform::Polymarket {
                    // 默认排行榜 period=1m；all 覆盖「全部」周期。1d/1w 由 official_pnl 补。
                    for cat in POLYMARKET_INGEST_CATEGORIES {
                        for period in ["1m", "all"] {
                            match ingest_leaderboard(&state, platform, Some(cat), period).await {
                                Ok(n) => tracing::info!(
                                    platform = platform.as_str(),
                                    category = cat,
                                    period,
                                    traders = n,
                                    "ingest leaderboard"
                                ),
                                Err(e) => tracing::warn!(
                                    platform = platform.as_str(),
                                    category = cat,
                                    period,
                                    error = %e,
                                    "ingest leaderboard 失败"
                                ),
                            }
                        }
                    }
                } else {
                    match ingest_leaderboard(&state, platform, None, "all").await {
                        Ok(n) => tracing::info!(
                            platform = platform.as_str(),
                            traders = n,
                            "ingest leaderboard"
                        ),
                        Err(e) => {
                            tracing::warn!(platform = platform.as_str(), error = %e, "ingest leaderboard 失败")
                        }
                    }
                }
                // 2) markets → upsert raw_markets
                let mq = MarketQuery {
                    q: None,
                    tag: None,
                    limit: 200,
                };
                match venue.markets(mq).await {
                    Ok(markets) => {
                        cache_markets(&state, platform, &markets).await;
                        tracing::info!(
                            platform = platform.as_str(),
                            markets = markets.len(),
                            "ingest markets"
                        );
                    }
                    Err(sharpside_venues_core::VenueError::Unsupported(_)) => {}
                    Err(e) => {
                        tracing::warn!(platform = platform.as_str(), error = %e, "ingest markets 失败")
                    }
                }
            }
        }
        state.worker_ticks.touch_ingest();
    }
}
