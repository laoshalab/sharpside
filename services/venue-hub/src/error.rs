//! API 错误类型。对应 `docs/ARCHITECTURE.md` §6.1 对外 API 错误约定。
//!
//! 统一转 HTTP 响应，避免上游（gateway）需要理解 db/venue 的内部错误。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// VenueHub API 错误。
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum ApiError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("venue unsupported: {0}")]
    Unsupported(String),
    #[error("db: {0}")]
    Db(#[from] sharpside_db::DbError),
    #[error("venue: {0}")]
    Venue(#[from] sharpside_venues_core::VenueError),
    #[error("internal: {0}")]
    Internal(String),
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden(_) => StatusCode::FORBIDDEN,
            ApiError::Unsupported(_) => StatusCode::NOT_IMPLEMENTED,
            ApiError::Db(_) | ApiError::Venue(_) | ApiError::Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = Json(json!({
            "error": self.to_string(),
            "kind": match &self {
                ApiError::NotFound(_) => "not_found",
                ApiError::BadRequest(_) => "bad_request",
                ApiError::Unauthorized(_) => "unauthorized",
                ApiError::Forbidden(_) => "forbidden",
                ApiError::Unsupported(_) => "unsupported",
                ApiError::Db(_) => "db",
                ApiError::Venue(_) => "venue",
                ApiError::Internal(_) => "internal",
            },
        }));
        (status, body).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError::Internal(e.to_string())
    }
}
