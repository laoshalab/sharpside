//! 环境变量配置。

use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_addr: String,
    pub database_url: String,
    pub db_max_connections: u32,
    /// admin 鉴权 token（仅 dev 回退；生产走 OIDC，此项被忽略）。
    pub admin_token: String,
    /// 安全修复 3.3：OIDC 配置。任一关键字段缺失即视为未配置 OIDC。
    pub oidc: Option<OidcConfig>,
    /// 安全修复 3.3：admin session cookie 的 Secure 属性（生产 HTTPS 须 true）。
    pub cookie_secure: bool,
}

/// 安全修复 3.3：OIDC（Authorization Code Flow）配置。
#[derive(Debug, Clone)]
pub struct OidcConfig {
    /// Issuer URL（如 https://accounts.google.com）。将拼 `/.well-known/openid-configuration` 做 discovery。
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    /// 回调地址（如 https://admin.sharpside.example/api/auth/oidc/callback）。
    pub redirect_uri: String,
    /// 允许登录的 admin 邮箱白名单（OIDC 验身份，白名单决定谁能进后台）。
    pub allowed_emails: Vec<String>,
    /// admin session 签名密钥（HS256），生产须独立强密钥。
    pub session_secret: String,
    /// admin session 有效期（秒），默认 1 小时（短时）。
    pub session_ttl_seconds: i64,
}

impl Config {
    pub fn from_env() -> Self {
        let admin_token = sharpside_shared::secrets::assert_secret(
            "ADMIN_TOKEN",
            &env::var("ADMIN_TOKEN").unwrap_or_else(|_| "dev-admin-token".into()),
        )
        .to_string();

        let oidc = build_oidc();
        // 生产环境强制 OIDC：未配置 OIDC 直接 panic（fail-closed，杜绝共享 token 上生产）。
        if sharpside_shared::secrets::is_production() && oidc.is_none() {
            panic!(
                "生产环境必须配置 OIDC（OIDC_ISSUER / OIDC_CLIENT_ID / OIDC_CLIENT_SECRET / \
                 OIDC_REDIRECT_URI / OIDC_ALLOWED_EMAILS / ADMIN_SESSION_SECRET），\
                 共享 ADMIN_TOKEN 不允许上生产"
            );
        }

        Self {
            listen_addr: env::var("ADMIN_LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8086".into()),
            database_url: env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://sharpside:sharpside@127.0.0.1:5432/sharpside".into()
            }),
            db_max_connections: env::var("DB_MAX_CONNECTIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            admin_token,
            oidc,
            cookie_secure: match env::var("COOKIE_SECURE").ok().as_deref() {
                Some("1") | Some("true") => true,
                Some("0") | Some("false") => false,
                _ => sharpside_shared::secrets::is_production(),
            },
        }
    }
}

fn build_oidc() -> Option<OidcConfig> {
    let issuer = env::var("OIDC_ISSUER").ok().filter(|s| !s.trim().is_empty())?;
    let client_id = env::var("OIDC_CLIENT_ID").ok().filter(|s| !s.trim().is_empty())?;
    let client_secret =
        env::var("OIDC_CLIENT_SECRET").ok().filter(|s| !s.trim().is_empty())?;
    let redirect_uri =
        env::var("OIDC_REDIRECT_URI").ok().filter(|s| !s.trim().is_empty())?;
    let mut session_secret =
        env::var("ADMIN_SESSION_SECRET").ok().filter(|s| !s.trim().is_empty())?;
    // 安全修复 3.5：生产环境强制强 session 密钥。
    session_secret = sharpside_shared::secrets::assert_secret(
        "ADMIN_SESSION_SECRET",
        &session_secret,
    )
    .to_string();
    let allowed_emails: Vec<String> = env::var("OIDC_ALLOWED_EMAILS")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let session_ttl_seconds = env::var("ADMIN_SESSION_TTL_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3_600);
    Some(OidcConfig {
        issuer,
        client_id,
        client_secret,
        redirect_uri,
        allowed_emails,
        session_secret,
        session_ttl_seconds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        std::env::remove_var("ADMIN_LISTEN_ADDR");
        std::env::remove_var("OIDC_ISSUER");
        let c = Config::from_env();
        assert_eq!(c.listen_addr, "0.0.0.0:8086");
        assert!(c.oidc.is_none(), "未配 OIDC_ISSUER 时 oidc=None");
        assert!(!c.cookie_secure, "非生产默认 cookie_secure=false");
    }
}
