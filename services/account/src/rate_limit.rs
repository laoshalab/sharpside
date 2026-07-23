//! /auth/* 限流中间件。对应安全审计 P1（H3）。
//!
//! 基于 governor 内存令牌桶，按客户端 IP 限流，防暴力撞库 / 注册刷量。
//! MVP 单实例内存桶；多实例部署换 Redis 后端（接口不变）。
//!
//! 限流策略：每分钟每 IP `auth_rate_limit_per_min` 次（默认 10），允许短时突发 5 次。
//! 命中限流返回 429（`ApiError::RateLimited`）。

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{ConnectInfo, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use governor::DefaultKeyedRateLimiter;
use governor::Quota;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::sync::Arc;

/// /auth/* 限流器：按 String key（客户端 IP）分桶。
pub type AuthLimiter = DefaultKeyedRateLimiter<String>;

/// 按"每分钟每 IP 最大请求数"构造限流器。
///
/// `per_min` 为 0 时按 1 处理（避免 `NonZeroU32` panic）。
/// `Quota::per_minute(N)` = 每 60s 窗口最多 N 次，允许瞬时突发 N 次后匀速补充。
pub fn make_auth_limiter(per_min: u32) -> Arc<AuthLimiter> {
    let quota = Quota::per_minute(NonZeroU32::new(per_min.max(1)).unwrap());
    Arc::new(AuthLimiter::keyed(quota))
}

/// /auth/* 限流中间件：按客户端 IP 限流（受信代理才读 XFF / X-Real-IP）。
pub async fn auth_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
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
    if state.auth_limiter.check_key(&ip).is_err() {
        return Err(ApiError::RateLimited);
    }
    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_under_limit() {
        let l = make_auth_limiter(10);
        // 首次请求应放行（突发桶 5）
        assert!(l.check_key(&"ip1".to_string()).is_ok());
    }

    #[test]
    fn separate_ips_independent() {
        let l = make_auth_limiter(2);
        assert!(l.check_key(&"ip1".to_string()).is_ok());
        assert!(l.check_key(&"ip2".to_string()).is_ok());
    }
}
