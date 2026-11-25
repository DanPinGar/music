//! Use cases of the video library.

use std::path::Path;

use crate::{
    DomainError, MediaProbe, MediaStore, Result, Video, VideoRepository, VideoSource, VideoStatus,
};

/// Imports a video file from this computer: checks that it is a readable video, places it in
/// the library and records it, ready to play.
pub async fn import_local_video(
    videos: &dyn VideoRepository,
    probe: &dyn MediaProbe,
    store: &dyn MediaStore,
    file: &Path,
    title: Option<&str>,
) -> Result<Video> {
    if !file.is_file() {
        return Err(DomainError::Invalid(format!(
            "not a file: {}",
            file.display()
        )));
    }
    let info = probe.probe(file).await?;
    if !info.has_video() {
        return Err(DomainError::Invalid(format!(
            "{} has no video stream",
            file.display()
        )));
    }

    let default_title = file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("video");
    let mut video = Video::new_importing(VideoSource::File, title.unwrap_or(default_title), None)?;
    video.apply_media_info(&info);
    video.media_file = Some(store.store_original(video.id, file).await?);
    video.status = VideoStatus::Ready;

    if let Err(err) = videos.insert(&video).await {
        // Do not leave orphan media behind; the insert error is the one worth reporting.
        let _ = store.remove_all(video.id).await;
        return Err(err);
    }
    Ok(video)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use async_trait::async_trait;

    use super::*;
    use crate::{MediaInfo, VideoId};

    #[derive(Default)]
    struct Videos {
        saved: Mutex<Vec<Video>>,
        fail: bool,
    }

    #[async_trait]
    impl VideoRepository for Videos {
        async fn list(&self) -> Result<Vec<Video>> {
            Ok(self.saved.lock().unwrap().clone())
        }
        async fn get(&self, id: VideoId) -> Result<Video> {
            self.saved
                .lock()
                .unwrap()
                .iter()
                .find(|v| v.id == id)
                .cloned()
                .ok_or(DomainError::NotFound {
                    entity: "video",
                    id: id.to_string(),
                })
        }
        async fn insert(&self, video: &Video) -> Result<()> {
            if self.fail {
                return Err(DomainError::storage(std::io::Error::other("disk full")));
            }
            self.saved.lock().unwrap().push(video.clone());
            Ok(())
        }
    }

    struct Probe(MediaInfo);

    #[async_trait]
    impl MediaProbe for Probe {
        async fn probe(&self, _: &Path) -> Result<MediaInfo> {
            Ok(self.0.clone())
        }
    }

    #[derive(Default)]
    struct Store {
        removed: Mutex<Vec<VideoId>>,
    }

    #[async_trait]
    impl MediaStore for Store {
        async fn store_original(&self, video: VideoId, _: &Path) -> Result<String> {
            Ok(format!("media/{video}/original.mp4"))
        }
        fn resolve(&self, relative: &str) -> Result<PathBuf> {
            Ok(PathBuf::from(relative))
        }
        async fn remove_all(&self, video: VideoId) -> Result<()> {
            self.removed.lock().unwrap().push(video);
            Ok(())
        }
    }

    fn video_info() -> MediaInfo {
        MediaInfo {
            duration_ms: 141_154,
            fps: Some(60.0),
            width: Some(1920),
            height: Some(1080),
            video_codec: Some("h264".into()),
            audio_codec: Some("aac".into()),
        }
    }

    fn existing_file() -> tempfile::NamedTempFile {
        tempfile::Builder::new()
            .prefix("Monk - Confessin")
            .suffix(".mp4")
            .tempfile()
            .unwrap()
    }

    #[tokio::test]
    async fn imports_a_video_ready_to_play() {
        let (videos, store, file) = (Videos::default(), Store::default(), existing_file());
        let video = import_local_video(&videos, &Probe(video_info()), &store, file.path(), None)
            .await
            .unwrap();

        assert_eq!(video.status, VideoStatus::Ready);
        assert_eq!(video.source, VideoSource::File);
        assert!(video.title.starts_with("Monk - Confessin"));
        assert_eq!(video.duration_ms, Some(141_154));
        assert_eq!(video.fps, Some(60.0));
        assert_eq!(
            video.media_file.as_deref(),
            Some(format!("media/{}/original.mp4", video.id).as_str())
        );
        assert_eq!(videos.list().await.unwrap(), vec![video]);
    }

    #[tokio::test]
    async fn uses_the_given_title() {
        let (videos, store, file) = (Videos::default(), Store::default(), existing_file());
        let video = import_local_video(
            &videos,
            &Probe(video_info()),
            &store,
            file.path(),
            Some("I'm Confessin'"),
        )
        .await
        .unwrap();
        assert_eq!(video.title, "I'm Confessin'");
    }

    #[tokio::test]
    async fn rejects_missing_files() {
        let (videos, store) = (Videos::default(), Store::default());
        let err = import_local_video(
            &videos,
            &Probe(video_info()),
            &store,
            Path::new("does/not/exist.mp4"),
            None,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, DomainError::Invalid(_)));
    }

    #[tokio::test]
    async fn rejects_files_without_video() {
        let (videos, store, file) = (Videos::default(), Store::default(), existing_file());
        let audio_only = MediaInfo {
            width: None,
            height: None,
            fps: None,
            ..video_info()
        };
        let err = import_local_video(&videos, &Probe(audio_only), &store, file.path(), None)
            .await
            .unwrap_err();
        assert!(matches!(err, DomainError::Invalid(_)));
        assert!(videos.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn removes_the_media_when_recording_fails() {
        let videos = Videos {
            fail: true,
            ..Videos::default()
        };
        let (store, file) = (Store::default(), existing_file());
        let err = import_local_video(&videos, &Probe(video_info()), &store, file.path(), None)
            .await
            .unwrap_err();
        assert!(matches!(err, DomainError::Storage(_)));
        assert_eq!(store.removed.lock().unwrap().len(), 1);
    }
}
