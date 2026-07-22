//! 启发式市场映射匹配。对应 `docs/VENUE_DESIGN.md` §6.2。
//!
//! 对每对 `(signal_source, execution_venue)` 拉两边 markets，算相似度，产候选映射：
//! `similarity = 0.5 * title_sim + 0.3 * tag_sim + 0.2 * time_sim`
//! `confidence ≥ 阈值`（默认 0.7）→ 入表 `market_mappings`（`manual_verified=false`）进审核队列。

use crate::types::CandidateMapping;
use chrono::{DateTime, Utc};
use sharpside_venues_core::Market;
use std::collections::HashSet;

/// 默认候选阈值。对应 `docs/VENUE_DESIGN.md` §6.2 与 `[mapping] auto_match_threshold = 0.7`。
pub const DEFAULT_AUTO_MATCH_THRESHOLD: f64 = 0.7;

/// 对每对 `(a, b)` 算相似度，`confidence ≥ threshold` 的进候选列表。
///
/// 输出按 `confidence` 降序排列，便于审核队列按优先级处理。
pub fn candidate_mappings<'a>(
    a: &'a [Market],
    b: &'a [Market],
    threshold: f64,
) -> Vec<CandidateMapping<'a>> {
    let mut out = Vec::new();
    for ma in a {
        for mb in b {
            // 同平台不需要跨 Venue 映射
            if ma.platform == mb.platform {
                continue;
            }
            let score = similarity(ma, mb);
            if score >= threshold {
                out.push(CandidateMapping {
                    from: ma,
                    to: mb,
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

/// 综合相似度：`0.5 * title + 0.3 * tag + 0.2 * time`。对应 `docs/VENUE_DESIGN.md` §6.2。
pub fn similarity(a: &Market, b: &Market) -> f64 {
    let title_sim = token_jaccard(&a.title, &b.title);
    let tag_sim = tag_overlap(&a.tags, &b.tags);
    let time_sim = end_date_closeness(a.end_date, b.end_date);
    0.5 * title_sim + 0.3 * tag_sim + 0.2 * time_sim
}

/// 标题 token Jaccard 相似度（大小写不敏感，按非字母数字字符分词）。返回 0.0–1.0。
pub fn token_jaccard(a: &str, b: &str) -> f64 {
    let ta: HashSet<String> = tokenize(a);
    let tb: HashSet<String> = tokenize(b);
    if ta.is_empty() && tb.is_empty() {
        return 1.0;
    }
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let inter = ta.intersection(&tb).count() as f64;
    let union = ta.union(&tb).count() as f64;
    inter / union
}

/// 标签重叠率（Jaccard，大小写不敏感）。返回 0.0–1.0。两边都空视为 1.0（都无标签=不冲突）。
pub fn tag_overlap(a: &[String], b: &[String]) -> f64 {
    let sa: HashSet<String> = a.iter().map(|s| s.to_lowercase()).collect();
    let sb: HashSet<String> = b.iter().map(|s| s.to_lowercase()).collect();
    if sa.is_empty() && sb.is_empty() {
        return 1.0;
    }
    if sa.is_empty() || sb.is_empty() {
        return 0.0;
    }
    let inter = sa.intersection(&sb).count() as f64;
    let union = sa.union(&sb).count() as f64;
    inter / union
}

/// 结算日期接近度。返回 0.0–1.0。
///
/// - 两边都无日期：1.0（不冲突，靠标题/标签兜底）
/// - 一边有一边无：0.0（信息不对称，降权）
/// - 都有：`max(0, 1 - |diff_days| / 30)`，30 天外视为 0
pub fn end_date_closeness(a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>) -> f64 {
    match (a, b) {
        (None, None) => 1.0,
        (Some(_), None) | (None, Some(_)) => 0.0,
        (Some(da), Some(db)) => {
            let diff = (da - db).num_seconds().abs() as f64 / 86_400.0; // 天
            (1.0 - diff / 30.0).max(0.0)
        }
    }
}

/// 按非字母数字字符分词，转小写。
fn tokenize(s: &str) -> HashSet<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use sharpside_shared::Platform;

    fn market(
        platform: Platform,
        id: &str,
        title: &str,
        tags: &[&str],
        end: Option<&str>,
    ) -> Market {
        Market {
            platform,
            venue_market_id: id.into(),
            title: title.into(),
            slug: None,
            tags: tags.iter().map(|s| s.to_string()).collect(),
            category: None,
            end_date: end.map(|e| DateTime::parse_from_rfc3339(e).unwrap().with_timezone(&Utc)),
            outcome_yes: None,
            outcome_no: None,
            closed: None,
        }
    }

    #[test]
    fn token_jaccard_identical() {
        assert!((token_jaccard("Will Trump win", "Will Trump win") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn token_jaccard_case_insensitive() {
        assert!((token_jaccard("Will Trump Win", "will trump win") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn token_jaccard_partial() {
        // "will trump win 2024" vs "will trump lose" → tokens {will,trump,win,2024} vs {will,trump,lose}
        // inter=2, union=5 → 0.4
        let s = token_jaccard("Will Trump win 2024", "Will Trump lose");
        assert!((s - 0.4).abs() < 1e-9);
    }

    #[test]
    fn token_jaccard_disjoint() {
        assert!((token_jaccard("abc", "xyz") - 0.0).abs() < 1e-9);
    }

    #[test]
    fn token_jaccard_both_empty() {
        assert!((token_jaccard("", "") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn tag_overlap_identical() {
        let a = vec!["politics".into(), "usa".into()];
        let b = vec!["Politics".into(), "USA".into()];
        assert!((tag_overlap(&a, &b) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn tag_overlap_both_empty() {
        assert!((tag_overlap(&[], &[]) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn tag_overlap_one_empty() {
        let a = vec!["politics".into()];
        assert!((tag_overlap(&a, &[]) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn end_date_both_none() {
        assert!((end_date_closeness(None, None) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn end_date_one_none() {
        let d = Some(Utc.timestamp_opt(0, 0).unwrap());
        assert!((end_date_closeness(d, None) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn end_date_same_day() {
        let d = Some(
            DateTime::parse_from_rfc3339("2026-11-05T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        );
        assert!((end_date_closeness(d, d) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn end_date_15_days() {
        let a = DateTime::parse_from_rfc3339("2026-11-05T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let b = DateTime::parse_from_rfc3339("2026-11-20T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        // diff=15d → 1 - 15/30 = 0.5
        assert!((end_date_closeness(Some(a), Some(b)) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn end_date_beyond_30_days() {
        let a = DateTime::parse_from_rfc3339("2026-11-05T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let b = DateTime::parse_from_rfc3339("2026-12-10T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        // diff=35d → 0
        assert!((end_date_closeness(Some(a), Some(b)) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn candidate_mappings_filters_below_threshold() {
        let a = vec![market(
            Platform::Polymarket,
            "c1",
            "Will Trump win the 2024 election",
            &["politics", "usa"],
            Some("2026-11-05T00:00:00Z"),
        )];
        let b = vec![
            // 高相似：同标题同标签同日期
            market(
                Platform::Kalshi,
                "k1",
                "Will Trump win the 2024 election",
                &["politics", "usa"],
                Some("2026-11-05T00:00:00Z"),
            ),
            // 低相似：完全不同
            market(
                Platform::Kalshi,
                "k2",
                "Bitcoin price above 100k",
                &["crypto"],
                Some("2026-12-01T00:00:00Z"),
            ),
        ];
        let cands = candidate_mappings(&a, &b, 0.7);
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].to.venue_market_id, "k1");
        assert!(cands[0].confidence >= 0.7);
    }

    #[test]
    fn candidate_mappings_skips_same_platform() {
        let a = vec![market(
            Platform::Polymarket,
            "c1",
            "Will Trump win",
            &["politics"],
            None,
        )];
        let b = vec![market(
            Platform::Polymarket,
            "c2",
            "Will Trump win",
            &["politics"],
            None,
        )];
        let cands = candidate_mappings(&a, &b, 0.0);
        assert!(cands.is_empty()); // 同平台跳过
    }

    #[test]
    fn candidate_mappings_sorted_by_confidence_desc() {
        let a = vec![
            market(
                Platform::Polymarket,
                "c1",
                "Will Trump win the 2024 election",
                &["politics"],
                Some("2026-11-05T00:00:00Z"),
            ),
            market(
                Platform::Polymarket,
                "c2",
                "Bitcoin above 100k",
                &["crypto"],
                Some("2026-12-01T00:00:00Z"),
            ),
        ];
        let b = vec![
            market(
                Platform::Kalshi,
                "k1",
                "Will Trump win the 2024 election",
                &["politics"],
                Some("2026-11-05T00:00:00Z"),
            ),
            market(
                Platform::Kalshi,
                "k2",
                "Bitcoin above 100k",
                &["crypto"],
                Some("2026-12-01T00:00:00Z"),
            ),
        ];
        let cands = candidate_mappings(&a, &b, 0.5);
        assert!(cands.len() >= 2);
        // 降序
        for w in cands.windows(2) {
            assert!(w[0].confidence >= w[1].confidence);
        }
    }

    #[test]
    fn similarity_weights() {
        // 完全相同 → 1.0
        let m = market(
            Platform::Polymarket,
            "c1",
            "Will Trump win",
            &["politics"],
            Some("2026-11-05T00:00:00Z"),
        );
        let n = market(
            Platform::Kalshi,
            "k1",
            "Will Trump win",
            &["politics"],
            Some("2026-11-05T00:00:00Z"),
        );
        assert!((similarity(&m, &n) - 1.0).abs() < 1e-9);
    }
}
