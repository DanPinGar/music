//! Keeps a tools folder complete and up to date in the background.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::Notify;

use crate::Error;
use crate::catalog::{Platform, Tool};
use crate::github::{GITHUB_API, GitHub};
use crate::install::{self, Offer, Placed};
use crate::manifest::Manifest;

/// How often the background loop wakes to install what is missing and check what is due.
const WAKE_EVERY: Duration = Duration::from_secs(60 * 60);

/// What the manager looks after.
#[derive(Debug, Clone)]
pub struct ToolsConfig {
    /// The folder Segno installs the tools into (`ffmpeg/`, `yt-dlp/`, `deno/`, `tools.json`).
    pub root: PathBuf,
    /// Tools found elsewhere (the repository's `.tools`, an environment variable): left alone.
    pub external: Vec<Tool>,
    /// GitHub's API; tests serve their own releases.
    pub api: String,
    pub platform: Platform,
}

impl ToolsConfig {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            external: Vec::new(),
            api: GITHUB_API.into(),
            platform: Platform::current(),
        }
    }
}

/// Where a tool stands.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolState {
    /// Found outside Segno's folder: Segno neither installs nor updates it.
    External,
    /// No build Segno can install on this computer: the system's is used, if any.
    Unsupported,
    Missing,
    /// Being downloaded and installed, from 0 to 1; an update when the status has a version.
    Installing {
        progress: f64,
    },
    Ready,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolStatus {
    pub tool: Tool,
    pub state: ToolState,
    /// The version Segno installed, if known.
    pub version: Option<String>,
    /// A newer version waiting for the next start.
    pub staged: Option<String>,
    /// When its newest release was last looked up.
    pub checked_at: Option<DateTime<Utc>>,
    /// Why the last install or update check failed.
    pub error: Option<String>,
}

/// Which tools a run looks after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// Only install the missing ones.
    Missing,
    /// Also update those not checked for a while (a day for yt-dlp, a week for the others).
    Due,
    /// Also look for updates of every tool now.
    All,
}

type OnChange = Box<dyn Fn(Tool) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// Installs the missing tools and keeps them up to date, in the background, reporting each
/// tool's state. Every download is checked against the SHA-256 published with its release.
pub struct ToolManager {
    config: ToolsConfig,
    github: GitHub,
    statuses: Mutex<BTreeMap<Tool, ToolStatus>>,
    /// Also serialises the runs' writes to `tools.json`.
    manifest: tokio::sync::Mutex<Manifest>,
    wake: Notify,
    check_all: AtomicBool,
    /// Awaited when a tool was installed or replaced, so its users pick it up before it shows
    /// as ready.
    on_change: OnChange,
}

