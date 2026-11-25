use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::response::Response;
use chrono::{DateTime, Utc};
use segno_domain::{Video, VideoId, VideoSource, VideoStatus, library};
use serde::{Deserialize, Serialize};
use tower::ServiceExt;
use tower_http::services::ServeFile;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{ApiError, AppState, Problem};

/// A video in the library.
#[derive(Debug, Serialize, ToSchema)]
pub struct VideoDto {
    pub id: Uuid,
    pub source: VideoSourceDto,
    /// Original address, for downloaded videos.
    pub source_url: Option<String>,
    pub title: String,
    pub channel: Option<String>,
    pub duration_ms: Option<i64>,
    pub fps: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub status: VideoStatusDto,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Where a video came from.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VideoSourceDto {
    Youtube,
    File,
}

/// Lifecycle of a library entry.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VideoStatusDto {
    Importing,
    Ready,
    Failed,
}

impl From<Video> for VideoDto {
    fn from(video: Video) -> Self {
        Self {
            id: video.id.as_uuid(),
            source: match video.source {
                VideoSource::Youtube => VideoSourceDto::Youtube,
                VideoSource::File => VideoSourceDto::File,
            },
            source_url: video.source_url,
            title: video.title,
            channel: video.channel,
            duration_ms: video.duration_ms,
            fps: video.fps,
            width: video.width,
            height: video.height,
            status: match video.status {
                VideoStatus::Importing => VideoStatusDto::Importing,
                VideoStatus::Ready => VideoStatusDto::Ready,
                VideoStatus::Failed => VideoStatusDto::Failed,
            },
            created_at: video.created_at,
            updated_at: video.updated_at,
        }
    }
}

/// Lists the videos in the library, newest first.
#[utoipa::path(
    get,
    path = "/videos",
    tag = "videos",
    security(("token" = [])),
    responses(
        (status = 200, description = "Videos in the library", body = [VideoDto]),
        (status = 401, description = "Missing or invalid token", body = Problem)
    )
)]
pub async fn list_videos(State(state): State<AppState>) -> Result<Json<Vec<VideoDto>>, ApiError> {
    let videos = state.videos.list().await?;
    Ok(Json(videos.into_iter().map(VideoDto::from).collect()))
}

/// Gets one video.
#[utoipa::path(
    get,
    path = "/videos/{id}",
    tag = "videos",
    security(("token" = [])),
    params(("id" = Uuid, Path, description = "Video id")),
    responses(
        (status = 200, description = "The video", body = VideoDto),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 404, description = "No such video", body = Problem)
    )
)]
pub async fn get_video(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<VideoDto>, ApiError> {
    let video = state.videos.get(VideoId::from_uuid(id)).await?;
    Ok(Json(video.into()))
}

/// A video file on this computer to add to the library.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ImportVideoRequest {
    /// Absolute path of the file.
    pub path: String,
    /// Title in the library; the file name when omitted.
    pub title: Option<String>,
}

/// Imports a video file from this computer into the library, ready to play.
#[utoipa::path(
    post,
    path = "/videos/import",
    tag = "videos",
    security(("token" = [])),
    request_body = ImportVideoRequest,
    responses(
        (status = 201, description = "The imported video", body = VideoDto),
        (status = 400, description = "Not a file, or not a readable video", body = Problem),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 503, description = "ffmpeg is not available", body = Problem)
    )
)]
pub async fn import_video(
    State(state): State<AppState>,
    Json(request): Json<ImportVideoRequest>,
) -> Result<(StatusCode, Json<VideoDto>), ApiError> {
    let probe = state.probe.as_deref().ok_or_else(|| {
        ApiError::Unavailable("ffmpeg was not found, so videos cannot be imported".into())
    })?;
    let video = library::import_local_video(
        state.videos.as_ref(),
        probe,
        state.media_files.as_ref(),
        std::path::Path::new(&request.path),
        request.title.as_deref(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(video.into())))
}

/// The video's media file, for the player. Supports `Range` requests, so jumps load only what
/// they need. Media elements cannot send headers: pass the token as the `token` query parameter.
#[utoipa::path(
    get,
    path = "/videos/{id}/media",
    tag = "videos",
    security(("token" = [])),
    params(("id" = Uuid, Path, description = "Video id")),
    responses(
        (status = 200, description = "The whole media file"),
        (status = 206, description = "The byte range asked for with a Range header"),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 404, description = "No such video, or it has no media yet", body = Problem)
    )
)]
pub async fn get_media(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    request: Request,
) -> Result<Response, ApiError> {
    let video = state.videos.get(VideoId::from_uuid(id)).await?;
    let relative = video
        .media_file
        .ok_or_else(|| ApiError::NotFound(format!("video {id} has no media yet")))?;
    let file = state.media_files.resolve(&relative)?;
    // Serving a file cannot fail at this level (its error type is `Infallible`).
    let Ok(response) = ServeFile::new(file).oneshot(request).await;
    Ok(response.map(Body::new))
}
