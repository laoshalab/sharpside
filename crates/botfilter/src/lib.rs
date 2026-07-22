//! Bot / 做市过滤——规则可解释版（一期，6 条规则）。
//!
//! 纯函数库：无 DB、无网络、deterministic。吃 worker 预聚合的 [`AggregatedStats`]，
//! 产出 [`BotFlags`]。被 venue-hub perf worker 调用，结果写入 `trader_tag`（`bot:*` 标签 +
//! evidence 入 `tag_attrs`），供排行榜 `include_bots=false` 过滤与跟单门控消费。
//!
//! 设计承诺（相对 Hashdive/Polydata 黑盒）：
//! - **每条规则可解释**：命中带 `evidence`（前端可下钻到具体计数/比率/阈值）。
//! - **每条规则有反例**：高频高胜率 / 真做市赚钱的钱包不命中，避免误伤真 skill。
//! - **阈值可调可审计**：全部阈值在 [`BotFilterConfig`]，运营后台改 `tag_rules` 后下次重算生效。
//!
//! 6 条规则（阈值默认值见 [`BotFilterConfig::default`]）：
//! 1. **HighFreqSymmetric**——高频且买卖近对称（做市 / scalper）。
//! 2. **WashTrade**——同 tx+token 买卖共存（wash / 对冲腿，借鉴 polyterm）。
//! 3. **RoundTripScalper**——大量短窗口 round-trip + 极短持仓。
//! 4. **TakerOnlyScalper**——大量 round-trip + 已结算胜率极低（无 edge churner）。
//! 5. **SizeConcentration**——大额成交集中于极少数 condition（pump / 单市做市）。
//! 6. **HighChurnNoEdge**——成交极高频 + 已结算胜率极低（高频噪声 bot）。
//!
//! 二期路线（需新数据源，暂不实现）：
//! - **MakerTakerSelfPair**——需 `raw_order_fills` 表（maker/taker 字段）。
//! - **KnownEntity**——需 `wallet_labels` 表（已知实体地址库）。

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 命中的规则类别。序列化为 snake_case（与 `as_snake_str` 一致，前端按字符串映射中文标签）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    /// 高频且买卖对称 → 做市 / scalper。
    HighFreqSymmetric,
    /// 同 tx+token 买卖共存 → wash trade。
    WashTrade,
    /// 大量短窗口 round-trip + 极短持仓 → scalper。
    RoundTripScalper,
    /// 大量 round-trip + 已结算胜率极低 → 无 edge 的 churner。
    TakerOnlyScalper,
    /// 大额成交集中于极少数 condition → pump / 单市做市 bot。
    SizeConcentration,
    /// 成交极高频 + 已结算胜率极低 → 高频噪声 bot。
    HighChurnNoEdge,
}

impl Rule {
    /// snake_case 名，用于 `trader_tag.tags` 的 `bot:*` 标签（如 `bot:wash_trade`）。
    pub fn as_snake_str(self) -> &'static str {
        match self {
            Self::HighFreqSymmetric => "high_freq_symmetric",
            Self::WashTrade => "wash_trade",
            Self::RoundTripScalper => "round_trip_scalper",
            Self::TakerOnlyScalper => "taker_only_scalper",
            Self::SizeConcentration => "size_concentration",
            Self::HighChurnNoEdge => "high_churn_no_edge",
        }
    }
}

/// 单条规则命中记录，含证据（前端可下钻）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleHit {
    pub rule: Rule,
    /// 该规则贡献的置信度 ∈ [0,1]。
    pub confidence: f64,
    /// 证据：触发该规则的具体指标快照（计数、比率等）。
    pub evidence: Value,
}

/// 钱包 bot 判定结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotFlags {
    pub is_bot: bool,
    /// 合成置信度 = min(1, Σ hit confidences)。
    pub confidence: f64,
    pub hit_rules: Vec<RuleHit>,
}

impl BotFlags {
    /// 未命中任何规则的安全默认。
    pub fn clean() -> Self {
        Self {
            is_bot: false,
            confidence: 0.0,
            hit_rules: Vec::new(),
        }
    }
}

