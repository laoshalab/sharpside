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
//! - **失败重试**：failed 的赎回不重试（链上 balanceOf 已 0 或签名失败）；由手动端点兜底。
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
        return Ok(());
    }
    info!(n = markets.len(), since = ?since, "赎回 worker 扫到新结算市场");

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
            // 已结算但 outcome 不明确，跳过（worker 不阻塞，手动端点兜底）。
            _ => continue,
        };

        // 候选用户集：跟单过该市场的用户（链上 balanceOf 兜底确认）。
        let users = match acct::list_users_for_market(&state.db, "polymarket", &m.venue_market_id)
            .await
        {
            Ok(u) => u,
            Err(e) => {
                warn!(condition_id = %m.venue_market_id, error = %e, "查市场候选用户失败，跳过");
                continue;
            }
        };
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
    if acct::redemption_exists_active(&state.db, user_id, condition_id, outcome).await? {
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
            acct::update_redemption_status(&state.db, pending.id, "failed", None, Some(&note))
                .await?;
        }
    }
    Ok(())
}
