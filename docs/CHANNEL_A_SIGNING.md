# 通道 A 签名方案 · Deposit Wallet (POLY_1271) + 委托交易 + KMS + Builder

> 对标 FrenFlow 的「嵌入式钱包 + 资产仓 + KMS 代签 + Builder 归因」签名模型，
> 适配 Polymarket 官方推荐方向（**新 API 用户走 deposit wallet / POLY_1271**）与 Sharpside 全 Rust 栈。
>
> **Sharpside 不引入嵌入式钱包 SaaS（Privy / Magic / Web3Auth / Turnkey 等）**：
> TG bot 用 TG auth 登录，无需 email/social → 钱包 UX；签名权由平台 KMS 持有（owner EOA 私钥加密入库），
> ERC-7739-wrapped POLY_1271 包装逻辑在本地 alloy 代码完全可控。详见 §4.3 与 Phase 2 升级路径。
>
> 对应 `docs/ARCHITECTURE.md` §6.3-6.4、§11；`docs/FLOWS.md` §6；`docs/VENUEHUB_STORAGE.md` §8。

## 1. 为什么用 Deposit Wallet

### 1.1 Polymarket 官方方向（引自 docs.polymarket.com，2026-07）

| 信号 | 出处 |
|---|---|
| "New API users should use deposit wallets with signature type 3." | trading/overview §Signature Types |
| "POLY_1271 = 3 — Deposit wallet flow for new API users. Orders are signed by the owner/session signer and validated by ERC-1271" | 同上 |

**结论**：deposit wallet (type 3, POLY_1271) 是新集成推荐路径。Sharpside 作为新 builder，主路径走 deposit wallet。

### 1.2 旧通道 A 的局限（与 FrenFlow 式升级动机）

| 问题 | 说明 |
|---|---|
| 权限边界粗 | 平台持 `Credential::Wallet.encrypted_handle`（session 句柄），能力模糊 |
| 资产地址不独立 | maker=signer=EOA，资产与签名权在同一钥 |
| 无 L2 HMAC | `post_order` 不带 `POLY_*` L2 头，无法过 CLOB 鉴权 |
| 无 Builder 归因 | 订单不带 `builderCode`，无免 gas / Leaderboard 归因 |
| 话术被动 | 只能叫「平台代签」，无法对齐「委托交易」分层 |

### 1.3 Deposit Wallet 模型的核心

```
资产所有权        →  Deposit Wallet（ERC-1967 proxy，用户地址，放 pUSD / conditional tokens）
交易代理权        →  owner EOA（平台 KMS 持其私钥）签 POLY_1271 订单
CLOB 鉴权         →  L2 HMAC（apiKey/secret/passphrase，由 owner EOA L1 派生）
归因 + 免 gas      →  Builder Code + Polymarket Relayer（WALLET-CREATE / WALLET batch）
被攻破后果        →  乱下单（交易权），难直接 transfer（deposit wallet owner 权在平台 KMS，但资产在 deposit wallet 合约）
```

## 2. 凭证模型

### 2.1 Credential 变体

```rust
pub enum Credential {
    /// 旧：session wallet 句柄（兼容，不推荐新用户）
    Wallet { encrypted_handle: String },

    /// 主路径：Deposit Wallet (POLY_1271) + 委托交易 owner EOA + L2 HMAC + Builder 归因
    DepositWalletDelegated {
        /// Deposit wallet 地址（ERC-1967 proxy）= CLOB order maker / signer / funder
        deposit_wallet_address: String,
        /// Owner EOA 地址（拥有 deposit wallet，签 POLY_1271 订单 + WALLET batch）
        owner_address: String,
        /// KMS 加密的 owner EOA 私钥（hex，可带 0x）
        encrypted_owner_key: String,
        /// L2 CLOB API 凭证（HMAC-SHA256 用）
        l2_api_key: String,
        /// KMS 加密的 L2 secret
        encrypted_l2_secret: String,
        /// L2 passphrase
        l2_passphrase: String,
        /// Polymarket Builder Code
        builder_code: String,
    },

    KycApiKey { encrypted_api_key: String, encrypted_api_secret: String },
    ApiKey { encrypted_key: String },
}
```

### 2.2 存储（`user_venue_credentials`）

jsonb blob 结构：

