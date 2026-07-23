//! 信号投递共享件：venue-hub 的 hot（diff 对账）与 trade_watch（逐笔）worker 共用。
//!
//! `emit_signals`：逐条 POST 到 follow `/internal/signals`，失败落 `signal_outbox` 由
//! `signal_replay` worker 重发（H4 修复，不再静默丢弃）。`signal_id` 与 follow 侧
//! `copy_order.signal_id` 用同一算法（含 `source_id`），保证 outbox 重发幂等。

use crate::state::AppState;
use serde::Serialize;
use sharpside_shared::{Platform, Side};

/// POST 到 follow `/internal/signals` 的 body。字段口径与 follow::signal::SignalEvent 对齐。
#[derive(Debug, Clone, Serialize)]
pub struct SignalPayload {
    pub platform: Platform,
    pub trader_id: String,
    pub token_id: String,
    pub market_id: String,
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub ts: chrono::DateTime<chrono::Utc>,
    pub identity_id: Option<uuid::Uuid>,
    /// 逐笔信号 = 成交 ID（raw_trades.trade_id/tx_hash）；diff 信号 = None。
    /// 与 follow 侧共同决定 signal_id，避免同秒同 token 多笔撞键。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
}

/// 批量 POST 信号到 follow `/internal/signals`。失败则落 `signal_outbox` 由 replay worker 重发，
/// 不再静默丢弃（H4 修复）。
pub async fn emit_signals(state: &AppState, signals: Vec<SignalPayload>) {
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
        // diff 信号无 source_id（ts=检测时刻，天然唯一）；逐笔信号带成交 ID。
        let sig_id = sharpside_shared::signal_id(
            sig.platform.as_str(),
            &sig.trader_id,
            &sig.token_id,
            sig.ts,
            sig.source_id.as_deref(),
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
            crate::metrics::inc_hot_emit_fail();
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
    tracing::info!(n, failed, "emit 信号完成（失败已落 outbox）");
}
