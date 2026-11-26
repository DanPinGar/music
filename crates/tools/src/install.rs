//! Getting a build onto disk: find it, download it, check it, unpack it and put it in place.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{TimeDelta, Utc};

use crate::Error;
use crate::catalog::{self, Build, Checksums, Packaging, Platform, Tool};
use crate::github::{Asset, GitHub};
use crate::manifest::{Installed, Manifest};

/// ffmpeg's builds are rebuilt every day under the same name: a newer one is only worth its
/// 80 MB once it is a month newer (or on a new branch).
const FFMPEG_REFRESH: TimeDelta = TimeDelta::days(30);

/// The newest build of a tool for this computer, with its expected SHA-256.
#[derive(Debug, Clone)]
pub(crate) struct Offer {
    pub tool: Tool,
    pub build: Build,
    pub asset: Asset,
    pub sha256: String,
}

/// Looks up the newest release of `tool` and the checksum published with it.
pub(crate) async fn offer(github: &GitHub, tool: Tool, platform: Platform) -> Result<Offer, Error> {
    let release = github.latest(tool.repository()).await?;
    let build = catalog::choose(tool, platform, &release.tag_name, &release.assets)?;
    let asset = release.asset(&build.asset)?.clone();
    let sha256 = match &build.checksums {
        Checksums::List(name) => {
            let list = github
                .text(&release.asset(name)?.browser_download_url)
                .await?;
            catalog::listed_sha256(&list, &asset.name)
        }
        Checksums::Single(name) => {
            let text = github
                .text(&release.asset(name)?.browser_download_url)
                .await?;
            catalog::single_sha256(&text)
        }
    }
    .ok_or_else(|| Error::Release(format!("no checksum is published for {}", asset.name)))?;
    // GitHub hashes every upload too: both must agree.
    if let Some(digest) = asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        && !digest.eq_ignore_ascii_case(&sha256)
    {
        return Err(Error::Release(format!(
            "the checksum published for {} differs from GitHub's",
            asset.name
        )));
    }
    Ok(Offer {
        tool,
        build,
        asset,
        sha256,
    })
}

impl Offer {
    /// Whether installing it improves on `current` (unknown: always).
    pub fn improves(&self, current: Option<&Installed>) -> bool {
        let Some(current) = current else {
            return true;
        };
        if self.asset.name != current.asset {
            return true; // A new ffmpeg branch.
        }
        match self.tool {
            Tool::Ffmpeg => self.asset.updated_at - current.built_at >= FFMPEG_REFRESH,
            Tool::YtDlp | Tool::Deno => self.build.version != current.version,
        }
    }

    /// The manifest's record of it, installed now.
    pub fn installed(&self) -> Installed {
        Installed {
            version: self.build.version.clone(),
            asset: self.asset.name.clone(),
            sha256: self.sha256.clone(),
            built_at: self.asset.updated_at,
            installed_at: Utc::now(),
        }
    }
}

/// Where an installed build ended up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Placed {
    /// In place: the next run of the tool uses it.
    Ready,
    /// Unpacked next to the folder in use, which it replaces at the next start.
    Staged,
}

/// Downloads `offer` into `root`, checks its SHA-256 and puts it in place. A program replaces
/// the one there even while it runs (renaming a running program works, even on Windows). ffmpeg's
/// folder holds libraries that running ffmpegs keep open, so an update waits for the next start.
pub(crate) async fn install(
    github: &GitHub,
    root: &Path,
    offer: &Offer,
    progress: &(dyn Fn(f64) + Send + Sync),
) -> Result<Placed, Error> {
    let work = work_dir(root);
    tokio::fs::create_dir_all(&work).await?;
    let download = work.join(&offer.asset.name);
    let result = async {
        let sha256 = github.download(&offer.asset, &download, progress).await?;
        if sha256 != offer.sha256 {
            return Err(Error::Checksum {
                asset: offer.asset.name.clone(),
                expected: offer.sha256.clone(),
                actual: sha256,
            });
        }
        let (root, tool, packaging, file) = (
            root.to_owned(),
            offer.tool,
            offer.build.packaging,
            download.clone(),
        );
        tokio::task::spawn_blocking(move || place(&root, tool, packaging, &file))
            .await
            .map_err(|err| Error::Io(io::Error::other(err)))?
    }
    .await;
    let _ = tokio::fs::remove_file(&download).await;
    result
}

/// Partial downloads and unpacked archives, removed at every start.
fn work_dir(root: &Path) -> PathBuf {
    root.join(".work")
}

