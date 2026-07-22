//! `trader_hub.traders` 查询。对应 `docs/TRADERS_TABLE.md` §5-§6。
//!
//! upsert 冲突策略：`ON CONFLICT (platform, address) DO UPDATE SET ... WHERE excluded.字段 IS DISTINCT FROM ...`，
//! 避免无意义写（对应 `docs/TRADERS_TABLE.md` §5）。

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::error::DbError;
use crate::models::{LeaderboardRow, Trader};

/// upsert 一个交易者。`address` 一律小写存储（链上地址）。
///
/// `source` 取值：`leaderboard` / `imported` / `manual`。
#[allow(clippy::too_many_arguments)]
pub async fn upsert_trader(
    pool: &PgPool,
    platform: &str,
    address: &str,
    source: &str,
    alias: Option<&str>,
    user_name: Option<&str>,
    profile_image: Option<&str>,
    x_username: Option<&str>,
    verified_badge: Option<bool>,
) -> Result<Trader, DbError> {
    let normalized = normalize_address(platform, address);
    let row = sqlx::query_as::<_, Trader>(
        r#"
        INSERT INTO trader_hub.traders
            (platform, address, source, alias, user_name, profile_image, x_username, verified_badge)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (platform, address) DO UPDATE SET
            alias          = COALESCE(excluded.alias, traders.alias),
            user_name      = COALESCE(excluded.user_name, traders.user_name),
            profile_image  = COALESCE(excluded.profile_image, traders.profile_image),
            x_username     = COALESCE(excluded.x_username, traders.x_username),
            verified_badge = COALESCE(excluded.verified_badge, traders.verified_badge)
        WHERE
            excluded.alias          IS DISTINCT FROM traders.alias
            OR excluded.user_name      IS DISTINCT FROM traders.user_name
            OR excluded.profile_image  IS DISTINCT FROM traders.profile_image
            OR excluded.x_username     IS DISTINCT FROM traders.x_username
            OR excluded.verified_badge IS DISTINCT FROM traders.verified_badge
        RETURNING *
        "#,
    )
    .bind(platform)
    .bind(&normalized)
    .bind(source)
    .bind(alias)
    .bind(user_name)
    .bind(profile_image)
    .bind(x_username)
    .bind(verified_badge)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// 仅插入不存在的交易者；已存在则返回 `None`（不更新）。
///
/// 供 leaderboard 批量采集去重：避免重复用户，也不触发无意义 UPDATE。
#[allow(clippy::too_many_arguments)]
pub async fn insert_trader_if_absent(
    pool: &PgPool,
    platform: &str,
    address: &str,
    source: &str,
    alias: Option<&str>,
    user_name: Option<&str>,
    profile_image: Option<&str>,
    x_username: Option<&str>,
    verified_badge: Option<bool>,
) -> Result<Option<Trader>, DbError> {
    let normalized = normalize_address(platform, address);
    let row = sqlx::query_as::<_, Trader>(
        r#"
        INSERT INTO trader_hub.traders
            (platform, address, source, alias, user_name, profile_image, x_username, verified_badge)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (platform, address) DO NOTHING
        RETURNING *
        "#,
    )
    .bind(platform)
    .bind(&normalized)
    .bind(source)
    .bind(alias)
    .bind(user_name)
    .bind(profile_image)
    .bind(x_username)
    .bind(verified_badge)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 按 `(platform, address)` 取单个交易者。
pub async fn get_trader(pool: &PgPool, platform: &str, address: &str) -> Result<Trader, DbError> {
    let normalized = normalize_address(platform, address);
    let row = sqlx::query_as::<_, Trader>(
        "SELECT * FROM trader_hub.traders WHERE platform = $1 AND address = $2",
    )
    .bind(platform)
    .bind(&normalized)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::not_found(format!("trader {platform}/{normalized}")))?;
    Ok(row)
}

/// 列出某 Venue 的可见交易者（排行榜用），分页。
pub async fn list_visible_traders(
    pool: &PgPool,
    platform: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<Trader>, DbError> {
    let rows = sqlx::query_as::<_, Trader>(
        r#"
        SELECT * FROM trader_hub.traders
        WHERE platform = $1 AND visibility = 'visible'
        ORDER BY updated_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(platform)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 列出某 Venue 的热钥（is_hot=true）。
pub async fn list_hot_traders(pool: &PgPool, platform: &str) -> Result<Vec<Trader>, DbError> {
    let rows = sqlx::query_as::<_, Trader>(
        "SELECT * FROM trader_hub.traders WHERE platform = $1 AND is_hot = true",
    )
    .bind(platform)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 列出所有 Venue 的可见交易者（跨平台，身份链接 worker 与 `/traders` 跨平台查询用）。
///
/// 按 `platform, updated_at DESC` 排序，分页。
pub async fn list_all_visible_traders(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<Trader>, DbError> {
    let rows = sqlx::query_as::<_, Trader>(
        r#"
        SELECT * FROM trader_hub.traders
        WHERE visibility = 'visible'
        ORDER BY platform ASC, updated_at DESC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// admin 视角：列出全部交易者（含 hidden/blocked），可选平台/搜索过滤。
/// 对应 `docs/FRONTEND_DESIGN.md` §7.6 可见性管控页。
pub async fn list_all_traders(
    pool: &PgPool,
    platform: Option<&str>,
    q: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Trader>, DbError> {
    let rows = sqlx::query_as::<_, Trader>(
        r#"
        SELECT * FROM trader_hub.traders
        WHERE ($1::text IS NULL OR platform = $1)
          AND ($2::text IS NULL
               OR address ILIKE '%' || $2 || '%'
               OR alias ILIKE '%' || $2 || '%'
               OR x_username ILIKE '%' || $2 || '%')
        ORDER BY platform ASC, updated_at DESC
        LIMIT $3 OFFSET $4
        "#,
    )
    .bind(platform)
    .bind(q)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 设置热钥标记（`traders.is_hot`，影响浮仓抓取频率；与 `hot_wallets` 监控表不同）。
pub async fn set_hot(
    pool: &PgPool,
    platform: &str,
    address: &str,
    is_hot: bool,
) -> Result<(), DbError> {
    let normalized = normalize_address(platform, address);
    let res = sqlx::query(
        "UPDATE trader_hub.traders SET is_hot = $3, updated_at = now() \
         WHERE platform = $1 AND address = $2",
    )
    .bind(platform)
    .bind(&normalized)
    .bind(is_hot)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::not_found(format!(
            "trader {platform}/{normalized}"
        )));
    }
    Ok(())
}

/// 设置站内显示名 `alias`；`alias=None` 或空串清空为 NULL。
pub async fn set_alias(
    pool: &PgPool,
    platform: &str,
    address: &str,
    alias: Option<&str>,
) -> Result<(), DbError> {
    let normalized = normalize_address(platform, address);
    let cleaned = alias.map(str::trim).filter(|s| !s.is_empty());
    let res = sqlx::query(
        "UPDATE trader_hub.traders SET alias = $3, updated_at = now() \
         WHERE platform = $1 AND address = $2",
    )
    .bind(platform)
    .bind(&normalized)
    .bind(cleaned)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::not_found(format!(
            "trader {platform}/{normalized}"
        )));
    }
    Ok(())
}

/// 设置交易者可见性（admin 可见性管控）。`visibility` ∈ visible / hidden / blocked。
pub async fn set_visibility(
    pool: &PgPool,
    platform: &str,
    address: &str,
    visibility: &str,
) -> Result<(), DbError> {
    let normalized = normalize_address(platform, address);
    let res = sqlx::query(
        "UPDATE trader_hub.traders SET visibility = $3, updated_at = now() \
         WHERE platform = $1 AND address = $2",
    )
    .bind(platform)
    .bind(&normalized)
    .bind(visibility)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(DbError::not_found(format!(
            "trader {platform}/{normalized}"
        )));
    }
    Ok(())
}

/// 链上地址小写化；玩钱/KYC 平台原值保留。
/// 对应 `docs/TRADERS_TABLE.md` §2 `address` 字段处理规则。
fn normalize_address(platform: &str, address: &str) -> String {
    match platform {
        "polymarket" | "zeitgeist" | "azuro" => address.to_lowercase(),
        _ => address.to_string(),
    }
}

/// 列出需要回填 raw_trades 的可见交易者：
///   - `trades_backfilled_at IS NULL`（从未回填），或
///   - `trades_backfilled_at < cutoff`（超过 refresh 窗口，需增量同步新成交）
///
/// 按 `trades_backfilled_at NULLS FIRST` 排序：从未回填的优先。
/// 回填 worker 每轮取一小批，配合 API rate limit。
pub async fn list_traders_needing_backfill(
    pool: &PgPool,
    cutoff: Option<DateTime<Utc>>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Trader>, DbError> {
    let rows = sqlx::query_as::<_, Trader>(
        r#"
        SELECT * FROM trader_hub.traders
        WHERE visibility = 'visible'
          AND ($1::timestamptz IS NULL OR trades_backfilled_at IS NULL OR trades_backfilled_at < $1)
        ORDER BY trades_backfilled_at IS NULL DESC, trades_backfilled_at ASC NULLS LAST,
                 platform ASC, updated_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(cutoff)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 标记某 `(platform, address)` 的 raw_trades 已回填到当前时刻。
/// 即使 Venue 返回 0 笔成交也标记，避免每轮重试无成交的交易者。
pub async fn mark_trades_backfilled(
    pool: &PgPool,
    platform: &str,
    address: &str,
) -> Result<(), DbError> {
    let normalized = normalize_address(platform, address);
    sqlx::query(
        "UPDATE trader_hub.traders SET trades_backfilled_at = now() \
         WHERE platform = $1 AND address = $2",
    )
    .bind(platform)
    .bind(&normalized)
    .execute(pool)
    .await?;
    Ok(())
}

/// 排行榜查询参数。对应 `docs/FRONTEND_DESIGN.md` §6.2 与 `docs/ARCHITECTURE.md` §6.1。
#[derive(Debug, Clone)]
pub struct LeaderboardQuery<'a> {
    pub platform: Option<&'a str>,
    pub period: &'a str, // 1d / 1w / 1m / 1y / ytd / all
    /// 站内分类：'OVERALL'（默认，全部成交）或某分类（perf worker 按 raw_markets.category 切片）。
    pub category: &'a str,
    pub sort: &'a str, // roi / sharpe / win_rate / max_drawdown / realized_pnl / total_volume / updated_at
    pub sort_desc: bool,
    pub q: Option<&'a str>,
    pub hot_only: bool,
    pub verified_only: bool,
    /// 是否包含被 botfilter 标记为机器人的交易者。
    /// `false`（默认）→ 排除 `trader_tag.tags` 含 `'bot'` 的行；`true` → 全部返回。
    /// 对应 `crates/botfilter` 产出的 `bot` 标签（perf worker 写入 `trader_tag`）。
    pub include_bots: bool,
    /// 是否要求交易者**必须存在** `period`/`category` 对应的绩效行（多条件共同筛选）。
    ///
    /// `false`（默认，向后兼容）→ `trader_performance` 用 LEFT JOIN：没有该周期/分类
    /// 绩效行的交易者仍保留（绩效列 NULL，靠 `NULLS LAST` 排末尾）。此时 `period`/`category`
    /// 仅决定展示哪行绩效、按哪列排序，**不缩小交易者范围**。
    ///
    /// `true` → `trader_performance` 改 INNER JOIN：没有该周期/分类绩效行的交易者被剔除，
    /// `period`/`category` 真正参与 AND 共同筛选。排行榜前端开启此开关以实现
    /// 「周期+分类+平台+热钥+验证+bot+搜索」全维度 AND 组合过滤。
    pub require_perf: bool,
    pub limit: i64,
    pub offset: i64,
}

/// 列出可见交易者并 join 当前周期绩效 + 标签。
///
/// `traders` LEFT JOIN `trader_performance`(period=$period) LEFT JOIN `trader_tag`，
/// WHERE visibility='visible' AND 平台/搜索/热钥/验证过滤，ORDER BY sort NULLS LAST，LIMIT/OFFSET。
///
/// 绩效字段为 `Option`（新导入未算或无该周期行时 NULL）。`sort` 白名单防 SQL 注入。
///
/// 周期 fallback（前端无感回填）：额外 LEFT JOIN 一份 `period='all'` 的绩效行 `pa`，
/// **仅** `realized_pnl` / `total_volume` 用 `COALESCE(p.x, pa.x)` 回退到 `all` —— 即 ingest
/// 时 `seed_trader_performance` 写入的临时种子（值来自 Venue 排行榜）。这样在 backfill + perf
/// worker 跑完前，切到 1d/1w/1m/1y/ytd 也能看到 PnL / Volume，而不是全 `—`。
///
/// 派生指标（roi / sharpe / win_rate / max_drawdown / open_positions）**不回退**：它们需要
/// 成交数据由 perf worker 实算，种子行里只是 schema 默认 0，回退会显示误导性的 `0.0% / 0.00`。
/// 没算过就保持 NULL → 前端显示 `—`，等 perf worker 下一轮覆盖为真实值。`$1='all'` 时 `p` 与
/// `pa` 命中同一行，COALESCE 等于 `p`，无副作用。
///
/// 多条件共同筛选（`require_perf=true`）：`p` 的 JOIN 由 LEFT 改 INNER，没有该
/// `period`/`category` 绩效行的交易者被剔除，使周期/分类真正参与 AND 过滤。`pa`（`period='all'`
/// 的 PnL/Volume 回退）保持 LEFT —— 它只用于展示兜底，不应反向缩小交易者范围。
pub async fn list_leaderboard(
    pool: &PgPool,
    q: LeaderboardQuery<'_>,
) -> Result<Vec<LeaderboardRow>, DbError> {
    // 排序字段白名单（防注入）；max_drawdown 默认 ASC（越小越好），其余默认 DESC。
    let (order_col, default_dir) = match q.sort {
        "roi" => ("roi", "DESC"),
        "sharpe" => ("sharpe", "DESC"),
        "win_rate" => ("win_rate", "DESC"),
        "max_drawdown" => ("max_drawdown", "ASC"),
        "realized_pnl" => ("realized_pnl", "DESC"),
        "total_volume" => ("total_volume", "DESC"),
        _ => ("t.updated_at", "DESC"), // updated_at 走 traders 表
    };
    let dir = if q.sort == "updated_at" {
        if q.sort_desc {
            "DESC"
        } else {
            "ASC"
        }
    } else if q.sort_desc {
        "DESC"
    } else {
        default_dir
    };

    let sql = format!(
        r#"
        SELECT
            t.platform, t.address, t.identity_id, t.alias, t.source, t.is_hot,
            t.visibility, t.profile_image, t.x_username, t.verified_badge,
            t.user_name, t.first_seen, t.updated_at, t.trades_backfilled_at,
            p.roi, p.sharpe, p.win_rate, p.max_drawdown, p.open_positions,
            COALESCE(p.realized_pnl, pa.realized_pnl) AS realized_pnl,
            COALESCE(p.total_volume, pa.total_volume) AS total_volume,
            COALESCE(tg.tags, ARRAY[]::text[]) AS tags,
            (tg.tag_attrs->'bot'->>'confidence')::float8 AS bot_confidence
        FROM trader_hub.traders t
        {p_join} trader_hub.trader_performance p
            ON p.platform = t.platform AND p.address = t.address AND p.period = $1 AND p.category = $2
        LEFT JOIN trader_hub.trader_performance pa
            ON pa.platform = t.platform AND pa.address = t.address AND pa.period = 'all' AND pa.category = $2
        LEFT JOIN trader_hub.trader_tag tg
            ON tg.platform = t.platform AND tg.address = t.address
        WHERE t.visibility = 'visible'
            AND ($3::text IS NULL OR t.platform = $3)
            AND ($4::text IS NULL OR t.address ILIKE '%' || $4 || '%' OR t.alias ILIKE '%' || $4 || '%' OR t.x_username ILIKE '%' || $4 || '%')
            AND ($5::boolean IS FALSE OR t.is_hot = true)
            AND ($6::boolean IS FALSE OR t.verified_badge = true)
            AND ($7::boolean IS TRUE OR NOT COALESCE(tg.tags @> ARRAY['bot']::text[], false))
        ORDER BY {order_col} {dir} NULLS LAST, t.platform ASC, t.updated_at DESC
        LIMIT $8 OFFSET $9
        "#,
        p_join = if q.require_perf {
            "INNER JOIN"
        } else {
            "LEFT JOIN"
        },
    );

    let rows = sqlx::query_as::<_, LeaderboardRow>(&sql)
        .bind(q.period)
        .bind(q.category)
        .bind(q.platform)
        .bind(q.q)
        .bind(q.hot_only)
        .bind(q.verified_only)
        .bind(q.include_bots)
        .bind(q.limit)
        .bind(q.offset)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

/// 排行榜结果总数（不含 LIMIT/OFFSET）。与 `list_leaderboard` 同 WHERE 过滤口径，
/// 供前端分页显示「显示 1-50 / 1,284」。
///
/// 注意：`period` / `category` 默认（`require_perf=false`）仅作用于 `list_leaderboard`
/// 的 LEFT JOIN 绩效行，不参与交易者过滤（LEFT JOIN 保留所有 traders），故此处不计入。
/// 当 `require_perf=true` 时，`period`/`category` 改 INNER JOIN 参与过滤，此处需同步
/// INNER JOIN `trader_performance` 以保持总数与列表口径一致。
pub async fn count_leaderboard(pool: &PgPool, q: LeaderboardQuery<'_>) -> Result<i64, DbError> {
    // 始终 LEFT JOIN 绩效行（PK 唯一，不会放大行数）；require_perf 时用
    // `p.address IS NOT NULL` 把「无该周期/分类绩效行」的交易者滤掉，等价 INNER JOIN。
    let (count,): (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*)::bigint
        FROM trader_hub.traders t
        LEFT JOIN trader_hub.trader_performance p
            ON p.platform = t.platform AND p.address = t.address AND p.period = $6 AND p.category = $7
        LEFT JOIN trader_hub.trader_tag tg
            ON tg.platform = t.platform AND tg.address = t.address
        WHERE t.visibility = 'visible'
            AND ($1::text IS NULL OR t.platform = $1)
            AND ($2::text IS NULL OR t.address ILIKE '%' || $2 || '%' OR t.alias ILIKE '%' || $2 || '%' OR t.x_username ILIKE '%' || $2 || '%')
            AND ($3::boolean IS FALSE OR t.is_hot = true)
            AND ($4::boolean IS FALSE OR t.verified_badge = true)
            AND ($5::boolean IS TRUE OR NOT COALESCE(tg.tags @> ARRAY['bot']::text[], false))
            AND ($8::boolean IS FALSE OR p.address IS NOT NULL)
        "#,
    )
    .bind(q.platform)
    .bind(q.q)
    .bind(q.hot_only)
    .bind(q.verified_only)
    .bind(q.include_bots)
    .bind(q.period)
    .bind(q.category)
    .bind(q.require_perf)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_lowercase_onchain() {
        assert_eq!(normalize_address("polymarket", "0xABCDEF"), "0xabcdef");
        assert_eq!(normalize_address("zeitgeist", "0xABC"), "0xabc");
    }

    #[test]
    fn normalize_preserves_kyc_manifold() {
        assert_eq!(normalize_address("kalshi", "User_123"), "User_123");
        assert_eq!(normalize_address("manifold", "Alice"), "Alice");
    }
}
