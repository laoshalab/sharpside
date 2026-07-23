//! `trader_hub.position_timeline` / `trader_performance` / `trader_equity_curve` /
//! `trader_tag` 查询。对应 `docs/VENUEHUB_STORAGE.md` §6 与 `docs/PERFORMANCE_PIPELINE.md`。
//!
//! perf worker 读取 `raw_trades` → 调 `sharpside-perf` 重建时间线与指标 → 覆盖写本层。

use rust_decimal::Decimal;
use sqlx::PgPool;

use crate::error::DbError;
use crate::models::TraderPerformance;

/// 插入一条 `/value` 快照。`(platform, address, ts)` 主键去重，重复插入静默跳过。
pub async fn insert_value_snapshot(
    pool: &PgPool,
    platform: &str,
    address: &str,
    ts: chrono::DateTime<chrono::Utc>,
    value: f64,
) -> Result<(), DbError> {
    let v = if value.is_nan() || value.is_infinite() {
        0.0
    } else {
        value
    };
    let v = Decimal::try_from(v).map_err(|e| DbError::Invalid(e.to_string()))?;
    sqlx::query(
        "INSERT INTO trader_hub.trader_value_snapshot (platform, address, ts, value) \
         VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
    )
    .bind(platform)
    .bind(address)
    .bind(ts)
    .bind(v)
    .execute(pool)
    .await?;
    Ok(())
}

