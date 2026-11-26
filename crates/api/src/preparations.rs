use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use segno_domain::library::{self, PreparationStage};
use segno_domain::{MediaPreparer, MediaStore, VideoId, VideoRepository};
use tokio::sync::mpsc;

/// Where a video is in its preparation for navigation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    /// `None` while the video waits for its turn.
    pub stage: Option<PreparationStage>,
    /// Progress of the whole preparation, 0.0 to 1.0.
    pub done: f64,
}

impl Progress {
    const QUEUED: Self = Self {
        stage: None,
        done: 0.0,
    };

    /// The seek proxy is most of the work; the other steps take a second or two.
    fn of(stage: PreparationStage, stage_done: f64) -> Self {
        let (start, weight) = match stage {
            PreparationStage::Peaks => (0.0, 0.03),
            PreparationStage::Attacks => (0.03, 0.04),
            PreparationStage::SeekProxy => (0.07, 0.9),
            PreparationStage::Thumbnails => (0.97, 0.03),
        };
        Self {
            stage: Some(stage),
            done: start + weight * stage_done.clamp(0.0, 1.0),
        }
    }
}

/// Background queue that prepares videos one at a time (encoding is heavy; one job keeps the
/// computer responsive) and remembers the progress of the videos queued or in preparation.
pub struct Preparations {
    queue: mpsc::UnboundedSender<VideoId>,
    progress: Arc<Mutex<HashMap<VideoId, Progress>>>,
}

impl Preparations {
    /// Starts the worker on the current Tokio runtime.
    pub fn start(
        videos: Arc<dyn VideoRepository>,
        store: Arc<dyn MediaStore>,
        preparer: Arc<dyn MediaPreparer>,
    ) -> Arc<Self> {
        let (queue, mut pending) = mpsc::unbounded_channel::<VideoId>();
        let progress = Arc::new(Mutex::new(HashMap::new()));
        let worker_progress = progress.clone();
        tokio::spawn(async move {
            while let Some(id) = pending.recv().await {
                let reporter = worker_progress.clone();
                let report = move |stage: PreparationStage, done: f64| {
                    lock(&reporter).insert(id, Progress::of(stage, done));
                };
                let started = std::time::Instant::now();
                match library::prepare_video(
                    videos.as_ref(),
                    store.as_ref(),
                    preparer.as_ref(),
                    id,
                    &report,
                )
                .await
                {
                    Ok(_) => {
                        tracing::info!(%id, seconds = started.elapsed().as_secs_f64(), "video prepared for navigation")
                    }
                    Err(err) => tracing::warn!(%id, error = %err, "preparing the video failed"),
                }
                lock(&worker_progress).remove(&id);
            }
        });
        Arc::new(Self { queue, progress })
    }

    /// Queues a video unless it is already queued or being prepared.
    pub fn enqueue(&self, id: VideoId) {
        let mut progress = lock(&self.progress);
        if progress.contains_key(&id) {
            return;
        }
        progress.insert(id, Progress::QUEUED);
        if self.queue.send(id).is_err() {
            progress.remove(&id);
            tracing::error!(%id, "the preparation worker has stopped");
        }
    }

    /// Progress of a video that is queued or being prepared; `None` otherwise.
    pub fn progress(&self, id: VideoId) -> Option<Progress> {
        lock(&self.progress).get(&id).copied()
    }
}

/// The map only holds plain values, so a panic elsewhere cannot leave it inconsistent.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overall_progress_weighs_the_proxy_most() {
        assert_eq!(Progress::of(PreparationStage::Peaks, 0.0).done, 0.0);
        assert_eq!(Progress::of(PreparationStage::SeekProxy, 0.0).done, 0.07);
        assert!((Progress::of(PreparationStage::SeekProxy, 0.5).done - 0.52).abs() < 1e-9);
        assert!((Progress::of(PreparationStage::Thumbnails, 1.0).done - 1.0).abs() < 1e-9);
        assert!((Progress::of(PreparationStage::SeekProxy, 7.0).done - 0.97).abs() < 1e-9);
    }
}
