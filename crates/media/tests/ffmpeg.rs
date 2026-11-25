//! Runs the real ffmpeg and ffprobe. Skipped when they are not installed (`pnpm tools`),
//! unless `SEGNO_REQUIRE_FFMPEG` is set, as in CI.

use std::path::Path;

use segno_domain::{DomainError, MediaProbe};
use segno_media::{FfprobeProbe, MediaTools, dev_tools_dir};

async fn tools() -> Option<MediaTools> {
    let tools = MediaTools::discover(&[dev_tools_dir()]).await;
    if tools.is_none() {
        assert!(
            std::env::var_os("SEGNO_REQUIRE_FFMPEG").is_none(),
            "ffmpeg is required but was not found"
        );
        eprintln!("skipped: ffmpeg not found (run `pnpm tools`)");
    }
    tools
}

/// A one-second 320×240 clip at 25 fps with a 440 Hz tone, encoded with the LGPL-friendly
/// OpenH264 encoder.
fn make_clip(tools: &MediaTools, file: &Path) {
    let status = std::process::Command::new(&tools.ffmpeg)
        .args(["-v", "error", "-y"])
        .args([
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x240:rate=25:duration=1",
        ])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=1"])
        .args(["-c:v", "libopenh264", "-c:a", "aac", "-shortest"])
        .arg(file)
        .status()
        .unwrap();
    assert!(status.success(), "ffmpeg could not make the test clip");
}

#[tokio::test]
async fn discovers_the_tools() {
    let Some(tools) = tools().await else { return };
    assert!(!tools.version.is_empty());
}

#[tokio::test]
async fn probes_a_clip_made_with_ffmpeg() {
    let Some(tools) = tools().await else { return };
    let dir = tempfile::tempdir().unwrap();
    let clip = dir.path().join("clip.mp4");
    make_clip(&tools, &clip);

    let info = FfprobeProbe::new(tools).probe(&clip).await.unwrap();

    assert_eq!((info.width, info.height), (Some(320), Some(240)));
    assert_eq!(info.fps, Some(25.0));
    assert!(
        (900..=1100).contains(&info.duration_ms),
        "{}",
        info.duration_ms
    );
    assert_eq!(info.video_codec.as_deref(), Some("h264"));
    assert_eq!(info.audio_codec.as_deref(), Some("aac"));
}

#[tokio::test]
async fn text_files_are_not_media() {
    let Some(tools) = tools().await else { return };
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.mp4");
    std::fs::write(&file, "just some text").unwrap();

    let err = FfprobeProbe::new(tools).probe(&file).await.unwrap_err();
    assert!(matches!(err, DomainError::Invalid(_)), "{err:?}");
}
