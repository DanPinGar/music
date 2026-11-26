use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::response::Response;
use std::collections::HashMap;

use chrono::{DateTime, Utc};
use segno_domain::download::{self, DownloadStage};
use segno_domain::library::{self, PreparationStage};
use segno_domain::{
    Studied, ThumbnailSprite, Video, VideoChanges, VideoId, VideoSource, VideoStatus,
};
use serde::{Deserialize, Serialize};
use tower::ServiceExt;
use tower_http::services::ServeFile;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::preparations::Progress;
use crate::{ApiError, AppState, DownloadProgress, Problem};

/// A video in the library.
#[derive(Debug, Serialize, ToSchema)]
pub struct VideoDto {
    pub id: Uuid,
    pub source: VideoSourceDto,
    /// Original address, for downloaded videos.
    pub source_url: Option<String>,
    /// The song's title.
    pub title: String,
    /// Who wrote or plays the song.
    pub artist: Option<String>,
    /// The year the song (or its album) was published.
    pub year: Option<i32>,
    /// The YouTube channel the video comes from.
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
    /// Whether `/attacks` serves the attacks (note onsets) found in the audio.
    pub has_attacks: bool,
    /// Present while the video is being prepared for navigation.
    pub preparation: Option<PreparationDto>,
    /// Why the preparation failed, if it did.
    pub preparation_error: Option<String>,
    /// Present while the video is being downloaded from YouTube.
    pub download: Option<DownloadDto>,
    /// Why the download or import failed (status `failed`), if it did.
    pub import_error: Option<String>,
    pub tags: Vec<String>,
    /// Where the user left the video, if they have opened it.
    pub position_ms: Option<i64>,
    /// When the user last studied the video.
    pub studied_at: Option<DateTime<Utc>>,
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

/// The step running, in order: waiting, the waveform peaks, the attacks, the seek proxy and
/// the thumbnails.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PreparationStageDto {
    Queued,
    Peaks,
    Attacks,
    SeekProxy,
    Thumbnails,
}

/// The audio waveform of a video, served by `/peaks`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PeaksDto {
    pub version: u32,
    /// Pairs per second of audio.
    pub peaks_per_second: u32,
    /// A minimum and a maximum per slot, in turn, from -127 to 127.
    pub peaks: Vec<i8>,
}

/// The attacks (note onsets) found in a video's audio, served by `/attacks`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AttacksDto {
    pub version: u32,
    /// When each attack happens, in seconds, in order.
    pub times: Vec<f64>,
    /// How strong each attack is, from 0 to 1 (1 for the strong attacks of the video).
    pub strengths: Vec<f32>,
}

/// Progress of downloading a video from YouTube.
#[derive(Debug, Serialize, ToSchema)]
pub struct DownloadDto {
    pub stage: DownloadStageDto,
    /// The whole download, from 0 to 1.
    pub progress: f64,
}

/// The step running, in order: waiting, reading the details, downloading, finishing.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStageDto {
    Queued,
    Fetching,
    Downloading,
    Finishing,
}

/// What a video has beyond its own row: how the background work goes, and how far the user
/// got in it.
#[derive(Default)]
struct Extras {
    preparation: Option<Progress>,
    download: Option<DownloadProgress>,
    studied: Option<Studied>,
}

