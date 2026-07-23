//! Polymarket Venue adapter。对应 `docs/VENUE_DESIGN.md` §5 与 `docs/DATA_SOURCES.md`。
//!
//! 实现 `Venue` trait 的 signal_source 能力（leaderboard / positions / trades / markets / book）
//! 与 execution_venue 能力（`place_order`：Deposit Wallet POLY_1271 委托签名 + L2 HMAC + Builder 归因，
//! 详见 `docs/CHANNEL_A_SIGNING.md`，Stage 3 已实盘验证）；`cancel_order` / `order_state`（对账用）
//! 已落地（CLOB L2 HMAC）。`order_status`（简单枚举）未 override，业务对账走 `order_state`。
//! 订单类型当前固定 GTC（wire `orderType`/`postOnly` 写死），FOK/FAK/GTD/post-only/split/merge 待补。
//!
//! 设计要点：
//! - **DTO 与 domain 分离**：`dto.rs` 是 API 原始响应，`lib.rs` 负责映射到 `venues::core` 通用类型
//! - **参数映射**：通用 `LeaderboardQuery.time_period`（`1d`/`1w`/`1m`/`1y`/`ytd`/`all`）映射到 Polymarket 的 `DAY`/`WEEK`/`MONTH`/`ALL`
//! - **限流在 adapter 内**（Phase A 落地 `docs/VENUE_DESIGN.md §9`）：`PolymarketClient` 按端点
//!   配额持 governor 令牌桶，超限 `await` 节流 + 上游 429 退避重试，映射为 `VenueError::RateLimited`

#![forbid(unsafe_code)]

pub mod client;
pub mod clob;
pub mod deposit;
pub mod dto;
pub mod onchain;
pub mod relayer;
pub mod wallet_batch;

pub use client::{
    L2Credentials, PolymarketClient, CLOB_API_DEFAULT, DATA_API_DEFAULT, GAMMA_API_DEFAULT,
};
pub use dto::{BookDto, BookLevelDto, LeaderboardEntry, MarketDto, PositionDto, TradeDto};
pub use relayer::RelayerClient;
/// 重新导出 [`OrderType`]，供仅依赖本 crate（不直接依赖 `sharpside-venues-core`）的下游
/// （如 `sharpside-daemon`）调用 `post_order` / `post_order_l2` 时构造订单类型。
pub use sharpside_venues_core::OrderType;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sharpside_shared::{Platform, Side};
use sharpside_venues_core::{
    AuthModel, Credential, Fill, Geo, LeaderboardQuery, Market, MarketQuery, Order, OrderBook,
    OrderBookLevel, Pagination, Position, Trade, Trader, Unit, Venue, VenueCapabilities,
    VenueError, VenueInfo,
};

use alloy_primitives::{Address, U256};

/// Polymarket Venue 适配器。signal + execution（EIP-712 钱包签名下单）。
pub struct PolymarketVenue {
    info: VenueInfo,
    client: PolymarketClient,
    /// 可注入的 dev/测试签名器（优先于 env / KMS 路径，便于离线测试 place_order）。
    dev_signer: Option<alloy_signer_local::PrivateKeySigner>,
    /// KMS（生产路径解密 owner EOA 私钥 / L2 secret）。dev 路径无 KMS 时用 env 明文。
    kms: Option<std::sync::Arc<dyn sharpside_kms::Kms>>,
    /// Relayer 客户端（提现走 WALLET batch transfer）。缺省时 withdraw 返回错误。
    /// place_order 不需要 relayer（CLOB 直连），故不强制注入。
    relayer: Option<RelayerClient>,
}

impl PolymarketVenue {
    /// 用默认 API base 构造。
    pub fn new() -> Self {
        Self::with_client(PolymarketClient::new())
    }

    /// 用自定义客户端构造（测试 / 代理用）。
    pub fn with_client(client: PolymarketClient) -> Self {
        Self {
            info: VenueInfo {
                platform: Platform::Polymarket,
                display_name: "Polymarket".into(),
                capabilities: VenueCapabilities::SIGNAL_SOURCE | VenueCapabilities::EXECUTION_VENUE,
                auth_model: AuthModel::Wallet,
                unit: Unit::UsdcCtf,
                geo: Geo::GlobalWithUsRestrictions,
            },
            client,
            dev_signer: None,
            kms: None,
            relayer: None,
        }
    }

    /// 注入 dev/测试签名器（优先于 env `POLYMARKET_DEV_PRIVATE_KEY` 与 KMS 路径）。
    pub fn with_dev_signer(mut self, signer: alloy_signer_local::PrivateKeySigner) -> Self {
        self.dev_signer = Some(signer);
        self
    }

    /// 注入 KMS（生产路径解密 owner EOA 私钥 / L2 secret）。
    /// copier 服务启动时注入 `DevKms` 或 `AwsKms`，place_order 自动走 KMS 解密。
    pub fn with_kms(mut self, kms: std::sync::Arc<dyn sharpside_kms::Kms>) -> Self {
        self.kms = Some(kms);
        self
    }

    /// 注入 Relayer 客户端（提现走 WALLET batch transfer 用）。
    /// copier 服务启动时注入 `RelayerClient::new()`（读 env `POLYMARKET_RELAYER_*`）。
    pub fn with_relayer(mut self, relayer: RelayerClient) -> Self {
        self.relayer = Some(relayer);
        self
    }
}

impl Default for PolymarketVenue {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Venue for PolymarketVenue {
    fn info(&self) -> &VenueInfo {
        &self.info
    }

    async fn leaderboard(&self, q: LeaderboardQuery) -> Result<Vec<Trader>, VenueError> {
        let category = q.category.as_deref().unwrap_or("OVERALL");
        let time_period = map_time_period(&q.time_period);
        let order_by = map_order_by(&q.order_by);
        let entries = self
            .client
            .leaderboard(category, time_period, order_by, q.limit, q.offset)
            .await?;
        Ok(entries.into_iter().map(map_leaderboard_entry).collect())
    }

    async fn positions(&self, trader_id: &str) -> Result<Vec<Position>, VenueError> {
        let dtos = self.client.positions(trader_id).await?;
        Ok(dtos
            .into_iter()
            .filter_map(|d| map_position(d, trader_id))
            .collect())
    }

    async fn trades(&self, trader_id: &str, q: Pagination) -> Result<Vec<Trade>, VenueError> {
        let dtos = self.client.trades(trader_id, q.limit, q.offset).await?;
        Ok(dtos
            .into_iter()
            .filter_map(|d| map_trade(d, trader_id))
            .collect())
    }

    async fn markets(&self, q: MarketQuery) -> Result<Vec<Market>, VenueError> {
        // MVP：Gamma /markets 不支持全文搜索，按 tag 过滤由调用方做；这里拉一批
        let _ = &q;
        let dtos = self.client.markets(100, 0).await?;
        Ok(dtos.into_iter().filter_map(map_market).collect())
    }

    async fn portfolio_value(&self, trader_id: &str) -> Result<f64, VenueError> {
        // `/value` 返回数组，取首元素的 value；空数组（无持仓）按 0。
        let dtos = self.client.value(trader_id).await?;
        Ok(dtos
            .into_iter()
            .next()
            .and_then(|d| d.value)
            .unwrap_or(0.0))
    }

    async fn book(&self, market_id: &str, token_id: &str) -> Result<OrderBook, VenueError> {
        let dto = self.client.book(market_id, token_id).await?;
        Ok(OrderBook {
            market_id: market_id.into(),
            token_id: token_id.into(),
            bids: dto.bids.into_iter().filter_map(map_book_level).collect(),
            asks: dto.asks.into_iter().filter_map(map_book_level).collect(),
        })
    }

