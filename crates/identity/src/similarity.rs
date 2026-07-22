//! 跨 Venue 身份启发式链接。对应 `docs/VENUE_DESIGN.md` §7.2。
//!
//! 对每对不同 Venue 的 trader 对算相似度，`confidence ≥ 阈值`（默认 0.6）进候选列表。
//!
//! 相似度信号：
//! - `x_username` 大小写不敏感相等：+0.5（最强信号，X 是跨平台身份桥梁）
//! - `alias` 大小写不敏感相等：+0.3
//! - 持仓相似度（同事件同方向）：由 VenueHub 离线计算后注入（本 crate 不含，预留扩展点）
//!
//! 上限 1.0。候选进 admin 审核队列；运营确认后由 `crates/db` 创建 `identities` 行
//! 并把两个 trader 的 `identity_id` 指向它。

use crate::types::CandidateLink;
use sharpside_venues_core::Trader;

/// 对每对 `(a, b)` 算身份相似度，`confidence ≥ threshold` 的进候选列表。
///
/// 同平台对跳过（同平台不需要跨 Venue 身份链接）。
/// 输出按 `confidence` 降序排列，便于审核队列按优先级处理。
pub fn candidate_identities<'a>(traders: &'a [Trader], threshold: f64) -> Vec<CandidateLink<'a>> {
    let mut out = Vec::new();
    for (i, a) in traders.iter().enumerate() {
        for b in traders.iter().skip(i + 1) {
            if a.platform == b.platform {
                continue;
            }
            let score = identity_similarity(a, b);
            if score >= threshold {
                out.push(CandidateLink {
                    a,
                    b,
                    confidence: score,
                });
            }
        }
    }
    out.sort_by(|x, y| {
        y.confidence
            .partial_cmp(&x.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// 身份相似度。对应 `docs/VENUE_DESIGN.md` §7.2 `identity_similarity`。
///
/// - `x_username` 相等（大小写不敏感）：+0.5
/// - `alias` 相等（大小写不敏感）：+0.3
/// - 上限 1.0
///
/// **持仓相似度（同事件同方向）由 VenueHub 离线计算后注入**，本函数不含；
/// 调用方可在外部加 `position_similarity` 后再与阈值比较。
pub fn identity_similarity(a: &Trader, b: &Trader) -> f64 {
    let mut score: f64 = 0.0;
    if let (Some(xa), Some(xb)) = (&a.x_username, &b.x_username) {
        if xa.eq_ignore_ascii_case(xb) {
            score += 0.5;
        }
    }
    if let (Some(na), Some(nb)) = (&a.alias, &b.alias) {
        if na.eq_ignore_ascii_case(nb) {
            score += 0.3;
        }
    }
    score.min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEFAULT_IDENTITY_THRESHOLD;
    use sharpside_shared::Platform;

    fn trader(platform: Platform, id: &str, alias: Option<&str>, x: Option<&str>) -> Trader {
        Trader {
            platform,
            venue_trader_id: id.into(),
            alias: alias.map(Into::into),
            profile_image: None,
            x_username: x.map(Into::into),
            verified: false,
            seed_pnl: None,
            seed_vol: None,
        }
    }

    #[test]
    fn similarity_x_username_match() {
        let a = trader(Platform::Polymarket, "0xabc", None, Some("whale"));
        let b = trader(Platform::Kalshi, "u123", None, Some("Whale"));
        assert!((identity_similarity(&a, &b) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn similarity_alias_match() {
        let a = trader(Platform::Polymarket, "0xabc", Some("CryptoKing"), None);
        let b = trader(Platform::Manifold, "m456", Some("cryptoking"), None);
        assert!((identity_similarity(&a, &b) - 0.3).abs() < 1e-9);
    }

    #[test]
    fn similarity_both_match_capped() {
        let a = trader(
            Platform::Polymarket,
            "0xabc",
            Some("CryptoKing"),
            Some("whale"),
        );
        let b = trader(Platform::Kalshi, "u123", Some("cryptoking"), Some("Whale"));
        // 0.5 + 0.3 = 0.8
        assert!((identity_similarity(&a, &b) - 0.8).abs() < 1e-9);
    }

    #[test]
    fn similarity_no_match() {
        let a = trader(Platform::Polymarket, "0xabc", Some("Alice"), Some("alice"));
        let b = trader(Platform::Kalshi, "u123", Some("Bob"), Some("bob"));
        assert!((identity_similarity(&a, &b) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn similarity_missing_fields() {
        let a = trader(Platform::Polymarket, "0xabc", None, None);
        let b = trader(Platform::Kalshi, "u123", Some("Bob"), Some("bob"));
        assert!((identity_similarity(&a, &b) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn candidate_identities_filters_below_threshold() {
        let traders = vec![
            trader(Platform::Polymarket, "0xabc", Some("Alice"), Some("alice")),
            trader(Platform::Kalshi, "u123", Some("Alice"), Some("alice")), // 0.8 ≥ 0.6
            trader(Platform::Manifold, "m456", Some("Bob"), Some("bob")),   // 0.0 < 0.6
        ];
        let cands = candidate_identities(&traders, DEFAULT_IDENTITY_THRESHOLD);
        assert_eq!(cands.len(), 1);
        assert!((cands[0].confidence - 0.8).abs() < 1e-9);
    }

    #[test]
    fn candidate_identities_skips_same_platform() {
        let traders = vec![
            trader(Platform::Polymarket, "0xabc", Some("Alice"), Some("alice")),
            trader(Platform::Polymarket, "0xdef", Some("Alice"), Some("alice")), // 同平台跳过
        ];
        let cands = candidate_identities(&traders, 0.0);
        assert!(cands.is_empty());
    }

    #[test]
    fn candidate_identities_sorted_desc() {
        let traders = vec![
            // a: Polymarket, alias=name, x=x1
            trader(Platform::Polymarket, "0x1", Some("name"), Some("x1")),
            // b: Kalshi, no alias, x=X1 → a-b: x match only = 0.5
            trader(Platform::Kalshi, "u1", None, Some("X1")),
            // c: Manifold, alias=name, x=x1 → a-c: x(0.5)+alias(0.3)=0.8; b-c: x(0.5)+alias(None vs Some)=0.5
            trader(Platform::Manifold, "m1", Some("name"), Some("x1")),
        ];
        let cands = candidate_identities(&traders, 0.4);
        assert_eq!(cands.len(), 3);
        // 降序：0.8 在前，两个 0.5 在后
        assert!((cands[0].confidence - 0.8).abs() < 1e-9);
        assert!((cands[1].confidence - 0.5).abs() < 1e-9);
        assert!((cands[2].confidence - 0.5).abs() < 1e-9);
    }

    #[test]
    fn candidate_identities_custom_threshold() {
        let traders = vec![
            trader(Platform::Polymarket, "0xabc", Some("Alice"), None), // alias only = 0.3
            trader(Platform::Kalshi, "u123", Some("alice"), None),
        ];
        // 默认 0.6 阈值下不达标
        assert!(candidate_identities(&traders, DEFAULT_IDENTITY_THRESHOLD).is_empty());
        // 放宽到 0.3 达标
        let cands = candidate_identities(&traders, 0.3);
        assert_eq!(cands.len(), 1);
    }

    #[test]
    fn candidate_identities_empty_input() {
        assert!(candidate_identities(&[], 0.0).is_empty());
    }

    #[test]
    fn candidate_identities_single_trader() {
        let traders = vec![trader(
            Platform::Polymarket,
            "0xabc",
            Some("Alice"),
            Some("alice"),
        )];
        assert!(candidate_identities(&traders, 0.0).is_empty());
    }
}