/// 规则阈值（可调，审计时公开）。对应 `trader_hub.tag_rules` 表的 botfilter 行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotFilterConfig {
    /// HighFreqSymmetric：最小成交笔数。
    pub hf_min_trades: u64,
    /// HighFreqSymmetric：最小对称比 `1 - |buys-sells|/(buys+sells)`。
    pub hf_min_symmetric: f64,
    /// WashTrade：触发阈值（≥ 此数即命中）。
    pub wash_min_count: u64,
    /// WashTrade：confidence 达 1.0 时的 self_trade 数。
    pub wash_full_count: u64,
    /// RoundTripScalper：最小 round-trip 数。
    pub rt_min_round_trips: u64,
    /// RoundTripScalper：持仓中位时长上限（秒）。
    pub rt_max_hold_secs: i64,
    /// TakerOnlyScalper：最小 round-trip 数。
    pub tos_min_round_trips: u64,
    /// TakerOnlyScalper：判定需要的最小已结算样本（避免小样本误判）。
    pub tos_min_resolved: u64,
    /// TakerOnlyScalper：胜率上限（≤ 此值视为无 edge）。
    pub tos_max_win_rate: f64,
    /// SizeConcentration：distinct condition 数上限（≤ 此值视为集中）。
    pub sc_max_conditions: u64,
    /// SizeConcentration：大额成交笔数下限。
    pub sc_min_large_trades: u64,
    /// SizeConcentration：大额门槛（USDC，单笔 notional = size * price）。
    pub sc_large_notional: f64,
    /// HighChurnNoEdge：成交笔数下限。
    pub hc_min_trades: u64,
    /// HighChurnNoEdge：已结算样本下限。
    pub hc_min_resolved: u64,
    /// HighChurnNoEdge：胜率上限。
    pub hc_max_win_rate: f64,
    /// 合成 confidence ≥ 此值 → is_bot = true。
    pub bot_threshold: f64,
}

impl Default for BotFilterConfig {
    fn default() -> Self {
        Self {
            hf_min_trades: 500,
            hf_min_symmetric: 0.85,
            wash_min_count: 1,
            wash_full_count: 5,
            rt_min_round_trips: 50,
            rt_max_hold_secs: 60,
            tos_min_round_trips: 50,
            tos_min_resolved: 10,
            tos_max_win_rate: 0.3,
            sc_max_conditions: 2,
            sc_min_large_trades: 20,
            sc_large_notional: 5_000.0,
            hc_min_trades: 2000,
            hc_min_resolved: 20,
            hc_max_win_rate: 0.3,
            bot_threshold: 0.5,
        }
    }
}

/// worker 预聚合的输入契约。由 venue-hub perf worker 从 `raw_trades` +
/// `position_timeline` + `trader_performance` 聚合后传入。
///
/// 字段口径见 `docs/BOTFILTER_RULES.md`（待补）：所有计数基于某周期窗口内的成交。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AggregatedStats {
    /// 总成交笔数。
    pub n_trades: u64,
    /// BUY 笔数。
    pub n_buys: u64,
    /// SELL 笔数。
    pub n_sells: u64,
    /// 买卖对称比 `1 - |buys-sells|/(buys+sells)`。
    pub symmetric_ratio: f64,
    /// 同 (tx_hash, token_id) 买卖共存的对数（wash proxy）。
    pub self_trade_count: u64,
    /// round-trip 数（BUY→最近 SELL 配对，hold ≤ 1h 的配对数）。
    pub round_trips: u64,
    /// 全部 BUY→最近 SELL 配对 hold 的中位秒数；无配对 → -1（未知）。
    pub median_hold_secs: i64,
    /// distinct condition_id 数。
    pub unique_conditions: u64,
    /// 单笔 notional ≥ `sc_large_notional` 的成交数。
    pub large_trade_count: u64,
    /// 已结算 condition 数（wins + losses）。
    pub n_resolved: u64,
    /// 已结算且盈利的 condition 数。
    pub n_resolved_wins: u64,
}

impl AggregatedStats {
    /// 已结算胜率 = n_resolved_wins / n_resolved；无样本 → 0.0。
    pub fn win_rate(&self) -> f64 {
        if self.n_resolved == 0 {
            0.0
        } else {
            self.n_resolved_wins as f64 / self.n_resolved as f64
        }
    }
}

