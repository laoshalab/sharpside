//! Polymarket API 响应 DTO。对应 `docs/DATA_SOURCES.md` §2-§4。
//!
//! 字段名按官方 API 的 camelCase，用 `#[serde(rename)]` 映射到 snake_case Rust 字段。
//! 可选字段用 `Option` + `#[serde(default)]`，应对 API 字段漂移。

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// 反序列化「字符串数组」字段，兼容两种 Gamma 返回形状：
/// - 原生 JSON 数组 `["Yes","No"]`
/// - 字符串化的 JSON 数组 `"[\"Yes\",\"No\"]"`（Gamma `/markets` 实际返回此形式）
///
/// 空字符串 / null → None。
fn deserialize_string_array<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    Ok(opt.and_then(|v| match v {
        serde_json::Value::Array(a) => Some(
            a.into_iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect(),
        ),
        serde_json::Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                serde_json::from_str::<Vec<String>>(trimmed).ok()
            }
        }
        _ => None,
    }))
}

/// 排行榜条目。对应 `docs/DATA_SOURCES.md` §3.1 `TraderLeaderboardEntry`。
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct LeaderboardEntry {
    #[serde(rename = "proxyWallet")]
    pub proxy_wallet: String,
    #[serde(rename = "userName", default)]
    pub user_name: Option<String>,
    #[serde(default)]
    pub vol: Option<f64>,
    #[serde(default)]
    pub pnl: Option<f64>,
    #[serde(rename = "profileImage", default)]
    pub profile_image: Option<String>,
    #[serde(rename = "xUsername", default)]
    pub x_username: Option<String>,
    #[serde(rename = "verifiedBadge", default)]
    pub verified_badge: Option<bool>,
    #[serde(default)]
    pub rank: Option<String>,
}

/// 持仓（Data API `/positions`）。
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PositionDto {
    /// 钱包地址
    #[serde(rename = "user", default)]
    pub user: Option<String>,
    /// market condition_id
    #[serde(rename = "market", default)]
    pub market: Option<String>,
    /// token_id（asset）
    #[serde(rename = "asset", default)]
    pub asset: Option<String>,
    #[serde(default)]
    pub size: Option<f64>,
    #[serde(rename = "avgPrice", default)]
    pub avg_price: Option<f64>,
    #[serde(rename = "currentPrice", default)]
    pub current_price: Option<f64>,
    #[serde(rename = "realizedPnl", default)]
    pub realized_pnl: Option<f64>,
    /// YES / NO
    #[serde(default)]
    pub side: Option<String>,
}

/// 组合估值（Data API `/value`）。当前快照，非时间序列。
/// `value` = 持仓总 USD 估值。worker 周期快照积累历史，按周期算 delta 近似官方盈亏。
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ValueDto {
    #[serde(rename = "user", default)]
    pub user: Option<String>,
    #[serde(default)]
    pub value: Option<f64>,
}

/// 成交（Data API `/trades`）。
///
/// 真实 API 与 mock 字段差异：
/// - `timestamp`：真实返数字（Unix 秒），mock 返字符串 → `deserialize_ts` 兼容两者。
/// - 市场 ID：真实用 `conditionId`，mock 用 `market` → 两个字段都收，`map_trade` 取 `market.or(condition_id)`。
/// - 拥有者：真实用 `proxyWallet`，mock 用 `tradeOwner` → `map_trade` 不读此字段（用 `trader_id` 参数），保留兼容。
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct TradeDto {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(rename = "takerSide", default)]
    pub taker_side: Option<String>,
    #[serde(default)]
    pub side: Option<String>,
    #[serde(default)]
    pub size: Option<f64>,
    #[serde(default)]
    pub price: Option<f64>,
    /// Unix 秒（字符串或数字）。
    #[serde(default, deserialize_with = "deserialize_ts")]
    pub timestamp: Option<String>,
    /// mock 用 `market`，真实 API 用 `conditionId`。
    #[serde(default)]
    pub market: Option<String>,
    #[serde(rename = "conditionId", default)]
    pub condition_id: Option<String>,
    #[serde(rename = "asset", default)]
    pub asset: Option<String>,
    #[serde(rename = "tradeOwner", default)]
    pub trade_owner: Option<String>,
    /// 真实 API 用 `proxyWallet` 标识成交拥有者（`tradeOwner` 的别名，map_trade 不读）。
    #[serde(rename = "proxyWallet", default)]
    pub proxy_wallet: Option<String>,
    /// 真实 API 用 `transactionHash` 做唯一键（mock 用 `id`）。
    #[serde(rename = "transactionHash", default)]
    pub transaction_hash: Option<String>,
}

