use std::ffi::OsString;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use segno_domain::{DomainError, MediaPreparer, PreparationInput, Result, ThumbnailSprite};
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::sync::Mutex;

use crate::MediaTools;
use crate::onsets::{self, Onset, OnsetDetector};
use crate::tools::command;

/// Most frames between two keyframes of the seek proxy. A seek decodes from the keyframe
/// before its target, so this bounds the work of any jump or frame step: about 1.6 ms a frame
/// at 1080p on the development laptop, which keeps frame steps of 60 fps videos at 17–27 ms
/// (p95) where a 30-frame interval measured 25–52 ms.
const MAX_KEYFRAME_FRAMES: u32 = 15;
/// ...and at most this long between them, for low frame rates.
const MAX_KEYFRAME_SECONDS: f64 = 0.5;
/// The proxy never exceeds this height (the original keeps full quality for analysis).
const MAX_PROXY_HEIGHT: u32 = 1080;
/// Waveform resolution, and the audio sample rate it is computed from.
const PEAKS_PER_SECOND: u32 = 100;
const PEAKS_SAMPLE_RATE: u32 = 8_000;

/// H.264 encoders for the seek proxy, best first. Hardware encoders keep the laptop cool; the
/// software OpenH264 encoder works everywhere (and keeps ffmpeg's licence LGPL, unlike x264).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoder {
    /// Intel Quick Sync, decoding on the GPU too.
    QuickSync,
    /// Windows Media Foundation: the GPU encoder of any vendor.
    MediaFoundation,
    OpenH264,
}

impl Encoder {
    const ALL: [Self; 3] = [Self::QuickSync, Self::MediaFoundation, Self::OpenH264];

    fn name(self) -> &'static str {
        match self {
            Self::QuickSync => "h264_qsv",
            Self::MediaFoundation => "h264_mf",
            Self::OpenH264 => "libopenh264",
        }
    }

    /// Arguments that go before `-i`.
    fn input_args(self, needs_scaling: bool) -> Vec<&'static str> {
        match self {
            Self::QuickSync if !needs_scaling => {
                vec!["-hwaccel", "qsv", "-hwaccel_output_format", "qsv"]
            }
            _ => vec![],
        }
    }

    fn video_args(self, height: u32) -> Vec<&'static str> {
        let scale = height > MAX_PROXY_HEIGHT;
        match self {
            Self::QuickSync if scale => vec![
                "-vf",
                "scale=-2:1080",
                "-c:v",
                "h264_qsv",
                "-preset",
                "faster",
                "-global_quality",
                "24",
            ],
            Self::QuickSync => vec![
                "-c:v",
                "h264_qsv",
                "-preset",
                "faster",
                "-global_quality",
                "24",
            ],
            Self::MediaFoundation => vec![
                "-vf",
                if scale {
                    "scale=-2:1080,format=nv12"
                } else {
                    "format=nv12"
                },
                "-c:v",
                "h264_mf",
                "-rate_control",
                "quality",
                "-quality",
                "60",
                "-hw_encoding",
                "1",
            ],
            Self::OpenH264 => {
                let mut args = vec!["-c:v", "libopenh264", "-b:v"];
                args.push(if height >= 1080 { "8M" } else { "5M" });
                if scale {
                    args.extend(["-vf", "scale=-2:1080"]);
                }
                args
            }
        }
    }
}

/// Prepares videos for navigation with ffmpeg.
#[derive(Debug)]
pub struct FfmpegPreparer {
    tools: MediaTools,
    /// The encoder that worked last time, tried first next time.
    preferred: Mutex<Option<Encoder>>,
}

impl FfmpegPreparer {
    pub fn new(tools: MediaTools) -> Self {
        Self {
            tools,
            preferred: Mutex::new(None),
        }
    }

    /// Name of the encoder that made the last proxy, if any did.
    pub async fn encoder_in_use(&self) -> Option<&'static str> {
        self.preferred.lock().await.map(Encoder::name)
    }

    async fn has_audio(&self, source: &Path) -> bool {
        let output = command(&self.tools.ffprobe)
            .args(["-v", "error", "-select_streams", "a"])
            .args(["-show_entries", "stream=index", "-of", "csv=p=0"])
            .arg(source)
            .output()
            .await;
        matches!(output, Ok(output) if output.status.success() && !output.stdout.trim_ascii().is_empty())
    }
}