/// 用默认配置判定单钱包。
pub fn detect(stats: &AggregatedStats) -> BotFlags {
    detect_with(stats, &BotFilterConfig::default())
}

/// 用自定义配置判定单钱包。
pub fn detect_with(stats: &AggregatedStats, cfg: &BotFilterConfig) -> BotFlags {
    let mut hits: Vec<RuleHit> = Vec::new();
    if let Some(h) = rule_high_freq_symmetric(stats, cfg) {
        hits.push(h);
    }
    if let Some(h) = rule_wash_trade(stats, cfg) {
        hits.push(h);
    }
    if let Some(h) = rule_round_trip_scalper(stats, cfg) {
        hits.push(h);
    }
    if let Some(h) = rule_taker_only_scalper(stats, cfg) {
        hits.push(h);
    }
    if let Some(h) = rule_size_concentration(stats, cfg) {
        hits.push(h);
    }
    if let Some(h) = rule_high_churn_no_edge(stats, cfg) {
        hits.push(h);
    }
    let confidence = hits.iter().map(|h| h.confidence).sum::<f64>().min(1.0);
    let is_bot = confidence >= cfg.bot_threshold;
    BotFlags {
        is_bot,
        confidence,
        hit_rules: hits,
    }
}

// ─── v1 规则 ───

fn rule_high_freq_symmetric(stats: &AggregatedStats, cfg: &BotFilterConfig) -> Option<RuleHit> {
    if stats.n_trades < cfg.hf_min_trades || stats.symmetric_ratio < cfg.hf_min_symmetric {
        return None;
    }
    let conf = ((stats.symmetric_ratio - cfg.hf_min_symmetric) / (1.0 - cfg.hf_min_symmetric))
        .clamp(0.0, 1.0);
    Some(RuleHit {
        rule: Rule::HighFreqSymmetric,
        confidence: conf.max(0.5),
        evidence: serde_json::json!({
            "n_trades": stats.n_trades,
            "n_buys": stats.n_buys,
            "n_sells": stats.n_sells,
            "symmetric_ratio": stats.symmetric_ratio,
            "thresholds": {
                "hf_min_trades": cfg.hf_min_trades,
                "hf_min_symmetric": cfg.hf_min_symmetric,
            },
        }),
    })
}

fn rule_wash_trade(stats: &AggregatedStats, cfg: &BotFilterConfig) -> Option<RuleHit> {
    if stats.self_trade_count < cfg.wash_min_count {
        return None;
    }
    let conf = (stats.self_trade_count as f64 / cfg.wash_full_count as f64).clamp(0.0, 1.0);
    Some(RuleHit {
        rule: Rule::WashTrade,
        confidence: conf.max(0.5),
        evidence: serde_json::json!({
            "self_trade_count": stats.self_trade_count,
            "thresholds": { "wash_min_count": cfg.wash_min_count },
        }),
    })
}

fn rule_round_trip_scalper(stats: &AggregatedStats, cfg: &BotFilterConfig) -> Option<RuleHit> {
    if stats.round_trips < cfg.rt_min_round_trips {
        return None;
    }
    let hold_ok = stats.median_hold_secs >= 0 && stats.median_hold_secs <= cfg.rt_max_hold_secs;
    if !hold_ok && stats.median_hold_secs >= 0 {
        return None;
    }
    let rt_conf = ((stats.round_trips - cfg.rt_min_round_trips) as f64
        / cfg.rt_min_round_trips as f64)
        .clamp(0.0, 1.0);
    let conf = if stats.median_hold_secs < 0 {
        rt_conf.min(0.3)
    } else {
        rt_conf.max(0.5)
    };
    Some(RuleHit {
        rule: Rule::RoundTripScalper,
        confidence: conf,
        evidence: serde_json::json!({
            "round_trips": stats.round_trips,
            "median_hold_secs": stats.median_hold_secs,
            "thresholds": {
                "rt_min_round_trips": cfg.rt_min_round_trips,
                "rt_max_hold_secs": cfg.rt_max_hold_secs,
            },
        }),
    })
}

