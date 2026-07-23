//! hot worker — 浮仓快照 + 仓位 diff 信号派生（自适应频率）。
//!
//! 对应 `docs/ARCHITECTURE.md` §6.1 / `docs/VENUE_DESIGN.md` §8 / `docs/FLOWS.md` §5。
//!
//! 每个 tick：对每个已注册 signal_source Venue 的监控目标（热钥 ∪ 活跃直接跟随 ∪
//! 活跃 identity 跟随下的各 trader），
//!   1. 拉 positions → 与 `latest_snapshots()` 上一轮 diff
//!   2. 检出仓位变化（增/减/平/新开）→ 构造 `SignalPayload`（携带 trader.identity_id）→ POST `{FOLLOW_URL}/internal/signals`
//!   3. 写 `trader_positions_snapshot`（append-only）
//!
//! 信号延迟不丢数据：失败的目标下一轮重试。`FOLLOW_URL` 为空串则禁用 emit（仅快照）。
//! 监控范围扩展到所有活跃跟随目标，避免「非热钥被跟随却无信号」的静默失效。

use crate::registry::enabled_signal_sources;
use crate::state::AppState;
use rust_decimal::prelude::ToPrimitive;
use serde::Serialize;
use sharpside_db::queries::monitor;
use sharpside_shared::{Platform, Side};
use sharpside_venues_core::{Position, VenueCapabilities};
use std::collections::HashMap;
use std::time::Duration;

/// POST 到 follow `/internal/signals` 的 body。字段口径与 follow::signal::SignalEvent 对齐。
#[derive(Debug, Clone, Serialize)]
struct SignalPayload {
    platform: Platform,
    trader_id: String,
    token_id: String,
    market_id: String,
    side: Side,
    price: f64,
    size: f64,
    ts: chrono::DateTime<chrono::Utc>,
    identity_id: Option<uuid::Uuid>,
}

/// 仓位变化阈值：|delta| > 此值才 emit（过滤浮点噪声）。
const DELTA_EPSILON: f64 = 1e-6;

pub async fn run(state: AppState) {
    let interval = state.config.workers.hot_secs.max(1);
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
                let targets = match monitor::list_signal_targets(&state.db, platform.as_str())
                    .await
                {
                    Ok(t) => t,
                    Err(e) => {
                        tracing::warn!(platform = platform.as_str(), error = %e, "hot 读监控目标清单失败");
                        continue;
                    }
                };
                for w in &targets {
                    match venue.positions(&w.address).await {
                        Ok(positions) => {
                            // 1. diff vs 上一轮快照
                            let signals = diff_positions(
                                platform,
                                &w.address,
                                w.identity_id,
                                &positions,
                                &state,
                            )
                            .await;
                            // 2. emit
                            emit_signals(&state, signals).await;
                            // 3. 写新快照
                            let now = chrono::Utc::now();
                            for p in &positions {
                                let _ = monitor::insert_position_snapshot(
                                    &state.db,
                                    platform.as_str(),
                                    &w.address,
                                    &p.token_id,
                                    Some(p.market_id.as_str()),
                                    p.size,
                                    p.avg_price,
                                    p.current_price,
                                    p.pnl,
                                    now,
                                )
                                .await;
                            }
                        }
                        Err(sharpside_venues_core::VenueError::Unsupported(_)) => {}
                        Err(e) => {
                            tracing::warn!(platform = platform.as_str(), address = %w.address, error = %e, "hot 拉 positions 失败")
                        }
                    }
                }
            }
        }
    }
}

