//! `segno-api`: serves the core API outside the desktop app (UI development in a
//! browser, API tests) and exports its OpenAPI document.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use clap::{Parser, Subcommand};
use segno_api::{AppState, SecurityConfig, SystemInfo};
use segno_storage::SqliteStore;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "segno-api", version, about = "Segno core API")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Serve the API on 127.0.0.1 for development.
    Serve {
        #[arg(long, default_value_t = 7700)]
        port: u16,
        /// Session token clients must present.
        #[arg(long, env = "SEGNO_API_TOKEN", default_value = "dev")]
        token: String,
        /// Library database file (created if missing).
        #[arg(long, default_value = "target/segno-dev.db")]
        db: PathBuf,
        /// Origin allowed by CORS (repeatable).
        #[arg(long = "allow-origin", default_value = "http://localhost:5173")]
        allowed_origins: Vec<String>,
    },
    /// Write the OpenAPI document as JSON.
    Openapi {
        /// Output file; standard output when omitted.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Serve {
            port,
            token,
            db,
            allowed_origins,
        } => serve(port, token, db, allowed_origins).await,
        Command::Openapi { out } => export_openapi(out),
    }
}

async fn serve(
    port: u16,
    token: String,
    db: PathBuf,
    allowed_origins: Vec<String>,
) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,segno=debug".into()),
        )
        .init();

    if let Some(parent) = db.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let store = SqliteStore::open(&db)
        .await
        .with_context(|| format!("opening {}", db.display()))?;
    let state = AppState {
        system: Arc::new(SystemInfo {
            version: env!("CARGO_PKG_VERSION").into(),
            library_path: db.parent().map(|dir| dir.display().to_string()),
            schema_version: store.schema_version().await?,
        }),
        videos: Arc::new(store),
    };
    let origins: Vec<&str> = allowed_origins.iter().map(String::as_str).collect();
    let security = SecurityConfig::local(token, port, &origins);

    let listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
        .await
        .with_context(|| format!("binding 127.0.0.1:{port}"))?;
    tracing::info!("Segno API listening on http://127.0.0.1:{port}");
    segno_api::serve(listener, state, security).await?;
    Ok(())
}

fn export_openapi(out: Option<PathBuf>) -> anyhow::Result<()> {
    let json = segno_api::openapi().to_pretty_json()? + "\n";
    match out {
        Some(path) => {
            std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?
        }
        None => print!("{json}"),
    }
    Ok(())
}
