use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use segno_domain::{Marker, MarkerFields, MarkerId, MarkerKind, VideoId};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{ApiError, AppState, Problem};

/// A marker on a video's timeline.
#[derive(Debug, Serialize, ToSchema)]
pub struct MarkerDto {
    pub id: Uuid,
    pub kind: MarkerKindDto,
    pub start_ms: i64,
    /// End of a section or loop; absent for points.
    pub end_ms: Option<i64>,
    /// May be empty: the UI then shows the marker's number.
    pub label: String,
    /// `#rrggbb`; absent for the default colour.
    pub color: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A point to start from, a named section or a saved loop.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarkerKindDto {
    Point,
    Section,
    Loop,
}

/// A new marker.
#[derive(Debug, Deserialize, ToSchema)]
pub struct NewMarkerDto {
    pub kind: MarkerKindDto,
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    #[serde(default)]
    pub label: String,
    pub color: Option<String>,
}

/// Every editable field of a marker (its kind cannot change).
#[derive(Debug, Deserialize, ToSchema)]
pub struct MarkerChangesDto {
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    #[serde(default)]
    pub label: String,
    pub color: Option<String>,
}

impl From<Marker> for MarkerDto {
    fn from(marker: Marker) -> Self {
        Self {
            id: marker.id.as_uuid(),
            kind: match marker.kind {
                MarkerKind::Point => MarkerKindDto::Point,
                MarkerKind::Section => MarkerKindDto::Section,
                MarkerKind::Loop => MarkerKindDto::Loop,
            },
            start_ms: marker.start_ms,
            end_ms: marker.end_ms,
            label: marker.label,
            color: marker.color,
            created_at: marker.created_at,
            updated_at: marker.updated_at,
        }
    }
}

fn kind(dto: MarkerKindDto) -> MarkerKind {
    match dto {
        MarkerKindDto::Point => MarkerKind::Point,
        MarkerKindDto::Section => MarkerKind::Section,
        MarkerKindDto::Loop => MarkerKind::Loop,
    }
}

/// The video's markers, in time order.
#[utoipa::path(
    get,
    path = "/videos/{id}/markers",
    tag = "study",
    security(("token" = [])),
    params(("id" = Uuid, Path, description = "Video id")),
    responses(
        (status = 200, description = "The markers", body = [MarkerDto]),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 404, description = "No such video", body = Problem)
    )
)]
pub async fn list_markers(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<MarkerDto>>, ApiError> {
    let video = state.videos.get(VideoId::from_uuid(id)).await?;
    let markers = state.markers.list_markers(video.id).await?;
    Ok(Json(markers.into_iter().map(MarkerDto::from).collect()))
}

/// Adds a marker to the video.
#[utoipa::path(
    post,
    path = "/videos/{id}/markers",
    tag = "study",
    security(("token" = [])),
    params(("id" = Uuid, Path, description = "Video id")),
    request_body = NewMarkerDto,
    responses(
        (status = 201, description = "The new marker", body = MarkerDto),
        (status = 400, description = "Not a valid marker", body = Problem),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 404, description = "No such video", body = Problem)
    )
)]
pub async fn create_marker(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<NewMarkerDto>,
) -> Result<(StatusCode, Json<MarkerDto>), ApiError> {
    let video = state.videos.get(VideoId::from_uuid(id)).await?;
    let marker = Marker::new(
        video.id,
        kind(body.kind),
        MarkerFields {
            start_ms: body.start_ms,
            end_ms: body.end_ms,
            label: body.label,
            color: body.color,
        },
    )?;
    state.markers.insert_marker(&marker).await?;
    Ok((StatusCode::CREATED, Json(marker.into())))
}

/// Moves, renames or recolours a marker.
#[utoipa::path(
    put,
    path = "/videos/{id}/markers/{marker_id}",
    tag = "study",
    security(("token" = [])),
    params(
        ("id" = Uuid, Path, description = "Video id"),
        ("marker_id" = Uuid, Path, description = "Marker id")
    ),
    request_body = MarkerChangesDto,
    responses(
        (status = 200, description = "The marker as saved", body = MarkerDto),
        (status = 400, description = "Not a valid marker", body = Problem),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 404, description = "No such video or marker", body = Problem)
    )
)]
pub async fn update_marker(
    State(state): State<AppState>,
    Path((id, marker_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<MarkerChangesDto>,
) -> Result<Json<MarkerDto>, ApiError> {
    let video = VideoId::from_uuid(id);
    let mut marker = state
        .markers
        .get_marker(video, MarkerId::from_uuid(marker_id))
        .await?;
    marker.change(MarkerFields {
        start_ms: body.start_ms,
        end_ms: body.end_ms,
        label: body.label,
        color: body.color,
    })?;
    state.markers.update_marker(&marker).await?;
    Ok(Json(marker.into()))
}

/// Deletes a marker.
#[utoipa::path(
    delete,
    path = "/videos/{id}/markers/{marker_id}",
    tag = "study",
    security(("token" = [])),
    params(
        ("id" = Uuid, Path, description = "Video id"),
        ("marker_id" = Uuid, Path, description = "Marker id")
    ),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 404, description = "No such video or marker", body = Problem)
    )
)]
pub async fn delete_marker(
    State(state): State<AppState>,
    Path((id, marker_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    state
        .markers
        .delete_marker(VideoId::from_uuid(id), MarkerId::from_uuid(marker_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