impl VideoDto {
    fn new(video: Video, extras: Extras) -> Self {
        let Extras {
            preparation: progress,
            download,
            studied,
        } = extras;
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
            artist: video.artist,
            year: video.year,
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
            has_attacks: video.attacks_file.is_some(),
            preparation: progress.map(|progress| PreparationDto {
                stage: match progress.stage {
                    None => PreparationStageDto::Queued,
                    Some(PreparationStage::Peaks) => PreparationStageDto::Peaks,
                    Some(PreparationStage::Attacks) => PreparationStageDto::Attacks,
                    Some(PreparationStage::SeekProxy) => PreparationStageDto::SeekProxy,
                    Some(PreparationStage::Thumbnails) => PreparationStageDto::Thumbnails,
                },
                progress: progress.done,
            }),
            preparation_error: video.preparation_error,
            download: download.map(|download| DownloadDto {
                stage: match download.stage {
                    None => DownloadStageDto::Queued,
                    Some(DownloadStage::Fetching) => DownloadStageDto::Fetching,
                    Some(DownloadStage::Downloading) => DownloadStageDto::Downloading,
                    Some(DownloadStage::Finishing) => DownloadStageDto::Finishing,
                },
                progress: download.done,
            }),
            import_error: video.import_error,
            tags: video.tags,
            position_ms: studied.map(|studied| studied.position_ms),
            studied_at: studied.map(|studied| studied.at),
            created_at: video.created_at,
            updated_at: video.updated_at,
        }
    }
}

impl AppState {
    fn dto(&self, video: Video, studied: Option<Studied>) -> VideoDto {
        let extras = Extras {
            preparation: self
                .preparations()
                .and_then(|preparations| preparations.progress(video.id)),
            download: self
                .downloads()
                .and_then(|downloads| downloads.progress(video.id)),
            studied,
        };
        VideoDto::new(video, extras)
    }

    /// The video with how far the user got in it.
    async fn video_dto(&self, video: Video) -> Result<VideoDto, ApiError> {
        let studied = self.studied().await?.remove(&video.id);
        Ok(self.dto(video, studied))
    }

    async fn studied(&self) -> Result<HashMap<VideoId, Studied>, ApiError> {
        Ok(self
            .states
            .studied()
            .await?
            .into_iter()
            .map(|studied| (studied.video, studied))
            .collect())
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
    let mut studied = state.studied().await?;
    Ok(Json(
        videos
            .into_iter()
            .map(|video| {
                let progress = studied.remove(&video.id);
                state.dto(video, progress)
            })
            .collect(),
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
    Ok(Json(state.video_dto(video).await?))
}

/// Changes to a video: each field only when given; `null` clears the artist or the year.
#[derive(Debug, Deserialize, ToSchema)]
pub struct VideoChangesDto {
    /// The song's title.
    pub title: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    pub artist: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<i32>, minimum = 1, maximum = 9999)]
    pub year: Option<Option<i32>>,
    /// Replaces all the video's tags.
    pub tags: Option<Vec<String>>,
}

/// A field that was given, even as `null` (`Some(None)`), apart from one left out (`None`).
fn present<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Changes a video's song (title, artist, year) or replaces its tags.
#[utoipa::path(
    patch,
    path = "/videos/{id}",
    tag = "videos",
    security(("token" = [])),
    params(("id" = Uuid, Path, description = "Video id")),
    request_body = VideoChangesDto,
    responses(
        (status = 200, description = "The video as saved", body = VideoDto),
        (status = 400, description = "An empty title, a year out of range, or too many or too long tags", body = Problem),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 404, description = "No such video", body = Problem)
    )
)]
pub async fn edit_video(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(changes): Json<VideoChangesDto>,
) -> Result<Json<VideoDto>, ApiError> {
    let mut video = state.videos.get(VideoId::from_uuid(id)).await?;
    video.edit(VideoChanges {
        title: changes.title.as_deref(),
        artist: changes.artist.as_ref().map(Option::as_deref),
        year: changes.year,
        tags: changes.tags.as_deref(),
    })?;
    state.videos.update(&video).await?;
    Ok(Json(state.video_dto(video).await?))
}

