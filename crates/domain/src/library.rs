//! Use cases of the video library.

use std::path::Path;

use chrono::Utc;

use crate::{
    DomainError, MediaPreparer, MediaProbe, MediaStore, PreparationInput, Result, ThumbnailSprite,
    Video, VideoId, VideoRepository, VideoSource, VideoStatus, guess_song,
};

/// Imports a video file from this computer: checks that it is a readable video, places it in
/// the library and records it, ready to play. Without a `title`, the song's title, artist and
/// year are guessed from the file name ("Chopin - Nocturne Op. 9 No. 2.mp4").
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
    if title.is_none() {
        video.apply_song(guess_song(default_title));
    }
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

/// Guesses the song's artist and year from the title of the videos that have neither, once
/// for libraries made before videos had them. Returns how many videos it filled in.
pub async fn guess_missing_songs(videos: &dyn VideoRepository) -> Result<usize> {
    let mut filled = 0;
    for mut video in videos.list().await? {
        if video.artist.is_some() || video.year.is_some() {
            continue;
        }
        let song = guess_song(&video.title);
        if song.artist.is_none() && song.year.is_none() {
            continue;
        }
        video.apply_song(song);
        videos.update(&video).await?;
        filled += 1;
    }
    Ok(filled)
}

/// The steps of preparing a video for navigation, in the order they run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreparationStage {
    Peaks,
    Attacks,
    SeekProxy,
    Thumbnails,
}

/// Prepares a video's media for instant navigation. The audio parts come first because they
/// take seconds: the waveform peaks and the attacks. Then the seek proxy, which takes most of
/// the time, and the thumbnails drawn from its keyframes. Each result is saved as soon as it
/// exists, so the timeline can use it straight away and an interruption keeps the finished
/// parts; parts already made are skipped. A failure is recorded on the video
/// (`preparation_error`) before being returned.
pub async fn prepare_video(
    videos: &dyn VideoRepository,
    store: &dyn MediaStore,
    preparer: &dyn MediaPreparer,
    id: VideoId,
    progress: &(dyn Fn(PreparationStage, f64) + Send + Sync),
) -> Result<Video> {
    let mut video = videos.get(id).await?;
    match prepare_missing_parts(&mut video, videos, store, preparer, progress).await {
        Ok(()) => Ok(video),
        Err(err) => {
            video.preparation_error = Some(err.to_string());
            video.updated_at = Utc::now();
            videos.update(&video).await?;
            Err(err)
        }
    }
}

async fn prepare_missing_parts(
    video: &mut Video,
    videos: &dyn VideoRepository,
    store: &dyn MediaStore,
    preparer: &dyn MediaPreparer,
    progress: &(dyn Fn(PreparationStage, f64) + Send + Sync),
) -> Result<()> {
    let original = store.resolve(
        video
            .media_file
            .as_deref()
            .ok_or_else(|| DomainError::Invalid(format!("video {} has no media", video.id)))?,
    )?;
    let input = PreparationInput {
        duration_ms: video.duration_ms.unwrap_or(0),
        fps: video.fps.unwrap_or(30.0),
        width: video.width.unwrap_or(0),
        height: video.height.unwrap_or(0),
    };

    if video.peaks_file.is_none() {
        let (relative, path) = store.derived(video.id, "peaks.json").await?;
        progress(PreparationStage::Peaks, 0.0);
        preparer.make_peaks(&original, &path).await?;
        video.peaks_file = Some(relative);
        save(video, videos).await?;
    }

    if video.attacks_file.is_none() {
        let (relative, path) = store.derived(video.id, "attacks.json").await?;
        progress(PreparationStage::Attacks, 0.0);
        preparer.make_attacks(&original, &path).await?;
        video.attacks_file = Some(relative);
        save(video, videos).await?;
    }

    let proxy = match &video.proxy_file {
        Some(relative) => store.resolve(relative)?,
        None => {
            let (relative, path) = store.derived(video.id, "proxy.mp4").await?;
            progress(PreparationStage::SeekProxy, 0.0);
            preparer
                .make_seek_proxy(&original, &path, input, &|done| {
                    progress(PreparationStage::SeekProxy, done)
                })
                .await?;
            video.proxy_file = Some(relative);
            save(video, videos).await?;
            path
        }
    };

    if video.thumbnails_file.is_none() {
        let (relative, path) = store.derived(video.id, "thumbnails.jpg").await?;
        progress(PreparationStage::Thumbnails, 0.0);
        let sprite = ThumbnailSprite::for_video(input.duration_ms, input.width, input.height);
        preparer.make_thumbnails(&proxy, &path, sprite).await?;
        video.thumbnails_file = Some(relative);
        save(video, videos).await?;
    }
    Ok(())
}