    /// Polymarket 最小下单股数：CLOB `/markets/{condition_id}` 的 `minimum_order_size`。
    /// 每市场不同（5/10/50/100…），服务端强制。拉取失败回退 0（不校验，由服务端兜底拒单）。
    async fn market_min_size(&self, market_id: &str) -> Result<f64, VenueError> {
        match self.client.clob_market(market_id).await {
            Ok(m) => Ok(m.minimum_order_size.unwrap_or(0.0)),
            Err(e) => {
                tracing::warn!(
                    market_id = market_id,
                    error = %e,
                    "/markets 拉取 minimum_order_size 失败，回退 0（不校验）"
                );
                Ok(0.0)
            }
        }
    }

    /// Polymarket 市场可交易性：CLOB `/markets/{condition_id}` 的 `active && accepting_orders`。
    /// 已结算/下架的市场 active=false 或 accepting_orders=false → 下单前早拒，避免撞服务端 400。
    /// 拉取失败 fail-open（Ok(true)）——与 market_min_size 一致，由 place_order 兜底拒单，
    /// 避免瞬态 /markets 故障阻断全部跟单。
    async fn market_tradable(&self, market_id: &str) -> Result<bool, VenueError> {
        match self.client.clob_market(market_id).await {
            Ok(m) => Ok(m.active && m.accepting_orders),
            Err(e) => {
                tracing::warn!(
                    market_id = market_id,
                    error = %e,
                    "/markets 拉取可交易性失败，fail-open（放行，由 place_order 兜底）"
                );
                Ok(true)
            }
        }
    }

    async fn place_order(&self, cred: &Credential, order: Order) -> Result<Fill, VenueError> {
        match cred {
            Credential::DepositWalletDelegated {
                deposit_wallet_address,
                owner_address,
                encrypted_owner_key,
                l2_api_key,
                encrypted_l2_secret,
                l2_passphrase,
                builder_code,
            } => {
                // 主路径 · FrenFlow 式 Deposit Wallet (POLY_1271) 委托签名。见 docs/CHANNEL_A_SIGNING.md §3.2。
                let signer = self
                    .resolve_owner_signer(encrypted_owner_key)
                    .map_err(VenueError::Auth)?;
                let deposit: Address = deposit_wallet_address.parse().map_err(|e| {
                    VenueError::Auth(format!("deposit_wallet_address 解析失败: {e}"))
                })?;
                if signer.address().to_string().to_lowercase() != owner_address.to_lowercase() {
                    return Err(VenueError::Auth(
                        "DepositWalletDelegated: owner_address 与解出的 owner EOA 不一致".into(),
                    ));
                }
                let live = std::env::var("POLYMARKET_CLOB_POST").ok().as_deref() == Some("1");
                // neg_risk 按 market metadata（CLOB /book 的 negRisk）选 V2 verifyingContract。
                // 真实提交（live）才发 /book 解析；dry-sign 离线默认 false（standard）。
                let neg_risk = if live {
                    self.client.resolve_neg_risk(&order.token_id).await
                } else {
                    false
                };
                let signed = crate::clob::sign_clob_order_deposit(
                    &signer,
                    deposit,
                    order.side,
                    &order.token_id,
                    order.price,
                    order.size,
                    Some(builder_code.clone()),
                    neg_risk,
                    order.idempotency_salt,
                    order.order_timestamp_ms,
                )
                .await
                .map_err(VenueError::Auth)?;

                if live {
                    // 真实提交：签名由 sign_clob_order_deposit 走 ERC-7739-wrapped POLY_1271
                    // （对齐官方 @polymarket/clob-client-v2，clob-auth crate golden vector 验证）。
                    let l2_secret = self
                        .resolve_l2_secret(encrypted_l2_secret)
                        .map_err(VenueError::Auth)?;
                    let order_id = self
                        .client
                        .post_order_l2(
                            &signed,
                            l2_api_key,
                            &l2_secret,
                            l2_passphrase,
                            signer.address(),
                            order.order_type,
                            order.expiration,
                        )
                        .await
                        .map_err(VenueError::Auth)?;
                    Ok(Fill {
                        order_id,
                        filled_size: order.size,
                        filled_price: order.price,
                        tx_hash: Some(signed.signature.clone()),
                        fee: 0.0,
                    })
                } else {
                    tracing::info!(
                        deposit_wallet = %deposit,
                        owner = %signer.address(),
                        sig_type = signed.signature_type,
                        neg_risk,
                        "dry-sign Deposit Wallet 委托订单（未提交 CLOB，设 POLYMARKET_CLOB_POST=1 提交）"
                    );
                    Ok(Fill {
                        order_id: format!(
                            "dry-sign-deposit-{}",
                            &signed.signature[..8.min(signed.signature.len())]
                        ),
                        filled_size: order.size,
                        filled_price: order.price,
                        tx_hash: Some(signed.signature.clone()),
                        fee: 0.0,
                    })
                }
            }
            Credential::Wallet {
                encrypted_handle: _,
            } => {
                // 旧 EOA 路径（兼容）。见 docs/CHANNEL_A_SIGNING.md §1.1。
                let signer = self.resolve_signer(cred).map_err(VenueError::Auth)?;
                let live = std::env::var("POLYMARKET_CLOB_POST").ok().as_deref() == Some("1");
                let neg_risk = if live {
                    self.client.resolve_neg_risk(&order.token_id).await
                } else {
                    false
                };
                let signed = crate::clob::sign_clob_order(
                    &signer,
                    order.side,
                    &order.token_id,
                    order.price,
                    order.size,
                    neg_risk,
                    order.idempotency_salt,
                    order.order_timestamp_ms,
                )
                .await
                .map_err(VenueError::Auth)?;

                if live {
                    let order_id = self
                        .client
                        .post_order(&signed, order.order_type, order.expiration)
                        .await?;
                    Ok(Fill {
                        order_id,
                        filled_size: order.size,
                        filled_price: order.price,
                        tx_hash: Some(signed.signature.clone()),
                        fee: 0.0,
                    })
                } else {
                    tracing::info!(
                        signer = %signed.signer_address,
                        "dry-sign 已签名（未提交 CLOB，设 POLYMARKET_CLOB_POST=1 提交）"
                    );
                    Ok(Fill {
                        order_id: format!(
                            "dry-sign-{}",
                            &signed.signature[..8.min(signed.signature.len())]
                        ),
                        filled_size: order.size,
                        filled_price: order.price,
                        tx_hash: Some(signed.signature.clone()),
                        fee: 0.0,
                    })
                }
            }
            _ => Err(VenueError::Auth(
                "Polymarket 仅支持 Wallet / DepositWalletDelegated 凭证".into(),
            )),
        }
    }

    /// 余额查询（DepositWalletDelegated 路径）：CLOB `GET /balance-allowance` 读 pUSD 余额。
    /// 用于下单前最低余额校验（防充值不足下单被拒）。positions 暂不填充（跟单风控只看 cash）。
    async fn balance(
        &self,
        cred: &Credential,
    ) -> Result<sharpside_venues_core::Balance, VenueError> {
        match cred {
            Credential::DepositWalletDelegated {
                owner_address,
                encrypted_owner_key,
                l2_api_key,
                encrypted_l2_secret,
                l2_passphrase,
                ..
            } => {
                let signer = self
                    .resolve_owner_signer(encrypted_owner_key)
                    .map_err(VenueError::Auth)?;
                if signer.address().to_string().to_lowercase() != owner_address.to_lowercase() {
                    return Err(VenueError::Auth(
                        "balance: owner_address 与解出的 owner EOA 不一致".into(),
                    ));
                }
                let l2_secret = self
                    .resolve_l2_secret(encrypted_l2_secret)
                    .map_err(VenueError::Auth)?;
                let v = self
                    .client
                    .get_balance_allowance(signer.address(), l2_api_key, &l2_secret, l2_passphrase)
                    .await
                    .map_err(VenueError::Auth)?;
                // CLOB `/balance-allowance` 返回原始原子单位字符串（USDC 6 位小数），
                // 形如 {"balance":"7000000","allowance":"..."} = 7.0 pUSD。除以 1e6 归一为美元，
                // 使下游（exec.rs min_dw_balance 美元风控、portfolio 展示）口径一致。
                let raw = v
                    .get("balance")
                    .and_then(|b| b.as_str().and_then(|s| s.parse::<f64>().ok()))
                    .or_else(|| v.get("balance").and_then(|b| b.as_f64()))
                    .unwrap_or(0.0);
                let cash = raw / 1_000_000.0;
                Ok(sharpside_venues_core::Balance {
                    cash,
                    positions: Vec::new(),
                })
            }
            _ => Err(VenueError::Auth(
                "balance 仅支持 DepositWalletDelegated 凭证".into(),
            )),
        }
    }

