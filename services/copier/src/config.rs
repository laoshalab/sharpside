//! 环境变量配置。对应 `docs/ARCHITECTURE.md` §6.3 / `docs/FLOWS.md` §6-10。

use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_addr: String,
    pub database_url: String,
    pub db_max_connections: u32,
    /// 通道 A 执行 worker 轮询间隔（秒）
    pub worker_exec_secs: u64,
    /// 干跑模式：不调 Venue::place_order，合成成交回报。离线/无凭证环境用 true。
    pub dry_run: bool,
    /// 风控默认参数（全局档；档位/用户/Venue 覆盖见 risk.rs）
    pub daily_max_notional: f64,
    pub max_open_positions: u32,
    pub rapid_flip_window_secs: i64,
    pub rapid_flip_max_count: u32,
    /// 连续亏损/失败熔断阈值（达到即跳过后续指令）
    pub consecutive_loss_limit: i32,
    /// Deposit wallet 最低 pUSD 余额（下单前校验；低于则 skip）。0 = 不校验。
    pub min_dw_balance: f64,
    /// 提现单笔下限（pUSD 人类单位）。低于则拒。0 = 不校验。
    pub withdraw_min_amount: f64,
    /// 提现单笔上限（pUSD 人类单位）。0 = 不校验。
    pub withdraw_max_amount: f64,
    /// 提现日累计上限（pUSD 人类单位，pending+mined 计入）。0 = 不校验。
    pub withdraw_daily_max: f64,
    /// 自动赎回 worker 轮询间隔（秒）。扫新结算市场 → 对有仓位用户自动赎回。
    pub worker_redeem_secs: u64,
    /// 自动赎回是否启用（false = 仅手动端点，worker 不跑）。默认 true（纯收益操作，建议开）。
    pub redeem_worker_enabled: bool,
    /// dispatched 超时回收 worker 轮询间隔（秒）。扫卡在 dispatched 的指令。
    pub worker_reclaim_secs: u64,
    /// dispatched 超时阈值（秒）：dispatched_at 早于 now()-此值的指令被回收为 failed。
    /// 取足够长以覆盖 place_order 正常耗时（含网络/签名/上链确认），避免误伤在途单。
    pub dispatched_timeout_secs: u64,
    /// dispatched 回收 worker 是否启用。默认 true（安全网，仅置 failed 不重下）。
    pub reclaim_worker_enabled: bool,
    /// 成交对账 worker 轮询间隔（秒）。扫 submitted 指令查 Venue 真实成交回写。
    pub worker_reconcile_secs: u64,
    /// submitted 超时阈值（秒）：submitted_at 早于 now()-此值且仍 LIVE 的订单撤单置 cancelled。
    /// 取足够长覆盖限价单正常成交窗口；过短会过早撤掉尚未成交的挂单。
    pub reconcile_timeout_secs: u64,
    /// 成交对账 worker 是否启用。默认 true（Channel A 真钱路径必需，否则 submitted 单永久卡死）。
    pub reconcile_worker_enabled: bool,
    /// JWT 签名密钥（与 account/gateway 共用，校验用户态端点的 Bearer token）。
    pub jwt_secret: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            listen_addr: env::var("COPIER_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8083".into()),
            database_url: env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://sharpside:sharpside@127.0.0.1:5432/sharpside".into()
            }),
            db_max_connections: env::var("DB_MAX_CONNECTIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            worker_exec_secs: env::var("WORKER_EXEC_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            dry_run: parse_bool("COPIER_DRY_RUN", true),
            daily_max_notional: env::var("RISK_DAILY_MAX_NOTIONAL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5000.0),
            max_open_positions: env::var("RISK_MAX_OPEN_POSITIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(20),
            rapid_flip_window_secs: env::var("RISK_RAPID_FLIP_WINDOW_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(60),
            rapid_flip_max_count: env::var("RISK_RAPID_FLIP_MAX_COUNT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            consecutive_loss_limit: env::var("RISK_CONSECUTIVE_LOSS_LIMIT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3),
            min_dw_balance: env::var("RISK_MIN_DW_BALANCE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5.0),
            withdraw_min_amount: env::var("WITHDRAW_MIN_AMOUNT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.0),
            withdraw_max_amount: env::var("WITHDRAW_MAX_AMOUNT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10000.0),
            withdraw_daily_max: env::var("WITHDRAW_DAILY_MAX")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10000.0),
            worker_redeem_secs: env::var("WORKER_REDEEM_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(300),
            redeem_worker_enabled: parse_bool("REDEEM_WORKER_ENABLED", true),
            worker_reclaim_secs: env::var("WORKER_RECLAIM_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(60),
            dispatched_timeout_secs: env::var("DISPATCHED_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(600),
            reclaim_worker_enabled: parse_bool("RECLAIM_WORKER_ENABLED", true),
            worker_reconcile_secs: env::var("WORKER_RECONCILE_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(15),
            reconcile_timeout_secs: env::var("RECONCILE_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(120),
            reconcile_worker_enabled: parse_bool("RECONCILE_WORKER_ENABLED", true),
            jwt_secret: sharpside_shared::secrets::assert_secret(
                "JWT_SECRET",
                &env::var("JWT_SECRET").unwrap_or_else(|_| "dev-secret-change-me".into()),
            )
            .to_string(),
        }
    }
}

fn parse_bool(key: &str, default: bool) -> bool {
    match env::var(key).ok().as_deref() {
        Some("true") | Some("1") | Some("yes") => true,
        Some("false") | Some("0") | Some("no") => false,
        _ => default,
    }
}

// 管辖域 → 允许的 execution_venue 集合已下沉到 `sharpside_shared::allowed_execute_venues`，
// 供 follow（创建时前置校验）/ copier（执行时兜底）/ gateway（BFF 展示）共用，避免重复。
// 早期版本本文件有一份拷贝，已删除——见 `crates/shared/src/jurisdiction.rs`。

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_default_true() {
        std::env::remove_var("COPIER_DRY_RUN");
        assert!(Config::from_env().dry_run);
    }
}
