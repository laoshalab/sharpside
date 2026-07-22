//! identity worker — 启发式链接 + 入审核队列。对应 `docs/ARCHITECTURE.md` §6.1 / `docs/VENUE_DESIGN.md` §7.2。
//!
//! 每个 tick：跨平台读可见 traders → 调 `sharpside_identity::candidate_identities`
//! → 对每个候选（且双方尚未链接 identity）创建 identity 并链接。
//! 不阻塞跟单：仅影响 Identity 展示。

use crate::state::AppState;
use sharpside_db::queries::identities as identity_q;
use sharpside_db::queries::traders as trader_q;
use sharpside_venues_core::Trader as VenueTrader;
use std::time::Duration;

/// `db::Trader` → 通用 `Trader`（identity crate 输入）。
fn to_venue_trader(t: &sharpside_db::Trader) -> VenueTrader {
    VenueTrader {
        platform: t
            .platform
            .parse()
            .unwrap_or(sharpside_shared::Platform::Polymarket),
        venue_trader_id: t.address.clone(),
        alias: t.alias.clone(),
        profile_image: t.profile_image.clone(),
        x_username: t.x_username.clone(),
        verified: t.verified_badge.unwrap_or(false),
        seed_pnl: None,
        seed_vol: None,
    }
}

pub async fn run(state: AppState) {
    let interval = state.config.workers.identity_secs.max(1);
    let mut ticker = tokio::time::interval(Duration::from_secs(interval));
    loop {
        ticker.tick().await;
        let db_traders = match trader_q::list_all_visible_traders(&state.db, 2000, 0).await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "identity 读 traders 失败");
                continue;
            }
        };
        let venue_traders: Vec<VenueTrader> = db_traders.iter().map(to_venue_trader).collect();
        let candidates = sharpside_identity::candidate_identities(
            &venue_traders,
            state.config.identity_threshold,
        );
        let mut created = 0usize;
        for c in &candidates {
            // 仅当双方均尚未链接 identity 时才创建，减少重复聚合。
            let a_db = db_traders
                .iter()
                .find(|t| t.platform == c.a.platform.as_str() && t.address == c.a.venue_trader_id);
            let b_db = db_traders
                .iter()
                .find(|t| t.platform == c.b.platform.as_str() && t.address == c.b.venue_trader_id);
            let (Some(a), Some(b)) = (a_db, b_db) else {
                continue;
            };
            if a.identity_id.is_some() && b.identity_id.is_some() {
                continue;
            }
            let keys: [(&str, &str); 2] = [(&a.platform, &a.address), (&b.platform, &b.address)];
            match identity_q::create_identity_with_links(
                &state.db,
                a.alias.as_deref().or(b.alias.as_deref()),
                c.confidence,
                &keys,
            )
            .await
            {
                Ok(id) => {
                    created += 1;
                    tracing::info!(identity_id = %id, confidence = c.confidence, "identity 创建候选");
                }
                Err(e) => tracing::warn!(error = %e, "identity 创建失败"),
            }
        }
        if created > 0 {
            tracing::info!(created, "identity 本轮创建候选数");
        }
    }
}
