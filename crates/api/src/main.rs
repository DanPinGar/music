//! `segno-api`: serves the core API outside the desktop app (UI development in a
//! browser, API tests) and exports its OpenAPI document.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use segno_api::{SecurityConfig, ToolsSetup};
use segno_media::dev_tools_root;
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
        /// Library folder: library.db and the media (created if missing).
        #[arg(long, default_value = "target/segno-dev-library")]
        library: PathBuf,
        /// Origin allowed by CORS (repeatable).
        #[arg(long = "allow-origin", default_value = "http://localhost:5173")]
        allowed_origins: Vec<String>,
        /// Folder where the core installs and updates ffmpeg, yt-dlp and Deno by itself; when
        /// omitted it uses the repository's `.tools` (filled by `pnpm tools`) and the PATH.
        #[arg(long)]
        tools: Option<PathBuf>,
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
            library,
            allowed_origins,
            tools,
        } => serve(port, token, library, allowed_origins, tools).await,
        Command::Openapi { out } => export_openapi(out),
    }
}

async fn serve(
    port: u16,
    token: String,
    library: PathBuf,
    allowed_origins: Vec<String>,
    tools: Option<PathBuf>,
) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,segno=debug".into()),
        )
        .init();

    let tools = match tools {
        Some(dir) => ToolsSetup {
            managed: Some(dir),
            roots: Vec::new(),
        },
        None => ToolsSetup {
            managed: None,
            roots: vec![dev_tools_root()],
        },
    };
    let state = segno_api::local_state(&library, tools).await?;
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
