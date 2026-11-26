use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use segno_domain::{
    Comment, CommentId, CommentRepository, DomainError, Marker, MarkerId, MarkerRepository, Result,
    Studied, Video, VideoId, VideoRepository, VideoState, VideoStateRepository,
};
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

    /// Whether a migration left `job` for the core to do once (a `pending:<job>` setting).
    pub async fn pending(&self, job: &str) -> Result<bool> {
        let found: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(format!("pending:{job}"))
            .fetch_optional(&self.pool)
            .await
            .map_err(DomainError::storage)?;
        Ok(found.is_some())
    }

    /// Marks a job a migration left as done.
    pub async fn done(&self, job: &str) -> Result<()> {
        sqlx::query("DELETE FROM settings WHERE key = ?")
            .bind(format!("pending:{job}"))
            .execute(&self.pool)
            .await
            .map_err(DomainError::storage)?;
        Ok(())
    }
}

macro_rules! video_columns {
    () => {
        "id, source, source_url, title, artist, year, channel, duration_ms, fps, width, height, status, media_file, proxy_file, thumbnails_file, peaks_file, attacks_file, preparation_error, import_error, created_at, updated_at"
    };
}

type SqliteQuery<'q> = sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>;

/// Binds every column after `id`, in the order of `video_columns!`.
fn bind_fields<'q>(query: SqliteQuery<'q>, video: &'q Video) -> SqliteQuery<'q> {
    query
        .bind(video.source.as_str())
        .bind(&video.source_url)
        .bind(&video.title)
        .bind(&video.artist)
        .bind(video.year)
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
        .bind(&video.import_error)
        .bind(video.created_at)
        .bind(video.updated_at)
}