// ─── v2 规则 ───

/// TakerOnlyScalper：大量 round-trip + 已结算胜率极低。
/// 反例：高频 round-trip 但高胜率（真 skill）→ win_rate 上限把 skill 钱包排除。
fn rule_taker_only_scalper(stats: &AggregatedStats, cfg: &BotFilterConfig) -> Option<RuleHit> {
    if stats.round_trips < cfg.tos_min_round_trips || stats.n_resolved < cfg.tos_min_resolved {
        return None;
    }
    let wr = stats.win_rate();
    if wr > cfg.tos_max_win_rate {
        return None;
    }
    let wr_factor = ((cfg.tos_max_win_rate - wr) / cfg.tos_max_win_rate).clamp(0.0, 1.0);
    let rt_factor = ((stats.round_trips - cfg.tos_min_round_trips) as f64
        / cfg.tos_min_round_trips as f64)
        .clamp(0.0, 1.0);
    let conf = (0.6 * wr_factor + 0.4 * rt_factor).clamp(0.5, 1.0);
    Some(RuleHit {
        rule: Rule::TakerOnlyScalper,
        confidence: conf,
        evidence: serde_json::json!({
            "round_trips": stats.round_trips,
            "n_resolved": stats.n_resolved,
            "n_resolved_wins": stats.n_resolved_wins,
            "win_rate": wr,
            "thresholds": {
                "tos_min_round_trips": cfg.tos_min_round_trips,
                "tos_min_resolved": cfg.tos_min_resolved,
                "tos_max_win_rate": cfg.tos_max_win_rate,
            },
        }),
    })
}

/// SizeConcentration：大额成交集中于极少数 condition。
/// 反例：在少数市场大额下注但高胜率（conviction 鲸鱼）——confidence cap 0.4，单独不触发 is_bot。
fn rule_size_concentration(stats: &AggregatedStats, cfg: &BotFilterConfig) -> Option<RuleHit> {
    if stats.unique_conditions == 0
        || stats.unique_conditions > cfg.sc_max_conditions
        || stats.large_trade_count < cfg.sc_min_large_trades
    {
        return None;
    }
    let large_factor = ((stats.large_trade_count - cfg.sc_min_large_trades) as f64
        / cfg.sc_min_large_trades as f64)
        .clamp(0.0, 1.0);
    let conc_factor = 1.0
        - ((stats.unique_conditions - 1) as f64 / cfg.sc_max_conditions.max(1) as f64)
            .clamp(0.0, 1.0);
    let conf = (0.5 * large_factor + 0.5 * conc_factor).clamp(0.0, 0.4);
    Some(RuleHit {
        rule: Rule::SizeConcentration,
        confidence: conf,
        evidence: serde_json::json!({
            "unique_conditions": stats.unique_conditions,
            "large_trade_count": stats.large_trade_count,
            "thresholds": {
                "sc_max_conditions": cfg.sc_max_conditions,
                "sc_min_large_trades": cfg.sc_min_large_trades,
                "sc_large_notional": cfg.sc_large_notional,
            },
        }),
    })
}

