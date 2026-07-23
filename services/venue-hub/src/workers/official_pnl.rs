//! official_pnl worker — 抓官方盈亏写回 `trader_performance.official_pnl`。
//!
//! 对应 `docs/PERFORMANCE_PIPELINE.md` / `docs/SHADOW_MODE.md`。
//!
//! 每个 tick 两阶段：
//!
//! **A. 排行榜（优先）**  
//!   对每个 signal_source Venue（当前仅 Polymarket 提供榜上 `pnl`）：
//!   1. 按官方周期（DAY/WEEK/MONTH/ALL）分页拉 `/v1/leaderboard?orderBy=PNL`
//!   2. 命中已跟踪地址 → `upsert_official_pnl(..., overwrite=true)`，source=`polymarket_leaderboard`
//!
//! **B. `/value` delta 兜底（非榜地址）**  
//!   Polymarket `/value` 只返当前估值快照（无历史）。worker 周期快照到
//!   `trader_value_snapshot`，积累足够历史后算  
//!   `delta = latest.value - baseline(≤cutoff).value`，作为官方口径近似
//!   （含出入金；前端副标明示）。仅当尚无排行榜来源时写入，source=`polymarket_value_delta`。
//!   短窗口（coverage 不足）跳过写入，避免把几天快照当成「1m」。
//!
//! 周期映射（与 `polymarket::map_time_period` 对齐）：
//!   - `1d`→DAY / `1w`→WEEK / `1m`→MONTH
//!   - `1y`/`ytd`/`all`→ALL（同一 ALL 值写入三个 period 行；delta 各自按 cutoff）
//!
//! 官方 `pnl`（排行榜）/ `value` delta 与 sharpside 自算 `realized_pnl` 并存；
//! 前端「盈亏」主显 `official_pnl`。偏差由 shadow mode `metric_audit` 审计。

use crate::registry::enabled_signal_sources;
use crate::state::AppState;
use chrono::{DateTime, Datelike, Utc};
use sharpside_db::queries::perf as perf_q;
use sharpside_db::queries::traders as trader_q;
use sharpside_venues_core::{LeaderboardQuery, VenueCapabilities};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// 单周期分页拉取上限（10 页 × 50 = 500 名）。Top N 之外靠 `/value` delta 兜底。
const PAGE_SIZE: u32 = 50;
const MAX_PAGES: u32 = 10;
/// 分类榜页数（每分类×周期）；低于 OVERALL，控制 API 量。
const CATEGORY_MAX_PAGES: u32 = 4;

/// 官方周期 → 写入的 sharpside period 键。
/// Polymarket 排行榜只暴露 DAY/WEEK/MONTH/ALL 四档；ALL 同时覆盖 1y/ytd/all。
const PERIOD_MAP: &[(&str, &[&str])] = &[
    ("1d", &["1d"]),
    ("1w", &["1w"]),
    ("1m", &["1m"]),
    ("1y", &["1y", "ytd", "all"]),
];

/// Polymarket 官方分类（不含 OVERALL）。与 ingest `POLYMARKET_INGEST_CATEGORIES` 对齐。
const POLYMARKET_CATEGORIES: &[&str] = &[
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

/// 各 period 写入 value_delta 的最低覆盖率（短于此时不写，避免假「整周期」）。
fn min_coverage(period: &str) -> f64 {
    match period {
        "1d" => 0.30,
        "1w" | "1m" => 0.50,
        "1y" | "ytd" => 0.30,
        // all：任意跨度可写（从首条快照起）
        _ => 0.0,
    }
}

/// 各 period 的 delta 回看窗口。`all` 用极大窗口 ≈ 从首条快照起。
fn period_cutoff(period: &str, now: DateTime<Utc>) -> DateTime<Utc> {
    match period {
        "1d" => now - chrono::Duration::days(1),
        "1w" => now - chrono::Duration::days(7),
        "1m" => now - chrono::Duration::days(30),
        "1y" => now - chrono::Duration::days(365),
        "ytd" => {
            let y = now.year();
            chrono::NaiveDate::from_ymd_opt(y, 1, 1)
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|n| n.and_utc())
                .unwrap_or_else(|| now - chrono::Duration::days(365))
        }
        // all：足够早，value_delta_since 取 cutoff 前最近 / 首条快照
        _ => now - chrono::Duration::days(3650),
    }
}

