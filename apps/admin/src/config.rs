//! 环境变量配置。

use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_addr: String,
    pub database_url: String,
    pub db_max_connections: u32,
    /// admin 鉴权 token（MVP 单一共享 token；生产接 SSO/OIDC）
    pub admin_token: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            listen_addr: env::var("ADMIN_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8086".into()),
            database_url: env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://sharpside:sharpside@127.0.0.1:5432/sharpside".into()
            }),
            db_max_connections: env::var("DB_MAX_CONNECTIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            admin_token: sharpside_shared::secrets::assert_secret(
                "ADMIN_TOKEN",
                &env::var("ADMIN_TOKEN").unwrap_or_else(|_| "dev-admin-token".into()),
            )
            .to_string(),
        }
    }
}