#[async_trait]
impl MediaPreparer for FfmpegPreparer {
    async fn make_seek_proxy(
        &self,
        source: &Path,
        target: &Path,
        input: PreparationInput,
        progress: &(dyn Fn(f64) + Send + Sync),
    ) -> Result<()> {
        let part = part_path(target);
        let gop = keyframe_interval(input.fps).to_string();
        let preferred = *self.preferred.lock().await;
        let order = preferred
            .into_iter()
            .chain(Encoder::ALL.into_iter().filter(|e| Some(*e) != preferred));

        let mut failures = Vec::new();
        for encoder in order {
            let mut args: Vec<OsString> = ["-v", "error", "-nostats", "-progress", "pipe:1", "-y"]
                .map(OsString::from)
                .to_vec();
            args.extend(
                encoder
                    .input_args(input.height > MAX_PROXY_HEIGHT)
                    .into_iter()
                    .map(OsString::from),
            );
            args.extend([OsString::from("-i"), source.into()]);
            args.extend(["-map", "0:v:0", "-map", "0:a:0?"].map(OsString::from));
            args.extend(
                encoder
                    .video_args(input.height)
                    .into_iter()
                    .map(OsString::from),
            );
            args.extend(["-g", gop.as_str(), "-bf", "0"].map(OsString::from));
            args.extend(["-c:a", "aac", "-b:a", "192k"].map(OsString::from));
            args.extend(["-movflags", "+faststart", "-f", "mp4"].map(OsString::from));
            args.push(part.clone().into());

            match run_ffmpeg(&self.tools.ffmpeg, &args, input.duration_ms, progress).await {
                Ok(()) => {
                    tokio::fs::rename(&part, target)
                        .await
                        .map_err(DomainError::storage)?;
                    *self.preferred.lock().await = Some(encoder);
                    return Ok(());
                }
                Err(reason) => failures.push(format!("{}: {reason}", encoder.name())),
            }
        }
        let _ = tokio::fs::remove_file(&part).await;
        Err(DomainError::Invalid(format!(
            "no H.264 encoder could make the seek proxy ({})",
            failures.join("; ")
        )))
    }

    async fn make_thumbnails(
        &self,
        source: &Path,
        target: &Path,
        sprite: ThumbnailSprite,
    ) -> Result<()> {
        let part = part_path(target);
        // Only keyframes are decoded (fast: the proxy has one every ~0.5 s). Tile n is the first
        // keyframe at or after n intervals, so tiles stay anchored to their time: the error is
        // at most one keyframe gap and never accumulates. (The fps filter fails on inputs with
        // a single keyframe.)
        let interval = f64::from(sprite.interval_ms) / 1000.0;
        let filter = format!(
            "select='if(isnan(selected_n)\\,1\\,gte(t\\,selected_n*{interval}))',\
             scale={w}:{h},tile={cols}x{rows}",
            w = sprite.tile_width,
            h = sprite.tile_height,
            cols = sprite.columns,
            rows = sprite.rows,
        );
        let mut args: Vec<OsString> = ["-v", "error", "-y", "-skip_frame", "nokey", "-i"]
            .map(OsString::from)
            .to_vec();
        args.push(source.into());
        args.extend(["-an", "-vf", &filter, "-fps_mode", "passthrough"].map(OsString::from));
        args.extend(["-frames:v", "1", "-q:v", "4"].map(OsString::from));
        args.extend(["-c:v", "mjpeg", "-f", "image2", "-update", "1"].map(OsString::from));
        args.push(part.clone().into());
        run_ffmpeg(&self.tools.ffmpeg, &args, 0, &|_| {})
            .await
            .map_err(|reason| DomainError::Invalid(format!("thumbnails failed: {reason}")))?;
        tokio::fs::rename(&part, target)
            .await
            .map_err(DomainError::storage)
    }