async fn save(video: &mut Video, videos: &dyn VideoRepository) -> Result<()> {
    video.updated_at = Utc::now();
    videos.update(video).await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;

    use super::*;
    use crate::MediaInfo;
    use crate::fakes::{Probe, Store, Videos, video_info};

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
        // The song comes from the file name ("Monk - Confessin<random>.mp4").
        assert!(video.title.starts_with("Confessin"));
        assert_eq!(video.artist.as_deref(), Some("Monk"));
        assert_eq!(video.duration_ms, Some(141_154));
        assert_eq!(video.fps, Some(60.0));
        assert_eq!(
            video.media_file.as_deref(),
            Some(format!("media/{}/original.mp4", video.id).as_str())
        );
        assert_eq!(videos.list().await.unwrap(), vec![video]);
    }

    #[tokio::test]
    async fn guesses_the_missing_songs_once_without_touching_the_others() {
        let videos = Videos::default();
        let mut old = Video::new_importing(
            VideoSource::Youtube,
            "Thelonious Monk - Dinah 1965 [Stride Piano Synthesia]",
            None,
        )
        .unwrap();
        old.status = VideoStatus::Ready;
        let mut edited = Video::new_importing(VideoSource::File, "Nocturne - Live", None).unwrap();
        edited.year = Some(1830);
        let plain = Video::new_importing(VideoSource::File, "clip", None).unwrap();
        for video in [&old, &edited, &plain] {
            videos.insert(video).await.unwrap();
        }

        assert_eq!(guess_missing_songs(&videos).await.unwrap(), 1);
        let old = videos.get(old.id).await.unwrap();
        assert_eq!(
            (old.title.as_str(), old.artist.as_deref(), old.year),
            ("Dinah", Some("Thelonious Monk"), Some(1965))
        );
        assert_eq!(
            videos.get(edited.id).await.unwrap().title,
            "Nocturne - Live"
        );
        assert_eq!(videos.get(plain.id).await.unwrap().title, "clip");
        assert_eq!(guess_missing_songs(&videos).await.unwrap(), 0);
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

    /// Records what it was asked to make; can fail the seek proxy.
    #[derive(Default)]
    struct Preparer {
        fail_proxy: bool,
        made: Mutex<Vec<String>>,
        sprite: Mutex<Option<ThumbnailSprite>>,
    }

    #[async_trait]
    impl MediaPreparer for Preparer {
        async fn make_seek_proxy(
            &self,
            _: &Path,
            target: &Path,
            input: PreparationInput,
            progress: &(dyn Fn(f64) + Send + Sync),
        ) -> Result<()> {
            if self.fail_proxy {
                return Err(DomainError::Invalid(
                    "no encoder could make the proxy".into(),
                ));
            }
            assert_eq!(input.fps, 60.0);
            progress(0.5);
            self.made.lock().unwrap().push(target.display().to_string());
            Ok(())
        }
        async fn make_thumbnails(
            &self,
            source: &Path,
            target: &Path,
            sprite: ThumbnailSprite,
        ) -> Result<()> {
            assert!(
                source.ends_with("proxy.mp4"),
                "thumbnails come from the proxy"
            );
            *self.sprite.lock().unwrap() = Some(sprite);
            self.made.lock().unwrap().push(target.display().to_string());
            Ok(())
        }
        async fn make_peaks(&self, source: &Path, target: &Path) -> Result<()> {
            assert!(
                source.ends_with("original.mp4"),
                "peaks come from the original"
            );
            self.made.lock().unwrap().push(target.display().to_string());
            Ok(())
        }
        async fn make_attacks(&self, source: &Path, target: &Path) -> Result<()> {
            assert!(
                source.ends_with("original.mp4"),
                "attacks come from the original"
            );
            self.made.lock().unwrap().push(target.display().to_string());
            Ok(())
        }
    }

    async fn imported(videos: &Videos) -> Video {
        let file = existing_file();
        import_local_video(
            videos,
            &Probe(video_info()),
            &Store::default(),
            file.path(),
            None,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn preparing_makes_and_records_every_part_in_order() {
        let videos = Videos::default();
        let video = imported(&videos).await;
        assert!(video.needs_preparation());
        let preparer = Preparer::default();
        let stages = Mutex::new(Vec::new());

        let prepared = prepare_video(
            &videos,
            &Store::default(),
            &preparer,
            video.id,
            &|stage, done| {
                stages.lock().unwrap().push((stage, done));
            },
        )
        .await
        .unwrap();

        assert!(!prepared.needs_preparation());
        assert_eq!(
            prepared.proxy_file.as_deref(),
            Some(format!("media/{}/proxy.mp4", video.id).as_str())
        );
        assert!(
            prepared
                .thumbnails_file
                .unwrap()
                .ends_with("thumbnails.jpg")
        );
        assert!(prepared.peaks_file.unwrap().ends_with("peaks.json"));
        assert!(prepared.attacks_file.unwrap().ends_with("attacks.json"));
        assert_eq!(preparer.made.lock().unwrap().len(), 4);
        assert_eq!(preparer.sprite.lock().unwrap().unwrap().count, 142);
        assert_eq!(
            *stages.lock().unwrap(),
            [
                (PreparationStage::Peaks, 0.0),
                (PreparationStage::Attacks, 0.0),
                (PreparationStage::SeekProxy, 0.0),
                (PreparationStage::SeekProxy, 0.5),
                (PreparationStage::Thumbnails, 0.0),
            ]
        );
        assert!(!videos.get(video.id).await.unwrap().needs_preparation());
    }

    #[tokio::test]
    async fn parts_already_made_are_skipped() {
        let videos = Videos::default();
        let mut video = imported(&videos).await;
        video.proxy_file = Some(format!("media/{}/proxy.mp4", video.id));
        videos.update(&video).await.unwrap();
        let preparer = Preparer::default();

        prepare_video(&videos, &Store::default(), &preparer, video.id, &|_, _| {})
            .await
            .unwrap();

        let made = preparer.made.lock().unwrap();
        assert_eq!(made.len(), 3);
        assert!(!made.iter().any(|path| path.ends_with("proxy.mp4")));
    }

    #[tokio::test]
    async fn videos_prepared_before_attacks_existed_only_get_their_attacks() {
        let videos = Videos::default();
        let mut video = imported(&videos).await;
        for (file, name) in [
            (&mut video.proxy_file, "proxy.mp4"),
            (&mut video.thumbnails_file, "thumbnails.jpg"),
            (&mut video.peaks_file, "peaks.json"),
        ] {
            *file = Some(format!("media/{}/{name}", video.id));
        }
        videos.update(&video).await.unwrap();
        assert!(video.needs_preparation());
        let preparer = Preparer::default();

        prepare_video(&videos, &Store::default(), &preparer, video.id, &|_, _| {})
            .await
            .unwrap();

        let made = preparer.made.lock().unwrap();
        assert_eq!(made.len(), 1);
        assert!(made[0].ends_with("attacks.json"));
    }

    #[tokio::test]
    async fn a_failure_is_recorded_on_the_video() {
        let videos = Videos::default();
        let video = imported(&videos).await;
        let preparer = Preparer {
            fail_proxy: true,
            ..Preparer::default()
        };

        let err = prepare_video(&videos, &Store::default(), &preparer, video.id, &|_, _| {})
            .await
            .unwrap_err();

        assert!(matches!(err, DomainError::Invalid(_)));
        let saved = videos.get(video.id).await.unwrap();
        assert!(
            saved
                .preparation_error
                .as_deref()
                .unwrap()
                .contains("no encoder")
        );
        assert!(
            !saved.needs_preparation(),
            "a failed video is not retried on its own"
        );
    }
}
