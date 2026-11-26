//! Media adapters: locating ffmpeg and ffprobe, reading media files with them and preparing
//! videos for instant navigation.

pub mod onsets;
mod prepare;
mod probe;
mod tools;

pub use prepare::FfmpegPreparer;
pub use probe::FfprobeProbe;
pub use tools::{MediaTools, dev_tools_dir};
