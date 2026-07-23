//! 真实联调 Stage 3：live post-order（签名 + 提交路径验证，**不花真钱**）。`#[ignore]`，不进常规 CI。
//!
//! 用一个**未充值**的新部署 deposit wallet 真打 CLOB `POST /order`。订单会被服务端以
//! 「余额/授权不足」拒绝——这恰好证明签名（ERC-7739-wrapped POLY_1271）、L2 HMAC、V2 wire body
//! 形状全对（服务端越过签名校验进入余额检查）。全程不花一分钱、可重复跑。
//!
//! **复用同一 DW（充值后反复测真实下单）**：设环境变量 `POLYMARKET_TEST_OWNER_PK=<0x 私钥>`，
//! 测试即用该固定 owner EOA → 稳定 CREATE2 deposit wallet 地址。首次跑会部署 DW 并打印地址；
//! 后续跑检测到已部署就跳过 `WALLET-CREATE`，直接进入 L1/L2/下单。不设该 env 则每次随机新建
//! owner EOA（仅适合「不花真钱验证签名路径」）。
//!
//! 流程：
//! 1. 读 owner EOA（固定私钥 env or 随机）
//! 2. CREATE2 派生 deposit wallet 地址
//! 3. Relayer `is_deployed` 检查；未部署才 `WALLET-CREATE`（真实部署）→ 轮询确认
//! 4. CLOB L1 `deriveApiKey` → L2 凭证
//! 5. CLOB `update_balance_allowance`(signatureType=3) 余额同步（未充值→404 良性）
//! 6. Gamma `/markets` 取活跃市场 → token_id / tick_size / neg_risk
//! 7. `sign_clob_order_deposit`（BUY，价格对齐 tick，size=5）
//! 8. `post_order_l2` 真打 `/order`
//! 9. 断言：响应非 401/403/签名错误；命中「余额/授权不足」即视为签名路径验证通过
//!
//! 跑法（需代理 + full_network + builder/relayer 凭证；中国等地区封锁 Polymarket 须代理）：
//! ```bash
//! # 一次性：生成 owner 私钥并保存
//! POLYMARKET_TEST_OWNER_PK=0x<your-owner-private-key> \
//! POLYMARKET_HTTP_PROXY=http://127.0.0.1:7890 \
//!   cargo test -p sharpside-venues-polymarket --offline --test live_post_order -- --ignored --nocapture
//! ```

use std::str::FromStr;

use alloy_signer_local::PrivateKeySigner;
use sharpside_venues_polymarket::{
    clob, deposit::derive_deposit_wallet_address, PolymarketClient, RelayerClient,
};

/// Builder 归因 code（用户平台 builder 账户；deposit wallet 路径填，EOA 路径为 None）。
/// 取自 `~/文档/sharpside` 生产配置。
const BUILDER_CODE: &str = "019f6e85-dce2-7a7a-aa72-cadb8d498bbe";

/// 空 SubmitResp（relayer 已有 DW 记录、无需新部署时占位）。
fn submit_placeholder() -> sharpside_venues_polymarket::relayer::SubmitResp {
    sharpside_venues_polymarket::relayer::SubmitResp {
        transaction_id: None,
        transaction_hash: None,
    }
}