/// 选一批需要刷新 `/value` 快照的 visible 交易者：从未快照过或最近 `min_age` 内无快照。
///
/// 优先级（高→低）：从未快照 → 缺 1d/1w/1m 官方盈亏 → 热钥 → 最久未快照。
/// 每 tick 只拉 `limit` 个，对 rate limit 友好，同时让导入/缺官方数据的地址更快成熟。
pub async fn pick_value_snapshot_candidates(
    pool: &PgPool,
    platform: &str,
    min_age: chrono::DateTime<chrono::Utc>,
    limit: i64,
) -> Result<Vec<String>, DbError> {
    let rows = sqlx::query_scalar::<_, String>(
        r#"
        SELECT t.address
        FROM trader_hub.traders t
        LEFT JOIN LATERAL (
            SELECT MAX(ts) AS last_ts
            FROM trader_hub.trader_value_snapshot s
            WHERE s.platform = t.platform AND s.address = t.address
        ) s ON true
        LEFT JOIN LATERAL (
            SELECT EXISTS (
                SELECT 1
                FROM trader_hub.trader_performance p
                WHERE p.platform = t.platform
                  AND p.address = t.address
                  AND p.category = 'OVERALL'
                  AND p.period IN ('1d', '1w', '1m')
                  AND p.official_pnl IS NOT NULL
            ) AS has_official
        ) o ON true
        WHERE t.platform = $1 AND t.visibility = 'visible'
          AND (s.last_ts IS NULL OR s.last_ts < $2)
        ORDER BY
          CASE WHEN s.last_ts IS NULL THEN 0 ELSE 1 END,
          CASE WHEN COALESCE(o.has_official, false) THEN 1 ELSE 0 END,
          CASE WHEN t.is_hot THEN 0 ELSE 1 END,
          s.last_ts ASC NULLS FIRST
        LIMIT $3
        "#,
    )
    .bind(platform)
    .bind(min_age)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// `/value` 周期 delta 结果（含覆盖率，便于 worker 门控短窗口假数据）。
#[derive(Debug, Clone)]
pub struct ValueDelta {
    pub delta: f64,
    /// 快照覆盖的窗口占比：`covered / (now - since)`，通常 ∈ (0, 1]，冷启动可更低。
    pub coverage: f64,
    pub base_ts: chrono::DateTime<chrono::Utc>,
    pub latest_ts: chrono::DateTime<chrono::Utc>,
}

/// 算某 `(platform, address)` 在 period 窗口内的估值 delta。
///
/// 基线优先取 `ts ≤ since` 最近一点（更接近「周期起点」）；若无则回落窗口内最早点。
/// `delta = latest.value - baseline.value`。需至少 2 个不同时点，否则返回 None。
///
/// 口径：持仓 MTM 变化，**含出入金**（存入会抬高 value，提出会压低）。前端副标明示此口径，
/// 精确扣除现金流留给 Phase 3C（接 `/activity`）。
pub async fn value_delta_since(
    pool: &PgPool,
    platform: &str,
    address: &str,
    since: chrono::DateTime<chrono::Utc>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Option<ValueDelta>, DbError> {
    #[derive(sqlx::FromRow)]
    struct V {
        ts: chrono::DateTime<chrono::Utc>,
        value: Decimal,
    }
    // 优先：cutoff 前最近一点（≈ 周期起点估值）。
    let before = sqlx::query_as::<_, V>(
        "SELECT ts, value FROM trader_hub.trader_value_snapshot \
         WHERE platform = $1 AND address = $2 AND ts <= $3 \
         ORDER BY ts DESC LIMIT 1",
    )
    .bind(platform)
    .bind(address)
    .bind(since)
    .fetch_optional(pool)
    .await?;
    // 回落：窗口内最早一点（冷启动尚无 cutoff 前快照时）。
    let baseline = if before.is_some() {
        before
    } else {
        sqlx::query_as::<_, V>(
            "SELECT ts, value FROM trader_hub.trader_value_snapshot \
             WHERE platform = $1 AND address = $2 AND ts >= $3 \
             ORDER BY ts ASC LIMIT 1",
        )
        .bind(platform)
        .bind(address)
        .bind(since)
        .fetch_optional(pool)
        .await?
    };
    // 最新一点（≈ now）。
    let latest = sqlx::query_as::<_, V>(
        "SELECT ts, value FROM trader_hub.trader_value_snapshot \
         WHERE platform = $1 AND address = $2 \
         ORDER BY ts DESC LIMIT 1",
    )
    .bind(platform)
    .bind(address)
    .fetch_optional(pool)
    .await?;
    match (baseline, latest) {
        (Some(base), Some(l)) => {
            if base.ts == l.ts {
                return Ok(None);
            }
            let delta = (l.value - base.value)
                .to_string()
                .parse::<f64>()
                .unwrap_or(0.0);
            // 覆盖率：从 max(base, since) 到 latest，相对完整窗口 (now - since)。
            let period_secs = (now - since).num_seconds().max(1) as f64;
            let cover_start = if base.ts > since { base.ts } else { since };
            let covered = (l.ts - cover_start).num_seconds().max(0) as f64;
            let coverage = (covered / period_secs).clamp(0.0, 1.0);
            Ok(Some(ValueDelta {
                delta,
                coverage,
                base_ts: base.ts,
                latest_ts: l.ts,
            }))
        }
        _ => Ok(None),
    }
}

/// `replace_position_timelines` 的单行输入（避免巨型元组）。
pub struct TimelineRow {
    pub token_id: String,
    pub condition_id: Option<String>,
    pub opened_at: Option<chrono::DateTime<chrono::Utc>>,
    pub closed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub total_bought_size: f64,
    pub total_sold_size: f64,
    pub avg_cost: f64,
    pub realized_pnl: f64,
    pub final_open_size: f64,
    pub is_closed: bool,
    pub holding_seconds: Option<i64>,
}

/// 覆盖写某 `(platform, address, period, category)` 的绩效行。
///
/// `category` = 'OVERALL'（全部成交）或站内分类（由 perf worker 按 raw_markets.category 切片）。
/// `ON CONFLICT DO UPDATE`：六周期 × N 分类各自一行，重算覆盖旧值。
#[allow(clippy::too_many_arguments)]
pub async fn upsert_trader_performance(
    pool: &PgPool,
    platform: &str,
    address: &str,
    period: &str,
    category: &str,
    roi: f64,
    sharpe: f64,
    sortino: f64,
    win_rate: f64,
    max_drawdown: f64,
    realized_pnl: f64,
    unrealized_pnl: f64,
    gross_profit: f64,
    gross_loss: f64,
    profit_factor: f64,
    wins: i32,
    losses: i32,
    position_count: i32,
    open_positions: i32,
    total_volume: f64,
    cost_basis: f64,
) -> Result<(), DbError> {
    let to_dec = |v: f64| -> Result<Decimal, DbError> {
        // NaN / ±inf（如无亏损时 profit_factor=inf、无方差时 sharpe=NaN）无法转 Decimal，
        // 会令整行 upsert 静默失败。统一归零，保证 performance 行总能落库。
        let sanitized = if v.is_nan() || v.is_infinite() {
            0.0
        } else {
            v
        };
        Decimal::try_from(sanitized).map_err(|e| DbError::Invalid(e.to_string()))
    };
    sqlx::query(
        r#"
        INSERT INTO trader_hub.trader_performance
            (platform, address, period, category, roi, sharpe, sortino, win_rate, max_drawdown,
             realized_pnl, unrealized_pnl, gross_profit, gross_loss, profit_factor,
             wins, losses, position_count, open_positions, total_volume, cost_basis)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)
        ON CONFLICT (platform, address, period, category) DO UPDATE SET
            roi=excluded.roi, sharpe=excluded.sharpe, sortino=excluded.sortino,
            win_rate=excluded.win_rate, max_drawdown=excluded.max_drawdown,
            realized_pnl=excluded.realized_pnl, unrealized_pnl=excluded.unrealized_pnl,
            gross_profit=excluded.gross_profit, gross_loss=excluded.gross_loss,
            profit_factor=excluded.profit_factor, wins=excluded.wins, losses=excluded.losses,
            position_count=excluded.position_count, open_positions=excluded.open_positions,
            total_volume=excluded.total_volume, cost_basis=excluded.cost_basis,
            computed_at=now()
        "#,
    )
    .bind(platform)
    .bind(address)
    .bind(period)
    .bind(category)
    .bind(to_dec(roi)?)
    .bind(to_dec(sharpe)?)
    .bind(to_dec(sortino)?)
    .bind(to_dec(win_rate)?)
    .bind(to_dec(max_drawdown)?)
    .bind(to_dec(realized_pnl)?)
    .bind(to_dec(unrealized_pnl)?)
    .bind(to_dec(gross_profit)?)
    .bind(to_dec(gross_loss)?)
    .bind(to_dec(profit_factor)?)
    .bind(wins)
    .bind(losses)
    .bind(position_count)
    .bind(open_positions)
    .bind(to_dec(total_volume)?)
    .bind(to_dec(cost_basis)?)
    .execute(pool)
    .await?;
    Ok(())
}

/// 临时绩效种子：用 Venue 排行榜自带的 `pnl`/`vol` 填一行 `trader_performance`（period='all'）。
///
/// 仅在交易者**首次入库**时由 ingest 调用，`ON CONFLICT DO NOTHING` 永不覆盖已有真实绩效。
/// 其余指标列保持默认 0；backfill + perf worker 跑完后会被 `upsert_trader_performance`
/// 覆盖升级为完整真实指标。对应 `docs/FLOWS.md` §1 临时展示层。
///
/// `realized_pnl`/`total_volume` 为 None 时跳过（该 Venue 未提供排行榜指标）。
pub async fn seed_trader_performance(
    pool: &PgPool,
    platform: &str,
    address: &str,
    realized_pnl: Option<f64>,
    total_volume: Option<f64>,
) -> Result<(), DbError> {
    let (Some(pnl), Some(vol)) = (realized_pnl, total_volume) else {
        return Ok(());
    };
    let sanitize = |v: f64| -> Result<Decimal, DbError> {
        // NaN / ±inf 无法转 Decimal，统一归零（与 upsert_trader_performance 一致）。
        let s = if v.is_nan() || v.is_infinite() {
            0.0
        } else {
            v
        };
        Decimal::try_from(s).map_err(|e| DbError::Invalid(e.to_string()))
    };
    sqlx::query(
        r#"
        INSERT INTO trader_hub.trader_performance
            (platform, address, period, category, realized_pnl, total_volume)
        VALUES ($1, $2, 'all', 'OVERALL', $3, $4)
        ON CONFLICT (platform, address, period, category) DO NOTHING
        "#,
    )
    .bind(platform)
    .bind(address)
    .bind(sanitize(pnl)?)
    .bind(sanitize(vol)?)
    .execute(pool)
    .await?;
    Ok(())
}

/// 用官方**分类**排行榜的 pnl/vol 种子/刷新非 `OVERALL` 绩效行。
///
/// 排行榜前端按 `period`×`category` INNER JOIN（`require_perf`）过滤；若只有 OVERALL
/// 行，点分类会得到 0 人。`raw_markets.category` 覆盖不足时，perf worker 无法切片，
/// 故用官方分类榜先填展示层，直到本地切片就绪。
///
/// - `category` 不得为 `OVERALL`（OVERALL 仍走 `seed_trader_performance` /
///   `upsert_official_pnl`）。
/// - 写入 `realized_pnl` / `total_volume` / 近似 `roi=pnl/vol`（便于默认按 ROI 排序），
///   同步写 `official_*`。
/// - 若行已被 perf worker 实算（`position_count`/`wins`/`losses` 任一非 0），**只**更新
///   `official_*`，不覆盖自算指标。
pub async fn upsert_category_leaderboard_seed(
    pool: &PgPool,
    platform: &str,
    address: &str,
    period: &str,
    category: &str,
    realized_pnl: Option<f64>,
    total_volume: Option<f64>,
    source: &str,
) -> Result<(), DbError> {
    if category.is_empty() || category.eq_ignore_ascii_case("OVERALL") {
        return Ok(());
    }
    let Some(pnl) = realized_pnl else {
        return Ok(());
    };
    let sanitize = |v: f64| -> Result<Decimal, DbError> {
        let s = if v.is_nan() || v.is_infinite() {
            0.0
        } else {
            v
        };
        Decimal::try_from(s).map_err(|e| DbError::Invalid(e.to_string()))
    };
    let pnl_dec = sanitize(pnl)?;
    let vol_dec: Option<Decimal> = match total_volume {
        Some(v) => Some(sanitize(v)?),
        None => None,
    };
    // 近似 ROI：官方榜无成本基数，用 pnl/vol 作可排序代理；vol 缺失或 ≤0 则 0。
    let roi_dec = match vol_dec {
        Some(v) if v > Decimal::ZERO => pnl_dec / v,
        _ => Decimal::ZERO,
    };
    sqlx::query(
        r#"
        INSERT INTO trader_hub.trader_performance
            (platform, address, period, category, realized_pnl, total_volume, roi,
             official_pnl, official_vol, official_source, official_pnl_at)
        VALUES ($1, $2, $3, $4, $5, COALESCE($6, 0), $7, $5, $6, $8, now())
        ON CONFLICT (platform, address, period, category) DO UPDATE SET
            official_pnl = excluded.official_pnl,
            official_vol = COALESCE(excluded.official_vol, trader_hub.trader_performance.official_vol),
            official_source = excluded.official_source,
            official_pnl_at = now(),
            realized_pnl = CASE
                WHEN trader_hub.trader_performance.position_count = 0
                 AND trader_hub.trader_performance.wins = 0
                 AND trader_hub.trader_performance.losses = 0
                THEN excluded.realized_pnl
                ELSE trader_hub.trader_performance.realized_pnl
            END,
            total_volume = CASE
                WHEN trader_hub.trader_performance.position_count = 0
                 AND trader_hub.trader_performance.wins = 0
                 AND trader_hub.trader_performance.losses = 0
                THEN COALESCE(excluded.total_volume, trader_hub.trader_performance.total_volume)
                ELSE trader_hub.trader_performance.total_volume
            END,
            roi = CASE
                WHEN trader_hub.trader_performance.position_count = 0
                 AND trader_hub.trader_performance.wins = 0
                 AND trader_hub.trader_performance.losses = 0
                THEN excluded.roi
                ELSE trader_hub.trader_performance.roi
            END
        "#,
    )
    .bind(platform)
    .bind(address)
    .bind(period)
    .bind(category)
    .bind(pnl_dec)
    .bind(vol_dec)
    .bind(roi_dec)
    .bind(source)
    .execute(pool)
    .await?;
    Ok(())
}

/// 写官方盈亏到 `(platform, address, period, 'OVERALL')` 绩效行。
///
/// 由 official_pnl worker 调用：排行榜命中或 `/value` delta 兜底写回。
/// 仅写 `category='OVERALL'`。行不存在时插入最小行（自算列留默认 0），
/// 由后续 perf worker 覆盖补齐；存在时只更新 official_* 列，**不碰**自算 `realized_pnl`。
///
/// - `pnl` 为 None 时跳过。
/// - `vol` 为 None 时写入 NULL（`/value` delta 无成交量）；更新时用 `COALESCE` 保留旧 vol。
/// - `overwrite=true`：排行榜路径，始终覆盖。
/// - `overwrite=false`：`/value` delta 兜底，仅当尚无官方数据或来源已是
///   `polymarket_value_delta` 时写入，**永不覆盖**排行榜口径。
pub async fn upsert_official_pnl(
    pool: &PgPool,
    platform: &str,
    address: &str,
    period: &str,
    pnl: Option<f64>,
    vol: Option<f64>,
    source: &str,
    overwrite: bool,
) -> Result<(), DbError> {
    let Some(pnl) = pnl else {
        return Ok(());
    };
    let sanitize = |v: f64| -> Result<Decimal, DbError> {
        let s = if v.is_nan() || v.is_infinite() {
            0.0
        } else {
            v
        };
        Decimal::try_from(s).map_err(|e| DbError::Invalid(e.to_string()))
    };
    let vol_dec: Option<Decimal> = match vol {
        Some(v) => Some(sanitize(v)?),
        None => None,
    };
    // overwrite=false 时 WHERE 拒绝覆盖排行榜来源；overwrite=true 时恒真。
    let sql = if overwrite {
        r#"
        INSERT INTO trader_hub.trader_performance
            (platform, address, period, category, official_pnl, official_vol, official_source, official_pnl_at)
        VALUES ($1, $2, $3, 'OVERALL', $4, $5, $6, now())
        ON CONFLICT (platform, address, period, category) DO UPDATE SET
            official_pnl = excluded.official_pnl,
            official_vol = COALESCE(excluded.official_vol, trader_hub.trader_performance.official_vol),
            official_source = excluded.official_source,
            official_pnl_at = now()
        "#
    } else {
        r#"
        INSERT INTO trader_hub.trader_performance
            (platform, address, period, category, official_pnl, official_vol, official_source, official_pnl_at)
        VALUES ($1, $2, $3, 'OVERALL', $4, $5, $6, now())
        ON CONFLICT (platform, address, period, category) DO UPDATE SET
            official_pnl = excluded.official_pnl,
            official_vol = COALESCE(excluded.official_vol, trader_hub.trader_performance.official_vol),
            official_source = excluded.official_source,
            official_pnl_at = now()
        WHERE trader_hub.trader_performance.official_source IS NULL
           OR trader_hub.trader_performance.official_source = 'polymarket_value_delta'
        "#
    };
    sqlx::query(sql)
        .bind(platform)
        .bind(address)
        .bind(period)
        .bind(sanitize(pnl)?)
        .bind(vol_dec)
        .bind(source)
        .execute(pool)
        .await?;
    Ok(())
}

/// 取某 `(platform, address, period)` 的绩效行。
pub async fn get_trader_performance(
    pool: &PgPool,
    platform: &str,
    address: &str,
    period: &str,
) -> Result<TraderPerformance, DbError> {
    let row = sqlx::query_as::<_, TraderPerformance>(
        "SELECT * FROM trader_hub.trader_performance \
         WHERE platform = $1 AND address = $2 AND period = $3",
    )
    .bind(platform)
    .bind(address)
    .bind(period)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| DbError::not_found(format!("perf {platform}/{address}/{period}")))?;
    Ok(row)
}