/// 反序列化 timestamp，兼容字符串与数字（Unix 秒）。空/null → None。
fn deserialize_ts<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    Ok(opt.and_then(|v| match v {
        serde_json::Value::String(s) => Some(s),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }))
}

/// 市场（Gamma API `/markets`）。
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct MarketDto {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(rename = "conditionId", default)]
    pub condition_id: Option<String>,
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default, deserialize_with = "deserialize_string_array")]
    pub tags: Option<Vec<String>>,
    #[serde(rename = "endDate", default)]
    pub end_date: Option<DateTime<Utc>>,
    #[serde(default, deserialize_with = "deserialize_string_array")]
    pub outcomes: Option<Vec<String>>,
    /// 市场是否已结算（Gamma `/markets` 的 `closed`）。赎回 worker 据此扫新结算市场。
    /// 缺省 false（未结算）。
    #[serde(default)]
    pub closed: bool,
}

/// 订单簿深度（CLOB API `/book`）。
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BookDto {
    #[serde(default)]
    pub bids: Vec<BookLevelDto>,
    #[serde(default)]
    pub asks: Vec<BookLevelDto>,
    /// CLOB `/book?token_id=` 返回的 `market` = condition_id（用于查 `/markets/{condition_id}` 取 neg_risk）。
    #[serde(default)]
    pub market: Option<String>,
    #[serde(rename = "asset_id", default)]
    pub asset_id: Option<String>,
}

