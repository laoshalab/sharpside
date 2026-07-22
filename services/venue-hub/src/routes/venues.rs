//! `GET /venues` — 列出已接入 Venue 及能力。对应 `docs/ARCHITECTURE.md` §6.1。

use crate::error::ApiError;
use crate::state::AppState;
use axum::Json;
use serde::{Deserialize, Serialize};
use sharpside_venues_core::{AuthModel, Geo, Unit, VenueCapabilities};

/// 对外 Venue 摘要。
#[derive(Debug, Serialize, Deserialize)]
pub struct VenueOut {
    pub platform: String,
    pub display_name: String,
    pub capabilities: Vec<String>,
    pub auth_model: String,
    pub unit: String,
    pub geo: String,
}

fn cap_names(caps: VenueCapabilities) -> Vec<String> {
    let mut out = Vec::new();
    if caps.contains(VenueCapabilities::SIGNAL_SOURCE) {
        out.push("signal_source".into());
    }
    if caps.contains(VenueCapabilities::EXECUTION_VENUE) {
        out.push("execution_venue".into());
    }
    out
}

pub async fn list_venues(state: AppState) -> Result<Json<Vec<VenueOut>>, ApiError> {
    let mut out = Vec::new();
    for p in state.registry.platforms() {
        if let Some(v) = state.registry.get(p) {
            let info = v.info();
            out.push(VenueOut {
                platform: info.platform.as_str().into(),
                display_name: info.display_name.clone(),
                capabilities: cap_names(info.capabilities),
                auth_model: match info.auth_model {
                    AuthModel::Wallet => "wallet".into(),
                    AuthModel::KycApiKey => "kyc_api_key".into(),
                    AuthModel::ApiKey => "api_key".into(),
                    AuthModel::None => "none".into(),
                },
                unit: match info.unit {
                    Unit::UsdcCtf => "usdc_ctf".into(),
                    Unit::UsdCents => "usd_cents".into(),
                    Unit::Mana => "mana".into(),
                    Unit::Native => "native".into(),
                },
                geo: match info.geo {
                    Geo::Global => "global".into(),
                    Geo::UsOnly => "us_only".into(),
                    Geo::GlobalWithUsRestrictions => "global_with_us_restrictions".into(),
                },
            });
        }
    }
    Ok(Json(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_names_lists_both() {
        let both = VenueCapabilities::SIGNAL_SOURCE | VenueCapabilities::EXECUTION_VENUE;
        let names = cap_names(both);
        assert_eq!(names, vec!["signal_source", "execution_venue"]);
    }

    #[test]
    fn cap_names_lists_signal_only() {
        let names = cap_names(VenueCapabilities::SIGNAL_SOURCE);
        assert_eq!(names, vec!["signal_source"]);
    }
}