    async fn make_peaks(&self, source: &Path, target: &Path) -> Result<()> {
        let peaks = if self.has_audio(source).await {
            decode_peaks(&self.tools.ffmpeg, source).await?
        } else {
            Vec::new()
        };
        write_json(
            target,
            &Peaks {
                version: 1,
                peaks_per_second: PEAKS_PER_SECOND,
                peaks,
            },
        )
        .await
    }

    async fn make_attacks(&self, source: &Path, target: &Path) -> Result<()> {
        let onsets = if self.has_audio(source).await {
            let mut detector = OnsetDetector::new();
            decode_audio(&self.tools.ffmpeg, source, onsets::SAMPLE_RATE, |samples| {
                detector.push(samples);
            })
            .await?;
            detector.finish()
        } else {
            Vec::new()
        };
        write_json(target, &Attacks::from(onsets.as_slice())).await
    }
}

/// Waveform file: `peaks` holds a minimum and a maximum per slot, scaled to -127…127.
#[derive(Serialize)]
struct Peaks {
    version: u32,
    peaks_per_second: u32,
    peaks: Vec<i8>,
}

/// Attacks file: `times` in seconds (to the millisecond), in order, and the `strengths` of the
/// same attacks, from 0 to 1.
#[derive(Serialize)]
struct Attacks {
    version: u32,
    times: Vec<f64>,
    strengths: Vec<f32>,
}

impl From<&[Onset]> for Attacks {
    fn from(onsets: &[Onset]) -> Self {
        Self {
            version: 1,
            times: onsets
                .iter()
                .map(|onset| (onset.seconds * 1000.0).round() / 1000.0)
                .collect(),
            strengths: onsets
                .iter()
                .map(|onset| (onset.strength * 100.0).round() / 100.0)
                .collect(),
        }
    }
}

/// Writes `value` as JSON to `target` atomically (through a part file).
async fn write_json(target: &Path, value: &impl Serialize) -> Result<()> {
    let json = serde_json::to_vec(value).map_err(DomainError::storage)?;
    let part = part_path(target);
    tokio::fs::write(&part, json)
        .await
        .map_err(DomainError::storage)?;
    tokio::fs::rename(&part, target)
        .await
        .map_err(DomainError::storage)
}

/// Frames between keyframes in the seek proxy (the encoder's `-g`): at most
/// `MAX_KEYFRAME_FRAMES`, and never more than `MAX_KEYFRAME_SECONDS` apart.
fn keyframe_interval(fps: f64) -> u32 {
    if fps.is_nan() || fps <= 0.0 {
        return MAX_KEYFRAME_FRAMES; // Unknown frame rate.
    }
    ((fps * MAX_KEYFRAME_SECONDS).floor() as u32).clamp(1, MAX_KEYFRAME_FRAMES)
}

fn part_path(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_owned();
    name.push(".part");
    PathBuf::from(name)
}

/// Runs ffmpeg; with `-progress pipe:1` in `args`, reports how far it got through
/// `duration_ms`. On failure returns the last lines ffmpeg wrote to stderr.
async fn run_ffmpeg(
    ffmpeg: &Path,
    args: &[OsString],
    duration_ms: i64,
    progress: &(dyn Fn(f64) + Send + Sync),
) -> std::result::Result<(), String> {
    let mut child = command(ffmpeg)
        .args(args)
        .spawn()
        .map_err(|err| format!("ffmpeg could not start: {err}"))?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    // Drain stderr concurrently so ffmpeg never blocks on a full pipe.
    let errors = tokio::spawn(async move {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text).await;
        text
    });

    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(micros) = line.strip_prefix("out_time_us=")
            && let Ok(micros) = micros.trim().parse::<i64>()
            && duration_ms > 0
        {
            progress((micros as f64 / 1000.0 / duration_ms as f64).clamp(0.0, 1.0));
        }
    }
    let status = child
        .wait()
        .await
        .map_err(|err| format!("ffmpeg did not finish: {err}"))?;
    let errors = errors.await.unwrap_or_default();
    if status.success() {
        Ok(())
    } else {
        let tail: Vec<&str> = errors.lines().rev().take(3).collect();
        Err(tail.into_iter().rev().collect::<Vec<_>>().join(" | "))
    }
}

