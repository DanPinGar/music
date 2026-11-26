use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use segno_domain::download::VideoDownloader;
use segno_domain::{
    CommentRepository, MarkerRepository, MediaPreparer, MediaProbe, MediaStore, VideoRepository,
    VideoSource, VideoStateRepository, VideoStatus,
};
use segno_media::{FfmpegPreparer, FfprobeProbe, MediaTools, YtDlpDownloader};
use segno_storage::{LibraryFiles, SqliteStore};
use segno_tools::{Tool, ToolManager, ToolsConfig};

use crate::{
    AppState, DownloadServices, Downloads, Preparations, SharedServices, SystemInfo, ToolVersions,
};

/// Where the core finds its tools: ffmpeg in `<root>/ffmpeg/bin`, yt-dlp in `<root>/yt-dlp`,
/// Deno in `<root>/deno`.
#[derive(Debug, Clone, Default)]
pub struct ToolsSetup {
    /// The folder where the core installs the missing tools and keeps them up to date by
    /// itself; `None` to use only the tools it finds.
    pub managed: Option<PathBuf>,
    /// Other folders with tools, looked in first (the repository's `.tools` while developing).
    /// The core leaves the tools found there alone.
    pub roots: Vec<PathBuf>,
}

/// State for serving a library on this computer: `library.db` and the media folder inside
/// `library_dir`. Each tool comes from `$SEGNO_<TOOL>_DIR`, the first folder of `tools` that
/// has it, or the PATH. Without ffmpeg the API serves the library but refuses imports; without
/// yt-dlp it refuses YouTube downloads. With a managed folder, the missing tools are installed
/// in the background, and what they make possible starts as soon as each one is ready.
pub async fn local_state(library_dir: &Path, tools: ToolsSetup) -> anyhow::Result<AppState> {
    std::fs::create_dir_all(library_dir)
        .with_context(|| format!("creating {}", library_dir.display()))?;
    let db = library_dir.join("library.db");
    let store = SqliteStore::open(&db)
        .await
        .with_context(|| format!("opening {}", db.display()))?;

    if let Some(managed) = &tools.managed {
        for tool in segno_tools::apply_staged(managed) {
            tracing::info!(tool = tool.name(), "update applied");
        }
    }
    let mut roots = tools.roots.clone();
    roots.extend(tools.managed.clone());

    let system = Arc::new(SystemInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        library_id: store.library_id().await?,
        library_path: Some(library_dir.display().to_string()),
        schema_version: store.schema_version().await?,
        ffmpeg_version: None,
        ytdlp_version: None,
    });
    let states: Arc<dyn VideoStateRepository> = Arc::new(store.clone());
    let markers: Arc<dyn MarkerRepository> = Arc::new(store.clone());
    let comments: Arc<dyn CommentRepository> = Arc::new(store.clone());
    let videos: Arc<dyn VideoRepository> = Arc::new(store);
    let media_files: Arc<dyn MediaStore> = Arc::new(LibraryFiles::new(library_dir));

    let wiring = Arc::new(Wiring {
        videos: videos.clone(),
        markers: markers.clone(),
        media_files: media_files.clone(),
        roots,
        services: Arc::default(),
        downloads_have_deno: tokio::sync::Mutex::new(false),
    });
    let found = wiring.refresh().await?;

    let manager = match &tools.managed {
        Some(managed) => {
            let config = ToolsConfig {
                external: found.outside(managed),
                ..ToolsConfig::new(managed)
            };
            let changed = wiring.clone();
            let manager = ToolManager::new(config, move |tool| {
                let wiring = changed.clone();
                async move {
                    tracing::info!(
                        tool = tool.name(),
                        "tool installed: looking for the tools again"
                    );
                    if let Err(err) = wiring.refresh().await {
                        tracing::error!(error = %err, "could not start what the new tool allows");
                    }
                }
            })?;
            manager.start();
            Some(manager)
        }
        None => None,
    };

    Ok(AppState {
        videos,
        states,
        markers,
        comments,
        media_files,
        services: wiring.services.clone(),
        tools: manager,
        system,
    })
}

/// Starts the services the tools make possible, at start-up and whenever a tool is installed.
struct Wiring {
    videos: Arc<dyn VideoRepository>,
    markers: Arc<dyn MarkerRepository>,
    media_files: Arc<dyn MediaStore>,
    roots: Vec<PathBuf>,
    services: Arc<SharedServices>,
    /// Whether the running downloads have Deno; also makes refreshes run one at a time.
    downloads_have_deno: tokio::sync::Mutex<bool>,
}

/// Where each tool was found.
struct Found {
    ffmpeg: Option<PathBuf>,
    ytdlp: Option<PathBuf>,
    deno: Option<PathBuf>,
}

