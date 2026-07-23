//! 风控引擎。对应 `docs/FLOWS.md` §10 三级覆盖 + `docs/ARCHITECTURE.md` §6.3 统一风控。
//!
//! 三级覆盖：全局默认（copier 配置）→ 档位（free/pro_plus）→ 用户（risk_overrides）→ Venue（ExecParams）。
//! [`RiskLimits`] 由 [`effective_limits`] 装配后传入 [`check_risk`]。
//!
//! 校验项：min_notional、daily_max_notional、max_open_positions、rapid-flip 守卫、连续亏损熔断。
//! 滑点保护需 `Venue::book()`，dry_run 模式跳过（无网络），非 dry_run 在 exec.rs 调用前由
//! [`check_slippage`] 校验。

use crate::config::Config;
use serde::Deserialize;

/// 风控统计快照（由 exec.rs 从 db 查询后注入）。
#[derive(Debug, Clone, Copy)]
pub struct RiskContext {
    pub daily_notional_used: f64,
    pub open_positions: i64,
    pub recent_orders_in_window: i64,
    /// 最近 N 条 copy_order 中从最新向前数的连续失败（failed/skipped）条数
    pub consecutive_failures: i32,
}

/// 生效风控限额（三级覆盖合并 + Venue 执行参数）。
#[derive(Debug, Clone, Copy)]
pub struct RiskLimits {
    pub min_notional: f64,
    pub max_slippage_bps: f64,
    pub daily_max_notional: f64,
    pub max_open_positions: i64,
    pub rapid_flip_max_count: i64,
    /// 连续亏损/失败熔断阈值；达到即跳过
    pub consecutive_loss_limit: i32,
    /// 最小下单股数（0 = 不校验；>0 时 size < 此值拒单）。来自 Venue 服务端元数据。
    pub min_size: f64,
}

/// 用户级风控覆盖（`account.users.risk_overrides` jsonb 子集）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UserRiskOverrides {
    #[serde(default)]
    pub daily_max_notional: Option<f64>,
    #[serde(default)]
    pub max_open_positions: Option<i64>,
    #[serde(default)]
    pub rapid_flip_max_count: Option<i64>,
    #[serde(default)]
    pub consecutive_loss_limit: Option<i32>,
}

/// 档位缩放系数。Pro+ 解锁更高额度。
fn tier_multipliers(tier: &str) -> (f64, f64, f64) {
    // (daily_max 乘数, max_open 乘数, rapid_flip_max 乘数)
    match tier {
        "pro_plus" => (3.0, 2.0, 2.0),
        _ => (1.0, 1.0, 1.0),
    }
}

/// 装配生效限额：全局默认 × 档位缩放 → 用户覆盖 → Venue 执行参数（min_notional / slippage / min_size）。
pub fn effective_limits(
    cfg: &Config,
    tier: &str,
    overrides: &UserRiskOverrides,
    exec: &ExecLimits,
) -> RiskLimits {
    let (dm_mul, mo_mul, rf_mul) = tier_multipliers(tier);
    let daily = overrides
        .daily_max_notional
        .unwrap_or(cfg.daily_max_notional * dm_mul);
    let open = overrides
        .max_open_positions
        .unwrap_or((cfg.max_open_positions as f64 * mo_mul) as i64);
    let rapid = overrides
        .rapid_flip_max_count
        .unwrap_or((cfg.rapid_flip_max_count as f64 * rf_mul) as i64);
    let consec = overrides
        .consecutive_loss_limit
        .unwrap_or(cfg.consecutive_loss_limit);
    RiskLimits {
        min_notional: exec.min_notional,
        max_slippage_bps: exec.max_slippage_bps,
        daily_max_notional: daily,
        max_open_positions: open,
        rapid_flip_max_count: rapid,
        consecutive_loss_limit: consec,
        min_size: exec.min_size,
    }
}

/// Venue 侧执行参数（从 `ExecParams` 提取）。
#[derive(Debug, Clone, Copy)]
pub struct ExecLimits {
    pub min_notional: f64,
    pub max_slippage_bps: f64,
    /// 最小下单股数（0 = 不校验）。
    pub min_size: f64,
}

