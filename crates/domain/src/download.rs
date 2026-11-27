//! Downloading videos from YouTube into the library.

use std::path::Path;

use async_trait::async_trait;
use chrono::Utc;

use crate::{
    DomainError, Marker, MarkerFields, MarkerKind, MarkerRepository, MediaProbe, MediaStore,
    Result, SongDetails, Video, VideoId, VideoRepository, VideoSource, VideoStatus, guess_song,
};

/// What YouTube says about a video before it is downloaded.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteVideo {
    pub title: String,
    pub channel: Option<String>,
    /// The song's title and artist, when YouTube knows them (music it identified).
    pub track: Option<String>,
    pub artist: Option<String>,
    pub duration_ms: Option<i64>,
    pub chapters: Vec<Chapter>,
}

/// A chapter of a YouTube video; it becomes a section of the song.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chapter {
    pub start_ms: i64,
    pub end_ms: i64,
    pub title: String,
}

/// The steps of a download, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadStage {
    /// Reading the title, channel and chapters.
    Fetching,
    Downloading,
    /// Joining picture and sound, checking the file.
    Finishing,
}

/// Port: downloads from YouTube (yt-dlp in the desktop app).
#[async_trait]
pub trait VideoDownloader: Send + Sync {
    async fn fetch_info(&self, url: &str) -> Result<RemoteVideo>;

    /// Downloads the video at `url` as one MP4 file at `target` (written there only when
    /// complete). Reports progress from 0.0 to 1.0.
    async fn download(
        &self,
        url: &str,
        target: &Path,
        progress: &(dyn Fn(f64) + Send + Sync),
    ) -> Result<()>;
}

/// The id of a YouTube video in an address (watch, youtu.be, shorts, music or mobile links),
/// or `None` if it is not one.
pub fn youtube_id(address: &str) -> Option<String> {
    let address = address.trim();
    let rest = address
        .strip_prefix("https://")
        .or_else(|| address.strip_prefix("http://"))
        .unwrap_or(address);
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    let candidate = match host {
        "youtu.be" => path.split('/').next(),
        "youtube.com" | "m.youtube.com" | "music.youtube.com" => {
            if path == "watch" {
                query.split('&').find_map(|pair| pair.strip_prefix("v="))
            } else {
                path.strip_prefix("shorts/")
                    .or_else(|| path.strip_prefix("live/"))
                    .or_else(|| path.strip_prefix("embed/"))
                    .map(|id| id.split('/').next().unwrap_or(id))
            }
        }
        _ => None,
    }?;
    let valid = candidate.len() == 11
        && candidate
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    valid.then(|| candidate.to_owned())
}

/// The address the library keeps for a YouTube video.
pub fn youtube_url(id: &str) -> String {
    format!("https://www.youtube.com/watch?v={id}")
}

/// Records a YouTube video to download, so the library shows it straight away; the download
/// itself is [`download_video`]. A video already in the library (and not failed) is returned
/// as it is, with `false`.
pub async fn begin_download(videos: &dyn VideoRepository, address: &str) -> Result<(Video, bool)> {
    let id = youtube_id(address).ok_or_else(|| {
        DomainError::Invalid(format!("not the address of a YouTube video: {address}"))
    })?;
    let url = youtube_url(&id);
    if let Some(existing) = videos
        .list()
        .await?
        .into_iter()
        .find(|video| video.source_url.as_deref() == Some(url.as_str()))
    {
        return Ok((existing, false));
    }
    // The real title arrives with the video's details.
    let video = Video::new_importing(VideoSource::Youtube, &format!("YouTube {id}"), Some(url))?;
    videos.insert(&video).await?;
    Ok((video, true))
}

/// Everything [`download_video`] works with.
pub struct DownloadContext<'a> {
    pub videos: &'a dyn VideoRepository,
    pub markers: &'a dyn MarkerRepository,
    pub probe: &'a dyn MediaProbe,
    pub store: &'a dyn MediaStore,
    pub downloader: &'a dyn VideoDownloader,
}

