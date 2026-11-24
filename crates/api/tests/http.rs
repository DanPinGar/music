//! End-to-end tests of the router: authentication, Host checks and the video routes.

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, HOST};
use axum::http::{Request, StatusCode};
use segno_api::{AppState, SecurityConfig, SystemInfo};
use segno_domain::{Video, VideoRepository, VideoSource};
use segno_storage::SqliteStore;
use serde_json::Value;
use tower::ServiceExt;

const PORT: u16 = 7700;
const TOKEN: &str = "test-token";

async fn app_with(store: SqliteStore) -> Router {
    let state = AppState {
        system: Arc::new(SystemInfo {
            version: "test".into(),
            library_path: None,
            schema_version: store.schema_version().await.unwrap(),
        }),
        videos: Arc::new(store),
    };
    segno_api::router(
        state,
        SecurityConfig::local(TOKEN, PORT, &["http://tauri.localhost"]),
    )
}

async fn app() -> Router {
    app_with(SqliteStore::in_memory().await.unwrap()).await
}

fn get(uri: &str) -> axum::http::request::Builder {
    Request::get(uri).header(HOST, format!("127.0.0.1:{PORT}"))
}

async fn send(
    app: Router,
    request: axum::http::request::Builder,
) -> (StatusCode, Option<String>, Value) {
    let response = app
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .map(|value| value.to_str().unwrap().to_owned());
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, content_type, body)
}

#[tokio::test]
async fn health_needs_no_token() {
    let (status, _, body) = send(app().await, get("/health")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn protected_routes_reject_missing_tokens_with_a_problem() {
    let (status, content_type, body) = send(app().await, get("/system")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(content_type.as_deref(), Some("application/problem+json"));
    assert_eq!(body["status"], 401);
}

#[tokio::test]
async fn the_bearer_token_opens_protected_routes() {
    let request = get("/system").header(AUTHORIZATION, format!("Bearer {TOKEN}"));
    let (status, _, body) = send(app().await, request).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["version"], "test");
    assert!(body["schema_version"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn the_token_can_travel_in_the_query_for_media_elements() {
    let (status, _, _) = send(app().await, get(&format!("/system?token={TOKEN}"))).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn unexpected_hosts_are_rejected() {
    let request = Request::get("/health").header(HOST, "evil.example");
    let (status, _, _) = send(app().await, request).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn videos_are_listed() {
    let store = SqliteStore::in_memory().await.unwrap();
    let video = Video::new_importing(VideoSource::File, "Clair de lune", None).unwrap();
    store.insert(&video).await.unwrap();

    let request = get("/videos").header(AUTHORIZATION, format!("Bearer {TOKEN}"));
    let (status, _, body) = send(app_with(store).await, request).await;
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
    let uri = format!("/videos/{}", uuid::Uuid::now_v7());
    let request = get(&uri).header(AUTHORIZATION, format!("Bearer {TOKEN}"));
    let (status, content_type, _) = send(app().await, request).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(content_type.as_deref(), Some("application/problem+json"));
}

#[test]
fn the_openapi_document_describes_every_route() {
    let doc = serde_json::to_value(segno_api::openapi()).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    for path in ["/health", "/system", "/videos", "/videos/{id}"] {
        assert!(paths.contains_key(path), "missing {path}");
    }
    assert!(doc["components"]["securitySchemes"]["token"].is_object());
}
