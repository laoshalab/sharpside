//! daemon 配置（环境变量）。

use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    /// copier 基址（`GET /me/copy-orders`、`POST /me/copy-orders/{id}/result`）
    pub copier_url: String,
    /// 用户 id（account 颁发）
    pub user_id: String,
    /// daemon_api_key 明文（account 颁发，仅本地持有）
    pub daemon_api_key: String,
    /// 轮询间隔（秒）
    pub poll_interval_secs: u64,
    /// 干跑：不本地签名下单，回传合成成交
    pub dry_run: bool,
    /// 本地风控：单笔最大 notional
    pub local_max_notional: f64,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            copier_url: env::var("COPIER_URL").unwrap_or_else(|_| "http://127.0.0.1:8083".into()),
            user_id: env::var("DAEMON_USER_ID").unwrap_or_default(),
            daemon_api_key: env::var("DAEMON_API_KEY").unwrap_or_default(),
            poll_interval_secs: env::var("DAEMON_POLL_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            dry_run: parse_bool("DAEMON_DRY_RUN", true),
            local_max_notional: env::var("DAEMON_LOCAL_MAX_NOTIONAL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1000.0),
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.user_id.is_empty() && !self.daemon_api_key.is_empty()
    }
}

pub(crate) fn parse_bool(key: &str, default: bool) -> bool {
    match env::var(key).ok().as_deref() {
        Some("true") | Some("1") | Some("yes") => true,
        Some("false") | Some("0") | Some("no") => false,
        _ => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unconfigured_when_missing() {
        std::env::remove_var("DAEMON_USER_ID");
        std::env::remove_var("DAEMON_API_KEY");
        assert!(!Config::from_env().is_configured());
    }

    #[test]
    fn dry_run_default_true() {
        std::env::remove_var("DAEMON_DRY_RUN");
        assert!(Config::from_env().dry_run);
    }
}
