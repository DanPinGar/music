//! End-to-end tests of the router: authentication, Host checks, the video routes, importing
//! and serving media.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::{Body, Bytes, to_bytes};
use axum::http::header::{AUTHORIZATION, CONTENT_RANGE, CONTENT_TYPE, HOST, RANGE};
use axum::http::{HeaderMap, Request, StatusCode};
use segno_api::{AppState, Preparations, SecurityConfig, SystemInfo};
use segno_domain::{
    MarkerRepository, MediaInfo, MediaPreparer, MediaProbe, MediaStore, PreparationInput,
    ThumbnailSprite, Video, VideoRepository, VideoSource, VideoStateRepository,
};
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

/// Writes tiny stand-ins for the derived files, so the routes have something to serve.
struct FakePreparer;

#[async_trait]
impl MediaPreparer for FakePreparer {
    async fn make_seek_proxy(
        &self,
        _: &Path,
        target: &Path,
        _: PreparationInput,
        progress: &(dyn Fn(f64) + Send + Sync),
    ) -> segno_domain::Result<()> {
        progress(0.5);
        std::fs::write(target, b"PROXY").unwrap();
        Ok(())
    }
    async fn make_thumbnails(
        &self,
        _: &Path,
        target: &Path,
        _: ThumbnailSprite,
    ) -> segno_domain::Result<()> {
        std::fs::write(target, b"JPEG").unwrap();
        Ok(())
    }
    async fn make_peaks(&self, _: &Path, target: &Path) -> segno_domain::Result<()> {
        std::fs::write(
            target,
            br#"{"version":1,"peaks_per_second":100,"peaks":[]}"#,
        )
        .unwrap();
        Ok(())
    }
    async fn make_attacks(&self, _: &Path, target: &Path) -> segno_domain::Result<()> {
        std::fs::write(
            target,
            br#"{"version":1,"times":[0.5,1.25],"strengths":[1.0,0.4]}"#,
        )
        .unwrap();
        Ok(())
    }
}