impl Found {
    /// The tools found in a folder other than `managed` (not on the PATH): the core leaves
    /// them alone.
    fn outside(&self, managed: &Path) -> Vec<Tool> {
        [
            (Tool::Ffmpeg, &self.ffmpeg),
            (Tool::YtDlp, &self.ytdlp),
            (Tool::Deno, &self.deno),
        ]
        .into_iter()
        .filter(|(_, path)| {
            path.as_deref().is_some_and(|path| {
                path.parent().is_some_and(|dir| dir != Path::new("")) && !path.starts_with(managed)
            })
        })
        .map(|(tool, _)| tool)
        .collect()
    }
}

impl Wiring {
    fn in_roots(&self, folder: &[&str]) -> Vec<PathBuf> {
        self.roots
            .iter()
            .map(|root| {
                folder
                    .iter()
                    .fold(root.clone(), |path, part| path.join(part))
            })
            .collect()
    }

    /// Looks for the tools and starts what they newly allow: imports and preparations once
    /// there is ffmpeg, downloads once there are yt-dlp and ffmpeg. What runs keeps running:
    /// updates of yt-dlp and Deno replace their files in place.
    async fn refresh(&self) -> anyhow::Result<Found> {
        let mut downloads_have_deno = self.downloads_have_deno.lock().await;
        let tools = MediaTools::discover(&self.in_roots(&["ffmpeg", "bin"])).await;
        let downloader = YtDlpDownloader::discover(
            &self.in_roots(&["yt-dlp"]),
            &self.in_roots(&["deno"]),
            tools.as_ref(),
        )
        .await;
        match &tools {
            Some(tools) => {
                tracing::info!(ffmpeg = %tools.ffmpeg.display(), version = %tools.version, "ffmpeg found")
            }
            None => tracing::warn!("ffmpeg not found: importing videos is disabled"),
        }
        match &downloader {
            Some(downloader) if !downloader.has_javascript_runtime() => {
                tracing::warn!(version = %downloader.version, "yt-dlp found without Deno: YouTube downloads will fail")
            }
            Some(downloader) => tracing::info!(version = %downloader.version, "yt-dlp found"),
            None => tracing::warn!("yt-dlp not found: YouTube downloads are disabled"),
        }

        let mut services = self.services.get();
        let mut started_preparations = false;
        if services.probe.is_none()
            && let Some(tools) = &tools
        {
            let preparer: Arc<dyn MediaPreparer> = Arc::new(FfmpegPreparer::new(tools.clone()));
            services.preparations = Some(Preparations::start(
                self.videos.clone(),
                self.media_files.clone(),
                preparer,
            ));
            services.probe =
                Some(Arc::new(FfprobeProbe::new(tools.clone())) as Arc<dyn MediaProbe>);
            started_preparations = true;
        }
        let mut started_downloads = false;
        if let (Some(downloader), Some(probe), Some(preparations)) =
            (&downloader, &services.probe, &services.preparations)
        {
            let start = match &services.downloads {
                None => true,
                // Deno arrived after yt-dlp: start over with it, once nothing is downloading.
                Some(downloads) => {
                    !*downloads_have_deno
                        && downloader.has_javascript_runtime()
                        && downloads.is_idle()
                }
            };
            if start {
                services.downloads = Some(Downloads::start(DownloadServices {
                    videos: self.videos.clone(),
                    markers: self.markers.clone(),
                    probe: probe.clone(),
                    store: self.media_files.clone(),
                    downloader: Arc::new(downloader.clone()) as Arc<dyn VideoDownloader>,
                    preparations: preparations.clone(),
                }));
                *downloads_have_deno = downloader.has_javascript_runtime();
                started_downloads = true;
            }
        }
        services.versions = ToolVersions {
            ffmpeg: tools.as_ref().map(|tools| tools.version.clone()),
            ytdlp: downloader.as_ref().map(|d| d.version.clone()),
            deno: downloader.as_ref().and_then(|d| d.deno_version.clone()),
        };
        let (preparations, downloads) = (services.preparations.clone(), services.downloads.clone());
        self.services.set(services);

        // Finish what an earlier run left half done.
        if started_preparations || started_downloads {
            for video in self.videos.list().await? {
                let interrupted_download =
                    video.source == VideoSource::Youtube && video.status == VideoStatus::Importing;
                if interrupted_download {
                    if let Some(downloads) = downloads.as_ref().filter(|_| started_downloads) {
                        downloads.enqueue(video.id);
                    }
                } else if let Some(preparations) =
                    preparations.as_ref().filter(|_| started_preparations)
                    && video.needs_preparation()
                {
                    preparations.enqueue(video.id);
                }
            }
        }

        Ok(Found {
            ffmpeg: tools.map(|tools| tools.ffmpeg),
            ytdlp: downloader.as_ref().map(|d| d.program().to_owned()),
            deno: downloader
                .as_ref()
                .and_then(|d| d.javascript_runtime().map(Path::to_owned)),
        })
    }
}
