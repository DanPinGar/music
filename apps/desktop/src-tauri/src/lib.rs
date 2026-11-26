//! Segno desktop shell: opens the library, starts the embedded core API on 127.0.0.1 and
//! tells the UI where to find it.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use segno_api::SecurityConfig;
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
        .plugin(tauri_plugin_dialog::init())
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
    let app_data = app.path().app_data_dir()?;
    // SEGNO_LIBRARY_DIR points tests and agents at a throwaway library, away from the user's.
    let library_dir = std::env::var_os("SEGNO_LIBRARY_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| app_data.join("library"));
    // The core installs its tools (ffmpeg, yt-dlp, Deno) in the app's data folder and keeps
    // them up to date. While developing it uses the repository's `.tools` instead, unless
    // SEGNO_TOOLS_DIR points it at a throwaway folder to try installing them.
    let tools_dir = std::env::var_os("SEGNO_TOOLS_DIR").map(PathBuf::from);
    let mut roots = Vec::new();
    if cfg!(debug_assertions) && tools_dir.is_none() {
        roots.push(segno_media::dev_tools_root());
    }
    let tools = segno_api::ToolsSetup {
        managed: Some(tools_dir.unwrap_or_else(|| app_data.join("tools"))),
        roots,
    };
    let state = segno_api::local_state(&library_dir, tools).await?;

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
