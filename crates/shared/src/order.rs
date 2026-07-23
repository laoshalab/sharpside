//! 跟单指令：`CopyOrder` / `Side` / `Channel` / `CopyOrderStatus`。
//!
//! 对应 `docs/ARCHITECTURE.md` §10 跨平台跟单流程与 `docs/FLOWS.md` §5。
//! `CopyOrder` 是 Follow 服务派生、Copier 消费的标准指令，含 `source_venue` + `execute_venue`。

use crate::platform::Platform;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 买卖方向。跨 Venue 跟单时若 `market_mappings.direction_flip=true` 则翻转。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    /// 翻转方向，用于 `direction_flip=true` 的跨 Venue 映射。
    pub fn flip(self) -> Self {
        match self {
            Self::Buy => Self::Sell,
            Self::Sell => Self::Buy,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Buy => "buy",
            Self::Sell => "sell",
        }
    }
}

impl std::str::FromStr for Side {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "buy" => Ok(Self::Buy),
            "sell" => Ok(Self::Sell),
            other => Err(format!("unknown side: {other}")),
        }
    }
}

/// 跟单通道。对应 `docs/ARCHITECTURE.md` §11 双通道 × Venue 矩阵。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// 通道 A · TG 热钥浮仓，平台代签 session wallet
    Tg,
    /// 通道 B · 自托管 daemon，用户本地·自持私钥·零钥
    Daemon,
}

impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tg => "tg",
            Self::Daemon => "daemon",
        }
    }
}

impl std::str::FromStr for Channel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tg" => Ok(Self::Tg),
            "daemon" => Ok(Self::Daemon),
            other => Err(format!("unknown channel: {other}")),
        }
    }
}

/// 跟单指令生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CopyOrderStatus {
    /// 已入 `copy_queue`，待 Copier 消费
    Pending,
    /// Copier 已取走，执行中
    Dispatched,
    /// 已成交（含部分成交）
    Filled,
    /// 已跳过（无 verified 映射 / 风控拒单 / 管辖域不合规 / 低于 min_notional）
    Skipped,
    /// 执行失败（不重试，避免重复成交）
    Failed,
    /// 已撤单
    Cancelled,
}

/// 标准化跟单指令。
///
/// 由 Follow 服务从 `trader.position.changed` 信号派生，入 Postgres `account.copy_order` 表队列，
/// Copier 消费后查 `market_mappings` 翻译、单位换算、风控、派发执行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyOrder {
    /// 指令 ID（UUID v4），daemon 回传 result 时用此 ID 关联
    pub id: Uuid,
    /// 跟随关系 ID
    pub follow_relation_id: Uuid,
    /// 用户 ID
    pub user_id: Uuid,
    /// 信号来源 Venue（热钥浮仓变化的平台）
    pub source_venue: Platform,
    /// 执行目标 Venue（受 `user.jurisdiction` 约束）
    pub execute_venue: Platform,
    /// 信号源市场 ID（Polymarket condition_id / Kalshi ticker / Manifold marketId）
    pub source_market_id: String,
    /// 信号源 token ID（YES/NO token）
    pub source_token_id: String,
    /// 执行市场 ID（同 Venue 跟单时 = source_market_id；跨 Venue 时由 Copier 查映射填入）
    #[serde(default)]
    pub execute_market_id: Option<String>,
    /// 执行 token ID（同上）
    #[serde(default)]
    pub execute_token_id: Option<String>,
    /// 买卖方向（跨 Venue 且 `direction_flip=true` 时 Copier 翻转）
    pub side: Side,
    /// 目标 Venue 单位下的价格
    pub price: f64,
    /// 目标 Venue 单位下的数量
    pub size: f64,
    /// 跟单通道
    pub channel: Channel,
    /// 信号发生时间（热钥浮仓快照 diff 检出时间）
    pub signal_at: DateTime<Utc>,
    /// 指令入队时间
    pub enqueued_at: DateTime<Utc>,
    /// 当前状态
    pub status: CopyOrderStatus,
}

impl CopyOrder {
    /// 是否跨 Venue 跟单（决定是否查 `market_mappings`）。
    pub fn is_cross_venue(&self) -> bool {
        self.source_venue != self.execute_venue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_flip() {
        assert_eq!(Side::Buy.flip(), Side::Sell);
        assert_eq!(Side::Sell.flip(), Side::Buy);
    }

    #[test]
    fn copy_order_serde_round_trip() {
        let order = CopyOrder {
            id: Uuid::new_v4(),
            follow_relation_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            source_venue: Platform::Polymarket,
            execute_venue: Platform::Kalshi,
            source_market_id: "0xabc".into(),
            source_token_id: "12345".into(),
            execute_market_id: Some("KXBTC-26JUL31".into()),
            execute_token_id: None,
            side: Side::Buy,
            price: 50.0,
            size: 100.0,
            channel: Channel::Daemon,
            signal_at: Utc::now(),
            enqueued_at: Utc::now(),
            status: CopyOrderStatus::Pending,
        };
        let json = serde_json::to_string(&order).unwrap();
        let back: CopyOrder = serde_json::from_str(&json).unwrap();
        assert_eq!(order.source_venue, back.source_venue);
        assert_eq!(order.execute_venue, back.execute_venue);
        assert_eq!(order.side, back.side);
        assert_eq!(order.channel, back.channel);
        assert!(order.is_cross_venue());
        assert!(!CopyOrder {
            execute_venue: Platform::Polymarket,
            ..order
        }
        .is_cross_venue());
    }
}
