//! 环境变量配置。对应 `docs/ARCHITECTURE.md` §6.4。

use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_addr: String,
    pub database_url: String,
    pub db_max_connections: u32,
    /// JWT 签名密钥（HS256）
    pub jwt_secret: String,
    /// JWT 过期秒数
    pub jwt_ttl_seconds: i64,
    /// PBKDF2 迭代次数（遗留：新哈希用 argon2，此值仅兼容旧 env，不再读取）。
    #[allow(dead_code)]
    pub pbkdf2_iterations: u32,
    /// TG bot 共享密钥：`POST /auth/tg` 须带 `X-TG-Bot-Secret` 匹配此值。
    /// bot 代 TG 用户换 JWT，故该端点需鉴权（不能裸开放）。
    pub tg_bot_secret: String,
    /// /auth/* 限流：每分钟每 IP 最大请求数（防暴力撞库 / 注册刷量）。
    pub auth_rate_limit_per_min: u32,
    /// 钱包登录：SIWE domain 绑定（前端 SIWE 消息的 domain 字段须等于此值，防钓鱼）。
    pub public_domain: String,
    /// 钱包登录：SIWE 消息最大有效期（秒，issued_at 距今的上限，防陈旧重放）。
    /// 同时用作 `auth_nonces` 行 TTL（consume 与 cleanup）。
    pub siwe_max_age_secs: i64,
    /// 钱包登录：允许的 chainId 白名单（Polymarket 在 137，主网 1）。
    pub siwe_allowed_chains: Vec<u64>,
    /// SIWE URI 白名单（防同域跨页钓鱼签名）。nonce 响应返回 `siwe_preferred_uri` 供前端使用。
    pub siwe_allowed_uris: Vec<String>,
    /// nonce 响应推荐 URI（前端应写入 SIWE 消息，避免用任意 location.origin）。
    pub siwe_preferred_uri: String,
    /// 安全修复 3.1：会话 cookie 是否带 `Secure`（仅 HTTPS 下浏览器才接收）。
    /// 生产（APP_ENV=production，HTTPS）默认 true；本地 HTTP 开发须 false。
    /// 可用 `COOKIE_SECURE=1|0` 覆盖。
    pub cookie_secure: bool,
    /// 内部管理端点共享密钥（`/internal/*` 鉴权，如凭证 upsert）。
    /// 须经服务网格 / 私网调用；gateway 已对 `/api/account/internal/*` 返回 404。
    /// 生产须 ≥32 字符且非默认（`assert_secret`）；空串时内部端点 401。
    pub internal_secret: String,
    // —— Pro+ USDC 计费（Polygon）——
    /// 平台收款地址（小写 0x）。空 = 未启用创建发票。
    pub billing_treasury_address: String,
    /// 收款代币合约（Polygon native USDC）。空 = 未启用。
    pub billing_usdc_address: String,
    pub billing_chain_id: i32,
    pub billing_price_30d_usdc: rust_decimal::Decimal,
    pub billing_price_90d_usdc: rust_decimal::Decimal,
    pub billing_invoice_ttl_secs: i64,
    pub billing_grace_secs: i64,
    /// 链上确认所需块数（`eth_blockNumber - receipt.blockNumber + 1`）。
    pub billing_confirmations: u32,
    /// getLogs 向前回看块数（无 submit-tx 认领窗口）。
    pub billing_logs_lookback_blocks: u64,
    /// getLogs 单次请求块跨度（公共 RPC 限制）。
    pub billing_logs_chunk_blocks: u64,
    /// true 时确认要求 from ∈ user_wallets。
    pub billing_require_linked_wallet: bool,
    pub billing_worker_enabled: bool,
    pub worker_billing_secs: u64,
    pub worker_billing_expiry_secs: u64,
}

impl Config {
    /// 收款地址与代币均已配置时可创建发票。
    pub fn billing_enabled(&self) -> bool {
        !self.billing_treasury_address.is_empty() && !self.billing_usdc_address.is_empty()
    }

