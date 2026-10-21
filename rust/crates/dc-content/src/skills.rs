//! Port of `content/skills.py`: the skill registry for creature abilities.
//!
//! A skill is metadata plus an id; the behaviour itself is the matching
//! branch of the creature's state machine. "Ability" skills are true
//! capabilities a slot can toggle; every other category is a behaviour the
//! personality controls.

use std::collections::BTreeSet;

use crate::personality_profiles::{PersonalityRef, ability_ids_for};

/// One selectable creature ability (`CreatureSkill`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreatureSkill {
    pub id: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub category: &'static str,
}

impl CreatureSkill {
    pub fn is_ability(&self) -> bool {
        self.category == "Ability"
    }
}

const fn skill(
    id: &'static str,
    display_name: &'static str,
    description: &'static str,
    category: &'static str,
) -> CreatureSkill {
    CreatureSkill { id, display_name, description, category }
}

/// `SKILLS`, in registry order (`SKILL_CLASSES`).
pub const SKILLS: [CreatureSkill; 17] = [
    skill(
        "approach",
        "Approach / stalk",
        "Allows cautious movement toward the cursor before chasing or inspecting.",
        "Interaction",
    ),
    skill("wander", "Wander / roam", "Allows idle roaming between cursor interactions.", "Movement"),
    skill(
        "drift",
        "Drift / slide",
        "Allows momentum-based drift sliding: builds speed first, breaks traction, leans/counter-steers through wide circle or corner skids, and keeps sliding after throws.",
        "Ability",
    ),
    skill("jump", "Jump / hop", "Allows small hops and the airborne part of pounces.", "Movement"),
    skill("roll", "Roll / tumble", "Allows happy curl-up tumbles and rolling flourishes.", "Movement"),
    skill("chase", "Chase", "Allows burst chasing of the cursor or a playmate.", "Interaction"),
    skill(
        "observe",
        "Observe / orbit",
        "Allows observer spiders to circle and watch a cursor or another spider.",
        "Interaction",
    ),
    skill(
        "run_away",
        "Run away / retreat",
        "Allows threat-triggered retreats away from fast cursor movement.",
        "Survival",
    ),
    skill(
        "prepare_jump_attack",
        "Prepare jump attack",
        "Allows the crouch, rangefinder feeler pose, and pounce wind-up.",
        "Interaction",
    ),
    skill("inspect", "Inspect", "Allows curious close inspection and studying behaviour.", "Interaction"),
    skill("cuddle", "Cuddle", "Allows affectionate approach, snuggling, and boop behaviour.", "Social"),
    skill(
        "social_play",
        "Social play",
        "Allows direct play bouts between spiders when social play is enabled.",
        "Social",
    ),
    skill("zoomies", "Zoomies", "Allows short playful sprint bursts.", "Movement"),
    skill(
        "weave_web",
        "Weave web",
        "Allows building silk webs thread by thread in screen corners, and finishing an abandoned unfinished web even if another spider began it.",
        "Ability",
    ),
    skill(
        "web_walk",
        "Walk on webs",
        "Allows walking onto a finished web and plucking it to test its bounce.",
        "Ability",
    ),
    skill(
        "shoot_web",
        "Shoot trapping web",
        "Allows aiming and firing a glob of sticky silk that pins its target in place, whether the mouse pointer or a fly. Wiggle a trapped pointer to break it free.",
        "Ability",
    ),
    skill(
        "wall_web",
        "Web-shove to wall",
        "Allows firing a web that shoves its target to the nearest wall and pins it there, whether the mouse pointer or a fly. Wiggle a trapped pointer to peel it off.",
        "Ability",
    ),
];

/// `SKILL_BY_ID`.
pub fn skill_by_id(id: &str) -> Option<&'static CreatureSkill> {
    SKILLS.iter().find(|s| s.id == id)
}

pub fn is_skill(id: &str) -> bool {
    skill_by_id(id).is_some()
}

/// `DEFAULT_SKILL_IDS`: every skill, in registry order.
pub fn default_skill_ids() -> Vec<&'static str> {
    SKILLS.iter().map(|s| s.id).collect()
}

/// `ABILITY_SKILL_IDS`.
pub fn ability_skill_ids() -> Vec<&'static str> {
    SKILLS.iter().filter(|s| s.is_ability()).map(|s| s.id).collect()
}

