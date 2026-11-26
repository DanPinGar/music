use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use segno_domain::{Comment, CommentFields, CommentId, VideoId};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{ApiError, AppState, Problem};

/// A comment on a moment or a stretch of a video.
#[derive(Debug, Serialize, ToSchema)]
pub struct CommentDto {
    pub id: Uuid,
    pub start_ms: i64,
    /// End of the stretch commented on; absent for a moment.
    pub end_ms: Option<i64>,
    /// Markdown.
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// The text and anchor of a comment, to create one or replace an existing one's.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CommentFieldsDto {
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    /// Markdown, not empty.
    pub body: String,
}

impl From<Comment> for CommentDto {
    fn from(comment: Comment) -> Self {
        Self {
            id: comment.id.as_uuid(),
            start_ms: comment.start_ms,
            end_ms: comment.end_ms,
            body: comment.body,
            created_at: comment.created_at,
            updated_at: comment.updated_at,
        }
    }
}

impl From<CommentFieldsDto> for CommentFields {
    fn from(dto: CommentFieldsDto) -> Self {
        Self {
            start_ms: dto.start_ms,
            end_ms: dto.end_ms,
            body: dto.body,
        }
    }
}

/// The video's comments, in time order.
#[utoipa::path(
    get,
    path = "/videos/{id}/comments",
    tag = "study",
    security(("token" = [])),
    params(("id" = Uuid, Path, description = "Video id")),
    responses(
        (status = 200, description = "The comments", body = [CommentDto]),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 404, description = "No such video", body = Problem)
    )
)]
pub async fn list_comments(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<CommentDto>>, ApiError> {
    let video = state.videos.get(VideoId::from_uuid(id)).await?;
    let comments = state.comments.list_comments(video.id).await?;
    Ok(Json(comments.into_iter().map(CommentDto::from).collect()))
}

/// Comments on a moment or a stretch of the video.
#[utoipa::path(
    post,
    path = "/videos/{id}/comments",
    tag = "study",
    security(("token" = [])),
    params(("id" = Uuid, Path, description = "Video id")),
    request_body = CommentFieldsDto,
    responses(
        (status = 201, description = "The new comment", body = CommentDto),
        (status = 400, description = "Empty, or a stretch that ends before it starts", body = Problem),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 404, description = "No such video", body = Problem)
    )
)]
pub async fn create_comment(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<CommentFieldsDto>,
) -> Result<(StatusCode, Json<CommentDto>), ApiError> {
    let video = state.videos.get(VideoId::from_uuid(id)).await?;
    let comment = Comment::new(video.id, body.into())?;
    state.comments.insert_comment(&comment).await?;
    Ok((StatusCode::CREATED, Json(comment.into())))
}

/// Rewrites a comment or moves it.
#[utoipa::path(
    put,
    path = "/videos/{id}/comments/{comment_id}",
    tag = "study",
    security(("token" = [])),
    params(
        ("id" = Uuid, Path, description = "Video id"),
        ("comment_id" = Uuid, Path, description = "Comment id")
    ),
    request_body = CommentFieldsDto,
    responses(
        (status = 200, description = "The comment as saved", body = CommentDto),
        (status = 400, description = "Empty, or a stretch that ends before it starts", body = Problem),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 404, description = "No such video or comment", body = Problem)
    )
)]
pub async fn update_comment(
    State(state): State<AppState>,
    Path((id, comment_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<CommentFieldsDto>,
) -> Result<Json<CommentDto>, ApiError> {
    let mut comment = state
        .comments
        .get_comment(VideoId::from_uuid(id), CommentId::from_uuid(comment_id))
        .await?;
    comment.change(body.into())?;
    state.comments.update_comment(&comment).await?;
    Ok(Json(comment.into()))
}

/// Deletes a comment.
#[utoipa::path(
    delete,
    path = "/videos/{id}/comments/{comment_id}",
    tag = "study",
    security(("token" = [])),
    params(
        ("id" = Uuid, Path, description = "Video id"),
        ("comment_id" = Uuid, Path, description = "Comment id")
    ),
    responses(
        (status = 204, description = "Deleted"),
        (status = 401, description = "Missing or invalid token", body = Problem),
        (status = 404, description = "No such video or comment", body = Problem)
    )
)]
pub async fn delete_comment(
    State(state): State<AppState>,
    Path((id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    state
        .comments
        .delete_comment(VideoId::from_uuid(id), CommentId::from_uuid(comment_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
