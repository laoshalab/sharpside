//! 信号去重键。venue-hub（outbox 落表）与 follow（copy_order 去重）共用同一算法，
//! 保证 outbox 重发同一信号时 follow 侧不重复派生 copy_order。
//!
//! 对应 H4 修复：信号投递从「fire-and-forget + warn」升级为「outbox + 重发 + 幂等」。

use chrono::{DateTime, Utc};

/// 信号去重键：`platform|trader_id|token_id|ts(rfc3339)`，可选追加 `|source_id`。
///
/// 确定性、跨进程一致：同一 (platform, trader, token, ts[, source_id]) 永远产出同一 key。
/// `ts` 用 RFC3339 字符串以避免浮点/时区漂移。venue-hub 检出信号时与 follow `/internal/signals`
/// 派生时各算一次，须完全一致。
///
/// `source_id`：第 3 层逐笔信号用成交 ID（`raw_trades.trade_id`/`tx_hash`），避免同一秒同 token
/// 多笔成交撞键；仓位 diff 信号传 `None`（其 ts=检测时刻，天然唯一）。两种 key 段数不同，永不碰撞。
pub fn signal_id(
    platform: &str,
    trader_id: &str,
    token_id: &str,
    ts: DateTime<Utc>,
    source_id: Option<&str>,
) -> String {
    // 安全修复 4.4：链上地址归一后再拼键，避免 checksum vs 小写产生两套 signal_id。
    let trader = crate::platform::normalize_trader_id(platform, trader_id);
    let base = format!("{}|{}|{}|{}", platform, trader, token_id, ts.to_rfc3339());
    match source_id {
        Some(s) if !s.is_empty() => format!("{base}|{s}"),
        _ => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_equal_across_calls() {
        let ts = Utc::now();
        let a = signal_id("polymarket", "0xabc", "tok1", ts, None);
        let b = signal_id("polymarket", "0xabc", "tok1", ts, None);
        assert_eq!(a, b);
        assert!(a.contains("polymarket|0xabc|tok1|"));
    }

    #[test]
    fn differs_on_token() {
        let ts = Utc::now();
        assert_ne!(
            signal_id("polymarket", "0xabc", "tok1", ts, None),
            signal_id("polymarket", "0xabc", "tok2", ts, None)
        );
    }

    #[test]
    fn signal_id_normalizes_chain_address() {
        let ts = Utc::now();
        assert_eq!(
            signal_id("polymarket", "0xAbC", "tok1", ts, None),
            signal_id("polymarket", "0xabc", "tok1", ts, None)
        );
    }

    #[test]
    fn source_id_disambiguates_same_second_trades() {
        // 同一秒同 token 两笔成交：无 source_id 会撞键，加 source_id 后分离
        let ts = Utc::now();
        assert_ne!(
            signal_id("polymarket", "0xabc", "tok1", ts, Some("trade-A")),
            signal_id("polymarket", "0xabc", "tok1", ts, Some("trade-B"))
        );
    }

    #[test]
    fn source_id_none_vs_some_never_collide() {
        // diff（None）与 trades（Some）段数不同 → 永不碰撞，跨源去重交给覆盖检查
        let ts = Utc::now();
        assert_ne!(
            signal_id("polymarket", "0xabc", "tok1", ts, None),
            signal_id("polymarket", "0xabc", "tok1", ts, Some("trade-A"))
        );
    }

    #[test]
    fn empty_source_id_treated_as_none() {
        let ts = Utc::now();
        assert_eq!(
            signal_id("polymarket", "0xabc", "tok1", ts, None),
            signal_id("polymarket", "0xabc", "tok1", ts, Some(""))
        );
    }
}
