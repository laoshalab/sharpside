//! 环境变量配置。对应 `docs/ARCHITECTURE.md` §6.1 与 `infra/.env.example`。
//!
//! MVP 不引入 figment（未缓存），直接从环境变量读取，缺失项回退默认值。
//! 生产部署通过 docker-compose `env_file` 注入。

use std::env;

/// VenueHub 运行配置。
#[derive(Debug, Clone)]
pub struct Config {
    /// 监听地址
    pub listen_addr: String,
    /// PostgreSQL 连接串
    pub database_url: String,
    /// 连接池大小
    pub db_max_connections: u32,
    /// 各 Venue 启用开关（signal_source）
    pub venues: VenueToggles,
    /// worker 触发间隔
    pub workers: WorkerIntervals,
    /// 启发式映射阈值（0–1）
    pub auto_match_threshold: f64,
    /// 启发式身份链接阈值（0–1）
    pub identity_threshold: f64,
    /// 影子校验 worker 间隔（秒）
    pub shadow_secs: u64,
    /// 影子模式 dry_run：不拉第三方 API，用自算绩效 + 小扰动合成第三方指标跑通 diff/审计链路
    pub shadow_dry_run: bool,
    /// 第三方数据源基址（非 dry_run 时用，离线未缓存故默认 dry_run）
    pub shadow_third_party_url: String,
    /// Polymarket API base 覆盖（None 用默认线上地址）。
    /// 生产可用于自托管代理/镜像；联调/测试指向本地 mock。
    pub polymarket_data_api: Option<String>,
    pub polymarket_gamma_api: Option<String>,
    pub polymarket_clob_api: Option<String>,
    /// Follow 服务基址（hot worker 检出仓位 diff 后 POST `/internal/signals`）。
    /// 默认 `http://127.0.0.1:8082`。设为空串则禁用信号 emit（仅快照）。
    pub follow_url: String,
    /// 调用 follow `/internal/signals` 时携带的 `X-Internal-Secret`。
    /// **必须配置**：follow 侧已强制要求（空串即 401 拒收信号），故本侧空串会导致所有信号被拒。
    /// 须与 follow 服务的 `INTERNAL_SIGNAL_SECRET` 一致。
    pub follow_signal_secret: String,
    /// 运维/admin 令牌：保护写端点（如 `/traders/import*`）免遭未鉴权滥用。
    /// 请求须带 `Authorization: Bearer <token>`。生产由 `assert_secret` 强制非空/非默认值。
    pub admin_token: String,
}

/// 各 Venue 的启用开关。未启用的 Venue 不注册到 VenueRegistry，worker 跳过。
#[derive(Debug, Clone)]
pub struct VenueToggles {
    pub polymarket: bool,
    pub kalshi: bool,
    pub manifold: bool,
    pub zeitgeist: bool,
    pub azuro: bool,
}

/// worker 触发间隔（秒）。单进程多 worker，各自独立 tokio task。
#[derive(Debug, Clone)]
pub struct WorkerIntervals {
    pub ingest_secs: u64,
    pub mapping_secs: u64,
    pub identity_secs: u64,
    pub perf_secs: u64,
    pub hot_secs: u64,
    /// official_pnl worker 间隔（秒）：抓 Polymarket 排行榜官方盈亏写回 `official_pnl` 列。
    pub official_pnl_secs: u64,
    /// 每 tick 拉取 `/value` 快照的候选地址上限（缓解 rate limit）。非榜地址靠积累的快照算 delta。
    pub official_value_batch: i64,
    /// backfill worker 间隔（秒）：异步回填 raw_trades。
    pub backfill_secs: u64,
    /// backfill 每轮拉取的交易者数量上限（缓解 Polymarket rate limit）。
    pub backfill_batch: u32,
    /// backfill refresh 窗口（天）：超过此时间未回填的交易者增量重拉新成交。
    pub backfill_refresh_days: u32,
    /// signal_replay worker 间隔（秒）：扫 signal_outbox 重发未投递信号（H4 修复）。
    pub signal_replay_secs: u64,
}

