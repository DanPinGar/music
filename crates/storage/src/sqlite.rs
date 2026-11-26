use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use segno_domain::{DomainError, Result, Video, VideoId, VideoRepository};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{FromRow, SqlitePool};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// The local library database.
#[derive(Clone)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    /// Opens the database file, creating it if needed, and applies pending migrations.
    pub async fn open(path: &Path) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        Self::connect(options, 8).await
    }

    /// A private in-memory database, for tests.
    pub async fn in_memory() -> Result<Self> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .map_err(DomainError::storage)?
            .foreign_keys(true);
        // Every in-memory connection is a separate database, so keep exactly one.
        Self::connect(options, 1).await
    }

    async fn connect(options: SqliteConnectOptions, max_connections: u32) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .connect_with(options)
            .await
            .map_err(DomainError::storage)?;
        MIGRATOR.run(&pool).await.map_err(DomainError::storage)?;
        Ok(Self { pool })
    }

    /// Version of the latest migration applied to this database.
    pub async fn schema_version(&self) -> Result<i64> {
        let version: Option<i64> =
            sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1")
                .fetch_one(&self.pool)
                .await
                .map_err(DomainError::storage)?;
        Ok(version.unwrap_or(0))
    }

    /// Identity of this library, created with the database and never changed.
    pub async fn library_id(&self) -> Result<String> {
        sqlx::query_scalar("SELECT value FROM settings WHERE key = 'library_id'")
            .fetch_one(&self.pool)
            .await
            .map_err(DomainError::storage)
    }
}

macro_rules! video_columns {
    () => {
        "id, source, source_url, title, channel, duration_ms, fps, width, height, status, media_file, proxy_file, thumbnails_file, peaks_file, attacks_file, preparation_error, created_at, updated_at"
    };
}

type SqliteQuery<'q> = sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>;

/// Binds every column after `id`, in the order of `video_columns!`.
fn bind_fields<'q>(query: SqliteQuery<'q>, video: &'q Video) -> SqliteQuery<'q> {
    query
        .bind(video.source.as_str())
        .bind(&video.source_url)
        .bind(&video.title)
        .bind(&video.channel)
        .bind(video.duration_ms)
        .bind(video.fps)
        .bind(video.width.map(i64::from))
        .bind(video.height.map(i64::from))
        .bind(video.status.as_str())
        .bind(&video.media_file)
        .bind(&video.proxy_file)
        .bind(&video.thumbnails_file)
        .bind(&video.peaks_file)
        .bind(&video.attacks_file)
        .bind(&video.preparation_error)
        .bind(video.created_at)
        .bind(video.updated_at)
}