```json
// 主：Polymarket · DepositWalletDelegated
{ "kind": "deposit_wallet_delegated",
  "deposit_wallet_address": "0x...",
  "owner_address": "0x...",
  "encrypted_owner_key": "AQICAHh...",
  "l2_api_key": "poly-uuid",
  "encrypted_l2_secret": "AQICAHh...",
  "l2_passphrase": "pass",
  "builder_code": "sharpside-builder" }

// 旧：Polymarket · Wallet（最早期 session 句柄，dev/兼容）
{ "kind": "wallet", "encrypted_handle": "..." }
```

新增列 `proxy_address`（deposit wallet 地址，便于按地址索引/对账）。

## 3. 签名链路

### 3.1 一次性 setup（用户在场）

```
1. 用户注册/登录（Sharpside email 或 TG auth）
2. account 服务生成 owner EOA 私钥（alloy PrivateKeySigner）
3. KMS 加密私钥 → encrypted_owner_key
4. deriveDepositWalletAddress(owner_eoa) → deposit_wallet_address（CREATE2 确定性）
5. Relayer WALLET-CREATE（gasless，无需用户签名）部署 deposit wallet
6. ClobClient L1：owner EOA 签 EIP-712 → createOrDeriveApiKey → L2 凭证
7. Relayer WALLET batch：deposit wallet approve pUSD → CTF/Exchange/NegRisk（gasless）
8. CLOB update_balance_allowance（signature_type=3）同步余额缓存
9. account 写 user_venue_credentials (kind=deposit_wallet_delegated)
```

### 3.2 跟单执行（用户不在场）

```
trader.position.changed
    │
    ▼
Follow 派生 copy_order(channel=tg)
    │
    ▼
Copier worker 取 pending(channel=tg)
    │
    ├─ 管辖域过滤
    ├─ 跨 Venue 映射 + 单位换算
    ├─ 风控（日限额 / 熔断 / 滑点）
    │
    ▼
load_credential → Credential::DepositWalletDelegated
    │
    ├─ KMS 解密 encrypted_owner_key → owner EOA 私钥
    ├─ KMS 解密 encrypted_l2_secret → L2 secret
    │
    ▼
build_order(signatureType=3 POLY_1271, maker=signer=deposit_wallet_address)
    │
    ▼
sign ERC-7739-wrapped POLY_1271 with owner EOA private key
    │
    ▼
L2 HMAC headers (POLY_ADDRESS / POLY_SIGNATURE / POLY_TIMESTAMP / POLY_API_KEY / POLY_PASSPHRASE)
    │
    ▼
POST /order (body含 builderCode) → CLOB
    │
    ▼
Fill → copy_execution
```

### 3.3 订单结构变化

| 字段 | 旧（EOA type 0） | **Deposit Wallet (type 3)** |
|---|---|---|
| `signatureType` | 0 | **3 (POLY_1271)** |
| `maker` | signer EOA | **deposit_wallet_address** |
| `signer` | signer EOA | **deposit_wallet_address**（= maker） |
| `signature` | EOA EIP-712 | **ERC-7739-wrapped POLY_1271** |
| HTTP headers | 无 L2 | POLY_* L2 HMAC |
| `builderCode` | 无 | 有 |
| 资产币 | USDC.e | **pUSD** |

## 4. 安全模型

### 4.1 权限分层

| 能力 | 谁持有 | 机制 |
|---|---|---|
| Deposit wallet 资产转账（任意 to） | 平台（owner EOA = deposit wallet owner） | WALLET batch 由 owner EOA 签，Relayer 提交 |
| 用余额下单 | 平台（跟单时） | owner EOA 签 POLY_1271 订单 + Exchange 已 approve |
| 收 Builder fee | 平台 | builderCode 归因 |
| 撤销交易权 | 用户 | 导出 owner EOA 私钥后自行 rotate owner / 撤销 approve |

### 4.2 被攻破后果

| 攻破层 | 后果 | 缓解 |
|---|---|---|
| Copier 服务被打 | 乱下单（交易权） | 风控限额 + 滑点保护 + 日 notional 上限 |
| KMS 被打 | 拿到 owner EOA 私钥 → 可签单 + 可转 deposit wallet 资产 | KMS 访问审计 + per-user KMS key + Phase 2 把 owner 设为用户独立钥 |
| DB 被打 | 拿到加密 blob，无 KMS 无法解密 | KMS 与 DB 物理隔离 |

