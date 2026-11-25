use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::{Result, VideoId};

/// What the library needs to know about a media file.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaInfo {
    pub duration_ms: i64,
    /// Average frame rate of the video stream.
    pub fps: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
}

impl MediaInfo {
    pub fn has_video(&self) -> bool {
        self.width.is_some() && self.height.is_some()
    }
}

/// Port: reads the technical facts of a media file (ffprobe in the desktop app).
#[async_trait]
pub trait MediaProbe: Send + Sync {
    /// Fails with [`crate::DomainError::Invalid`] for files that are not readable media, and
    /// with [`crate::DomainError::Unavailable`] when the probing tool cannot run.
    async fn probe(&self, file: &Path) -> Result<MediaInfo>;
}

/// Port: the media files of the library (a folder today, object storage in the cloud).
#[async_trait]
pub trait MediaStore: Send + Sync {
    /// Places `source` in the library as the original media of `video` and returns its
    /// library-relative path.
    async fn store_original(&self, video: VideoId, source: &Path) -> Result<String>;

    /// Absolute location of a library-relative media path.
    fn resolve(&self, relative: &str) -> Result<PathBuf>;

    /// Removes every media file of `video` (e.g. after a failed import).
    async fn remove_all(&self, video: VideoId) -> Result<()>;
}
