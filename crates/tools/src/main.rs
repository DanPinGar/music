//! `segno-tools`: installs or updates ffmpeg, yt-dlp and Deno in a folder, as the app does in
//! its own. `pnpm tools` runs it on the repository's `.tools` for development.

use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use segno_tools::{Check, Tool, ToolManager, ToolState, ToolsConfig};

#[derive(Parser)]
#[command(
    name = "segno-tools",
    version,
    about = "Install or update ffmpeg, yt-dlp and Deno"
)]
struct Cli {
    /// Tools folder.
    #[arg(long, default_value = ".tools")]
    dir: PathBuf,
    /// Also replace installed tools that have a newer release.
    #[arg(long)]
    update: bool,
    /// Which tools: ffmpeg, yt-dlp, deno (all when omitted).
    #[arg(value_parser = parse_tool)]
    tools: Vec<Tool>,
}

fn parse_tool(name: &str) -> Result<Tool, String> {
    Tool::from_name(name).ok_or_else(|| format!("unknown tool {name:?}: ffmpeg, yt-dlp or deno"))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let tools = if cli.tools.is_empty() {
        Tool::ALL.to_vec()
    } else {
        cli.tools
    };
    segno_tools::apply_staged(&cli.dir);
    let manager = ToolManager::new(ToolsConfig::new(&cli.dir), |_| async {})?;

    let reporter = tokio::spawn({
        let manager = manager.clone();
        async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                for status in manager.statuses() {
                    if let ToolState::Installing { progress } = status.state {
                        println!("{}: {:.0} %", status.tool.name(), progress * 100.0);
                    }
                }
            }
        }
    });
    let check = if cli.update {
        Check::All
    } else {
        Check::Missing
    };
    manager.run(&tools, check).await;
    reporter.abort();
    let errors: Vec<(Tool, String)> = manager
        .statuses()
        .into_iter()
        .filter_map(|status| Some((status.tool, status.error?)))
        .collect();
    // Nothing here runs ffmpeg: a staged update can go in place now.
    segno_tools::apply_staged(&cli.dir);

    let mut failed = false;
    let installed = ToolManager::new(ToolsConfig::new(&cli.dir), |_| async {})?;
    for status in installed.statuses() {
        if !tools.contains(&status.tool) {
            continue;
        }
        let name = status.tool.name();
        let error = errors
            .iter()
            .find(|(tool, _)| *tool == status.tool)
            .map(|(_, error)| error.as_str());
        match (&status.state, error) {
            (ToolState::Ready, error) => {
                let version = status.version.as_deref().unwrap_or("(version unknown)");
                println!(
                    "{name} {version} in {}",
                    status.tool.folder(&cli.dir).display()
                );
                if let Some(error) = error {
                    eprintln!("  could not look for updates: {error}");
                }
            }
            (ToolState::Unsupported, _) => {
                println!(
                    "{name}: no build for this computer: install it with your package manager"
                );
            }
            (_, error) => {
                failed = true;
                eprintln!(
                    "{name}: not installed: {}",
                    error.unwrap_or("unknown error")
                );
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
    Ok(())
}