### 4.3 诚实口径

- **仍不叫「完全非托管」**：平台持 owner EOA 私钥，owner 即 deposit wallet 的 owner，平台可签 WALLET batch 转资产。
- **Phase 2 升级**：把 deposit wallet owner 设为用户独立导出的钥，平台只持 session signer（ERC-1271 delegate），则平台只持交易权、不持资产权——届时可叫「非托管交易」。
- **当前定位**：**「Deposit Wallet 资产 + 委托交易 owner」**，比旧 session 句柄边界更细，但未到完全非托管。

## 5. 与通道 B 的关系

| | 通道 A（Deposit Wallet 委托） | 通道 B（daemon 零钥） |
|---|---|---|
| 私钥位置 | 平台 KMS | 用户本机 |
| 平台能否独立下单 | 能 | 不能 |
| 平台零钥 | 否 | 是 |
| UX | 登录即用、免 gas、TG 一键跟 | 要装 daemon |
| 定位 | 大众默认档 | Pro+ 高信任档 |
| 延迟 | 可冲亚秒（KMS 池化 + 并行） | 受轮询影响 |

**共存**：`follow_relation.channel` 已支持 `tg` | `daemon`；`channel=tg` 走 DepositWalletDelegated，`channel=daemon` 仍走 daemon 零钥。

## 6. 实现拆解

| 模块 | 改动 | 状态 |
|---|---|---|
| `crates/venues/core/src/types.rs` | `Credential::DepositWalletDelegated` | ✅ |
| `crates/venues/polymarket/src/clob.rs` | `sign_clob_order_deposit`（POLY_1271，maker=signer=deposit wallet）+ L2 HMAC + L1 auth 签名 | ✅ |
| `crates/venues/polymarket/src/deposit.rs` | ERC-7739-wrapped POLY_1271 签名 + `derive_deposit_wallet_address`（CREATE2，Solady ERC1967 beacon clone，canary 验证） | ✅（env `POLYMARKET_DEPOSIT_SIG_MODE=erc7739` 启用包装） |
| `crates/venues/polymarket/src/relayer.rs` | Relayer REST 客户端（`WALLET-CREATE` + `WALLET` batch） | ✅ |
| `crates/venues/polymarket/src/lib.rs` | `place_order` 路由 DepositWalletDelegated / Wallet + KMS 注入 | ✅ |
| `crates/venues/polymarket/src/client.rs` | `derive_api_key_l1` + `update_balance_allowance(signature_type=3)` | ✅ |
| `crates/venues/polymarket/Cargo.toml` | +hmac +sha2 +hex +sharpside-kms | ✅ |
| `crates/kms/`（新 crate） | `Kms` trait + `DevKms`（env 明文）+ `AwsKms`（stub） | ✅ |
| `crates/db/migrations/0012_deposit_wallet.sql` | `proxy_address` 列 | ✅ |
| `crates/db/src/queries/account.rs` | `upsert_credential_with_proxy` | ✅ |
| `services/account/src/deposit.rs` | `/me/deposit-wallet/provision` 端点（离线/在线双模式） | ✅ |
| `services/account/src/routes.rs` | provision 路由 | ✅ |
| `services/copier/src/main.rs` | 启动注入 KMS（DevKms env / 生产换真 AwsKms） | ✅ |
| 文档 | 本文件 + ARCHITECTURE/VENUEHUB_STORAGE/FLOWS/TECH_STACK 更新 | ✅ |

## 7. 待办（需网络/外部依赖）

- [~] Polymarket Builder API key 申请（builderCode / secret / passphrase）
  - ✅ `POLYMARKET_BUILDER_API_KEY=019f691f-178d-7b2a-938c-65a28d3f0e92`（来源 `~/文档/sharpside/.env`，= RELAYER_API_KEY 回退）已填入 `.env`
  - ✅ `POLYMARKET_BUILDER_CODE=0x599ec9d1...3a17b92a`、`POLYMARKET_BUILDER_ADDRESS=0x1c404b67...beb388`、`POLYMARKET_RELAYER_URL` / `_API_KEY_ADDRESS=0x9CC0...bb5c` / `_PROXY` 已填入 `.env`
  - ✅ `POLYMARKET_BUILDER_API_KEY=019f84e5-...3044` + `POLYMARKET_BUILDER_SECRET` + `POLYMARKET_BUILDER_PASSPHRASE`：2026/7/21 重建 key 后一次性保存，已填入 `.env`（L2 HMAC 三元组齐）