/// 校验单笔指令。返回 `Err(reason)` 表示跳过（写 copy_order.status=skipped）。
///
/// `size` 为下单股数（经单位换算后的 exec_size），用于 min_size 校验。
pub fn check_risk(
    ctx: RiskContext,
    notional: f64,
    size: f64,
    limits: &RiskLimits,
) -> Result<(), String> {
    if notional < limits.min_notional {
        return Err(format!(
            "notional {notional:.2} 低于 min {}",
            limits.min_notional
        ));
    }
    // 股数下限：Polymarket 每市场 minimum_order_size 不同，下单前校验避免撞服务端 400。
    if limits.min_size > 0.0 && size < limits.min_size {
        return Err(format!(
            "股数 {size} 低于市场最小 {min}",
            min = limits.min_size
        ));
    }
    if limits.daily_max_notional > 0.0
        && ctx.daily_notional_used + notional > limits.daily_max_notional
    {
        return Err(format!(
            "日累计 notional {}/{} 超 {}",
            ctx.daily_notional_used, notional, limits.daily_max_notional
        ));
    }
    if limits.max_open_positions > 0 && ctx.open_positions >= limits.max_open_positions {
        return Err(format!(
            "持仓数 {} 达上限 {}",
            ctx.open_positions, limits.max_open_positions
        ));
    }
    if limits.rapid_flip_max_count > 0 && ctx.recent_orders_in_window >= limits.rapid_flip_max_count
    {
        return Err(format!(
            "rapid-flip 守卫：{} 秒内 {} 笔超 {}",
            "window", ctx.recent_orders_in_window, limits.rapid_flip_max_count
        ));
    }
    if limits.consecutive_loss_limit > 0
        && ctx.consecutive_failures >= limits.consecutive_loss_limit
    {
        return Err(format!(
            "连续亏损/失败熔断：{} ≥ {}",
            ctx.consecutive_failures, limits.consecutive_loss_limit
        ));
    }
    Ok(())
}

/// Per-follow 风控限额（来自 FollowConfig，独立于全局/档位/用户覆盖）。
/// 0 表示不限制。
#[derive(Debug, Clone, Copy, Default)]
pub struct FollowRiskLimits {
    pub daily_max_notional: f64,
    pub max_open_positions: i64,
}

/// Per-follow 风控上下文（仅该跟随关系的累计/持仓）。
#[derive(Debug, Clone, Copy)]
pub struct FollowRiskContext {
    pub daily_notional_used: f64,
    pub open_positions: i64,
}

/// 校验单笔指令是否超出 per-follow 限额。返回 `Err(reason)` 表示跳过。
/// 在全局 `check_risk` 之外额外约束单条跟随关系的日累计与持仓数。
pub fn check_follow_risk(
    ctx: FollowRiskContext,
    notional: f64,
    limits: &FollowRiskLimits,
) -> Result<(), String> {
    if limits.daily_max_notional > 0.0 && ctx.daily_notional_used + notional > limits.daily_max_notional
    {
        return Err(format!(
            "per-follow 日累计 notional {}/{} 超 {}",
            ctx.daily_notional_used, notional, limits.daily_max_notional
        ));
    }
    if limits.max_open_positions > 0 && ctx.open_positions >= limits.max_open_positions {
        return Err(format!(
            "per-follow 持仓数 {} 达上限 {}",
            ctx.open_positions, limits.max_open_positions
        ));
    }
    Ok(())
}

