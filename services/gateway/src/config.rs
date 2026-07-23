//! 环境变量配置。对应 `docs/ARCHITECTURE.md` §6.5 与 `infra/.env.example`。
//!
//! MVP 不引入 figment（未缓存），直接从环境变量读取，缺失项回退默认值。
//! 生产部署通过 docker-compose `env_file` 注入。

use std::env;

/// Gateway 运行配置。
#[derive(Debug, Clone)]
pub struct Config {
    /// 监听地址
    pub listen_addr: String,
    /// Postgres（JWT denylist，与 account/follow/copier 同库；本服务不跑 migrate）
    pub database_url: String,
    pub db_max_connections: u32,
    /// JWT 签名密钥（HS256）
    pub jwt_secret: String,
    /// JWT 过期秒数
    pub jwt_ttl_seconds: i64,
    /// 上游服务 URL
    pub upstreams: Upstreams,
    /// 限流配置
    pub rate_limit: RateLimitConfig,
    /// 是否启用开发辅助端点（`/dev/token`）。
    ///
    /// 生产环境必须关闭：该端点无鉴权，可对任意 user_id 签发 JWT。
    /// 启用条件（任一）：debug 构建（`cfg!(debug_assertions)`）、
    /// 或显式 `DEV_ENDPOINTS_ENABLED=1`。
    pub dev_endpoints_enabled: bool,
    /// 安全修复 3.1：会话 cookie 是否带 Secure。生产默认 true，本地 HTTP 须 false。
    pub cookie_secure: bool,
}

#[derive(Debug, Clone)]
pub struct Upstreams {
    pub venue_hub: String,
    pub follow: String,
    pub copier: String,
    pub account: String,
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// 默认组：每秒每 IP 请求数
    pub default_rps: u32,
}

impl Config {
    /// 从环境变量加载。缺失项回退默认值（便于本地开发）。
    pub fn from_env() -> Self {
        Self {
            listen_addr: env::var("GATEWAY_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            database_url: env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://sharpside:sharpside@127.0.0.1:5432/sharpside".into()
            }),
            db_max_connections: env::var("DB_MAX_CONNECTIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            jwt_secret: sharpside_shared::secrets::assert_secret(
                "JWT_SECRET",
                &env::var("JWT_SECRET").unwrap_or_else(|_| "dev-secret-change-me".into()),
            )
            .to_string(),
            jwt_ttl_seconds: env::var("JWT_TTL_SECONDS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1_800),
            upstreams: Upstreams {
                venue_hub: env::var("VENUE_HUB_URL")
                    .unwrap_or_else(|_| "http://127.0.0.1:8081".into()),
                follow: env::var("FOLLOW_URL").unwrap_or_else(|_| "http://127.0.0.1:8082".into()),
                copier: env::var("COPIER_URL").unwrap_or_else(|_| "http://127.0.0.1:8083".into()),
                account: env::var("ACCOUNT_URL").unwrap_or_else(|_| "http://127.0.0.1:8084".into()),
            },
            rate_limit: RateLimitConfig {
                default_rps: env::var("RATE_LIMIT_DEFAULT_RPS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(20),
            },
            // 生产环境（APP_ENV=production）强制关闭 dev 端点，忽略 DEV_ENDPOINTS_ENABLED。
            // 启用条件：debug 构建 且 非生产；或显式 DEV_ENDPOINTS_ENABLED=1 且 非生产。
            dev_endpoints_enabled: {
                let prod = sharpside_shared::secrets::is_production();
                if prod {
                    false
                } else {
                    cfg!(debug_assertions)
                        || env::var("DEV_ENDPOINTS_ENABLED")
                            .map(|v| v == "1")
                            .unwrap_or(false)
                }
            },
            cookie_secure: match env::var("COOKIE_SECURE").ok().as_deref() {
                Some("1") | Some("true") => true,
                Some("0") | Some("false") => false,
                _ => sharpside_shared::secrets::is_production(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        // 不设任何环境变量时应回退默认值
        std::env::remove_var("GATEWAY_LISTEN_ADDR");
        std::env::remove_var("JWT_SECRET");
        let c = Config::from_env();
        assert_eq!(c.listen_addr, "0.0.0.0:8080");
        assert!(!c.jwt_secret.is_empty());
        assert_eq!(c.jwt_ttl_seconds, 1_800);
        assert!(c.upstreams.venue_hub.starts_with("http://"));
    }
}