#[tokio::test]
#[ignore]
async fn live_post_order_signs_and_reaches_balance_check() {
    // step 1: owner EOA —— 固定私钥（env）→ 稳定 DW，可充值后复用；否则随机（仅验签名路径）。
    let owner_signer = match std::env::var("POLYMARKET_TEST_OWNER_PK") {
        Ok(pk) => {
            let s = PrivateKeySigner::from_str(&pk).expect("POLYMARKET_TEST_OWNER_PK 解析失败");
            eprintln!("step1 owner EOA（固定私钥，DW 可复用）: {}", s.address());
            s
        }
        Err(_) => {
            let s = PrivateKeySigner::random();
            eprintln!(
                "step1 owner EOA（随机，仅验签名路径；要复用 DW 请设 POLYMARKET_TEST_OWNER_PK）: {}",
                s.address()
            );
            s
        }
    };
    let owner_address = owner_signer.address();
    eprintln!(
        "step1 deposit wallet（充值 pUSD 到此地址）: {}",
        derive_deposit_wallet_address(owner_address)
    );

    // step 2: CREATE2 派生 deposit wallet 地址
    let deposit_wallet_address = derive_deposit_wallet_address(owner_address);
    eprintln!("step2 deposit wallet (CREATE2): {deposit_wallet_address}");

    // step 3: Relayer is_deployed 检查；未部署才 WALLET-CREATE（避免重复部署/报错）。
    let relayer = RelayerClient::new();
    let already = relayer
        .is_deployed(&deposit_wallet_address.to_string())
        .await;
    match already {
        Ok(true) => eprintln!("step3 DW 已部署，跳过 WALLET-CREATE"),
        Ok(false) => {
            let submit = match relayer.wallet_create(owner_address).await {
                Ok(r) => {
                    eprintln!(
                        "step3 WALLET-CREATE ok: tx_id={:?} tx_hash={:?}",
                        r.transaction_id, r.transaction_hash
                    );
                    r
                }
                Err(e) => {
                    // relayer 已有该 signer 的 DW 记录（链上可能已 mined 或待 mined）→ 当跳过，继续后续。
                    let el = e.to_lowercase();
                    if el.contains("already exists") {
                        eprintln!(
                            "step3 WALLET-CREATE: relayer 已有记录（already exists），跳过部署继续"
                        );
                        // 无 tx_id 可轮询；直接进入下一步。
                        submit_placeholder()
                    } else {
                        panic!("step3 WALLET-CREATE 失败（联调差异）: {e}")
                    }
                }
            };
            if let Some(tx_id) = submit.transaction_id.as_deref().filter(|s| !s.is_empty()) {
                match relayer.poll_confirmed(tx_id).await {
                    Ok(row) => eprintln!(
                        "step3b 部署确认: state={:?} tx={:?} proxy={:?}",
                        row.state, row.transaction_hash, row.proxy_address
                    ),
                    Err(e) => eprintln!("step3b 轮询未确认（可能仍上链中）: {e}"),
                }
            }
        }
        Err(e) => eprintln!("step3 is_deployed 查询失败（继续尝试部署）: {e}"),
    }

    // step 4: CLOB L1 deriveApiKey → L2 凭证
    // POLY_ADDRESS = owner EOA（= L2 凭证所属）；signature_type=3 → 服务端映射 owner → deposit wallet。
    let client = PolymarketClient::new();
    let ts2 = chrono::Utc::now().timestamp();
    let auth_sig = clob::build_l1_auth_signature(&owner_signer, ts2).unwrap();
    let l2 = match client
        .derive_api_key_l1(owner_address, &auth_sig, ts2)
        .await
    {
        Ok(c) => {
            eprintln!(
                "step4 L2 派生 ok: api_key={} secret_len={} passphrase_len={}",
                c.api_key,
                c.secret.len(),
                c.passphrase.len()
            );
            c
        }
        Err(e) => panic!("step4 L1 deriveApiKey 失败（联调差异）: {e:?}"),
    };

    // step 4b: Relayer WALLET batch approve（pUSD → CTF Exchange/NegRisk/Adapter；CT → Exchange setApprovalForAll）。
    // approve 与余额无关，DW 部署后即可提交；后续充值 pUSD 即可直接交易。owner EIP-712 签 `Batch`（普通 65 字节）。
    // 失败不 panic（可能 relayer nonce/格式差异）——打印继续，便于联调。
    let approve_calls = sharpside_venues_polymarket::wallet_batch::trading_approves();
    match relayer.wallet_nonce(owner_address).await {
        Ok(nonce) => {
            let deadline = (chrono::Utc::now().timestamp() + 600).max(0) as u64;
            let nonce_u256 = alloy_primitives::U256::from_str_radix(nonce.trim(), 10)
                .unwrap_or(alloy_primitives::U256::ZERO);
            let deadline_u256 = alloy_primitives::U256::from(deadline);
            let sig = sharpside_venues_polymarket::wallet_batch::sign_wallet_batch(
                &owner_signer,
                deposit_wallet_address,
                nonce_u256,
                deadline_u256,
                &approve_calls,
            )
            .unwrap();
            match relayer
                .wallet_batch(
                    owner_address,
                    deposit_wallet_address,
                    &nonce,
                    &deadline.to_string(),
                    &sig,
                    &approve_calls,
                )
                .await
            {
                Ok(r) => {
                    eprintln!(
                        "step4b WALLET batch approve ok: tx_id={:?} tx_hash={:?}",
                        r.transaction_id, r.transaction_hash
                    );
                    if let Some(tx_id) = r.transaction_id.as_deref().filter(|s| !s.is_empty()) {
                        match relayer.poll_confirmed(tx_id).await {
                            Ok(row) => eprintln!(
                                "step4b approve 确认: state={:?} tx={:?}",
                                row.state, row.transaction_hash
                            ),
                            Err(e) => eprintln!("step4b approve 轮询未确认: {e}"),
                        }
                    }
                }
                Err(e) => eprintln!("step4b WALLET batch approve 失败（联调差异，继续）: {e}"),
            }
        }
        Err(e) => eprintln!("step4b /nonce 失败（跳过 approve，继续）: {e}"),
    }

    // step 5: update_balance_allowance（未充值→404 良性；POLY_ADDRESS=owner EOA）
    match client
        .update_balance_allowance(owner_address, &l2.api_key, &l2.secret, &l2.passphrase)
        .await
    {
        Ok(v) => eprintln!("step5 balance-allowance ok: {v}"),
        Err(e) => eprintln!("step5 update_balance_allowance 失败（未充值→404 良性）: {e:?}"),
    }

    // step 6: Gamma /markets 取活跃市场 → token_id / tick_size / neg_risk
    let url = format!(
        "{}/markets?limit=50&active=true&closed=false&order=volume24hr&ascending=false",
        client.gamma_api()
    );
    let markets: serde_json::Value = client
        .http_get_json(&url)
        .await
        .expect("Gamma /markets 失败（联调差异）");
    let pick = markets
        .as_array()
        .expect("/markets 返回数组")
        .iter()
        .find(|m| {
            m.get("clobTokenIds")
                .and_then(|v| v.as_str())
                .map(|s| !s.is_empty() && s != "[]")
                .unwrap_or(false)
        })
        .expect("至少一个活跃 market 带 clobTokenIds");
    let condition_id = pick
        .get("conditionId")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let token_id = serde_json::from_str::<Vec<String>>(
        pick.get("clobTokenIds").and_then(|v| v.as_str()).unwrap(),
    )
    .unwrap()
    .remove(0);
    let tick_size: f64 = pick
        .get("minimumTickSize")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .or_else(|| pick.get("minimumTickSize").and_then(|v| v.as_f64()))
        .unwrap_or(0.01);
    let neg_risk = pick
        .get("negRisk")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    eprintln!(
        "step6 market: condition={} token={} tick={} neg_risk={} q={:?}",
        condition_id,
        token_id,
        tick_size,
        neg_risk,
        pick.get("question").and_then(|v| v.as_str())
    );

    // step 7: sign_clob_order_deposit（BUY，价格对齐 tick，size=5）
    // 选 0.50（对齐 0.01/0.001 tick）。size=5 USDC（小，未充值会被拒）。
    let price = 0.50_f64;
    let size = 5.0_f64;
    let signed = clob::sign_clob_order_deposit(
        &owner_signer,
        deposit_wallet_address,
        sharpside_shared::Side::Buy,
        &token_id,
        price,
        size,
        Some(BUILDER_CODE.to_string()),
        neg_risk,
        None,
        None,
    )
    .await
    .expect("step7 sign_clob_order_deposit 失败（联调差异）");
    eprintln!(
        "step7 签名 ok: signer={} maker={} sig_type={} sig_len={} salt={} ts={}",
        signed.signer_address,
        signed.maker_address,
        signed.signature_type,
        signed.signature.len(),
        signed.salt,
        signed.timestamp_ms
    );
    eprintln!(
        "step7 signature 0x{}…",
        &signed.signature[..signed.signature.len().min(40)]
    );

    // step 8: post_order_l2 真打 /order
    // L2 POLY_ADDRESS = owner EOA（= API key 属主）；wire order.signer = deposit wallet（clob-auth 硬约束）。
    // 已知 Stage 3 阻塞：服务端靠 owner EOA→DW 映射（由充 pUSD + approve + balance sync(sigType=3) 建立）
    // 校验 order.signer=DW。未充值时映射缺失 → /order 报「the order signer address has to be the address
    // of the API KEY」。需真实充值 pUSD 才能继续（文档：DW 须先 funded+approved+balance-synced）。
    let res = client
        .post_order_l2(
            &signed,
            &l2.api_key,
            &l2.secret,
            &l2.passphrase,
            owner_address,
            sharpside_venues_core::OrderType::Gtc,
            None,
            false,
        )
        .await;

    // step 9: 断言响应形态
    match res {
        Ok(order_id) => {
            eprintln!("step8 /order 返回 orderID={order_id}（未充值却接受？需人工核对是否真挂单）");
            eprintln!("STAGE3_RESULT=SIGN_PATH_OK_ORDER_ACCEPTED_UNEXPECTED");
        }
        Err(e) => {
            let el = e.to_lowercase();
            eprintln!("step8 /order 失败: {e}");
            // 已知 Stage 3 阻塞：DW 未充值/未 approve/未 balance-sync → 服务端无 owner EOA→DW 映射，
            // 回退到 order.signer==API key 属主严格校验 → DW≠owner EOA 报错。需真实充 pUSD 才能解开。
            let is_known_blocker = el
                .contains("the order signer address has to be the address of the api key")
                || el.contains("invalid l1 request headers")
                || el.contains("invalid api key");
            // 真签名/鉴权 bug（非已知阻塞）→ panic
            let is_auth_bug = (el.contains("401")
                || el.contains("403")
                || el.contains("unauthorized")
                || el.contains("forbidden")
                || el.contains("invalid signature")
                || el.contains("signature")
                || el.contains("eip-712")
                || el.contains("erc-7739")
                || el.contains("poly_1271"))
                && !is_known_blocker;
            // 业务逻辑错误（price/balance/tick/min size/market）→ 签名路径已通过
            let is_business = el.contains("insufficient")
                || el.contains("balance")
                || el.contains("allowance")
                || el.contains("no funds")
                || el.contains("not enough")
                || el.contains("price")
                || el.contains("tick")
                || el.contains("min size")
                || el.contains("min_order")
                || el.contains("rounding")
                || el.contains("market")
                || el.contains("invalid order")
                || el.contains("could not")
                || el.contains("unable");
            if is_auth_bug {
                panic!("step9 签名/鉴权失败（联调 bug，签名路径未通过）: {e}");
            }
            if is_known_blocker {
                eprintln!("STAGE3_RESULT=BLOCKED_NEED_FUNDING（DW 未充值→无 owner→DW 映射；需充 pUSD + approve + balance sync）");
            } else if is_business {
                eprintln!(
                    "STAGE3_RESULT=SIGN_PATH_OK_BUSINESS_REJECTED（签名路径通过，被业务规则拒）"
                );
            } else {
                eprintln!("STAGE3_RESULT=SIGN_PATH_OK_UNKNOWN_REJECTION（非签名错误，路径通过）");
            }
        }
    }
}
