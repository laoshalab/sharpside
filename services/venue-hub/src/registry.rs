//! VenueRegistry 构建。对应 `docs/VENUE_DESIGN.md` §4 与 `docs/ARCHITECTURE.md` §6.1。
//!
//! 启动时按 `Config.venues` 开关注入已接入的 Venue adapter。
//! 新增平台 = 新增 `crates/venues/<name>` + 在此注册，主路径零改动。

use crate::config::{Config, VenueToggles};
use sharpside_venues_core::VenueRegistry;
use sharpside_venues_polymarket::{PolymarketClient, PolymarketVenue};
use std::sync::Arc;

/// 按 config 构建 VenueRegistry。未启用的 Venue 不注册。
pub fn build_registry(config: &Config) -> VenueRegistry {
    let mut registry = VenueRegistry::new();
    if config.venues.polymarket {
        let venue = match (
            &config.polymarket_data_api,
            &config.polymarket_gamma_api,
            &config.polymarket_clob_api,
        ) {
            (Some(data), Some(gamma), Some(clob)) => {
                PolymarketVenue::with_client(PolymarketClient::with_urls(data, gamma, clob))
            }
            _ => PolymarketVenue::new(),
        };
        registry.register(Arc::new(venue));
    }
    // Kalshi / Manifold / Zeitgeist / Azuro 待对应 adapter 落地后在此注册。
    registry
}

/// 已注册的 signal_source platform 列表（worker 用，避免对未注册 venue 调用）。
pub fn enabled_signal_sources(toggles: &VenueToggles) -> Vec<sharpside_shared::Platform> {
    use sharpside_shared::Platform;
    let mut out = Vec::new();
    if toggles.polymarket {
        out.push(Platform::Polymarket);
    }
    if toggles.kalshi {
        out.push(Platform::Kalshi);
    }
    if toggles.manifold {
        out.push(Platform::Manifold);
    }
    if toggles.zeitgeist {
        out.push(Platform::Zeitgeist);
    }
    if toggles.azuro {
        out.push(Platform::Azuro);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_includes_enabled_venues_only() {
        let config = Config {
            listen_addr: String::new(),
            database_url: String::new(),
            db_max_connections: 1,
            venues: VenueToggles {
                polymarket: true,
                kalshi: false,
                manifold: false,
                zeitgeist: false,
                azuro: false,
            },
            workers: crate::config::WorkerIntervals {
                ingest_secs: 1,
                mapping_secs: 1,
                identity_secs: 1,
                perf_secs: 1,
                hot_secs: 1,
                follow_scan_secs: 1,
                hot_due_cap: 1,
                official_pnl_secs: 1,
                official_value_batch: 10,
                backfill_secs: 1,
                backfill_batch: 1,
                backfill_refresh_days: 1,
                signal_replay_secs: 1,
                trade_watch_secs: 1,
            },
            auto_match_threshold: 0.7,
            identity_threshold: 0.6,
            shadow_secs: 1,
            shadow_dry_run: true,
            shadow_third_party_url: String::new(),
            polymarket_data_api: None,
            polymarket_gamma_api: None,
            polymarket_clob_api: None,
            follow_url: String::new(),
            follow_signal_secret: String::new(),
            admin_token: String::new(),
            jwt_secret: String::from("dev-secret-change-me"),
        };
        let registry = build_registry(&config);
        assert!(registry
            .get(sharpside_shared::Platform::Polymarket)
            .is_some());
        assert!(registry.get(sharpside_shared::Platform::Kalshi).is_none());
    }

    #[test]
    fn enabled_signal_sources_respects_toggles() {
        let toggles = VenueToggles {
            polymarket: true,
            kalshi: true,
            manifold: false,
            zeitgeist: false,
            azuro: false,
        };
        let platforms = enabled_signal_sources(&toggles);
        assert_eq!(platforms.len(), 2);
    }
}
