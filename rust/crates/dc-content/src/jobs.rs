//! The job table from `world/jobs.py` that content needs.
//!
//! `preset_io.validate_preset` checks a slot's `job` against `JOB_BY_ID`.
//! `dc-world` owns jobs, but it depends on this crate, so the ids (and the
//! data that defines a job) are mirrored here; `dc-world` should re-export
//! these rather than redefine them. Values copied unchanged.

/// `JobDefinition`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JobDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub ability_ids: &'static [&'static str],
    pub creates_base: bool,
    pub protects_base: bool,
}

/// `JOB_DEFINITIONS`.
pub const JOB_DEFINITIONS: [JobDefinition; 6] = [
    JobDefinition {
        id: "none",
        display_name: "No job",
        description: "Acts only from temperament and selected abilities.",
        ability_ids: &[],
        creates_base: false,
        protects_base: false,
    },
    JobDefinition {
        id: "hunter",
        display_name: "Hunter",
        description: "Tracks nearby prey and threats with focused pursuit.",
        ability_ids: &[],
        creates_base: false,
        protects_base: false,
    },
    JobDefinition {
        id: "builder",
        display_name: "Builder",
        description: "Establishes and upgrades a shared colony base.",
        ability_ids: &[],
        creates_base: true,
        protects_base: false,
    },
    JobDefinition {
        id: "guard",
        display_name: "Guard",
        description: "Patrols a friendly base, and pins declared foes with silk.",
        ability_ids: &["shoot_web"],
        creates_base: false,
        protects_base: true,
    },
    JobDefinition {
        id: "scout",
        display_name: "Scout",
        description: "Ranges beyond the base and reports points of interest.",
        ability_ids: &[],
        creates_base: false,
        protects_base: false,
    },
    JobDefinition {
        id: "webber",
        display_name: "Web tender",
        description: "Maintains the colony's silk structures, and throws silk.",
        ability_ids: &["weave_web", "shoot_web"],
        creates_base: false,
        protects_base: false,
    },
];

/// `JOB_IDS`.
pub const JOB_IDS: [&str; 6] = ["none", "hunter", "builder", "guard", "scout", "webber"];

/// `JOB_BY_ID[id]`.
pub fn job_by_id(id: &str) -> Option<&'static JobDefinition> {
    JOB_DEFINITIONS.iter().find(|j| j.id == id)
}

/// `normalize_job_id`: aliases folded, unknown ids become `"none"`.
pub fn normalize_job_id(value: Option<&str>) -> &'static str {
    let id = value.unwrap_or("none").trim().to_lowercase();
    let id = match id.as_str() {
        "" | "default" => "none".to_string(),
        "weaver" => "webber".to_string(),
        "web_builder" => "builder".to_string(),
        _ => id,
    };
    job_by_id(&id).map(|j| j.id).unwrap_or("none")
}