- [x] **ERC-7739-wrapped POLY_1271 签名实现** —— 移植至独立 crate `crates/clob-auth`（`sign_poly_1271_order` / `sign_poly_1271_order_with_signer`），对齐官方 `@polymarket/clob-client-v2@1.1.0`
  - [x] 外层 domain = CTF Exchange V2 domain（非独立 DepositWallet domain）；外层 struct = `TypedDataSign{Order contents, name="DepositWallet", version="1", chainId, verifyingContract=DW, salt=0}`
  - [x] wire 签名 = `innerSig(65) || appDomainSep(32) || contentsHash(32) || OrderTypeString(186) || u16be(len)` = 317 字节
  - [x] **golden vector 字节级验证**：`type3_sdk_sign_only.json`（clob-auth 19 测试全绿，含 `poly_1271_erc7739_matches_official_ts_sdk_vector`）
  - [x] `sign_clob_order_deposit` 恒走 ERC-7739 wrap（移除 `POLYMARKET_DEPOSIT_SIG_MODE` plain/erc7739 开关——plain 对 POLY_1271 是错的）
  - [x] V2 Order struct（去 taker/expiration/nonce/feeRateBps，加 timestamp(ms)/metadata(bytes32)/builder(bytes32)）；domain version="2"，verifyingContract 按 neg_risk 切换 standard/neg-risk
  - [x] L1 ClobAuth EIP-712（domain `ClobAuthDomain`/v1/137 无 verifyingContract，struct `ClobAuth{address,timestamp(string),nonce,message}`，固定文案）—— `clob::build_l1_auth_signature` 走 `clob_auth::sign_l1_with_signer`
  - [x] L2 HMAC = `base64url(HMAC_SHA256(base64url_decode(secret), "{ts秒}{METHOD}{path不含query}{body}"))`，5 个 `POLY_*` 头，timestamp 单位=秒 —— `clob::l2_headers` 走 `clob_auth::l2_headers`
  - [x] `neg_risk` 按 market metadata 切换 V2 verifyingContract：`place_order` / daemon `execute_polymarket` 在真实提交（`POLYMARKET_CLOB_POST=1`）时解析，dry-sign 离线默认 false（standard）
    - [x] **真实联调修正**：CLOB `/book?token_id=` 响应**不含 `negRisk`**（早期假设错误）。正确两步：`/book?token_id=` → `market`(condition_id) → `GET /markets/{condition_id}` → `neg_risk`（snake_case）。`PolymarketClient::resolve_neg_risk` 已按此重写，新增 `clob_market(condition_id)` 取 `ClobMarketDto{neg_risk,active,accepting_orders}`
    - [x] **真实联调修正**：`PolymarketClient::book()` 原用 `?market=&asset=`（返回 "Invalid token id"），改为 `?token_id=`（venue trait 签名 `(market_id,token_id)` 保留，`market_id` 不再用作查询参数）
    - [x] **真实联调修正**：Gamma `/markets` 的 `outcomes`/`tags` 是**字符串化 JSON 数组**（`"[\"Yes\",\"No\"]"`）非原生数组；`MarketDto` 加 `deserialize_string_array` 兼容两种形状
    - [x] **代理支持**：reqwest `default-features=false` 不启用 `system-proxy`，新增 `build_http_client(timeout)` 显式读 `POLYMARKET_HTTP_PROXY`（回退 `POLYMARKET_RELAYER_PROXY`/`HTTPS_PROXY`/`HTTP_PROXY`）→ `Proxy::all`。`PolymarketClient` / `RelayerClient` 均走此构造。中国等地区封锁 Polymarket 时须经代理（如 Clash `http://127.0.0.1:7890`，`.env` 的 `POLYMARKET_RELAYER_PROXY` 在 Docker 内为 `host.docker.internal:7890`，宿主为 `127.0.0.1:7890`）
    - [x] **只读联调验证**（`#[ignore]` 测试，需代理 + full_network）：`client::tests::live_read_probe` / `live_read_probe_neg_risk` 真实命中 Polymarket——Gamma `/markets` 取活跃市场 → CLOB `/book?token_id=` 非空订单簿 → `resolve_neg_risk` 与 `/markets/{condition_id}` 交叉验证一致（standard=false ✓ / neg-risk=true ✓）
