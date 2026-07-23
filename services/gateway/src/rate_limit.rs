//! 限流中间件。对应 `docs/ARCHITECTURE.md` §6.5（默认组按 IP 限流）。
//!
//! 基于 governor 的内存令牌桶，按 key（IP）限流。
//! MVP 不依赖 Redis（多实例部署时换 Redis 后端，接口不变）。

use crate::error::ApiError;
use axum::extract::Request;
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;
use governor::DefaultKeyedRateLimiter;
use governor::Quota;
use std::num::NonZeroU32;
use std::sync::Arc;

/// 限流器类型：按 String key（IP）分桶。
pub type Limiter = DefaultKeyedRateLimiter<String>;

/// 限流组：默认组。
#[derive(Clone)]
pub struct RateLimiters {
    pub default: Arc<Limiter>,
}

impl RateLimiters {
    /// 按 rps 配置创建限流器。
    pub fn new(default_rps: u32) -> Self {
        Self {
            default: Arc::new(make_limiter(default_rps)),
        }
    }

    /// 对默认组检查。key 通常是客户端 IP。
    pub fn check_default(&self, key: &str) -> Result<(), ApiError> {
        self.default
            .check_key(&key.to_string())
            .map(|_| ())
            .map_err(|_| ApiError::RateLimited)
    }
}

fn make_limiter(rps: u32) -> Limiter {
    let quota = Quota::per_second(NonZeroU32::new(rps.max(1)).unwrap());
    Limiter::keyed(quota)
}

/// 默认限流中间件：按客户端 IP（X-Forwarded-For 首段，缺失用 "unknown"）限流。
pub async fn default_middleware(
    axum::Extension(limiters): axum::Extension<RateLimiters>,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let ip = client_ip(req.headers());
    limiters.check_default(&ip)?;
    Ok(next.run(req).await)
}

/// 从 X-Forwarded-For 取首段 IP；缺失返回 "unknown"。
fn client_ip(headers: &HeaderMap) -> String {
    headers
        .get("X-Forwarded-For")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_under_limit() {
        let l = RateLimiters::new(10);
        assert!(l.check_default("ip1").is_ok());
    }

    #[test]
    fn separate_keys_independent() {
        let l = RateLimiters::new(2);
        // 不同 key 互不影响
        assert!(l.check_default("ip1").is_ok());
        assert!(l.check_default("ip2").is_ok());
        assert!(l.check_default("ip3").is_ok());
    }
}
