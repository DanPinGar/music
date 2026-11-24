//! HTTP API of the Segno core.
//!
//! The desktop app serves it on `127.0.0.1` for its own webview; the future cloud
//! server will serve the same router. Every route except `/health` requires the
//! session token (see [`SecurityConfig`]).

mod error;
mod routes;
mod security;

use std::sync::Arc;

use axum::Router;
use axum::middleware;
use segno_domain::VideoRepository;
use serde::Serialize;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use utoipa::openapi::security::{Http, HttpAuthScheme, SecurityScheme};
use utoipa::{Modify, OpenApi, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub use error::{ApiError, Problem};
pub use security::SecurityConfig;

/// Shared state for the handlers.
#[derive(Clone)]
pub struct AppState {
    pub videos: Arc<dyn VideoRepository>,
    pub system: Arc<SystemInfo>,
}

/// Facts about the running core, shown in the UI's status bar.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SystemInfo {
    /// Version of the core.
    pub version: String,
    /// Folder holding the library database and media, when served locally.
    pub library_path: Option<String>,
    /// Latest database migration applied.
    pub schema_version: i64,
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Segno API",
        description = "Core API of Segno: library, navigation data, markers, comments and analysis jobs."
    ),
    modifiers(&TokenScheme),
    tags(
        (name = "system", description = "Health and version of the core"),
        (name = "videos", description = "Videos in the library")
    )
)]
struct ApiDoc;

struct TokenScheme;

impl Modify for TokenScheme {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "token",
            SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
        );
    }
}

fn public_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(routes::system::health))
}

fn protected_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(routes::system::get_system))
        .routes(routes!(routes::videos::list_videos))
        .routes(routes!(routes::videos::get_video))
}

/// The OpenAPI document of the API (source of the generated TypeScript client).
pub fn openapi() -> utoipa::openapi::OpenApi {
    let (_, mut api) = OpenApiRouter::<AppState>::with_openapi(ApiDoc::openapi())
        .merge(public_routes())
        .merge(protected_routes())
        .split_for_parts();
    // utoipa copies the (unset) Cargo licence; the project has not chosen one yet.
    api.info.license = None;
    api
}

/// Builds the API router with authentication, Host checks, CORS and request tracing.
pub fn router(state: AppState, security: SecurityConfig) -> Router {
    let security = Arc::new(security);
    let (public, _) = public_routes().split_for_parts();
    let (protected, _) = protected_routes().split_for_parts();
    let protected = protected.route_layer(middleware::from_fn_with_state(
        security.clone(),
        security::require_token,
    ));

    public
        .merge(protected)
        .layer(middleware::from_fn_with_state(
            security.clone(),
            security::check_host,
        ))
        .layer(security.cors_layer())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Serves the API on an already bound listener until the process stops.
pub async fn serve(
    listener: TcpListener,
    state: AppState,
    security: SecurityConfig,
) -> std::io::Result<()> {
    axum::serve(listener, router(state, security)).await
}
