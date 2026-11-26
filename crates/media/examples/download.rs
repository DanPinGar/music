//! Downloads a YouTube video as the app does, printing its details and progress, to try the
//! yt-dlp setup:
//!
//! ```text
//! cargo run -p segno-media --example download -- <youtube address> <folder>
//! ```

use std::io::Write;
use std::path::PathBuf;

use segno_domain::download::VideoDownloader;
use segno_media::{MediaTools, YtDlpDownloader, dev_tools_root};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(url), Some(folder)) = (args.next(), args.next().map(PathBuf::from)) else {
        eprintln!("usage: download <youtube address> <folder>");
        std::process::exit(2);
    };
    let root = dev_tools_root();
    let ffmpeg = MediaTools::discover(&[root.join("ffmpeg").join("bin")]).await;
    let Some(downloader) = YtDlpDownloader::discover(
        &[root.join("yt-dlp")],
        &[root.join("deno")],
        ffmpeg.as_ref(),
    )
    .await
    else {
        eprintln!("yt-dlp was not found: run `pnpm tools`");
        std::process::exit(1);
    };
    println!(
        "yt-dlp {} (JavaScript runtime: {})",
        downloader.version,
        if downloader.has_javascript_runtime() {
            "Deno"
        } else {
            "none"
        }
    );

    let started = std::time::Instant::now();
    match downloader.fetch_info(&url).await {
        Ok(info) => println!(
            "{info:#?}\n(details in {:.1} s)",
            started.elapsed().as_secs_f64()
        ),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }

    std::fs::create_dir_all(&folder).unwrap();
    let target = folder.join("original.mp4");
    let started = std::time::Instant::now();
    let result = downloader
        .download(&url, &target, &|done| {
            print!("\r{:5.1} %", done * 100.0);
            let _ = std::io::stdout().flush();
        })
        .await;
    println!();
    match result {
        Ok(()) => println!(
            "{} ({:.1} MB) in {:.1} s",
            target.display(),
            std::fs::metadata(&target).unwrap().len() as f64 / 1e6,
            started.elapsed().as_secs_f64()
        ),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}
