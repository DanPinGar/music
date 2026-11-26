use axum::Json;
use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;

use crate::{AppState, Problem, SystemInfo};

/// Result of the liveness probe.
#[derive(Debug, Serialize, ToSchema)]
pub struct Health {
    /// Always `ok` while the API is serving.
    pub status: String,
}

/// Liveness probe. Needs no token.
#[utoipa::path(
    get,
    path = "/health",
    tag = "system",
    responses((status = 200, description = "The API is serving", body = Health))
)]
pub async fn health() -> Json<Health> {
    Json(Health {
        status: "ok".into(),
    })
}

/// Version and storage details of the running core.
#[utoipa::path(
    get,
    path = "/system",
    tag = "system",
    security(("token" = [])),
    responses(
        (status = 200, description = "Facts about the core", body = SystemInfo),
        (status = 401, description = "Missing or invalid token", body = Problem)
    )
)]
pub async fn get_system(State(state): State<AppState>) -> Json<SystemInfo> {
    let versions = state.services.get().versions;
    Json(SystemInfo {
        ffmpeg_version: versions.ffmpeg,
        ytdlp_version: versions.ytdlp,
        ..state.system.as_ref().clone()
    })
}
