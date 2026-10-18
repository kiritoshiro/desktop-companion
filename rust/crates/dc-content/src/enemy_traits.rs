//! Port of `content/enemy_traits.py`: what each enemy kind is like.
//!
//! Body numbers (speed, health, bite, shell), a temperament (how far it
//! strays, how long it chases) and a few abilities. Pure data, read by the
//! mission brain and the squad tactics.

/// `Temperament` (enemy flavour): leash and notice radii, chase rules.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnemyTemperament {
    pub id: &'static str,
    pub label: &'static str,
    /// How far from its post it fights.
    pub leash: f64,
    /// How close a foe must come before it reacts.
    pub notice: f64,
    pub chase_forever: bool,
    /// Seconds out of reach before it goes home (0: never).
    pub give_up: f64,
}

const fn temper(
    id: &'static str,
    label: &'static str,
    leash: f64,
    notice: f64,
    chase_forever: bool,
    give_up: f64,
) -> EnemyTemperament {
    EnemyTemperament { id, label, leash, notice, chase_forever, give_up }
}

/// `TEMPERAMENTS`.
pub const TEMPERAMENTS: [EnemyTemperament; 7] = [
    temper("sentinel", "Sentinel: holds its post", 170.0, 200.0, false, 1.5),
    temper("territorial", "Territorial", 260.0, 260.0, false, 3.0),
    temper("stalker", "Stalker: follows you far", 520.0, 340.0, false, 8.0),
    temper("aggressive", "Aggressive: chases for ever", 10_000.0, 380.0, true, 0.0),
    temper("skirmisher", "Skirmisher: hit and run", 320.0, 300.0, false, 4.0),
    temper("ambusher", "Ambusher: waits, then strikes", 220.0, 150.0, false, 2.0),
    temper("sniper", "Sniper: shoots from range", 300.0, 320.0, false, 3.0),
];

pub fn temperament(id: &str) -> Option<&'static EnemyTemperament> {
    TEMPERAMENTS.iter().find(|t| t.id == id)
}

pub const ABILITIES: [&str; 9] =
    ["pounce", "web", "spit", "charge", "regen", "enrage", "hit_and_run", "rally", "thick_shell"];

/// What a squad role means to the tactics.
pub const SQUAD_ROLES: [&str; 5] = ["tank", "melee", "fast", "ranged", "support"];

/// `EnemyTraits`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnemyTraits {
    pub temperament: &'static str,
    pub abilities: &'static [&'static str],
    /// x the walking pace
    pub speed: f64,
    /// x health
    pub health: f64,
    /// x damage
    pub bite: f64,
    /// armour added
    pub shell: f64,
    pub squad_role: &'static str,
    /// "hunter", "weaver", "spitter", "guard"; `None`: by its role.
    pub style: Option<&'static str>,
    /// The one-line reason, for the map editor and tests.
    pub look: &'static str,
}

impl EnemyTraits {
    /// `traits.temper`.
    pub fn temper(&self) -> &'static EnemyTemperament {
        temperament(self.temperament).expect("traits name a known temperament")
    }
    pub fn has(&self, ability: &str) -> bool {
        self.abilities.contains(&ability)
    }
}

/// The dataclass defaults: speed/health/bite 1.0, shell 0.0, role "melee".
pub const DEFAULT_TRAITS: EnemyTraits = EnemyTraits {
    temperament: "sentinel",
    abilities: &[],
    speed: 1.0,
    health: 1.0,
    bite: 1.0,
    shell: 0.0,
    squad_role: "melee",
    style: None,
    look: "",
};