/// HighChurnNoEdge：成交极高频 + 已结算胜率极低。
/// 反例：高频高胜率（做市赚钱或真 edge）→ 不命中。
fn rule_high_churn_no_edge(stats: &AggregatedStats, cfg: &BotFilterConfig) -> Option<RuleHit> {
    if stats.n_trades < cfg.hc_min_trades || stats.n_resolved < cfg.hc_min_resolved {
        return None;
    }
    let wr = stats.win_rate();
    if wr > cfg.hc_max_win_rate {
        return None;
    }
    let wr_factor = ((cfg.hc_max_win_rate - wr) / cfg.hc_max_win_rate).clamp(0.0, 1.0);
    let vol_factor =
        ((stats.n_trades - cfg.hc_min_trades) as f64 / cfg.hc_min_trades as f64).clamp(0.0, 1.0);
    let conf = (0.6 * wr_factor + 0.4 * vol_factor).clamp(0.5, 1.0);
    Some(RuleHit {
        rule: Rule::HighChurnNoEdge,
        confidence: conf,
        evidence: serde_json::json!({
            "n_trades": stats.n_trades,
            "n_resolved": stats.n_resolved,
            "n_resolved_wins": stats.n_resolved_wins,
            "win_rate": wr,
            "thresholds": {
                "hc_min_trades": cfg.hc_min_trades,
                "hc_min_resolved": cfg.hc_min_resolved,
                "hc_max_win_rate": cfg.hc_max_win_rate,
            },
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats() -> AggregatedStats {
        AggregatedStats::default()
    }

    // ─── v1 规则 ───

    /// 高频对称做市钱包：2000 笔，buys/sells 近 1:1 → 命中且 is_bot。
    #[test]
    fn high_freq_symmetric_flagged() {
        let mut s = stats();
        s.n_trades = 2000;
        s.n_buys = 1010;
        s.n_sells = 990;
        s.symmetric_ratio = 1.0 - (20.0 / 2000.0); // 0.99
        let f = detect(&s);
        assert!(f.is_bot);
        assert!(f
            .hit_rules
            .iter()
            .any(|h| h.rule == Rule::HighFreqSymmetric));
    }

    /// 正常长线钱包：成交少、不对称 → clean。
    #[test]
    fn normal_wallet_not_flagged() {
        let mut s = stats();
        s.n_trades = 30;
        s.n_buys = 25;
        s.n_sells = 5;
        s.symmetric_ratio = 1.0 - (20.0 / 30.0); // ~0.33
        s.n_resolved = 12;
        s.n_resolved_wins = 8;
        let f = detect(&s);
        assert!(!f.is_bot);
        assert!(f.hit_rules.is_empty());
    }

    /// Wash trade：3 条同 tx+token 对冲腿 → 命中且 is_bot。
    #[test]
    fn wash_trade_flagged() {
        let mut s = stats();
        s.self_trade_count = 3;
        let f = detect(&s);
        assert!(f.is_bot);
        assert!(f.hit_rules.iter().any(|h| h.rule == Rule::WashTrade));
    }

    /// Round-trip scalper：80 round-trips + 中位持仓 10s → 命中且 is_bot。
    #[test]
    fn round_trip_scalper_flagged() {
        let mut s = stats();
        s.round_trips = 80;
        s.median_hold_secs = 10;
        let f = detect(&s);
        assert!(f.is_bot);
        assert!(f.hit_rules.iter().any(|h| h.rule == Rule::RoundTripScalper));
    }

    /// round-trip 量足但持仓时长未知 → 弱命中（≤ 0.3），不触发 is_bot。
    #[test]
    fn round_trip_unknown_hold_is_weak() {
        let mut s = stats();
        s.round_trips = 80;
        s.median_hold_secs = -1;
        let f = detect(&s);
        let rt = f
            .hit_rules
            .iter()
            .find(|h| h.rule == Rule::RoundTripScalper);
        assert!(rt.is_some(), "should still hit weakly");
        assert!(rt.unwrap().confidence <= 0.3);
        assert!(!f.is_bot, "weak signal alone must not flag bot");
    }

    /// 两条弱信号合成超过阈值 → is_bot（多因子叠加）。
    #[test]
    fn combined_weak_signals_flag() {
        let mut s = stats();
        s.n_trades = 500;
        s.n_buys = 252;
        s.n_sells = 248;
        s.symmetric_ratio = 1.0 - (4.0 / 500.0); // 0.992
        s.self_trade_count = 1;
        let f = detect(&s);
        assert!(f.is_bot);
        assert_eq!(f.hit_rules.len(), 2);
    }

    // ─── v2 规则 ───

    /// TakerOnlyScalper：100 round-trips + 已结算 20 仅 4 胜（0.2 ≤ 0.3）→ 命中且 is_bot。
    #[test]
    fn taker_only_scalper_flagged() {
        let mut s = stats();
        s.round_trips = 100;
        s.n_resolved = 20;
        s.n_resolved_wins = 4; // 0.2
        let f = detect(&s);
        assert!(f.hit_rules.iter().any(|h| h.rule == Rule::TakerOnlyScalper));
        assert!(f.is_bot);
    }

    /// TakerOnly 反例：高频 round-trip 但高胜率（真 skill）→ 不命中。
    #[test]
    fn taker_only_skill_wallet_not_flagged() {
        let mut s = stats();
        s.round_trips = 100;
        s.n_resolved = 20;
        s.n_resolved_wins = 15; // 0.75 > 0.3
        let f = detect(&s);
        assert!(!f.hit_rules.iter().any(|h| h.rule == Rule::TakerOnlyScalper));
    }

    /// SizeConcentration：1 个 condition + 40 笔大额 → 弱命中（≤ 0.4），单独不触发 is_bot。
    #[test]
    fn size_concentration_weak_signal() {
        let mut s = stats();
        s.unique_conditions = 1;
        s.large_trade_count = 40;
        let f = detect(&s);
        let sc = f
            .hit_rules
            .iter()
            .find(|h| h.rule == Rule::SizeConcentration);
        assert!(sc.is_some());
        assert!(sc.unwrap().confidence <= 0.4);
        assert!(!f.is_bot, "size concentration alone must not flag bot");
    }

    /// SizeConcentration 反例：diversified 钱包 unique_conditions=8 → 不命中。
    #[test]
    fn size_concentration_diversified_not_flagged() {
        let mut s = stats();
        s.unique_conditions = 8;
        s.large_trade_count = 40;
        let f = detect(&s);
        assert!(!f
            .hit_rules
            .iter()
            .any(|h| h.rule == Rule::SizeConcentration));
    }

    /// HighChurnNoEdge：3000 笔 + 已结算 30 仅 6 胜（0.2）→ 命中且 is_bot。
    #[test]
    fn high_churn_no_edge_flagged() {
        let mut s = stats();
        s.n_trades = 3000;
        s.n_resolved = 30;
        s.n_resolved_wins = 6; // 0.2
        let f = detect(&s);
        assert!(f.hit_rules.iter().any(|h| h.rule == Rule::HighChurnNoEdge));
        assert!(f.is_bot);
    }

    /// HighChurn 反例：高频但高胜率 → 不命中。
    #[test]
    fn high_churn_skill_not_flagged() {
        let mut s = stats();
        s.n_trades = 3000;
        s.n_resolved = 30;
        s.n_resolved_wins = 22; // 0.73 > 0.3
        let f = detect(&s);
        assert!(!f.hit_rules.iter().any(|h| h.rule == Rule::HighChurnNoEdge));
    }

    // ─── 通用 ───

    /// win_rate helper 正常工作。
    #[test]
    fn win_rate_helper() {
        let mut s = stats();
        s.n_resolved = 10;
        s.n_resolved_wins = 7;
        assert!((s.win_rate() - 0.7).abs() < 1e-9);
        s.n_resolved = 0;
        assert_eq!(s.win_rate(), 0.0);
    }

    /// 自定义配置可收紧/放宽阈值。
    #[test]
    fn config_is_tunable() {
        let s = AggregatedStats {
            n_trades: 100,
            n_buys: 51,
            n_sells: 49,
            symmetric_ratio: 1.0 - (2.0 / 100.0), // 0.98
            ..stats()
        };
        assert!(!detect(&s).is_bot, "默认阈值 500 → 不命中");
        let cfg = BotFilterConfig {
            hf_min_trades: 50,
            ..BotFilterConfig::default()
        };
        assert!(detect_with(&s, &cfg).is_bot, "收紧到 50 → 命中");
    }

    /// BotFlags 可序列化（worker 落 `trader_tag.tag_attrs` 用）。
    #[test]
    fn bot_flags_serialize_roundtrip() {
        let f = BotFlags {
            is_bot: true,
            confidence: 0.75,
            hit_rules: vec![RuleHit {
                rule: Rule::WashTrade,
                confidence: 0.6,
                evidence: serde_json::json!({"self_trade_count": 3}),
            }],
        };
        let s = serde_json::to_string(&f).unwrap();
        let back: BotFlags = serde_json::from_str(&s).unwrap();
        assert!(back.is_bot);
        assert_eq!(back.confidence, 0.75);
        assert_eq!(back.hit_rules[0].rule, Rule::WashTrade);
    }
}