    /// 成交对账：调 CLOB `/data/order/{id}`（L2 HMAC）查订单真实状态与已成交股数。
    /// Polymarket order 响应：`status`（LIVE/MATCHED/CANCELLED）、`size_matched`（已成交股数）、
    /// `price`（限价）。MATCHED → Filled；LIVE 且 size_matched>0 → PartiallyFilled，否则 Open；
    /// CANCELLED/EXPIRED → Cancelled。filled_price 取响应 price（无独立均价字段，限价近似）。
    async fn order_state(
        &self,
        cred: &Credential,
        order_id: &str,
    ) -> Result<sharpside_venues_core::OrderState, VenueError> {
        let Credential::DepositWalletDelegated {
            owner_address,
            encrypted_owner_key,
            l2_api_key,
            encrypted_l2_secret,
            l2_passphrase,
            ..
        } = cred
        else {
            return Err(VenueError::Auth(
                "order_state 仅支持 DepositWalletDelegated 凭证".into(),
            ));
        };
        let signer = self
            .resolve_owner_signer(encrypted_owner_key)
            .map_err(VenueError::Auth)?;
        if signer.address().to_string().to_lowercase() != owner_address.to_lowercase() {
            return Err(VenueError::Auth(
                "order_state: owner_address 与解出的 owner EOA 不一致".into(),
            ));
        }
        let l2_secret = self
            .resolve_l2_secret(encrypted_l2_secret)
            .map_err(VenueError::Auth)?;
        let v = self
            .client
            .get_order_l2(
                order_id,
                l2_api_key,
                &l2_secret,
                l2_passphrase,
                signer.address(),
            )
            .await
            .map_err(VenueError::Auth)?;
        let status_str = v
            .get("status")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_ascii_uppercase();
        // size_matched：Polymarket 返回字符串或数字，兼容两种。
        let f64_of = |key: &str| -> f64 {
            v.get(key)
                .and_then(|x| x.as_str().and_then(|s| s.parse::<f64>().ok()))
                .or_else(|| v.get(key).and_then(|x| x.as_f64()))
                .unwrap_or(0.0)
        };
        let size_matched = f64_of("size_matched");
        let price = f64_of("price");
        let status = match status_str.as_str() {
            "MATCHED" => sharpside_venues_core::OrderStatus::Filled,
            "LIVE" if size_matched > 0.0 => {
                sharpside_venues_core::OrderStatus::PartiallyFilled
            }
            "LIVE" => sharpside_venues_core::OrderStatus::Open,
            "CANCELLED" | "EXPIRED" => sharpside_venues_core::OrderStatus::Cancelled,
            _ => sharpside_venues_core::OrderStatus::Open,
        };
        Ok(sharpside_venues_core::OrderState {
            status,
            filled_size: size_matched,
            filled_price: price,
            fee: 0.0,
        })
    }

    /// 撤单：调 CLOB `DELETE /order`（L2 HMAC）。reconcile worker 对超时未成交的 submitted 单撤单，
    /// 避免挂单长期占用资金 / 产生非预期成交。
    async fn cancel_order(
        &self,
        cred: &Credential,
        order_id: &str,
    ) -> Result<(), VenueError> {
        let Credential::DepositWalletDelegated {
            owner_address,
            encrypted_owner_key,
            l2_api_key,
            encrypted_l2_secret,
            l2_passphrase,
            ..
        } = cred
        else {
            return Err(VenueError::Auth(
                "cancel_order 仅支持 DepositWalletDelegated 凭证".into(),
            ));
        };
        let signer = self
            .resolve_owner_signer(encrypted_owner_key)
            .map_err(VenueError::Auth)?;
        if signer.address().to_string().to_lowercase() != owner_address.to_lowercase() {
            return Err(VenueError::Auth(
                "cancel_order: owner_address 与解出的 owner EOA 不一致".into(),
            ));
        }
        let l2_secret = self
            .resolve_l2_secret(encrypted_l2_secret)
            .map_err(VenueError::Auth)?;
        self.client
            .cancel_order_l2(
                order_id,
                l2_api_key,
                &l2_secret,
                l2_passphrase,
                signer.address(),
            )
            .await
            .map(|_| ())
            .map_err(VenueError::Auth)
    }