/// Decodes the audio as mono 8 kHz floats and keeps the minimum and maximum of each slot.
async fn decode_peaks(ffmpeg: &Path, source: &Path) -> Result<Vec<i8>> {
    let slot = (PEAKS_SAMPLE_RATE / PEAKS_PER_SECOND) as usize;
    let mut peaks = Vec::new();
    let (mut low, mut high, mut filled) = (0.0f32, 0.0f32, 0usize);
    decode_audio(ffmpeg, source, PEAKS_SAMPLE_RATE, |samples| {
        for &sample in samples {
            low = low.min(sample);
            high = high.max(sample);
            filled += 1;
            if filled == slot {
                peaks.extend([quantize(low), quantize(high)]);
                (low, high, filled) = (0.0, 0.0, 0);
            }
        }
    })
    .await?;
    if filled > 0 {
        peaks.extend([quantize(low), quantize(high)]);
    }
    Ok(peaks)
}

/// Decodes the audio of `source` as mono `f32` samples at `sample_rate` and hands them to
/// `consume` in chunks as they arrive, so long recordings never sit whole in memory.
async fn decode_audio(
    ffmpeg: &Path,
    source: &Path,
    sample_rate: u32,
    mut consume: impl FnMut(&[f32]),
) -> Result<()> {
    let mut child = command(ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(source)
        .args(["-vn", "-ac", "1", "-ar"])
        .arg(sample_rate.to_string())
        .args(["-f", "f32le", "pipe:1"])
        .spawn()
        .map_err(|err| DomainError::Unavailable(format!("ffmpeg could not start: {err}")))?;
    let mut stdout = child.stdout.take().expect("stdout is piped");

    let mut buffer = vec![0u8; 64 * 1024];
    let mut carry: Vec<u8> = Vec::new();
    let mut samples: Vec<f32> = Vec::new();
    loop {
        let read = stdout
            .read(&mut buffer)
            .await
            .map_err(DomainError::storage)?;
        if read == 0 {
            break;
        }
        carry.extend_from_slice(&buffer[..read]);
        let (whole, _) = carry.as_chunks::<4>();
        samples.clear();
        samples.extend(whole.iter().map(|bytes| f32::from_le_bytes(*bytes)));
        let used = whole.len() * 4;
        carry.drain(..used);
        consume(&samples);
    }
    let status = child.wait().await.map_err(DomainError::storage)?;
    if !status.success() {
        return Err(DomainError::Invalid(
            "ffmpeg could not decode the audio".into(),
        ));
    }
    Ok(())
}

fn quantize(sample: f32) -> i8 {
    (sample.clamp(-1.0, 1.0) * 127.0).round() as i8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyframes_are_at_most_15_frames_and_half_a_second_apart() {
        assert_eq!(keyframe_interval(60.0), 15); // 0.25 s
        assert_eq!(keyframe_interval(59.94), 15);
        assert_eq!(keyframe_interval(30.0), 15); // 0.5 s
        assert_eq!(keyframe_interval(29.97), 14);
        assert_eq!(keyframe_interval(25.0), 12); // 0.48 s
        assert_eq!(keyframe_interval(23.976), 11);
        assert_eq!(keyframe_interval(1.0), 1);
        assert_eq!(keyframe_interval(0.0), 15);
        assert_eq!(keyframe_interval(f64::NAN), 15);
    }

    #[test]
    fn part_files_sit_next_to_their_target() {
        assert_eq!(
            part_path(Path::new("media/x/proxy.mp4")),
            PathBuf::from("media/x/proxy.mp4.part")
        );
    }

    #[test]
    fn samples_quantize_to_signed_bytes() {
        assert_eq!(quantize(1.0), 127);
        assert_eq!(quantize(-1.0), -127);
        assert_eq!(quantize(2.5), 127);
        assert_eq!(quantize(0.0), 0);
    }

    #[test]
    fn proxies_above_1080p_are_scaled_by_every_encoder() {
        for encoder in Encoder::ALL {
            let args = encoder.video_args(2160).join(" ");
            assert!(args.contains("1080"), "{}: {args}", encoder.name());
            assert!(
                encoder.input_args(true).is_empty(),
                "no GPU-only decoding when scaling"
            );
        }
        assert!(
            !Encoder::OpenH264
                .video_args(720)
                .join(" ")
                .contains("scale")
        );
    }
}
