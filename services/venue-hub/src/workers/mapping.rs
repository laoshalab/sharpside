//! mapping worker — 启发式候选 + 入审核队列。对应 `docs/ARCHITECTURE.md` §6.1 / §8.1。
//!
//! 每个 tick：对每对已注册 signal_source Venue `(A, B)`，
//!   1) 读各自 raw_markets 缓存（ingest 已抓）
//!   2) 调 `sharpside_mapping::candidate_mappings` 产候选
//!   3) `insert_candidate`（`ON CONFLICT DO NOTHING`，不覆盖人工已校对）
//!
//! 不阻塞跟单：无 verified 映射则 Copier 跳过该 copy_order。

use crate::registry::enabled_signal_sources;
use crate::state::AppState;
use sharpside_db::queries::mappings as mapping_q;
use sharpside_db::queries::raw;
use sharpside_venues_core::Platform as VenuePlatform;
use sharpside_venues_core::{Market, VenueCapabilities};
use std::time::Duration;

/// `raw_markets` 行 → 通用 `Market`（mapping crate 输入）。
fn to_market(m: &sharpside_db::RawMarket) -> Market {
    Market {
        platform: m
            .platform
            .parse::<VenuePlatform>()
            .unwrap_or(VenuePlatform::Polymarket),
        venue_market_id: m.venue_market_id.clone(),
        title: m.title.clone(),
        slug: m.slug.clone(),
        tags: m.tags.iter().flatten().cloned().collect(),
        category: m.category.clone(),
        end_date: m.end_date,
        outcome_yes: m.outcome_yes.and_then(|d| d.to_string().parse().ok()),
        outcome_no: m.outcome_no.and_then(|d| d.to_string().parse().ok()),
        closed: Some(m.closed),
    }
}

pub async fn run(state: AppState) {
    let interval = state.config.workers.mapping_secs.max(1);
    let mut ticker = tokio::time::interval(Duration::from_secs(interval));
    loop {
        ticker.tick().await;
        let platforms = enabled_signal_sources(&state.config.venues);
        // 预取各 platform 的 markets（缓存）
        let mut cached: std::collections::HashMap<VenuePlatform, Vec<Market>> =
            std::collections::HashMap::new();
        for p in &platforms {
            if let Some(venue) = state.registry.get(*p) {
                if !venue
                    .info()
                    .capabilities
                    .contains(VenueCapabilities::SIGNAL_SOURCE)
                {
                    continue;
                }
                match raw::list_raw_markets(&state.db, p.as_str()).await {
                    Ok(rows) => {
                        cached.insert(*p, rows.iter().map(to_market).collect());
                    }
                    Err(e) => {
                        tracing::warn!(platform = p.as_str(), error = %e, "mapping 读 raw_markets 失败")
                    }
                }
            }
        }
        // 对每对 (A, B) 产候选
        for (i, a) in platforms.iter().enumerate() {
            for b in platforms.iter().skip(i + 1) {
                let (Some(ma), Some(mb)) = (cached.get(a), cached.get(b)) else {
                    continue;
                };
                let candidates = sharpside_mapping::candidate_mappings(
                    ma,
                    mb,
                    state.config.auto_match_threshold,
                );
                for c in &candidates {
                    if let (Err(_), Err(_)) = (
                        c.from.platform.as_str().parse::<VenuePlatform>(),
                        c.to.platform.as_str().parse::<VenuePlatform>(),
                    ) {
                        continue;
                    }
                    let _ = mapping_q::insert_candidate(
                        &state.db,
                        c.from.platform.as_str(),
                        &c.from.venue_market_id,
                        c.to.platform.as_str(),
                        &c.to.venue_market_id,
                        c.confidence,
                    )
                    .await;
                }
                if !candidates.is_empty() {
                    tracing::info!(
                        from = a.as_str(),
                        to = b.as_str(),
                        candidates = candidates.len(),
                        "mapping 产候选"
                    );
                }
            }
        }
    }
}