/// 列出某 `(platform, address)` 的全部周期绩效行（1d/1w/1m/1y/ytd/all）。
pub async fn list_trader_performance(
    pool: &PgPool,
    platform: &str,
    address: &str,
) -> Result<Vec<TraderPerformance>, DbError> {
    let rows = sqlx::query_as::<_, TraderPerformance>(
        "SELECT * FROM trader_hub.trader_performance \
         WHERE platform = $1 AND address = $2 ORDER BY period",
    )
    .bind(platform)
    .bind(address)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 覆盖写某 `(platform, address)` 的仓位时间线（先删后插，单事务）。
///
/// 重算时整组替换，避免陈旧仓位残留。
pub async fn replace_position_timelines(
    pool: &PgPool,
    platform: &str,
    address: &str,
    rows: &[TimelineRow],
) -> Result<(), DbError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM trader_hub.position_timeline WHERE platform = $1 AND address = $2")
        .bind(platform)
        .bind(address)
        .execute(&mut *tx)
        .await?;
    for r in rows {
        sqlx::query(
            r#"
            INSERT INTO trader_hub.position_timeline
                (platform, address, token_id, condition_id, opened_at, closed_at,
                 total_bought_size, total_sold_size, avg_cost, realized_pnl,
                 final_open_size, is_closed, holding_seconds)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
            "#,
        )
        .bind(platform)
        .bind(address)
        .bind(&r.token_id)
        .bind(&r.condition_id)
        .bind(r.opened_at)
        .bind(r.closed_at)
        .bind(to_dec(r.total_bought_size)?)
        .bind(to_dec(r.total_sold_size)?)
        .bind(to_dec(r.avg_cost)?)
        .bind(to_dec(r.realized_pnl)?)
        .bind(to_dec(r.final_open_size)?)
        .bind(r.is_closed)
        .bind(r.holding_seconds)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// 覆盖写某 `(platform, address)` 的权益曲线（先删后插，单事务）。
pub async fn replace_equity_curve(
    pool: &PgPool,
    platform: &str,
    address: &str,
    rows: &[(chrono::DateTime<chrono::Utc>, f64, f64, f64)],
) -> Result<(), DbError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM trader_hub.trader_equity_curve WHERE platform = $1 AND address = $2")
        .bind(platform)
        .bind(address)
        .execute(&mut *tx)
        .await?;
    for (ts, equity, daily_pnl, drawdown_pct) in rows {
        sqlx::query(
            r#"
            INSERT INTO trader_hub.trader_equity_curve
                (platform, address, ts, equity, daily_pnl, drawdown_pct)
            VALUES ($1,$2,$3,$4,$5,$6)
            "#,
        )
        .bind(platform)
        .bind(address)
        .bind(ts)
        .bind(to_dec(*equity)?)
        .bind(to_dec(*daily_pnl)?)
        .bind(to_dec(*drawdown_pct)?)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// upsert 某 `(platform, address)` 的运营标签。
pub async fn upsert_trader_tag(
    pool: &PgPool,
    platform: &str,
    address: &str,
    tags: &[String],
    tag_attrs: &serde_json::Value,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        INSERT INTO trader_hub.trader_tag (platform, address, tags, tag_attrs)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (platform, address) DO UPDATE SET
            tags = excluded.tags,
            tag_attrs = excluded.tag_attrs,
            tagged_at = now()
        "#,
    )
    .bind(platform)
    .bind(address)
    .bind(tags)
    .bind(tag_attrs)
    .execute(pool)
    .await?;
    Ok(())
}

/// 取某 `(platform, address)` 的标签数组（无则空）。
pub async fn get_trader_tag(
    pool: &PgPool,
    platform: &str,
    address: &str,
) -> Result<Vec<String>, DbError> {
    let row: Option<(Vec<String>,)> = sqlx::query_as(
        "SELECT tags FROM trader_hub.trader_tag WHERE platform = $1 AND address = $2",
    )
    .bind(platform)
    .bind(address)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(tags,)| tags).unwrap_or_default())
}