    /// 链上余额兜底（无需 CLOB 凭证）：Polygon JSON-RPC `eth_call` 读 pUSD `balanceOf(deposit_wallet)`。
    ///
    /// 用于离线预配（`provision_live=false`）时展示 Deposit Wallet 的 pUSD 持有量——
    /// `deposit_wallet_address` 已由 CREATE2 派生，无需 L2 HMAC 即可链上读 ERC-20 余额。
    /// 口径为链上原始 pUSD 余额（非 CLOB `/balance-allowance` 的 collateral），前端标注区别。
    ///
    /// RPC URL 由 env `POLYGON_RPC_URL` 覆盖（缺省 [`crate::onchain::POLYGON_RPC_DEFAULT`]）。
    /// HTTP 客户端默认直连；仅 `POLYGON_RPC_PROXY` 时走代理（不继承 `POLYMARKET_HTTP_PROXY`）。
    async fn balance_onchain(&self, deposit_wallet_address: &str) -> Result<f64, VenueError> {
        let dw: Address = deposit_wallet_address
            .parse()
            .map_err(|e| VenueError::Auth(format!("deposit_wallet_address 解析失败: {e}")))?;
        let rpc_url = std::env::var("POLYGON_RPC_URL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| crate::onchain::POLYGON_RPC_DEFAULT.to_string());
        let pusd = crate::wallet_batch::contracts::COLLATERAL
            .parse::<Address>()
            .map_err(|e| VenueError::Internal(format!("COLLATERAL 地址解析失败: {e}")))?;
        crate::onchain::pusd_balance_of(&rpc_url, pusd, dw)
            .await
            .map_err(VenueError::Internal)
    }

    /// 提现：从 deposit wallet 转出 pUSD 到外部地址。对应 `docs/CHANNEL_A_SIGNING.md` §4.1。
    ///
    /// 链路：解出 owner EOA 私钥（KMS）→ 构造 `pUSD.transfer(to, amount)` calldata →
    /// 取 relayer WALLET nonce → owner 对 `DepositWallet.Batch` 签 EIP-712 → relayer `WALLET` batch
    /// 提交 → 轮询至确认。gasless（relayer 代付 gas）。
    ///
    /// - `to`：目标地址 hex（调用方须已校验属于用户绑定钱包）。
    /// - `amount`：人类单位 pUSD（如 7.0），按 6 decimals 转 raw。
    /// - 需注入 relayer（`with_relayer`），否则返回 `Auth` 错误。
    async fn withdraw(
        &self,
        cred: &Credential,
        to: &str,
        amount: f64,
    ) -> Result<sharpside_venues_core::WithdrawResult, VenueError> {
        let Credential::DepositWalletDelegated {
            deposit_wallet_address,
            owner_address,
            encrypted_owner_key,
            ..
        } = cred
        else {
            return Err(VenueError::Auth(
                "withdraw 仅支持 DepositWalletDelegated 凭证".into(),
            ));
        };

        let signer = self
            .resolve_owner_signer(encrypted_owner_key)
            .map_err(VenueError::Auth)?;
        if signer.address().to_string().to_lowercase() != owner_address.to_lowercase() {
            return Err(VenueError::Auth(
                "withdraw: owner_address 与解出的 owner EOA 不一致".into(),
            ));
        }

        let deposit: Address = deposit_wallet_address
            .parse()
            .map_err(|e| VenueError::Auth(format!("deposit_wallet_address 解析失败: {e}")))?;
        let to_addr: Address = to
            .parse()
            .map_err(|e| VenueError::Auth(format!("提现目标地址解析失败: {e}")))?;
        if amount <= 0.0 {
            return Err(VenueError::Auth("提现金额须大于 0".into()));
        }
        // pUSD = USDC，6 decimals。
        let raw = (amount * 1_000_000.0).round() as i128;
        if raw < 0 {
            return Err(VenueError::Auth("提现金额溢出".into()));
        }
        let raw_u256 = U256::try_from(raw)
            .map_err(|_| VenueError::Auth("提现金额超出 uint256 范围".into()))?;

        let relayer = self.relayer.clone().ok_or_else(|| {
            VenueError::Auth(
                "relayer 未注入：无法发起 WALLET batch 提现（copier 须 with_relayer）".into(),
            )
        })?;

        // 构造单笔 transfer call：pUSD.transfer(to, amount)。
        let pusd = crate::wallet_batch::contracts::COLLATERAL
            .parse::<Address>()
            .map_err(|e| VenueError::Internal(format!("COLLATERAL 地址解析失败: {e}")))?;
        let calls = vec![crate::wallet_batch::WalletCall {
            target: pusd,
            value: U256::ZERO,
            data: crate::wallet_batch::transfer_calldata(to_addr, raw_u256),
        }];

        // 取当前 WALLET batch nonce（relayer 链上读取）。
        let nonce = relayer
            .wallet_nonce(signer.address())
            .await
            .map_err(VenueError::Auth)?;
        // deadline = now + 1h（秒）。
        let deadline = (chrono::Utc::now().timestamp() + 3600).max(0) as u64;
        let deadline_str = deadline.to_string();

        let nonce_u256 = U256::from_str_radix(&nonce, 10).unwrap_or(U256::ZERO);
        let signature = crate::wallet_batch::sign_wallet_batch(
            &signer,
            deposit,
            nonce_u256,
            U256::from(deadline),
            &calls,
        )
        .map_err(VenueError::Auth)?;

        let submit = relayer
            .wallet_batch(
                signer.address(),
                deposit,
                &nonce,
                &deadline_str,
                &signature,
                &calls,
            )
            .await
            .map_err(VenueError::Auth)?;

        let relayer_tx_id = submit.transaction_id.clone();
        let mut tx_hash = submit.transaction_hash.clone();

        // 轮询至确认（最多 ~90s）。失败/超时仍返回 relayer_tx_id 供对账。
        if let Some(tx_id) = relayer_tx_id.as_deref() {
            if !tx_id.is_empty() {
                match relayer.poll_confirmed(tx_id).await {
                    Ok(row) => {
                        if tx_hash.is_none() {
                            tx_hash = row.transaction_hash;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            user_to = %to_addr,
                            relayer_tx_id = %tx_id,
                            error = %e,
                            "提现 relayer 轮询未确认（可能仍在上链）"
                        );
                    }
                }
            }
        }

        Ok(sharpside_venues_core::WithdrawResult {
            to: to_addr.to_string(),
            amount,
            tx_hash,
            relayer_tx_id,
        })
    }

    /// 赎回：把已结算市场的赢仓位 CTF token 换回 pUSD，转入 deposit wallet。
    /// 对应 `docs/CHANNEL_A_SIGNING.md` §4.2。
    ///
    /// 链路：解出 owner EOA 私钥（KMS）→ 构造 `CTF.redeemPositions(pUSD, 0, conditionId, [1,2])` calldata
    /// → 取 relayer WALLET nonce → owner 对 `DepositWallet.Batch` 签 EIP-712 → relayer `WALLET` batch
    /// 提交 → 轮询至确认。gasless（relayer 代付 gas）。
    ///
    /// - `condition_id`：市场 conditionId（0x hex，bytes32）。
    /// - `amount`：赢方 token 数量（人类单位，仅审计用；链上按 deposit wallet 余额全赎，输方余额为 0 自动忽略）。
    /// - 需注入 relayer（`with_relayer`），否则返回 `Auth` 错误。
    /// - 仅支持标准市场（neg-risk 后补，走 NegRisk Adapter）。
    async fn redeem(
        &self,
        cred: &Credential,
        condition_id: &str,
        amount: f64,
    ) -> Result<sharpside_venues_core::RedeemResult, VenueError> {
        let Credential::DepositWalletDelegated {
            deposit_wallet_address,
            owner_address,
            encrypted_owner_key,
            ..
        } = cred
        else {
            return Err(VenueError::Auth(
                "redeem 仅支持 DepositWalletDelegated 凭证".into(),
            ));
        };

        let signer = self
            .resolve_owner_signer(encrypted_owner_key)
            .map_err(VenueError::Auth)?;
        if signer.address().to_string().to_lowercase() != owner_address.to_lowercase() {
            return Err(VenueError::Auth(
                "redeem: owner_address 与解出的 owner EOA 不一致".into(),
            ));
        }

        let deposit: Address = deposit_wallet_address
            .parse()
            .map_err(|e| VenueError::Auth(format!("deposit_wallet_address 解析失败: {e}")))?;
        if amount <= 0.0 {
            return Err(VenueError::Auth("赎回数量须大于 0".into()));
        }
        // conditionId 须为合法 bytes32 hex（64 hex 字符 + 可选 0x）。
        let cond_hex = condition_id.trim().trim_start_matches("0x");
        if cond_hex.len() != 64 || !cond_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(VenueError::Auth(
                "condition_id 须为 0x 前缀的 32 字节 hex（bytes32）".into(),
            ));
        }

        let relayer = self.relayer.clone().ok_or_else(|| {
            VenueError::Auth(
                "relayer 未注入：无法发起 WALLET batch 赎回（copier 须 with_relayer）".into(),
            )
        })?;

        // 构造单笔 redeemPositions call：CTF.redeemPositions(pUSD, 0, conditionId, [1,2])。
        // 标准市场二元：indexSets=[1,2]（NO=1, YES=2），合约自动烧输方、按赢方余额 1:1 付 pUSD。
        let pusd = crate::wallet_batch::contracts::COLLATERAL
            .parse::<Address>()
            .map_err(|e| VenueError::Internal(format!("COLLATERAL 地址解析失败: {e}")))?;
        let ctf = crate::wallet_batch::contracts::CONDITIONAL_TOKENS
            .parse::<Address>()
            .map_err(|e| VenueError::Internal(format!("CONDITIONAL_TOKENS 地址解析失败: {e}")))?;
        let calls = vec![crate::wallet_batch::WalletCall {
            target: ctf,
            value: U256::ZERO,
            data: crate::wallet_batch::redeem_positions_calldata(pusd, condition_id, &[1, 2]),
        }];

        // 取当前 WALLET batch nonce（relayer 链上读取）。
        let nonce = relayer
            .wallet_nonce(signer.address())
            .await
            .map_err(VenueError::Auth)?;
        // deadline = now + 1h（秒）。
        let deadline = (chrono::Utc::now().timestamp() + 3600).max(0) as u64;
        let deadline_str = deadline.to_string();

        let nonce_u256 = U256::from_str_radix(&nonce, 10).unwrap_or(U256::ZERO);
        let signature = crate::wallet_batch::sign_wallet_batch(
            &signer,
            deposit,
            nonce_u256,
            U256::from(deadline),
            &calls,
        )
        .map_err(VenueError::Auth)?;

        let submit = relayer
            .wallet_batch(
                signer.address(),
                deposit,
                &nonce,
                &deadline_str,
                &signature,
                &calls,
            )
            .await
            .map_err(VenueError::Auth)?;

        let relayer_tx_id = submit.transaction_id.clone();
        let mut tx_hash = submit.transaction_hash.clone();

        // 轮询至确认（最多 ~90s）。失败/超时仍返回 relayer_tx_id 供对账。
        if let Some(tx_id) = relayer_tx_id.as_deref() {
            if !tx_id.is_empty() {
                match relayer.poll_confirmed(tx_id).await {
                    Ok(row) => {
                        if tx_hash.is_none() {
                            tx_hash = row.transaction_hash;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            condition_id = condition_id,
                            relayer_tx_id = %tx_id,
                            error = %e,
                            "赎回 relayer 轮询未确认（可能仍在上链）"
                        );
                    }
                }
            }
        }

        Ok(sharpside_venues_core::RedeemResult {
            condition_id: condition_id.to_string(),
            amount,
            tx_hash,
            relayer_tx_id,
        })
    }
}

impl PolymarketVenue {
    /// 从凭证解析签名器（旧 Wallet 路径）。
    /// 优先级：注入 dev_signer > env `POLYMARKET_DEV_PRIVATE_KEY`
    /// > `Credential::Wallet.encrypted_handle`（`POLYMARKET_DEV_PLAINTEXT_HANDLE=1` 时视为明文）> KMS。
    ///
    /// 生产路径应由 KMS 解密 `encrypted_handle` 得签名材料——此处未接入，返回错误。
    fn resolve_signer(
        &self,
        cred: &Credential,
    ) -> Result<alloy_signer_local::PrivateKeySigner, String> {
        if let Some(s) = &self.dev_signer {
            return Ok(s.clone());
        }
        if let Ok(k) = std::env::var("POLYMARKET_DEV_PRIVATE_KEY") {
            if !k.is_empty() {
                return crate::clob::signer_from_hex(&k);
            }
        }
        match cred {
            Credential::Wallet { encrypted_handle } => {
                if std::env::var("POLYMARKET_DEV_PLAINTEXT_HANDLE")
                    .ok()
                    .as_deref()
                    == Some("1")
                {
                    crate::clob::signer_from_hex(encrypted_handle)
                } else {
                    Err("KMS 解密未接入：无法从 encrypted_handle 解出私钥（dev 可设 POLYMARKET_DEV_PRIVATE_KEY 或 POLYMARKET_DEV_PLAINTEXT_HANDLE=1）".into())
                }
            }
            _ => Err(
                "resolve_signer 仅支持 Wallet 凭证；DepositWalletDelegated 用 resolve_owner_signer"
                    .into(),
            ),
        }
    }

