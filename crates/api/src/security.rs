use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, HOST};
use axum::http::{HeaderValue, Method};
use axum::middleware::Next;
use axum::response::Response;
use subtle::ConstantTimeEq;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::ApiError;

/// How a served API protects itself.
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Secret that every protected request must present, either as
    /// `Authorization: Bearer <token>` or, for media elements that cannot send
    /// headers, as a `token` query parameter.
    pub token: String,
    /// Accepted values of the `Host` header (protection against DNS rebinding).
    /// Empty means any host, e.g. behind a trusted reverse proxy.
    pub allowed_hosts: Vec<String>,
    /// Origins allowed by CORS: the webview of the app and, in development, the Vite server.
    pub allowed_origins: Vec<String>,
}

impl SecurityConfig {
    /// Settings for an API listening on `127.0.0.1:port` for the given webview origins.
    pub fn local(token: impl Into<String>, port: u16, origins: &[&str]) -> Self {
        Self {
            token: token.into(),
            allowed_hosts: vec![format!("127.0.0.1:{port}"), format!("localhost:{port}")],
            allowed_origins: origins.iter().map(|origin| (*origin).to_owned()).collect(),
        }
    }

    pub(crate) fn cors_layer(&self) -> CorsLayer {
        let origins = self
            .allowed_origins
            .iter()
            .filter_map(|origin| HeaderValue::from_str(origin).ok());
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::PATCH,
                Method::DELETE,
            ])
            .allow_headers([AUTHORIZATION, CONTENT_TYPE])
            .max_age(Duration::from_secs(600))
    }

    fn accepts_token(&self, presented: Option<&str>) -> bool {
        presented.is_some_and(|token| bool::from(token.as_bytes().ct_eq(self.token.as_bytes())))
    }

    fn accepts_host(&self, host: Option<&str>) -> bool {
        self.allowed_hosts.is_empty()
            || host.is_some_and(|host| {
                self.allowed_hosts
                    .iter()
                    .any(|allowed| allowed.eq_ignore_ascii_case(host))
            })
    }
}

pub(crate) async fn require_token(
    State(security): State<Arc<SecurityConfig>>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let presented = bearer_token(&request).or_else(|| query_token(&request));
    if security.accepts_token(presented) {
        Ok(next.run(request).await)
    } else {
        Err(ApiError::Unauthorized)
    }
}

pub(crate) async fn check_host(
    State(security): State<Arc<SecurityConfig>>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let host = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok());
    if security.accepts_host(host) {
        Ok(next.run(request).await)
    } else {
        Err(ApiError::Forbidden("unexpected Host header".into()))
    }
}

fn bearer_token(request: &Request) -> Option<&str> {
    request
        .headers()
        .get(AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

fn query_token(request: &Request) -> Option<&str> {
    request
        .uri()
        .query()?
        .split('&')
        .find_map(|pair| pair.strip_prefix("token="))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> SecurityConfig {
        SecurityConfig::local("secret", 7700, &["http://tauri.localhost"])
    }

    #[test]
    fn only_the_exact_token_is_accepted() {
        let config = config();
        assert!(config.accepts_token(Some("secret")));
        assert!(!config.accepts_token(Some("secret2")));
        assert!(!config.accepts_token(Some("")));
        assert!(!config.accepts_token(None));
    }

    #[test]
    fn only_loopback_hosts_on_the_right_port_are_accepted() {
        let config = config();
        assert!(config.accepts_host(Some("127.0.0.1:7700")));
        assert!(config.accepts_host(Some("LOCALHOST:7700")));
        assert!(!config.accepts_host(Some("127.0.0.1:7701")));
        assert!(!config.accepts_host(Some("evil.example:7700")));
        assert!(!config.accepts_host(None));
    }
}