/// Removes a video from the library with its media files, stopping its download if it is
/// still downloading.
#[utoipa::path(
    delete,
    path = "/videos/{id}",
    tag = "videos",
    security(("token" = [])),
    params(("id" = Uuid, Path, description = "Video id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 404, description = "No such video", body = Problem)
    )
)]
pub async fn delete_video(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let id = VideoId::from_uuid(id);
    if let Some(downloads) = state.downloads() {
        downloads.cancel(id);
    }
    state.videos.delete(id).await?;
    state.media_files.remove_all(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn no_downloads() -> ApiError {
    ApiError::Unavailable(
        "yt-dlp is not installed yet, so videos cannot be downloaded: Segno installs it by itself"
            .into(),
    )
}

/// A YouTube video to download.
#[derive(Debug, Deserialize, ToSchema)]
pub struct DownloadRequest {
    /// Its address: a watch, youtu.be, shorts or music link.
    pub url: String,
}

/// Downloads a YouTube video into the library in the background (its progress is in the
/// video's `download`), then prepares it for navigation. Its chapters become sections. A
/// video already in the library is returned as it is.
#[utoipa::path(
    post,
    path = "/videos/download",
    tag = "videos",
    security(("token" = [])),
    request_body = DownloadRequest,
    responses(
        (status = 201, description = "The video, queued to download", body = VideoDto),
        (status = 200, description = "The video was already in the library", body = VideoDto),
        (status = 400, description = "Not the address of a YouTube video", body = Problem),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 503, description = "yt-dlp or ffmpeg is not available", body = Problem)
    )
)]
pub async fn download_video(
    State(state): State<AppState>,
    Json(request): Json<DownloadRequest>,
) -> Result<(StatusCode, Json<VideoDto>), ApiError> {
    let downloads = state.downloads().ok_or_else(no_downloads)?;
    let (video, created) = download::begin_download(state.videos.as_ref(), &request.url).await?;
    if created {
        downloads.enqueue(video.id);
    }
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(state.video_dto(video).await?)))
}

/// Downloads a failed YouTube video again.
#[utoipa::path(
    post,
    path = "/videos/{id}/retry",
    tag = "videos",
    security(("token" = [])),
    params(("id" = Uuid, Path, description = "Video id")),
    responses(
        (status = 200, description = "The video, queued to download again", body = VideoDto),
        (status = 400, description = "The video is not a failed download", body = Problem),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 404, description = "No such video", body = Problem),
        (status = 503, description = "yt-dlp or ffmpeg is not available", body = Problem)
    )
)]
pub async fn retry_video(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<VideoDto>, ApiError> {
    let downloads = state.downloads().ok_or_else(no_downloads)?;
    let video = download::retry_download(state.videos.as_ref(), VideoId::from_uuid(id)).await?;
    downloads.enqueue(video.id);
    Ok(Json(state.video_dto(video).await?))
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
    let probe = state.probe().ok_or_else(|| {
        ApiError::Unavailable(
            "ffmpeg is not installed yet, so videos cannot be imported: Segno installs it by itself"
                .into(),
        )
    })?;
    let video = library::import_local_video(
        state.videos.as_ref(),
        probe.as_ref(),
        state.media_files.as_ref(),
        std::path::Path::new(&request.path),
        request.title.as_deref(),
    )
    .await?;
    if let Some(preparations) = state.preparations() {
        preparations.enqueue(video.id);
    }
    Ok((StatusCode::CREATED, Json(state.dto(video, None))))
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

/// The audio waveform: a minimum and a maximum per slot of audio.
#[utoipa::path(
    get,
    path = "/videos/{id}/peaks",
    tag = "videos",
    security(("token" = [])),
    params(("id" = Uuid, Path, description = "Video id")),
    responses(
        (status = 200, description = "The waveform peaks", body = PeaksDto),
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

/// The attacks (note onsets) found in the audio, for stepping from note to note.
#[utoipa::path(
    get,
    path = "/videos/{id}/attacks",
    tag = "videos",
    security(("token" = [])),
    params(("id" = Uuid, Path, description = "Video id")),
    responses(
        (status = 200, description = "The attacks", body = AttacksDto),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 404, description = "No such video, or no attacks yet", body = Problem)
    )
)]
pub async fn get_attacks(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    request: Request,
) -> Result<Response, ApiError> {
    let video = state.videos.get(VideoId::from_uuid(id)).await?;
    serve(&state, video.attacks_file, "attacks", id, request).await
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
