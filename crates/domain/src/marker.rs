//! Markers on a video's timeline: points to start from, named sections and saved loops.

use std::fmt;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{DomainError, Result, VideoId};

/// Identifier of a marker (UUIDv7, like videos).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MarkerId(Uuid);

impl MarkerId {
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

impl Default for MarkerId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MarkerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for MarkerId {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|_| DomainError::Invalid(format!("invalid marker id: {s}")))
    }
}

/// A point to start playing from, a named section ("Intro", "Chorus") or a saved loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerKind {
    Point,
    Section,
    Loop,
}

impl MarkerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Point => "point",
            Self::Section => "section",
            Self::Loop => "loop",
        }
    }

    /// Sections and loops cover a range; points mark one moment.
    pub fn is_range(self) -> bool {
        !matches!(self, Self::Point)
    }
}

impl FromStr for MarkerKind {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "point" => Ok(Self::Point),
            "section" => Ok(Self::Section),
            "loop" => Ok(Self::Loop),
            other => Err(DomainError::Invalid(format!(
                "unknown marker kind: {other}"
            ))),
        }
    }
}

/// What can change in a marker (its kind cannot).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkerFields {
    pub start_ms: i64,
    /// End of a section or loop; `None` for points.
    pub end_ms: Option<i64>,
    pub label: String,
    /// `#rrggbb`, or `None` for the default colour.
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    pub id: MarkerId,
    pub video: VideoId,
    pub kind: MarkerKind,
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    pub label: String,
    pub color: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const MAX_LABEL_CHARS: usize = 120;

impl Marker {
    pub fn new(video: VideoId, kind: MarkerKind, fields: MarkerFields) -> Result<Self> {
        let fields = validate(kind, fields)?;
        let now = Utc::now();
        Ok(Self {
            id: MarkerId::new(),
            video,
            kind,
            start_ms: fields.start_ms,
            end_ms: fields.end_ms,
            label: fields.label,
            color: fields.color,
            created_at: now,
            updated_at: now,
        })
    }

    /// Replaces the editable fields, keeping the kind.
    pub fn change(&mut self, fields: MarkerFields) -> Result<()> {
        let fields = validate(self.kind, fields)?;
        self.start_ms = fields.start_ms;
        self.end_ms = fields.end_ms;
        self.label = fields.label;
        self.color = fields.color;
        self.updated_at = Utc::now();
        Ok(())
    }
}

fn validate(kind: MarkerKind, mut fields: MarkerFields) -> Result<MarkerFields> {
    let invalid = |what: String| Err(DomainError::Invalid(what));
    if fields.start_ms < 0 {
        return invalid("a marker cannot start before the video".into());
    }
    match (kind.is_range(), fields.end_ms) {
        (false, Some(_)) => return invalid("a point marker has no end".into()),
        (true, None) => return invalid(format!("a {} needs an end", kind.as_str())),
        (true, Some(end)) if end <= fields.start_ms => {
            return invalid(format!("a {} must end after it starts", kind.as_str()));
        }
        _ => {}
    }
    fields.label = fields.label.trim().to_owned();
    if fields.label.chars().count() > MAX_LABEL_CHARS {
        return invalid(format!("labels are at most {MAX_LABEL_CHARS} characters"));
    }
    if let Some(color) = &fields.color {
        let hex = color.strip_prefix('#').unwrap_or("");
        if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return invalid(format!("not a #rrggbb colour: {color}"));
        }
        fields.color = Some(color.to_ascii_lowercase());
    }
    Ok(fields)
}

/// Port: persistence of the markers.
#[async_trait]
pub trait MarkerRepository: Send + Sync {
    /// The markers of a video that have not been deleted, in time order.
    async fn list_markers(&self, video: VideoId) -> Result<Vec<Marker>>;

    /// One marker of a video, or [`DomainError::NotFound`].
    async fn get_marker(&self, video: VideoId, id: MarkerId) -> Result<Marker>;

    async fn insert_marker(&self, marker: &Marker) -> Result<()>;

    /// Saves every field of an existing marker.
    async fn update_marker(&self, marker: &Marker) -> Result<()>;

    /// Deletes a marker (softly, for future sync), or [`DomainError::NotFound`].
    async fn delete_marker(&self, video: VideoId, id: MarkerId) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(start_ms: i64, end_ms: Option<i64>) -> MarkerFields {
        MarkerFields {
            start_ms,
            end_ms,
            label: "  Chorus ".into(),
            color: Some("#FFAA00".into()),
        }
    }

    #[test]
    fn markers_are_cleaned_up() {
        let marker = Marker::new(
            VideoId::new(),
            MarkerKind::Section,
            fields(1000, Some(5000)),
        )
        .unwrap();
        assert_eq!(marker.label, "Chorus");
        assert_eq!(marker.color.as_deref(), Some("#ffaa00"));
    }

    #[test]
    fn points_have_no_end_and_ranges_need_one() {
        let video = VideoId::new();
        assert!(Marker::new(video, MarkerKind::Point, fields(1000, None)).is_ok());
        assert!(Marker::new(video, MarkerKind::Point, fields(1000, Some(2000))).is_err());
        assert!(Marker::new(video, MarkerKind::Loop, fields(1000, None)).is_err());
        assert!(Marker::new(video, MarkerKind::Loop, fields(1000, Some(1000))).is_err());
    }

    #[test]
    fn bad_colours_and_negative_times_are_rejected() {
        let video = VideoId::new();
        let mut bad = fields(1000, None);
        bad.color = Some("orange".into());
        assert!(Marker::new(video, MarkerKind::Point, bad).is_err());
        assert!(Marker::new(video, MarkerKind::Point, fields(-5, None)).is_err());
    }

    #[test]
    fn changes_keep_the_kind_and_touch_updated_at() {
        let mut marker =
            Marker::new(VideoId::new(), MarkerKind::Point, fields(1000, None)).unwrap();
        let before = marker.updated_at;
        marker
            .change(MarkerFields {
                start_ms: 2500,
                end_ms: None,
                label: "Bridge".into(),
                color: None,
            })
            .unwrap();
        assert_eq!((marker.start_ms, marker.label.as_str()), (2500, "Bridge"));
        assert!(marker.updated_at >= before);
        assert!(
            marker
                .change(MarkerFields {
                    start_ms: 0,
                    end_ms: Some(10),
                    label: String::new(),
                    color: None,
                })
                .is_err()
        );
    }

    #[test]
    fn kinds_round_trip_through_their_storage_names() {
        for kind in [MarkerKind::Point, MarkerKind::Section, MarkerKind::Loop] {
            assert_eq!(kind.as_str().parse::<MarkerKind>().unwrap(), kind);
        }
    }
}