async fn harness_with(probe: Option<Arc<dyn MediaProbe>>, prepare: bool) -> Harness {
    let store = SqliteStore::in_memory().await.unwrap();
    let library = tempfile::tempdir().unwrap();
    let videos: Arc<dyn VideoRepository> = Arc::new(store.clone());
    let media_files: Arc<dyn MediaStore> = Arc::new(LibraryFiles::new(library.path()));
    let preparations = prepare
        .then(|| Preparations::start(videos.clone(), media_files.clone(), Arc::new(FakePreparer)));
    let state = AppState {
        system: Arc::new(SystemInfo {
            version: "test".into(),
            library_id: store.library_id().await.unwrap(),
            library_path: None,
            schema_version: store.schema_version().await.unwrap(),
            ffmpeg_version: None,
        }),
        videos,
        states: Arc::new(store.clone()) as Arc<dyn VideoStateRepository>,
        markers: Arc::new(store.clone()) as Arc<dyn MarkerRepository>,
        media_files,
        probe,
        preparations,
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

/// Imports work; nothing is prepared, so `/media` keeps serving the original.
async fn harness() -> Harness {
    harness_with(Some(Arc::new(FakeProbe)), false).await
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
    let h = harness_with(None, false).await;
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

#[tokio::test]
async fn imported_videos_are_prepared_in_the_background() {
    let h = harness_with(Some(Arc::new(FakeProbe)), true).await;
    let file = h.library.path().join("clip.mp4");
    std::fs::write(&file, b"ORIGINAL").unwrap();

    let (status, _, video) = send(&h.app, import(&file)).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = video["id"].as_str().unwrap().to_owned();

    let mut prepared = Value::Null;
    for _ in 0..100 {
        let (_, _, current) = send(&h.app, empty(authorized(get(&format!("/videos/{id}"))))).await;
        if current["seek_proxy"] == true && current["preparation"].is_null() {
            prepared = current;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(prepared["has_peaks"], true, "{prepared}");
    assert_eq!(prepared["has_attacks"], true);
    assert_eq!(prepared["thumbnails"]["count"], 142);
    assert_eq!(prepared["thumbnails"]["tile_height"], 90);
    assert!(prepared["preparation_error"].is_null());

    let (status, _, bytes) = raw(
        &h.app,
        empty(get(&format!("/videos/{id}/media?token={TOKEN}"))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(&bytes[..], b"PROXY", "the player gets the seek proxy");
    let (status, headers, _) = raw(
        &h.app,
        empty(get(&format!("/videos/{id}/thumbnails?token={TOKEN}"))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[CONTENT_TYPE], "image/jpeg");
    let (status, _, peaks) = send(
        &h.app,
        empty(authorized(get(&format!("/videos/{id}/peaks")))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(peaks["peaks_per_second"], 100);
    let (status, _, attacks) = send(
        &h.app,
        empty(authorized(get(&format!("/videos/{id}/attacks")))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(attacks["times"], json!([0.5, 1.25]));
}

#[tokio::test]
async fn unprepared_videos_have_no_navigation_files_yet() {
    let h = harness().await;
    let file = h.library.path().join("clip.mp4");
    std::fs::write(&file, b"x").unwrap();
    let (_, _, video) = send(&h.app, import(&file)).await;
    assert_eq!(video["seek_proxy"], false);
    assert_eq!(video["has_attacks"], false);
    let id = video["id"].as_str().unwrap();
    for route in ["thumbnails", "peaks", "attacks"] {
        let uri = format!("/videos/{id}/{route}?token={TOKEN}");
        let (status, _, _) = send(&h.app, empty(get(&uri))).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{route}");
    }
}

fn json_request(method: &str, uri: &str, body: Value) -> Request<Body> {
    authorized(
        Request::builder()
            .method(method)
            .uri(uri)
            .header(HOST, format!("127.0.0.1:{PORT}")),
    )
    .header(CONTENT_TYPE, "application/json")
    .body(Body::from(body.to_string()))
    .unwrap()
}

async fn stored_video(h: &Harness) -> Video {
    let video = Video::new_importing(VideoSource::File, "Monk", None).unwrap();
    h.store.insert(&video).await.unwrap();
    video
}

#[tokio::test]
async fn a_video_never_opened_has_the_default_state() {
    let h = harness().await;
    let video = stored_video(&h).await;
    let (status, _, body) = send(
        &h.app,
        empty(authorized(get(&format!("/videos/{}/state", video.id)))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["position_ms"], 0);
    assert_eq!(body["speed"], 1.0);
    assert_eq!(body["step_unit"], "seconds");
}

#[tokio::test]
async fn the_state_is_saved_whole_and_validated() {
    let h = harness().await;
    let video = stored_video(&h).await;
    let uri = format!("/videos/{}/state", video.id);
    let state = json!({
        "position_ms": 30_200,
        "speed": 0.75,
        "step_unit": "bar",
        "view": { "start_ms": 28_000, "span_ms": 10_000 },
        "beat_grid": { "beats_ms": [500, 1100, 1700], "beats_per_bar": 3, "downbeat": 0 },
        "loop": { "start_ms": 1000, "end_ms": 9000, "enabled": true },
        "practice": { "pre_roll_beats": 2, "count_in": true,
                      "trainer": { "from": 0.6, "step": 0.05, "to": 1.0 } }
    });
    let (status, _, saved) = send(&h.app, json_request("PUT", &uri, state.clone())).await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    let (_, _, loaded) = send(&h.app, empty(authorized(get(&uri)))).await;
    assert_eq!(loaded, state);

    let mut bad = state;
    bad["loop"]["end_ms"] = json!(500);
    let (status, content_type, problem) = send(&h.app, json_request("PUT", &uri, bad)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(content_type.as_deref(), Some("application/problem+json"));
    assert!(problem["detail"].as_str().unwrap().contains("loop"));
}

#[tokio::test]
async fn state_of_an_unknown_video_is_404() {
    let h = harness().await;
    let uri = format!("/videos/{}/state", uuid::Uuid::now_v7());
    let (status, _, _) = send(&h.app, empty(authorized(get(&uri)))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn markers_are_created_listed_changed_and_deleted() {
    let h = harness().await;
    let video = stored_video(&h).await;
    let markers = format!("/videos/{}/markers", video.id);

    let (status, _, chorus) = send(
        &h.app,
        json_request(
            "POST",
            &markers,
            json!({ "kind": "section", "start_ms": 20_000, "end_ms": 45_000, "label": "Chorus" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{chorus}");
    let (status, _, point) = send(
        &h.app,
        json_request(
            "POST",
            &markers,
            json!({ "kind": "point", "start_ms": 5_000 }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(point["label"], "");

    let (_, _, listed) = send(&h.app, empty(authorized(get(&markers)))).await;
    let starts: Vec<i64> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["start_ms"].as_i64().unwrap())
        .collect();
    assert_eq!(starts, [5_000, 20_000]);

    let one = format!("{markers}/{}", chorus["id"].as_str().unwrap());
    let (status, _, changed) = send(
        &h.app,
        json_request(
            "PUT",
            &one,
            json!({ "start_ms": 21_000, "end_ms": 46_000, "label": "Coro", "color": "#22AA88" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["label"], "Coro");
    assert_eq!(changed["color"], "#22aa88");
    assert_eq!(changed["kind"], "section");

    let delete = authorized(Request::delete(&one).header(HOST, format!("127.0.0.1:{PORT}")));
    let (status, _, _) = raw(&h.app, empty(delete)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, _, listed) = send(&h.app, empty(authorized(get(&markers)))).await;
    assert_eq!(listed.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_markers_are_400_problems() {
    let h = harness().await;
    let video = stored_video(&h).await;
    let markers = format!("/videos/{}/markers", video.id);
    for body in [
        json!({ "kind": "point", "start_ms": 5_000, "end_ms": 6_000 }),
        json!({ "kind": "loop", "start_ms": 5_000 }),
        json!({ "kind": "section", "start_ms": 5_000, "end_ms": 4_000 }),
        json!({ "kind": "point", "start_ms": 5_000, "color": "red" }),
    ] {
        let (status, _, problem) = send(&h.app, json_request("POST", &markers, body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{problem}");
    }
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
        "/videos/{id}/thumbnails",
        "/videos/{id}/peaks",
        "/videos/{id}/attacks",
        "/videos/{id}/state",
        "/videos/{id}/markers",
        "/videos/{id}/markers/{marker_id}",
    ] {
        assert!(paths.contains_key(path), "missing {path}");
    }
    assert!(doc["components"]["securitySchemes"]["token"].is_object());
}
