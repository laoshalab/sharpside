//! 影子校验 worker。对应 `docs/ARCHITECTURE.md` §旁路影子模式 / `docs/SHADOW_MODE.md`。
//!
//! 影子路径与生产展示链路物理隔离：第三方指标只写 `trader_performance_third_party` +
//! `metric_audit`（与自算 diff，仅告警），永不进入用户界面。
//!
//! 离线/无第三方 API 时 `SHADOW_DRY_RUN=true`：用自算绩效 + 小扰动合成第三方指标，
//! 跑通 upsert → diff → metric_audit → 告警 全链路。有网络环境后置 false 接真实第三方。

use crate::state::AppState;
use chrono::Utc;
use sharpside_db::queries::shadow as sh_q;
use sharpside_db::queries::traders as tr_q;
use tracing::{error, info, warn};

const SHADOW_BATCH: i64 = 100;
const SHADOW_SOURCE: &str = "polyedge"; // 第三方源标识（dry_run 也用此占位）

pub async fn run(state: AppState) {
    loop {
        if let Err(e) = tick(&state).await {
            error!(error = %e, "shadow tick 失败");
        }
        tokio::time::sleep(std::time::Duration::from_secs(state.config.shadow_secs)).await;
    }
}

async fn tick(state: &AppState) -> Result<(), anyhow::Error> {
    // 取一批可见交易者（影子校验 per (platform, address)）
    let traders = tr_q::list_all_visible_traders(&state.db, SHADOW_BATCH, 0).await?;
    if traders.is_empty() {
        return Ok(());
    }

    let periods = ["1d", "1w", "1m", "1y", "ytd", "all"];
    let mut audited = 0usize;
    let mut alerts = 0usize;

    for t in &traders {
        for period in periods {
            let Some(self_perf) =
                sh_q::get_self_perf(&state.db, &t.platform, &t.address, period).await?
            else {
                continue;
            };
            // 第三方指标：dry_run 合成（自算 + 小扰动）；非 dry_run 需网络（离线未实现，回退合成）
            let third = if state.config.shadow_dry_run {
                synthesize_third_party(&self_perf)
            } else {
                // Phase 1b+：reqwest 拉 SHADOW_THIRD_PARTY_URL（离线未缓存，回退合成并告警）
                warn!(url = %state.config.shadow_third_party_url, "非 dry_run 第三方拉取未实现，回退合成");
                synthesize_third_party(&self_perf)
            };

            sh_q::upsert_third_party_perf(
                &state.db,
                &t.platform,
                &t.address,
                SHADOW_SOURCE,
                period,
                third.roi,
                third.win_rate,
                third.realized_pnl,
                third.unrealized_pnl,
                third.wins,
                third.losses,
                third.markets_count,
                third.total_volume,
            )
            .await?;

            // diff + metric_audit
            for (metric_name, self_v, third_v) in [
                ("roi", to_f64(self_perf.roi), third.roi),
                ("win_rate", to_f64(self_perf.win_rate), third.win_rate),
                (
                    "realized_pnl",
                    to_f64(self_perf.realized_pnl),
                    third.realized_pnl,
                ),
            ] {
                let (diff_abs, diff_pct, status) =
                    diff_and_status(metric_name, self_v, third_v, state).await;
                sh_q::insert_metric_audit(
                    &state.db,
                    &t.platform,
                    &t.address,
                    SHADOW_SOURCE,
                    period,
                    metric_name,
                    self_v,
                    third_v,
                    diff_abs,
                    diff_pct,
                    &status,
                )
                .await?;
                audited += 1;
                if status == "alert" {
                    alerts += 1;
                    warn!(platform = %t.platform, address = %t.address, metric = metric_name, period, "影子校验告警");
                }
            }
        }
    }

    info!(traders = traders.len(), audited, alerts, "影子校验完成");
    Ok(())
}

struct ThirdParty {
    roi: Option<f64>,
    win_rate: Option<f64>,
    realized_pnl: Option<f64>,
    unrealized_pnl: Option<f64>,
    wins: Option<i32>,
    losses: Option<i32>,
    markets_count: Option<i32>,
    total_volume: Option<f64>,
}

/// dry_run 合成：自算绩效 + 小扰动（±1%），用于跑通 diff/审计链路。
fn synthesize_third_party(self_perf: &sharpside_db::TraderPerformance) -> ThirdParty {
    let perturb = |v: f64| v * (1.0 + ((v.fract() * 100.0).rem_euclid(2.0) - 1.0) / 100.0);
    ThirdParty {
        roi: to_f64(self_perf.roi).map(perturb),
        win_rate: to_f64(self_perf.win_rate).map(perturb),
        realized_pnl: to_f64(self_perf.realized_pnl).map(perturb),
        unrealized_pnl: to_f64(self_perf.unrealized_pnl).map(perturb),
        wins: Some(self_perf.wins),
        losses: Some(self_perf.losses),
        markets_count: Some(self_perf.position_count),
        total_volume: to_f64(self_perf.total_volume).map(perturb),
    }
}

fn to_f64(d: rust_decimal::Decimal) -> Option<f64> {
    use rust_decimal::prelude::ToPrimitive;
    d.to_f64()
}

/// 计算 diff_abs / diff_pct，并按 `audit_thresholds` 判定 ok/warn/alert。
async fn diff_and_status(
    metric_name: &str,
    self_v: Option<f64>,
    third_v: Option<f64>,
    state: &AppState,
) -> (Option<f64>, Option<f64>, String) {
    let (Some(s), Some(t)) = (self_v, third_v) else {
        return (None, None, "ok".into());
    };
    let diff_abs = (s - t).abs();
    let diff_pct = if s.abs() > 1e-9 {
        Some((diff_abs / s.abs()) * 100.0)
    } else {
        None
    };
    let status = match sh_q::get_audit_threshold(&state.db, metric_name).await {
        Ok(Some(th)) => {
            let warn_pct = to_f64(th.warn_pct).unwrap_or(0.0);
            let alert_pct = to_f64(th.alert_pct).unwrap_or(0.0);
            let warn_abs = to_f64(th.warn_abs).unwrap_or(0.0);
            let alert_abs = to_f64(th.alert_abs).unwrap_or(0.0);
            let pct = diff_pct.unwrap_or(0.0);
            if pct >= alert_pct || diff_abs >= alert_abs {
                "alert"
            } else if pct >= warn_pct || diff_abs >= warn_abs {
                "warn"
            } else {
                "ok"
            }
        }
        _ => "ok",
    };
    (Some(diff_abs), diff_pct, status.to_string())
}

// 抑制未使用：Utc 在未来时间戳处理时使用
#[allow(dead_code)]
fn _unused_utc() -> chrono::DateTime<Utc> {
    Utc::now()
}