/// Downloads a recorded YouTube video: its details first (title, channel and chapters, which
/// become sections), then the media, into the library. Ready to play when it returns. A
/// failure is recorded on the video (`import_error`, status failed) and its files removed.
pub async fn download_video(
    context: &DownloadContext<'_>,
    id: VideoId,
    progress: &(dyn Fn(DownloadStage, f64) + Send + Sync),
) -> Result<Video> {
    let mut video = context.videos.get(id).await?;
    match fetch_and_download(context, &mut video, progress).await {
        Ok(()) => Ok(video),
        Err(err) => {
            let _ = context.store.remove_all(video.id).await;
            video.status = VideoStatus::Failed;
            video.import_error = Some(err.to_string());
            video.media_file = None;
            video.updated_at = Utc::now();
            context.videos.update(&video).await?;
            Err(err)
        }
    }
}

async fn fetch_and_download(
    context: &DownloadContext<'_>,
    video: &mut Video,
    progress: &(dyn Fn(DownloadStage, f64) + Send + Sync),
) -> Result<()> {
    let url = video
        .source_url
        .clone()
        .ok_or_else(|| DomainError::Invalid(format!("video {} has no address", video.id)))?;

    progress(DownloadStage::Fetching, 0.0);
    let info = context.downloader.fetch_info(&url).await?;
    video.apply_song(song_of(&info));
    video.channel = info.channel.clone();
    video.duration_ms = info.duration_ms.or(video.duration_ms);
    video.status = VideoStatus::Importing;
    video.import_error = None;
    save(video, context.videos).await?;
    add_chapters(context, video.id, &info.chapters).await?;

    let (relative, path) = context.store.derived(video.id, "original.mp4").await?;
    progress(DownloadStage::Downloading, 0.0);
    context
        .downloader
        .download(&url, &path, &|done| {
            progress(DownloadStage::Downloading, done)
        })
        .await?;

    progress(DownloadStage::Finishing, 0.0);
    let media = context.probe.probe(&path).await?;
    if !media.has_video() {
        return Err(DomainError::Invalid("the download has no video".into()));
    }
    video.apply_media_info(&media);
    video.media_file = Some(relative);
    video.status = VideoStatus::Ready;
    save(video, context.videos).await
}

/// The song in a YouTube video: what YouTube says about the music when it knows, guessed from
/// the video's title otherwise. Never its year, which only the user gives.
pub fn song_of(info: &RemoteVideo) -> SongDetails {
    match (&info.track, &info.artist) {
        (Some(track), Some(artist)) => SongDetails {
            title: track.clone(),
            artist: Some(artist.clone()),
        },
        _ => guess_song(&info.title),
    }
}

/// Chapters become sections, unless the video already has markers (a retried download).
async fn add_chapters(
    context: &DownloadContext<'_>,
    video: VideoId,
    chapters: &[Chapter],
) -> Result<()> {
    if chapters.is_empty() || !context.markers.list_markers(video).await?.is_empty() {
        return Ok(());
    }
    for (index, chapter) in chapters.iter().enumerate() {
        let fields = MarkerFields {
            start_ms: chapter.start_ms,
            end_ms: Some(chapter.end_ms),
            label: chapter.title.chars().take(120).collect(),
            color: Some(CHAPTER_COLORS[index % CHAPTER_COLORS.len()].to_owned()),
        };
        // A malformed chapter (zero length, say) is skipped rather than failing the download.
        if let Ok(marker) = Marker::new(video, MarkerKind::Section, fields) {
            context.markers.insert_marker(&marker).await?;
        }
    }
    Ok(())
}

/// The UI's section colours, in its order.
const CHAPTER_COLORS: [&str; 8] = [
    "#e5484d", "#f76b15", "#ffc53d", "#46a758", "#12a594", "#0090ff", "#8e4ec6", "#d6409f",
];

/// Puts a failed download back in the queue's hands: importing again, without the error.
pub async fn retry_download(videos: &dyn VideoRepository, id: VideoId) -> Result<Video> {
    let mut video = videos.get(id).await?;
    if video.source != VideoSource::Youtube || video.status != VideoStatus::Failed {
        return Err(DomainError::Invalid(
            "only failed downloads can be retried".into(),
        ));
    }
    video.status = VideoStatus::Importing;
    video.import_error = None;
    save(&mut video, videos).await?;
    Ok(video)
}

