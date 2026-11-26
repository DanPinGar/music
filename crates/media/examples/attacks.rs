//! Writes the attacks (note onsets) of a media file as the app stores them, for evaluation:
//!
//! ```text
//! cargo run --release -p segno-media --example attacks -- video.mp4 attacks.json
//! ```

use std::path::PathBuf;

use segno_domain::MediaPreparer;
use segno_media::{FfmpegPreparer, MediaTools, dev_tools_dir};

#[tokio::main]
async fn main() {
    let mut args = std::env::args_os().skip(1).map(PathBuf::from);
    let (Some(source), Some(target)) = (args.next(), args.next()) else {
        eprintln!("usage: attacks <media file> <attacks.json>");
        std::process::exit(2);
    };
    let Some(tools) = MediaTools::discover(&[dev_tools_dir()]).await else {
        eprintln!("ffmpeg was not found: run `pnpm tools`");
        std::process::exit(1);
    };
    let started = std::time::Instant::now();
    if let Err(err) = FfmpegPreparer::new(tools)
        .make_attacks(&source, &target)
        .await
    {
        eprintln!("{err}");
        std::process::exit(1);
    }
    eprintln!(
        "attacks written to {} in {:.2} s",
        target.display(),
        started.elapsed().as_secs_f64()
    );
}
