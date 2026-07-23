//! 限流中间件。对应 `docs/ARCHITECTURE.md` §6.5（默认组按 IP 限流）。
//!
//! 基于 governor 的内存令牌桶，按 key（IP）限流。
//! MVP 不依赖 Redis（多实例部署时换 Redis 后端，接口不变）。

use crate::error::ApiError;
use axum::extract::{ConnectInfo, Request};
use axum::middleware::Next;
use axum::response::Response;
use governor::DefaultKeyedRateLimiter;
use governor::Quota;
use std::net::SocketAddr;
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

/// 默认限流中间件：按客户端 IP 限流（受信代理才读 XFF / X-Real-IP）。
pub async fn default_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    axum::Extension(limiters): axum::Extension<RateLimiters>,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let ip = sharpside_shared::client_ip::resolve_client_ip(
        Some(addr.ip()),
        req.headers()
            .get("x-real-ip")
            .and_then(|v| v.to_str().ok()),
        req.headers()
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok()),
    );
    limiters.check_default(&ip)?;
    Ok(next.run(req).await)
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
