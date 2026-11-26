//! YouTube downloads with yt-dlp (and Deno, the JavaScript runtime it needs for YouTube).

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use segno_domain::download::{Chapter, RemoteVideo, VideoDownloader};
use segno_domain::{DomainError, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

use crate::MediaTools;
use crate::tools::{command, executable};

/// H.264 + AAC in MP4, up to 1080p, preferring 60 fps: plays everywhere, and 60 fps doubles
/// the time resolution of vision transcription.
const FORMAT: &str = "bv*[vcodec^=avc1][height<=1080][fps>30]+ba[ext=m4a]/\
                      bv*[vcodec^=avc1][height<=1080]+ba[ext=m4a]/\
                      b[ext=mp4][height<=1080]/b";
/// Share of a two-part download (picture, then sound) taken by the picture.
const PICTURE_SHARE: f64 = 0.9;
/// How long the details read by `fetch_info` serve the download that follows. YouTube's
/// links in them last hours; the download normally starts right away.
const INFO_LIFETIME: Duration = Duration::from_secs(30 * 60);

/// Downloads with the yt-dlp executable.
#[derive(Debug, Clone)]
pub struct YtDlpDownloader {
    ytdlp: PathBuf,
    deno: Option<PathBuf>,
    ffmpeg_dir: Option<PathBuf>,
    /// Version reported by yt-dlp, e.g. `2026.11.23`.
    pub version: String,
    /// Version of the Deno found, e.g. `2.9.8`.
    pub deno_version: Option<String>,
    infos: Arc<InfoCache>,
}

/// The details `fetch_info` read, by address, kept for the download that follows: loading
/// them spares yt-dlp a second extraction, which on YouTube takes most of the download
/// (8 of 11 s for a 2-minute video, solving its JavaScript challenges).
#[derive(Debug, Default)]
struct InfoCache(Mutex<HashMap<String, (Instant, Vec<u8>)>>);

impl InfoCache {
    fn keep(&self, url: &str, json: Vec<u8>) {
        let mut infos = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        infos.retain(|_, (read, _)| read.elapsed() < INFO_LIFETIME);
        infos.insert(url.to_owned(), (Instant::now(), json));
    }

    fn take(&self, url: &str) -> Option<Vec<u8>> {
        let mut infos = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        infos
            .remove(url)
            .filter(|(read, _)| read.elapsed() < INFO_LIFETIME)
            .map(|(_, json)| json)
    }
}

/// What yt-dlp downloads from: the video's address, or the details already read from it.
enum Source<'a> {
    Address(&'a str),
    Details(&'a Path),
}

impl YtDlpDownloader {
    /// Finds yt-dlp in `$SEGNO_YTDLP_DIR`, then in `ytdlp_dirs`, then on the PATH; and Deno in
    /// `deno_dirs`, then on the PATH. Merging picture and sound needs `ffmpeg`.
    pub async fn discover(
        ytdlp_dirs: &[PathBuf],
        deno_dirs: &[PathBuf],
        ffmpeg: Option<&MediaTools>,
    ) -> Option<Self> {
        let mut dirs: Vec<Option<PathBuf>> = Vec::new();
        if let Some(dir) = std::env::var_os("SEGNO_YTDLP_DIR") {
            dirs.push(Some(PathBuf::from(dir)));
        }
        dirs.extend(ytdlp_dirs.iter().cloned().map(Some));
        dirs.push(None);
        let mut found = None;
        for dir in dirs {
            let program = match &dir {
                Some(dir) => dir.join(executable("yt-dlp")),
                None => executable("yt-dlp").into(),
            };
            if dir.is_some() && !program.is_file() {
                continue;
            }
            if let Some(version) = first_line(&program, "--version").await {
                found = Some((program, version));
                break;
            }
        }
        let (ytdlp, version) = found?;

        let mut deno = None;
        let mut deno_version = None;
        for candidate in deno_dirs
            .iter()
            .map(|dir| dir.join(executable("deno")))
            .chain([executable("deno").into()])
        {
            // "deno 2.9.8 (stable, release, x86_64-pc-windows-msvc)"
            if let Some(line) = first_line(&candidate, "--version").await {
                deno_version = line.split_whitespace().nth(1).map(str::to_owned);
                deno = Some(candidate);
                break;
            }
        }
        Some(Self {
            ytdlp,
            deno,
            ffmpeg_dir: ffmpeg.and_then(|tools| tools.ffmpeg.parent().map(Path::to_path_buf)),
            version,
            deno_version,
            infos: Arc::default(),
        })
    }

    /// Whether Deno was found: without it YouTube downloads fail.
    pub fn has_javascript_runtime(&self) -> bool {
        self.deno.is_some()
    }

    /// The yt-dlp program run (a bare name when found on the PATH).
    pub fn program(&self) -> &Path {
        &self.ytdlp
    }

    /// The Deno program given to yt-dlp, if found.
    pub fn javascript_runtime(&self) -> Option<&Path> {
        self.deno.as_deref()
    }

    fn command(&self) -> tokio::process::Command {
        let mut command = command(&self.ytdlp);
        // yt-dlp is Python: make it write UTF-8 whatever the console code page.
        command.env("PYTHONIOENCODING", "utf-8");
        command.args(["--no-playlist", "--no-warnings", "--no-color"]);
        if let Some(deno) = &self.deno {
            let mut runtime = OsString::from("deno:");
            runtime.push(deno);
            command.arg("--js-runtimes").arg(runtime);
        }
        if let Some(dir) = &self.ffmpeg_dir {
            command.arg("--ffmpeg-location").arg(dir);
        }
        command
    }
}

#[async_trait]
impl VideoDownloader for YtDlpDownloader {
    async fn fetch_info(&self, url: &str) -> Result<RemoteVideo> {
        let output = self
            .command()
            .args(["--dump-single-json", "--skip-download"])
            .arg(url)
            .output()
            .await
            .map_err(|err| DomainError::Unavailable(format!("yt-dlp could not start: {err}")))?;
        if !output.status.success() {
            return Err(failure(&String::from_utf8_lossy(&output.stderr)));
        }
        let json: Value = serde_json::from_slice(&output.stdout).map_err(DomainError::storage)?;
        let info = parse_info(&json)?;
        self.infos.keep(url, output.stdout);
        Ok(info)
    }

    async fn download(
        &self,
        url: &str,
        target: &Path,
        progress: &(dyn Fn(f64) + Send + Sync),
    ) -> Result<()> {
        let folder = target
            .parent()
            .ok_or_else(|| DomainError::Invalid("the download has no folder".into()))?;
        if let Some(json) = self.infos.take(url) {
            let details = folder.join("download.info.json");
            if tokio::fs::write(&details, &json).await.is_ok() {
                let result = self
                    .run(Source::Details(&details), folder, target, progress)
                    .await;
                let _ = tokio::fs::remove_file(&details).await;
                if result.is_ok() {
                    return result;
                }
                // The links in the details may have expired: start again from the address.
            }
        }
        self.run(Source::Address(url), folder, target, progress)
            .await
    }
}

impl YtDlpDownloader {
    /// One yt-dlp run that leaves the video in `target`.
    async fn run(
        &self,
        source: Source<'_>,
        folder: &Path,
        target: &Path,
        progress: &(dyn Fn(f64) + Send + Sync),
    ) -> Result<()> {
        // yt-dlp writes the parts and the merged file next to each other; the merged file
        // becomes `target` only once complete.
        let template = folder.join("download.%(ext)s");
        let merged = folder.join("download.mp4");
        let mut command = self.command();
        command
            .args([
                "--newline",
                "--format",
                FORMAT,
                "--merge-output-format",
                "mp4",
            ])
            .args(["--progress-template", PROGRESS_TEMPLATE])
            .arg("--output")
            .arg(&template);
        match source {
            Source::Address(url) => command.arg(url),
            Source::Details(path) => command.arg("--load-info-json").arg(path),
        };
        let mut child = command
            .spawn()
            .map_err(|err| DomainError::Unavailable(format!("yt-dlp could not start: {err}")))?;
        let stdout = child.stdout.take().expect("stdout is piped");
        let mut stderr = child.stderr.take().expect("stderr is piped");
        let errors = tokio::spawn(async move {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text).await;
            text
        });

        let mut tracker = ProgressTracker::default();
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(done) = tracker.read(&line) {
                progress(done);
            }
        }
        let status = child.wait().await.map_err(DomainError::storage)?;
        let errors = errors.await.unwrap_or_default();
        if !status.success() {
            return Err(failure(&errors));
        }
        tokio::fs::rename(&merged, target)
            .await
            .map_err(|err| DomainError::Invalid(format!("yt-dlp left no MP4 file: {err}")))
    }
}

