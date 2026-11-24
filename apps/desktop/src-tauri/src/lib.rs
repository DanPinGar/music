//! Segno desktop shell: opens the library database, starts the embedded core API on
//! 127.0.0.1 and tells the UI where to find it.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use anyhow::Context;
use segno_api::{AppState, SecurityConfig, SystemInfo};
use segno_storage::SqliteStore;
use serde::Serialize;
use tauri::Manager;
use tracing_subscriber::EnvFilter;

/// Webview origins allowed to call the API: the packaged app on each platform and,
/// in debug builds only, the Vite dev server.
const WEBVIEW_ORIGINS: &[&str] = &[
    "http://tauri.localhost",
    "https://tauri.localhost",
    "tauri://localhost",
];
const DEV_SERVER_ORIGIN: &str = "http://localhost:5173";

/// Where the UI reaches the embedded API, and the session token it must present.
#[derive(Clone, Serialize)]
struct ApiConfig {
    base_url: String,
    token: String,
}

#[tauri::command]
fn api_config(config: tauri::State<'_, ApiConfig>) -> ApiConfig {
    config.inner().clone()
}

/// Entry point of the desktop app.
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,segno=debug".into()),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
            let config = tauri::async_runtime::block_on(start_core(app.handle()))?;
            app.manage(config);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![api_config])
        .run(tauri::generate_context!())
        .expect("error while running the Segno desktop app");
}

/// Opens the library and serves the API on a random loopback port with a fresh token.
async fn start_core(app: &tauri::AppHandle) -> anyhow::Result<ApiConfig> {
    let library_dir = app.path().app_data_dir()?.join("library");
    std::fs::create_dir_all(&library_dir)
        .with_context(|| format!("creating {}", library_dir.display()))?;
    let store = SqliteStore::open(&library_dir.join("library.db")).await?;

    let state = AppState {
        system: Arc::new(SystemInfo {
            version: env!("CARGO_PKG_VERSION").into(),
            library_path: Some(library_dir.display().to_string()),
            schema_version: store.schema_version().await?,
        }),
        videos: Arc::new(store),
    };

    let listener =
        tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).await?;
    let port = listener.local_addr()?.port();
    let token = session_token();

    let mut origins = WEBVIEW_ORIGINS.to_vec();
    if cfg!(debug_assertions) {
        origins.push(DEV_SERVER_ORIGIN);
    }
    let security = SecurityConfig::local(token.clone(), port, &origins);

    tauri::async_runtime::spawn(async move {
        if let Err(err) = segno_api::serve(listener, state, security).await {
            tracing::error!(error = %err, "the core API stopped");
        }
    });
    tracing::info!(port, "core API listening on 127.0.0.1");

    Ok(ApiConfig {
        base_url: format!("http://127.0.0.1:{port}"),
        token,
    })
}

/// 244 random bits from the operating system, as hex.
fn session_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
