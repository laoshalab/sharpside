//! 自动赎回 worker。对应 `docs/CHANNEL_A_SIGNING.md` §4.2 与 migration 0025。
//!
//! 定期扫 `raw_markets WHERE closed=true AND resolved_at > $last_scan`（游标推进），
//! 对每个新结算市场：判定赢方 → 查跟单过该市场的用户（copy_execution 候选集）
//! → 链上 CTF balanceOf 确认有仓位 → 落库 redemptions(pending, auto)
//! → `venue.redeem`（owner 签 WALLET batch 调 CTF.redeemPositions）→ 更新状态。
//!
//! 设计要点：
//! - **游标推进**：每轮只处理 `resolved_at > last_scan` 的新结算市场，避免全表扫。
//!   首跑 last_scan=None（回填全部已结算市场）。
//! - **防重复**：DB 唯一约束 `(user, condition_id, outcome) WHERE status IN (pending,mined)`
//!   兜底；insert_redemption 冲突返回 Conflict，worker 跳过。
//! - **失败重试**：failed 的赎回有限重试（attempts<3，指数退避 30s/120s/300s）。
//!   瞬态失败（relayer 5xx / RPC 超时）可恢复；重试前先查链上 balanceOf，0 则标 mined（上轮已成功）。
//!   永久错误（KMS/签名/neg-risk）会耗尽 attempts 后保留 failed 交人工。
//! - **纯收益操作**：赎回无金额风控（赢仓位换 pUSD，1:1，无市场风险）。

use crate::state::AppState;
use sharpside_db::queries::account as acct;
use sharpside_db::queries::raw;
use sharpside_venues_core::{Credential, Venue};
use tracing::{error, info, warn};

/// 内存游标：上次扫到的 resolved_at。进程重启从 None（回填）开始。
/// 生产可持久化到 ops 表避免重启重扫，MVP 内存即可（已赎回的会被唯一约束挡）。
static LAST_SCAN: std::sync::OnceLock<std::sync::Mutex<Option<chrono::DateTime<chrono::Utc>>>> =
    std::sync::OnceLock::new();

pub async fn run(state: AppState) {
    if !state.config.redeem_worker_enabled {
        info!("自动赎回 worker 已禁用（REDEEM_WORKER_ENABLED=false）");
        return;
    }
    let cursor = LAST_SCAN.get_or_init(|| std::sync::Mutex::new(None));
    loop {
        if let Err(e) = tick(&state, cursor).await {
            error!(error = %e, "赎回 worker tick 失败");
        }
        tokio::time::sleep(std::time::Duration::from_secs(
            state.config.worker_redeem_secs.max(1),
        ))
        .await;
    }
}

