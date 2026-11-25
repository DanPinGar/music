use std::path::Path;

use async_trait::async_trait;
use segno_domain::{DomainError, MediaInfo, MediaProbe, Result};
use serde::Deserialize;

use crate::MediaTools;
use crate::tools::command;

/// Reads media facts with ffprobe.
#[derive(Debug, Clone)]
pub struct FfprobeProbe {
    tools: MediaTools,
}

impl FfprobeProbe {
    pub fn new(tools: MediaTools) -> Self {
        Self { tools }
    }
}

#[async_trait]
impl MediaProbe for FfprobeProbe {
    async fn probe(&self, file: &Path) -> Result<MediaInfo> {
        let output = command(&self.tools.ffprobe)
            .args(["-v", "error", "-print_format", "json"])
            .args(["-show_format", "-show_streams"])
            .arg(file)
            .output()
            .await
            .map_err(|err| DomainError::Unavailable(format!("ffprobe could not run: {err}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let reason = stderr.lines().last().unwrap_or("unknown error").trim();
            return Err(DomainError::Invalid(format!(
                "{} is not a media file: {reason}",
                file.display()
            )));
        }
        parse(&output.stdout)
    }
}

#[derive(Deserialize)]
struct Report {
    #[serde(default)]
    streams: Vec<Stream>,
    format: Option<Format>,
}

#[derive(Deserialize)]
struct Stream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    duration: Option<String>,
    #[serde(default)]
    disposition: Disposition,
}

#[derive(Deserialize, Default)]
struct Disposition {
    #[serde(default)]
    attached_pic: u8,
}

#[derive(Deserialize)]
struct Format {
    duration: Option<String>,
}

/// Turns `ffprobe -print_format json -show_format -show_streams` output into [`MediaInfo`].
fn parse(json: &[u8]) -> Result<MediaInfo> {
    let report: Report = serde_json::from_slice(json)
        .map_err(|err| DomainError::Invalid(format!("unexpected ffprobe output: {err}")))?;
    // Cover art in audio files is a "video" stream too; it is not a video.
    let video = report.streams.iter().find(|stream| {
        stream.codec_type.as_deref() == Some("video") && stream.disposition.attached_pic == 0
    });
    let audio = report
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("audio"));
    let seconds = report
        .format
        .as_ref()
        .and_then(|format| format.duration.as_deref())
        .or_else(|| video.and_then(|stream| stream.duration.as_deref()))
        .and_then(|duration| duration.parse::<f64>().ok())
        .ok_or_else(|| DomainError::Invalid("the media has no known duration".into()))?;

    Ok(MediaInfo {
        duration_ms: (seconds * 1000.0).round() as i64,
        fps: video.and_then(|stream| {
            rate(stream.avg_frame_rate.as_deref()).or_else(|| rate(stream.r_frame_rate.as_deref()))
        }),
        width: video.and_then(|stream| stream.width),
        height: video.and_then(|stream| stream.height),
        video_codec: video.and_then(|stream| stream.codec_name.clone()),
        audio_codec: audio.and_then(|stream| stream.codec_name.clone()),
    })
}

/// A frame rate written as a fraction: "30000/1001" → 29.97…; "0/0" → None.
fn rate(value: Option<&str>) -> Option<f64> {
    let (numerator, denominator) = value?.split_once('/')?;
    let numerator: f64 = numerator.parse().ok()?;
    let denominator: f64 = denominator.parse().ok()?;
    (numerator > 0.0 && denominator > 0.0).then(|| numerator / denominator)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed ffprobe output for the 1080p60 test video of the transcription prototype.
    const MONK: &str = r#"{
      "streams": [
        {"index": 0, "codec_name": "h264", "codec_type": "video", "width": 1920, "height": 1080,
         "r_frame_rate": "60/1", "avg_frame_rate": "60/1", "duration": "141.083333",
         "disposition": {"attached_pic": 0}},
        {"index": 1, "codec_name": "aac", "codec_type": "audio",
         "r_frame_rate": "0/0", "avg_frame_rate": "0/0", "duration": "141.154104",
         "disposition": {"attached_pic": 0}}
      ],
      "format": {"format_name": "mov,mp4,m4a,3gp,3g2,mj2", "duration": "141.154104"}
    }"#;

    #[test]
    fn reads_a_youtube_download() {
        let info = parse(MONK.as_bytes()).unwrap();
        assert_eq!(
            info,
            MediaInfo {
                duration_ms: 141_154,
                fps: Some(60.0),
                width: Some(1920),
                height: Some(1080),
                video_codec: Some("h264".into()),
                audio_codec: Some("aac".into()),
            }
        );
    }

    #[test]
    fn cover_art_is_not_video() {
        let json = r#"{"streams": [
            {"codec_type": "audio", "codec_name": "mp3"},
            {"codec_type": "video", "codec_name": "mjpeg", "width": 600, "height": 600,
             "avg_frame_rate": "0/0", "disposition": {"attached_pic": 1}}
          ], "format": {"duration": "200.5"}}"#;
        let info = parse(json.as_bytes()).unwrap();
        assert!(!info.has_video());
        assert_eq!(info.duration_ms, 200_500);
    }

    #[test]
    fn reads_fractional_frame_rates() {
        assert!((rate(Some("30000/1001")).unwrap() - 29.97).abs() < 0.001);
        assert_eq!(rate(Some("25/1")), Some(25.0));
        for bad in ["0/0", "60", "a/b", "60/0"] {
            assert_eq!(rate(Some(bad)), None, "{bad}");
        }
    }

    #[test]
    fn needs_a_duration() {
        let json = r#"{"streams": [], "format": {}}"#;
        assert!(matches!(
            parse(json.as_bytes()),
            Err(DomainError::Invalid(_))
        ));
        assert!(matches!(parse(b"not json"), Err(DomainError::Invalid(_))));
    }
}
