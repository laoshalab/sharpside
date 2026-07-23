//! 数据库层：sqlx 0.8 + 迁移 + 类型化查询封装。
//!
//! 对应 `docs/VENUEHUB_STORAGE.md`（trader_hub 八层）与 `docs/TRADERS_TABLE.md`。
//! 双 schema 物理隔离：`trader_hub`（交易者数据）+ `account`（用户/跟随/跟单）。
//! 所有业务表带 `platform` 列，`traders` 复合主键 `(platform, address)`，首版即多平台结构。
//!
//! 设计要点：
//! - 使用运行时 `sqlx::query_as`（非编译期 `query_as!` 宏），`cargo check` 无需连接 DB
//! - 迁移用运行时 [`sqlx::migrate::Migrator`] + `include_str!` 编译期嵌入 SQL，
//!   而非 `sqlx::migrate!` 宏——后者依赖 `macros` feature，会经 sqlx-macros-core
//!   非可选地拉入 sqlx-sqlite → flume → yanked `spin 0.9.8`，阻塞离线编译。
//!   有网络环境可切回 `sqlx::migrate!("./migrations")` 以获得编译期校验。
//! - `PgPool` 通过 [`connect`] 创建，通过 [`migrate`] 应用迁移

#![forbid(unsafe_code)]

pub mod error;
pub mod models;
pub mod queries;

use std::borrow::Cow;

use sqlx::migrate::{Migration, MigrationType, Migrator};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub use error::DbError;
pub use models::{
    BillingInvoice, BillingPayment, CopyExecution, CopyOrderRow, CredentialArchive,
    EquityCurveBatchRow, EquityCurvePoint, FollowRelation, HotWallet, Identity, LeaderboardRow,
    MarketMapping, PositionRow, PositionSnapshot, RawMarket, RawTrade, Redemption, Trader,
    TraderPerformance, User, UserVenueCredential, UserWallet, Watchlist, Withdrawal,
};

/// 连接 PostgreSQL 并配置连接池。
///
/// `max_connections` 默认 10，适合单服务；高并发服务可自行调优。
pub async fn connect(database_url: &str, max_connections: u32) -> Result<PgPool, DbError> {
    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await?;
    Ok(pool)
}

/// 应用所有待执行的迁移。
///
/// 迁移文件在 `crates/db/migrations/`，由 `include_str!` 在编译期嵌入，
/// 启动时构造运行时 [`Migrator`] 应用。幂等：已应用的迁移不会重复执行。
pub async fn migrate(pool: &PgPool) -> Result<(), DbError> {
    let migrations = build_migrations();
    let migrator = Migrator {
        migrations: Cow::Owned(migrations),
        ..Migrator::DEFAULT
    };
    migrator
        .run(pool)
        .await
        .map_err(|e| DbError::Migrate(e.to_string()))?;
    Ok(())
}

/// 健康检查：能 ping 通 DB 即可。
pub async fn ping(pool: &PgPool) -> Result<(), DbError> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}

