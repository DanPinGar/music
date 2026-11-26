use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// The ffmpeg and ffprobe executables the core runs.
#[derive(Debug, Clone)]
pub struct MediaTools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    /// Version reported by ffmpeg, e.g. `n9.0.2-963-g4d831360c9-20261124`.
    pub version: String,
}

/// `.tools` in this repository, where `pnpm tools` installs the tools for development: ffmpeg in
/// `ffmpeg/bin`, yt-dlp in `yt-dlp` and Deno in `deno`, as the app keeps them in `tools/`.
/// The path is fixed at compile time, so it only exists on development machines.
pub fn dev_tools_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.tools")
}

/// `.tools/ffmpeg/bin` in this repository, where `pnpm tools` installs ffmpeg for development.
/// The path is fixed at compile time, so it only exists on development machines.
pub fn dev_tools_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.tools/ffmpeg/bin")
}

impl MediaTools {
    /// Finds a working ffmpeg + ffprobe pair in `$SEGNO_FFMPEG_DIR`, then in each of
    /// `candidates`, then on the PATH.
    pub async fn discover(candidates: &[PathBuf]) -> Option<Self> {
        let mut dirs: Vec<Option<PathBuf>> = Vec::new();
        if let Some(dir) = std::env::var_os("SEGNO_FFMPEG_DIR") {
            dirs.push(Some(PathBuf::from(dir)));
        }
        dirs.extend(candidates.iter().cloned().map(Some));
        dirs.push(None); // the PATH

        for dir in dirs {
            let (ffmpeg, ffprobe) = match &dir {
                Some(dir) => (
                    dir.join(executable("ffmpeg")),
                    dir.join(executable("ffprobe")),
                ),
                None => (executable("ffmpeg").into(), executable("ffprobe").into()),
            };
            if dir.is_some() && !(ffmpeg.is_file() && ffprobe.is_file()) {
                continue;
            }
            if let Some(version) = version_of(&ffmpeg).await
                && version_of(&ffprobe).await.is_some()
            {
                return Some(Self {
                    ffmpeg,
                    ffprobe,
                    version,
                });
            }
        }
        None
    }
}

pub(crate) fn executable(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

async fn version_of(program: &Path) -> Option<String> {
    let output = command(program).arg("-version").output().await.ok()?;
    if !output.status.success() {
        return None;
    }
    // "ffmpeg version n9.0.2-963-g4d831360c9-20261124 Copyright (c) 2000-2026 …"
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .next()?
        .split_whitespace()
        .nth(2)
        .map(str::to_owned)
}

/// A command for a media tool. It never opens a console window: on Windows the desktop app is
/// a GUI process, and each console child would flash a window.
pub(crate) fn command(program: impl AsRef<OsStr>) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(program);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}
