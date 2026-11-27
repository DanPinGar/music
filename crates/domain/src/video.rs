use std::fmt;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{DomainError, MediaInfo, Result, SongDetails};

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
    /// The song's title: what the library shows first.
    pub title: String,
    /// Who wrote or plays the song.
    pub artist: Option<String>,
    /// The year the song (or its album) was published: the library is sorted by it.
    pub year: Option<i32>,
    /// The YouTube channel the video comes from.
    pub channel: Option<String>,
    pub duration_ms: Option<i64>,
    pub fps: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub status: VideoStatus,
    /// Playable media, relative to the library folder (e.g. `media/<id>/original.mp4`).
    pub media_file: Option<String>,
    /// Copy of the media re-encoded for instant seeking (frequent keyframes, no B-frames).
    pub proxy_file: Option<String>,
    /// Sprite of one thumbnail per second (see [`crate::ThumbnailSprite`]).
    pub thumbnails_file: Option<String>,
    /// Waveform peaks of the audio, as JSON.
    pub peaks_file: Option<String>,
    /// Attacks (note onsets) found in the audio, as JSON.
    pub attacks_file: Option<String>,
    /// Why preparing the media for navigation failed, if it did.
    pub preparation_error: Option<String>,
    /// Why the download or import failed, if it did (status failed).
    pub import_error: Option<String>,
    /// The user's labels for organising the library ("Jazz", "To learn"…).
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const MAX_TAGS: usize = 20;
const MAX_TAG_CHARS: usize = 40;
const MAX_TITLE_CHARS: usize = 300;
const MAX_ARTIST_CHARS: usize = 200;
const YEARS: std::ops::RangeInclusive<i32> = 1..=9999;

/// Changes to a video's details: each field only when given; `Some(None)` clears it.
#[derive(Debug, Clone, Copy, Default)]
pub struct VideoChanges<'a> {
    pub title: Option<&'a str>,
    pub artist: Option<Option<&'a str>>,
    pub year: Option<Option<i32>>,
    pub tags: Option<&'a [String]>,
}

/// Tags trimmed, without empty ones or repeats (ignoring case), in the order given.
pub fn clean_tags(tags: &[String]) -> Result<Vec<String>> {
    let mut clean: Vec<String> = Vec::new();
    for tag in tags {
        let tag = tag.trim();
        if tag.is_empty() || clean.iter().any(|kept| kept.eq_ignore_ascii_case(tag)) {
            continue;
        }
        if tag.chars().count() > MAX_TAG_CHARS {
            return Err(DomainError::Invalid(format!(
                "tags are at most {MAX_TAG_CHARS} characters"
            )));
        }
        clean.push(tag.to_owned());
    }
    if clean.len() > MAX_TAGS {
        return Err(DomainError::Invalid(format!(
            "a video has at most {MAX_TAGS} tags"
        )));
    }
    Ok(clean)
}

impl Video {
    /// Whether the media still needs preparing for navigation (and has not failed already).
    pub fn needs_preparation(&self) -> bool {
        self.media_file.is_some()
            && self.preparation_error.is_none()
            && (self.proxy_file.is_none()
                || self.thumbnails_file.is_none()
                || self.peaks_file.is_none()
                || self.attacks_file.is_none())
    }