/// The folder an update of `tool` waits in.
fn staged_folder(root: &Path, tool: Tool) -> PathBuf {
    root.join(format!("{}.next", tool.name()))
}

fn place(root: &Path, tool: Tool, packaging: Packaging, download: &Path) -> Result<Placed, Error> {
    let target = tool.executable(root);
    let staging = work_dir(root).join(tool.name());
    match packaging {
        Packaging::Program => {
            make_executable(download)?;
            swap_file(download, &target)?;
            Ok(Placed::Ready)
        }
        Packaging::Zip => {
            reset_dir(&staging)?;
            let program = catalog::executable(tool.name());
            unzip(download, &staging, |path| {
                (path == Path::new(&program)).then(|| path.to_owned())
            })?;
            let unpacked = staging.join(&program);
            if !unpacked.is_file() {
                return Err(Error::Archive(format!(
                    "{program} is missing from the archive"
                )));
            }
            make_executable(&unpacked)?;
            swap_file(&unpacked, &target)?;
            let _ = fs::remove_dir_all(&staging);
            Ok(Placed::Ready)
        }
        Packaging::ZipFolder => {
            reset_dir(&staging)?;
            // Only the programs, their libraries and the licence: not ffplay, headers or docs.
            unzip(download, &staging, |path| {
                let mut parts = path.components();
                parts.next()?; // The archive's single folder.
                let inner = parts.as_path();
                let program = inner.file_stem().and_then(|stem| stem.to_str());
                let keep = inner == Path::new("LICENSE.txt")
                    || (inner.starts_with("bin") && program != Some("ffplay"));
                keep.then(|| inner.to_owned())
            })?;
            for program in ["ffmpeg", "ffprobe"] {
                if !staging
                    .join("bin")
                    .join(catalog::executable(program))
                    .is_file()
                {
                    return Err(Error::Archive(format!(
                        "{program} is missing from the archive"
                    )));
                }
            }
            let folder = tool.folder(root);
            if folder.exists() {
                let next = staged_folder(root, tool);
                remove_dir(&next)?;
                rename(&staging, &next)?;
                Ok(Placed::Staged)
            } else {
                rename(&staging, &folder)?;
                Ok(Placed::Ready)
            }
        }
    }
}

/// Run at start-up, before the tools are looked for: puts staged updates in place and removes
/// what earlier runs left behind (partial downloads, programs replaced while they ran).
/// Returns the tools updated.
pub fn apply_staged(root: &Path) -> Vec<Tool> {
    let _ = fs::remove_dir_all(work_dir(root));
    let mut manifest = Manifest::load(root);
    let mut applied = Vec::new();
    for tool in Tool::ALL {
        let _ = fs::remove_file(old_path(&tool.executable(root)));
        let old = root.join(format!("{}.old", tool.name()));
        let _ = fs::remove_dir_all(&old);
        let next = staged_folder(root, tool);
        if !next.is_dir() {
            continue;
        }
        let folder = tool.folder(root);
        let result = (|| {
            if folder.exists() {
                rename(&folder, &old)?;
            }
            if let Err(err) = rename(&next, &folder) {
                let _ = fs::rename(&old, &folder);
                return Err(err);
            }
            Ok::<_, io::Error>(())
        })();
        match result {
            Ok(()) => {
                let _ = fs::remove_dir_all(&old);
                if let Some(staged) = manifest.staged.remove(&tool) {
                    manifest.installed.insert(tool, staged);
                }
                applied.push(tool);
            }
            Err(err) => {
                tracing::warn!(tool = tool.name(), error = %err, "could not apply the update");
            }
        }
    }
    if !applied.is_empty()
        && let Err(err) = manifest.save(root)
    {
        tracing::warn!(error = %err, "could not save tools.json");
    }
    applied
}

/// `<file>.old`, where a replaced program waits until it stops running.
fn old_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(".old");
    path.with_file_name(name)
}

/// Puts `new` at `target`, keeping the old file aside until then so a failure leaves the old
/// one working.
fn swap_file(new: &Path, target: &Path) -> io::Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let old = old_path(target);
    let _ = fs::remove_file(&old);
    let had_one = target.exists();
    if had_one {
        rename(target, &old)?;
    }
    if let Err(err) = rename(new, target) {
        if had_one {
            let _ = fs::rename(&old, target);
        }
        return Err(err);
    }
    // Fails while the old program runs (Windows); `apply_staged` removes it at the next start.
    let _ = fs::remove_file(&old);
    Ok(())
}

