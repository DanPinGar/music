//! Installs and updates the external tools Segno runs: ffmpeg (importing and preparing
//! videos), yt-dlp (YouTube downloads) and Deno (the JavaScript runtime yt-dlp needs).
//!
//! Each comes from its project's GitHub releases and is checked against the SHA-256 published
//! with the release (and GitHub's own digest). A tools folder holds one folder per tool,
//! where the media adapters look for them, and `tools.json` with what was installed:
//!
//! ```text
//! tools/
//! ├─ ffmpeg/bin/   ffmpeg, ffprobe and their libraries (LGPL shared build)
//! ├─ yt-dlp/       yt-dlp
//! ├─ deno/         deno
//! └─ tools.json
//! ```

mod catalog;
mod github;
mod install;
mod manager;
mod manifest;
#[cfg(test)]
mod tests;

pub use catalog::{Os, Platform, Tool};
pub use github::GITHUB_API;
pub use install::apply_staged;
pub use manager::{Check, ToolManager, ToolState, ToolStatus, ToolsConfig};

/// Why a tool could not be installed or updated.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("download failed: {0}")]
    Network(String),

    /// The release is not as expected (an asset or its checksum is missing).
    #[error("{0}")]
    Release(String),

    #[error("{0}")]
    Unsupported(String),

    #[error("{asset} does not match its published checksum (expected {expected}, got {actual})")]
    Checksum {
        asset: String,
        expected: String,
        actual: String,
    },

    #[error("{0}")]
    Archive(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