/// Machine-readable progress: "SEGNO <downloaded> <total>" per line.
const PROGRESS_TEMPLATE: &str = "download:SEGNO %(progress.downloaded_bytes)s %(progress.total_bytes,progress.total_bytes_estimate)s";

/// Turns yt-dlp's output into the progress of the whole download, picture and sound.
#[derive(Debug, Default)]
struct ProgressTracker {
    /// Parts to download (2 when picture and sound come separately).
    parts: usize,
    /// The part being downloaded, from 0.
    part: usize,
    last_downloaded: f64,
}

impl ProgressTracker {
    /// The overall progress after `line`, if it says something about it.
    fn read(&mut self, line: &str) -> Option<f64> {
        // "[info] 7A12z9S4F8U: Downloading 1 format(s): 299+140"
        if let Some((_, formats)) = line.split_once("Downloading 1 format(s): ") {
            self.parts = formats.trim().split('+').count();
            return None;
        }
        if line.starts_with("[Merger]") {
            return Some(0.99);
        }
        let numbers = line.strip_prefix("SEGNO ")?;
        let mut values = numbers
            .split_whitespace()
            .map(|value| value.parse::<f64>().ok());
        let downloaded = values.next().flatten()?;
        let total = values.next().flatten().filter(|total| *total > 0.0)?;
        // A new part starts from zero again.
        if downloaded < self.last_downloaded {
            self.part += 1;
        }
        self.last_downloaded = downloaded;
        let fraction = (downloaded / total).clamp(0.0, 1.0);
        let done = match (self.parts, self.part) {
            (2, 0) => fraction * PICTURE_SHARE,
            (2, _) => PICTURE_SHARE + fraction * (1.0 - PICTURE_SHARE),
            _ => fraction,
        };
        Some(done.min(0.98))
    }
}