/// 取某 `(platform, address)` 的 `tag_attrs`（jsonb，无则 None）。
///
/// perf worker 写入结构：`{ "style": [...], "bot": BotFlags{ is_bot, confidence, hit_rules } }`。
/// 前端机器人检测面板读 `tag_attrs.bot` 下钻命中规则与 evidence。
pub async fn get_trader_tag_attrs(
    pool: &PgPool,
    platform: &str,
    address: &str,
) -> Result<Option<serde_json::Value>, DbError> {
    let row: Option<(serde_json::Value,)> = sqlx::query_as(
        "SELECT tag_attrs FROM trader_hub.trader_tag WHERE platform = $1 AND address = $2",
    )
    .bind(platform)
    .bind(address)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(a,)| a))
}

/// 权益曲线粒度。对应 `docs/PERFORMANCE_PIPELINE.md` §降采样。
///
/// - `Hour`：全历史小时级（原始粒度，长历史 trader 会返回大量点）。
/// - `Day`：全历史日级（`date_trunc('day')` 取每日最后一个点）。
/// - `Auto`（默认）：近 30 天小时级 + 30 天前日级（UNION ALL），
///   兼顾近期平滑度与长历史规模，单曲线点数上限约 `720 + 365*N年`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquityGranularity {
    Hour,
    Day,
    Auto,
}

