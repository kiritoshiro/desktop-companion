//! Port of `manager/constants.py`.
pub const RANDOM_MODEL_ID: &str = "__random_model__";
pub const RANDOM_PERSONALITY_ID: &str = "__random_personality__";
pub const FEED_XP_REWARD: f64 = 110.0;
pub const DAMAGE_XP_PER_POINT: f64 = 4.0;
pub const KILL_XP_REWARD: f64 = 90.0;
pub const RUNTIME_STATE_FLUSH_SECONDS: f64 = 5.0;
pub fn clamp(value: f64, low: f64, high: f64) -> f64 {
    low.max(high.min(value))
}