/// `TRAITS`, by enemy kind id.
pub const TRAITS: [(&str, EnemyTraits); 11] = [
    (
        "redback_raider",
        EnemyTraits {
            temperament: "aggressive",
            abilities: &["charge", "enrage"],
            speed: 1.30,
            health: 0.80,
            bite: 1.05,
            squad_role: "fast",
            style: Some("hunter"),
            look: "red and lean: fast, reckless, chases for ever",
            ..DEFAULT_TRAITS
        },
    ),
    (
        "ash_wolf",
        EnemyTraits {
            temperament: "stalker",
            abilities: &["pounce", "charge"],
            speed: 1.15,
            health: 1.0,
            bite: 1.0,
            squad_role: "melee",
            style: Some("hunter"),
            look: "grey wolf spider: follows far, then leaps",
            ..DEFAULT_TRAITS
        },
    ),
    (
        "jumping_skirmisher",
        EnemyTraits {
            temperament: "skirmisher",
            abilities: &["pounce", "hit_and_run"],
            speed: 1.25,
            health: 0.70,
            bite: 0.90,
            squad_role: "fast",
            style: Some("hunter"),
            look: "small and bright: darts in, springs back",
            ..DEFAULT_TRAITS
        },
    ),
    (
        "harvest_stalker",
        EnemyTraits {
            temperament: "sniper",
            abilities: &["web"],
            speed: 1.05,
            health: 0.80,
            bite: 0.85,
            squad_role: "ranged",
            style: Some("weaver"),
            look: "long legs: keeps its distance and shoots silk",
            ..DEFAULT_TRAITS
        },
    ),
    (
        "trapdoor_brute",
        EnemyTraits {
            temperament: "ambusher",
            abilities: &["thick_shell", "pounce"],
            speed: 0.75,
            health: 1.80,
            bite: 1.30,
            shell: 3.0,
            squad_role: "tank",
            style: Some("guard"),
            look: "thick-bodied: waits by its burrow, hits hard",
        },
    ),
    (
        "pale_cave",
        EnemyTraits {
            temperament: "territorial",
            abilities: &["spit", "regen"],
            speed: 0.95,
            health: 1.10,
            bite: 0.90,
            squad_role: "ranged",
            style: Some("spitter"),
            look: "pale cave dweller: spits acid, slowly heals",
            ..DEFAULT_TRAITS
        },
    ),
    (
        "crab_reaver",
        EnemyTraits {
            temperament: "sentinel",
            abilities: &["thick_shell", "charge"],
            speed: 0.85,
            health: 1.55,
            bite: 1.10,
            shell: 4.0,
            squad_role: "tank",
            style: Some("guard"),
            look: "crab-armoured: a wall in front of its post",
        },
    ),
    (
        "thorn_weaver",
        EnemyTraits {
            temperament: "sniper",
            abilities: &["web", "rally"],
            speed: 0.95,
            health: 0.95,
            bite: 0.90,
            squad_role: "support",
            style: Some("weaver"),
            look: "thorned orb-weaver: shoots silk, rallies the others",
            ..DEFAULT_TRAITS
        },
    ),
    (
        "ember_matriarch",
        EnemyTraits {
            temperament: "aggressive",
            abilities: &["charge", "enrage", "rally"],
            speed: 1.05,
            health: 1.0,
            bite: 1.10,
            squad_role: "tank",
            look: "ember matriarch: charges and rages when hurt",
            ..DEFAULT_TRAITS
        },
    ),
    (
        "ivory_regent",
        EnemyTraits {
            temperament: "territorial",
            abilities: &["thick_shell", "regen", "charge"],
            speed: 0.9,
            health: 1.15,
            shell: 5.0,
            squad_role: "tank",
            look: "ivory crab regent: armoured and self-mending",
            ..DEFAULT_TRAITS
        },
    ),
    (
        "thorn_crown",
        EnemyTraits {
            temperament: "stalker",
            abilities: &["web", "rally", "enrage"],
            speed: 1.0,
            health: 1.05,
            bite: 1.05,
            squad_role: "support",
            look: "thorn crown: silk, a war cry, and fury",
            ..DEFAULT_TRAITS
        },
    ),
];

/// `ROLE_TRAITS`: how a spider with no kind fights, by its role.
pub const ROLE_TRAITS: [(&str, EnemyTraits); 5] = [
    ("guard", EnemyTraits { temperament: "sentinel", abilities: &[], squad_role: "tank", ..DEFAULT_TRAITS }),
    ("hunter", EnemyTraits { temperament: "stalker", abilities: &["pounce"], squad_role: "melee", ..DEFAULT_TRAITS }),
    ("weaver", EnemyTraits { temperament: "sniper", abilities: &["web"], squad_role: "ranged", ..DEFAULT_TRAITS }),
    (
        "spitter",
        EnemyTraits { temperament: "territorial", abilities: &["spit"], squad_role: "ranged", ..DEFAULT_TRAITS },
    ),
    (
        "guardian",
        EnemyTraits {
            temperament: "territorial",
            abilities: &["charge", "enrage"],
            squad_role: "tank",
            ..DEFAULT_TRAITS
        },
    ),
];

pub fn kind_traits(kind_id: &str) -> Option<&'static EnemyTraits> {
    TRAITS.iter().find(|(k, _)| *k == kind_id).map(|(_, t)| t)
}

pub fn role_traits(role: &str) -> Option<&'static EnemyTraits> {
    ROLE_TRAITS.iter().find(|(k, _)| *k == role).map(|(_, t)| t)
}

/// `traits_for`: the kind's traits; without a kind, the role's (guard by default).
pub fn traits_for(kind_id: Option<&str>, role: &str) -> &'static EnemyTraits {
    if let Some(t) = kind_id.and_then(kind_traits) {
        return t;
    }
    role_traits(role).unwrap_or_else(|| role_traits("guard").expect("guard role exists"))
}