/// The video's details from yt-dlp's JSON.
fn parse_info(json: &Value) -> Result<RemoteVideo> {
    let text = |key: &str| {
        json.get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    };
    let seconds_ms = |value: &Value| {
        value
            .as_f64()
            .map(|seconds| (seconds * 1000.0).round() as i64)
    };
    let title =
        text("title").ok_or_else(|| DomainError::Invalid("the video has no title".into()))?;
    let duration_ms = json.get("duration").and_then(seconds_ms);
    let chapters = json
        .get("chapters")
        .and_then(Value::as_array)
        .map(|chapters| {
            chapters
                .iter()
                .filter_map(|chapter| {
                    Some(Chapter {
                        start_ms: seconds_ms(chapter.get("start_time")?)?,
                        end_ms: seconds_ms(chapter.get("end_time")?)?,
                        title: chapter.get("title")?.as_str()?.trim().to_owned(),
                    })
                })
                .filter(|chapter| chapter.end_ms > chapter.start_ms)
                .collect()
        })
        .unwrap_or_default();
    // YouTube's music details, for the songs it identified: "artists" is a list in recent
    // yt-dlp versions, "artist" a comma-separated text in older ones.
    let artists = json
        .get("artists")
        .and_then(Value::as_array)
        .map(|artists| {
            artists
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|artists| !artists.is_empty());
    let release_year = json
        .get("release_year")
        .and_then(Value::as_i64)
        .and_then(|year| i32::try_from(year).ok());
    Ok(RemoteVideo {
        title,
        channel: text("channel").or_else(|| text("uploader")),
        track: text("track"),
        artist: artists.or_else(|| text("artist")),
        release_year,
        duration_ms,
        chapters,
    })
}

/// yt-dlp's error, from the last "ERROR:" line it wrote.
fn failure(stderr: &str) -> DomainError {
    let message = stderr
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix("ERROR: "))
        .or_else(|| stderr.lines().rev().find(|line| !line.trim().is_empty()))
        .unwrap_or("yt-dlp failed without saying why")
        .trim()
        .to_owned();
    DomainError::Unavailable(message)
}

