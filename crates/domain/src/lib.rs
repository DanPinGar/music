//! Segno domain model.
//!
//! Entities, business rules, use cases and ports (traits). Adapters such as `segno-storage`
//! and `segno-media` implement the ports; nothing here knows about SQLite, ffmpeg, HTTP or
//! Tauri.

mod comment;
pub mod download;
mod error;
#[cfg(test)]
mod fakes;
pub mod library;
mod marker;
mod media;
mod state;
mod video;

pub use comment::{Comment, CommentFields, CommentId, CommentRepository};
pub use error::{DomainError, Result};
pub use marker::{Marker, MarkerFields, MarkerId, MarkerKind, MarkerRepository};
pub use media::{
    MediaInfo, MediaPreparer, MediaProbe, MediaStore, PreparationInput, ThumbnailSprite,
};
pub use state::{
    BeatGrid, LoopRange, MAX_SPEED, MIN_SPEED, Practice, SpeedTrainer, StepUnit, Studied,
    TimelineView, VideoState, VideoStateRepository,
};
pub use video::{Video, VideoId, VideoRepository, VideoSource, VideoStatus, clean_tags};
