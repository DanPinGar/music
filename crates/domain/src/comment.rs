//! Comments on a video: Markdown notes anchored to a moment or a stretch of it.

use std::fmt;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{DomainError, Result, VideoId};

/// Identifier of a comment (UUIDv7, like videos).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommentId(Uuid);

impl CommentId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for CommentId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CommentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for CommentId {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|_| DomainError::Invalid(format!("invalid comment id: {s}")))
    }
}

/// What can change in a comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentFields {
    pub start_ms: i64,
    /// End of the stretch commented on; `None` for a moment.
    pub end_ms: Option<i64>,
    /// Markdown.
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub id: CommentId,
    pub video: VideoId,
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const MAX_BODY_CHARS: usize = 10_000;

impl Comment {
    pub fn new(video: VideoId, fields: CommentFields) -> Result<Self> {
        let fields = validate(fields)?;
        let now = Utc::now();
        Ok(Self {
            id: CommentId::new(),
            video,
            start_ms: fields.start_ms,
            end_ms: fields.end_ms,
            body: fields.body,
            created_at: now,
            updated_at: now,
        })
    }

    /// Replaces the text and the anchor.
    pub fn change(&mut self, fields: CommentFields) -> Result<()> {
        let fields = validate(fields)?;
        self.start_ms = fields.start_ms;
        self.end_ms = fields.end_ms;
        self.body = fields.body;
        self.updated_at = Utc::now();
        Ok(())
    }
}

fn validate(mut fields: CommentFields) -> Result<CommentFields> {
    let invalid = |what: String| Err(DomainError::Invalid(what));
    if fields.start_ms < 0 {
        return invalid("a comment cannot start before the video".into());
    }
    if fields.end_ms.is_some_and(|end| end <= fields.start_ms) {
        return invalid("a comment's stretch must end after it starts".into());
    }
    fields.body = fields.body.trim().to_owned();
    if fields.body.is_empty() {
        return invalid("a comment needs some text".into());
    }
    if fields.body.chars().count() > MAX_BODY_CHARS {
        return invalid(format!("comments are at most {MAX_BODY_CHARS} characters"));
    }
    Ok(fields)
}

/// Port: persistence of the comments.
#[async_trait]
pub trait CommentRepository: Send + Sync {
    /// The comments of a video that have not been deleted, in time order.
    async fn list_comments(&self, video: VideoId) -> Result<Vec<Comment>>;

    /// One comment of a video, or [`DomainError::NotFound`].
    async fn get_comment(&self, video: VideoId, id: CommentId) -> Result<Comment>;

    async fn insert_comment(&self, comment: &Comment) -> Result<()>;

    /// Saves every field of an existing comment.
    async fn update_comment(&self, comment: &Comment) -> Result<()>;

    /// Deletes a comment (softly, for future sync), or [`DomainError::NotFound`].
    async fn delete_comment(&self, video: VideoId, id: CommentId) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(start_ms: i64, end_ms: Option<i64>, body: &str) -> CommentFields {
        CommentFields {
            start_ms,
            end_ms,
            body: body.into(),
        }
    }

    #[test]
    fn comments_keep_their_markdown_trimmed() {
        let comment =
            Comment::new(VideoId::new(), fields(1000, None, "  **Pedal** here\n")).unwrap();
        assert_eq!(comment.body, "**Pedal** here");
    }

    #[test]
    fn empty_comments_and_backwards_stretches_are_rejected() {
        let video = VideoId::new();
        assert!(Comment::new(video, fields(1000, None, "   ")).is_err());
        assert!(Comment::new(video, fields(1000, Some(900), "x")).is_err());
        assert!(Comment::new(video, fields(-1, None, "x")).is_err());
        assert!(Comment::new(video, fields(1000, Some(2000), "a stretch")).is_ok());
    }

    #[test]
    fn changes_replace_text_and_anchor() {
        let mut comment = Comment::new(VideoId::new(), fields(1000, None, "first")).unwrap();
        comment.change(fields(500, Some(1500), "second")).unwrap();
        assert_eq!(
            (comment.start_ms, comment.end_ms, comment.body.as_str()),
            (500, Some(1500), "second")
        );
        assert!(comment.change(fields(0, None, "")).is_err());
    }
}