/// CLOB `/markets/{condition_id}` 元数据（只取下单所需字段）。
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ClobMarketDto {
    #[serde(rename = "condition_id", default)]
    pub condition_id: Option<String>,
    /// 是否 neg-risk 市场（决定 V2 Order EIP-712 verifyingContract：standard vs neg-risk 合约）。
    /// CLOB `/markets` 返回 `neg_risk`（snake_case）。缺省 false（standard）。
    #[serde(rename = "neg_risk", default)]
    pub neg_risk: bool,
    #[serde(default)]
    pub active: bool,
    #[serde(rename = "accepting_orders", default)]
    pub accepting_orders: bool,
    /// 最小下单股数（每市场不同，Polymarket 服务端强制；size < 此值 → 400 拒单）。
    /// 用于风控层下单前校验，避免撞服务端 400。缺省 0 = 未知/不限。
    #[serde(rename = "minimum_order_size", default)]
    pub minimum_order_size: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BookLevelDto {
    #[serde(default)]
    pub price: Option<String>,
    #[serde(default)]
    pub size: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaderboard_entry_deserialize() {
        let json = r#"{
            "rank": "1",
            "proxyWallet": "0xabc",
            "userName": "whale",
            "vol": 12345.6,
            "pnl": 678.9,
            "profileImage": "https://img/1.png",
            "xUsername": "whale_x",
            "verifiedBadge": true
        }"#;
        let e: LeaderboardEntry = serde_json::from_str(json).unwrap();
        assert_eq!(e.proxy_wallet, "0xabc");
        assert_eq!(e.user_name.as_deref(), Some("whale"));
        assert!((e.vol.unwrap() - 12345.6).abs() < 1e-9);
        assert!(e.verified_badge.unwrap());
    }

    #[test]
    fn leaderboard_entry_minimal() {
        // 缺字段应回退默认
        let json = r#"{ "proxyWallet": "0xdef" }"#;
        let e: LeaderboardEntry = serde_json::from_str(json).unwrap();
        assert_eq!(e.proxy_wallet, "0xdef");
        assert!(e.user_name.is_none());
        assert!(e.pnl.is_none());
    }

    #[test]
    fn position_dto_deserialize() {
        let json = r#"{
            "user": "0xabc",
            "market": "0xcond",
            "asset": "12345",
            "size": 100.0,
            "avgPrice": 0.5,
            "currentPrice": 0.6,
            "realizedPnl": 10.0,
            "side": "YES"
        }"#;
        let p: PositionDto = serde_json::from_str(json).unwrap();
        assert_eq!(p.user.as_deref(), Some("0xabc"));
        assert!((p.size.unwrap() - 100.0).abs() < 1e-9);
        assert_eq!(p.side.as_deref(), Some("YES"));
    }

    #[test]
    fn trade_dto_deserialize() {
        let json = r#"{
            "id": "t1",
            "side": "BUY",
            "size": 50.0,
            "price": 0.42,
            "timestamp": "1700000000",
            "market": "0xcond",
            "asset": "12345",
            "tradeOwner": "0xabc"
        }"#;
        let t: TradeDto = serde_json::from_str(json).unwrap();
        assert_eq!(t.id.as_deref(), Some("t1"));
        assert_eq!(t.timestamp.as_deref(), Some("1700000000"));
        assert_eq!(t.trade_owner.as_deref(), Some("0xabc"));
    }

    #[test]
    fn market_dto_deserialize() {
        let json = r#"{
            "id": "m1",
            "conditionId": "0xcond",
            "question": "Will Trump win?",
            "slug": "trump-win",
            "tags": ["politics", "usa"],
            "endDate": "2026-11-05T00:00:00Z",
            "outcomes": ["Yes", "No"]
        }"#;
        let m: MarketDto = serde_json::from_str(json).unwrap();
        assert_eq!(m.condition_id.as_deref(), Some("0xcond"));
        assert_eq!(m.question.as_deref(), Some("Will Trump win?"));
        assert_eq!(m.tags.as_ref().unwrap().len(), 2);
        assert!(m.end_date.is_some());
    }

    #[test]
    fn market_dto_outcomes_stringified_array() {
        // Gamma `/markets` 实际返回 outcomes/tags 为字符串化 JSON 数组（非原生数组）。
        let json = r#"{
            "id": "m1",
            "conditionId": "0xcond",
            "tags": "[\"politics\", \"usa\"]",
            "outcomes": "[\"Yes\", \"No\"]"
        }"#;
        let m: MarketDto = serde_json::from_str(json).unwrap();
        assert_eq!(
            m.outcomes.as_deref(),
            Some(&["Yes".to_string(), "No".to_string()][..])
        );
        assert_eq!(m.tags.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn market_dto_closed_default_false_and_parses_true() {
        // 缺 closed 字段 → 默认 false（未结算）。
        let json = r#"{"id":"m1","conditionId":"0xcond","question":"q"}"#;
        let m: MarketDto = serde_json::from_str(json).unwrap();
        assert!(!m.closed);

        // closed=true（已结算）应正确解析。
        let json = r#"{"id":"m1","conditionId":"0xcond","question":"q","closed":true}"#;
        let m: MarketDto = serde_json::from_str(json).unwrap();
        assert!(m.closed);
    }

    #[test]
    fn book_dto_deserialize() {
        let json = r#"{
            "market": "0xcond",
            "asset_id": "12345",
            "bids": [{"price": "0.49", "size": "100"}],
            "asks": [{"price": "0.51", "size": "200"}]
        }"#;
        let b: BookDto = serde_json::from_str(json).unwrap();
        assert_eq!(b.bids.len(), 1);
        assert_eq!(b.bids[0].price.as_deref(), Some("0.49"));
        assert_eq!(b.asks[0].size.as_deref(), Some("200"));
        assert_eq!(b.market.as_deref(), Some("0xcond"));
        assert_eq!(b.asset_id.as_deref(), Some("12345"));
    }

    #[test]
    fn clob_market_dto_neg_risk() {
        // CLOB `/markets/{condition_id}` 返回 `neg_risk`（snake_case）。
        let j =
            r#"{"condition_id":"0xcond","neg_risk":true,"active":true,"accepting_orders":true}"#;
        let m: ClobMarketDto = serde_json::from_str(j).unwrap();
        assert!(m.neg_risk);
        assert!(m.active);
        // 缺省 neg_risk → false（standard）
        let j2 = r#"{"condition_id":"0xcond"}"#;
        let m2: ClobMarketDto = serde_json::from_str(j2).unwrap();
        assert!(!m2.neg_risk);
    }
}