    /// Changes the song's title, artist and year and replaces the tags, each only when given.
    /// Nothing changes if any of them is not valid.
    pub fn edit(&mut self, changes: VideoChanges<'_>) -> Result<()> {
        let title = changes
            .title
            .map(|title| {
                let title = title.trim();
                if title.is_empty() || title.chars().count() > MAX_TITLE_CHARS {
                    return Err(DomainError::Invalid(format!(
                        "a title has between 1 and {MAX_TITLE_CHARS} characters"
                    )));
                }
                Ok(title.to_owned())
            })
            .transpose()?;
        let artist = changes
            .artist
            .map(|artist| {
                let artist = artist.map(str::trim).filter(|artist| !artist.is_empty());
                if artist.is_some_and(|artist| artist.chars().count() > MAX_ARTIST_CHARS) {
                    return Err(DomainError::Invalid(format!(
                        "an artist has at most {MAX_ARTIST_CHARS} characters"
                    )));
                }
                Ok(artist.map(str::to_owned))
            })
            .transpose()?;
        if let Some(Some(year)) = changes.year
            && !YEARS.contains(&year)
        {
            return Err(DomainError::Invalid(format!(
                "a year is between {} and {}",
                YEARS.start(),
                YEARS.end()
            )));
        }
        let tags = changes.tags.map(clean_tags).transpose()?;

        if let Some(title) = title {
            self.title = title;
        }
        if let Some(artist) = artist {
            self.artist = artist;
        }
        if let Some(year) = changes.year {
            self.year = year;
        }
        if let Some(tags) = tags {
            self.tags = tags;
        }
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Takes the song's title and artist from `song` (an empty title is not taken). The year
    /// stays as it is: only the user gives it.
    pub fn apply_song(&mut self, song: SongDetails) {
        let title = song.title.trim();
        if !title.is_empty() {
            self.title = title.chars().take(MAX_TITLE_CHARS).collect();
        }
        self.artist = song
            .artist
            .map(|artist| {
                artist
                    .trim()
                    .chars()
                    .take(MAX_ARTIST_CHARS)
                    .collect::<String>()
            })
            .filter(|artist| !artist.is_empty());
        self.updated_at = Utc::now();
    }

    /// Copies the technical facts of the media into the entry.
    pub fn apply_media_info(&mut self, info: &MediaInfo) {
        self.duration_ms = Some(info.duration_ms);
        self.fps = info.fps;
        self.width = info.width;
        self.height = info.height;
        self.updated_at = Utc::now();
    }

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
            artist: None,
            year: None,
            channel: None,
            duration_ms: None,
            fps: None,
            width: None,
            height: None,
            status: VideoStatus::Importing,
            media_file: None,
            proxy_file: None,
            thumbnails_file: None,
            peaks_file: None,
            attacks_file: None,
            preparation_error: None,
            import_error: None,
            tags: Vec::new(),
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

    /// Saves every field of an existing video, as given (callers set `updated_at`).
    async fn update(&self, video: &Video) -> Result<()>;

    /// Removes a video from the library (softly, for future sync), or
    /// [`DomainError::NotFound`].
    async fn delete(&self, id: VideoId) -> Result<()>;
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
    fn tags_are_trimmed_and_kept_once() {
        let tags: Vec<String> = ["  Jazz ", "jazz", "", "To learn", "JAZZ"]
            .map(String::from)
            .to_vec();
        assert_eq!(clean_tags(&tags).unwrap(), ["Jazz", "To learn"]);
        assert!(clean_tags(&["x".repeat(41)]).is_err());
    }

    #[test]
    fn videos_are_renamed_and_tagged() {
        let mut video = Video::new_importing(VideoSource::File, "clip", None).unwrap();
        let tags = ["Ballad".to_owned()];
        video
            .edit(VideoChanges {
                title: Some("  Dinah  "),
                artist: Some(Some(" Thelonious Monk ")),
                year: Some(Some(1965)),
                tags: Some(&tags),
            })
            .unwrap();
        assert_eq!(
            (
                video.title.as_str(),
                video.artist.as_deref(),
                video.year,
                video.tags.as_slice()
            ),
            (
                "Dinah",
                Some("Thelonious Monk"),
                Some(1965),
                tags.as_slice()
            )
        );

        video.edit(VideoChanges::default()).unwrap();
        assert_eq!(
            (video.title.as_str(), video.artist.as_deref()),
            ("Dinah", Some("Thelonious Monk"))
        );
        video
            .edit(VideoChanges {
                artist: Some(Some("  ")),
                year: Some(None),
                ..VideoChanges::default()
            })
            .unwrap();
        assert_eq!((video.artist.as_deref(), video.year), (None, None));

        for wrong in [
            VideoChanges {
                title: Some("  "),
                ..VideoChanges::default()
            },
            VideoChanges {
                title: Some("Kept?"),
                year: Some(Some(12_000)),
                ..VideoChanges::default()
            },
        ] {
            assert!(video.edit(wrong).is_err());
        }
        assert_eq!(video.title, "Dinah", "a refused edit changes nothing");
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
