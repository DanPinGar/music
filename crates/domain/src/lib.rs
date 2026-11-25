//! Segno domain model.
//!
//! Entities, business rules, use cases and ports (traits). Adapters such as `segno-storage`
//! and `segno-media` implement the ports; nothing here knows about SQLite, ffmpeg, HTTP or
//! Tauri.

mod error;
pub mod library;
mod media;
mod video;

pub use error::{DomainError, Result};
pub use media::{
    MediaInfo, MediaPreparer, MediaProbe, MediaStore, PreparationInput, ThumbnailSprite,
};
pub use video::{Video, VideoId, VideoRepository, VideoSource, VideoStatus};