- [x] Relayer `WALLET-CREATE`（部署 deposit wallet，reqwest 直调 REST）—— `relayer.rs`
  - [x] **真实联调修正**：Relayer REST 形状与早期假设完全不符，已对齐 `~/文档/sharpside/bins/api/src/poly_relayer.rs`（生产参考）：
    - [x] 端点 `POST /submit`（非 `/wallet-create`），body `{"type":"WALLET-CREATE","from":owner,"to":DEPOSIT_WALLET_FACTORY}`，**无需用户签名**（builder HMAC 鉴权即可）
    - [x] 鉴权 = Builder HMAC 头（`POLY_BUILDER_API_KEY/PASSPHRASE/TIMESTAMP/SIGNATURE`，算法同 CLOB L2 HMAC，复用 `sharpside_clob_auth::builder_headers`），非 `Authorization: Bearer`。凭证 = 平台 builder 账户（env `POLYMARKET_BUILDER_*`），非 per-user L2 凭证
    - [x] `DEPOSIT_WALLET_FACTORY = 0x00000000000Fb5C9ADea0298D729A0CB3823Cc07`
    - [x] 部署异步：`POST /submit` → `transactionID` → 轮询 `GET /transaction?id=` 至 `STATE_MINED`/`STATE_CONFIRMED`；`GET /deployed?address=` 查部署
    - [x] **真实联调验证**（`tests/live_provision.rs` `#[ignore]`）：WALLET-CREATE → `STATE_MINED`（deposit wallet 真实上链，tx hash 返回）✓
- [ ] Relayer `WALLET` batch（approve pUSD → Exchange）—— `relayer.rs`
  - [ ] approve calldata 构造（`pUSD.approve(CTF_EXCHANGE, type(uint256).max)`）+ owner EIP-712 `wallet_batch_typed_data` 签名待接入（provision 端点 TODO 标注；当前 live 分支跳过 batch approve，不阻塞部署验证）
- [x] `deriveDepositWalletAddress` Rust 实现（CREATE2 + factory 0x00000000000Fb5C9ADea0298D729A0CB3823Cc07）—— `deposit.rs`
  - [x] init code hash 从 beacon 地址按 Solady LibClone `initCodeHashERC1967Beacon` 计算（移植自 `~/文档/sharpside/crates/poly-relayer/src/derive.rs`），无需 env
  - [x] salt = `keccak256(abi.encode(factory, bytes32(owner)))`（对齐工厂实现）
  - [x] Polygon beacon clone 路径（`DEPOSIT_WALLET_BEACON_POLYGON=0x7a18edfe...`）+ UUPS 路径保留
  - [x] 真实链上 canary 向量验证：owner `0x7b51...1a8a` → `0xa7a8...3711`（beacon）/ `0xeb07...11e7`（UUPS）
- [x] AWS KMS 接入（dev 路径用 env 明文）—— `crates/kms`：`DevKms` 可用，`AwsKms` stub
  - [ ] 真 `AwsKms`：加 `aws-sdk-kms` 依赖，替换 stub 为 `client.encrypt/decrypt` 调用
  - [ ] per-user KMS key_id 存 `account.users.kms_key_id`（未来迁移 0013）
- [x] account 服务 `/me/deposit-wallet/provision` 端点 —— `services/account/src/deposit.rs`（离线模式可用，在线模式需 env + 网络）
- [x] CLOB `update_balance_allowance(signature_type=3)` 余额同步 —— `client.rs`
  - [x] **真实联调修正**：官方 clob-client-v2 是 `GET /balance-allowance/update?asset_type=COLLATERAL&signature_type=3`（非 POST `/balance-allowance`），L2 HMAC method=GET、path=`/balance-allowance/update`。**POLY_ADDRESS = owner EOA**（L2 凭证所属地址，非 deposit wallet），`signature_type=3` 让服务端映射 owner→deposit wallet
  - [x] **真实联调修正**：L1 `deriveApiKey` 用**头** `POLY_ADDRESS/POLY_SIGNATURE/POLY_TIMESTAMP/POLY_NONCE`（非 body），先 `GET /auth/derive-api-key`（派生已有）失败再 `POST /auth/api-key`（创建新），**无 signatureType**（服务端按地址自动识别 wallet 类型）。`derive_api_key_l1` 已按此重写
  - [x] **真实联调验证**：L1 deriveApiKey → 真实 L2 凭证（api_key/secret/passphrase）✓；balance-allowance 对未充值 wallet 返回 404（良性，无余额可同步）

