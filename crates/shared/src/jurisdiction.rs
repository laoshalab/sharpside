//! 管辖域（Jurisdiction）→ 可执行 Venue 映射。
//!
//! 对应 `docs/ARCHITECTURE.md` §6.3 管辖域路由与 `docs/FLOWS.md` §8。
//! 用户 `account.users.jurisdiction`（us / eu / other）决定可用 execution_venue 集合。
//!
//! 此映射是纯领域逻辑（无 IO），下沉到 `crates/shared` 供三处共用，避免重复：
//! - `services/follow`：创建跟随 / 升级 watchlist 时**前置校验** execute_venue，早拒绝。
//! - `services/copier`：执行时**兜底校验**（防御纵深，即便 follow 创建后用户改了 jurisdiction）。
//! - `services/gateway`：BFF 仪表盘展示「可用 Venue」。
//!
//! 早期版本该函数在 copier 与 gateway 各有一份拷贝，已收敛至此。

use crate::platform::Platform;

/// 管辖域 → 允许的 execution_venue 集合。
///
/// - US → Polymarket（限类目）+ Kalshi
/// - EU → Polymarket + Zeitgeist + Azuro
/// - OTHER → Polymarket + Manifold（仅信号）+ Zeitgeist + Azuro
///
/// 未知 jurisdiction 回退到 OTHER 集合（最宽松），避免把未知法域用户误锁死。
pub fn allowed_execute_venues(jurisdiction: &str) -> Vec<Platform> {
    match jurisdiction {
        "us" => vec![Platform::Polymarket, Platform::Kalshi],
        "eu" => vec![Platform::Polymarket, Platform::Zeitgeist, Platform::Azuro],
        _ => vec![
            Platform::Polymarket,
            Platform::Manifold,
            Platform::Zeitgeist,
            Platform::Azuro,
        ],
    }
}

/// 判定某 execute_venue 是否被该管辖域允许。便捷封装。
pub fn is_allowed_venue(jurisdiction: &str, venue: Platform) -> bool {
    allowed_execute_venues(jurisdiction).contains(&venue)
}

/// 已在 copier `build_registry` 接入执行 adapter 的 Venue 集合。
///
/// 与管辖域允许集合不同：`allowed_execute_venues` 是「法域允许」，本函数是「工程已接入」。
/// follow 创建时须同时满足两者——否则会建出「每个信号都被 copier 因无 adapter 跳过」的
/// 静默失效跟随（H3 修复）。**新增 Venue 执行 adapter 时须同步更新本清单与 copier
/// `build_registry`**（两处同源，注释互引）。
pub fn implemented_execute_venues() -> &'static [Platform] {
    &[Platform::Polymarket]
}

/// 判定某 execute_venue 是否已接入执行 adapter。便捷封装。
pub fn is_implemented_venue(venue: Platform) -> bool {
    implemented_execute_venues().contains(&venue)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us_allows_kalshi_not_manifold() {
        let v = allowed_execute_venues("us");
        assert!(v.contains(&Platform::Kalshi));
        assert!(v.contains(&Platform::Polymarket));
        assert!(!v.contains(&Platform::Manifold));
    }

    #[test]
    fn eu_allows_zeitgeist_not_kalshi() {
        let v = allowed_execute_venues("eu");
        assert!(v.contains(&Platform::Polymarket));
        assert!(v.contains(&Platform::Zeitgeist));
        assert!(v.contains(&Platform::Azuro));
        assert!(!v.contains(&Platform::Kalshi));
    }

    #[test]
    fn other_allows_manifold() {
        let v = allowed_execute_venues("other");
        assert!(v.contains(&Platform::Manifold));
        assert!(v.contains(&Platform::Polymarket));
    }

    #[test]
    fn unknown_jurisdiction_falls_back_to_other() {
        let v = allowed_execute_venues("???");
        assert!(v.contains(&Platform::Manifold));
        assert!(v.contains(&Platform::Polymarket));
    }

    #[test]
    fn is_allowed_venue_helper() {
        assert!(is_allowed_venue("us", Platform::Kalshi));
        assert!(!is_allowed_venue("eu", Platform::Kalshi));
        assert!(is_allowed_venue("other", Platform::Manifold));
    }

    #[test]
    fn implemented_venues_currently_only_polymarket() {
        // Phase 1：仅 Polymarket 接入执行 adapter。新增 Venue 时同步更新本断言与 copier build_registry。
        let v = implemented_execute_venues();
        assert!(v.contains(&Platform::Polymarket));
        assert!(!v.contains(&Platform::Kalshi));
        assert!(!v.contains(&Platform::Manifold));
        assert!(is_implemented_venue(Platform::Polymarket));
        assert!(!is_implemented_venue(Platform::Kalshi));
    }
}
