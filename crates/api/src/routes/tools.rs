use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use segno_tools::{Tool, ToolState};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{ApiError, AppState, Problem, ToolVersions};

/// An external tool the core runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub enum ToolNameDto {
    /// ffmpeg and ffprobe: importing and preparing videos.
    #[serde(rename = "ffmpeg")]
    Ffmpeg,
    /// yt-dlp: YouTube downloads.
    #[serde(rename = "yt-dlp")]
    YtDlp,
    /// Deno: the JavaScript runtime yt-dlp needs for YouTube.
    #[serde(rename = "deno")]
    Deno,
}

/// Where a tool stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolStateDto {
    /// Installed and in use.
    Ready,
    /// Not installed: what needs it is unavailable until it is.
    Missing,
    /// Being downloaded for the first time.
    Installing,
    /// A newer version is being downloaded; the installed one keeps working.
    Updating,
    /// Found outside the core's tools folder (development, an environment variable): the
    /// core neither installs nor updates it.
    External,
    /// No build the core can install on this computer: the system's is used, if any.
    Unsupported,
}

/// An external tool: its version in use, and what the core is doing with it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ToolDto {
    pub name: ToolNameDto,
    pub state: ToolStateDto,
    /// Version in use, as the tool reports it.
    pub version: Option<String>,
    /// Download progress while installing or updating, from 0 to 1.
    pub progress: Option<f64>,
    /// A newer version already downloaded, used from the next start.
    pub pending_version: Option<String>,
    /// When the core last looked for a newer release.
    pub checked_at: Option<DateTime<Utc>>,
    /// Why the last install or update failed.
    pub error: Option<String>,
}

/// The external tools and what the core does with them.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ToolsDto {
    /// Whether the core installs and updates its tools itself.
    pub managed: bool,
    pub tools: Vec<ToolDto>,
}

impl AppState {
    fn tools_dto(&self) -> ToolsDto {
        let versions = self.services.get().versions;
        let tools = Tool::ALL
            .into_iter()
            .map(|tool| {
                let version = in_use(&versions, tool);
                let Some(manager) = &self.tools else {
                    return ToolDto {
                        name: name(tool),
                        state: if version.is_some() {
                            ToolStateDto::Ready
                        } else {
                            ToolStateDto::Missing
                        },
                        version,
                        progress: None,
                        pending_version: None,
                        checked_at: None,
                        error: None,
                    };
                };
                let status = manager.status(tool);
                let (state, progress) = match status.state {
                    ToolState::Ready => (ToolStateDto::Ready, None),
                    ToolState::Missing => (ToolStateDto::Missing, None),
                    ToolState::External => (ToolStateDto::External, None),
                    ToolState::Unsupported => (ToolStateDto::Unsupported, None),
                    ToolState::Installing { progress } if status.version.is_some() => {
                        (ToolStateDto::Updating, Some(progress))
                    }
                    ToolState::Installing { progress } => {
                        (ToolStateDto::Installing, Some(progress))
                    }
                };
                ToolDto {
                    name: name(tool),
                    state,
                    version,
                    progress,
                    pending_version: status.staged,
                    checked_at: status.checked_at,
                    error: status.error,
                }
            })
            .collect();
        ToolsDto {
            managed: self.tools.is_some(),
            tools,
        }
    }
}

fn name(tool: Tool) -> ToolNameDto {
    match tool {
        Tool::Ffmpeg => ToolNameDto::Ffmpeg,
        Tool::YtDlp => ToolNameDto::YtDlp,
        Tool::Deno => ToolNameDto::Deno,
    }
}

fn in_use(versions: &ToolVersions, tool: Tool) -> Option<String> {
    match tool {
        Tool::Ffmpeg => versions.ffmpeg.clone(),
        Tool::YtDlp => versions.ytdlp.clone(),
        Tool::Deno => versions.deno.clone(),
    }
}

/// The external tools (ffmpeg, yt-dlp, Deno): versions in use, installs and updates running.
#[utoipa::path(
    get,
    path = "/tools",
    tag = "tools",
    security(("token" = [])),
    responses(
        (status = 200, description = "The tools", body = ToolsDto),
        (status = 401, description = "Missing or invalid token", body = Problem)
    )
)]
pub async fn list_tools(State(state): State<AppState>) -> Json<ToolsDto> {
    Json(state.tools_dto())
}

/// Installs the missing tools and looks for newer releases now, in the background.
#[utoipa::path(
    post,
    path = "/tools/check",
    tag = "tools",
    security(("token" = [])),
    responses(
        (status = 202, description = "Checking; follow it with GET /tools", body = ToolsDto),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 503, description = "This core does not manage its tools", body = Problem)
    )
)]
pub async fn check_tools(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<ToolsDto>), ApiError> {
    let manager = state.tools.as_ref().ok_or_else(|| {
        ApiError::Unavailable("this core uses the tools it finds and does not update them".into())
    })?;
    manager.check_now();
    Ok((StatusCode::ACCEPTED, Json(state.tools_dto())))
}
