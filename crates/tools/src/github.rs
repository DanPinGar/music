//! GitHub releases: the newest release of a repository, and downloads of its assets.

use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::Error;

/// GitHub's API, where releases are listed.
pub const GITHUB_API: &str = "https://api.github.com";

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Release {
    pub tag_name: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Asset {
    pub name: String,
    pub size: u64,
    pub browser_download_url: String,
    /// When this file was uploaded: ffmpeg's rolling release keeps names and replaces files.
    pub updated_at: DateTime<Utc>,
    /// `sha256:<hex>`, computed by GitHub on upload.
    #[serde(default)]
    pub digest: Option<String>,
}

impl Release {
    pub fn asset(&self, name: &str) -> Result<&Asset, Error> {
        self.assets
            .iter()
            .find(|asset| asset.name == name)
            .ok_or_else(|| Error::Release(format!("{name} is missing from the release")))
    }
}

/// A client for GitHub's releases.
#[derive(Debug, Clone)]
pub(crate) struct GitHub {
    client: reqwest::Client,
    api: String,
}

impl GitHub {
    pub fn new(api: &str) -> Result<Self, Error> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("segno/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(30))
            .read_timeout(Duration::from_secs(60))
            .build()
            .map_err(|err| Error::Network(err.to_string()))?;
        Ok(Self {
            client,
            api: api.trim_end_matches('/').to_owned(),
        })
    }

    /// The newest release of `repository` (`owner/name`).
    pub async fn latest(&self, repository: &str) -> Result<Release, Error> {
        let url = &format!("{}/repos/{repository}/releases/latest", self.api);
        let body = retrying(|| async move {
            let response = self
                .client
                .get(url)
                .header("Accept", "application/vnd.github+json")
                .send()
                .await?
                .error_for_status()?;
            response.bytes().await
        })
        .await?;
        serde_json::from_slice(&body)
            .map_err(|err| Error::Release(format!("unexpected release of {repository}: {err}")))
    }

    /// A small text asset, such as a checksums list.
    pub async fn text(&self, url: &str) -> Result<String, Error> {
        let body = retrying(|| async move {
            self.client
                .get(url)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await
        })
        .await?;
        Ok(String::from_utf8_lossy(&body).into_owned())
    }

    /// Downloads `asset` into `path`, reporting progress from 0 to 1, and returns its SHA-256.
    pub async fn download(
        &self,
        asset: &Asset,
        path: &Path,
        progress: &(dyn Fn(f64) + Send + Sync),
    ) -> Result<String, Error> {
        retrying(|| async move {
            let mut response = self
                .client
                .get(&asset.browser_download_url)
                .send()
                .await?
                .error_for_status()?;
            let mut file = tokio::fs::File::create(path).await?;
            let mut hasher = Sha256::new();
            let mut done = 0_u64;
            while let Some(chunk) = response.chunk().await? {
                hasher.update(&chunk);
                file.write_all(&chunk).await?;
                done += chunk.len() as u64;
                if asset.size > 0 {
                    progress((done as f64 / asset.size as f64).min(1.0));
                }
            }
            file.flush().await?;
            Ok::<_, Attempt>(hex(&hasher.finalize()))
        })
        .await
    }
}

/// Lower-case hex of a digest.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Why one attempt failed, and whether trying again may help.
#[derive(Debug)]
enum Attempt {
    Http(reqwest::Error),
    Io(std::io::Error),
}

impl From<reqwest::Error> for Attempt {
    fn from(err: reqwest::Error) -> Self {
        Self::Http(err)
    }
}

impl From<std::io::Error> for Attempt {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl Attempt {
    /// Dropped connections, timeouts and server errors pass; refusals (404, rate limits) and
    /// local disk errors do not.
    fn transient(&self) -> bool {
        match self {
            Self::Http(err) => err.status().is_none_or(|status| status.is_server_error()),
            Self::Io(_) => false,
        }
    }
}

impl From<Attempt> for Error {
    fn from(attempt: Attempt) -> Self {
        match attempt {
            Attempt::Http(err) => Error::Network(describe(&err)),
            Attempt::Io(err) => Error::Io(err),
        }
    }
}

/// reqwest's message with its causes ("error sending request: … connection refused").
fn describe(err: &reqwest::Error) -> String {
    let mut text = err.to_string();
    let mut source = std::error::Error::source(err);
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

/// Runs a network action up to three times: GitHub drops connections now and then.
async fn retrying<T, E, F, Attempted>(action: F) -> Result<T, Error>
where
    F: Fn() -> Attempted,
    Attempted: Future<Output = Result<T, E>>,
    E: Into<Attempt>,
{
    const ATTEMPTS: u32 = 3;
    let mut number = 1;
    loop {
        match action().await.map_err(Into::into) {
            Ok(value) => return Ok(value),
            Err(attempt) if number < ATTEMPTS && attempt.transient() => {
                tracing::debug!(error = ?attempt, number, "network error, trying again");
                tokio::time::sleep(Duration::from_secs(2 * u64::from(number))).await;
                number += 1;
            }
            Err(attempt) => return Err(attempt.into()),
        }
    }
}
