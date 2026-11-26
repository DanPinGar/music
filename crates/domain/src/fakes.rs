//! In-memory stand-ins for the ports, for the domain's tests.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;

use crate::{
    DomainError, Marker, MarkerId, MarkerRepository, MediaInfo, MediaProbe, MediaStore, Result,
    Video, VideoId, VideoRepository,
};

#[derive(Default)]
pub struct Videos {
    pub saved: Mutex<Vec<Video>>,
    /// Inserts fail, as with a full disk.
    pub fail: bool,
}

#[async_trait]
impl VideoRepository for Videos {
    async fn list(&self) -> Result<Vec<Video>> {
        Ok(self.saved.lock().unwrap().clone())
    }
    async fn get(&self, id: VideoId) -> Result<Video> {
        self.saved
            .lock()
            .unwrap()
            .iter()
            .find(|v| v.id == id)
            .cloned()
            .ok_or(DomainError::NotFound {
                entity: "video",
                id: id.to_string(),
            })
    }
    async fn insert(&self, video: &Video) -> Result<()> {
        if self.fail {
            return Err(DomainError::storage(std::io::Error::other("disk full")));
        }
        self.saved.lock().unwrap().push(video.clone());
        Ok(())
    }
    async fn update(&self, video: &Video) -> Result<()> {
        let mut saved = self.saved.lock().unwrap();
        let slot = saved.iter_mut().find(|v| v.id == video.id).unwrap();
        *slot = video.clone();
        Ok(())
    }
    async fn delete(&self, id: VideoId) -> Result<()> {
        self.saved.lock().unwrap().retain(|video| video.id != id);
        Ok(())
    }
}

pub struct Probe(pub MediaInfo);

#[async_trait]
impl MediaProbe for Probe {
    async fn probe(&self, _: &Path) -> Result<MediaInfo> {
        Ok(self.0.clone())
    }
}

/// A 2:21 1080p60 H.264 + AAC video, like the Monk reference.
pub fn video_info() -> MediaInfo {
    MediaInfo {
        duration_ms: 141_154,
        fps: Some(60.0),
        width: Some(1920),
        height: Some(1080),
        video_codec: Some("h264".into()),
        audio_codec: Some("aac".into()),
    }
}

#[derive(Default)]
pub struct Store {
    pub removed: Mutex<Vec<VideoId>>,
}

#[async_trait]
impl MediaStore for Store {
    async fn store_original(&self, video: VideoId, _: &Path) -> Result<String> {
        Ok(format!("media/{video}/original.mp4"))
    }
    async fn derived(&self, video: VideoId, file_name: &str) -> Result<(String, PathBuf)> {
        let relative = format!("media/{video}/{file_name}");
        Ok((relative.clone(), PathBuf::from(relative)))
    }
    fn resolve(&self, relative: &str) -> Result<PathBuf> {
        Ok(PathBuf::from(relative))
    }
    async fn remove_all(&self, video: VideoId) -> Result<()> {
        self.removed.lock().unwrap().push(video);
        Ok(())
    }
}

#[derive(Default)]
pub struct Markers {
    pub saved: Mutex<Vec<Marker>>,
}

#[async_trait]
impl MarkerRepository for Markers {
    async fn list_markers(&self, video: VideoId) -> Result<Vec<Marker>> {
        let mut markers: Vec<Marker> = self
            .saved
            .lock()
            .unwrap()
            .iter()
            .filter(|marker| marker.video == video)
            .cloned()
            .collect();
        markers.sort_by_key(|marker| marker.start_ms);
        Ok(markers)
    }
    async fn get_marker(&self, video: VideoId, id: MarkerId) -> Result<Marker> {
        self.list_markers(video)
            .await?
            .into_iter()
            .find(|marker| marker.id == id)
            .ok_or(DomainError::NotFound {
                entity: "marker",
                id: id.to_string(),
            })
    }
    async fn insert_marker(&self, marker: &Marker) -> Result<()> {
        self.saved.lock().unwrap().push(marker.clone());
        Ok(())
    }
    async fn update_marker(&self, marker: &Marker) -> Result<()> {
        let mut saved = self.saved.lock().unwrap();
        if let Some(slot) = saved.iter_mut().find(|m| m.id == marker.id) {
            *slot = marker.clone();
        }
        Ok(())
    }
    async fn delete_marker(&self, _: VideoId, id: MarkerId) -> Result<()> {
        self.saved.lock().unwrap().retain(|marker| marker.id != id);
        Ok(())
    }
}
