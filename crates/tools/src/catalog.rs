//! The tools Segno runs, where each one is published, and which build suits this computer.

use std::path::{Path, PathBuf};

use chrono::TimeDelta;
use serde::{Deserialize, Serialize};

use crate::Error;
use crate::github::Asset;

/// An external program Segno runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Tool {
    /// ffmpeg and ffprobe: reading, importing and preparing videos.
    #[serde(rename = "ffmpeg")]
    Ffmpeg,
    /// yt-dlp: YouTube downloads.
    #[serde(rename = "yt-dlp")]
    YtDlp,
    /// Deno: the JavaScript runtime yt-dlp needs to read YouTube's pages.
    #[serde(rename = "deno")]
    Deno,
}

impl Tool {
    pub const ALL: [Self; 3] = [Self::Ffmpeg, Self::YtDlp, Self::Deno];

    pub fn name(self) -> &'static str {
        match self {
            Self::Ffmpeg => "ffmpeg",
            Self::YtDlp => "yt-dlp",
            Self::Deno => "deno",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tool| tool.name() == name)
    }

    /// Its folder in a tools folder: `ffmpeg`, `yt-dlp` or `deno`.
    pub fn folder(self, root: &Path) -> PathBuf {
        root.join(self.name())
    }

    /// Its program in a tools folder: `ffmpeg/bin/ffmpeg`, `yt-dlp/yt-dlp` or `deno/deno`
    /// (with `.exe` on Windows).
    pub fn executable(self, root: &Path) -> PathBuf {
        let folder = self.folder(root);
        match self {
            Self::Ffmpeg => folder.join("bin").join(executable("ffmpeg")),
            Self::YtDlp => folder.join(executable("yt-dlp")),
            Self::Deno => folder.join(executable("deno")),
        }
    }

    /// Its GitHub repository.
    pub(crate) fn repository(self) -> &'static str {
        match self {
            // LGPL builds, the licence family Segno ships with.
            Self::Ffmpeg => "FFmpeg-Builds/FFmpeg-Builds",
            Self::YtDlp => "yt-dlp/yt-dlp",
            Self::Deno => "denoland/deno",
        }
    }

    /// How often a newer release is looked for. yt-dlp follows YouTube's changes, which can
    /// break downloads any week; the others change little that Segno notices.
    pub(crate) fn check_every(self) -> TimeDelta {
        match self {
            Self::YtDlp => TimeDelta::days(1),
            Self::Deno | Self::Ffmpeg => TimeDelta::days(7),
        }
    }
}

/// A program's file name on this operating system.
pub(crate) fn executable(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

/// The computer a build is chosen for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Platform {
    pub os: Os,
    /// An ARM processor (aarch64); otherwise x86-64.
    pub arm: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    Linux,
    MacOs,
    Other,
}

impl Platform {
    /// This computer.
    pub fn current() -> Self {
        let os = match std::env::consts::OS {
            "windows" => Os::Windows,
            "linux" => Os::Linux,
            "macos" => Os::MacOs,
            _ => Os::Other,
        };
        Self {
            os,
            arm: std::env::consts::ARCH == "aarch64",
        }
    }

    /// Whether `tool` has a build Segno can install here. There are LGPL builds of ffmpeg only
    /// for Windows as a zip; elsewhere Segno uses the system's ffmpeg.
    pub fn supports(self, tool: Tool) -> bool {
        match tool {
            Tool::Ffmpeg => self.os == Os::Windows,
            Tool::YtDlp | Tool::Deno => self.os != Os::Other,
        }
    }
}

/// How a build is packed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Packaging {
    /// The program itself.
    Program,
    /// A zip archive with the program at its top.
    Zip,
    /// A zip archive with one folder holding `bin/` (ffmpeg's shared build: programs + DLLs).
    ZipFolder,
}

/// Where a build's SHA-256 is published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Checksums {
    /// A "<sha256>  <file>" list in this asset.
    List(String),
    /// An asset holding just this build's digest, in any layout (Deno's `.sha256sum`).
    Single(String),
}

/// The build of a release that suits a computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Build {
    pub asset: String,
    pub checksums: Checksums,
    pub packaging: Packaging,
    /// What the user sees: the release (`2026.11.23`, `2.9.8`) or ffmpeg's branch (`n9.0`).
    pub version: String,
}

