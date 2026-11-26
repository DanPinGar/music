//! The services that run external tools. They start when their tools are found, which may be
//! after start-up: the core installs missing tools in the background.

use std::sync::{Arc, PoisonError, RwLock};

use segno_domain::MediaProbe;

use crate::{Downloads, Preparations};

/// Versions of the external tools in use, as they report them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolVersions {
    pub ffmpeg: Option<String>,
    pub ytdlp: Option<String>,
    pub deno: Option<String>,
}

/// What the tools found so far make possible.
#[derive(Clone, Default)]
pub struct ToolServices {
    /// Reads media files; `None` without ffmpeg, so imports are refused.
    pub probe: Option<Arc<dyn MediaProbe>>,
    /// Prepares videos for navigation in the background; `None` without ffmpeg.
    pub preparations: Option<Arc<Preparations>>,
    /// Downloads from YouTube in the background; `None` without yt-dlp (or ffmpeg).
    pub downloads: Option<Arc<Downloads>>,
    pub versions: ToolVersions,
}

/// The current [`ToolServices`], replaced as tools appear. Requests take a snapshot.
#[derive(Default)]
pub struct SharedServices(RwLock<ToolServices>);

impl SharedServices {
    pub fn new(services: ToolServices) -> Self {
        Self(RwLock::new(services))
    }

    pub fn get(&self) -> ToolServices {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn set(&self, services: ToolServices) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = services;
    }
}
