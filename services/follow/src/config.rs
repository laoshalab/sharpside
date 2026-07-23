//! 环境变量配置。对应 `docs/ARCHITECTURE.md` §6.2。

use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_addr: String,
    pub database_url: String,
    pub db_max_connections: u32,
    /// JWT 校验密钥（与 account 服务共用）
    pub jwt_secret: String,
    /// venue-hub 地址（信号派生回查 identity verified 用，可选）
    #[allow(dead_code)]
    pub venue_hub_url: String,
    /// 内部信号端点共享密钥（`/internal/signals` 鉴权）。
    /// **强制配置**：空串时 `/internal/signals` 直接 401 拒绝接收信号（防 follow 端口误暴露公网被灌单）。
    /// 须与 venue-hub 的 `FOLLOW_SIGNAL_SECRET` 一致。dev/e2e 用 `e2e-internal-secret`。
    pub internal_signal_secret: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            listen_addr: env::var("FOLLOW_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8082".into()),
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
            venue_hub_url: env::var("VENUE_HUB_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8081".into()),
            internal_signal_secret: sharpside_shared::secrets::assert_secret(
                "INTERNAL_SIGNAL_SECRET",
                &env::var("INTERNAL_SIGNAL_SECRET").unwrap_or_default(),
            )
            .to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        std::env::remove_var("FOLLOW_LISTEN_ADDR");
        let c = Config::from_env();
        assert_eq!(c.listen_addr, "0.0.0.0:8082");
        assert!(c.venue_hub_url.starts_with("http://"));
    }
}
