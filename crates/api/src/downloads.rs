use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use segno_domain::download::{self, DownloadContext, DownloadStage, VideoDownloader};
use segno_domain::{MarkerRepository, MediaProbe, MediaStore, VideoId, VideoRepository};
use tokio::sync::{mpsc, oneshot};

use crate::Preparations;

/// Where a video is in its download.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DownloadProgress {
    /// `None` while the video waits for its turn.
    pub stage: Option<DownloadStage>,
    /// Progress of the whole download, 0.0 to 1.0.
    pub done: f64,
}

impl DownloadProgress {
    const QUEUED: Self = Self {
        stage: None,
        done: 0.0,
    };

    /// Reading the details takes seconds; downloading is nearly all of it.
    fn of(stage: DownloadStage, stage_done: f64) -> Self {
        let (start, weight) = match stage {
            DownloadStage::Fetching => (0.0, 0.05),
            DownloadStage::Downloading => (0.05, 0.9),
            DownloadStage::Finishing => (0.95, 0.05),
        };
        Self {
            stage: Some(stage),
            done: start + weight * stage_done.clamp(0.0, 1.0),
        }
    }
}

/// What a download job needs.
pub struct DownloadServices {
    pub videos: Arc<dyn VideoRepository>,
    pub markers: Arc<dyn MarkerRepository>,
    pub probe: Arc<dyn MediaProbe>,
    pub store: Arc<dyn MediaStore>,
    pub downloader: Arc<dyn VideoDownloader>,
    /// Prepares each video for navigation once downloaded.
    pub preparations: Arc<Preparations>,
}

struct Job {
    progress: DownloadProgress,
    /// Dropping it (with the job) cancels the download.
    _cancel: oneshot::Sender<()>,
    /// Taken by the worker when the download starts.
    cancelled: Option<oneshot::Receiver<()>>,
}

/// Background queue that downloads videos one at a time and remembers the progress of the
/// ones queued or downloading. A download can be cancelled at any point: yt-dlp is stopped.
pub struct Downloads {
    queue: mpsc::UnboundedSender<VideoId>,
    jobs: Arc<Mutex<HashMap<VideoId, Job>>>,
}

impl Downloads {
    /// Starts the worker on the current Tokio runtime.
    pub fn start(services: DownloadServices) -> Arc<Self> {
        let (queue, mut pending) = mpsc::unbounded_channel::<VideoId>();
        let jobs: Arc<Mutex<HashMap<VideoId, Job>>> = Arc::new(Mutex::new(HashMap::new()));
        let worker_jobs = jobs.clone();
        tokio::spawn(async move {
            while let Some(id) = pending.recv().await {
                let Some(cancelled) = lock(&worker_jobs)
                    .get_mut(&id)
                    .and_then(|job| job.cancelled.take())
                else {
                    continue; // Cancelled while queued.
                };
                let reporter = worker_jobs.clone();
                let report = move |stage: DownloadStage, done: f64| {
                    if let Some(job) = lock(&reporter).get_mut(&id) {
                        job.progress = DownloadProgress::of(stage, done);
                    }
                };
                let context = DownloadContext {
                    videos: services.videos.as_ref(),
                    markers: services.markers.as_ref(),
                    probe: services.probe.as_ref(),
                    store: services.store.as_ref(),
                    downloader: services.downloader.as_ref(),
                };
                let started = std::time::Instant::now();
                tokio::select! {
                    result = download::download_video(&context, id, &report) => match result {
                        Ok(_) => {
                            tracing::info!(%id, seconds = started.elapsed().as_secs_f64(), "video downloaded");
                            services.preparations.enqueue(id);
                        }
                        Err(err) => tracing::warn!(%id, error = %err, "downloading the video failed"),
                    },
                    // Dropping the download stops yt-dlp (its process is killed on drop).
                    _ = cancelled => {
                        tracing::info!(%id, "download cancelled");
                        let _ = services.store.remove_all(id).await;
                    }
                }
                lock(&worker_jobs).remove(&id);
            }
        });
        Arc::new(Self { queue, jobs })
    }

    /// Queues a video unless it is already queued or downloading.
    pub fn enqueue(&self, id: VideoId) {
        let mut jobs = lock(&self.jobs);
        if jobs.contains_key(&id) {
            return;
        }
        let (cancel, cancelled) = oneshot::channel();
        jobs.insert(
            id,
            Job {
                progress: DownloadProgress::QUEUED,
                _cancel: cancel,
                cancelled: Some(cancelled),
            },
        );
        if self.queue.send(id).is_err() {
            jobs.remove(&id);
            tracing::error!(%id, "the download worker has stopped");
        }
    }

    /// Stops a video's download, queued or running; false if it had none.
    pub fn cancel(&self, id: VideoId) -> bool {
        // Dropping the job drops its sender, which wakes the worker's `cancelled`.
        lock(&self.jobs).remove(&id).is_some()
    }

    /// Whether no video is queued or downloading.
    pub fn is_idle(&self) -> bool {
        lock(&self.jobs).is_empty()
    }

    /// Progress of a video that is queued or downloading; `None` otherwise.
    pub fn progress(&self, id: VideoId) -> Option<DownloadProgress> {
        lock(&self.jobs).get(&id).map(|job| job.progress)
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
    fn overall_progress_is_mostly_the_download() {
        assert_eq!(DownloadProgress::of(DownloadStage::Fetching, 0.0).done, 0.0);
        assert!((DownloadProgress::of(DownloadStage::Downloading, 0.5).done - 0.5).abs() < 1e-9);
        assert!((DownloadProgress::of(DownloadStage::Finishing, 1.0).done - 1.0).abs() < 1e-9);
    }
}