async fn save(video: &mut Video, videos: &dyn VideoRepository) -> Result<()> {
    video.updated_at = Utc::now();
    videos.update(video).await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::fakes::{Markers, Probe, Store, Videos, video_info};

    const MONK: &str = "https://www.youtube.com/watch?v=7A12z9S4F8U&list=RD7A12z9S4F8U";

    /// Answers like YouTube with two chapters; can fail the download.
    #[derive(Default)]
    struct Downloader {
        fail: bool,
        downloaded: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl VideoDownloader for Downloader {
        async fn fetch_info(&self, url: &str) -> Result<RemoteVideo> {
            assert_eq!(url, "https://www.youtube.com/watch?v=7A12z9S4F8U");
            Ok(RemoteVideo {
                title: "Thelonious Monk - I'm Confessin' 1930 [Stride Piano Synthesia]".into(),
                channel: Some("itsRemco".into()),
                track: None,
                artist: None,
                duration_ms: Some(141_000),
                chapters: vec![
                    Chapter {
                        start_ms: 0,
                        end_ms: 60_000,
                        title: "Theme".into(),
                    },
                    Chapter {
                        start_ms: 60_000,
                        end_ms: 141_000,
                        title: "Solo".into(),
                    },
                ],
            })
        }
        async fn download(
            &self,
            _: &str,
            target: &Path,
            progress: &(dyn Fn(f64) + Send + Sync),
        ) -> Result<()> {
            if self.fail {
                return Err(DomainError::Unavailable("ERROR: Video unavailable".into()));
            }
            progress(0.5);
            self.downloaded
                .lock()
                .unwrap()
                .push(target.display().to_string());
            Ok(())
        }
    }

    struct World {
        videos: Videos,
        markers: Markers,
        probe: Probe,
        store: Store,
        downloader: Downloader,
    }

    impl World {
        fn new(fail: bool) -> Self {
            Self {
                videos: Videos::default(),
                markers: Markers::default(),
                probe: Probe(video_info()),
                store: Store::default(),
                downloader: Downloader {
                    fail,
                    ..Downloader::default()
                },
            }
        }
        fn context(&self) -> DownloadContext<'_> {
            DownloadContext {
                videos: &self.videos,
                markers: &self.markers,
                probe: &self.probe,
                store: &self.store,
                downloader: &self.downloader,
            }
        }
    }

    #[test]
    fn youtube_music_details_come_before_the_guess() {
        let info = RemoteVideo {
            title: "Dave Brubeck - Take Five 1959 (Piano Tutorial)".into(),
            channel: None,
            track: Some("Take Five".into()),
            artist: Some("The Dave Brubeck Quartet".into()),
            duration_ms: None,
            chapters: Vec::new(),
        };
        assert_eq!(
            song_of(&info),
            SongDetails {
                title: "Take Five".into(),
                artist: Some("The Dave Brubeck Quartet".into()),
            }
        );
        let unknown = RemoteVideo {
            track: None,
            artist: None,
            ..info
        };
        assert_eq!(
            song_of(&unknown),
            SongDetails {
                title: "Take Five".into(),
                artist: Some("Dave Brubeck".into()),
            }
        );
    }

    #[tokio::test]
    async fn a_download_is_recorded_at_once_and_only_once() {
        let world = World::new(false);
        let (video, created) = begin_download(&world.videos, MONK).await.unwrap();
        assert!(created);
        assert_eq!(video.status, VideoStatus::Importing);
        assert_eq!(video.source, VideoSource::Youtube);
        let (again, created) = begin_download(&world.videos, "https://youtu.be/7A12z9S4F8U")
            .await
            .unwrap();
        assert!(!created);
        assert_eq!(again.id, video.id);
        assert!(
            begin_download(&world.videos, "https://vimeo.com/1")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn downloads_get_their_details_media_and_chapters() {
        let world = World::new(false);
        let (video, _) = begin_download(&world.videos, MONK).await.unwrap();
        let stages = Mutex::new(Vec::new());

        let ready = download_video(&world.context(), video.id, &|stage, done| {
            stages.lock().unwrap().push((stage, done));
        })
        .await
        .unwrap();

        assert_eq!(ready.status, VideoStatus::Ready);
        // The year in the title is left to the user.
        assert_eq!(
            (ready.title.as_str(), ready.artist.as_deref(), ready.year),
            ("I'm Confessin'", Some("Thelonious Monk"), None)
        );
        assert_eq!(ready.channel.as_deref(), Some("itsRemco"));
        assert_eq!(ready.fps, Some(60.0));
        assert!(
            ready
                .media_file
                .as_deref()
                .unwrap()
                .ends_with("original.mp4")
        );
        assert!(ready.needs_preparation());
        let sections = world.markers.list_markers(video.id).await.unwrap();
        let labels: Vec<&str> = sections.iter().map(|m| m.label.as_str()).collect();
        assert_eq!(labels, ["Theme", "Solo"]);
        assert!(sections.iter().all(|m| m.kind == MarkerKind::Section));
        assert_eq!(
            *stages.lock().unwrap(),
            [
                (DownloadStage::Fetching, 0.0),
                (DownloadStage::Downloading, 0.0),
                (DownloadStage::Downloading, 0.5),
                (DownloadStage::Finishing, 0.0),
            ]
        );
    }

    #[tokio::test]
    async fn a_failed_download_is_recorded_and_cleaned_up_then_retried() {
        let world = World::new(true);
        let (video, _) = begin_download(&world.videos, MONK).await.unwrap();

        let err = download_video(&world.context(), video.id, &|_, _| {})
            .await
            .unwrap_err();

        assert!(matches!(err, DomainError::Unavailable(_)));
        let failed = world.videos.get(video.id).await.unwrap();
        assert_eq!(failed.status, VideoStatus::Failed);
        assert!(failed.import_error.unwrap().contains("unavailable"));
        assert_eq!(*world.store.removed.lock().unwrap(), [video.id]);

        let retried = retry_download(&world.videos, video.id).await.unwrap();
        assert_eq!(retried.status, VideoStatus::Importing);
        assert!(retried.import_error.is_none());
        assert!(retry_download(&world.videos, video.id).await.is_err());
    }

    #[tokio::test]
    async fn a_retried_download_does_not_repeat_its_chapters() {
        let world = World::new(false);
        let (video, _) = begin_download(&world.videos, MONK).await.unwrap();
        for _ in 0..2 {
            download_video(&world.context(), video.id, &|_, _| {})
                .await
                .unwrap();
        }
        assert_eq!(world.markers.list_markers(video.id).await.unwrap().len(), 2);
    }

    #[test]
    fn youtube_addresses_give_their_video_id() {
        for address in [
            "https://www.youtube.com/watch?v=7A12z9S4F8U&list=RD7A12z9S4F8U&start_radio=1",
            "https://youtube.com/watch?feature=share&v=7A12z9S4F8U",
            "http://m.youtube.com/watch?v=7A12z9S4F8U",
            "https://music.youtube.com/watch?v=7A12z9S4F8U",
            "https://youtu.be/7A12z9S4F8U?t=42",
            "youtu.be/7A12z9S4F8U",
            "https://www.youtube.com/shorts/7A12z9S4F8U",
            "https://www.youtube.com/embed/7A12z9S4F8U?start=3",
            "  www.youtube.com/watch?v=7A12z9S4F8U  ",
        ] {
            assert_eq!(
                youtube_id(address).as_deref(),
                Some("7A12z9S4F8U"),
                "{address}"
            );
        }
    }

    #[test]
    fn other_addresses_are_not_youtube_videos() {
        for address in [
            "https://vimeo.com/12345678",
            "https://www.youtube.com/playlist?list=RD7A12z9S4F8U",
            "https://www.youtube.com/watch?v=short",
            "https://evil.example/watch?v=7A12z9S4F8U",
            "https://www.youtube.com.evil.example/watch?v=7A12z9S4F8U",
            "7A12z9S4F8U",
            "",
        ] {
            assert_eq!(youtube_id(address), None, "{address}");
        }
    }
}