### Stage 3：真实下单（`POST /order`）联调发现

- [x] V2 wire body 形状对齐官方 `orderToJsonV2` —— `client.rs::post_order_l2`
  - [x] **真实联调修正**：`signature` 须在 `order` 对象**内**（非顶层）；`salt` 为 JSON **整数**（`parseInt`，≤2^53）；`order` 内加 `expiration:"0"`；顶层 `owner`=**L2 API key**（非 signer 地址，官方测试断言 `owner=="api-key-uuid"`）；顶层加 `deferExec:false`/`postOnly:false`；无 `taker`（OrderV2 无此字段）
- [x] `sign_clob_order_deposit` ERC-7739 POLY_1271 签名端到端可用（636 字节 wrapped sig）—— `clob.rs`
- [x] live 集成测试 `tests/live_post_order.rs`（`#[ignore]`）：部署 DW → L1 deriveApiKey → 选真实市场 → 签 V2 POLY_1271 单 → POST `/order`，全程不花真钱（未充值）。**复用同一 DW**：设 `POLYMARKET_TEST_OWNER_PK=<0x 私钥>` → 固定 owner EOA → 稳定 CREATE2 DW 地址；`is_deployed` 命中则跳过 `WALLET-CREATE`，便于充值后反复测真实下单
- [x] **Stage 3 签名路径已端到端验证**（不花真钱）：live 测试 `/order` 越过 L2 HMAC + `order.signer=DW` 校验 + ERC-7739 POLY_1271 签名 + V2 wire body + price 校验，仅卡在 `not enough balance / allowance: balance: 0, order amount: 2562500`（DW 未充值）——即 `STAGE3_OK_BUSINESS_REJECTED`，证明签名+提交全路径正确。
- [x] **amount 方向修正**（对齐官方 `getOrderRawAmounts`）：`build_v2_input` BUY=`maker:usdc(price*size), taker:shares(size)`、SELL=`maker:shares(size), taker:usdc(price*size)`（早先两者设反 → 服务端 price=maker/taker 算成 ≥1）。单测 `build_v2_input_buy_sell_amounts` 已更新。
- [x] **WALLET batch approve 已实现并链上验证**：`crates/venues/polymarket/src/wallet_batch.rs`
  - `approve_calldata`（ERC-20 `approve`，selector `0x095ea7b3`）/ `set_approval_for_all_calldata`（ERC-1155，selector `0xa22cb465`）
  - `trading_approves()`：pUSD → CTF Exchange V2 / NegRisk Exchange V2 / NegRisk Adapter（max approve）；Conditional Tokens → CTF/NegRisk Exchange（setApprovalForAll）——覆盖标准与 neg-risk 两类市场
  - `sign_wallet_batch`：owner EOA 对 `DepositWallet` `Batch` EIP-712 签名（普通 65 字节，**非** ERC-7739 wrap）；domain=`{name:"DepositWallet",version:"1",chainId:137,verifyingContract:DW}`，types=`Call[{target,value,data}]`/`Batch[{wallet,nonce,deadline,calls:Call[]}]`
  - `RelayerClient::wallet_nonce`（`GET /nonce?address=&type=WALLET`）+ `wallet_batch`（`POST /submit` type=WALLET，body 含 `depositWalletParams`）
  - live 验证：fresh owner PK → 部署 DW（STATE_MINED）→ WALLET batch approve（**STATE_MINED**，链上确认）→ `/order` 仅余额不足
  - 已接入 `services/account/src/deposit.rs` provision step 7（替换原 TODO 跳过）
- [x] **Stage 3 真实下单完成** ✅：DW 充值 7.0 pUSD 后，`live_post_order` 真打 `/order` 返回真实 orderID（`0xf04d21...`）——Channel A 端到端真实下单打通。
  - 充值/配置链上核验：pUSD 余额 7.0（`0x6acfc0`）；pUSD→CTF Exchange V2 / NegRisk Exchange V2 allowance 均 MAX；DW code 非空（EIP-1167 minimal proxy，已部署）。
  - 订单：BUY 5 USDC @ 0.50，市场 "Will Trump be in the WC Champions Photo?"（condition `0xc0b7319f...`，token `64778757908501179476331390591326653229579537200061619395979045269181713848562`，neg_risk=false，postOnly=false）。
  - 本地联调凭证存档：`.env.local`（gitignored）。
