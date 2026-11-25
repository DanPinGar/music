use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use async_trait::async_trait;
use segno_domain::{DomainError, MediaStore, Result, VideoId};

/// The library's media folder on disk: `<root>/media/<video id>/…`.
#[derive(Debug, Clone)]
pub struct LibraryFiles {
    root: PathBuf,
}

impl LibraryFiles {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn video_dir(&self, video: VideoId) -> PathBuf {
        self.root.join("media").join(video.to_string())
    }
}

#[async_trait]
impl MediaStore for LibraryFiles {
    async fn store_original(&self, video: VideoId, source: &Path) -> Result<String> {
        let extension = source
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .filter(|ext| ext.len() <= 8 && ext.chars().all(|c| c.is_ascii_alphanumeric()))
            .unwrap_or_else(|| "bin".into());
        let file_name = format!("original.{extension}");
        let dir = self.video_dir(video);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(DomainError::storage)?;
        let target = dir.join(&file_name);

        // On the same drive a hard link is instant and takes no extra space; the library never
        // writes to originals, and deleting the user's file does not affect the link.
        if tokio::fs::hard_link(source, &target).await.is_err() {
            tokio::fs::copy(source, &target)
                .await
                .map_err(DomainError::storage)?;
        }
        Ok(format!("media/{video}/{file_name}"))
    }

    fn resolve(&self, relative: &str) -> Result<PathBuf> {
        let path = Path::new(relative);
        let inside = !path.as_os_str().is_empty()
            && path
                .components()
                .all(|component| matches!(component, Component::Normal(_)));
        if !inside {
            return Err(DomainError::Invalid(format!(
                "media path outside the library: {relative}"
            )));
        }
        Ok(self.root.join(path))
    }

    async fn remove_all(&self, video: VideoId) -> Result<()> {
        match tokio::fs::remove_dir_all(self.video_dir(video)).await {
            Err(err) if err.kind() != ErrorKind::NotFound => Err(DomainError::storage(err)),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stores_originals_under_the_video_folder() {
        let library = tempfile::tempdir().unwrap();
        let files = LibraryFiles::new(library.path());
        let source = library.path().join("Clip.MP4");
        std::fs::write(&source, b"not really a video").unwrap();
        let video = VideoId::new();

        let relative = files.store_original(video, &source).await.unwrap();

        assert_eq!(relative, format!("media/{video}/original.mp4"));
        let stored = files.resolve(&relative).unwrap();
        assert_eq!(std::fs::read(stored).unwrap(), b"not really a video");
        std::fs::remove_file(&source).unwrap();
        assert!(
            files.resolve(&relative).unwrap().exists(),
            "the copy outlives the source"
        );
    }

    #[tokio::test]
    async fn odd_extensions_become_bin() {
        let library = tempfile::tempdir().unwrap();
        let files = LibraryFiles::new(library.path());
        let source = library.path().join("clip.mp4;rm -rf");
        std::fs::write(&source, b"x").unwrap();
        let relative = files.store_original(VideoId::new(), &source).await.unwrap();
        assert!(relative.ends_with("original.bin"));
    }

    #[test]
    fn refuses_paths_that_leave_the_library() {
        let files = LibraryFiles::new("library");
        assert!(files.resolve("media/x/original.mp4").is_ok());
        for bad in [
            "../secret.txt",
            "media/../../x",
            "/etc/passwd",
            "C:\\Windows",
            "",
        ] {
            assert!(files.resolve(bad).is_err(), "{bad} should be refused");
        }
    }

    #[tokio::test]
    async fn removing_media_is_idempotent() {
        let library = tempfile::tempdir().unwrap();
        let files = LibraryFiles::new(library.path());
        let source = library.path().join("clip.mp4");
        std::fs::write(&source, b"x").unwrap();
        let video = VideoId::new();
        files.store_original(video, &source).await.unwrap();

        files.remove_all(video).await.unwrap();
        files.remove_all(video).await.unwrap();
        assert!(
            !library
                .path()
                .join("media")
                .join(video.to_string())
                .exists()
        );
    }
}
