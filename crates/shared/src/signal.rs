//! 信号去重键。venue-hub（outbox 落表）与 follow（copy_order 去重）共用同一算法，
//! 保证 outbox 重发同一信号时 follow 侧不重复派生 copy_order。
//!
//! 对应 H4 修复：信号投递从「fire-and-forget + warn」升级为「outbox + 重发 + 幂等」。

use chrono::{DateTime, Utc};

/// 信号去重键：`platform|trader_id|token_id|ts(rfc3339)`。
///
/// 确定性、跨进程一致：同一 (platform, trader, token, ts) 永远产出同一 key。
/// `ts` 用 RFC3339 字符串以避免浮点/时区漂移。venue-hub hot worker 检出信号时与
/// follow `/internal/signals` 派生时各算一次，须完全一致。
pub fn signal_id(
    platform: &str,
    trader_id: &str,
    token_id: &str,
    ts: DateTime<Utc>,
) -> String {
    format!("{}|{}|{}|{}", platform, trader_id, token_id, ts.to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_equal_across_calls() {
        let ts = Utc::now();
        let a = signal_id("polymarket", "0xabc", "tok1", ts);
        let b = signal_id("polymarket", "0xabc", "tok1", ts);
        assert_eq!(a, b);
        assert!(a.contains("polymarket|0xabc|tok1|"));
    }

    #[test]
    fn differs_on_token() {
        let ts = Utc::now();
        assert_ne!(
            signal_id("polymarket", "0xabc", "tok1", ts),
            signal_id("polymarket", "0xabc", "tok2", ts)
        );
    }
}
