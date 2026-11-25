//! End-to-end tests of the router: authentication, Host checks, the video routes, importing
//! and serving media.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::{Body, Bytes, to_bytes};
use axum::http::header::{AUTHORIZATION, CONTENT_RANGE, CONTENT_TYPE, HOST, RANGE};
use axum::http::{HeaderMap, Request, StatusCode};
use segno_api::{AppState, SecurityConfig, SystemInfo};
use segno_domain::{MediaInfo, MediaProbe, Video, VideoRepository, VideoSource};
use segno_storage::{LibraryFiles, SqliteStore};
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;

const PORT: u16 = 7700;
const TOKEN: &str = "test-token";

/// Answers like ffprobe would for a 1080p60 video, whatever the file.
struct FakeProbe;

#[async_trait]
impl MediaProbe for FakeProbe {
    async fn probe(&self, _: &Path) -> segno_domain::Result<MediaInfo> {
        Ok(MediaInfo {
            duration_ms: 141_154,
            fps: Some(60.0),
            width: Some(1920),
            height: Some(1080),
            video_codec: Some("h264".into()),
            audio_codec: Some("aac".into()),
        })
    }
}

struct Harness {
    app: Router,
    store: SqliteStore,
    library: TempDir,
}

async fn harness_with(probe: Option<Arc<dyn MediaProbe>>) -> Harness {
    let store = SqliteStore::in_memory().await.unwrap();
    let library = tempfile::tempdir().unwrap();
    let state = AppState {
        system: Arc::new(SystemInfo {
            version: "test".into(),
            library_id: store.library_id().await.unwrap(),
            library_path: None,
            schema_version: store.schema_version().await.unwrap(),
            ffmpeg_version: None,
        }),
        videos: Arc::new(store.clone()),
        media_files: Arc::new(LibraryFiles::new(library.path())),
        probe,
    };
    let app = segno_api::router(
        state,
        SecurityConfig::local(TOKEN, PORT, &["http://tauri.localhost"]),
    );
    Harness {
        app,
        store,
        library,
    }
}

async fn harness() -> Harness {
    harness_with(Some(Arc::new(FakeProbe))).await
}

fn get(uri: &str) -> axum::http::request::Builder {
    Request::get(uri).header(HOST, format!("127.0.0.1:{PORT}"))
}

fn authorized(builder: axum::http::request::Builder) -> axum::http::request::Builder {
    builder.header(AUTHORIZATION, format!("Bearer {TOKEN}"))
}

fn import(path: &Path) -> Request<Body> {
    authorized(Request::post("/videos/import").header(HOST, format!("127.0.0.1:{PORT}")))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({ "path": path.to_str().unwrap() }).to_string(),
        ))
        .unwrap()
}

async fn raw(app: &Router, request: Request<Body>) -> (StatusCode, HeaderMap, Bytes) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, headers, bytes)
}

async fn send(app: &Router, request: Request<Body>) -> (StatusCode, Option<String>, Value) {
    let (status, headers, bytes) = raw(app, request).await;
    let content_type = headers
        .get(CONTENT_TYPE)
        .map(|value| value.to_str().unwrap().to_owned());
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, content_type, body)
}

