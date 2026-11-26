//! `tools.json`: what Segno installed in a tools folder, and when it last looked for updates.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Tool;

/// One installed build.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Installed {
    /// What the user sees: `2026.11.23`, `2.9.8`, `n9.0`.
    pub version: String,
    /// The release asset it came from.
    pub asset: String,
    pub sha256: String,
    /// When the asset was uploaded (ffmpeg's rolling builds share one name).
    pub built_at: DateTime<Utc>,
    pub installed_at: DateTime<Utc>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct Manifest {
    #[serde(default)]
    pub installed: BTreeMap<Tool, Installed>,
    /// Downloaded and unpacked, applied at the next start (ffmpeg, whose files are in use).
    #[serde(default)]
    pub staged: BTreeMap<Tool, Installed>,
    /// When each tool was last compared with its newest release.
    #[serde(default)]
    pub checked: BTreeMap<Tool, DateTime<Utc>>,
}

impl Manifest {
    fn path(root: &Path) -> PathBuf {
        root.join("tools.json")
    }

    /// The manifest of `root`; empty when missing or unreadable (the tools are then checked
    /// as if their versions were unknown).
    pub fn load(root: &Path) -> Self {
        match std::fs::read(Self::path(root)) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|err| {
                tracing::warn!(error = %err, "tools.json is unreadable: starting afresh");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Writes the manifest, replacing the old one only once the new one is complete.
    pub fn save(&self, root: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(root)?;
        let path = Self::path(root);
        let partial = path.with_extension("json.partial");
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&partial, json)?;
        std::fs::rename(&partial, &path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_a_missing_or_broken_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Manifest::load(dir.path()).installed.is_empty());

        let mut manifest = Manifest::default();
        let now = Utc::now();
        manifest.installed.insert(
            Tool::YtDlp,
            Installed {
                version: "2026.11.23".into(),
                asset: "yt-dlp.exe".into(),
                sha256: "a".repeat(64),
                built_at: now,
                installed_at: now,
            },
        );
        manifest.checked.insert(Tool::YtDlp, now);
        manifest.save(dir.path()).unwrap();
        let text = std::fs::read_to_string(dir.path().join("tools.json")).unwrap();
        assert!(text.contains("\"yt-dlp\""), "{text}");
        let loaded = Manifest::load(dir.path());
        assert_eq!(loaded.installed[&Tool::YtDlp].version, "2026.11.23");
        assert_eq!(loaded.checked[&Tool::YtDlp], now);

        std::fs::write(dir.path().join("tools.json"), "{ broken").unwrap();
        assert!(Manifest::load(dir.path()).installed.is_empty());
    }
}
