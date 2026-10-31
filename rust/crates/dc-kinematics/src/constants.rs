//! Port of the parts of `creature/constants.py` the kinematics use.
//!
//! The creature crate should re-export these rather than define its own:
//! `smootherstep`, `GAIT_BY_STATE` (as [`gait_by_state`]), `GAIT_SPELL_GAP`,
//! `GAIT_SPELL_LENGTH`, `VALID_GAIT_STYLES`, `GAIT_LABELS` and
//! `normalize_gait_style`. Everything else in `constants.py` (combat, webbing,
//! nerve, jobs) is behaviour and belongs to `dc-creature`.

use crate::core::{GaitStyle, MotionState};
use dc_core::math::clamp;

/// `smootherstep(t)`: Perlin's quintic ease, clamped to `[0, 1]`.
#[inline]
pub fn smootherstep(t: f64) -> f64 {
    let t = clamp(t, 0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// DC-56: the activity that overrides a spider's baseline gait, if any.
/// A state absent from the Python map (Wander, most notably) returns `None`.
pub fn gait_by_state(state: MotionState) -> Option<GaitStyle> {
    use MotionState::*;
    match state {
        Inspect | Observe | Alert | WebWalk | WebApproach => Some(GaitStyle::Skitter),
        Chase | Approach | Retreat | Startled | Zoom | DriftRun | Play => Some(GaitStyle::Lively),
        _ => None,
    }
}

/// `GAIT_BY_STATE` as name pairs, in the Python dictionary's order.
pub const GAIT_BY_STATE: &[(&str, &str)] = &[
    ("Inspect", "skitter"),
    ("Observe", "skitter"),
    ("Alert", "skitter"),
    ("WebWalk", "skitter"),
    ("WebApproach", "skitter"),
    ("Chase", "lively"),
    ("Approach", "lively"),
    ("Retreat", "lively"),
    ("Startled", "lively"),
    ("Zoom", "lively"),
    ("DriftRun", "lively"),
    ("Play", "lively"),
];

/// DC-63: seconds between gait spells, drawn `uniform(*GAIT_SPELL_GAP)`.
pub const GAIT_SPELL_GAP: (f64, f64) = (9.0, 26.0);
/// DC-63: how long a spell lasts.
pub const GAIT_SPELL_LENGTH: (f64, f64) = (1.8, 4.5);

pub const VALID_GAIT_STYLES: [&str; 3] = ["classic", "lively", "skitter"];

pub const GAIT_LABELS: [(&str, &str); 3] =
    [("classic", "Classic"), ("lively", "Lively (lifted legs)"), ("skitter", "Skitter (rapid bursts)")];

/// `normalize_gait_style(style)`: lower-cased and trimmed, `classic` when unknown.
pub fn normalize_gait_style(style: Option<&str>) -> GaitStyle {
    match style.unwrap_or("classic").trim().to_lowercase().as_str() {
        "lively" => GaitStyle::Lively,
        "skitter" => GaitStyle::Skitter,
        _ => GaitStyle::Classic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smootherstep_endpoints() {
        assert_eq!(smootherstep(-1.0), 0.0);
        assert_eq!(smootherstep(0.5), 0.5);
        assert_eq!(smootherstep(2.0), 1.0);
    }

    #[test]
    fn gait_map_matches_table() {
        for (name, style) in GAIT_BY_STATE {
            let mapped = gait_by_state(MotionState::from_name(name)).unwrap();
            assert_eq!(mapped.as_str(), *style, "{name}");
        }
        assert_eq!(gait_by_state(MotionState::Wander), None);
        assert_eq!(normalize_gait_style(Some(" Skitter ")), GaitStyle::Skitter);
        assert_eq!(normalize_gait_style(Some("bogus")), GaitStyle::Classic);
        assert_eq!(normalize_gait_style(None), GaitStyle::Classic);
    }
}
