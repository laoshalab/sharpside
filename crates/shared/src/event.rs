//! 信号事件：`TradeEvent`（`trader.position.changed`）。
//!
//! 对应 `docs/FLOWS.md` §5 信号派生流程。
//! VenueHub 的 hot worker 检测热钥浮仓快照 diff 后 publish 此事件到 Redis `trader.events`，
//! Follow 服务消费后匹配 `follow_relation` 派生 `CopyOrder`。

use crate::platform::Platform;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 热钥浮仓变化事件。
///
/// 字段口径与 `trader_positions_snapshot` 一致，diff 检出后由 VenueHub 发布。
/// 跨平台身份跟随时，Follow 服务会查 `traders.identity_id` 把同一 identity 下
/// 所有 trader 的变化都触发。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeEvent {
    /// 信号源 Venue
    pub platform: Platform,
    /// 交易者在该 Venue 的标识（proxy wallet / user id，小写）
    pub trader_id: String,
    /// 关联的跨平台身份 ID（若已链接）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_id: Option<uuid::Uuid>,
    /// 市场 ID（Polymarket condition_id / Kalshi ticker / Manifold marketId）
    pub market_id: String,
    /// token ID（YES/NO token）
    pub token_id: String,
    /// 变化方向（增仓 / 减仓）
    pub change: PositionChange,
    /// 快照时间
    pub captured_at: DateTime<Utc>,
}

/// 浮仓变化类型与幅度。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionChange {
    /// 变化前的 size
    pub prev_size: f64,
    /// 变化后的 size
    pub new_size: f64,
    /// 变化量（new - prev，正=增仓，负=减仓）
    pub delta: f64,
    /// 变化前的均价
    pub prev_avg_price: f64,
    /// 变化后的均价
    pub new_avg_price: f64,
}

impl PositionChange {
    /// 是否为开新仓或加仓（delta > 0）。
    pub fn is_increase(&self) -> bool {
        self.delta > 0.0
    }

    /// 是否为减仓或平仓（delta < 0）。
    pub fn is_decrease(&self) -> bool {
        self.delta < 0.0
    }

    /// 是否完全平仓（prev > 0 且 new == 0）。
    pub fn is_full_close(&self) -> bool {
        self.prev_size > 0.0 && self.new_size == 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trade_event_serde_round_trip() {
        let ev = TradeEvent {
            platform: Platform::Polymarket,
            trader_id: "0x56687bf447db6ffa42ffe2204a05edaa20f55839".into(),
            identity_id: None,
            market_id: "0xabc".into(),
            token_id: "12345".into(),
            change: PositionChange {
                prev_size: 100.0,
                new_size: 150.0,
                delta: 50.0,
                prev_avg_price: 0.5,
                new_avg_price: 0.55,
            },
            captured_at: Utc::now(),
        };
        let json = serde_json::to_string(&ev).unwrap();
        let back: TradeEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev.platform, back.platform);
        assert!(back.change.is_increase());
        assert!(!back.change.is_decrease());
        assert!(!back.change.is_full_close());
    }

    #[test]
    fn full_close_detection() {
        let c = PositionChange {
            prev_size: 100.0,
            new_size: 0.0,
            delta: -100.0,
            prev_avg_price: 0.5,
            new_avg_price: 0.0,
        };
        assert!(c.is_full_close());
        assert!(c.is_decrease());
    }
}