/// 比对当前 positions 与上一轮快照，检出仓位变化并构造信号。
/// - delta > 0 → Buy |delta|
/// - delta < 0 → Sell |delta|
/// - 新 token（prev 无）→ Buy current.size
/// - 消失 token（prev 有，current 无）→ Sell prev.size（完全平仓）
async fn diff_positions(
    platform: Platform,
    address: &str,
    identity_id: Option<uuid::Uuid>,
    current: &[Position],
    state: &AppState,
) -> Vec<SignalPayload> {
    let prev = match monitor::latest_snapshots(&state.db, platform.as_str(), address).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(platform = platform.as_str(), address, error = %e, "读上一轮快照失败，本轮不 emit");
            return Vec::new();
        }
    };
    // Bootstrap：首次监控该 address 时 prev 为空。若直接 diff，会把 trader 现有全部持仓
    // 当作 Buy 信号发出（用户刚跟随即收到存量仓位跟单指令）。本轮只写 baseline 快照、不 emit；
    // 下一轮起 prev 非空，真实增量才派生信号。
    if prev.is_empty() {
        tracing::info!(
            platform = platform.as_str(),
            address,
            "bootstrap：首次监控，仅写 baseline 快照，本轮不 emit 信号"
        );
        return Vec::new();
    }
    let now = chrono::Utc::now();
    let prev_map: HashMap<String, &sharpside_db::models::PositionSnapshot> =
        prev.iter().map(|s| (s.token_id.clone(), s)).collect();
    let cur_tokens: HashMap<&str, ()> = current.iter().map(|p| (p.token_id.as_str(), ())).collect();

    let mut signals = Vec::new();

    // 当前持仓变化 / 新开
    for p in current {
        let prev_size = prev_map
            .get(&p.token_id)
            .map(|s| s.size.to_f64().unwrap_or(0.0))
            .unwrap_or(0.0);
        let delta = p.size - prev_size;
        if delta.abs() <= DELTA_EPSILON {
            continue;
        }
        let side = if delta > 0.0 { Side::Buy } else { Side::Sell };
        signals.push(SignalPayload {
            platform,
            trader_id: address.into(),
            token_id: p.token_id.clone(),
            market_id: p.market_id.clone(),
            side,
            // 用当前均价作为信号价格（近似成交价；follow 派生 sizing 用此）
            price: if p.avg_price > 0.0 {
                p.avg_price
            } else {
                p.current_price
            },
            size: delta.abs(),
            ts: now,
            identity_id,
        });
    }

    // 完全平仓：prev 有但 current 无
    for s in &prev {
        if cur_tokens.contains_key(s.token_id.as_str()) {
            continue;
        }
        let prev_size = s.size.to_f64().unwrap_or(0.0);
        if prev_size <= DELTA_EPSILON {
            continue;
        }
        let prev_price = s.avg_price.to_f64().unwrap_or(0.0);
        signals.push(SignalPayload {
            platform,
            trader_id: address.into(),
            token_id: s.token_id.clone(),
            market_id: s.condition_id.clone().unwrap_or_default(),
            side: Side::Sell,
            price: if prev_price > 0.0 {
                prev_price
            } else {
                s.current_price.to_f64().unwrap_or(0.0)
            },
            size: prev_size,
            ts: now,
            identity_id,
        });
    }

    signals
}

/// 批量 POST 信号到 follow `/internal/signals`。失败则落 `signal_outbox` 由 replay worker 重发，
/// 不再静默丢弃（H4 修复）。快照写入仍正常推进——outbox 兜底重发，不阻塞 diff。
async fn emit_signals(state: &AppState, signals: Vec<SignalPayload>) {
    if signals.is_empty() {
        return;
    }
    let follow_url = state.config.follow_url.trim();
    if follow_url.is_empty() {
        return;
    }
    let url = format!("{}/internal/signals", follow_url.trim_end_matches('/'));
    let n = signals.len();
    let secret = state.config.follow_signal_secret.trim();
    let mut failed = 0usize;
    for sig in signals {
        let trader = sig.trader_id.clone();
        let token = sig.token_id.clone();
        // 与 follow 侧 copy_order.signal_id 用同一算法，保证 outbox 重发幂等。
        let sig_id = sharpside_shared::signal_id(
            sig.platform.as_str(),
            &sig.trader_id,
            &sig.token_id,
            sig.ts,
        );
        let mut req = state.http.post(&url).json(&sig);
        if !secret.is_empty() {
            req = req.header("x-internal-secret", secret);
        }
        let ok = match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                if !status.is_success() {
                    let body = resp.text().await.unwrap_or_default();
                    tracing::warn!(
                        status = %status,
                        trader = %trader,
                        token = %token,
                        body = body.chars().take(200).collect::<String>(),
                        "follow /internal/signals 非 2xx，落 outbox 待重发",
                    );
                    false
                } else {
                    true
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, trader = %trader, token = %token, "POST follow /internal/signals 失败，落 outbox 待重发");
                false
            }
        };
        if !ok {
            failed += 1;
            let payload = match serde_json::to_value(&sig) {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(error = %e, "SignalPayload 序列化失败，无法落 outbox，该信号丢失");
                    continue;
                }
            };
            if let Err(e) =
                sharpside_db::queries::outbox::enqueue_signal_outbox(&state.db, &sig_id, &payload, &url, 5)
                    .await
            {
                tracing::error!(error = %e, signal_id = %sig_id, "signal_outbox 落表失败，该信号丢失");
            }
        }
    }
    tracing::info!(n, failed, "hot worker emit 信号完成（失败已落 outbox）");
}