    /// 从 DepositWalletDelegated 凭证解出 owner EOA 私钥。
    /// 优先级：注入 dev_signer > env `POLYMARKET_DEV_PRIVATE_KEY`
    /// > 注入 KMS 解密 `encrypted_owner_key` > `encrypted_owner_key`（`POLYMARKET_DEV_PLAINTEXT_HANDLE=1` 时视为明文）。
    ///
    /// 详见 `docs/CHANNEL_A_SIGNING.md` §3.2。生产路径由 KMS 解密 `encrypted_owner_key`。
    fn resolve_owner_signer(
        &self,
        encrypted_owner_key: &str,
    ) -> Result<alloy_signer_local::PrivateKeySigner, String> {
        if let Some(s) = &self.dev_signer {
            return Ok(s.clone());
        }
        if let Ok(k) = std::env::var("POLYMARKET_DEV_PRIVATE_KEY") {
            if !k.is_empty() {
                return crate::clob::signer_from_hex(&k);
            }
        }
        if let Some(kms) = &self.kms {
            let plaintext = kms
                .decrypt(encrypted_owner_key)
                .map_err(|e| format!("KMS 解密 owner key 失败: {e}"))?;
            return crate::clob::signer_from_hex(&plaintext);
        }
        if std::env::var("POLYMARKET_DEV_PLAINTEXT_HANDLE")
            .ok()
            .as_deref()
            == Some("1")
        {
            crate::clob::signer_from_hex(encrypted_owner_key)
        } else {
            Err("KMS 未注入且 env 未设：无法从 encrypted_owner_key 解出 owner EOA 私钥（dev 可设 POLYMARKET_DEV_PRIVATE_KEY / POLYMARKET_DEV_PLAINTEXT_HANDLE=1，生产注入 KMS）".into())
        }
    }

    /// 从加密的 L2 secret 解出明文。
    /// 优先级：注入 KMS 解密 > dev 路径 `POLYMARKET_DEV_PLAINTEXT_HANDLE=1` 视为明文。
    fn resolve_l2_secret(&self, encrypted_l2_secret: &str) -> Result<String, String> {
        if let Some(kms) = &self.kms {
            return kms
                .decrypt(encrypted_l2_secret)
                .map_err(|e| format!("KMS 解密 L2 secret 失败: {e}"));
        }
        if std::env::var("POLYMARKET_DEV_PLAINTEXT_HANDLE")
            .ok()
            .as_deref()
            == Some("1")
        {
            Ok(encrypted_l2_secret.to_string())
        } else {
            Err("KMS 未注入且 env 未设：无法从 encrypted_l2_secret 解出 L2 secret（dev 可设 POLYMARKET_DEV_PLAINTEXT_HANDLE=1，生产注入 KMS）".into())
        }
    }
}

// ── 参数映射 ──

/// 通用 `1d`/`1w`/`1m`/`1y`/`ytd`/`all` → Polymarket `DAY`/`WEEK`/`MONTH`/`ALL`。
///
/// Polymarket 排行榜仅暴露 `DAY`/`WEEK`/`MONTH`/`ALL` 四档；`1y`/`ytd` 无对应端点，
/// 退化为 `ALL`（本地仍按 period 物化精确值）。
fn map_time_period(p: &str) -> &'static str {
    match p {
        "1d" => "DAY",
        "1w" => "WEEK",
        "1m" => "MONTH",
        "1y" | "ytd" | "all" => "ALL",
        _ => "ALL",
    }
}

/// 通用 `pnl`/`vol`/`roi`/`win_rate` → Polymarket `PNL`/`VOL`（仅支持 PNL/VOL）。
fn map_order_by(o: &str) -> &'static str {
    match o {
        "vol" => "VOL",
        // Polymarket 官方只支持 PNL/VOL；roi/win_rate 由 sharpside 自算
        _ => "PNL",
    }
}

// ── DTO → domain 映射 ──

fn map_leaderboard_entry(e: LeaderboardEntry) -> Trader {
    Trader {
        platform: Platform::Polymarket,
        venue_trader_id: e.proxy_wallet,
        alias: e.user_name,
        profile_image: e.profile_image,
        x_username: e.x_username,
        verified: e.verified_badge.unwrap_or(false),
        // 保留 Polymarket 排行榜自带的 pnl/vol 作为临时绩效种子，
        // 供 ingest 写临时 trader_performance 行（backfill + perf 跑完前先有数）。
        seed_pnl: e.pnl,
        seed_vol: e.vol,
    }
}

