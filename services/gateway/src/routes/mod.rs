//! 路由构建。对应 `docs/ARCHITECTURE.md` §6.5。

pub mod bff;
pub mod dev;
pub mod health;
pub mod proxy;

use crate::rate_limit::RateLimiters;
use crate::state::AppState;
use axum::middleware::from_fn;
use axum::routing::{any, get};
use axum::Router;

/// 构建完整路由树。
pub fn router(state: AppState, limiters: RateLimiters) -> Router {
    // daemon 长轮询路由：单独限流组
    let daemon_route = Router::new()
        .route("/me/copy-orders", get(bff::copy_orders))
        .layer(from_fn(crate::rate_limit::daemon_middleware));

    let mut router: Router<AppState> = Router::new()
        // 健康检查（无需鉴权）
        .route("/health", get(health::live))
        .route("/ready", get(health::ready));
    // 开发辅助：签发 JWT（生产 / release 默认禁用，见 config.dev_endpoints_enabled）
    if state.config.dev_endpoints_enabled {
        router = router.route("/dev/token", get(dev::issue_dev_token));
    }
    router = router
        // BFF（JWT 鉴权，由 AuthUser extractor 强制）
        .route("/me/dashboard", get(bff::dashboard))
        // BFF 同路径 `/api/` 前缀别名：web 前端统一走 `/api/*` 反代（见 apps/web/src/main.rs），
        // 无此别名则 `/me/dashboard` 不在 web 反代范围内。对应 `docs/FRONTEND_DESIGN.md` §6.6。
        .route("/api/me/dashboard", get(bff::dashboard))
        // daemon 长轮询（daemon_api_key 鉴权 + 单独限流）
        .merge(daemon_route)
        // 反向代理到上游（透传，鉴权由上游负责）
        // axum 0.7 catch-all 语法为 `/*path`（`/{*path}` 是 0.8 语法，会在启动时 panic）。
        .route("/api/venue-hub/*path", any(proxy::to_venue_hub))
        .route("/api/follow/*path", any(proxy::to_follow))
        .route("/api/copier/*path", any(proxy::to_copier))
        .route("/api/account/*path", any(proxy::to_account));
    // with_state 将 Router<AppState> 转为 Router<()>，故在所有路由注册完成后统一调用。
    router
        .with_state(state)
        // 全局限流（默认组，按 IP）
        .layer(from_fn(crate::rate_limit::default_middleware))
        // 限流器作为扩展状态注入
        .layer(axum::Extension(limiters))
}
