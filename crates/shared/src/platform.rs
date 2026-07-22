//! `Platform` 枚举：Venue 一等公民标识。
//!
//! 对应 `docs/VENUE_DESIGN.md` §2 与 `docs/ARCHITECTURE.md` §7。
//! 一个 `Platform` 变体 = 一个预测市场平台。新增平台 = 新增变体 + 实现 `Venue` trait。

use serde::{Deserialize, Serialize};

/// 预测市场平台标识。
///
/// 序列化为 snake_case 字符串，与 DB `platform` 列、API 路径参数、
/// `traders` 复合主键 `(platform, address)` 中的 `platform` 取值一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    /// Polymarket — Polygon 链上 USDC CTF，signal + execution
    Polymarket,
    /// Kalshi — CFTC 监管 USD 法币，execution only（无交易者数据）
    Kalshi,
    /// Manifold — 玩钱 mana，signal only
    Manifold,
    /// Zeitgeist — Polkadot 链上，signal + execution（Phase 4）
    Zeitgeist,
    /// Azuro — 多链体育，signal + execution（Phase 4）
    Azuro,
}

impl Platform {
    /// 序列化键名，用于 DB 列值与 API 路径。等价于 `serde_json::to_string` 去引号。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Polymarket => "polymarket",
            Self::Kalshi => "kalshi",
            Self::Manifold => "manifold",
            Self::Zeitgeist => "zeitgeist",
            Self::Azuro => "azuro",
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Platform {
    type Err = UnknownPlatform;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "polymarket" => Ok(Self::Polymarket),
            "kalshi" => Ok(Self::Kalshi),
            "manifold" => Ok(Self::Manifold),
            "zeitgeist" => Ok(Self::Zeitgeist),
            "azuro" => Ok(Self::Azuro),
            other => Err(UnknownPlatform(other.into())),
        }
    }
}

/// 未知平台字符串错误。
#[derive(Debug, thiserror::Error)]
#[error("unknown platform: {0}")]
pub struct UnknownPlatform(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_round_trip() {
        for p in [
            Platform::Polymarket,
            Platform::Kalshi,
            Platform::Manifold,
            Platform::Zeitgeist,
            Platform::Azuro,
        ] {
            let json = serde_json::to_string(&p).unwrap();
            let back: Platform = serde_json::from_str(&json).unwrap();
            assert_eq!(p, back);
        }
    }

    #[test]
    fn snake_case_serialization() {
        assert_eq!(
            serde_json::to_string(&Platform::Polymarket).unwrap(),
            "\"polymarket\""
        );
        assert_eq!(
            serde_json::to_string(&Platform::Zeitgeist).unwrap(),
            "\"zeitgeist\""
        );
    }

    #[test]
    fn from_str_round_trip() {
        for p in [
            Platform::Polymarket,
            Platform::Kalshi,
            Platform::Manifold,
            Platform::Zeitgeist,
            Platform::Azuro,
        ] {
            let s = p.as_str();
            assert_eq!(s.parse::<Platform>().unwrap(), p);
        }
    }

    #[test]
    fn unknown_platform_errors() {
        assert!("predictit".parse::<Platform>().is_err());
    }
}
