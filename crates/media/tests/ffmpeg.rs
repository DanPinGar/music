//! Runs the real ffmpeg and ffprobe. Skipped when they are not installed (`pnpm tools`),
//! unless `SEGNO_REQUIRE_FFMPEG` is set, as in CI.

use std::path::Path;
use std::sync::Mutex;

use segno_domain::{DomainError, MediaPreparer, MediaProbe, PreparationInput, ThumbnailSprite};
use segno_media::{FfmpegPreparer, FfprobeProbe, MediaTools, dev_tools_dir};

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

/// Seconds of the keyframes of a file's video stream.
fn keyframes(tools: &MediaTools, file: &Path) -> Vec<f64> {
    let output = std::process::Command::new(&tools.ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v",
            "-skip_frame",
            "nokey",
        ])
        .args(["-show_entries", "frame=pts_time", "-of", "csv=p=0"])
        .arg(file)
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().trim_end_matches(',').parse().ok())
        .collect()
}

#[tokio::test]
async fn the_seek_proxy_has_frequent_keyframes_and_keeps_the_audio() {
    let Some(tools) = tools().await else { return };
    let dir = tempfile::tempdir().unwrap();
    let (clip, proxy) = (dir.path().join("clip.mp4"), dir.path().join("proxy.mp4"));
    make_clip(&tools, &clip);
    let input = PreparationInput {
        duration_ms: 1_000,
        fps: 25.0,
        width: 320,
        height: 240,
    };
    let reported = Mutex::new(Vec::new());

    let preparer = FfmpegPreparer::new(tools.clone());
    preparer
        .make_seek_proxy(&clip, &proxy, input, &|done| {
            reported.lock().unwrap().push(done)
        })
        .await
        .unwrap();

    let gaps: Vec<f64> = keyframes(&tools, &proxy)
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .collect();
    // 12 frames at 25 fps: the most that fits in half a second.
    assert!(
        !gaps.is_empty() && gaps.iter().all(|gap| *gap <= 0.48 + 1e-6),
        "{gaps:?}"
    );
    let info = FfprobeProbe::new(tools).probe(&proxy).await.unwrap();
    assert_eq!(info.audio_codec.as_deref(), Some("aac"));
    assert_eq!(info.video_codec.as_deref(), Some("h264"));
    assert!(
        !proxy.with_extension("mp4.part").exists(),
        "no leftover part file"
    );
    assert!(reported.lock().unwrap().last().copied().unwrap_or(0.0) > 0.8);
    println!("encoder: {}", preparer.encoder_in_use().await.unwrap());
}

#[tokio::test]
async fn thumbnails_form_a_sprite_of_one_tile_per_second() {
    let Some(tools) = tools().await else { return };
    let dir = tempfile::tempdir().unwrap();
    let (clip, sprite_file) = (dir.path().join("clip.mp4"), dir.path().join("thumbs.jpg"));
    make_clip(&tools, &clip);
    let sprite = ThumbnailSprite::for_video(1_000, 320, 240);

    FfmpegPreparer::new(tools.clone())
        .make_thumbnails(&clip, &sprite_file, sprite)
        .await
        .unwrap();

    let bytes = std::fs::read(&sprite_file).unwrap();
    assert_eq!(&bytes[..2], [0xFF, 0xD8], "a JPEG file");
    let (width, height) = jpeg_size(&bytes);
    assert_eq!(
        (width, height),
        (
            sprite.columns * sprite.tile_width,
            sprite.rows * sprite.tile_height
        )
    );
}

#[tokio::test]
async fn peaks_follow_the_audio() {
    let Some(tools) = tools().await else { return };
    let dir = tempfile::tempdir().unwrap();
    let (clip, peaks_file) = (dir.path().join("clip.mp4"), dir.path().join("peaks.json"));
    make_clip(&tools, &clip);

    FfmpegPreparer::new(tools)
        .make_peaks(&clip, &peaks_file)
        .await
        .unwrap();

    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&peaks_file).unwrap()).unwrap();
    assert_eq!(json["peaks_per_second"], 100);
    let peaks = json["peaks"].as_array().unwrap();
    // One second of audio: about 100 slots of (min, max).
    assert!((190..=210).contains(&peaks.len()), "{}", peaks.len());
    // lavfi's sine source plays at 1/8 of full scale: 127 / 8 ≈ 16.
    let loudest = peaks
        .iter()
        .map(|p| p.as_i64().unwrap().abs())
        .max()
        .unwrap();
    assert!((14..=18).contains(&loudest), "{loudest}");
}