/// A rename that tries again for a moment: an antivirus scanning a new file holds it briefly.
fn rename(from: &Path, to: &Path) -> io::Result<()> {
    let mut attempt = 0;
    loop {
        match fs::rename(from, to) {
            Err(err) if err.kind() == io::ErrorKind::PermissionDenied && attempt < 10 => {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(200));
            }
            result => return result,
        }
    }
}

fn remove_dir(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
        _ => Ok(()),
    }
}

fn reset_dir(path: &Path) -> io::Result<()> {
    remove_dir(path)?;
    fs::create_dir_all(path)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

/// Unpacks the files of a zip archive that `select` maps to a path inside `into`.
fn unzip(
    archive: &Path,
    into: &Path,
    select: impl Fn(&Path) -> Option<PathBuf>,
) -> Result<usize, Error> {
    let bad = |err: zip::result::ZipError| Error::Archive(format!("unreadable archive: {err}"));
    let file = io::BufReader::new(fs::File::open(archive)?);
    let mut zip = zip::ZipArchive::new(file).map_err(bad)?;
    let mut unpacked = 0;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(bad)?;
        if entry.is_dir() {
            continue;
        }
        // `enclosed_name` refuses absolute paths and `..`, which would write outside `into`.
        let Some(relative) = entry.enclosed_name().and_then(|name| select(&name)) else {
            continue;
        };
        let path = into.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(&path)?;
        io::copy(&mut entry, &mut out)?;
        unpacked += 1;
    }
    Ok(unpacked)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(tool: Tool, version: &str, asset: &str, uploaded: chrono::DateTime<Utc>) -> Offer {
        Offer {
            tool,
            build: Build {
                asset: asset.into(),
                checksums: Checksums::List("x".into()),
                packaging: Packaging::Program,
                version: version.into(),
            },
            asset: Asset {
                name: asset.into(),
                size: 1,
                browser_download_url: String::new(),
                updated_at: uploaded,
                digest: None,
            },
            sha256: "0".repeat(64),
        }
    }

    #[test]
    fn new_releases_replace_old_ones_and_ffmpeg_waits_a_month() {
        let now = Utc::now();
        let ytdlp = offer(Tool::YtDlp, "2026.11.23", "yt-dlp.exe", now).installed();
        assert!(!offer(Tool::YtDlp, "2026.11.23", "yt-dlp.exe", now).improves(Some(&ytdlp)));
        assert!(offer(Tool::YtDlp, "2026.12.01", "yt-dlp.exe", now).improves(Some(&ytdlp)));
        assert!(offer(Tool::YtDlp, "2026.11.23", "yt-dlp.exe", now).improves(None));

        let name = "ffmpeg-n9.0-latest-win64-lgpl-shared-9.0.zip";
        let ffmpeg = offer(Tool::Ffmpeg, "n9.0", name, now).installed();
        let later = |days| offer(Tool::Ffmpeg, "n9.0", name, now + TimeDelta::days(days));
        assert!(!later(1).improves(Some(&ffmpeg)));
        assert!(later(30).improves(Some(&ffmpeg)));
        let branch = offer(
            Tool::Ffmpeg,
            "n9.1",
            "ffmpeg-n9.1-latest-win64-lgpl-shared-9.1.zip",
            now,
        );
        assert!(branch.improves(Some(&ffmpeg)));
    }

    #[test]
    fn a_running_program_is_replaced_and_its_old_copy_removed_later() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("yt-dlp").join("yt-dlp.exe");
        let new = dir.path().join("new");
        fs::write(&new, "v1").unwrap();
        swap_file(&new, &target).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "v1");

        fs::write(&new, "v2").unwrap();
        // An old copy that could not be removed earlier (it was running).
        fs::write(old_path(&target), "v0").unwrap();
        swap_file(&new, &target).unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "v2");
        assert!(!new.exists());
        assert!(!old_path(&target).exists());
    }

    #[test]
    fn archives_cannot_write_outside_their_folder() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("evil.zip");
        {
            let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
            let options = zip::write::SimpleFileOptions::default();
            for name in ["../escaped.exe", "deno.exe"] {
                zip.start_file(name, options).unwrap();
                zip.write_all(b"x").unwrap();
            }
            zip.finish().unwrap();
        }
        let into = dir.path().join("out");
        let unpacked = unzip(&archive, &into, |path| Some(path.to_owned())).unwrap();
        assert_eq!(unpacked, 1);
        assert!(into.join("deno.exe").is_file());
        assert!(!dir.path().join("escaped.exe").exists());
    }
}
