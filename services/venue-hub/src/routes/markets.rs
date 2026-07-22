//! `GET /markets` / `GET /market-mappings`。对应 `docs/ARCHITECTURE.md` §6.1 / §8.1。
//!
//! `GET /markets` 优先读 `raw_markets` 缓存（ingest worker 已抓取），未命中时回源 Venue。
//! `GET /market-mappings` 读 `market_mappings`（跨 Venue 跟单只读 verified 的映射）。

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::Query;
use axum::Json;
use serde::{Deserialize, Serialize};
use sharpside_db::queries::mappings as mapping_q;
use sharpside_db::queries::raw;
use sharpside_venues_core::{Market, MarketQuery};

#[derive(Debug, Deserialize)]
pub struct MarketsQuery {
    pub platform: Option<String>,
    pub q: Option<String>,
    pub limit: Option<u32>,
}

/// 对外市场摘要（从 raw_markades 映射，隐藏 raw_json）。
#[derive(Debug, Serialize)]
pub struct MarketOut {
    pub platform: String,
    pub venue_market_id: String,
    pub title: String,
    pub slug: Option<String>,
    pub tags: Vec<String>,
    pub category: Option<String>,
    pub end_date: Option<chrono::DateTime<chrono::Utc>>,
}

/// `GET /markets?platform=&q=` — 市场搜索。
///
/// 先读 `raw_markets` 缓存（按 platform 过滤 + 标题模糊匹配）；
/// 若该 platform 无任何缓存且 Venue 已注册，则回源拉取并 upsert 缓存后返回。
pub async fn list_markets(
    state: AppState,
    Query(q): Query<MarketsQuery>,
) -> Result<Json<Vec<MarketOut>>, ApiError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let platform = q.platform.as_deref().unwrap_or("polymarket");

    let cached = raw::list_raw_markets(&state.db, platform).await?;
    let mut markets: Vec<MarketOut> = cached
        .into_iter()
        .map(|m| MarketOut {
            platform: m.platform,
            venue_market_id: m.venue_market_id,
            title: m.title,
            slug: m.slug,
            tags: m.tags.into_iter().flatten().collect(),
            category: m.category,
            end_date: m.end_date,
        })
        .collect();

    // 缓存空 → 回源 Venue（若已注册）。
    if markets.is_empty() {
        if let Ok(platform_enum) = platform.parse::<sharpside_shared::Platform>() {
            if let Some(venue) = state.registry.get(platform_enum) {
                let mq = MarketQuery {
                    q: q.q.clone(),
                    tag: None,
                    limit,
                };
                match venue.markets(mq).await {
                    Ok(fetched) => {
                        for m in &fetched {
                            let _ = raw::upsert_raw_market(
                                &state.db,
                                platform_enum.as_str(),
                                &m.venue_market_id,
                                &m.title,
                                m.slug.as_deref(),
                                &m.tags,
                                m.category.as_deref(),
                                m.end_date,
                                m.outcome_yes,
                                m.outcome_no,
                                None,
                                m.closed,
                            )
                            .await;
                        }
                        markets = fetched
                            .into_iter()
                            .map(|m| MarketOut {
                                platform: platform_enum.as_str().into(),
                                venue_market_id: m.venue_market_id,
                                title: m.title,
                                slug: m.slug,
                                tags: m.tags,
                                category: m.category,
                                end_date: m.end_date,
                            })
                            .collect();
                    }
                    Err(sharpside_venues_core::VenueError::Unsupported(_)) => {
                        tracing::debug!(platform, "venue 不支持 markets");
                    }
                    Err(e) => {
                        tracing::warn!(platform, error = %e, "回源 markets 失败");
                    }
                }
            }
        }
    }

    // 标题模糊过滤
    if let Some(needle) = &q.q {
        let needle = needle.to_lowercase();
        markets.retain(|m| m.title.to_lowercase().contains(&needle));
    }
    markets.truncate(limit as usize);
    Ok(Json(markets))
}

#[derive(Debug, Deserialize)]
pub struct MappingQuery {
    pub from_platform: String,
    pub from_market_id: String,
    pub to_platform: Option<String>,
}

/// `GET /market-mappings?from_platform=&from_market_id=&to_platform=` — 映射查询。
///
/// 带 `to_platform`：返回 verified 的单条映射（跟单用）。
/// 不带 `to_platform`：返回该 source 的全部 active 映射（admin 审核队列用）。
pub async fn list_market_mappings(
    state: AppState,
    Query(q): Query<MappingQuery>,
) -> Result<Json<Vec<sharpside_db::MarketMapping>>, ApiError> {
    let rows = match q.to_platform.as_deref() {
        Some(to) => {
            match mapping_q::resolve_mapping(&state.db, &q.from_platform, &q.from_market_id, to)
                .await
            {
                Ok(m) => vec![m],
                Err(sharpside_db::DbError::NotFound(_)) => vec![],
                Err(e) => return Err(e.into()),
            }
        }
        None => {
            mapping_q::list_mappings_from(&state.db, &q.from_platform, &q.from_market_id).await?
        }
    };
    Ok(Json(rows))
}

/// 供 mapping worker 复用：把 `Market`（通用）upsert 到 raw_markets。
pub(crate) async fn cache_markets(
    state: &AppState,
    platform: sharpside_shared::Platform,
    markets: &[Market],
) {
    for m in markets {
        let _ = raw::upsert_raw_market(
            &state.db,
            platform.as_str(),
            &m.venue_market_id,
            &m.title,
            m.slug.as_deref(),
            &m.tags,
            m.category.as_deref(),
            m.end_date,
            m.outcome_yes,
            m.outcome_no,
            None,
            m.closed,
        )
        .await;
    }
}
