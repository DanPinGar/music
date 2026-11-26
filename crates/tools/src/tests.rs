//! The manager against a fake GitHub served on 127.0.0.1: installing, updating, staging
//! ffmpeg, and refusing downloads that do not match their checksums.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::github::hex;
use crate::{Check, Platform, Tool, ToolManager, ToolState, ToolsConfig, apply_staged};

#[derive(Default)]
struct Published {
    releases: HashMap<String, Value>,
    files: HashMap<String, Vec<u8>>,
    /// Requests per file name.
    served: HashMap<String, usize>,
}

#[derive(Clone)]
struct FakeGitHub {
    base: String,
    published: Arc<Mutex<Published>>,
}

impl FakeGitHub {
    async fn start() -> Self {
        let published = Arc::new(Mutex::new(Published::default()));
        let app = Router::new()
            .route("/repos/{owner}/{repo}/releases/latest", get(release))
            .route("/files/{name}", get(file))
            .with_state(published.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self { base, published }
    }

    /// Publishes a release of `repository` with these files; `listed` overrides the checksum
    /// published for a file (to publish a wrong one).
    fn publish(&self, repository: &str, tag: &str, files: Vec<(String, Vec<u8>)>) {
        let mut published = self.published.lock().unwrap();
        let assets: Vec<Value> = files
            .iter()
            .map(|(name, bytes)| {
                json!({
                    "name": name,
                    "size": bytes.len(),
                    "browser_download_url": format!("{}/files/{name}", self.base),
                    "updated_at": chrono::Utc::now(),
                    "digest": format!("sha256:{}", sha256(bytes)),
                })
            })
            .collect();
        published.releases.insert(
            repository.into(),
            json!({ "tag_name": tag, "assets": assets }),
        );
        published.files.extend(files);
    }

    /// Serves other bytes under a published name, as a corrupted download would.
    fn corrupt(&self, name: &str) {
        let mut published = self.published.lock().unwrap();
        published
            .files
            .insert(name.into(), b"not what was published".to_vec());
    }

    fn served(&self, name: &str) -> usize {
        let published = self.published.lock().unwrap();
        published.served.get(name).copied().unwrap_or(0)
    }

    fn downloads(&self) -> usize {
        let published = self.published.lock().unwrap();
        published
            .served
            .iter()
            .filter(|(name, _)| !is_checksums(name))
            .map(|(_, count)| count)
            .sum()
    }
}

async fn release(
    State(published): State<Arc<Mutex<Published>>>,
    UrlPath((owner, repo)): UrlPath<(String, String)>,
) -> Response {
    let published = published.lock().unwrap();
    match published.releases.get(&format!("{owner}/{repo}")) {
        Some(release) => axum::Json(release.clone()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn file(
    State(published): State<Arc<Mutex<Published>>>,
    UrlPath(name): UrlPath<String>,
) -> Response {
    let mut published = published.lock().unwrap();
    *published.served.entry(name.clone()).or_default() += 1;
    match published.files.get(&name) {
        Some(bytes) => bytes.clone().into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn is_checksums(name: &str) -> bool {
    name == "SHA2-256SUMS" || name == "checksums.sha256" || name.ends_with(".sha256sum")
}

fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// A release of each tool, version `n`, with builds for every platform: the manager picks.
fn publish_all(github: &FakeGitHub, n: u32) {
    let program = format!("yt-dlp {n}").into_bytes();
    let mut ytdlp: Vec<(String, Vec<u8>)> = [
        "yt-dlp.exe",
        "yt-dlp_linux",
        "yt-dlp_linux_aarch64",
        "yt-dlp_macos",
    ]
    .map(|name| (name.to_owned(), program.clone()))
    .into();
    let sums: String = ytdlp
        .iter()
        .map(|(name, bytes)| format!("{}  {name}\n", sha256(bytes)))
        .collect();
    ytdlp.push(("SHA2-256SUMS".into(), sums.into_bytes()));
    github.publish("yt-dlp/yt-dlp", &format!("2026.11.{n}"), ytdlp);

    let deno_bytes = format!("deno {n}").into_bytes();
    let archive = zip(&[
        ("deno.exe", &deno_bytes),
        ("deno", &deno_bytes),
        ("README.md", b"x"),
    ]);
    let mut deno = Vec::new();
    for arch in ["x86_64", "aarch64"] {
        for target in ["pc-windows-msvc", "unknown-linux-gnu", "apple-darwin"] {
            let name = format!("deno-{arch}-{target}.zip");
            // Deno publishes a PowerShell table on Windows.
            let sum = format!(
                "Algorithm Hash Path\r\n--------- ---- ----\r\nSHA256 {} D:\\a\\{name}\r\n",
                sha256(&archive).to_uppercase()
            );
            deno.push((format!("{name}.sha256sum"), sum.into_bytes()));
            deno.push((name, archive.clone()));
        }
    }
    github.publish("denoland/deno", &format!("v2.{n}.0"), deno);

    let branch = format!("9.{n}");
    let ffmpeg_bytes = format!("ffmpeg {n}").into_bytes();
    let mut ffmpeg = Vec::new();
    let mut sums = String::new();
    for platform in ["win64", "winarm64"] {
        let top = format!("ffmpeg-n{branch}-latest-{platform}-lgpl-shared-{branch}");
        let entries: Vec<(String, &[u8])> = vec![
            (format!("{top}/bin/ffmpeg.exe"), &ffmpeg_bytes),
            (format!("{top}/bin/ffprobe.exe"), &ffmpeg_bytes),
            (format!("{top}/bin/ffmpeg"), &ffmpeg_bytes),
            (format!("{top}/bin/ffprobe"), &ffmpeg_bytes),
            (format!("{top}/bin/avcodec-63.dll"), b"library"),
            (format!("{top}/bin/ffplay.exe"), b"player"),
            (format!("{top}/doc/ffmpeg.html"), b"docs"),
            (format!("{top}/LICENSE.txt"), b"LGPL"),
        ];
        let entries: Vec<(&str, &[u8])> = entries
            .iter()
            .map(|(name, bytes)| (name.as_str(), *bytes))
            .collect();
        let archive = zip(&entries);
        let name = format!("{top}.zip");
        sums.push_str(&format!("{}  {name}\n", sha256(&archive)));
        ffmpeg.push((name, archive));
    }
    ffmpeg.push(("checksums.sha256".into(), sums.into_bytes()));
    github.publish("FFmpeg-Builds/FFmpeg-Builds", "latest", ffmpeg);
}

struct Setup {
    github: FakeGitHub,
    dir: tempfile::TempDir,
    changed: Arc<Mutex<Vec<Tool>>>,
}

impl Setup {
    async fn new() -> Self {
        Self {
            github: FakeGitHub::start().await,
            dir: tempfile::tempdir().unwrap(),
            changed: Arc::default(),
        }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn manager(&self, external: Vec<Tool>) -> Arc<ToolManager> {
        let changed = self.changed.clone();
        let config = ToolsConfig {
            external,
            api: self.github.base.clone(),
            ..ToolsConfig::new(self.root())
        };
        ToolManager::new(config, move |tool| {
            changed.lock().unwrap().push(tool);
            async {}
        })
        .unwrap()
    }

    fn changed(&self) -> Vec<Tool> {
        let mut changed = std::mem::take(&mut *self.changed.lock().unwrap());
        changed.sort();
        changed
    }

    fn read(&self, tool: Tool) -> String {
        std::fs::read_to_string(tool.executable(self.root())).unwrap_or_default()
    }
}

/// The tools Segno can install on the computer running the tests.
fn installable() -> Vec<Tool> {
    Tool::ALL
        .into_iter()
        .filter(|tool| Platform::current().supports(*tool))
        .collect()
}

#[tokio::test]
async fn installs_every_tool_once_after_checking_it() {
    let setup = Setup::new().await;
    publish_all(&setup.github, 1);
    let manager = setup.manager(vec![]);
    assert_eq!(manager.status(Tool::YtDlp).state, ToolState::Missing);

    manager.run(&Tool::ALL, Check::Due).await;
    for tool in installable() {
        let status = manager.status(tool);
        assert_eq!(status.state, ToolState::Ready, "{status:?}");
        assert!(status.checked_at.is_some() && status.error.is_none());
    }
    assert_eq!(setup.read(Tool::YtDlp), "yt-dlp 1");
    assert_eq!(setup.read(Tool::Deno), "deno 1");
    assert_eq!(
        manager.status(Tool::YtDlp).version.as_deref(),
        Some("2026.11.1")
    );
    assert_eq!(manager.status(Tool::Deno).version.as_deref(), Some("2.1.0"));
    assert_eq!(setup.changed(), installable());
    if installable().contains(&Tool::Ffmpeg) {
        assert_eq!(setup.read(Tool::Ffmpeg), "ffmpeg 1");
        let folder = Tool::Ffmpeg.folder(setup.root());
        assert!(folder.join("bin").join("avcodec-63.dll").is_file());
        assert!(folder.join("LICENSE.txt").is_file());
        assert!(!folder.join("bin").join("ffplay.exe").exists(), "no player");
        assert!(!folder.join("doc").exists(), "no docs");
        assert_eq!(
            manager.status(Tool::Ffmpeg).version.as_deref(),
            Some("n9.1")
        );
    }
    // Nothing is left half done.
    let work: Vec<_> = std::fs::read_dir(setup.root().join(".work"))
        .unwrap()
        .collect();
    assert!(work.is_empty(), "{work:?}");

    // Checked just now: nothing to do. Asked to look again: nothing newer to download.
    let downloads = setup.github.downloads();
    manager.run(&Tool::ALL, Check::Due).await;
    assert_eq!(setup.github.served("SHA2-256SUMS"), 1);
    manager.run(&Tool::ALL, Check::All).await;
    assert_eq!(setup.github.served("SHA2-256SUMS"), 2);
    assert_eq!(setup.github.downloads(), downloads);
    assert!(setup.changed().is_empty());

    // A new manager (the next start) knows what is installed.
    let restarted = setup.manager(vec![]);
    assert_eq!(restarted.status(Tool::YtDlp).state, ToolState::Ready);
    assert_eq!(
        restarted.status(Tool::YtDlp).version.as_deref(),
        Some("2026.11.1")
    );
}

#[tokio::test]
async fn updates_replace_programs_at_once_and_ffmpeg_at_the_next_start() {
    let setup = Setup::new().await;
    publish_all(&setup.github, 1);
    let manager = setup.manager(vec![]);
    manager.run(&Tool::ALL, Check::Due).await;
    setup.changed();

    publish_all(&setup.github, 2);
    manager.run(&Tool::ALL, Check::All).await;
    assert_eq!(setup.read(Tool::YtDlp), "yt-dlp 2");
    assert_eq!(setup.read(Tool::Deno), "deno 2");
    assert_eq!(
        manager.status(Tool::YtDlp).version.as_deref(),
        Some("2026.11.2")
    );
    let replaced: Vec<Tool> = installable()
        .into_iter()
        .filter(|tool| *tool != Tool::Ffmpeg)
        .collect();
    assert_eq!(setup.changed(), replaced, "ffmpeg waits");

    if installable().contains(&Tool::Ffmpeg) {
        let status = manager.status(Tool::Ffmpeg);
        assert_eq!(status.version.as_deref(), Some("n9.1"));
        assert_eq!(status.staged.as_deref(), Some("n9.2"));
        assert_eq!(setup.read(Tool::Ffmpeg), "ffmpeg 1", "still the one in use");

        assert_eq!(apply_staged(setup.root()), [Tool::Ffmpeg]);
        assert_eq!(setup.read(Tool::Ffmpeg), "ffmpeg 2");
        let restarted = setup.manager(vec![]);
        let status = restarted.status(Tool::Ffmpeg);
        assert_eq!(status.version.as_deref(), Some("n9.2"));
        assert_eq!(status.staged, None);
        assert!(apply_staged(setup.root()).is_empty());
    }
}

#[tokio::test]
async fn downloads_that_do_not_match_their_checksum_are_refused() {
    let setup = Setup::new().await;
    publish_all(&setup.github, 1);
    setup.github.corrupt("yt-dlp.exe");
    setup.github.corrupt("yt-dlp_linux");
    setup.github.corrupt("yt-dlp_linux_aarch64");
    setup.github.corrupt("yt-dlp_macos");
    let manager = setup.manager(vec![]);
    manager.run(&[Tool::YtDlp], Check::Due).await;

    let status = manager.status(Tool::YtDlp);
    assert_eq!(status.state, ToolState::Missing);
    let error = status.error.unwrap();
    assert!(
        error.contains("does not match its published checksum"),
        "{error}"
    );
    assert!(!Tool::YtDlp.executable(setup.root()).exists());
    assert!(setup.changed().is_empty());
}

#[tokio::test]
async fn external_tools_are_left_alone_and_failures_are_reported() {
    let setup = Setup::new().await;
    // Nothing published: every lookup is a 404.
    let manager = setup.manager(vec![Tool::Ffmpeg]);
    manager.run(&Tool::ALL, Check::All).await;
    assert_eq!(manager.status(Tool::Ffmpeg).state, ToolState::External);
    assert_eq!(manager.status(Tool::Ffmpeg).error, None);
    let status = manager.status(Tool::Deno);
    assert_eq!(status.state, ToolState::Missing);
    assert!(status.error.unwrap().starts_with("download failed"));
    assert_eq!(setup.github.downloads(), 0);
}
