use axum::Json;
use axum::extract::{Path, State};
use segno_domain::{
    BeatGrid, LoopRange, Practice, SpeedTrainer, StepUnit, TimelineView, VideoId, VideoState,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{ApiError, AppState, Problem};

/// Where the user left a video, so it reopens the same way.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct VideoStateDto {
    pub position_ms: i64,
    /// Playback rate, from 0.25 to 2 (1 is normal speed).
    pub speed: f64,
    pub step_unit: StepUnitDto,
    /// The stretch the detail timeline showed.
    pub view: Option<TimelineViewDto>,
    pub beat_grid: Option<BeatGridDto>,
    #[serde(rename = "loop")]
    pub loop_range: Option<LoopRangeDto>,
    pub practice: PracticeDto,
}

/// What the arrow keys step by.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepUnitDto {
    Seconds,
    Beat,
    Bar,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TimelineViewDto {
    pub start_ms: i64,
    pub span_ms: i64,
}

/// Beats tapped by the user. Before the first and after the last, the grid continues at the
/// tempo of its ends.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BeatGridDto {
    /// Times of the beats, strictly increasing.
    pub beats_ms: Vec<i64>,
    pub beats_per_bar: u32,
    /// Index of a beat that starts bar 1 (it may lie outside `beats_ms`).
    pub downbeat: i32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LoopRangeDto {
    pub start_ms: i64,
    pub end_ms: i64,
    pub enabled: bool,
}

/// How loops are practised.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PracticeDto {
    /// Beats played before the loop start on every repetition.
    pub pre_roll_beats: u32,
    /// A bar of metronome clicks before playback starts.
    pub count_in: bool,
    pub trainer: Option<SpeedTrainerDto>,
}

/// Speeds the loop up on every repetition: `from`, then `step` more each time, up to `to`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SpeedTrainerDto {
    pub from: f64,
    pub step: f64,
    pub to: f64,
}

impl From<VideoState> for VideoStateDto {
    fn from(state: VideoState) -> Self {
        Self {
            position_ms: state.position_ms,
            speed: state.speed,
            step_unit: match state.step_unit {
                StepUnit::Seconds => StepUnitDto::Seconds,
                StepUnit::Beat => StepUnitDto::Beat,
                StepUnit::Bar => StepUnitDto::Bar,
            },
            view: state.view.map(|view| TimelineViewDto {
                start_ms: view.start_ms,
                span_ms: view.span_ms,
            }),
            beat_grid: state.beat_grid.map(|grid| BeatGridDto {
                beats_ms: grid.beats_ms,
                beats_per_bar: grid.beats_per_bar,
                downbeat: grid.downbeat,
            }),
            loop_range: state.loop_range.map(|range| LoopRangeDto {
                start_ms: range.start_ms,
                end_ms: range.end_ms,
                enabled: range.enabled,
            }),
            practice: PracticeDto {
                pre_roll_beats: state.practice.pre_roll_beats,
                count_in: state.practice.count_in,
                trainer: state.practice.trainer.map(|trainer| SpeedTrainerDto {
                    from: trainer.from,
                    step: trainer.step,
                    to: trainer.to,
                }),
            },
        }
    }
}

impl From<VideoStateDto> for VideoState {
    fn from(dto: VideoStateDto) -> Self {
        Self {
            position_ms: dto.position_ms,
            speed: dto.speed,
            step_unit: match dto.step_unit {
                StepUnitDto::Seconds => StepUnit::Seconds,
                StepUnitDto::Beat => StepUnit::Beat,
                StepUnitDto::Bar => StepUnit::Bar,
            },
            view: dto.view.map(|view| TimelineView {
                start_ms: view.start_ms,
                span_ms: view.span_ms,
            }),
            beat_grid: dto.beat_grid.map(|grid| BeatGrid {
                beats_ms: grid.beats_ms,
                beats_per_bar: grid.beats_per_bar,
                downbeat: grid.downbeat,
            }),
            loop_range: dto.loop_range.map(|range| LoopRange {
                start_ms: range.start_ms,
                end_ms: range.end_ms,
                enabled: range.enabled,
            }),
            practice: Practice {
                pre_roll_beats: dto.practice.pre_roll_beats,
                count_in: dto.practice.count_in,
                trainer: dto.practice.trainer.map(|trainer| SpeedTrainer {
                    from: trainer.from,
                    step: trainer.step,
                    to: trainer.to,
                }),
            },
        }
    }
}

/// Where the user left the video; the defaults for a video never opened.
#[utoipa::path(
    get,
    path = "/videos/{id}/state",
    tag = "study",
    security(("token" = [])),
    params(("id" = Uuid, Path, description = "Video id")),
    responses(
        (status = 200, description = "The video's study state", body = VideoStateDto),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 404, description = "No such video", body = Problem)
    )
)]
pub async fn get_state(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<VideoStateDto>, ApiError> {
    let video = state.videos.get(VideoId::from_uuid(id)).await?;
    let saved = state.states.get_state(video.id).await?.unwrap_or_default();
    Ok(Json(saved.into()))
}

/// Saves where the user is in the video (the whole document).
#[utoipa::path(
    put,
    path = "/videos/{id}/state",
    tag = "study",
    security(("token" = [])),
    params(("id" = Uuid, Path, description = "Video id")),
    request_body = VideoStateDto,
    responses(
        (status = 200, description = "The state as saved", body = VideoStateDto),
        (status = 400, description = "The state makes no sense (e.g. a loop ending before it starts)", body = Problem),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 404, description = "No such video", body = Problem)
    )
)]
pub async fn put_state(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<VideoStateDto>,
) -> Result<Json<VideoStateDto>, ApiError> {
    let video = state.videos.get(VideoId::from_uuid(id)).await?;
    let saved = VideoState::from(body).validated()?;
    state.states.save_state(video.id, &saved).await?;
    Ok(Json(saved.into()))
}