impl EquityGranularity {
    /// 解析 query 参数；非法值回落到 `Auto`。
    pub fn parse(s: &str) -> Self {
        match s {
            "hour" => Self::Hour,
            "day" => Self::Day,
            _ => Self::Auto,
        }
    }
}

/// 列出某 `(platform, address)` 的权益曲线，按 ts 升序。
/// 对应 `docs/FRONTEND_DESIGN.md` §6.1。`period` 当前仅用于前端语义，曲线本身按全历史返回（前端按 period 截取）。
///
/// `granularity` 控制降采样策略（见 `EquityGranularity`）；默认 `Auto` 平衡点数与平滑度。
pub async fn list_equity_curve(
    pool: &PgPool,
    platform: &str,
    address: &str,
    granularity: EquityGranularity,
) -> Result<Vec<crate::models::EquityCurvePoint>, DbError> {
    let sql = match granularity {
        EquityGranularity::Hour => {
            // 全历史小时级（原始粒度）。
            "SELECT ts, equity, daily_pnl, drawdown_pct \
             FROM trader_hub.trader_equity_curve \
             WHERE platform = $1 AND address = $2 ORDER BY ts ASC"
                .to_string()
        }
        EquityGranularity::Day => {
            // 全历史日级：按天取最后一个点（DISTINCT ON + 降序取末点）。
            "SELECT ts, equity, daily_pnl, drawdown_pct FROM ( \
                SELECT DISTINCT ON (date_trunc('day', ts)) \
                       date_trunc('day', ts) AS ts, equity, daily_pnl, drawdown_pct \
                FROM trader_hub.trader_equity_curve \
                WHERE platform = $1 AND address = $2 \
                ORDER BY date_trunc('day', ts), ts DESC \
             ) d ORDER BY ts ASC"
                .to_string()
        }
        EquityGranularity::Auto => {
            // 近 30 天小时级 + 30 天前日级，UNION ALL 后按 ts 升序。
            "SELECT ts, equity, daily_pnl, drawdown_pct FROM ( \
                SELECT ts, equity, daily_pnl, drawdown_pct \
                FROM trader_hub.trader_equity_curve \
                WHERE platform = $1 AND address = $2 AND ts >= now() - interval '30 days' \
                UNION ALL \
                SELECT date_trunc('day', ts) AS ts, equity, daily_pnl, drawdown_pct \
                FROM ( \
                    SELECT DISTINCT ON (date_trunc('day', ts)) \
                           date_trunc('day', ts) AS ts, equity, daily_pnl, drawdown_pct \
                    FROM trader_hub.trader_equity_curve \
                    WHERE platform = $1 AND address = $2 AND ts < now() - interval '30 days' \
                    ORDER BY date_trunc('day', ts), ts DESC \
                ) old \
             ) u ORDER BY ts ASC"
                .to_string()
        }
    };
    let rows = sqlx::query_as::<_, crate::models::EquityCurvePoint>(&sql)
        .bind(platform)
        .bind(address)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

/// 批量取多个交易者的权益曲线（供排行榜 sparkline，消除 N+1）。
///
/// `ids`：(platform, address) 对列表。`since`：period 截断点；None = 全历史。
/// `granularity`：短周期(1d/1w)用 `Hour`，长周期用 `Day`（服务端按天取末点降采样）。
///
/// 返回按 (platform, address) 分组的点列表（顺序与首次出现一致）。每点含完整字段，
/// 调用方按需取 `equity` 画 sparkline。
pub async fn list_equity_curves_batch(
    pool: &PgPool,
    ids: &[(String, String)],
    since: Option<chrono::DateTime<chrono::Utc>>,
    granularity: EquityGranularity,
) -> Result<Vec<(String, String, Vec<crate::models::EquityCurveBatchRow>)>, DbError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let platforms: Vec<String> = ids.iter().map(|(p, _)| p.clone()).collect();
    let addresses: Vec<String> = ids.iter().map(|(_, a)| a.clone()).collect();
    // Hour：全粒度；Day/Auto：按天取最后一小时点（DISTINCT ON + 末点）。
    let sql = match granularity {
        EquityGranularity::Hour => r#"
            SELECT ec.platform, ec.address, ec.ts, ec.equity, ec.daily_pnl, ec.drawdown_pct
            FROM trader_hub.trader_equity_curve ec
            JOIN unnest($1::text[], $2::text[]) AS req(platform, address)
              ON ec.platform = req.platform AND ec.address = req.address
            WHERE ($3::timestamptz IS NULL OR ec.ts >= $3)
            ORDER BY ec.platform, ec.address, ec.ts ASC
        "#
        .to_string(),
        _ => r#"
            SELECT platform, address, ts, equity, daily_pnl, drawdown_pct FROM (
                SELECT DISTINCT ON (ec.platform, ec.address, date_trunc('day', ec.ts))
                       ec.platform, ec.address, date_trunc('day', ec.ts) AS ts,
                       ec.equity, ec.daily_pnl, ec.drawdown_pct
                FROM trader_hub.trader_equity_curve ec
                JOIN unnest($1::text[], $2::text[]) AS req(platform, address)
                  ON ec.platform = req.platform AND ec.address = req.address
                WHERE ($3::timestamptz IS NULL OR ec.ts >= $3)
                ORDER BY ec.platform, ec.address, date_trunc('day', ec.ts), ec.ts DESC
            ) d ORDER BY platform, address, ts ASC
        "#
        .to_string(),
    };
    let rows = sqlx::query_as::<_, crate::models::EquityCurveBatchRow>(&sql)
        .bind(&platforms)
        .bind(&addresses)
        .bind(since)
        .fetch_all(pool)
        .await?;
    // 按 (platform, address) 分组，保留首次出现顺序。
    let mut out: Vec<(String, String, Vec<crate::models::EquityCurveBatchRow>)> = Vec::new();
    let mut idx: std::collections::HashMap<(String, String), usize> =
        std::collections::HashMap::new();
    for r in rows {
        let key = (r.platform.clone(), r.address.clone());
        if let Some(&i) = idx.get(&key) {
            out[i].2.push(r);
        } else {
            let i = out.len();
            idx.insert(key, i);
            out.push((r.platform.clone(), r.address.clone(), vec![r]));
        }
    }
    Ok(out)
}

/// 列出某 `(platform, address)` 的仓位时间线。
/// 对应 `docs/FRONTEND_DESIGN.md` §6.1 当前持仓表（前端过滤 `is_closed=false` 为当前持仓）。
pub async fn list_position_timeline(
    pool: &PgPool,
    platform: &str,
    address: &str,
) -> Result<Vec<crate::models::PositionRow>, DbError> {
    let rows = sqlx::query_as::<_, crate::models::PositionRow>(
        "SELECT token_id, condition_id, opened_at, closed_at, total_bought_size, \
                total_sold_size, avg_cost, realized_pnl, final_open_size, is_closed \
         FROM trader_hub.position_timeline \
         WHERE platform = $1 AND address = $2 ORDER BY opened_at DESC NULLS LAST",
    )
    .bind(platform)
    .bind(address)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

fn to_dec(v: f64) -> Result<Decimal, DbError> {
    let sanitized = if v.is_nan() || v.is_infinite() {
        0.0
    } else {
        v
    };
    Decimal::try_from(sanitized).map_err(|e| DbError::Invalid(e.to_string()))
}