impl ToolManager {
    pub fn new<F>(
        config: ToolsConfig,
        on_change: impl Fn(Tool) -> F + Send + Sync + 'static,
    ) -> Result<Arc<Self>, Error>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let github = GitHub::new(&config.api)?;
        let manifest = Manifest::load(&config.root);
        let statuses = Tool::ALL
            .into_iter()
            .map(|tool| (tool, initial_status(&config, &manifest, tool)))
            .collect();
        Ok(Arc::new(Self {
            config,
            github,
            statuses: Mutex::new(statuses),
            manifest: tokio::sync::Mutex::new(manifest),
            wake: Notify::new(),
            check_all: AtomicBool::new(false),
            on_change: Box::new(move |tool| Box::pin(on_change(tool))),
        }))
    }

    /// The folder it looks after.
    pub fn root(&self) -> &Path {
        &self.config.root
    }

    pub fn statuses(&self) -> Vec<ToolStatus> {
        self.lock().values().cloned().collect()
    }

    pub fn status(&self, tool: Tool) -> ToolStatus {
        self.lock()[&tool].clone()
    }

    /// Starts the background loop: right away, then every hour, installing what is missing
    /// and updating what is due.
    pub fn start(self: &Arc<Self>) {
        let manager = self.clone();
        tokio::spawn(async move {
            loop {
                let check = if manager.check_all.swap(false, Ordering::SeqCst) {
                    Check::All
                } else {
                    Check::Due
                };
                manager.run(&Tool::ALL, check).await;
                tokio::select! {
                    () = manager.wake.notified() => {}
                    () = tokio::time::sleep(WAKE_EVERY) => {}
                }
            }
        });
    }

    /// Asks the background loop to install what is missing and look for updates now.
    pub fn check_now(&self) {
        self.check_all.store(true, Ordering::SeqCst);
        self.wake.notify_one();
    }

    /// Looks after `tools` once, all at the same time.
    pub async fn run(self: &Arc<Self>, tools: &[Tool], check: Check) {
        let jobs: Vec<_> = tools
            .iter()
            .map(|&tool| {
                let manager = self.clone();
                tokio::spawn(async move { manager.maintain(tool, check).await })
            })
            .collect();
        for job in jobs {
            let _ = job.await;
        }
    }

    async fn maintain(&self, tool: Tool, check: Check) {
        let status = self.status(tool);
        match status.state {
            ToolState::External | ToolState::Unsupported | ToolState::Installing { .. } => {}
            _ if !tool.executable(&self.config.root).is_file() => self.install(tool, None).await,
            _ => {
                let due = match check {
                    Check::Missing => false,
                    Check::Due => status
                        .checked_at
                        .is_none_or(|at| Utc::now() - at >= tool.check_every()),
                    Check::All => true,
                };
                if due {
                    self.update(tool).await;
                }
            }
        }
    }

    /// Installs the newest release if it improves on the one installed (or staged).
    async fn update(&self, tool: Tool) {
        let offer = match install::offer(&self.github, tool, self.config.platform).await {
            Ok(offer) => offer,
            Err(err) => return self.failed(tool, &err),
        };
        let now = Utc::now();
        let improves = {
            let mut manifest = self.manifest.lock().await;
            manifest.checked.insert(tool, now);
            self.save(&manifest);
            let current = manifest
                .staged
                .get(&tool)
                .or_else(|| manifest.installed.get(&tool));
            offer.improves(current)
        };
        self.edit(tool, |status| {
            status.checked_at = Some(now);
            status.error = None;
        });
        if improves {
            self.install(tool, Some(offer)).await;
        }
    }

    async fn install(&self, tool: Tool, offer: Option<Offer>) {
        self.edit(tool, |status| {
            status.state = ToolState::Installing { progress: 0.0 };
            status.error = None;
        });
        let result = async {
            let offer = match offer {
                Some(offer) => offer,
                None => install::offer(&self.github, tool, self.config.platform).await?,
            };
            tracing::info!(tool = tool.name(), version = %offer.build.version, size = offer.asset.size, "downloading");
            let progress = |done: f64| {
                self.edit(tool, |status| {
                    status.state = ToolState::Installing { progress: done };
                });
            };
            let placed = install::install(&self.github, &self.config.root, &offer, &progress).await?;
            Ok::<_, Error>((offer, placed))
        }
        .await;
        match result {
            Ok((offer, placed)) => {
                let installed = offer.installed();
                let version = installed.version.clone();
                {
                    let mut manifest = self.manifest.lock().await;
                    manifest.checked.insert(tool, installed.installed_at);
                    match placed {
                        Placed::Ready => {
                            manifest.staged.remove(&tool);
                            manifest.installed.insert(tool, installed.clone());
                        }
                        Placed::Staged => {
                            manifest.staged.insert(tool, installed.clone());
                        }
                    }
                    self.save(&manifest);
                }
                if placed == Placed::Ready {
                    (self.on_change)(tool).await;
                }
                self.edit(tool, |status| {
                    status.state = ToolState::Ready;
                    status.checked_at = Some(installed.installed_at);
                    match placed {
                        Placed::Ready => status.version = Some(version.clone()),
                        Placed::Staged => status.staged = Some(version.clone()),
                    }
                });
                tracing::info!(tool = tool.name(), %version, staged = placed == Placed::Staged, "installed");
            }
            Err(err) => self.failed(tool, &err),
        }
    }

    fn failed(&self, tool: Tool, err: &Error) {
        tracing::warn!(tool = tool.name(), error = %err, "could not install or update");
        let present = tool.executable(&self.config.root).is_file();
        self.edit(tool, |status| {
            status.state = match err {
                _ if present => ToolState::Ready,
                Error::Unsupported(_) => ToolState::Unsupported,
                _ => ToolState::Missing,
            };
            status.error = Some(err.to_string());
        });
    }

    fn save(&self, manifest: &Manifest) {
        if let Err(err) = manifest.save(&self.config.root) {
            tracing::warn!(error = %err, "could not save tools.json");
        }
    }

    fn edit(&self, tool: Tool, change: impl FnOnce(&mut ToolStatus)) {
        if let Some(status) = self.lock().get_mut(&tool) {
            change(status);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<Tool, ToolStatus>> {
        self.statuses.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn initial_status(config: &ToolsConfig, manifest: &Manifest, tool: Tool) -> ToolStatus {
    let external = config.external.contains(&tool);
    let state = if external {
        ToolState::External
    } else if tool.executable(&config.root).is_file() {
        ToolState::Ready
    } else if !config.platform.supports(tool) {
        ToolState::Unsupported
    } else {
        ToolState::Missing
    };
    let managed = |map: &BTreeMap<Tool, crate::manifest::Installed>| {
        map.get(&tool)
            .filter(|_| !external)
            .map(|installed| installed.version.clone())
    };
    ToolStatus {
        tool,
        version: managed(&manifest.installed).filter(|_| state == ToolState::Ready),
        staged: managed(&manifest.staged),
        checked_at: manifest.checked.get(&tool).copied().filter(|_| !external),
        state,
        error: None,
    }
}