/// `BEHAVIOUR_SKILL_IDS`.
pub fn behaviour_skill_ids() -> Vec<&'static str> {
    SKILLS.iter().filter(|s| !s.is_ability()).map(|s| s.id).collect()
}

pub fn is_ability_skill(id: &str) -> bool {
    skill_by_id(id).is_some_and(CreatureSkill::is_ability)
}

pub const COMMON_BEHAVIOUR_IDS: [&str; 12] = [
    "approach",
    "wander",
    "jump",
    "roll",
    "chase",
    "observe",
    "run_away",
    "prepare_jump_attack",
    "inspect",
    "cuddle",
    "social_play",
    "zoomies",
];
pub const COMMON_ABILITY_IDS: [&str; 1] = ["web_walk"];

/// `COMMON_SKILL_IDS`.
pub fn common_skill_ids() -> Vec<&'static str> {
    COMMON_BEHAVIOUR_IDS.iter().chain(COMMON_ABILITY_IDS.iter()).copied().collect()
}

/// `SHORT_LABELS`.
pub fn short_label(id: &str) -> Option<&'static str> {
    Some(match id {
        "approach" => "approach",
        "wander" => "wander",
        "drift" => "drift",
        "jump" => "jump",
        "roll" => "roll",
        "chase" => "chase",
        "observe" => "observe",
        "run_away" => "run away",
        "prepare_jump_attack" => "jump attack",
        "inspect" => "inspect",
        "cuddle" => "cuddle",
        "social_play" => "play",
        "zoomies" => "zoomies",
        "weave_web" => "weave",
        "web_walk" => "web walk",
        "shoot_web" => "web trap",
        "wall_web" => "wall pin",
        _ => return None,
    })
}

fn wanted_set<S: AsRef<str>>(value: &[S]) -> BTreeSet<String> {
    value.iter().map(|s| s.as_ref().trim().to_lowercase()).filter(|s| !s.is_empty()).collect()
}

/// `normalize_skill_ids`: known ids in registry order; `None` means all.
pub fn normalize_skill_ids<S: AsRef<str>>(value: Option<&[S]>) -> Vec<String> {
    let Some(value) = value else {
        return default_skill_ids().into_iter().map(String::from).collect();
    };
    let wanted = wanted_set(value);
    SKILLS.iter().filter(|s| wanted.contains(s.id)).map(|s| s.id.to_string()).collect()
}

/// `normalize_ability_ids`: only capability ids, in registry order; `None` means all.
pub fn normalize_ability_ids<S: AsRef<str>>(value: Option<&[S]>) -> Vec<String> {
    let Some(value) = value else {
        return ability_skill_ids().into_iter().map(String::from).collect();
    };
    let wanted = wanted_set(value);
    SKILLS.iter().filter(|s| s.is_ability() && wanted.contains(s.id)).map(|s| s.id.to_string()).collect()
}

/// `default_skills_for_personality`: behaviours and abilities for a
/// personality (`None` means the full default set).
pub fn default_skills_for_personality<'a>(personality: Option<PersonalityRef<'a>>) -> Vec<String> {
    let Some(p) = personality else {
        return normalize_skill_ids::<&str>(None);
    };
    match p {
        PersonalityRef::Def(def) => {
            if let Some(skills) = &def.skills {
                return normalize_skill_ids(Some(skills.as_slice()));
            }
            let mut wanted: Vec<String> = match &def.behaviours {
                Some(b) => b.iter().map(|s| s.trim().to_lowercase()).collect(),
                None => COMMON_BEHAVIOUR_IDS.iter().map(|s| s.to_string()).collect(),
            };
            if def.include_common_abilities() {
                wanted.extend(COMMON_ABILITY_IDS.iter().map(|s| s.to_string()));
            }
            for extra in ability_ids_for(p) {
                if !wanted.contains(&extra) {
                    wanted.push(extra);
                }
            }
            normalize_skill_ids(Some(wanted.as_slice()))
        }
        PersonalityRef::Id(_) => {
            let mut wanted: Vec<String> = common_skill_ids().into_iter().map(String::from).collect();
            for extra in ability_ids_for(p) {
                if !wanted.contains(&extra) {
                    wanted.push(extra);
                }
            }
            normalize_skill_ids(Some(wanted.as_slice()))
        }
    }
}