/// 编译期嵌入全部迁移 SQL，按版本号升序构造 [`Migration`] 列表。
///
/// 新增迁移时在此追加一行 `include_str!`。
fn build_migrations() -> Vec<Migration> {
    // (version, description, sql)
    const SOURCES: [(i64, &str, &str); 43] = [
        (
            1,
            "create_schemas",
            include_str!("../migrations/0001_create_schemas.sql"),
        ),
        (
            2,
            "raw_layer",
            include_str!("../migrations/0002_raw_layer.sql"),
        ),
        (
            3,
            "entities",
            include_str!("../migrations/0003_entities.sql"),
        ),
        (4, "mapping", include_str!("../migrations/0004_mapping.sql")),
        (5, "compute", include_str!("../migrations/0005_compute.sql")),
        (6, "monitor", include_str!("../migrations/0006_monitor.sql")),
        (7, "ops", include_str!("../migrations/0007_ops.sql")),
        (8, "shadow", include_str!("../migrations/0008_shadow.sql")),
        (
            9,
            "identity_perf_mview",
            include_str!("../migrations/0009_identity_perf_mview.sql"),
        ),
        (
            10,
            "account",
            include_str!("../migrations/0010_account.sql"),
        ),
        (
            11,
            "user_venue_credentials",
            include_str!("../migrations/0011_user_venue_credentials.sql"),
        ),
        (
            12,
            "deposit_wallet",
            include_str!("../migrations/0012_deposit_wallet.sql"),
        ),
        (
            13,
            "perf_periods",
            include_str!("../migrations/0013_perf_periods.sql"),
        ),
        (
            14,
            "equity_curve_hourly",
            include_str!("../migrations/0014_equity_curve_hourly.sql"),
        ),
        (
            15,
            "wallet_login",
            include_str!("../migrations/0015_wallet_login.sql"),
        ),
        (
            16,
            "drop_email_auth",
            include_str!("../migrations/0016_drop_email_auth.sql"),
        ),
        (
            17,
            "credential_kind",
            include_str!("../migrations/0017_credential_kind.sql"),
        ),
        (
            18,
            "trades_backfilled_at",
            include_str!("../migrations/0018_trades_backfilled_at.sql"),
        ),
        (
            19,
            "withdrawals",
            include_str!("../migrations/0019_withdrawals.sql"),
        ),
        (
            20,
            "leaderboard_category",
            include_str!("../migrations/0020_leaderboard_category.sql"),
        ),
        (
            21,
            "botfilter_seed",
            include_str!("../migrations/0021_botfilter_seed.sql"),
        ),
        (
            22,
            "watchlist",
            include_str!("../migrations/0022_watchlist.sql"),
        ),
        (
            23,
            "polymarket_official_categories",
            include_str!("../migrations/0023_polymarket_official_categories.sql"),
        ),
        (
            24,
            "official_pnl",
            include_str!("../migrations/0024_official_pnl.sql"),
        ),
        (
            25,
            "redemptions",
            include_str!("../migrations/0025_redemptions.sql"),
        ),
        (
            26,
            "value_snapshot",
            include_str!("../migrations/0026_value_snapshot.sql"),
        ),
        (
            27,
            "follow_unique",
            include_str!("../migrations/0027_follow_unique.sql"),
        ),
        (
            28,
            "follow_deleted_at",
            include_str!("../migrations/0028_follow_deleted_at.sql"),
        ),
        (
            29,
            "copy_order_dispatched_at",
            include_str!("../migrations/0029_copy_order_dispatched_at.sql"),
        ),
        (
            30,
            "signal_outbox",
            include_str!("../migrations/0030_signal_outbox.sql"),
        ),
        (
            31,
            "copy_order_signal_id",
            include_str!("../migrations/0031_copy_order_signal_id.sql"),
        ),
        (
            32,
            "migrate_percent_of_balance",
            include_str!("../migrations/0032_migrate_percent_of_balance.sql"),
        ),
        (
            33,
            "copy_order_submission",
            include_str!("../migrations/0033_copy_order_submission.sql"),
        ),
        (
            34,
            "copy_order_idempotency",
            include_str!("../migrations/0034_copy_order_idempotency.sql"),
        ),
        (
            35,
            "jwt_denylist",
            include_str!("../migrations/0035_jwt_denylist.sql"),
        ),
        (
            36,
            "copy_execution_unique",
            include_str!("../migrations/0036_copy_execution_unique.sql"),
        ),
        (
            37,
            "credential_passphrase_enc",
            include_str!("../migrations/0037_credential_passphrase_enc.sql"),
        ),
        (
            38,
            "credential_revoked",
            include_str!("../migrations/0038_credential_revoked.sql"),
        ),
        (
            39,
            "credential_archives",
            include_str!("../migrations/0039_credential_archives.sql"),
        ),
        (
            40,
            "billing",
            include_str!("../migrations/0040_billing.sql"),
        ),
        (
            41,
            "redemptions_deposit_wallet",
            include_str!("../migrations/0041_redemptions_deposit_wallet.sql"),
        ),
        (
            42,
            "copy_status_submitted",
            include_str!("../migrations/0042_copy_status_submitted.sql"),
        ),
        (
            43,
            "signal_ledger_trades_index",
            include_str!("../migrations/0043_signal_ledger_trades_index.sql"),
        ),
    ];

    SOURCES
        .into_iter()
        .map(|(version, description, sql)| {
            Migration::new(
                version,
                Cow::Borrowed(description),
                MigrationType::Simple,
                Cow::Borrowed(sql),
                false,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_migrations_has_ten_in_order() {
        let migrations = build_migrations();
        assert_eq!(migrations.len(), 43, "迁移数量应为 43");
        for w in migrations.windows(2) {
            assert!(
                w[0].version < w[1].version,
                "迁移应按版本升序：{} < {}",
                w[0].version,
                w[1].version
            );
        }
        assert_eq!(migrations[0].version, 1);
        assert_eq!(migrations[13].version, 14);
        assert_eq!(migrations[14].version, 15);
        assert_eq!(migrations[15].version, 16);
        assert_eq!(migrations[16].version, 17);
        assert_eq!(migrations[17].version, 18);
        assert_eq!(migrations[18].version, 19);
        assert_eq!(migrations[19].version, 20);
        assert_eq!(migrations[20].version, 21);
        assert_eq!(migrations[21].version, 22);
        assert_eq!(migrations[22].version, 23);
        assert_eq!(migrations[24].version, 25);
    }

    #[test]
    fn migrations_have_non_empty_sql() {
        for m in build_migrations() {
            assert!(
                !m.sql.trim().is_empty(),
                "迁移 {} 的 SQL 不应为空",
                m.version
            );
        }
    }
}