async fn tick(
    state: &AppState,
    cursor: &std::sync::Mutex<Option<chrono::DateTime<chrono::Utc>>>,
) -> Result<(), anyhow::Error> {
    let since = *cursor.lock().unwrap();
    let markets = raw::list_resolved_markets(&state.db, "polymarket", since).await?;
    if markets.is_empty() {
        // 无新结算市场，仍可能有待重试的 failed 赎回。
    } else {
        info!(n = markets.len(), since = ?since, "赎回 worker 扫到新结算市场");
    }

    // P0-8：重试可恢复的 failed 赎回（relayer/RPC 瞬态失败）。
    // 在新市场扫描之前处理，避免新市场积压阻塞重试。
    if let Err(e) = retry_failed_redemptions(state).await {
        warn!(error = %e, "failed 赎回重试批次失败");
    }

    if markets.is_empty() {
        return Ok(());
    }

    let venue_impl = match state.registry.get(sharpside_shared::Platform::Polymarket) {
        Some(v) => v.clone(),
        None => {
            warn!("polymarket venue 未注册，赎回 worker 跳过");
            return Ok(());
        }
    };

    let rpc_url = std::env::var("POLYGON_RPC_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| crate::routes::POLYGON_RPC_DEFAULT_FALLBACK.to_string());
    let pusd = crate::routes::parse_address_const(crate::routes::PUSD_CONST)
        .map_err(|e| anyhow::anyhow!(e))?;
    let ctf = crate::routes::parse_address_const(crate::routes::CTF_CONST)
        .map_err(|e| anyhow::anyhow!(e))?;

    let mut max_resolved = since;
    for m in markets {
        // 推进游标到本轮最大 resolved_at。
        if let Some(r) = m.resolved_at {
            max_resolved = Some(r).max(max_resolved);
        }
        let (outcome, index_set) = match (
            m.outcome_yes
                .and_then(|d| d.to_string().parse::<f64>().ok()),
            m.outcome_no.and_then(|d| d.to_string().parse::<f64>().ok()),
        ) {
            (Some(1.0), _) => ("YES", 2u64),
            (_, Some(1.0)) => ("NO", 1u64),
            // P1-16：已结算但 outcome 不明确，warn 升级（旧逻辑静默 continue 无日志，运维无法发现）。
            // 不阻塞 worker；手动端点兜底。outcome 不明确常见于争议/数据延迟。
            _ => {
                warn!(
                    condition_id = %m.venue_market_id,
                    outcome_yes = ?m.outcome_yes, outcome_no = ?m.outcome_no,
                    "市场已结算但 outcome 不明确，跳过自动赎回（手动端点兜底）"
                );
                continue;
            }
        };

        // P1-15：候选用户集 = 跟单过该市场的用户 ∪ 有在线预配凭证的所有用户。
        // 旧逻辑仅 copy_execution 用户；官网手动买入无 copy_execution → 自动 worker 不扫到。
        // 凭证候选由 worker 的链上 balanceOf 过滤（无仓位自动跳过），不会误赎回。
        let copy_users = match acct::list_users_for_market(&state.db, "polymarket", &m.venue_market_id)
            .await
        {
            Ok(u) => u,
            Err(e) => {
                warn!(condition_id = %m.venue_market_id, error = %e, "查 copy_execution 候选用户失败，仅用凭证候选");
                Vec::new()
            }
        };
        let cred_users = match acct::list_users_with_live_credentials(&state.db, "polymarket").await {
            Ok(u) => u,
            Err(e) => {
                warn!(error = %e, "查在线凭证候选用户失败，仅用 copy_execution 候选");
                Vec::new()
            }
        };
        // 并集去重。
        let mut users: Vec<uuid::Uuid> = copy_users;
        users.extend(cred_users);
        users.sort_by_key(|u| *u);
        users.dedup();
        if users.is_empty() {
            continue;
        }

        for user_id in users {
            if let Err(e) = process_user_redeem(
                state,
                &venue_impl,
                user_id,
                &m.venue_market_id,
                outcome,
                index_set,
                pusd,
                ctf,
                &rpc_url,
            )
            .await
            {
                warn!(
                    user_id = %user_id,
                    condition_id = %m.venue_market_id,
                    error = %e,
                    "自动赎回单用户失败，继续下一个"
                );
            }
        }
    }

    // 推进游标。
    if let Some(latest) = max_resolved {
        *cursor.lock().unwrap() = Some(latest);
    }
    Ok(())
}

/// 对单用户单市场发起自动赎回：加载凭证 → balanceOf 确认 → 落库 → venue.redeem → 更新状态。
#[allow(clippy::too_many_arguments)]
async fn process_user_redeem(
    state: &AppState,
    venue_impl: &std::sync::Arc<dyn Venue>,
    user_id: uuid::Uuid,
    condition_id: &str,
    outcome: &str,
    index_set: u64,
    pusd: alloy_primitives::Address,
    ctf: alloy_primitives::Address,
    rpc_url: &str,
) -> Result<(), anyhow::Error> {
    // 1. 加载凭证（需 DepositWalletDelegated + 在线预配）。
    let creds = acct::list_credentials(&state.db, user_id).await?;
    let cred_row = creds
        .into_iter()
        .find(|c| c.platform == "polymarket")
        .ok_or_else(|| anyhow::anyhow!("polymarket 凭证未预配"))?;
    let blob = &cred_row.encrypted_blob;
    let provision_live = blob
        .get("provision_live")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !provision_live {
        return Err(anyhow::anyhow!("离线预配，跳过自动赎回"));
    }
    let cred: Credential = serde_json::from_value(blob.clone())?;
    let deposit_wallet_address = match &cred {
        Credential::DepositWalletDelegated {
            deposit_wallet_address,
            ..
        } => deposit_wallet_address.clone(),
        _ => return Err(anyhow::anyhow!("非 DepositWalletDelegated 凭证，跳过")),
    };

    // 2. 防重复：已有 pending/mined 赎回则跳过。
    if acct::redemption_exists_active(
        &state.db,
        user_id,
        condition_id,
        outcome,
        &deposit_wallet_address,
    )
    .await?
    {
        return Ok(());
    }

    // 3. 链上 balanceOf 确认有可赎回量。
    let dw: alloy_primitives::Address = deposit_wallet_address
        .parse()
        .map_err(|e| anyhow::anyhow!("deposit_wallet_address 解析失败: {e}"))?;
    let position_id =
        sharpside_venues_polymarket::onchain::ctf_position_id(pusd, condition_id, index_set);
    let balance =
        sharpside_venues_polymarket::onchain::ctf_balance_of(rpc_url, ctf, dw, position_id)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
    if balance <= 0.0 {
        // 无仓位（已赎回/未持有赢方），跳过。
        return Ok(());
    }

    // 4. 落库审计（pending, auto）。冲突（已被手动/上轮赎回）则跳过。
    let amount_dec = rust_decimal::Decimal::from_f64_retain(balance)
        .ok_or_else(|| anyhow::anyhow!("赎回数量精度异常"))?;
    let token_id_str = position_id.to_string();
    let pending = match acct::insert_redemption(
        &state.db,
        user_id,
        "polymarket",
        condition_id,
        outcome,
        &token_id_str,
        amount_dec,
        "auto",
        &deposit_wallet_address,
    )
    .await
    {
        Ok(p) => p,
        Err(sharpside_db::DbError::Conflict(_)) => return Ok(()),
        Err(e) => return Err(e.into()),
    };

    // 5. 发起赎回：owner 签 WALLET batch → relayer 提交 → 轮询确认。
    info!(
        user_id = %user_id,
        condition_id = condition_id,
        outcome = outcome,
        amount = balance,
        "自动赎回发起"
    );
    let result = venue_impl.redeem(&cred, condition_id, balance).await;
    match result {
        Ok(r) => {
            let status = if r.tx_hash.is_some() {
                "mined"
            } else {
                "pending"
            };
            acct::update_redemption_status(
                &state.db,
                pending.id,
                status,
                r.tx_hash.as_deref(),
                None,
            )
            .await?;
        }
        Err(e) => {
            let note = format!("{e}");
            warn!(
                user_id = %user_id,
                condition_id = condition_id,
                error = %e,
                "自动赎回链上失败，标记 failed"
            );
            // P0-8：瞬态失败可重试。首次失败设 next_attempt_at（30s 后），attempts+1。
            // 永久错误（KMS/签名/neg-risk）也会落 failed，但重试前 balanceOf 校验会兜住已赎回/无仓位。
            acct::mark_redemption_retry_failed(&state.db, pending.id, &note).await?;
        }
    }
    Ok(())
}

/// P0-8：重试可恢复的 failed 赎回。
///
/// 扫 `status='failed' AND attempts<3 AND next_attempt_at<=now` 的行，对每条：
///   1. 链上 balanceOf：0 → 上轮可能已成功但回报丢失，直接标 mined（幂等收尾）。
///   2. 否则改回 pending → venue.redeem → 成功 mined / 失败 attempts+1（指数退避）。
///
/// 永久错误（KMS 解密失败、neg-risk revert、签名失败）会在 venue.redeem 再次失败，
/// attempts 累加至上限后停止自动重试，保留 failed 交人工。
async fn retry_failed_redemptions(state: &AppState) -> Result<(), anyhow::Error> {
    const RETRY_BATCH: i64 = 20;
    let pending_retry = acct::list_retryable_failed_redemptions(&state.db, RETRY_BATCH).await?;
    if pending_retry.is_empty() {
        return Ok(());
    }
    info!(n = pending_retry.len(), "扫到可重试 failed 赎回");

    let venue_impl = match state.registry.get(sharpside_shared::Platform::Polymarket) {
        Some(v) => v.clone(),
        None => {
            warn!("polymarket venue 未注册，failed 赎回重试跳过");
            return Ok(());
        }
    };

    let rpc_url = std::env::var("POLYGON_RPC_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| crate::routes::POLYGON_RPC_DEFAULT_FALLBACK.to_string());
    let pusd = crate::routes::parse_address_const(crate::routes::PUSD_CONST)
        .map_err(|e| anyhow::anyhow!(e))?;
    let ctf = crate::routes::parse_address_const(crate::routes::CTF_CONST)
        .map_err(|e| anyhow::anyhow!(e))?;

    for r in pending_retry {
        if let Err(e) = retry_one_redemption(state, &venue_impl, &r, pusd, ctf, &rpc_url).await {
            warn!(
                redemption_id = %r.id, user_id = %r.user_id, error = %e,
                "重试单笔赎回失败，保留 failed 待下轮"
            );
        }
    }
    Ok(())
}

/// 重试单笔 failed 赎回。
async fn retry_one_redemption(
    state: &AppState,
    venue_impl: &std::sync::Arc<dyn Venue>,
    r: &sharpside_db::models::Redemption,
    pusd: alloy_primitives::Address,
    ctf: alloy_primitives::Address,
    rpc_url: &str,
) -> Result<(), anyhow::Error> {
    // 重新加载凭证（凭证可能已被更新/轮换）。
    let creds = acct::list_credentials(&state.db, r.user_id).await?;
    let cred_row = creds
        .into_iter()
        .find(|c| c.platform == r.venue.as_str())
        .ok_or_else(|| anyhow::anyhow!("{} 凭证未预配", r.venue))?;
    let blob = &cred_row.encrypted_blob;
    let provision_live = blob
        .get("provision_live")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !provision_live {
        // 凭证已离线化，不再可自动赎回；保留 failed 不重试。
        return Ok(());
    }
    let cred: Credential = serde_json::from_value(blob.clone())?;
    let deposit_wallet_address = match &cred {
        Credential::DepositWalletDelegated {
            deposit_wallet_address,
            ..
        } => deposit_wallet_address.clone(),
        _ => return Err(anyhow::anyhow!("非 DepositWalletDelegated 凭证，跳过重试")),
    };

    // 链上 balanceOf 兜底：0 说明上轮已赎回成功（回报丢失）或仓位已转出，直接标 mined。
    let dw: alloy_primitives::Address = deposit_wallet_address
        .parse()
        .map_err(|e| anyhow::anyhow!("deposit_wallet_address 解析失败: {e}"))?;
    let index_set = if r.outcome == "YES" { 2u64 } else { 1u64 };
    let position_id = sharpside_venues_polymarket::onchain::ctf_position_id(
        pusd,
        &r.condition_id,
        index_set,
    );
    let balance =
        sharpside_venues_polymarket::onchain::ctf_balance_of(rpc_url, ctf, dw, position_id)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
    if balance <= 0.0 {
        info!(
            redemption_id = %r.id, user_id = %r.user_id, condition_id = %r.condition_id,
            "重试时链上 balanceOf=0，判定上轮已赎回，标 mined 收尾"
        );
        acct::update_redemption_status(&state.db, r.id, "mined", None, Some("重试时 balanceOf=0，判定已赎回"))
            .await?;
        return Ok(());
    }

    // 改回 pending（唯一约束防并发），重新发起 redeem。
    let revived = acct::revive_redemption_to_pending(&state.db, r.id).await?;
    info!(
        redemption_id = %r.id, user_id = %r.user_id, condition_id = %r.condition_id,
        attempts = r.attempts, balance, "重试发起赎回"
    );
    let result = venue_impl.redeem(&cred, &r.condition_id, balance).await;
    match result {
        Ok(res) => {
            let status = if res.tx_hash.is_some() { "mined" } else { "pending" };
            acct::update_redemption_status(&state.db, revived.id, status, res.tx_hash.as_deref(), None)
                .await?;
        }
        Err(e) => {
            let note = format!("重试失败(attempt {}): {e}", r.attempts + 1);
            warn!(
                redemption_id = %r.id, user_id = %r.user_id, error = %e,
                "重试赎回链上失败，attempts+1"
            );
            acct::mark_redemption_retry_failed(&state.db, revived.id, &note).await?;
        }
    }
    Ok(())
}