- [ ] Phase 2：deposit wallet owner 设为用户独立钥 + session signer delegate

## 8. 提现流程（WALLET batch transfer）

> 用户发起、平台代签、relayer gasless 提交。对应 `docs/FRONTEND_DESIGN.md` §6.5b 钱包页。

### 8.1 链路

```
用户在 #/wallet 提交 { to, amount }
    │  （to 须为用户绑定钱包 account.user_wallets 之一）
    ▼
copier POST /me/wallet/withdraw
    │
    ├─ 校验：to ∈ 绑定钱包 / 金额 ∈ [min,max] / 实时余额 ≥ 金额 / 日累计 ≤ daily_max
    ├─ 落库 account.withdrawals(status=pending)  ← 先审计再执行
    │
    ▼
PolymarketVenue::withdraw(cred, to, amount)
    │
    ├─ KMS 解密 encrypted_owner_key → owner EOA 私钥
    ├─ 构造 pUSD.transfer(to, amount*1e6) calldata（wallet_batch::transfer_calldata）
    ├─ relayer.wallet_nonce(owner) → 当前 WALLET batch nonce
    ├─ owner 对 DepositWallet.Batch 签 EIP-712（sign_wallet_batch，65 字节）
    ├─ relayer.wallet_batch(...) → transactionID
    ├─ relayer.poll_confirmed(tx_id) → STATE_MINED/CONFIRMED（~90s）
    │
    ▼
更新 account.withdrawals：mined(tx_hash) / pending(轮询超时) / failed(签名或链上失败)
```

### 8.2 安全考量

- **目标地址白名单**：`to` 须为用户已绑定钱包（`account.user_wallets`）。防资产被转到非本人地址（缓解 §4.1 "平台可签 WALLET 转资产"风险）。
- **金额风控**：单笔 `[WITHDRAW_MIN_AMOUNT, WITHDRAW_MAX_AMOUNT]` + 日累计 `WITHDRAW_DAILY_MAX`（pending+mined 计入）。env 可调。
- **先审计后执行**：落库 `account.withdrawals(status=pending)` 在发起链上交易之前，确保任何失败（签名/relayer/链上回退）都有据可查，状态机 pending→mined/failed。
- **gasless**：relayer 代付 gas，用户无需持有 MATIC；签名权仍在平台 KMS（owner EOA）。
- **Phase 2 升级**：deposit wallet owner 设为用户独立钥后，提现签名权回归用户（平台只转发），届时"平台可签 WALLET 转资产"风险消除。

### 8.3 实现拆解

| 模块 | 改动 | 状态 |
|---|---|---|
| `crates/venues/polymarket/src/wallet_batch.rs` | `transfer_calldata(to, amount)`（ERC-20 transfer，selector `0xa9059cbb`） | ✅ |
| `crates/venues/core/src/types.rs` | `WithdrawResult{ to, amount, tx_hash, relayer_tx_id }` | ✅ |
| `crates/venues/core/src/lib.rs` | `Venue::withdraw` 默认 `Unsupported` | ✅ |
| `crates/venues/polymarket/src/lib.rs` | `PolymarketVenue::withdraw`（KMS 解密→transfer calldata→nonce→签 Batch→relayer→轮询）+ `with_relayer` 注入 | ✅ |
| `crates/db/migrations/0019_withdrawals.sql` | `account.withdrawals` 审计表 | ✅ |
| `crates/db/src/queries/account.rs` | `insert_withdrawal` / `update_withdrawal_status` / `list_withdrawals` / `daily_withdrawal_total` | ✅ |
| `services/copier/src/routes.rs` | `GET /me/wallet` `POST /me/wallet/withdraw` `GET /me/wallet/withdrawals` | ✅ |
| `services/copier/src/config.rs` | `withdraw_min_amount` / `withdraw_max_amount` / `withdraw_daily_max` | ✅ |
| `services/copier/src/main.rs` | `build_registry` 注入 `RelayerClient` | ✅ |
