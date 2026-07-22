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
    pub siwe_max_age_secs: i64,
    /// 钱包登录：允许的 chainId 白名单（Polymarket 在 137，主网 1）。
    pub siwe_allowed_chains: Vec<u64>,
}

impl Config {
    pub fn from_env() -> Self {
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
                .unwrap_or(86_400),
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
            public_domain: env::var("PUBLIC_DOMAIN").unwrap_or_else(|_| "localhost".into()),
            siwe_max_age_secs: env::var("SIWE_MAX_AGE_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(300),
            siwe_allowed_chains: env::var("SIWE_ALLOWED_CHAIN_IDS")
                .unwrap_or_else(|_| "137,1".into())
                .split(',')
                .filter_map(|s| s.trim().parse::<u64>().ok())
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        std::env::remove_var("ACCOUNT_LISTEN_ADDR");
        let c = Config::from_env();
        assert_eq!(c.listen_addr, "0.0.0.0:8084");
        assert!(c.pbkdf2_iterations >= 10_000);
    }
}
