//! 跟随配置：`FollowConfig` / `SizingMode`。
//!
//! 对应 `docs/ARCHITECTURE.md` §6.2 跟随关系 CRUD 与 `docs/FLOWS.md` §4。
//! 跟随对象可以是单 Venue 的 Trader，或跨 Venue 的 Identity（须 `manual_verified=true`）。

use crate::platform::Platform;
use serde::{Deserialize, Serialize};

/// 仓位规模模式。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mode", content = "value")]
pub enum SizingMode {
    /// 固定金额（USDC）每笔
    Fixed { amount: f64 },
    /// 按被跟随者仓位比例复制
    Proportional { ratio: f64 },
    /// 按用户总余额百分比
    PercentOfBalance { pct: f64 },
}

/// 跟随配置，跟随关系创建时由用户指定。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowConfig {
    /// 仓位规模模式
    pub sizing: SizingMode,
    /// 单笔最大 notional（USDC），0 = 不限
    #[serde(default)]
    pub max_notional_per_order: f64,
    /// 日累计成交上限（USDC），0 = 不限
    #[serde(default)]
    pub daily_max_notional: f64,
    /// 最大持仓数，0 = 不限
    #[serde(default)]
    pub max_open_positions: u32,
    /// 用户偏好的执行 Venue（受 `user.jurisdiction` 约束）
    pub execute_venue: Platform,
    /// 跟单通道
    pub channel: crate::order::Channel,
    /// 是否只跟同 Venue（true = source_venue == execute_venue，不跨 Venue）
    #[serde(default)]
    pub same_venue_only: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizing_mode_serde_round_trip() {
        for s in [
            SizingMode::Fixed { amount: 50.0 },
            SizingMode::Proportional { ratio: 0.5 },
            SizingMode::PercentOfBalance { pct: 0.05 },
        ] {
            let json = serde_json::to_string(&s).unwrap();
            let back: SizingMode = serde_json::from_str(&json).unwrap();
            assert_eq!(s, back);
        }
    }

    #[test]
    fn follow_config_serde_round_trip() {
        let cfg = FollowConfig {
            sizing: SizingMode::Fixed { amount: 100.0 },
            max_notional_per_order: 500.0,
            daily_max_notional: 2000.0,
            max_open_positions: 10,
            execute_venue: Platform::Polymarket,
            channel: crate::order::Channel::Daemon,
            same_venue_only: false,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: FollowConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg.execute_venue, back.execute_venue);
        assert_eq!(cfg.max_open_positions, back.max_open_positions);
    }
}
