//! 会话 cookie 辅助（安全修复 3.1）。
//!
//! JWT 迁 HttpOnly cookie：浏览器端 JS 不可读 → XSS 无法窃 token。
//! 本模块提供 cookie 名 + `Set-Cookie` 值构造 + 从 `Cookie` 头解析 token，纯字符串无 axum 依赖，
//! 供 account（签发/登出）与 gateway（dev token）共用，提取侧由各服务 `AuthUser` 调用。
//!
//! `Secure` 属性按部署环境条件附加：生产（HTTPS）须置 true，本地 HTTP 开发置 false
//! （否则浏览器拒收 cookie）。由调用方按 `is_production()` 或 `COOKIE_SECURE` 决定。

/// 存放 JWT 的 cookie 名。
pub const AUTH_COOKIE: &str = "sharpside_token";

/// 构造登录 `Set-Cookie` 值：`sharpside_token=<jwt>; HttpOnly; SameSite=Lax; Path=/; Max-Age=<ttl>[; Secure]`。
pub fn build_set_cookie(token: &str, ttl_seconds: i64, secure: bool) -> String {
    format!(
        "{AUTH_COOKIE}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={ttl_seconds}{secure_attr}",
        secure_attr = if secure { "; Secure" } else { "" }
    )
}

/// 构造登出 `Set-Cookie` 值：清空 + Max-Age=0 立即删除。
pub fn clear_set_cookie(secure: bool) -> String {
    format!(
        "{AUTH_COOKIE}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0{secure_attr}",
        secure_attr = if secure { "; Secure" } else { "" }
    )
}

/// 从 `Cookie` 请求头值中解析 `sharpside_token`。命中且非空返回 token，否则 None。
pub fn extract_token_from_cookie_header(cookie_header: &str) -> Option<String> {
    for pair in cookie_header.split(';') {
        let pair = pair.trim();
        if let Some(rest) = pair.strip_prefix(AUTH_COOKIE) {
            if let Some(val) = rest.strip_prefix('=') {
                let v = val.trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_cookie_secure_flag() {
        assert!(build_set_cookie("t", 1800, true).contains("; Secure"));
        assert!(!build_set_cookie("t", 1800, false).contains("; Secure"));
        assert!(build_set_cookie("t", 1800, true).contains("HttpOnly"));
        assert!(build_set_cookie("t", 1800, true).contains("SameSite=Lax"));
    }

    #[test]
    fn extract_token_from_various_cookie_headers() {
        assert_eq!(
            extract_token_from_cookie_header("sharpside_token=abc.def.ghi"),
            Some("abc.def.ghi".into())
        );
        assert_eq!(
            extract_token_from_cookie_header("theme=dark; sharpside_token=xyz; other=1"),
            Some("xyz".into())
        );
        assert_eq!(extract_token_from_cookie_header("theme=dark"), None);
        assert_eq!(extract_token_from_cookie_header("sharpside_token="), None);
        assert_eq!(extract_token_from_cookie_header(""), None);
    }
}