/// `default_ability_ids`: only the capabilities a personality grants itself.
pub fn default_ability_ids<'a>(personality: Option<PersonalityRef<'a>>) -> Vec<String> {
    default_skills_for_personality(personality).into_iter().filter(|s| is_ability_skill(s)).collect()
}

/// `skills_with_selected_abilities`: personality behaviours plus an explicit
/// capability selection (`None` means every ability).
pub fn skills_with_selected_abilities<'a, S: AsRef<str>>(
    personality: Option<PersonalityRef<'a>>,
    abilities: Option<&[S]>,
) -> Vec<String> {
    let base = default_skills_for_personality(personality);
    let mut out: Vec<String> = base.into_iter().filter(|s| !is_ability_skill(s)).collect();
    out.extend(normalize_ability_ids(abilities));
    out
}

/// `skills_with_default_abilities`: personality behaviours, its own
/// abilities, plus `extra_abilities` (job-required ones).
pub fn skills_with_default_abilities<'a, S: AsRef<str>>(
    personality: Option<PersonalityRef<'a>>,
    extra_abilities: Option<&[S]>,
) -> Vec<String> {
    let mut wanted = default_ability_ids(personality);
    if let Some(extra) = extra_abilities {
        wanted.extend(extra.iter().map(|s| s.as_ref().trim().to_lowercase()));
    }
    skills_with_selected_abilities(personality, Some(wanted.as_slice()))
}

/// `unknown_skill_ids`.
pub fn unknown_skill_ids<S: AsRef<str>>(value: Option<&[S]>) -> Vec<String> {
    match value {
        None => Vec::new(),
        Some(v) => {
            v.iter().filter(|s| !is_skill(&s.as_ref().trim().to_lowercase())).map(|s| s.as_ref().to_string()).collect()
        }
    }
}

fn summary(ids: &[String], all: &[&str], all_label: &str, none_label: &str, noun: &str) -> String {
    let set: BTreeSet<&str> = ids.iter().map(String::as_str).collect();
    let all_set: BTreeSet<&str> = all.iter().copied().collect();
    if set == all_set {
        return all_label.to_string();
    }
    if ids.is_empty() {
        return none_label.to_string();
    }
    let labels: Vec<&str> = ids.iter().map(|id| short_label(id).unwrap_or(id)).collect();
    if labels.len() <= 3 {
        return labels.join(", ");
    }
    format!("{} {noun}: {}…", labels.len(), labels[..3].join(", "))
}

/// `compact_skill_summary`.
pub fn compact_skill_summary<S: AsRef<str>>(skill_ids: Option<&[S]>) -> String {
    let ids = normalize_skill_ids(skill_ids);
    summary(&ids, &default_skill_ids(), "All skills", "No optional skills", "skills")
}

/// `compact_ability_summary`.
pub fn compact_ability_summary<S: AsRef<str>>(skill_ids: Option<&[S]>) -> String {
    let ids = normalize_ability_ids(skill_ids);
    summary(&ids, &ability_skill_ids(), "All abilities", "No optional abilities", "abilities")
}

/// `SkillSet`: the mutable allowed-skill set of one live creature.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkillSet {
    ids: BTreeSet<String>,
}

impl SkillSet {
    pub fn new<S: AsRef<str>>(skill_ids: Option<&[S]>) -> Self {
        Self { ids: normalize_skill_ids(skill_ids).into_iter().collect() }
    }
    pub fn has(&self, skill_id: &str) -> bool {
        self.ids.contains(&skill_id.trim().to_lowercase())
    }
    pub fn set_enabled(&mut self, skill_id: &str, enabled: bool) {
        let id = skill_id.trim().to_lowercase();
        if !is_skill(&id) {
            return;
        }
        if enabled {
            self.ids.insert(id);
        } else {
            self.ids.remove(&id);
        }
    }
    pub fn replace<S: AsRef<str>>(&mut self, skill_ids: Option<&[S]>) {
        self.ids = normalize_skill_ids(skill_ids).into_iter().collect();
    }
    /// The enabled ids in registry order.
    pub fn ids(&self) -> Vec<String> {
        SKILLS.iter().filter(|s| self.ids.contains(s.id)).map(|s| s.id.to_string()).collect()
    }
    pub fn summary(&self) -> String {
        compact_skill_summary(Some(self.ids().as_slice()))
    }
}