/// Picks the build of `tool` for `platform` among a release's assets.
pub(crate) fn choose(
    tool: Tool,
    platform: Platform,
    tag: &str,
    assets: &[Asset],
) -> Result<Build, Error> {
    let unsupported = || Error::Unsupported(format!("no {} build for this computer", tool.name()));
    if !platform.supports(tool) {
        return Err(unsupported());
    }
    let build = match tool {
        Tool::YtDlp => {
            let asset = match (platform.os, platform.arm) {
                (Os::Windows, _) => "yt-dlp.exe",
                (Os::Linux, false) => "yt-dlp_linux",
                (Os::Linux, true) => "yt-dlp_linux_aarch64",
                (Os::MacOs, _) => "yt-dlp_macos",
                (Os::Other, _) => return Err(unsupported()),
            };
            Build {
                asset: asset.into(),
                checksums: Checksums::List("SHA2-256SUMS".into()),
                packaging: Packaging::Program,
                version: tag.into(),
            }
        }
        Tool::Deno => {
            let arch = if platform.arm { "aarch64" } else { "x86_64" };
            let target = match platform.os {
                Os::Windows => "pc-windows-msvc",
                Os::Linux => "unknown-linux-gnu",
                Os::MacOs => "apple-darwin",
                Os::Other => return Err(unsupported()),
            };
            let asset = format!("deno-{arch}-{target}.zip");
            Build {
                checksums: Checksums::Single(format!("{asset}.sha256sum")),
                asset,
                packaging: Packaging::Zip,
                version: tag.trim_start_matches('v').into(),
            }
        }
        Tool::Ffmpeg => {
            let tag = if platform.arm { "winarm64" } else { "win64" };
            let (version, asset) = newest_ffmpeg_branch(assets, tag).ok_or_else(unsupported)?;
            Build {
                asset,
                checksums: Checksums::List("checksums.sha256".into()),
                packaging: Packaging::ZipFolder,
                version,
            }
        }
    };
    if assets.iter().any(|asset| asset.name == build.asset) {
        Ok(build)
    } else {
        Err(unsupported())
    }
}

/// The LGPL shared build of the newest FFmpeg branch, e.g.
/// `("n9.0", "ffmpeg-n9.0-latest-win64-lgpl-shared-9.0.zip")`.
fn newest_ffmpeg_branch(assets: &[Asset], platform_tag: &str) -> Option<(String, String)> {
    let suffix = format!("-latest-{platform_tag}-lgpl-shared-");
    assets
        .iter()
        .filter_map(|asset| {
            let rest = asset.name.strip_prefix("ffmpeg-n")?;
            let (branch, tail) = rest.split_once(&suffix)?;
            if tail != format!("{branch}.zip") {
                return None;
            }
            let (major, minor) = branch.split_once('.')?;
            let key: (u32, u32) = (major.parse().ok()?, minor.parse().ok()?);
            Some((key, format!("n{branch}"), asset.name.clone()))
        })
        .max_by_key(|(key, _, _)| *key)
        .map(|(_, version, name)| (version, name))
}

/// The digest of `file` in a "<sha256>  <file>" list (`*file` in binary mode).
pub(crate) fn listed_sha256(list: &str, file: &str) -> Option<String> {
    list.lines().find_map(|line| {
        let (digest, name) = line.trim().split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == file && is_sha256(digest))
            .then(|| digest.to_ascii_lowercase())
    })
}