    pub fn from_env() -> Self {
        let public_domain = env::var("PUBLIC_DOMAIN").unwrap_or_else(|_| "localhost".into());
        let (siwe_preferred_uri, siwe_allowed_uris) = build_siwe_uris(&public_domain);
        Self {
            listen_addr: env::var("ACCOUNT_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8084".into()),
            database_url: env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://sharpside:sharpside@127.0.0.1:5432/sharpside".into()
            }),
            db_max_connections: env::var("DB_MAX_CONNECTIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            jwt_secret: sharpside_shared::secrets::assert_secret(
                "JWT_SECRET",
                &env::var("JWT_SECRET").unwrap_or_else(|_| "dev-secret-change-me".into()),
            )
            .to_string(),
            jwt_ttl_seconds: env::var("JWT_TTL_SECONDS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1_800),
            pbkdf2_iterations: env::var("PBKDF2_ITERATIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100_000),
            tg_bot_secret: sharpside_shared::secrets::assert_secret(
                "TG_BOT_SECRET",
                &env::var("TG_BOT_SECRET").unwrap_or_else(|_| "dev-tg-bot-secret".into()),
            )
            .to_string(),
            auth_rate_limit_per_min: env::var("AUTH_RATE_LIMIT_PER_MIN")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            public_domain,
            siwe_max_age_secs: env::var("SIWE_MAX_AGE_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(300),
            siwe_allowed_chains: env::var("SIWE_ALLOWED_CHAIN_IDS")
                .unwrap_or_else(|_| "137,1".into())
                .split(',')
                .filter_map(|s| s.trim().parse::<u64>().ok())
                .collect(),
            siwe_preferred_uri,
            siwe_allowed_uris,
            cookie_secure: match env::var("COOKIE_SECURE").ok().as_deref() {
                Some("1") | Some("true") => true,
                Some("0") | Some("false") => false,
                _ => sharpside_shared::secrets::is_production(),
            },
            internal_secret: sharpside_shared::secrets::assert_secret(
                "ACCOUNT_INTERNAL_SECRET",
                &env::var("ACCOUNT_INTERNAL_SECRET")
                    .unwrap_or_else(|_| "e2e-account-internal-secret".into()),
            )
            .to_string(),
            billing_treasury_address: normalize_optional_address(
                &env::var("BILLING_TREASURY_ADDRESS").unwrap_or_default(),
            ),
            billing_usdc_address: normalize_optional_address(
                &env::var("BILLING_USDC_ADDRESS").unwrap_or_else(|_| {
                    // Polygon native USDC；空 treasury 时仍不启用创建。
                    "0x3c499c542cef5e3811e1192ce70d8cc03d5c3359".into()
                }),
            ),
            billing_chain_id: env::var("BILLING_CHAIN_ID")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(137),
            billing_price_30d_usdc: parse_decimal_env("BILLING_PRICE_30D_USDC", "30"),
            billing_price_90d_usdc: parse_decimal_env("BILLING_PRICE_90D_USDC", "72"),
            billing_invoice_ttl_secs: env::var("BILLING_INVOICE_TTL_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1_800),
            billing_grace_secs: env::var("BILLING_GRACE_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(86_400),
            billing_confirmations: env::var("BILLING_CONFIRMATIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
            billing_logs_lookback_blocks: env::var("BILLING_LOGS_LOOKBACK_BLOCKS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5_000),
            billing_logs_chunk_blocks: env::var("BILLING_LOGS_CHUNK_BLOCKS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1_000),
            billing_require_linked_wallet: env::var("BILLING_REQUIRE_LINKED_WALLET")
                .ok()
                .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            billing_worker_enabled: env::var("BILLING_WORKER_ENABLED")
                .ok()
                .map(|s| !(s == "0" || s.eq_ignore_ascii_case("false")))
                .unwrap_or(true),
            worker_billing_secs: env::var("WORKER_BILLING_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(15),
            worker_billing_expiry_secs: env::var("WORKER_BILLING_EXPIRY_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(300),
        }
    }
}

fn parse_decimal_env(name: &str, default: &str) -> rust_decimal::Decimal {
    let raw = env::var(name).unwrap_or_else(|_| default.to_string());
    raw.parse()
        .unwrap_or_else(|_| default.parse().expect("default decimal"))
}

fn normalize_optional_address(raw: &str) -> String {
    let t = raw.trim().to_lowercase();
    if t.is_empty() {
        return String::new();
    }
    t
}

/// 构造 SIWE URI 白名单 + 推荐 URI。
///
/// - `PUBLIC_URI`：显式推荐 URI（优先）。
/// - `SIWE_ALLOWED_URIS`：逗号分隔额外允许项。
/// - 默认：`https://{domain}`；localhost/127.0.0.1 另允 http 与常见本地端口。
fn build_siwe_uris(domain: &str) -> (String, Vec<String>) {
    let mut uris: Vec<String> = Vec::new();
    let push = |list: &mut Vec<String>, u: String| {
        let n = normalize_uri(&u);
        if !n.is_empty() && !list.iter().any(|x| x == &n) {
            list.push(n);
        }
    };

    let preferred = env::var("PUBLIC_URI").unwrap_or_else(|_| {
        if is_local_dev_host(domain) {
            format!("http://{domain}")
        } else {
            format!("https://{domain}")
        }
    });
    push(&mut uris, preferred.clone());
    push(&mut uris, format!("https://{domain}"));

    if is_local_dev_host(domain) {
        push(&mut uris, format!("http://{domain}"));
        for port in [8070_u16, 8080, 3000, 5173] {
            push(&mut uris, format!("http://{domain}:{port}"));
        }
        let alt = if domain == "localhost" {
            "127.0.0.1"
        } else {
            "localhost"
        };
        push(&mut uris, format!("http://{alt}"));
        for port in [8070_u16, 8080, 3000, 5173] {
            push(&mut uris, format!("http://{alt}:{port}"));
        }
    }

    if let Ok(extra) = env::var("SIWE_ALLOWED_URIS") {
        for part in extra.split(',') {
            push(&mut uris, part.trim().to_string());
        }
    }

    (normalize_uri(&preferred), uris)
}

fn is_local_dev_host(domain: &str) -> bool {
    domain == "localhost" || domain == "127.0.0.1"
}

/// 去掉尾部 `/`，便于白名单比对。
pub fn normalize_uri(uri: &str) -> String {
    let t = uri.trim();
    if t.len() > 1 {
        t.trim_end_matches('/').to_string()
    } else {
        t.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        std::env::remove_var("ACCOUNT_LISTEN_ADDR");
        std::env::remove_var("PUBLIC_URI");
        std::env::remove_var("SIWE_ALLOWED_URIS");
        let c = Config::from_env();
        assert_eq!(c.listen_addr, "0.0.0.0:8084");
        assert!(c.pbkdf2_iterations >= 10_000);
        assert!(c.siwe_allowed_uris.iter().any(|u| u.contains("localhost")));
    }

    #[test]
    fn normalize_uri_strips_trailing_slash() {
        assert_eq!(normalize_uri("https://app.example/"), "https://app.example");
        assert_eq!(normalize_uri("http://localhost:8070"), "http://localhost:8070");
    }
}
