//! Media adapters: locating ffmpeg and ffprobe and reading media files with them.

mod probe;
mod tools;

pub use probe::FfprobeProbe;
pub use tools::{MediaTools, dev_tools_dir};
