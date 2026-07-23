//! 路由构建。对应 `docs/ARCHITECTURE.md` §6.5。

pub mod bff;
pub mod dev;
pub mod health;
pub mod proxy;

use crate::rate_limit::RateLimiters;
use crate::state::AppState;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::{from_fn, Next};
use axum::response::Response;
use axum::routing::{any, get};
use axum::Router;

/// 拦截上游 `/internal/*` 端点经 gateway 反代暴露。返回 404 而非 403，避免泄露端点存在。
async fn block_internal() -> StatusCode {
    StatusCode::NOT_FOUND
}

/// 安全响应头：对所有响应注入浏览器防护头，降低 XSS / 点击劫持 / MIME 嗅探风险。
/// HSTS 由 TLS 终止层（ingress/load balancer）负责，此处不设以免破坏 dev HTTP。
async fn security_headers(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let headers = res.headers_mut();
    headers.insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        "x-frame-options",
        HeaderValue::from_static("DENY"),
    );
    headers.insert(
        axum::http::header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    res
}

/// 构建完整路由树。
pub fn router(state: AppState, limiters: RateLimiters) -> Router {
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
        // 安全：屏蔽上游 `/internal/*` 端点经公网入口暴露。内部信号路径（如 follow 的
        // `/internal/signals`）只能走服务间网络，绝不可经 gateway 反代到达公网。
        .route("/api/venue-hub/internal/*path", any(block_internal))
        .route("/api/follow/internal/*path", any(block_internal))
        .route("/api/copier/internal/*path", any(block_internal))
        .route("/api/account/internal/*path", any(block_internal))
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
        // 安全响应头（对所有响应注入 nosniff / DENY / no-referrer）
        .layer(from_fn(security_headers))
        // 限流器作为扩展状态注入
        .layer(axum::Extension(limiters))
}