/// Three seconds of plucked notes (three harmonics, decaying) starting at known times, encoded
/// as AAC like the audio of a downloaded video.
fn make_notes(tools: &MediaTools, file: &Path, notes: &[(f64, f64)]) {
    let expression = notes
        .iter()
        .map(|(start, frequency)| {
            let phase = format!("PI*{frequency}*(t-{start})");
            format!(
                "gte(t,{start})*exp(-(t-{start})*4)*\
                 (sin(2*{phase})+0.5*sin(4*{phase})+0.3*sin(6*{phase}))"
            )
        })
        .collect::<Vec<_>>()
        .join("+");
    let status = std::process::Command::new(&tools.ffmpeg)
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg(format!("aevalsrc='0.2*({expression})':s=48000:d=3"))
        .args(["-c:a", "aac"])
        .arg(file)
        .status()
        .unwrap();
    assert!(status.success(), "ffmpeg could not make the notes");
}

#[tokio::test]
async fn attacks_are_found_where_the_notes_start() {
    let Some(tools) = tools().await else { return };
    let dir = tempfile::tempdir().unwrap();
    let (clip, attacks_file) = (
        dir.path().join("notes.mp4"),
        dir.path().join("attacks.json"),
    );
    let notes = [(0.5, 440.0), (1.1, 523.25), (1.7, 659.25), (2.3, 392.0)];
    make_notes(&tools, &clip, &notes);

    FfmpegPreparer::new(tools)
        .make_attacks(&clip, &attacks_file)
        .await
        .unwrap();

    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&attacks_file).unwrap()).unwrap();
    assert_eq!(json["version"], 1);
    let times = json["times"].as_array().unwrap();
    assert_eq!(times.len(), notes.len(), "{json}");
    for (time, (start, _)) in times.iter().zip(&notes) {
        let time = time.as_f64().unwrap();
        assert!((time - start).abs() <= 0.02, "{time} for {start}: {json}");
    }
    let strengths = json["strengths"].as_array().unwrap();
    assert_eq!(strengths.len(), times.len());
    assert!(
        strengths
            .iter()
            .all(|s| (0.0..=1.0).contains(&s.as_f64().unwrap()))
    );
}

#[tokio::test]
async fn videos_without_audio_have_empty_peaks_and_attacks() {
    let Some(tools) = tools().await else { return };
    let dir = tempfile::tempdir().unwrap();
    let clip = dir.path().join("silent.mp4");
    let status = std::process::Command::new(&tools.ffmpeg)
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg("testsrc2=size=320x240:rate=25:duration=1")
        .args(["-c:v", "libopenh264"])
        .arg(&clip)
        .status()
        .unwrap();
    assert!(status.success());
    let preparer = FfmpegPreparer::new(tools);
    let (peaks_file, attacks_file) = (
        dir.path().join("peaks.json"),
        dir.path().join("attacks.json"),
    );

    preparer.make_peaks(&clip, &peaks_file).await.unwrap();
    preparer.make_attacks(&clip, &attacks_file).await.unwrap();

    let read = |file: &Path| -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap()
    };
    assert_eq!(read(&peaks_file)["peaks"], serde_json::json!([]));
    assert_eq!(read(&attacks_file)["times"], serde_json::json!([]));
}

/// Width and height from a JPEG's start-of-frame marker.
fn jpeg_size(bytes: &[u8]) -> (u32, u32) {
    let mut i = 2;
    while i + 9 < bytes.len() {
        let (marker, length) = (
            bytes[i + 1],
            usize::from(bytes[i + 2]) << 8 | usize::from(bytes[i + 3]),
        );
        if (0xC0..=0xC3).contains(&marker) {
            let height = u32::from(bytes[i + 5]) << 8 | u32::from(bytes[i + 6]);
            let width = u32::from(bytes[i + 7]) << 8 | u32::from(bytes[i + 8]);
            return (width, height);
        }
        i += 2 + length;
    }
    panic!("no start-of-frame marker");
}
