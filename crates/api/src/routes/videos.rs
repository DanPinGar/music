use axum::Json;
use axum::extract::{Path, State};
use chrono::{DateTime, Utc};
use segno_domain::{Video, VideoId, VideoSource, VideoStatus};
use serde::Serialize;
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