#[derive(FromRow)]
struct VideoRow {
    id: String,
    source: String,
    source_url: Option<String>,
    title: String,
    channel: Option<String>,
    duration_ms: Option<i64>,
    fps: Option<f64>,
    width: Option<i64>,
    height: Option<i64>,
    status: String,
    media_file: Option<String>,
    proxy_file: Option<String>,
    thumbnails_file: Option<String>,
    peaks_file: Option<String>,
    attacks_file: Option<String>,
    preparation_error: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<VideoRow> for Video {
    type Error = DomainError;

    fn try_from(row: VideoRow) -> Result<Self> {
        Ok(Self {
            id: row.id.parse()?,
            source: row.source.parse()?,
            source_url: row.source_url,
            title: row.title,
            channel: row.channel,
            duration_ms: row.duration_ms,
            fps: row.fps,
            width: dimension(row.width)?,
            height: dimension(row.height)?,
            status: row.status.parse()?,
            media_file: row.media_file,
            proxy_file: row.proxy_file,
            thumbnails_file: row.thumbnails_file,
            peaks_file: row.peaks_file,
            attacks_file: row.attacks_file,
            preparation_error: row.preparation_error,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

fn dimension(value: Option<i64>) -> Result<Option<u32>> {
    value
        .map(|v| u32::try_from(v).map_err(DomainError::storage))
        .transpose()
}

#[async_trait]
impl VideoRepository for SqliteStore {
    async fn list(&self) -> Result<Vec<Video>> {
        let rows: Vec<VideoRow> = sqlx::query_as(concat!(
            "SELECT ",
            video_columns!(),
            " FROM videos WHERE deleted_at IS NULL ORDER BY created_at DESC, id DESC"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(DomainError::storage)?;
        rows.into_iter().map(Video::try_from).collect()
    }

    async fn get(&self, id: VideoId) -> Result<Video> {
        let row: Option<VideoRow> = sqlx::query_as(concat!(
            "SELECT ",
            video_columns!(),
            " FROM videos WHERE id = ? AND deleted_at IS NULL"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(DomainError::storage)?;
        row.ok_or_else(|| DomainError::NotFound {
            entity: "video",
            id: id.to_string(),
        })?
        .try_into()
    }

    async fn insert(&self, video: &Video) -> Result<()> {
        let query = sqlx::query(concat!(
            "INSERT INTO videos (",
            video_columns!(),
            ") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(video.id.to_string());
        bind_fields(query, video)
            .execute(&self.pool)
            .await
            .map_err(DomainError::storage)?;
        Ok(())
    }

    async fn update(&self, video: &Video) -> Result<()> {
        let query = sqlx::query(
            "UPDATE videos SET source = ?, source_url = ?, title = ?, channel = ?, \
             duration_ms = ?, fps = ?, width = ?, height = ?, status = ?, media_file = ?, \
             proxy_file = ?, thumbnails_file = ?, peaks_file = ?, attacks_file = ?, \
             preparation_error = ?, created_at = ?, updated_at = ? \
             WHERE id = ? AND deleted_at IS NULL",
        );
        let result = bind_fields(query, video)
            .bind(video.id.to_string())
            .execute(&self.pool)
            .await
            .map_err(DomainError::storage)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "video",
                id: video.id.to_string(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use segno_domain::{VideoSource, VideoStatus};

    use super::*;

    fn sample(title: &str) -> Video {
        let mut video = Video::new_importing(
            VideoSource::Youtube,
            title,
            Some("https://youtu.be/x".into()),
        )
        .unwrap();
        video.fps = Some(59.94);
        video.width = Some(1920);
        video.height = Some(1080);
        video.media_file = Some(format!("media/{}/original.mp4", video.id));
        video
    }

    #[tokio::test]
    async fn migrations_are_applied() {
        let store = SqliteStore::in_memory().await.unwrap();
        assert!(store.schema_version().await.unwrap() > 0);
    }

    #[tokio::test]
    async fn inserted_videos_are_listed_newest_first() {
        let store = SqliteStore::in_memory().await.unwrap();
        let first = sample("First");
        let second = sample("Second");
        store.insert(&first).await.unwrap();
        store.insert(&second).await.unwrap();

        let titles: Vec<_> = store
            .list()
            .await
            .unwrap()
            .into_iter()
            .map(|v| v.title)
            .collect();
        assert_eq!(titles, ["Second", "First"]);
    }

    #[tokio::test]
    async fn a_video_survives_the_round_trip() {
        let store = SqliteStore::in_memory().await.unwrap();
        let video = sample("Round trip");
        store.insert(&video).await.unwrap();

        let loaded = store.get(video.id).await.unwrap();
        assert_eq!(loaded, video);
        assert_eq!(loaded.status, VideoStatus::Importing);
    }

    #[tokio::test]
    async fn unknown_videos_are_not_found() {
        let store = SqliteStore::in_memory().await.unwrap();
        let err = store.get(VideoId::new()).await.unwrap_err();
        assert!(matches!(err, DomainError::NotFound { .. }));
    }

    #[tokio::test]
    async fn updates_save_every_field() {
        let store = SqliteStore::in_memory().await.unwrap();
        let mut video = sample("Prepared");
        store.insert(&video).await.unwrap();

        video.proxy_file = Some(format!("media/{}/proxy.mp4", video.id));
        video.thumbnails_file = Some(format!("media/{}/thumbnails.jpg", video.id));
        video.peaks_file = Some(format!("media/{}/peaks.json", video.id));
        video.attacks_file = Some(format!("media/{}/attacks.json", video.id));
        video.preparation_error = Some("no encoder".into());
        video.status = VideoStatus::Ready;
        store.update(&video).await.unwrap();

        assert_eq!(store.get(video.id).await.unwrap(), video);
    }

    #[tokio::test]
    async fn updating_an_unknown_video_is_not_found() {
        let store = SqliteStore::in_memory().await.unwrap();
        let err = store.update(&sample("Ghost")).await.unwrap_err();
        assert!(matches!(err, DomainError::NotFound { .. }));
    }

    #[tokio::test]
    async fn every_library_has_its_own_lasting_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let id = SqliteStore::open(&path)
            .await
            .unwrap()
            .library_id()
            .await
            .unwrap();
        assert_eq!(id.len(), 32);

        let reopened = SqliteStore::open(&path).await.unwrap();
        assert_eq!(reopened.library_id().await.unwrap(), id);

        let other = SqliteStore::in_memory().await.unwrap();
        assert_ne!(other.library_id().await.unwrap(), id);
    }

    #[tokio::test]
    async fn the_database_file_is_created_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let store = SqliteStore::open(&path).await.unwrap();
        store.insert(&sample("On disk")).await.unwrap();
        assert!(path.exists());
    }
}
