use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use segno_domain::MediaProbe;
use segno_media::{FfprobeProbe, MediaTools};
use segno_storage::{LibraryFiles, SqliteStore};

use crate::{AppState, SystemInfo};

/// State for serving a library on this computer: `library.db` and the media folder inside
/// `library_dir`, and ffmpeg from the first of `tool_dirs` that has it (or the PATH).
/// Without ffmpeg the API still serves the library but refuses imports.
pub async fn local_state(library_dir: &Path, tool_dirs: &[PathBuf]) -> anyhow::Result<AppState> {
    std::fs::create_dir_all(library_dir)
        .with_context(|| format!("creating {}", library_dir.display()))?;
    let db = library_dir.join("library.db");
    let store = SqliteStore::open(&db)
        .await
        .with_context(|| format!("opening {}", db.display()))?;

    let tools = MediaTools::discover(tool_dirs).await;
    match &tools {
        Some(tools) => {
            tracing::info!(ffmpeg = %tools.ffmpeg.display(), version = %tools.version, "media tools found")
        }
        None => tracing::warn!("ffmpeg not found: importing videos is disabled"),
    }

    Ok(AppState {
        system: Arc::new(SystemInfo {
            version: env!("CARGO_PKG_VERSION").into(),
            library_id: store.library_id().await?,
            library_path: Some(library_dir.display().to_string()),
            schema_version: store.schema_version().await?,
            ffmpeg_version: tools.as_ref().map(|tools| tools.version.clone()),
        }),
        probe: tools.map(|tools| Arc::new(FfprobeProbe::new(tools)) as Arc<dyn MediaProbe>),
        media_files: Arc::new(LibraryFiles::new(library_dir)),
        videos: Arc::new(store),
    })
}
