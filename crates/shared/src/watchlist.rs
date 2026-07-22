//! 观察名单（Watchlist）：用户收藏观察 trader / identity，不进执行路径。
//!
//! 对应 Watchlist 功能规划。与 `FollowConfig` 的区别：
//! - 无 execute_venue / channel / config —— 仅观察，不派生信号、不下单。
//! - 无 botfilter / identity manual_verified 门控 —— 观察不等于跟单。
//! - 一键升级为 Follow 时消费掉本行（删除）。
//!
//! 配额按 `subscription_tier` 差异化（商业化），见 [`WATCHLIST_LIMIT_*`]。

use serde::Deserialize;
use uuid::Uuid;

/// Free 档位 watchlist 收藏上限。
pub const WATCHLIST_LIMIT_FREE: i64 = 20;
/// Pro+ 档位 watchlist 收藏上限。
pub const WATCHLIST_LIMIT_PRO_PLUS: i64 = 200;

/// 按订阅档位返回 watchlist 配额。
pub fn watchlist_limit(tier: &str) -> i64 {
    match tier {
        "pro_plus" => WATCHLIST_LIMIT_PRO_PLUS,
        _ => WATCHLIST_LIMIT_FREE,
    }
}

/// 创建 watchlist 请求体（trader 或 identity 二选一）。
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum WatchlistCreate {
    Trader {
        watch_platform: String,
        watch_address: String,
    },
    Identity {
        watch_identity_id: Uuid,
    },
}

/// 升级为 Follow 的请求体（执行路径参数，由用户在升级时补齐）。
#[derive(Debug, Clone, Deserialize)]
pub struct WatchlistUpgrade {
    pub execute_venue: String,
    pub channel: String,
    /// 跟随配置（sizing / 上限 / 过滤）。复用 `FollowConfig`。
    pub config: crate::follow::FollowConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_limit_is_20() {
        assert_eq!(watchlist_limit("free"), 20);
    }

    #[test]
    fn pro_plus_limit_is_200() {
        assert_eq!(watchlist_limit("pro_plus"), 200);
    }

    #[test]
    fn unknown_tier_falls_back_to_free() {
        assert_eq!(watchlist_limit("???"), 20);
    }

    #[test]
    fn create_trader_parses() {
        let json = r#"{"watch_platform":"polymarket","watch_address":"0xabc"}"#;
        let body: WatchlistCreate = serde_json::from_str(json).unwrap();
        match body {
            WatchlistCreate::Trader {
                watch_platform,
                watch_address,
            } => {
                assert_eq!(watch_platform, "polymarket");
                assert_eq!(watch_address, "0xabc");
            }
            _ => panic!("应为 Trader 变体"),
        }
    }

    #[test]
    fn create_identity_parses() {
        let json = r#"{"watch_identity_id":"11111111-1111-1111-1111-111111111111"}"#;
        let body: WatchlistCreate = serde_json::from_str(json).unwrap();
        match body {
            WatchlistCreate::Identity { watch_identity_id } => {
                assert_eq!(
                    watch_identity_id,
                    Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()
                );
            }
            _ => panic!("应为 Identity 变体"),
        }
    }
}