async fn first_line(program: &Path, flag: &str) -> Option<String> {
    let output = command(program).arg(flag).output().await.ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(|line| line.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn details_and_chapters_are_read_from_the_json() {
        let json = serde_json::json!({
            "id": "abcdefghijk",
            "title": "Chopin - Nocturne Op. 9 No. 2",
            "uploader": "Rousseau",
            "duration": 271.5,
            "chapters": [
                { "start_time": 0.0, "end_time": 95.25, "title": " Theme " },
                { "start_time": 95.25, "end_time": 95.25, "title": "Empty" },
                { "start_time": 95.25, "end_time": 271.5, "title": "Variations" }
            ]
        });
        let info = parse_info(&json).unwrap();
        assert_eq!(info.title, "Chopin - Nocturne Op. 9 No. 2");
        assert_eq!(info.channel.as_deref(), Some("Rousseau"));
        assert_eq!(info.duration_ms, Some(271_500));
        let chapters: Vec<(i64, i64, &str)> = info
            .chapters
            .iter()
            .map(|c| (c.start_ms, c.end_ms, c.title.as_str()))
            .collect();
        assert_eq!(
            chapters,
            [(0, 95_250, "Theme"), (95_250, 271_500, "Variations")]
        );
    }

    #[test]
    fn videos_without_chapters_have_none() {
        let json = serde_json::json!({ "title": "Monk", "channel": "itsRemco", "chapters": null });
        let info = parse_info(&json).unwrap();
        assert!(info.chapters.is_empty());
        assert_eq!(info.channel.as_deref(), Some("itsRemco"));
        assert_eq!(
            (info.track, info.artist, info.release_year),
            (None, None, None)
        );
    }

    #[test]
    fn music_details_are_read_when_youtube_has_them() {
        let json = serde_json::json!({
            "title": "Take Five",
            "track": "Take Five",
            "artists": ["The Dave Brubeck Quartet", "Paul Desmond"],
            "release_year": 1959
        });
        let info = parse_info(&json).unwrap();
        assert_eq!(info.track.as_deref(), Some("Take Five"));
        assert_eq!(
            info.artist.as_deref(),
            Some("The Dave Brubeck Quartet, Paul Desmond")
        );
        assert_eq!(info.release_year, Some(1959));
        let older = serde_json::json!({ "title": "x", "artist": "Monk", "track": " " });
        let info = parse_info(&older).unwrap();
        assert_eq!((info.track, info.artist.as_deref()), (None, Some("Monk")));
    }

    #[test]
    fn progress_covers_picture_then_sound() {
        let mut tracker = ProgressTracker::default();
        assert_eq!(
            tracker.read("[info] 7A12z9S4F8U: Downloading 1 format(s): 299+140"),
            None
        );
        assert_eq!(tracker.read("SEGNO 10000000 20000000"), Some(0.45));
        assert_eq!(tracker.read("SEGNO 20000000 20000000"), Some(0.9));
        let sound = tracker.read("SEGNO 1000000 2000000").unwrap();
        assert!((sound - 0.95).abs() < 1e-9);
        assert_eq!(
            tracker.read("[Merger] Merging formats into \"x.mp4\""),
            Some(0.99)
        );
        assert_eq!(tracker.read("SEGNO NA NA"), None);
    }

    #[test]
    fn single_files_count_whole() {
        let mut tracker = ProgressTracker::default();
        tracker.read("[info] x: Downloading 1 format(s): 18");
        assert_eq!(tracker.read("SEGNO 5 10"), Some(0.5));
    }

    #[test]
    fn details_serve_one_download_of_the_same_address() {
        let infos = InfoCache::default();
        infos.keep("https://youtu.be/a", b"{}".to_vec());
        assert_eq!(infos.take("https://youtu.be/b"), None);
        assert_eq!(infos.take("https://youtu.be/a"), Some(b"{}".to_vec()));
        assert_eq!(infos.take("https://youtu.be/a"), None);
    }

    #[test]
    fn errors_keep_yt_dlps_message() {
        let error =
            failure("[youtube] x: Downloading webpage\nERROR: [youtube] x: Video unavailable\n");
        assert_eq!(
            error.to_string(),
            "unavailable: [youtube] x: Video unavailable"
        );
    }
}