/// 滑点保护：`(order_price - mid) / mid` 超过 `max_slippage_bps` 拒单。
/// `book_best` = (best_bid, best_ask)；mid 为二者均值。
pub fn check_slippage(
    order_price: f64,
    best_bid: f64,
    best_ask: f64,
    max_slippage_bps: f64,
) -> Result<(), String> {
    if best_bid <= 0.0 || best_ask <= 0.0 {
        return Err("盘口为空，无法计算 mid".into());
    }
    let mid = (best_bid + best_ask) / 2.0;
    if mid <= 0.0 {
        return Err("mid<=0".into());
    }
    let slip_bps = ((order_price - mid).abs() / mid) * 10_000.0;
    if slip_bps > max_slippage_bps {
        return Err(format!("滑点 {slip_bps:.0}bps 超 {max_slippage_bps:.0}bps"));
    }
    Ok(())
}

/// 从最近 status 列表计算尾部连续失败条数（failed/skipped 视为失败）。
pub fn count_trailing_failures(statuses: &[String]) -> i32 {
    statuses
        .iter()
        .take_while(|s| matches!(s.as_str(), "failed" | "skipped"))
        .count() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        Config {
            listen_addr: "0.0.0.0:8083".into(),
            database_url: "x".into(),
            db_max_connections: 1,
            worker_exec_secs: 5,
            dry_run: true,
            daily_max_notional: 1000.0,
            max_open_positions: 10,
            rapid_flip_window_secs: 60,
            rapid_flip_max_count: 5,
            consecutive_loss_limit: 3,
            min_dw_balance: 5.0,
            withdraw_min_amount: 1.0,
            withdraw_max_amount: 10000.0,
            withdraw_daily_max: 10000.0,
            worker_redeem_secs: 300,
            redeem_worker_enabled: true,
            worker_reclaim_secs: 60,
            dispatched_timeout_secs: 600,
            reclaim_worker_enabled: true,
            worker_reconcile_secs: 15,
            reconcile_timeout_secs: 120,
            reconcile_worker_enabled: true,
            jwt_secret: "test-secret".into(),
        }
    }

    fn exec_limits() -> ExecLimits {
        ExecLimits {
            min_notional: 1.0,
            max_slippage_bps: 200.0,
            min_size: 0.0,
        }
    }

    fn ctx() -> RiskContext {
        RiskContext {
            daily_notional_used: 0.0,
            open_positions: 0,
            recent_orders_in_window: 0,
            consecutive_failures: 0,
        }
    }

    fn fctx(used: f64, open: i64) -> FollowRiskContext {
        FollowRiskContext {
            daily_notional_used: used,
            open_positions: open,
        }
    }

    #[test]
    fn follow_daily_max_exceeded_skipped() {
        let limits = FollowRiskLimits {
            daily_max_notional: 100.0,
            max_open_positions: 0,
        };
        let err = check_follow_risk(fctx(80.0, 0), 30.0, &limits).unwrap_err();
        assert!(err.contains("per-follow 日累计"));
    }

    #[test]
    fn follow_max_open_positions_skipped() {
        let limits = FollowRiskLimits {
            daily_max_notional: 0.0,
            max_open_positions: 5,
        };
        let err = check_follow_risk(fctx(0.0, 5), 10.0, &limits).unwrap_err();
        assert!(err.contains("per-follow 持仓数"));
    }

    #[test]
    fn follow_zero_limits_allow_all() {
        let limits = FollowRiskLimits::default();
        assert!(check_follow_risk(fctx(999.0, 999), 999.0, &limits).is_ok());
    }

    #[test]
    fn pro_plus_tier_scales_limits() {
        let limits = effective_limits(
            &cfg(),
            "pro_plus",
            &UserRiskOverrides::default(),
            &exec_limits(),
        );
        assert_eq!(limits.daily_max_notional, 3000.0);
        assert_eq!(limits.max_open_positions, 20);
        assert_eq!(limits.rapid_flip_max_count, 10);
    }

    #[test]
    fn user_overrides_win_over_tier() {
        let ov = UserRiskOverrides {
            daily_max_notional: Some(999.0),
            max_open_positions: Some(7),
            rapid_flip_max_count: None,
            consecutive_loss_limit: None,
        };
        let limits = effective_limits(&cfg(), "pro_plus", &ov, &exec_limits());
        assert_eq!(limits.daily_max_notional, 999.0);
        assert_eq!(limits.max_open_positions, 7);
        assert_eq!(limits.rapid_flip_max_count, 10); // 档位缩放保留
    }

    #[test]
    fn consecutive_loss_breaker_skips() {
        let mut c = ctx();
        c.consecutive_failures = 3;
        let limits = effective_limits(
            &cfg(),
            "free",
            &UserRiskOverrides::default(),
            &exec_limits(),
        );
        let r = check_risk(c, 50.0, 100.0, &limits);
        assert!(r.unwrap_err().contains("连续亏损"));
    }

    #[test]
    fn below_min_notional_skipped() {
        let limits = effective_limits(
            &cfg(),
            "free",
            &UserRiskOverrides::default(),
            &exec_limits(),
        );
        let r = check_risk(ctx(), 0.5, 100.0, &limits);
        assert!(r.unwrap_err().contains("min"));
    }

    #[test]
    fn daily_max_exceeded_skipped() {
        let mut c = ctx();
        c.daily_notional_used = 950.0;
        let limits = effective_limits(
            &cfg(),
            "free",
            &UserRiskOverrides::default(),
            &exec_limits(),
        );
        let r = check_risk(c, 100.0, 100.0, &limits);
        assert!(r.unwrap_err().contains("日累计"));
    }

    #[test]
    fn max_open_positions_skipped() {
        let mut c = ctx();
        c.open_positions = 10;
        let limits = effective_limits(
            &cfg(),
            "free",
            &UserRiskOverrides::default(),
            &exec_limits(),
        );
        let r = check_risk(c, 10.0, 100.0, &limits);
        assert!(r.unwrap_err().contains("持仓数"));
    }

    #[test]
    fn rapid_flip_skipped() {
        let mut c = ctx();
        c.recent_orders_in_window = 5;
        let limits = effective_limits(
            &cfg(),
            "free",
            &UserRiskOverrides::default(),
            &exec_limits(),
        );
        let r = check_risk(c, 10.0, 100.0, &limits);
        assert!(r.unwrap_err().contains("rapid-flip"));
    }

    #[test]
    fn ok_when_all_clear() {
        let limits = effective_limits(
            &cfg(),
            "free",
            &UserRiskOverrides::default(),
            &exec_limits(),
        );
        assert!(check_risk(ctx(), 50.0, 100.0, &limits).is_ok());
    }

    #[test]
    fn below_min_size_skipped() {
        let exec = ExecLimits {
            min_notional: 1.0,
            max_slippage_bps: 200.0,
            min_size: 5.0,
        };
        let limits = effective_limits(&cfg(), "free", &UserRiskOverrides::default(), &exec);
        // notional=2.5 >= 1.0 过，但 size=3 < 5 → 拒
        let r = check_risk(ctx(), 2.5, 3.0, &limits);
        let err = r.unwrap_err();
        assert!(err.contains("股数"), "got: {err}");
        assert!(err.contains("3"));
        assert!(err.contains("5"));
    }

    #[test]
    fn min_size_zero_skips_check() {
        // min_size=0 → 不校验股数，size=1 也过
        let limits = effective_limits(
            &cfg(),
            "free",
            &UserRiskOverrides::default(),
            &exec_limits(),
        );
        assert!(check_risk(ctx(), 50.0, 1.0, &limits).is_ok());
    }

    #[test]
    fn slippage_within_limit_ok() {
        assert!(check_slippage(0.51, 0.50, 0.52, 200.0).is_ok());
    }

    #[test]
    fn slippage_exceeded_rejected() {
        // mid=0.50, order=0.60 → 2000bps > 200
        assert!(check_slippage(0.60, 0.49, 0.51, 200.0).is_err());
    }

    #[test]
    fn trailing_failures_count() {
        assert_eq!(count_trailing_failures(&[]), 0);
        assert_eq!(
            count_trailing_failures(&["failed".into(), "skipped".into(), "filled".into()]),
            2
        );
        assert_eq!(
            count_trailing_failures(&["filled".into(), "failed".into()]),
            0
        );
    }
}