fn empty(builder: axum::http::request::Builder) -> Request<Body> {
    builder.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn health_needs_no_token() {
    let h = harness().await;
    let (status, _, body) = send(&h.app, empty(get("/health"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn protected_routes_reject_missing_tokens_with_a_problem() {
    let h = harness().await;
    let (status, content_type, body) = send(&h.app, empty(get("/system"))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(content_type.as_deref(), Some("application/problem+json"));
    assert_eq!(body["status"], 401);
}

#[tokio::test]
async fn the_bearer_token_opens_protected_routes() {
    let h = harness().await;
    let (status, _, body) = send(&h.app, empty(authorized(get("/system")))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["version"], "test");
    assert_eq!(body["library_id"].as_str().unwrap().len(), 32);
    assert!(body["schema_version"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn the_token_can_travel_in_the_query_for_media_elements() {
    let h = harness().await;
    let (status, _, _) = send(&h.app, empty(get(&format!("/system?token={TOKEN}")))).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn unexpected_hosts_are_rejected() {
    let h = harness().await;
    let request = empty(Request::get("/health").header(HOST, "evil.example"));
    let (status, _, _) = send(&h.app, request).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn videos_are_listed() {
    let h = harness().await;
    let video = Video::new_importing(VideoSource::File, "Clair de lune", None).unwrap();
    h.store.insert(&video).await.unwrap();

    let (status, _, body) = send(&h.app, empty(authorized(get("/videos")))).await;
    assert_eq!(status, StatusCode::OK);
    let videos = body.as_array().unwrap();
    assert_eq!(videos.len(), 1);
    assert_eq!(videos[0]["title"], "Clair de lune");
    assert_eq!(videos[0]["source"], "file");
    assert_eq!(videos[0]["status"], "importing");
    assert_eq!(videos[0]["id"], video.id.to_string());
}

#[tokio::test]
async fn unknown_videos_are_404_problems() {
    let h = harness().await;
    let uri = format!("/videos/{}", uuid::Uuid::now_v7());
    let (status, content_type, _) = send(&h.app, empty(authorized(get(&uri)))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(content_type.as_deref(), Some("application/problem+json"));
}

#[tokio::test]
async fn imported_videos_can_be_played_with_range_requests() {
    let h = harness().await;
    let file = h.library.path().join("Monk - I'm Confessin'.mp4");
    std::fs::write(&file, b"0123456789").unwrap();

    let (status, _, video) = send(&h.app, import(&file)).await;
    assert_eq!(status, StatusCode::CREATED, "{video}");
    assert_eq!(video["title"], "Monk - I'm Confessin'");
    assert_eq!(video["status"], "ready");
    assert_eq!(video["fps"], 60.0);
    assert_eq!(video["duration_ms"], 141_154);

    let media = format!(
        "/videos/{}/media?token={TOKEN}",
        video["id"].as_str().unwrap()
    );
    let (status, headers, bytes) = raw(&h.app, empty(get(&media))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[CONTENT_TYPE], "video/mp4");
    assert_eq!(&bytes[..], b"0123456789");

    let (status, headers, bytes) = raw(&h.app, empty(get(&media).header(RANGE, "bytes=2-5"))).await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(headers[CONTENT_RANGE], "bytes 2-5/10");
    assert_eq!(&bytes[..], b"2345");
}

#[tokio::test]
async fn media_needs_the_token() {
    let h = harness().await;
    let file = h.library.path().join("clip.mp4");
    std::fs::write(&file, b"x").unwrap();
    let (_, _, video) = send(&h.app, import(&file)).await;
    let media = format!("/videos/{}/media", video["id"].as_str().unwrap());
    let (status, _, _) = send(&h.app, empty(get(&media))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn importing_a_missing_file_is_a_400_problem() {
    let h = harness().await;
    let (status, content_type, body) = send(&h.app, import(Path::new("C:/nope/missing.mp4"))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(content_type.as_deref(), Some("application/problem+json"));
    assert!(body["detail"].as_str().unwrap().contains("not a file"));
}

#[tokio::test]
async fn importing_without_ffmpeg_is_a_503_problem() {
    let h = harness_with(None).await;
    let file = h.library.path().join("clip.mp4");
    std::fs::write(&file, b"x").unwrap();
    let (status, _, body) = send(&h.app, import(&file)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body["detail"].as_str().unwrap().contains("ffmpeg"));
}

#[tokio::test]
async fn videos_without_media_have_no_media_route() {
    let h = harness().await;
    let video = Video::new_importing(VideoSource::File, "Downloading", None).unwrap();
    h.store.insert(&video).await.unwrap();
    let uri = format!("/videos/{}/media?token={TOKEN}", video.id);
    let (status, _, _) = send(&h.app, empty(get(&uri))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[test]
fn the_openapi_document_describes_every_route() {
    let doc = serde_json::to_value(segno_api::openapi()).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    for path in [
        "/health",
        "/system",
        "/videos",
        "/videos/{id}",
        "/videos/import",
        "/videos/{id}/media",
    ] {
        assert!(paths.contains_key(path), "missing {path}");
    }
    assert!(doc["components"]["securitySchemes"]["token"].is_object());
}