pub async fn run(state: AppState) {
    let interval = state.config.workers.official_pnl_secs.max(1);
    let mut ticker = tokio::time::interval(Duration::from_secs(interval));
    loop {
        ticker.tick().await;
        if let Err(e) = run_once(&state).await {
            tracing::warn!(error = %e, "official_pnl 本轮失败");
        }
    }
}

async fn run_once(state: &AppState) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let now = Utc::now();

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

        // 该平台全部 visible 地址（无 5000 硬上限）。
        let tracked_addrs: HashSet<String> =
            trader_q::list_visible_addresses(&state.db, platform.as_str())
                .await?
                .into_iter()
                .map(|a| a.to_lowercase())
                .collect();
        if tracked_addrs.is_empty() {
            continue;
        }

        // ── A. 排行榜 ──
        // period → 本轮命中地址（小写），供 B 阶段跳过已有榜上数据的地址。
        // OVERALL：写 official_pnl（不碰自算 realized_pnl）。
        // 各分类：写分类绩效种子（realized_pnl/roi 展示层），否则前端点分类 → 0 人。
        let mut on_board: HashMap<String, HashSet<String>> = HashMap::new();

        // (category_opt, max_pages)：None = OVERALL。
        let mut boards: Vec<(Option<&str>, u32)> = vec![(None, MAX_PAGES)];
        if platform == sharpside_shared::Platform::Polymarket {
            for cat in POLYMARKET_CATEGORIES {
                boards.push((Some(*cat), CATEGORY_MAX_PAGES));
            }
        }

        for (cat_opt, max_pages) in boards {
            for (tp, period_keys) in PERIOD_MAP {
                let mut map: HashMap<String, (Option<f64>, Option<f64>)> = HashMap::new();
                for page in 0..max_pages {
                    let offset = page * PAGE_SIZE;
                    let q = LeaderboardQuery {
                        category: cat_opt.map(|s| s.to_string()),
                        time_period: (*tp).to_string(),
                        order_by: "pnl".into(),
                        limit: PAGE_SIZE,
                        offset,
                    };
                    let entries = match venue.leaderboard(q).await {
                        Ok(e) => e,
                        Err(e) => {
                            tracing::warn!(
                                platform = platform.as_str(),
                                category = cat_opt.unwrap_or("OVERALL"),
                                period = tp,
                                page,
                                error = %e,
                                "official_pnl 拉排行榜失败"
                            );
                            break;
                        }
                    };
                    if entries.is_empty() {
                        break;
                    }
                    let page_len = entries.len();
                    for e in entries {
                        // 仅当该 Venue 提供官方 pnl（Polymarket 有；Kalshi 等无则跳过）。
                        if e.seed_pnl.is_none() && e.seed_vol.is_none() {
                            continue;
                        }
                        map.insert(
                            e.venue_trader_id.to_lowercase(),
                            (e.seed_pnl, e.seed_vol),
                        );
                    }
                    if (page_len as u32) < PAGE_SIZE {
                        break;
                    }
                }

                let mut written = 0usize;
                let mut board_set = HashSet::new();
                for (addr_lower, (pnl, vol)) in &map {
                    // 仅已跟踪地址：排行榜 FROM traders JOIN performance，未入库无展示意义。
                    if !tracked_addrs.contains(addr_lower) {
                        continue;
                    }
                    for pk in *period_keys {
                        let res = if let Some(cat) = cat_opt {
                            perf_q::upsert_category_leaderboard_seed(
                                &state.db,
                                platform.as_str(),
                                addr_lower,
                                pk,
                                cat,
                                *pnl,
                                *vol,
                                "polymarket_leaderboard",
                            )
                            .await
                        } else {
                            perf_q::upsert_official_pnl(
                                &state.db,
                                platform.as_str(),
                                addr_lower,
                                pk,
                                *pnl,
                                *vol,
                                "polymarket_leaderboard",
                                true, // 排行榜始终覆盖
                            )
                            .await
                        };
                        if let Err(e) = res {
                            tracing::warn!(
                                platform = platform.as_str(),
                                address = addr_lower,
                                category = cat_opt.unwrap_or("OVERALL"),
                                period = pk,
                                error = %e,
                                "official_pnl 写回失败"
                            );
                        } else {
                            written += 1;
                            board_set.insert(addr_lower.clone());
                        }
                    }
                }
                // on_board 仅跟踪 OVERALL（B 阶段 value_delta 跳过口径）。
                if cat_opt.is_none() {
                    for pk in *period_keys {
                        on_board.insert((*pk).to_string(), board_set.clone());
                    }
                }
                tracing::info!(
                    platform = platform.as_str(),
                    category = cat_opt.unwrap_or("OVERALL"),
                    period = tp,
                    fetched = map.len(),
                    written,
                    "official_pnl 排行榜本轮完成"
                );
            }
        }

        // ── B. `/value` 快照 + delta 兜底 ──
        // 1) 拉一批该刷新快照的地址（从未快照或距上次 ≥ interval；优先缺官方/热钥）。
        let min_age = now - chrono::Duration::seconds(state.config.workers.official_pnl_secs as i64);
        let batch = state.config.workers.official_value_batch.max(1);
        let candidates = match perf_q::pick_value_snapshot_candidates(
            &state.db,
            platform.as_str(),
            min_age,
            batch,
        )
        .await
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    platform = platform.as_str(),
                    error = %e,
                    "official_pnl 选 /value 候选失败"
                );
                Vec::new()
            }
        };

        let mut snap_ok = 0usize;
        for addr in &candidates {
            match venue.portfolio_value(addr).await {
                Ok(v) => {
                    if let Err(e) =
                        perf_q::insert_value_snapshot(&state.db, platform.as_str(), addr, now, v)
                            .await
                    {
                        tracing::warn!(
                            platform = platform.as_str(),
                            address = %addr,
                            error = %e,
                            "official_pnl 写 /value 快照失败"
                        );
                    } else {
                        snap_ok += 1;
                    }
                }
                Err(sharpside_venues_core::VenueError::Unsupported(_)) => {
                    // Venue 不支持 /value，整阶段跳过。
                    break;
                }
                Err(e) => {
                    tracing::warn!(
                        platform = platform.as_str(),
                        address = %addr,
                        error = %e,
                        "official_pnl 拉 /value 失败"
                    );
                }
            }
        }

        // 2) 对未上榜的已跟踪地址，按周期算 delta 并 fallback 写回。
        //    需要至少两个不同时点的快照 + 足够覆盖率；刚启动几轮可能写不出，属预期。
        const ALL_PERIODS: &[&str] = &["1d", "1w", "1m", "1y", "ytd", "all"];
        let mut delta_written = 0usize;
        let mut delta_skipped_short = 0usize;
        let mut delta_skipped_none = 0usize;
        for addr in &tracked_addrs {
            for pk in ALL_PERIODS {
                // 本轮该 period 已有排行榜数据 → 跳过（排行榜优先）。
                if on_board
                    .get(*pk)
                    .map(|s| s.contains(addr))
                    .unwrap_or(false)
                {
                    continue;
                }
                let since = period_cutoff(pk, now);
                let delta = match perf_q::value_delta_since(
                    &state.db,
                    platform.as_str(),
                    addr,
                    since,
                    now,
                )
                .await
                {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::warn!(
                            platform = platform.as_str(),
                            address = %addr,
                            period = pk,
                            error = %e,
                            "official_pnl 算 value delta 失败"
                        );
                        continue;
                    }
                };
                let Some(vd) = delta else {
                    delta_skipped_none += 1;
                    continue;
                };
                if vd.coverage < min_coverage(pk) {
                    delta_skipped_short += 1;
                    continue;
                }
                if let Err(e) = perf_q::upsert_official_pnl(
                    &state.db,
                    platform.as_str(),
                    addr,
                    pk,
                    Some(vd.delta),
                    None, // /value 无 vol
                    "polymarket_value_delta",
                    false, // 不覆盖排行榜来源
                )
                .await
                {
                    tracing::warn!(
                        platform = platform.as_str(),
                        address = %addr,
                        period = pk,
                        error = %e,
                        "official_pnl value_delta 写回失败"
                    );
                } else {
                    delta_written += 1;
                }
            }
        }

        tracing::info!(
            platform = platform.as_str(),
            tracked = tracked_addrs.len(),
            candidates = candidates.len(),
            snap_ok,
            delta_written,
            delta_skipped_short,
            delta_skipped_none,
            "official_pnl /value 兜底本轮完成"
        );
    }
    Ok(())
}