fn parse_bool(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn parse_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn parse_f64(name: &str, default: f64) -> f64 {
    env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

impl Config {
    /// 从环境变量加载。缺失项回退默认值（便于本地开发）。
    pub fn from_env() -> Self {
        Self {
            listen_addr: env::var("VENUE_HUB_LISTEN_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:8081".into()),
            database_url: env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://sharpside:sharpside@127.0.0.1:5432/sharpside".into()
            }),
            db_max_connections: env::var("DB_MAX_CONNECTIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            venues: VenueToggles {
                polymarket: parse_bool("VENUE_POLYMARKET_ENABLED", true),
                kalshi: parse_bool("VENUE_KALSHI_ENABLED", false),
                manifold: parse_bool("VENUE_MANIFOLD_ENABLED", false),
                zeitgeist: parse_bool("VENUE_ZEITGEIST_ENABLED", false),
                azuro: parse_bool("VENUE_AZURO_ENABLED", false),
            },
            workers: WorkerIntervals {
                ingest_secs: parse_u64("WORKER_INGEST_SECS", 300),
                mapping_secs: parse_u64("WORKER_MAPPING_SECS", 600),
                identity_secs: parse_u64("WORKER_IDENTITY_SECS", 600),
                perf_secs: parse_u64("WORKER_PERF_SECS", 900),
                hot_secs: parse_u64("WORKER_HOT_SECS", 30),
                official_pnl_secs: parse_u64("WORKER_OFFICIAL_PNL_SECS", 600),
                official_value_batch: parse_u64("WORKER_OFFICIAL_VALUE_BATCH", 100) as i64,
                backfill_secs: parse_u64("WORKER_BACKFILL_SECS", 120),
                backfill_batch: parse_u64("WORKER_BACKFILL_BATCH", 25) as u32,
                backfill_refresh_days: parse_u64("WORKER_BACKFILL_REFRESH_DAYS", 7) as u32,
                signal_replay_secs: parse_u64("WORKER_SIGNAL_REPLAY_SECS", 15),
            },
            auto_match_threshold: parse_f64("AUTO_MATCH_THRESHOLD", 0.7),
            identity_threshold: parse_f64("IDENTITY_THRESHOLD", 0.6),
            shadow_secs: parse_u64("WORKER_SHADOW_SECS", 1800),
            shadow_dry_run: parse_bool("SHADOW_DRY_RUN", true),
            shadow_third_party_url: env::var("SHADOW_THIRD_PARTY_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:9100".into()),
            polymarket_data_api: env::var("POLYMARKET_DATA_API_URL")
                .ok()
                .filter(|s| !s.is_empty()),
            polymarket_gamma_api: env::var("POLYMARKET_GAMMA_API_URL")
                .ok()
                .filter(|s| !s.is_empty()),
            polymarket_clob_api: env::var("POLYMARKET_CLOB_API_URL")
                .ok()
                .filter(|s| !s.is_empty()),
            follow_url: env::var("FOLLOW_URL").unwrap_or_else(|_| "http://127.0.0.1:8082".into()),
            follow_signal_secret: sharpside_shared::secrets::assert_secret(
                "FOLLOW_SIGNAL_SECRET",
                &env::var("FOLLOW_SIGNAL_SECRET").unwrap_or_default(),
            )
            .to_string(),
            admin_token: sharpside_shared::secrets::assert_secret(
                "VENUE_HUB_ADMIN_TOKEN",
                &env::var("VENUE_HUB_ADMIN_TOKEN")
                    .unwrap_or_else(|_| "dev-admin-token".into()),
            )
            .to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        std::env::remove_var("VENUE_HUB_LISTEN_ADDR");
        std::env::remove_var("DATABASE_URL");
        let c = Config::from_env();
        assert_eq!(c.listen_addr, "0.0.0.0:8081");
        assert!(c.database_url.starts_with("postgres://"));
        assert!(c.venues.polymarket);
        assert!(!c.venues.kalshi);
        assert!(c.workers.hot_secs <= c.workers.ingest_secs);
        assert!((0.0..=1.0).contains(&c.auto_match_threshold));
    }
}
