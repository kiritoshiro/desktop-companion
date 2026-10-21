//! Port of `content/body_plans.py`: the body plans every spider is built on.
//!
//! Four plans (`bug`, `segmented`, `tarantula`, `jumper`) lifted verbatim from
//! the models that define them, plus two enemy-only rigs derived from the bug
//! plan. Each plan carries its own gait. A model keeps whatever it states for
//! itself: an explicit `legs` array wins, and an `appearance.spider_gait`
//! overrides the plan's. The leg/gait tables are in `data/body_plans.json`,
//! exported unchanged from the Python module.

use std::sync::LazyLock;

use serde_json::{Value, json};

use crate::model::{Appearance, LegDef, ModelDef, SpiderGait};
use crate::util::py_key_or_empty;

pub const BODY_PLAN_IDS: [&str; 6] = ["bug", "segmented", "tarantula", "jumper", "enemy_crab", "enemy_orb"];

pub const DEFAULT_BODY_PLAN: &str = "bug";

/// One plan: its rig and its gait, as raw JSON (the shape Python holds) and typed.
#[derive(Clone, Debug)]
pub struct BodyPlan {
    pub id: &'static str,
    pub legs_json: Vec<Value>,
    pub gait_json: Value,
    pub legs: Vec<LegDef>,
    pub gait: SpiderGait,
}

static RAW: &str = include_str!("data/body_plans.json");

fn enemy_legs(bug_legs: &[Value], crab: bool) -> Vec<Value> {
    let mut legs = bug_legs.to_vec();
    for (index, leg) in legs.iter_mut().enumerate() {
        let pair = index / 2;
        let angle: i64 = if crab { [58, 82, 120, 152][pair] } else { [35, 66, 118, 155][pair] };
        let reach: f64 = if crab { [2.25, 2.15, 1.28, 1.15][pair] } else { [1.85, 1.65, 1.50, 1.95][pair] };
        let attach_side: f64 = if crab { [0.25, 0.36, 0.36, 0.25][pair] } else { [0.19, 0.27, 0.27, 0.19][pair] };
        let left = leg.get("side").and_then(Value::as_str) == Some("left");
        let radians = (angle as f64).to_radians();
        let obj = leg.as_object_mut().expect("plan legs are objects");
        // dict.update keeps the position of keys that already exist.
        obj.insert("attach_forward".into(), json!([0.48, 0.29, 0.09, -0.10][pair]));
        obj.insert("attach_side".into(), json!(attach_side));
        obj.insert("rest_angle".into(), json!(angle * if left { -1 } else { 1 }));
        obj.insert("rest_forward".into(), json!(radians.cos() * reach));
        obj.insert("rest_side".into(), json!(radians.sin() * reach));
        obj.insert("reach".into(), json!(reach));
        obj.insert("upper_len".into(), json!(reach * 0.48));
        obj.insert("lower_len".into(), json!(reach * 0.62));
    }
    legs
}

static BODY_PLANS: LazyLock<Vec<BodyPlan>> = LazyLock::new(|| {
    let raw: Value = serde_json::from_str(RAW).expect("body_plans.json is valid");
    let legs_of = |id: &str| raw["legs"][id].as_array().expect("plan legs").clone();
    let gait_of = |id: &str| raw["gaits"][id].clone();
    let bug_legs = legs_of("bug");
    let with_hz = |hz: f64| {
        let mut g = gait_of("bug");
        g["cycle_hz"] = json!(hz);
        g
    };
    let entries: Vec<(&'static str, Vec<Value>, Value)> = vec![
        ("bug", bug_legs.clone(), gait_of("bug")),
        ("segmented", legs_of("segmented"), gait_of("segmented")),
        ("tarantula", legs_of("tarantula"), gait_of("tarantula")),
        ("jumper", legs_of("jumper"), gait_of("jumper")),
        ("enemy_crab", enemy_legs(&bug_legs, true), with_hz(2.15)),
        ("enemy_orb", enemy_legs(&bug_legs, false), with_hz(1.65)),
    ];
    entries
        .into_iter()
        .map(|(id, legs_json, gait_json)| BodyPlan {
            id,
            legs: legs_json.iter().map(|l| serde_json::from_value(l.clone()).expect("plan leg")).collect(),
            gait: serde_json::from_value(gait_json.clone()).expect("plan gait"),
            legs_json,
            gait_json,
        })
        .collect()
});

/// Every plan, in `BODY_PLAN_IDS` order.
pub fn body_plans() -> &'static [BodyPlan] {
    &BODY_PLANS
}

/// The plan for an id (case- and whitespace-insensitive), if it exists.
pub fn body_plan(plan_id: &str) -> Option<&'static BodyPlan> {
    let key = plan_id.trim().to_lowercase();
    BODY_PLANS.iter().find(|p| p.id == key)
}

pub fn is_body_plan(plan_id: &str) -> bool {
    body_plan(plan_id).is_some()
}

/// `body_plan_legs`: a fresh copy of the plan's rig (empty for an unknown plan).
pub fn body_plan_legs(plan_id: &str) -> Vec<LegDef> {
    body_plan(plan_id).map(|p| p.legs.clone()).unwrap_or_default()
}

/// `body_plan_gait`: a copy of the plan's gait, `None` for an unknown plan.
pub fn body_plan_gait(plan_id: &str) -> Option<SpiderGait> {
    body_plan(plan_id).map(|p| p.gait.clone())
}

/// `resolve_body_plan` on raw JSON, exactly as Python mutates the dict.
pub fn resolve_body_plan(data: &mut Value) {
    let Some(obj) = data.as_object_mut() else { return };
    let plan_id = py_key_or_empty(obj.get("body_plan"));
    let Some(plan) = (if plan_id.is_empty() { None } else { body_plan(&plan_id) }) else { return };
    let has_legs = obj.get("legs").is_some_and(crate::util::py_truthy);
    if !has_legs {
        obj.insert("legs".into(), Value::Array(plan.legs_json.clone()));
    }
    if !obj.get("appearance").is_some_and(Value::is_object) {
        obj.insert("appearance".into(), Value::Object(Default::default()));
    }
    let appearance = obj.get_mut("appearance").and_then(Value::as_object_mut).expect("appearance object");
    if !appearance.contains_key("spider_gait") {
        appearance.insert("spider_gait".into(), plan.gait_json.clone());
    }
}

/// `resolve_body_plan` on a typed model.
pub fn resolve_model_body_plan(model: &mut ModelDef) {
    let Some(plan) = model.body_plan.as_deref().filter(|p| !p.trim().is_empty()).and_then(body_plan) else {
        return;
    };
    if model.legs.is_empty() {
        model.legs = plan.legs.clone();
    }
    let appearance = model.appearance.get_or_insert_with(Appearance::default);
    if appearance.spider_gait.is_none() {
        appearance.spider_gait = Some(plan.gait.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_load() {
        assert_eq!(body_plans().len(), 6);
        for plan in body_plans() {
            assert_eq!(plan.legs.len(), 8, "{}", plan.id);
            assert_eq!(plan.gait.enabled, Some(true));
        }
        assert_eq!(body_plan("enemy_crab").unwrap().gait.cycle_hz, Some(2.15));
        assert_eq!(body_plan(" BUG ").unwrap().id, "bug");
    }
}
