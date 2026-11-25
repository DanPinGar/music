use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::response::Response;
use chrono::{DateTime, Utc};
use segno_domain::library::{self, PreparationStage};
use segno_domain::{ThumbnailSprite, Video, VideoId, VideoSource, VideoStatus};
use serde::{Deserialize, Serialize};
use tower::ServiceExt;
use tower_http::services::ServeFile;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::preparations::Progress;
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
    /// Whether `/media` already serves the copy made for instant seeking.
    pub seek_proxy: bool,
    /// Thumbnail sprite served at `/thumbnails`, once made.
    pub thumbnails: Option<ThumbnailsDto>,
    /// Whether `/peaks` serves the audio waveform.
    pub has_peaks: bool,
    /// Present while the video is being prepared for navigation.
    pub preparation: Option<PreparationDto>,
    /// Why the preparation failed, if it did.
    pub preparation_error: Option<String>,
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

/// Layout of the thumbnail sprite: tile `n` shows the video at `n * interval_ms`, at column
/// `n % columns` and row `n / columns`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ThumbnailsDto {
    pub columns: u32,
    pub rows: u32,
    pub tile_width: u32,
    pub tile_height: u32,
    pub interval_ms: u32,
    pub count: u32,
}

/// Progress of preparing a video for navigation.
#[derive(Debug, Serialize, ToSchema)]
pub struct PreparationDto {
    pub stage: PreparationStageDto,
    /// The whole preparation, from 0 to 1.
    pub progress: f64,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PreparationStageDto {
    Queued,
    SeekProxy,
    Thumbnails,
    Peaks,
}

impl VideoDto {
    fn new(video: Video, progress: Option<Progress>) -> Self {
        let thumbnails = match (
            &video.thumbnails_file,
            video.duration_ms,
            video.width,
            video.height,
        ) {
            (Some(_), Some(duration), Some(width), Some(height)) => {
                let sprite = ThumbnailSprite::for_video(duration, width, height);
                Some(ThumbnailsDto {
                    columns: sprite.columns,
                    rows: sprite.rows,
                    tile_width: sprite.tile_width,
                    tile_height: sprite.tile_height,
                    interval_ms: sprite.interval_ms,
                    count: sprite.count,
                })
            }
            _ => None,
        };
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
            seek_proxy: video.proxy_file.is_some(),
            thumbnails,
            has_peaks: video.peaks_file.is_some(),
            preparation: progress.map(|progress| PreparationDto {
                stage: match progress.stage {
                    None => PreparationStageDto::Queued,
                    Some(PreparationStage::SeekProxy) => PreparationStageDto::SeekProxy,
                    Some(PreparationStage::Thumbnails) => PreparationStageDto::Thumbnails,
                    Some(PreparationStage::Peaks) => PreparationStageDto::Peaks,
                },
                progress: progress.done,
            }),
            preparation_error: video.preparation_error,
            created_at: video.created_at,
            updated_at: video.updated_at,
        }
    }
}

impl AppState {
    fn dto(&self, video: Video) -> VideoDto {
        let progress = self
            .preparations
            .as_ref()
            .and_then(|preparations| preparations.progress(video.id));
        VideoDto::new(video, progress)
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
    Ok(Json(
        videos.into_iter().map(|video| state.dto(video)).collect(),
    ))
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
    Ok(Json(state.dto(video)))
}

/// A video file on this computer to add to the library.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ImportVideoRequest {
    /// Absolute path of the file.
    pub path: String,
    /// Title in the library; the file name when omitted.
    pub title: Option<String>,
}

/// Imports a video file from this computer into the library, ready to play, and queues its
/// preparation for instant navigation.
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
    if let Some(preparations) = &state.preparations {
        preparations.enqueue(video.id);
    }
    Ok((StatusCode::CREATED, Json(state.dto(video))))
}

/// The video for the player: the copy made for instant seeking once it exists, the original
/// until then. Supports `Range` requests. Media elements cannot send headers: pass the token
/// as the `token` query parameter.
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
    let file = video.proxy_file.or(video.media_file);
    serve(&state, file, "media", id, request).await
}

/// The thumbnail sprite (JPEG); its layout is in the video's `thumbnails`.
#[utoipa::path(
    get,
    path = "/videos/{id}/thumbnails",
    tag = "videos",
    security(("token" = [])),
    params(("id" = Uuid, Path, description = "Video id")),
    responses(
        (status = 200, description = "The sprite, as a JPEG image"),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 404, description = "No such video, or no thumbnails yet", body = Problem)
    )
)]
pub async fn get_thumbnails(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    request: Request,
) -> Result<Response, ApiError> {
    let video = state.videos.get(VideoId::from_uuid(id)).await?;
    serve(&state, video.thumbnails_file, "thumbnails", id, request).await
}

/// The audio waveform: `{"version": 1, "peaks_per_second": 100, "peaks": [min, max, …]}` with
/// values from -127 to 127.
#[utoipa::path(
    get,
    path = "/videos/{id}/peaks",
    tag = "videos",
    security(("token" = [])),
    params(("id" = Uuid, Path, description = "Video id")),
    responses(
        (status = 200, description = "The waveform peaks, as JSON"),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 404, description = "No such video, or no waveform yet", body = Problem)
    )
)]
pub async fn get_peaks(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    request: Request,
) -> Result<Response, ApiError> {
    let video = state.videos.get(VideoId::from_uuid(id)).await?;
    serve(&state, video.peaks_file, "waveform", id, request).await
}

async fn serve(
    state: &AppState,
    relative: Option<String>,
    what: &str,
    id: Uuid,
    request: Request,
) -> Result<Response, ApiError> {
    let relative =
        relative.ok_or_else(|| ApiError::NotFound(format!("video {id} has no {what} yet")))?;
    let file = state.media_files.resolve(&relative)?;
    // Serving a file cannot fail at this level (its error type is `Infallible`).
    let Ok(response) = ServeFile::new(file).oneshot(request).await;
    Ok(response.map(Body::new))
}