/// The one SHA-256 in a text, whatever its layout (a PowerShell table, "<digest>  <file>"…).
pub(crate) fn single_sha256(text: &str) -> Option<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .find(|word| is_sha256(word))
        .map(str::to_ascii_lowercase)
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> Asset {
        Asset {
            name: name.into(),
            size: 1,
            browser_download_url: format!("https://example.com/{name}"),
            updated_at: chrono::Utc::now(),
            digest: None,
        }
    }

    const WINDOWS: Platform = Platform {
        os: Os::Windows,
        arm: false,
    };

    #[test]
    fn ffmpeg_is_the_lgpl_shared_build_of_the_newest_branch() {
        let assets: Vec<Asset> = [
            "ffmpeg-master-latest-win64-lgpl-shared.zip",
            "ffmpeg-n8.1-latest-win64-lgpl-shared-8.1.zip",
            "ffmpeg-n9.0-latest-win64-gpl-shared-9.0.zip",
            "ffmpeg-n9.0-latest-win64-lgpl-9.0.zip",
            "ffmpeg-n9.0-latest-win64-lgpl-shared-9.0.zip",
            "ffmpeg-n9.0-latest-linux64-lgpl-shared-9.0.tar.xz",
            "ffmpeg-n10.0-latest-winarm64-lgpl-shared-10.0.zip",
            "checksums.sha256",
        ]
        .map(asset)
        .into();
        let build = choose(Tool::Ffmpeg, WINDOWS, "latest", &assets).unwrap();
        assert_eq!(build.asset, "ffmpeg-n9.0-latest-win64-lgpl-shared-9.0.zip");
        assert_eq!(build.version, "n9.0");
        assert_eq!(build.packaging, Packaging::ZipFolder);
        let arm = Platform {
            os: Os::Windows,
            arm: true,
        };
        assert_eq!(
            choose(Tool::Ffmpeg, arm, "latest", &assets)
                .unwrap()
                .version,
            "n10.0"
        );
        let linux = Platform {
            os: Os::Linux,
            arm: false,
        };
        assert!(matches!(
            choose(Tool::Ffmpeg, linux, "latest", &assets),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn yt_dlp_and_deno_builds_follow_the_platform() {
        let assets: Vec<Asset> = [
            "yt-dlp.exe",
            "yt-dlp_linux",
            "yt-dlp_macos",
            "deno-x86_64-pc-windows-msvc.zip",
            "deno-aarch64-apple-darwin.zip",
        ]
        .map(asset)
        .into();
        let ytdlp = choose(Tool::YtDlp, WINDOWS, "2026.11.23", &assets).unwrap();
        assert_eq!(ytdlp.asset, "yt-dlp.exe");
        assert_eq!(ytdlp.version, "2026.11.23");
        let deno = choose(Tool::Deno, WINDOWS, "v2.9.8", &assets).unwrap();
        assert_eq!(deno.asset, "deno-x86_64-pc-windows-msvc.zip");
        assert_eq!(
            deno.checksums,
            Checksums::Single("deno-x86_64-pc-windows-msvc.zip.sha256sum".into())
        );
        assert_eq!(deno.version, "2.9.8");
        let mac = Platform {
            os: Os::MacOs,
            arm: true,
        };
        assert_eq!(
            choose(Tool::Deno, mac, "v2.9.8", &assets).unwrap().asset,
            "deno-aarch64-apple-darwin.zip"
        );
        // A build missing from the release.
        let linux_arm = Platform {
            os: Os::Linux,
            arm: true,
        };
        assert!(choose(Tool::YtDlp, linux_arm, "x", &assets).is_err());
    }

    #[test]
    fn checksums_are_read_from_lists_and_single_files() {
        let digest = "37a4506c3efdc1c5e2a2a1d1b6b6c1f8d0e0a5e3b4c2d1f0e9a8b7c6d5e4f3a2";
        let list = format!(
            "{}  other.zip\n{digest}  ffmpeg.zip\n{} *yt-dlp.exe\n",
            "0".repeat(64),
            "A".repeat(64)
        );
        assert_eq!(listed_sha256(&list, "ffmpeg.zip").as_deref(), Some(digest));
        assert_eq!(
            listed_sha256(&list, "yt-dlp.exe"),
            Some("a".repeat(64)),
            "binary-mode names and upper case"
        );
        assert_eq!(listed_sha256(&list, "missing.zip"), None);

        let powershell = format!(
            "\r\nAlgorithm       Hash                                                                   Path\r\n\
             ---------       ----                                                                   ----\r\n\
             SHA256          {}       D:\\a\\deno\\deno\\target\\release\\deno-x86_64-pc-windows-msvc.zip\r\n",
            digest.to_uppercase()
        );
        assert_eq!(single_sha256(&powershell).as_deref(), Some(digest));
        assert_eq!(
            single_sha256(&format!("{digest}  deno.zip")).as_deref(),
            Some(digest)
        );
        assert_eq!(single_sha256("no digest here"), None);
    }
}