fn map_position(d: PositionDto, trader_id: &str) -> Option<Position> {
    Some(Position {
        platform: Platform::Polymarket,
        trader_id: trader_id.into(),
        market_id: d.market?,
        token_id: d.asset?,
        size: d.size.unwrap_or(0.0),
        avg_price: d.avg_price.unwrap_or(0.0),
        current_price: d.current_price.unwrap_or(0.0),
        pnl: d.realized_pnl.unwrap_or(0.0),
    })
}

fn map_trade(d: TradeDto, trader_id: &str) -> Option<Trade> {
    Some(Trade {
        platform: Platform::Polymarket,
        trader_id: trader_id.into(),
        // 真实 API 用 `conditionId`，mock 用 `market`；两者择一。
        market_id: d.market.or(d.condition_id)?,
        token_id: d.asset?,
        side: map_side(d.side.as_deref())?,
        price: d.price.unwrap_or(0.0),
        size: d.size.unwrap_or(0.0),
        ts: parse_ts(d.timestamp.as_deref())?,
        // mock 用 `id`，真实 API 用 `transactionHash`；两者择一（满足 raw_trades 去重约束）。
        tx_hash: d.id.or(d.transaction_hash),
    })
}

fn map_market(d: MarketDto) -> Option<Market> {
    let tags = d.tags.unwrap_or_default();
    let category = derive_category(&tags);
    Some(Market {
        platform: Platform::Polymarket,
        venue_market_id: d.condition_id?,
        title: d.question?,
        slug: d.slug,
        tags,
        category,
        end_date: d.end_date,
        outcome_yes: None,
        outcome_no: None,
        closed: Some(d.closed),
    })
}

/// 从 Polymarket Gamma `/markets` 返回的 `tags` 派生站内分类。
///
/// Polymarket tags 是自由标签（如 `["Politics","Election"]`），其排行榜 category 参数
/// 用固定枚举（OVERALL/POLITICS/SPORTS/ESPORTS/CRYPTO/CULTURE/MENTIONS/WEATHER/
/// ECONOMICS/TECH/FINANCE）。这里按 tag 大小写无关匹配已知分类，
/// 命中即返回该分类；无匹配返回 None（归入 OVERALL，由 perf worker 兜底）。
///
/// 多 tag 命中时按 `CATEGORY_TAGS` 顺序取首个（Politics 优先于 Election 等子类）。
fn derive_category(tags: &[String]) -> Option<String> {
    // (tag 关键字, 对应站内分类)。顺序即优先级。
    // 站内分类与 Polymarket Data API `/v1/leaderboard` category 枚举对齐；
    // 部分别名 tag（election/geopolitics/art/…）归入最近的官方分类。
    const CATEGORY_TAGS: &[(&str, &str)] = &[
        ("politics", "POLITICS"),
        ("election", "POLITICS"),
        ("geopolitics", "POLITICS"),
        ("iran", "POLITICS"),
        ("sports", "SPORTS"),
        ("esports", "ESPORTS"),
        ("crypto", "CRYPTO"),
        ("culture", "CULTURE"),
        ("art", "CULTURE"),
        ("mentions", "MENTIONS"),
        ("weather", "WEATHER"),
        ("economics", "ECONOMICS"),
        ("economy", "ECONOMICS"),
        ("tech", "TECH"),
        ("technology", "TECH"),
        ("finance", "FINANCE"),
    ];
    for (needle, cat) in CATEGORY_TAGS {
        if tags.iter().any(|t| t.to_lowercase() == *needle) {
            return Some((*cat).to_string());
        }
    }
    None
}

fn map_book_level(l: BookLevelDto) -> Option<OrderBookLevel> {
    Some(OrderBookLevel {
        price: l.price?.parse().ok()?,
        size: l.size?.parse().ok()?,
    })
}

/// Polymarket `side`：`BUY`/`SELL`（部分端点用 `YES`/`NO`，按 BUY/SELL 处理）。
fn map_side(s: Option<&str>) -> Option<Side> {
    match s.map(|x| x.to_uppercase()).as_deref() {
        Some("BUY") | Some("YES") => Some(Side::Buy),
        Some("SELL") | Some("NO") => Some(Side::Sell),
        _ => None,
    }
}