#[derive(FromRow)]
struct VideoRow {
    id: String,
    source: String,
    source_url: Option<String>,
    title: String,
    artist: Option<String>,
    year: Option<i32>,
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
    import_error: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl VideoRow {
    fn into_video(self, tags: Vec<String>) -> Result<Video> {
        Ok(Video {
            id: self.id.parse()?,
            source: self.source.parse()?,
            source_url: self.source_url,
            title: self.title,
            artist: self.artist,
            year: self.year,
            channel: self.channel,
            duration_ms: self.duration_ms,
            fps: self.fps,
            width: dimension(self.width)?,
            height: dimension(self.height)?,
            status: self.status.parse()?,
            media_file: self.media_file,
            proxy_file: self.proxy_file,
            thumbnails_file: self.thumbnails_file,
            peaks_file: self.peaks_file,
            attacks_file: self.attacks_file,
            preparation_error: self.preparation_error,
            import_error: self.import_error,
            tags,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

fn dimension(value: Option<i64>) -> Result<Option<u32>> {
    value
        .map(|v| u32::try_from(v).map_err(DomainError::storage))
        .transpose()
}

fn video_not_found(id: VideoId) -> DomainError {
    DomainError::NotFound {
        entity: "video",
        id: id.to_string(),
    }
}

impl SqliteStore {
    /// Tags of every video (or of one), each video's in their order.
    async fn tags(&self, video: Option<VideoId>) -> Result<HashMap<String, Vec<String>>> {
        let rows: Vec<(String, String)> = match video {
            Some(id) => {
                sqlx::query_as(
                    "SELECT video_id, tag FROM video_tags WHERE video_id = ? ORDER BY position",
                )
                .bind(id.to_string())
                .fetch_all(&self.pool)
                .await
            }
            None => {
                sqlx::query_as("SELECT video_id, tag FROM video_tags ORDER BY video_id, position")
                    .fetch_all(&self.pool)
                    .await
            }
        }
        .map_err(DomainError::storage)?;
        let mut tags: HashMap<String, Vec<String>> = HashMap::new();
        for (video, tag) in rows {
            tags.entry(video).or_default().push(tag);
        }
        Ok(tags)
    }
}

/// Replaces a video's tags inside `transaction`.
async fn replace_tags(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    video: &Video,
) -> Result<()> {
    sqlx::query("DELETE FROM video_tags WHERE video_id = ?")
        .bind(video.id.to_string())
        .execute(&mut **transaction)
        .await
        .map_err(DomainError::storage)?;
    for (position, tag) in video.tags.iter().enumerate() {
        sqlx::query("INSERT INTO video_tags (video_id, tag, position) VALUES (?, ?, ?)")
            .bind(video.id.to_string())
            .bind(tag)
            .bind(position as i64)
            .execute(&mut **transaction)
            .await
            .map_err(DomainError::storage)?;
    }
    Ok(())
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
        let mut tags = self.tags(None).await?;
        rows.into_iter()
            .map(|row| {
                let video_tags = tags.remove(&row.id).unwrap_or_default();
                row.into_video(video_tags)
            })
            .collect()
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
        let row = row.ok_or_else(|| video_not_found(id))?;
        let tags = self
            .tags(Some(id))
            .await?
            .remove(&row.id)
            .unwrap_or_default();
        row.into_video(tags)
    }

    async fn insert(&self, video: &Video) -> Result<()> {
        let mut transaction = self.pool.begin().await.map_err(DomainError::storage)?;
        let query = sqlx::query(concat!(
            "INSERT INTO videos (",
            video_columns!(),
            ") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(video.id.to_string());
        bind_fields(query, video)
            .execute(&mut *transaction)
            .await
            .map_err(DomainError::storage)?;
        replace_tags(&mut transaction, video).await?;
        transaction.commit().await.map_err(DomainError::storage)
    }

    async fn update(&self, video: &Video) -> Result<()> {
        let mut transaction = self.pool.begin().await.map_err(DomainError::storage)?;
        let query = sqlx::query(
            "UPDATE videos SET source = ?, source_url = ?, title = ?, artist = ?, year = ?, \
             channel = ?, duration_ms = ?, fps = ?, width = ?, height = ?, status = ?, \
             media_file = ?, proxy_file = ?, thumbnails_file = ?, peaks_file = ?, attacks_file = ?, \
             preparation_error = ?, import_error = ?, created_at = ?, updated_at = ? \
             WHERE id = ? AND deleted_at IS NULL",
        );
        let result = bind_fields(query, video)
            .bind(video.id.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(DomainError::storage)?;
        if result.rows_affected() == 0 {
            return Err(video_not_found(video.id));
        }
        replace_tags(&mut transaction, video).await?;
        transaction.commit().await.map_err(DomainError::storage)
    }

    async fn delete(&self, id: VideoId) -> Result<()> {
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE videos SET deleted_at = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(now)
        .bind(now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(DomainError::storage)?;
        if result.rows_affected() == 0 {
            return Err(video_not_found(id));
        }
        Ok(())
    }
}

#[async_trait]
impl VideoStateRepository for SqliteStore {
    async fn get_state(&self, video: VideoId) -> Result<Option<VideoState>> {
        let json: Option<String> =
            sqlx::query_scalar("SELECT state FROM video_states WHERE video_id = ?")
                .bind(video.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(DomainError::storage)?;
        json.map(|json| serde_json::from_str(&json).map_err(DomainError::storage))
            .transpose()
    }

    async fn studied(&self) -> Result<Vec<Studied>> {
        let rows: Vec<(String, Option<i64>, DateTime<Utc>)> = sqlx::query_as(
            "SELECT video_id, json_extract(state, '$.position_ms'), updated_at FROM video_states",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(DomainError::storage)?;
        rows.into_iter()
            .map(|(video, position_ms, at)| {
                Ok(Studied {
                    video: video.parse()?,
                    position_ms: position_ms.unwrap_or(0),
                    at,
                })
            })
            .collect()
    }

    async fn save_state(&self, video: VideoId, state: &VideoState) -> Result<()> {
        let json = serde_json::to_string(state).map_err(DomainError::storage)?;
        sqlx::query(
            "INSERT INTO video_states (video_id, state, updated_at) VALUES (?, ?, ?) \
             ON CONFLICT (video_id) DO UPDATE SET state = excluded.state, \
             updated_at = excluded.updated_at",
        )
        .bind(video.to_string())
        .bind(json)
        .bind(Utc::now())
        .execute(&self.pool)
        .await
        .map_err(DomainError::storage)?;
        Ok(())
    }
}

macro_rules! marker_columns {
    () => {
        "id, video_id, kind, start_ms, end_ms, label, color, created_at, updated_at"
    };
}

#[derive(FromRow)]
struct MarkerRow {
    id: String,
    video_id: String,
    kind: String,
    start_ms: i64,
    end_ms: Option<i64>,
    label: String,
    color: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<MarkerRow> for Marker {
    type Error = DomainError;

    fn try_from(row: MarkerRow) -> Result<Self> {
        Ok(Self {
            id: row.id.parse()?,
            video: row.video_id.parse()?,
            kind: row.kind.parse()?,
            start_ms: row.start_ms,
            end_ms: row.end_ms,
            label: row.label,
            color: row.color,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

fn marker_not_found(id: MarkerId) -> DomainError {
    DomainError::NotFound {
        entity: "marker",
        id: id.to_string(),
    }
}

#[async_trait]
impl MarkerRepository for SqliteStore {
    async fn list_markers(&self, video: VideoId) -> Result<Vec<Marker>> {
        let rows: Vec<MarkerRow> = sqlx::query_as(concat!(
            "SELECT ",
            marker_columns!(),
            " FROM markers WHERE video_id = ? AND deleted_at IS NULL ORDER BY start_ms, id"
        ))
        .bind(video.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(DomainError::storage)?;
        rows.into_iter().map(Marker::try_from).collect()
    }

    async fn get_marker(&self, video: VideoId, id: MarkerId) -> Result<Marker> {
        let row: Option<MarkerRow> = sqlx::query_as(concat!(
            "SELECT ",
            marker_columns!(),
            " FROM markers WHERE id = ? AND video_id = ? AND deleted_at IS NULL"
        ))
        .bind(id.to_string())
        .bind(video.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(DomainError::storage)?;
        row.ok_or_else(|| marker_not_found(id))?.try_into()
    }

    async fn insert_marker(&self, marker: &Marker) -> Result<()> {
        sqlx::query(concat!(
            "INSERT INTO markers (",
            marker_columns!(),
            ") VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(marker.id.to_string())
        .bind(marker.video.to_string())
        .bind(marker.kind.as_str())
        .bind(marker.start_ms)
        .bind(marker.end_ms)
        .bind(&marker.label)
        .bind(&marker.color)
        .bind(marker.created_at)
        .bind(marker.updated_at)
        .execute(&self.pool)
        .await
        .map_err(DomainError::storage)?;
        Ok(())
    }

    async fn update_marker(&self, marker: &Marker) -> Result<()> {
        let result = sqlx::query(
            "UPDATE markers SET start_ms = ?, end_ms = ?, label = ?, color = ?, updated_at = ? \
             WHERE id = ? AND video_id = ? AND deleted_at IS NULL",
        )
        .bind(marker.start_ms)
        .bind(marker.end_ms)
        .bind(&marker.label)
        .bind(&marker.color)
        .bind(marker.updated_at)
        .bind(marker.id.to_string())
        .bind(marker.video.to_string())
        .execute(&self.pool)
        .await
        .map_err(DomainError::storage)?;
        if result.rows_affected() == 0 {
            return Err(marker_not_found(marker.id));
        }
        Ok(())
    }

    async fn delete_marker(&self, video: VideoId, id: MarkerId) -> Result<()> {
        let result = sqlx::query(
            "UPDATE markers SET deleted_at = ?, updated_at = ? \
             WHERE id = ? AND video_id = ? AND deleted_at IS NULL",
        )
        .bind(Utc::now())
        .bind(Utc::now())
        .bind(id.to_string())
        .bind(video.to_string())
        .execute(&self.pool)
        .await
        .map_err(DomainError::storage)?;
        if result.rows_affected() == 0 {
            return Err(marker_not_found(id));
        }
        Ok(())
    }
}

macro_rules! comment_columns {
    () => {
        "id, video_id, start_ms, end_ms, body, created_at, updated_at"
    };
}

#[derive(FromRow)]
struct CommentRow {
    id: String,
    video_id: String,
    start_ms: i64,
    end_ms: Option<i64>,
    body: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<CommentRow> for Comment {
    type Error = DomainError;

    fn try_from(row: CommentRow) -> Result<Self> {
        Ok(Self {
            id: row.id.parse()?,
            video: row.video_id.parse()?,
            start_ms: row.start_ms,
            end_ms: row.end_ms,
            body: row.body,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

fn comment_not_found(id: CommentId) -> DomainError {
    DomainError::NotFound {
        entity: "comment",
        id: id.to_string(),
    }
}

#[async_trait]
impl CommentRepository for SqliteStore {
    async fn list_comments(&self, video: VideoId) -> Result<Vec<Comment>> {
        let rows: Vec<CommentRow> = sqlx::query_as(concat!(
            "SELECT ",
            comment_columns!(),
            " FROM comments WHERE video_id = ? AND deleted_at IS NULL ORDER BY start_ms, id"
        ))
        .bind(video.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(DomainError::storage)?;
        rows.into_iter().map(Comment::try_from).collect()
    }

    async fn get_comment(&self, video: VideoId, id: CommentId) -> Result<Comment> {
        let row: Option<CommentRow> = sqlx::query_as(concat!(
            "SELECT ",
            comment_columns!(),
            " FROM comments WHERE id = ? AND video_id = ? AND deleted_at IS NULL"
        ))
        .bind(id.to_string())
        .bind(video.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(DomainError::storage)?;
        row.ok_or_else(|| comment_not_found(id))?.try_into()
    }

    async fn insert_comment(&self, comment: &Comment) -> Result<()> {
        sqlx::query(concat!(
            "INSERT INTO comments (",
            comment_columns!(),
            ") VALUES (?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(comment.id.to_string())
        .bind(comment.video.to_string())
        .bind(comment.start_ms)
        .bind(comment.end_ms)
        .bind(&comment.body)
        .bind(comment.created_at)
        .bind(comment.updated_at)
        .execute(&self.pool)
        .await
        .map_err(DomainError::storage)?;
        Ok(())
    }

    async fn update_comment(&self, comment: &Comment) -> Result<()> {
        let result = sqlx::query(
            "UPDATE comments SET start_ms = ?, end_ms = ?, body = ?, updated_at = ? \
             WHERE id = ? AND video_id = ? AND deleted_at IS NULL",
        )
        .bind(comment.start_ms)
        .bind(comment.end_ms)
        .bind(&comment.body)
        .bind(comment.updated_at)
        .bind(comment.id.to_string())
        .bind(comment.video.to_string())
        .execute(&self.pool)
        .await
        .map_err(DomainError::storage)?;
        if result.rows_affected() == 0 {
            return Err(comment_not_found(comment.id));
        }
        Ok(())
    }

    async fn delete_comment(&self, video: VideoId, id: CommentId) -> Result<()> {
        let now = Utc::now();
        let result = sqlx::query(
            "UPDATE comments SET deleted_at = ?, updated_at = ? \
             WHERE id = ? AND video_id = ? AND deleted_at IS NULL",
        )
        .bind(now)
        .bind(now)
        .bind(id.to_string())
        .bind(video.to_string())
        .execute(&self.pool)
        .await
        .map_err(DomainError::storage)?;
        if result.rows_affected() == 0 {
            return Err(comment_not_found(id));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use segno_domain::{
        BeatGrid, LoopRange, MarkerFields, MarkerKind, StepUnit, VideoSource, VideoStatus,
    };

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
    async fn tags_and_import_errors_are_saved_with_the_video() {
        let store = SqliteStore::in_memory().await.unwrap();
        let mut video = sample("Tagged");
        video.tags = vec!["Jazz".into(), "To learn".into()];
        store.insert(&video).await.unwrap();
        assert_eq!(
            store.get(video.id).await.unwrap().tags,
            ["Jazz", "To learn"]
        );

        video.tags = vec!["Ballad".into()];
        video.import_error = Some("Video unavailable".into());
        video.artist = Some("Thelonious Monk".into());
        video.year = Some(1965);
        store.update(&video).await.unwrap();
        let loaded = store.list().await.unwrap();
        assert_eq!(loaded, vec![video]);
    }

    #[tokio::test]
    async fn migrations_leave_one_off_jobs_until_they_are_done() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.db");
        let store = SqliteStore::open(&path).await.unwrap();
        assert!(store.pending("guess_songs").await.unwrap());
        assert!(!store.pending("unknown").await.unwrap());
        store.done("guess_songs").await.unwrap();
        let reopened = SqliteStore::open(&path).await.unwrap();
        assert!(!reopened.pending("guess_songs").await.unwrap());
    }

    #[tokio::test]
    async fn deleted_videos_leave_the_library() {
        let store = SqliteStore::in_memory().await.unwrap();
        let (kept, gone) = (sample("Kept"), sample("Gone"));
        store.insert(&kept).await.unwrap();
        store.insert(&gone).await.unwrap();
        store.delete(gone.id).await.unwrap();
        assert_eq!(store.list().await.unwrap(), vec![kept]);
        assert!(store.get(gone.id).await.is_err());
        assert!(matches!(
            store.delete(gone.id).await,
            Err(DomainError::NotFound { .. })
        ));
    }

    #[tokio::test]
    async fn studied_videos_tell_where_they_were_left() {
        let store = SqliteStore::in_memory().await.unwrap();
        let video = sample("Studied");
        store.insert(&video).await.unwrap();
        assert!(store.studied().await.unwrap().is_empty());
        let state = VideoState {
            position_ms: 61_500,
            ..VideoState::default()
        };
        store.save_state(video.id, &state).await.unwrap();
        let studied = store.studied().await.unwrap();
        assert_eq!(studied.len(), 1);
        assert_eq!(
            (studied[0].video, studied[0].position_ms),
            (video.id, 61_500)
        );
    }

    #[tokio::test]
    async fn video_states_are_saved_and_replaced() {
        let store = SqliteStore::in_memory().await.unwrap();
        let video = sample("Stateful");
        store.insert(&video).await.unwrap();
        assert_eq!(store.get_state(video.id).await.unwrap(), None);

        let mut state = VideoState {
            position_ms: 42_000,
            speed: 0.8,
            step_unit: StepUnit::Beat,
            beat_grid: Some(BeatGrid {
                beats_ms: vec![500, 1100, 1700],
                beats_per_bar: 3,
                downbeat: 0,
            }),
            loop_range: Some(LoopRange {
                start_ms: 1000,
                end_ms: 9000,
                enabled: true,
            }),
            ..VideoState::default()
        };
        store.save_state(video.id, &state).await.unwrap();
        assert_eq!(
            store.get_state(video.id).await.unwrap().as_ref(),
            Some(&state)
        );

        state.position_ms = 1;
        store.save_state(video.id, &state).await.unwrap();
        assert_eq!(
            store
                .get_state(video.id)
                .await
                .unwrap()
                .unwrap()
                .position_ms,
            1
        );
    }

    #[tokio::test]
    async fn markers_are_listed_in_time_order_until_deleted() {
        let store = SqliteStore::in_memory().await.unwrap();
        let video = sample("Marked");
        store.insert(&video).await.unwrap();
        let marker = |kind, start_ms, end_ms: Option<i64>| {
            Marker::new(
                video.id,
                kind,
                MarkerFields {
                    start_ms,
                    end_ms,
                    label: format!("at {start_ms}"),
                    color: None,
                },
            )
            .unwrap()
        };
        let late = marker(MarkerKind::Point, 9000, None);
        let mut section = marker(MarkerKind::Section, 1000, Some(5000));
        for item in [&late, &section] {
            store.insert_marker(item).await.unwrap();
        }

        let starts: Vec<i64> = store
            .list_markers(video.id)
            .await
            .unwrap()
            .iter()
            .map(|m| m.start_ms)
            .collect();
        assert_eq!(starts, [1000, 9000]);

        section
            .change(MarkerFields {
                start_ms: 2000,
                end_ms: Some(6000),
                label: "Chorus".into(),
                color: Some("#ff0000".into()),
            })
            .unwrap();
        store.update_marker(&section).await.unwrap();
        assert_eq!(
            store.get_marker(video.id, section.id).await.unwrap(),
            section
        );

        store.delete_marker(video.id, late.id).await.unwrap();
        assert_eq!(store.list_markers(video.id).await.unwrap(), vec![section]);
        assert!(matches!(
            store.delete_marker(video.id, late.id).await,
            Err(DomainError::NotFound { .. })
        ));
    }

    #[tokio::test]
    async fn comments_are_listed_in_time_order_until_deleted() {
        let store = SqliteStore::in_memory().await.unwrap();
        let video = sample("Commented");
        store.insert(&video).await.unwrap();
        let comment = |start_ms, end_ms, body: &str| {
            Comment::new(
                video.id,
                segno_domain::CommentFields {
                    start_ms,
                    end_ms,
                    body: body.into(),
                },
            )
            .unwrap()
        };
        let late = comment(50_000, None, "Fingering: 1-3-5");
        let mut range = comment(10_000, Some(20_000), "Pedal through the bridge");
        store.insert_comment(&late).await.unwrap();
        store.insert_comment(&range).await.unwrap();

        let bodies: Vec<String> = store
            .list_comments(video.id)
            .await
            .unwrap()
            .into_iter()
            .map(|c| c.body)
            .collect();
        assert_eq!(bodies, ["Pedal through the bridge", "Fingering: 1-3-5"]);

        range
            .change(segno_domain::CommentFields {
                start_ms: 11_000,
                end_ms: None,
                body: "**Pedal** here".into(),
            })
            .unwrap();
        store.update_comment(&range).await.unwrap();
        assert_eq!(store.get_comment(video.id, range.id).await.unwrap(), range);

        store.delete_comment(video.id, late.id).await.unwrap();
        assert_eq!(store.list_comments(video.id).await.unwrap(), vec![range]);
        assert!(store.get_comment(video.id, late.id).await.is_err());
    }

    #[tokio::test]
    async fn markers_belong_to_their_video() {
        let store = SqliteStore::in_memory().await.unwrap();
        let (mine, other) = (sample("Mine"), sample("Other"));
        store.insert(&mine).await.unwrap();
        store.insert(&other).await.unwrap();
        let marker = Marker::new(
            mine.id,
            MarkerKind::Point,
            MarkerFields {
                start_ms: 0,
                end_ms: None,
                label: String::new(),
                color: None,
            },
        )
        .unwrap();
        store.insert_marker(&marker).await.unwrap();
        assert!(store.get_marker(other.id, marker.id).await.is_err());
        assert!(store.delete_marker(other.id, marker.id).await.is_err());
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
