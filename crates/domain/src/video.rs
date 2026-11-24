use std::fmt;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{DomainError, Result};

/// Identifier of a video in the library.
///
/// UUIDv7: time-ordered, so ids sort by creation and are safe to merge when
/// libraries are synchronised in the future.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VideoId(Uuid);

impl VideoId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for VideoId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for VideoId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for VideoId {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|_| DomainError::Invalid(format!("invalid video id: {s}")))
    }
}

/// Where a video came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoSource {
    Youtube,
    File,
}

impl VideoSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Youtube => "youtube",
            Self::File => "file",
        }
    }
}

impl FromStr for VideoSource {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "youtube" => Ok(Self::Youtube),
            "file" => Ok(Self::File),
            other => Err(DomainError::Invalid(format!(
                "unknown video source: {other}"
            ))),
        }
    }
}

/// Lifecycle of a library entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoStatus {
    /// Being downloaded, imported or prepared for navigation.
    Importing,
    /// Ready to be played.
    Ready,
    /// The import failed; the entry is kept so the user can retry or remove it.
    Failed,
}

impl VideoStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Importing => "importing",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }
}

impl FromStr for VideoStatus {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "importing" => Ok(Self::Importing),
            "ready" => Ok(Self::Ready),
            "failed" => Ok(Self::Failed),
            other => Err(DomainError::Invalid(format!(
                "unknown video status: {other}"
            ))),
        }
    }
}

/// A video in the library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Video {
    pub id: VideoId,
    pub source: VideoSource,
    pub source_url: Option<String>,
    pub title: String,
    pub channel: Option<String>,
    pub duration_ms: Option<i64>,
    pub fps: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub status: VideoStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Video {
    /// Creates a library entry that is still being imported.
    pub fn new_importing(
        source: VideoSource,
        title: &str,
        source_url: Option<String>,
    ) -> Result<Self> {
        let title = title.trim();
        if title.is_empty() {
            return Err(DomainError::Invalid("a video needs a title".into()));
        }
        let now = Utc::now();
        Ok(Self {
            id: VideoId::new(),
            source,
            source_url,
            title: title.to_owned(),
            channel: None,
            duration_ms: None,
            fps: None,
            width: None,
            height: None,
            status: VideoStatus::Importing,
            created_at: now,
            updated_at: now,
        })
    }
}

/// Port: persistence of the videos in the library.
#[async_trait]
pub trait VideoRepository: Send + Sync {
    /// All videos that have not been deleted, newest first.
    async fn list(&self) -> Result<Vec<Video>>;

    /// One video, or [`DomainError::NotFound`].
    async fn get(&self, id: VideoId) -> Result<Video>;

    async fn insert(&self, video: &Video) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_videos_start_importing_with_a_trimmed_title() {
        let video = Video::new_importing(VideoSource::File, "  Nocturne  ", None).unwrap();
        assert_eq!(video.title, "Nocturne");
        assert_eq!(video.status, VideoStatus::Importing);
        assert_eq!(video.created_at, video.updated_at);
    }

    #[test]
    fn a_title_is_required() {
        let err = Video::new_importing(VideoSource::File, "   ", None).unwrap_err();
        assert!(matches!(err, DomainError::Invalid(_)));
    }

    #[test]
    fn ids_round_trip_through_text() {
        let id = VideoId::new();
        assert_eq!(id.to_string().parse::<VideoId>().unwrap(), id);
        assert!("not-a-uuid".parse::<VideoId>().is_err());
    }

    #[test]
    fn enums_round_trip_through_their_storage_names() {
        for source in [VideoSource::Youtube, VideoSource::File] {
            assert_eq!(source.as_str().parse::<VideoSource>().unwrap(), source);
        }
        for status in [
            VideoStatus::Importing,
            VideoStatus::Ready,
            VideoStatus::Failed,
        ] {
            assert_eq!(status.as_str().parse::<VideoStatus>().unwrap(), status);
        }
    }
}
