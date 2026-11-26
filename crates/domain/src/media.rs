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

    /// Library-relative and absolute paths for a file derived from `video`'s media
    /// (e.g. `proxy.mp4`), creating its folder if needed.
    async fn derived(&self, video: VideoId, file_name: &str) -> Result<(String, PathBuf)>;

    /// Absolute location of a library-relative media path.
    fn resolve(&self, relative: &str) -> Result<PathBuf>;

    /// Removes every media file of `video` (e.g. after a failed import).
    async fn remove_all(&self, video: VideoId) -> Result<()>;
}

/// Layout of a thumbnail sprite: one tile per second, row after row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThumbnailSprite {
    pub columns: u32,
    pub rows: u32,
    pub tile_width: u32,
    pub tile_height: u32,
    pub interval_ms: u32,
    /// Number of tiles actually drawn (the last row may be partly empty).
    pub count: u32,
}

impl ThumbnailSprite {
    pub const COLUMNS: u32 = 10;
    pub const TILE_WIDTH: u32 = 160;
    pub const INTERVAL_MS: u32 = 1_000;

    /// The sprite for a video of this length and size: a tile at 0 s, 1 s, 2 s… up to the end.
    pub fn for_video(duration_ms: i64, width: u32, height: u32) -> Self {
        let count = u32::try_from(duration_ms.max(0) / i64::from(Self::INTERVAL_MS))
            .unwrap_or(u32::MAX - 1)
            + 1;
        // Even height (encoders need it), keeping the video's aspect ratio.
        let tile_height = if width == 0 {
            90
        } else {
            let exact = f64::from(Self::TILE_WIDTH) * f64::from(height) / f64::from(width);
            ((exact / 2.0).round() as u32).max(1) * 2
        };
        Self {
            columns: Self::COLUMNS,
            rows: count.div_ceil(Self::COLUMNS),
            tile_width: Self::TILE_WIDTH,
            tile_height,
            interval_ms: Self::INTERVAL_MS,
            count,
        }
    }
}

/// What preparing a video needs to know about it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreparationInput {
    pub duration_ms: i64,
    pub fps: f64,
    pub width: u32,
    pub height: u32,
}

/// Port: derives the files that make navigation instant (ffmpeg in the desktop app). Each
/// method writes its target atomically: a reader never sees a half-written file.
#[async_trait]
pub trait MediaPreparer: Send + Sync {
    /// Re-encodes `source` so that any frame decodes fast: frequent keyframes and no
    /// B-frames. Reports progress from 0.0 to 1.0.
    async fn make_seek_proxy(
        &self,
        source: &Path,
        target: &Path,
        input: PreparationInput,
        progress: &(dyn Fn(f64) + Send + Sync),
    ) -> Result<()>;

    /// Draws `sprite` from `source` (fast from a seek proxy: it only decodes keyframes).
    async fn make_thumbnails(
        &self,
        source: &Path,
        target: &Path,
        sprite: ThumbnailSprite,
    ) -> Result<()>;

    /// Writes the audio waveform peaks of `source` as JSON.
    async fn make_peaks(&self, source: &Path, target: &Path) -> Result<()>;

    /// Finds the attacks (note onsets) in the audio of `source` and writes them as JSON.
    async fn make_attacks(&self, source: &Path, target: &Path) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sprite_has_one_tile_per_second_including_the_last_partial_one() {
        let sprite = ThumbnailSprite::for_video(141_154, 1920, 1080);
        assert_eq!(sprite.count, 142);
        assert_eq!((sprite.columns, sprite.rows), (10, 15));
        assert_eq!((sprite.tile_width, sprite.tile_height), (160, 90));
    }

    #[test]
    fn tiles_keep_the_aspect_ratio_with_an_even_height() {
        assert_eq!(
            ThumbnailSprite::for_video(5_000, 1440, 1080).tile_height,
            120
        );
        assert_eq!(
            ThumbnailSprite::for_video(5_000, 1080, 1920).tile_height,
            284
        );
        assert_eq!(ThumbnailSprite::for_video(5_000, 0, 0).tile_height, 90);
    }

    #[test]
    fn short_videos_get_one_row() {
        let sprite = ThumbnailSprite::for_video(5_000, 1280, 720);
        assert_eq!((sprite.count, sprite.rows), (6, 1));
    }
}
