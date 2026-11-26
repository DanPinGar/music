//! Where the user left each video, so it reopens the same way: position, speed, step unit,
//! zoom, beat grid, loop and practice settings.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{DomainError, Result, VideoId};

/// The study state of one video. Every field has a default, so documents saved by older
/// versions still load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoState {
    pub position_ms: i64,
    /// Playback rate, 1.0 being normal speed.
    pub speed: f64,
    pub step_unit: StepUnit,
    /// The stretch the detail timeline showed.
    pub view: Option<TimelineView>,
    pub beat_grid: Option<BeatGrid>,
    #[serde(rename = "loop")]
    pub loop_range: Option<LoopRange>,
    pub practice: Practice,
}

impl Default for VideoState {
    fn default() -> Self {
        Self {
            position_ms: 0,
            speed: 1.0,
            step_unit: StepUnit::Seconds,
            view: None,
            beat_grid: None,
            loop_range: None,
            practice: Practice::default(),
        }
    }
}

/// What the arrow keys step by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepUnit {
    #[default]
    Seconds,
    Beat,
    Bar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineView {
    pub start_ms: i64,
    pub span_ms: i64,
}

/// The beats of the song, tapped by the user (and snapped to the attacks). Before the first
/// and after the last beat the grid continues at the tempo of its ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeatGrid {
    /// Times of the beats, strictly increasing.
    pub beats_ms: Vec<i64>,
    pub beats_per_bar: u32,
    /// Index of a beat that starts bar 1 (beats before it belong to bar 0 and earlier; it
    /// may lie outside `beats_ms`, where the grid is extrapolated).
    pub downbeat: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopRange {
    pub start_ms: i64,
    pub end_ms: i64,
    pub enabled: bool,
}

/// How loops are practised.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Practice {
    /// Beats played before the loop start on every repetition (0: none).
    pub pre_roll_beats: u32,
    /// A bar of metronome clicks before playback starts.
    pub count_in: bool,
    pub trainer: Option<SpeedTrainer>,
}

/// Speeds the loop up on every repetition: `from`, then `step` more each time, up to `to`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpeedTrainer {
    pub from: f64,
    pub step: f64,
    pub to: f64,
}

pub const MIN_SPEED: f64 = 0.25;
pub const MAX_SPEED: f64 = 2.0;
const MAX_BEATS: usize = 20_000;

impl VideoState {
    /// The state if it makes sense, or [`DomainError::Invalid`] saying what does not.
    pub fn validated(self) -> Result<Self> {
        let invalid = |what: &str| Err(DomainError::Invalid(what.to_owned()));
        if self.position_ms < 0 {
            return invalid("the position cannot be negative");
        }
        if !(MIN_SPEED..=MAX_SPEED).contains(&self.speed) {
            return invalid("the speed must be between 0.25 and 2");
        }
        if let Some(view) = self.view
            && (view.start_ms < 0 || view.span_ms < 100)
        {
            return invalid("the timeline view must start at 0 or later and span 100 ms or more");
        }
        if let Some(grid) = &self.beat_grid {
            if grid.beats_ms.len() < 2 || grid.beats_ms.len() > MAX_BEATS {
                return invalid("a beat grid needs between 2 and 20000 beats");
            }
            if grid.beats_ms.windows(2).any(|pair| pair[0] >= pair[1]) || grid.beats_ms[0] < 0 {
                return invalid("the beats must be in order, at 0 or later");
            }
            if !(1..=16).contains(&grid.beats_per_bar) {
                return invalid("a bar has between 1 and 16 beats");
            }
        }
        if let Some(range) = self.loop_range
            && (range.start_ms < 0 || range.end_ms <= range.start_ms)
        {
            return invalid("a loop must end after it starts, at 0 or later");
        }
        if self.practice.pre_roll_beats > 16 {
            return invalid("the pre-roll is at most 16 beats");
        }
        if let Some(trainer) = self.practice.trainer {
            let speeds = MIN_SPEED..=MAX_SPEED;
            if !speeds.contains(&trainer.from)
                || !speeds.contains(&trainer.to)
                || trainer.from > trainer.to
                || !(trainer.step > 0.0 && trainer.step <= 1.0)
            {
                return invalid("the speed trainer goes up from a speed to a higher one in steps");
            }
        }
        Ok(self)
    }
}

/// Port: persistence of each video's study state.
#[async_trait]
pub trait VideoStateRepository: Send + Sync {
    /// The saved state, or `None` if the video was never opened.
    async fn get_state(&self, video: VideoId) -> Result<Option<VideoState>>;

    async fn save_state(&self, video: VideoId, state: &VideoState) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(beats_ms: Vec<i64>) -> BeatGrid {
        BeatGrid {
            beats_ms,
            beats_per_bar: 4,
            downbeat: 0,
        }
    }

    #[test]
    fn the_default_state_is_valid() {
        assert!(VideoState::default().validated().is_ok());
    }

    #[test]
    fn older_documents_load_with_defaults() {
        let state: VideoState = serde_json::from_str(r#"{"position_ms": 1500}"#).unwrap();
        assert_eq!(state.position_ms, 1500);
        assert_eq!(state.speed, 1.0);
        assert_eq!(state.step_unit, StepUnit::Seconds);
        assert_eq!(state.practice, Practice::default());
    }

    #[test]
    fn the_loop_is_stored_as_loop() {
        let state = VideoState {
            loop_range: Some(LoopRange {
                start_ms: 1,
                end_ms: 2,
                enabled: false,
            }),
            ..VideoState::default()
        };
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["loop"]["end_ms"], 2);
    }

    #[test]
    fn nonsense_is_rejected() {
        let cases = [
            VideoState {
                speed: 3.0,
                ..VideoState::default()
            },
            VideoState {
                position_ms: -1,
                ..VideoState::default()
            },
            VideoState {
                beat_grid: Some(grid(vec![500, 400])),
                ..VideoState::default()
            },
            VideoState {
                loop_range: Some(LoopRange {
                    start_ms: 2000,
                    end_ms: 1000,
                    enabled: true,
                }),
                ..VideoState::default()
            },
            VideoState {
                practice: Practice {
                    trainer: Some(SpeedTrainer {
                        from: 1.0,
                        step: 0.05,
                        to: 0.6,
                    }),
                    ..Practice::default()
                },
                ..VideoState::default()
            },
        ];
        for state in cases {
            assert!(
                matches!(state.clone().validated(), Err(DomainError::Invalid(_))),
                "{state:?}"
            );
        }
    }

    #[test]
    fn a_full_state_is_valid() {
        let state = VideoState {
            position_ms: 30_000,
            speed: 0.75,
            step_unit: StepUnit::Bar,
            view: Some(TimelineView {
                start_ms: 28_000,
                span_ms: 10_000,
            }),
            beat_grid: Some(grid(vec![500, 1000, 1500])),
            loop_range: Some(LoopRange {
                start_ms: 1000,
                end_ms: 5000,
                enabled: true,
            }),
            practice: Practice {
                pre_roll_beats: 2,
                count_in: true,
                trainer: Some(SpeedTrainer {
                    from: 0.6,
                    step: 0.05,
                    to: 1.0,
                }),
            },
        };
        assert_eq!(state.clone().validated().unwrap(), state);
    }
}
