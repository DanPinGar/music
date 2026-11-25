use axum::Json;
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use segno_domain::DomainError;
use serde::Serialize;
use utoipa::ToSchema;

/// Errors returned by the handlers, rendered as RFC 9457 problem details.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("missing or invalid session token")]
    Unauthorized,

    #[error("{0}")]
    Forbidden(String),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    BadRequest(String),

    /// A tool the request needs is missing (e.g. ffmpeg).
    #[error("{0}")]
    Unavailable(String),

    #[error("internal error")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// RFC 9457 problem details (`application/problem+json`).
#[derive(Debug, Serialize, ToSchema)]
pub struct Problem {
    /// URI identifying the problem type; `about:blank` when the status code says it all.
    #[serde(rename = "type")]
    pub problem_type: String,
    /// Short summary of the problem type.
    pub title: String,
    /// HTTP status code.
    pub status: u16,
    /// Explanation specific to this occurrence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<DomainError> for ApiError {
    fn from(err: DomainError) -> Self {
        match err {
            DomainError::NotFound { .. } => Self::NotFound(err.to_string()),
            DomainError::Invalid(message) => Self::BadRequest(message),
            DomainError::Unavailable(message) => Self::Unavailable(message),
            DomainError::Storage(source) => Self::Internal(source),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let detail = match &self {
            Self::Internal(source) => {
                tracing::error!(error = %source, "request failed");
                None
            }
            other => Some(other.to_string()),
        };
        let problem = Problem {
            problem_type: "about:blank".into(),
            title: status.canonical_reason().unwrap_or("Error").into(),
            status: status.as_u16(),
            detail,
        };
        (
            status,
            [(CONTENT_TYPE, "application/problem+json")],
            Json(problem),
        )
            .into_response()
    }
}