/// Polymarket timestamp：Unix 秒（字符串）。失败返回 None（该 trade 被过滤）。
fn parse_ts(s: Option<&str>) -> Option<DateTime<Utc>> {
    let s = s?;
    let secs: i64 = s.parse().ok()?;
    DateTime::from_timestamp(secs, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sharpside_kms::Kms;

    #[test]
    fn map_time_period_cases() {
        assert_eq!(map_time_period("1d"), "DAY");
        assert_eq!(map_time_period("1w"), "WEEK");
        assert_eq!(map_time_period("1m"), "MONTH");
        assert_eq!(map_time_period("1y"), "ALL");
        assert_eq!(map_time_period("ytd"), "ALL");
        assert_eq!(map_time_period("all"), "ALL");
        assert_eq!(map_time_period("unknown"), "ALL");
    }

    #[test]
    fn map_order_by_cases() {
        assert_eq!(map_order_by("vol"), "VOL");
        assert_eq!(map_order_by("pnl"), "PNL");
        assert_eq!(map_order_by("roi"), "PNL"); // 自算，回退 PNL
    }

    #[test]
    fn map_side_buy_sell() {
        assert_eq!(map_side(Some("BUY")), Some(Side::Buy));
        assert_eq!(map_side(Some("SELL")), Some(Side::Sell));
        assert_eq!(map_side(Some("buy")), Some(Side::Buy));
        assert_eq!(map_side(Some("YES")), Some(Side::Buy));
        assert_eq!(map_side(Some("NO")), Some(Side::Sell));
        assert_eq!(map_side(None), None);
        assert_eq!(map_side(Some("weird")), None);
    }

    #[test]
    fn parse_ts_unix_seconds() {
        let ts = parse_ts(Some("1700000000"));
        assert!(ts.is_some());
        assert_eq!(ts.unwrap().timestamp(), 1700000000);
    }

    #[test]
    fn parse_ts_invalid() {
        assert!(parse_ts(None).is_none());
        assert!(parse_ts(Some("not-a-number")).is_none());
    }

    #[test]
    fn map_leaderboard_entry_full() {
        let e = LeaderboardEntry {
            proxy_wallet: "0xabc".into(),
            user_name: Some("whale".into()),
            vol: Some(100.0),
            pnl: Some(50.0),
            profile_image: Some("img".into()),
            x_username: Some("whale_x".into()),
            verified_badge: Some(true),
            rank: Some("1".into()),
        };
        let t = map_leaderboard_entry(e);
        assert_eq!(t.platform, Platform::Polymarket);
        assert_eq!(t.venue_trader_id, "0xabc");
        assert_eq!(t.alias.as_deref(), Some("whale"));
        assert!(t.verified);
        // pnl/vol 作为临时绩效种子保留（之前被丢弃）
        assert_eq!(t.seed_pnl, Some(50.0));
        assert_eq!(t.seed_vol, Some(100.0));
    }

    #[test]
    fn map_position_filters_missing_fields() {
        // 缺 market → None
        let d = PositionDto {
            user: Some("0xabc".into()),
            market: None,
            asset: Some("12345".into()),
            size: Some(100.0),
            avg_price: Some(0.5),
            current_price: Some(0.6),
            realized_pnl: Some(10.0),
            side: Some("YES".into()),
        };
        assert!(map_position(d, "0xabc").is_none());

        // 完整 → Some
        let d = PositionDto {
            user: Some("0xabc".into()),
            market: Some("0xcond".into()),
            asset: Some("12345".into()),
            size: Some(100.0),
            avg_price: Some(0.5),
            current_price: Some(0.6),
            realized_pnl: Some(10.0),
            side: Some("YES".into()),
        };
        let p = map_position(d, "0xabc").unwrap();
        assert_eq!(p.market_id, "0xcond");
        assert!((p.size - 100.0).abs() < 1e-9);
    }

    #[test]
    fn map_trade_filters_missing() {
        // 缺 side → None
        let d = TradeDto {
            id: Some("t1".into()),
            taker_side: None,
            side: None,
            size: Some(50.0),
            price: Some(0.42),
            timestamp: Some("1700000000".into()),
            market: Some("0xcond".into()),
            condition_id: None,
            asset: Some("12345".into()),
            trade_owner: Some("0xabc".into()),
            proxy_wallet: None,
            transaction_hash: None,
        };
        assert!(map_trade(d, "0xabc").is_none());

        // 完整 → Some
        let d = TradeDto {
            id: Some("t1".into()),
            taker_side: None,
            side: Some("BUY".into()),
            size: Some(50.0),
            price: Some(0.42),
            timestamp: Some("1700000000".into()),
            market: Some("0xcond".into()),
            condition_id: None,
            asset: Some("12345".into()),
            trade_owner: Some("0xabc".into()),
            proxy_wallet: None,
            transaction_hash: None,
        };
        let t = map_trade(d, "0xabc").unwrap();
        assert_eq!(t.side, Side::Buy);
        assert!((t.price - 0.42).abs() < 1e-9);
        assert_eq!(t.tx_hash.as_deref(), Some("t1"));
    }

    #[test]
    fn map_trade_real_api_shape() {
        // 真实 API：timestamp 为数字、市场 ID 在 conditionId、无 market/tradeOwner/id。
        let d = TradeDto {
            id: None,
            taker_side: None,
            side: Some("BUY".into()),
            size: Some(265999.48),
            price: Some(0.43),
            timestamp: Some("1782518426".into()),
            market: None,
            condition_id: Some(
                "0xe322faca2a534900680db54e3a4349a61427d347b6f906d2eeb01f81ae1b082c".into(),
            ),
            asset: Some(
                "19257598872615589709464917571360097739195684520039112732916777341566206587544"
                    .into(),
            ),
            trade_owner: None,
            proxy_wallet: Some("0xd6505aab3c6bef32ae6c96dbd8023d7c4df114fb".into()),
            transaction_hash: Some(
                "0xe7d31926128f026ebc429a570fb0d169726a9d5acf69167844263068bbdc75aa".into(),
            ),
        };
        let t = map_trade(d, "0xd6505aab3c6bef32ae6c96dbd8023d7c4df114fb").unwrap();
        assert_eq!(t.side, Side::Buy);
        assert!((t.size - 265999.48).abs() < 1e-6);
        assert_eq!(
            t.market_id,
            "0xe322faca2a534900680db54e3a4349a61427d347b6f906d2eeb01f81ae1b082c"
        );
        assert_eq!(t.ts.timestamp(), 1782518426);
        assert_eq!(
            t.tx_hash.as_deref(),
            Some("0xe7d31926128f026ebc429a570fb0d169726a9d5acf69167844263068bbdc75aa")
        );
    }

    #[test]
    fn trade_dto_timestamp_numeric() {
        // 真实 API timestamp 为数字，应反序列化为字符串供 parse_ts 使用。
        let json = r#"{"side":"BUY","timestamp":1782518426,"conditionId":"0xcond","asset":"12345","size":1.0,"price":0.5}"#;
        let d: TradeDto = serde_json::from_str(json).unwrap();
        assert_eq!(d.timestamp.as_deref(), Some("1782518426"));
        assert_eq!(d.condition_id.as_deref(), Some("0xcond"));
        assert!(d.market.is_none());
        assert!(map_trade(d, "0xabc").is_some());
    }

    #[test]
    fn map_market_filters_missing() {
        let d = MarketDto {
            id: Some("m1".into()),
            condition_id: None, // 缺 → None
            question: Some("q".into()),
            slug: None,
            tags: None,
            end_date: None,
            outcomes: None,
            closed: false,
        };
        assert!(map_market(d).is_none());

        let d = MarketDto {
            id: Some("m1".into()),
            condition_id: Some("0xcond".into()),
            question: Some("Will Trump win?".into()),
            slug: Some("trump".into()),
            tags: Some(vec!["politics".into()]),
            end_date: None,
            outcomes: None,
            closed: false,
        };
        let m = map_market(d).unwrap();
        assert_eq!(m.venue_market_id, "0xcond");
        assert_eq!(m.title, "Will Trump win?");
        assert_eq!(m.tags.len(), 1);
        assert!(!m.closed.unwrap_or(true));
    }

    #[test]
    fn map_market_closed_propagates() {
        // closed=true 应透传到 Market.closed，赎回 worker 据此扫结算市场。
        let d = MarketDto {
            id: Some("m1".into()),
            condition_id: Some("0xcond".into()),
            question: Some("q".into()),
            slug: None,
            tags: None,
            end_date: None,
            outcomes: None,
            closed: true,
        };
        let m = map_market(d).unwrap();
        assert!(m.closed.unwrap_or(false));
    }

    #[test]
    fn map_book_level_parses_strings() {
        let l = BookLevelDto {
            price: Some("0.49".into()),
            size: Some("100".into()),
        };
        let lvl = map_book_level(l).unwrap();
        assert!((lvl.price - 0.49).abs() < 1e-9);
        assert!((lvl.size - 100.0).abs() < 1e-9);
    }

    #[test]
    fn venue_info_correct() {
        let v = PolymarketVenue::new();
        assert_eq!(v.info().platform, Platform::Polymarket);
        assert!(v
            .info()
            .capabilities
            .contains(VenueCapabilities::SIGNAL_SOURCE));
        assert!(v
            .info()
            .capabilities
            .contains(VenueCapabilities::EXECUTION_VENUE));
        assert_eq!(v.info().auth_model, AuthModel::Wallet);
        assert_eq!(v.info().unit, Unit::UsdcCtf);
        assert_eq!(v.info().geo, Geo::GlobalWithUsRestrictions);
    }

    #[tokio::test]
    async fn place_order_without_key_returns_auth_error() {
        // 无 dev 签名器 + KMS 未接入 + 无 env → Auth 错误（不再 Unsupported）。
        std::env::remove_var("POLYMARKET_DEV_PRIVATE_KEY");
        std::env::remove_var("POLYMARKET_DEV_PLAINTEXT_HANDLE");
        std::env::remove_var("POLYMARKET_CLOB_POST");
        let v = PolymarketVenue::new();
        let err = v
            .place_order(
                &sharpside_venues_core::Credential::Wallet {
                    encrypted_handle: "x".into(),
                },
                sharpside_venues_core::Order {
                    market_id: "m".into(),
                    token_id: "12345".into(),
                    side: Side::Buy,
                    price: 0.5,
                    size: 10.0,
                    idempotency_salt: None,
                    order_timestamp_ms: None,
                    order_type: sharpside_venues_core::OrderType::Gtc,
                    expiration: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, VenueError::Auth(_)));
    }

    #[tokio::test]
    async fn place_order_dry_signs_with_injected_signer() {
        // 注入 dev 签名器 → dry-sign 真签名，返回合成 Fill（tx_hash=签名）。无 env 竞态。
        std::env::remove_var("POLYMARKET_CLOB_POST");
        let signer = crate::clob::signer_from_hex(
            "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
        )
        .unwrap();
        let v = PolymarketVenue::new().with_dev_signer(signer);
        let fill = v
            .place_order(
                &sharpside_venues_core::Credential::Wallet {
                    encrypted_handle: "x".into(),
                },
                sharpside_venues_core::Order {
                    market_id: "m".into(),
                    token_id: "12345".into(),
                    side: Side::Buy,
                    price: 0.5,
                    size: 10.0,
                    idempotency_salt: None,
                    order_timestamp_ms: None,
                    order_type: sharpside_venues_core::OrderType::Gtc,
                    expiration: None,
                },
            )
            .await
            .unwrap();
        assert!(fill.order_id.starts_with("dry-sign-"));
        let tx = fill.tx_hash.unwrap();
        assert!(tx.starts_with("0x"));
        assert_eq!(tx.len(), 2 + 130); // 65 字节签名
    }

    #[tokio::test]
    async fn place_order_deposit_wallet_delegated_dry_signs() {
        // DepositWalletDelegated 凭证（主路径）+ 注入 dev signer → dry-sign POLY_1271 订单。
        std::env::remove_var("POLYMARKET_CLOB_POST");
        let signer = crate::clob::signer_from_hex(
            "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
        )
        .unwrap();
        let owner_addr = signer.address();
        let v = PolymarketVenue::new().with_dev_signer(signer);
        let cred = sharpside_venues_core::Credential::DepositWalletDelegated {
            deposit_wallet_address: "0x000000000000000000000000000000000000dEaD".into(),
            owner_address: owner_addr.to_string(),
            encrypted_owner_key: "ignored-with-dev-signer".into(),
            l2_api_key: "api-key".into(),
            encrypted_l2_secret: "l2-secret".into(),
            l2_passphrase: "pass".into(),
            builder_code: "sharpside-builder".into(),
        };
        let fill = v
            .place_order(
                &cred,
                sharpside_venues_core::Order {
                    market_id: "m".into(),
                    token_id: "12345".into(),
                    side: Side::Buy,
                    price: 0.5,
                    size: 10.0,
                    idempotency_salt: None,
                    order_timestamp_ms: None,
                    order_type: sharpside_venues_core::OrderType::Gtc,
                    expiration: None,
                },
            )
            .await
            .unwrap();
        assert!(fill.order_id.starts_with("dry-sign-deposit-"));
        let tx = fill.tx_hash.unwrap();
        assert!(tx.starts_with("0x"));
        // POLY_1271 走 ERC-7739 wrap = 317 字节 = 634 hex + 0x（对齐官方 TS SDK）。
        assert_eq!(tx.len(), 2 + 634);
    }

    #[tokio::test]
    async fn place_order_deposit_wallet_owner_mismatch_errors() {
        // DepositWalletDelegated.owner_address 与解出的 dev signer 不一致 → Auth 错误。
        std::env::remove_var("POLYMARKET_CLOB_POST");
        std::env::remove_var("POLYMARKET_DEV_PRIVATE_KEY");
        std::env::remove_var("POLYMARKET_DEV_PLAINTEXT_HANDLE");
        let signer = crate::clob::signer_from_hex(
            "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
        )
        .unwrap();
        let v = PolymarketVenue::new().with_dev_signer(signer);
        let cred = sharpside_venues_core::Credential::DepositWalletDelegated {
            deposit_wallet_address: "0x000000000000000000000000000000000000dEaD".into(),
            owner_address: "0x0000000000000000000000000000000000000bAd".into(), // 故意不一致
            encrypted_owner_key: "x".into(),
            l2_api_key: "k".into(),
            encrypted_l2_secret: "s".into(),
            l2_passphrase: "p".into(),
            builder_code: "bc".into(),
        };
        let err = v
            .place_order(
                &cred,
                sharpside_venues_core::Order {
                    market_id: "m".into(),
                    token_id: "12345".into(),
                    side: Side::Buy,
                    price: 0.5,
                    size: 10.0,
                    idempotency_salt: None,
                    order_timestamp_ms: None,
                    order_type: sharpside_venues_core::OrderType::Gtc,
                    expiration: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, VenueError::Auth(_)));
    }

    #[tokio::test]
    async fn place_order_deposit_wallet_with_kms_decrypts_and_dry_signs() {
        // 注入 DevKms + DepositWalletDelegated（密文 owner key）→ KMS 解密 → dry-sign POLY_1271。
        // 验证生产路径：copier 注入 Kms 后 place_order 自动解密 encrypted_owner_key。
        std::env::remove_var("POLYMARKET_CLOB_POST");
        std::env::remove_var("POLYMARKET_DEV_PRIVATE_KEY");
        std::env::remove_var("POLYMARKET_DEV_PLAINTEXT_HANDLE");
        let signer = crate::clob::signer_from_hex(
            "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
        )
        .unwrap();
        let owner_addr = signer.address();
        // 用 DevKms 加密 owner 私钥，模拟 account 预配写入的密文。
        let kms = sharpside_kms::DevKms::enabled_for_test();
        let owner_key_hex = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
        let encrypted_owner_key = kms.encrypt(owner_key_hex).unwrap();
        let encrypted_l2_secret = kms.encrypt("l2-secret-plain").unwrap();

        let v = PolymarketVenue::new().with_kms(std::sync::Arc::new(kms));
        let cred = sharpside_venues_core::Credential::DepositWalletDelegated {
            deposit_wallet_address: "0x000000000000000000000000000000000000dEaD".into(),
            owner_address: owner_addr.to_string(),
            encrypted_owner_key,
            l2_api_key: "api-key".into(),
            encrypted_l2_secret,
            l2_passphrase: "pass".into(),
            builder_code: "sharpside-builder".into(),
        };
        let fill = v
            .place_order(
                &cred,
                sharpside_venues_core::Order {
                    market_id: "m".into(),
                    token_id: "12345".into(),
                    side: Side::Buy,
                    price: 0.5,
                    size: 10.0,
                    idempotency_salt: None,
                    order_timestamp_ms: None,
                    order_type: sharpside_venues_core::OrderType::Gtc,
                    expiration: None,
                },
            )
            .await
            .unwrap();
        assert!(fill.order_id.starts_with("dry-sign-deposit-"));
        let tx = fill.tx_hash.unwrap();
        assert!(tx.starts_with("0x"));
        // POLY_1271 走 ERC-7739 wrap = 317 字节 = 634 hex + 0x（对齐官方 TS SDK）。
        assert_eq!(tx.len(), 2 + 634);
    }

    #[tokio::test]
    async fn place_order_deposit_wallet_kms_decrypt_failure_errors() {
        // 注入 Kms 但密文损坏 → KMS 解密失败 → Auth 错误（非 panic）。
        std::env::remove_var("POLYMARKET_CLOB_POST");
        std::env::remove_var("POLYMARKET_DEV_PRIVATE_KEY");
        std::env::remove_var("POLYMARKET_DEV_PLAINTEXT_HANDLE");
        let kms = sharpside_kms::DevKms::enabled_for_test();
        let v = PolymarketVenue::new().with_kms(std::sync::Arc::new(kms));
        let cred = sharpside_venues_core::Credential::DepositWalletDelegated {
            deposit_wallet_address: "0x000000000000000000000000000000000000dEaD".into(),
            owner_address: "0x0000000000000000000000000000000000000bAd".into(),
            encrypted_owner_key: "not-a-dev-ciphertext".into(),
            l2_api_key: "k".into(),
            encrypted_l2_secret: "s".into(),
            l2_passphrase: "p".into(),
            builder_code: "bc".into(),
        };
        let err = v
            .place_order(
                &cred,
                sharpside_venues_core::Order {
                    market_id: "m".into(),
                    token_id: "12345".into(),
                    side: Side::Buy,
                    price: 0.5,
                    size: 10.0,
                    idempotency_salt: None,
                    order_timestamp_ms: None,
                    order_type: sharpside_venues_core::OrderType::Gtc,
                    expiration: None,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, VenueError::Auth(_)));
    }
}
