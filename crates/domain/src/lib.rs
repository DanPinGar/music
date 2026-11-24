//! Segno domain model.
//!
//! Entities, business rules and ports (traits). Adapters such as `segno-storage`
//! implement the ports; nothing here knows about SQLite, HTTP or Tauri.

mod error;
mod video;

pub use error::{DomainError, Result};
pub use video::{Video, VideoId, VideoRepository, VideoSource, VideoStatus};
