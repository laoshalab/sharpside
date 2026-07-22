//! 交易者标签：`Tag` / `TagKind`。
//!
//! 对应 `docs/PERFORMANCE_PIPELINE.md` §4.4 与 `docs/VENUEHUB_STORAGE.md` §6 `trader_tag` 表。
//! 标签阈值从 `tag_rules` 表读，运营后台可调，不硬编码。

use serde::{Deserialize, Serialize};

/// 标签种类。对应 `trader_tag.tags` text[] 取值。
///
/// - `DW:*` — 持有型/高胜率型（Diamond/Win）
/// - `type-3:*` — 交易手法型（限价狙击/市价跟随/再平衡）
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagKind {
    /// 持有型：median(holding_seconds) > 24h
    #[serde(rename = "DW:diamond")]
    DwDiamond,
    /// 高胜率型：win_rate > 60% 且 roi > 0
    #[serde(rename = "DW:win")]
    DwWin,
    /// 限价狙击：限价单占比 > 70% 且 fill 时长 < 2 block
    #[serde(rename = "type-3:limit_sniper")]
    Type3LimitSniper,
    /// 市价跟随：市价单占比 > 70%
    #[serde(rename = "type-3:market_follow")]
    Type3MarketFollow,
    /// 再平衡：同 token_id 单日反向交易次数 > 阈值
    #[serde(rename = "type-3:rebalance")]
    Type3Rebalance,
}

impl TagKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DwDiamond => "DW:diamond",
            Self::DwWin => "DW:win",
            Self::Type3LimitSniper => "type-3:limit_sniper",
            Self::Type3MarketFollow => "type-3:market_follow",
            Self::Type3Rebalance => "type-3:rebalance",
        }
    }

    /// 是否属于 DW 家族。
    pub fn is_dw(&self) -> bool {
        matches!(self, Self::DwDiamond | Self::DwWin)
    }

    /// 是否属于 type-3 家族。
    pub fn is_type3(&self) -> bool {
        matches!(
            self,
            Self::Type3LimitSniper | Self::Type3MarketFollow | Self::Type3Rebalance
        )
    }
}

/// 单个标签实例，含打标依据（jsonb）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub kind: TagKind,
    /// 打标依据（如 holding_seconds 中位数、限价单占比等），写入 `trader_tag.tag_attrs`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attrs: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_kind_serde_round_trip() {
        for k in [
            TagKind::DwDiamond,
            TagKind::DwWin,
            TagKind::Type3LimitSniper,
            TagKind::Type3MarketFollow,
            TagKind::Type3Rebalance,
        ] {
            let json = serde_json::to_string(&k).unwrap();
            let back: TagKind = serde_json::from_str(&json).unwrap();
            assert_eq!(k, back);
        }
    }

    #[test]
    fn tag_kind_families() {
        assert!(TagKind::DwDiamond.is_dw());
        assert!(TagKind::DwWin.is_dw());
        assert!(!TagKind::DwWin.is_type3());
        assert!(TagKind::Type3LimitSniper.is_type3());
        assert!(!TagKind::Type3LimitSniper.is_dw());
    }

    #[test]
    fn tag_with_attrs_serde() {
        let t = Tag {
            kind: TagKind::DwDiamond,
            attrs: Some(serde_json::json!({"median_holding_seconds": 172800})),
        };
        let json = serde_json::to_string(&t).unwrap();
        let back: Tag = serde_json::from_str(&json).unwrap();
        assert_eq!(t.kind, back.kind);
        assert!(back.attrs.is_some());
    }
}
