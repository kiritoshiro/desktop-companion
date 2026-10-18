//! Port of `desktop_bug/content/*`: the data a spider is made of.
//!
//! - [`model`], [`personality`], [`preset`]: typed JSON files.
//! - [`body_plans`]: the six shared leg rigs and gaits.
//! - [`personality_profiles`]: temperaments, phase decks, behaviour modules.
//! - [`skills`]: the skill registry and composition rules.
//! - [`palettes`], [`enemy_kinds`], [`enemy_traits`]: colours and enemies.
//! - [`discovery`]: data roots, `state_dir`, model/personality/preset discovery
//!   and validation. [`preset_io`]: preset validation, load and save.
//! - [`jobs`]: the job id table preset validation needs (mirrors `world/jobs.py`).

pub mod body_plans;
pub mod discovery;
pub mod enemy_kinds;
pub mod enemy_traits;
pub mod jobs;
pub mod model;
pub mod palettes;
pub mod personality;
pub mod personality_profiles;
pub mod preset;
pub mod preset_io;
pub mod skills;
pub mod util;

pub use model::{Appearance, LegDef, ModelDef, SpiderGait};
pub use personality::{Personality, Temperament};
pub use preset::{Preset, PresetSettings, PresetSlot};
pub use util::{ColorMap, OrderedMap};

/// Errors from loading or saving content files.
#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    #[error("{0}")]
    Invalid(String),
    #[error("{path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("{path}: {source}")]
    Json { path: String, source: serde_json::Error },
}

impl ContentError {
    pub fn invalid(message: impl Into<String>) -> Self {
        ContentError::Invalid(message.into())
    }
}
