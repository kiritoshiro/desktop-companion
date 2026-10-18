//! Port of `state/progression.py`: levels, XP, talents, armour, teams.
//!
//! Free of rendering and creature code so the rules work headlessly. XP is
//! *awarded* by the creature (`Creature.gain_experience`); this module owns
//! the curve, the catalogues and the saved per-spider state.

use dc_content::OrderedMap;
use dc_content::util::{JsonMap, py_int, py_str, py_truthy};
use dc_core::math::py_round;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Value, json};

pub const MAX_LEVEL: i64 = 30;
pub const RELATIONS: [&str; 3] = ["friend", "neutral", "foe"];

/// `TEAM_OPTIONS`: (label, id) for the preset-time and runtime team pickers.
pub const TEAM_OPTIONS: [(&str, &str); 5] = [
    ("Neutral / solo", "neutral"),
    ("Pack A", "pack_a"),
    ("Pack B", "pack_b"),
    ("Hunters", "hunters"),
    ("Rivals", "rivals"),
];

/// Teams hostile to every other named team unless a preset says otherwise.
pub const DEFAULT_HOSTILE_TEAMS: [&str; 1] = ["rivals"];

// ---------------------------------------------------------------------------
// Talents
// ---------------------------------------------------------------------------

/// One node of the talent tree (`AbilityNode`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AbilityNode {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub level_required: i64,
    pub cost: i64,
    pub prerequisites: &'static [&'static str],
    pub effects: &'static [(&'static str, f64)],
}

const fn node(
    id: &'static str,
    name: &'static str,
    description: &'static str,
    level_required: i64,
    prerequisites: &'static [&'static str],
    effects: &'static [(&'static str, f64)],
) -> AbilityNode {
    AbilityNode { id, name, description, level_required, cost: 1, prerequisites, effects }
}

/// `ABILITY_TREE`, in unlock order.
pub const ABILITY_TREE: [AbilityNode; 9] = [
    node(
        "vitality",
        "Vitality",
        "A sturdier body: you can take more hits before you fall.",
        2,
        &[],
        &[("max_hp", 18.0)],
    ),
    node("quick_step", "Quick step", "Quicker legs: you walk and turn faster.", 3, &[], &[("speed", 0.06)]),
    node(
        "silk_sense",
        "Silk sense",
        "A deeper silk reserve: more stamina for webs and pounces, and it refills faster.",
        4,
        &[],
        &[("max_energy", 12.0), ("energy_regen", 0.8)],
    ),
    node(
        "carapace_harden",
        "Hardened carapace",
        "A denser shell: every hit you take lands softer.",
        5,
        &["vitality"],
        &[("armor", 2.0)],
    ),
    node("power_strike", "Power strike", "Stronger chelicerae: every bite hits harder.", 6, &[], &[("damage", 2.0)]),
    node(
        "web_crafter",
        "Web crafter",
        "Practised spinning: more stamina, and it opens Silk tracking.",
        8,
        &["silk_sense"],
        &[("max_energy", 8.0)],
    ),
    node(
        "silk_tracking",
        "Silk tracking",
        "Thrown silk corrects itself in flight and follows a moving target.",
        9,
        &["web_crafter"],
        &[("web_homing", 1.0)],
    ),
    node(
        "long_stride",
        "Long stride",
        "Each step reaches further: you move faster still.",
        10,
        &["quick_step"],
        &[("speed", 0.08)],
    ),
    node(
        "apex_predator",
        "Apex predator",
        "The peak of the hunt: much harder bites and a deep stamina reserve.",
        15,
        &["power_strike", "carapace_harden"],
        &[("damage", 4.0), ("max_energy", 16.0)],
    ),
];

/// `ABILITY_BY_ID`.
pub fn ability_by_id(id: &str) -> Option<&'static AbilityNode> {
    ABILITY_TREE.iter().find(|n| n.id == id)
}

// ---------------------------------------------------------------------------
// Armour
// ---------------------------------------------------------------------------

/// One armour piece (`ArmorItem`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArmorItem {
    pub id: &'static str,
    pub name: &'static str,
    pub slot: &'static str,
    pub description: &'static str,
    pub armor: f64,
    pub max_hp: f64,
    pub max_energy: f64,
    pub damage: f64,
    pub speed: f64,
    pub size_min: f64,
    pub size_max: f64,
    /// Pieces of one set share an id; wearing all of them adds its bonus.
    pub set_id: &'static str,
    /// Quality, lowest first: see [`ARMOR_TIERS`].
    pub tier: &'static str,
}

impl ArmorItem {
    /// A stat by name (`getattr(item, key)` over [`ITEM_STATS`]).
    pub fn stat(&self, key: &str) -> f64 {
        match key {
            "armor" => self.armor,
            "max_hp" => self.max_hp,
            "max_energy" => self.max_energy,
            "damage" => self.damage,
            "speed" => self.speed,
            _ => 0.0,
        }
    }
}

const BASE_ITEM: ArmorItem = ArmorItem {
    id: "",
    name: "",
    slot: "",
    description: "",
    armor: 0.0,
    max_hp: 0.0,
    max_energy: 0.0,
    damage: 0.0,
    speed: 0.0,
    size_min: 0.55,
    size_max: 2.25,
    set_id: "",
    tier: "common",
};

/// Armour quality, lowest first.
pub const ARMOR_TIERS: [&str; 5] = ["common", "uncommon", "rare", "epic", "legendary"];

macro_rules! item {
    ($id:expr, $name:expr, $slot:expr, $desc:expr $(, $k:ident = $v:expr)* $(,)?) => {
        ArmorItem { id: $id, name: $name, slot: $slot, description: $desc, $($k: $v,)* ..BASE_ITEM }
    };
}

/// `ARMOR_CATALOG`.
pub const ARMOR_CATALOG: [ArmorItem; 30] = [
    item!(
        "silk_carapace",
        "Woven carapace",
        "carapace",
        "A flexible silk shell over the carapace.",
        armor = 2.0,
        max_hp = 8.0,
        speed = -0.015
    ),
    item!(
        "fluffy_mantle",
        "Soft setae mantle",
        "abdomen",
        "A warm, light mantle that cushions impacts.",
        armor = 1.0,
        max_hp = 12.0,
        max_energy = 4.0,
        speed = -0.02
    ),
    item!(
        "leg_guard_set",
        "Leg guard set",
        "legs",
        "Small guards fitted around the leg joints.",
        armor = 1.0,
        speed = 0.025
    ),
    item!(
        "pedipalp_cuffs",
        "Pedipalp cuffs",
        "pedipalps",
        "Tiny protective cuffs that leave the hands free.",
        armor = 0.5,
        max_energy = 5.0,
        damage = 0.5,
        speed = -0.01
    ),
    item!(
        "chelicerae_cap",
        "Fang cap",
        "head",
        "A narrow guard around the small head and chelicerae.",
        armor = 1.0,
        damage = 2.0,
        speed = -0.008
    ),
    item!(
        "warden_crest",
        "Warden ocular crest",
        "head",
        "A ridged visor over the eye mound; the eight eyes stay clear and the chelicerae get capped tips.",
        armor = 1.0,
        damage = 2.5,
        set_id = "warden",
        tier = "rare"
    ),
    item!(
        "warden_carapace",
        "Warden carapace plate",
        "carapace",
        "A shield moulded to the prosoma, the fovea left open; the orange rim still shows around it.",
        armor = 2.5,
        max_hp = 12.0,
        speed = -0.02,
        set_id = "warden",
        tier = "rare"
    ),
    item!(
        "warden_tergites",
        "Warden tergite bands",
        "abdomen",
        "Overlapping bands over the soft abdomen, the most vulnerable part; they flex as it breathes and leave the spinnerets free.",
        armor = 1.5,
        max_hp = 20.0,
        speed = -0.02,
        set_id = "warden",
        tier = "rare"
    ),
    item!(
        "warden_greaves",
        "Warden leg plates",
        "legs",
        "Plates down all eight legs, femur to metatarsus, with a cop over each knee; the clawed tarsi stay bare to grip.",
        armor = 1.0,
        max_hp = 6.0,
        speed = -0.01,
        set_id = "warden",
        tier = "rare"
    ),
    item!(
        "warden_bracers",
        "Warden palp bracers",
        "pedipalps",
        "Bracers on both pedipalps; the palp tips stay free to feel and to hold prey.",
        armor = 0.5,
        damage = 1.0,
        max_energy = 6.0,
        set_id = "warden",
        tier = "rare"
    ),
    item!(
        "forager_hood",
        "Forager elytra hood",
        "head",
        "A beetle's wing case bent over the eye mound and lashed with silk.",
        armor = 0.5,
        damage = 1.0,
        set_id = "forager",
        tier = "uncommon"
    ),
    item!(
        "forager_shell",
        "Forager shell",
        "carapace",
        "Two elytra halves over the carapace, still green-bronze where the light hits.",
        armor = 1.2,
        max_hp = 8.0,
        set_id = "forager",
        tier = "uncommon"
    ),
    item!(
        "forager_mantle",
        "Forager mantle",
        "abdomen",
        "Wing-case scales sewn on silk over the abdomen.",
        armor = 0.8,
        max_hp = 12.0,
        speed = -0.01,
        set_id = "forager",
        tier = "uncommon"
    ),
    item!(
        "forager_leggings",
        "Forager leggings",
        "legs",
        "Light chitin splints down each leg; barely heavier than bare legs.",
        armor = 0.6,
        speed = 0.02,
        set_id = "forager",
        tier = "uncommon"
    ),
    item!(
        "forager_wraps",
        "Forager palp wraps",
        "pedipalps",
        "Chitin slivers wrapped round the palps.",
        armor = 0.3,
        max_energy = 4.0,
        set_id = "forager",
        tier = "uncommon"
    ),
    item!(
        "frost_diadem",
        "Frostglass diadem",
        "head",
        "A ring of ice-clear crystal round the eye mound; the eyes look out through it.",
        armor = 1.0,
        damage = 2.0,
        speed = 0.01,
        set_id = "frost",
        tier = "epic"
    ),
    item!(
        "frost_aegis",
        "Frostglass aegis",
        "carapace",
        "A faceted crystal shield over the carapace, light as frost.",
        armor = 2.0,
        max_hp = 12.0,
        speed = 0.01,
        set_id = "frost",
        tier = "epic"
    ),
    item!(
        "frost_scales",
        "Frostglass scales",
        "abdomen",
        "Overlapping crystal scales that ring faintly as the spider walks.",
        armor = 1.5,
        max_hp = 18.0,
        set_id = "frost",
        tier = "epic"
    ),
    item!(
        "frost_shards",
        "Frostglass leg shards",
        "legs",
        "Crystal sheaths down every leg; they shed the air like ice on a blade.",
        armor = 1.2,
        speed = 0.04,
        set_id = "frost",
        tier = "epic"
    ),
    item!(
        "frost_talons",
        "Frostglass talons",
        "pedipalps",
        "Crystal points over the palps, cold enough to numb what they touch.",
        armor = 0.5,
        damage = 1.5,
        max_energy = 8.0,
        set_id = "frost",
        tier = "epic"
    ),
    item!(
        "brood_crown",
        "Brood obsidian crown",
        "head",
        "A spiked crown of black glass; the runes on it glow when the spider bites.",
        armor = 1.2,
        damage = 4.0,
        set_id = "brood",
        tier = "epic"
    ),
    item!(
        "brood_bulwark",
        "Brood obsidian bulwark",
        "carapace",
        "A heavy plate of black glass over the carapace, veined with violet runes.",
        armor = 2.8,
        max_hp = 16.0,
        speed = -0.02,
        set_id = "brood",
        tier = "epic"
    ),
    item!(
        "brood_plates",
        "Brood obsidian plates",
        "abdomen",
        "Rune-cut obsidian bands over the abdomen.",
        armor = 2.0,
        max_hp = 24.0,
        speed = -0.02,
        set_id = "brood",
        tier = "epic"
    ),
    item!(
        "brood_spurs",
        "Brood obsidian leg spurs",
        "legs",
        "Black-glass plates down each leg, spurred at the knee.",
        armor = 1.2,
        damage = 1.0,
        speed = -0.01,
        set_id = "brood",
        tier = "epic"
    ),
    item!(
        "brood_claws",
        "Brood obsidian claws",
        "pedipalps",
        "Glass-edged palp claws that cut as they grip.",
        armor = 0.6,
        damage = 2.0,
        set_id = "brood",
        tier = "epic"
    ),
    item!(
        "sun_crown",
        "Sunforged crown",
        "head",
        "A gold crown over the eye mound set with an ember-red stone.",
        armor = 1.5,
        damage = 3.5,
        max_energy = 6.0,
        set_id = "sun",
        tier = "legendary"
    ),
    item!(
        "sun_aegis",
        "Sunforged aegis",
        "carapace",
        "A gold shield over the carapace with ember veins that never cool.",
        armor = 3.0,
        max_hp = 20.0,
        set_id = "sun",
        tier = "legendary"
    ),
    item!(
        "sun_scales",
        "Sunforged scales",
        "abdomen",
        "Gold bands over the abdomen, each lit from within.",
        armor = 2.2,
        max_hp = 30.0,
        set_id = "sun",
        tier = "legendary"
    ),
    item!(
        "sun_legs",
        "Sunforged leg plates",
        "legs",
        "Gold plates down every leg; they hold their warmth, and the legs stay quick.",
        armor = 1.5,
        max_hp = 8.0,
        speed = 0.02,
        set_id = "sun",
        tier = "legendary"
    ),
    item!(
        "sun_gauntlets",
        "Sunforged palp gauntlets",
        "pedipalps",
        "Gold gauntlets over the palps, ember-hot at the tips.",
        armor = 0.8,
        damage = 2.5,
        max_energy = 10.0,
        set_id = "sun",
        tier = "legendary"
    ),
];

/// `ARMOR_BY_ID`.
pub fn armor_by_id(id: &str) -> Option<&'static ArmorItem> {
    ARMOR_CATALOG.iter().find(|i| i.id == id)
}

/// Armour levels: a duplicate stacked onto a piece raises it one level.
pub const MAX_ITEM_LEVEL: i64 = 5;
/// Each level adds 20% of the piece's base stats; level 5 is 1.8x.
pub const LEVEL_STEP: f64 = 0.2;
pub const ITEM_STATS: [&str; 5] = ["armor", "max_hp", "max_energy", "damage", "speed"];

/// `level_multiplier`.
pub fn level_multiplier(level: i64) -> f64 {
    1.0 + LEVEL_STEP * (level.clamp(1, MAX_ITEM_LEVEL) - 1) as f64
}

/// `item_at_level`: the piece with its stats scaled to `level`.
pub fn item_at_level(item: &ArmorItem, level: i64) -> ArmorItem {
    let k = level_multiplier(level);
    if k == 1.0 {
        return *item;
    }
    ArmorItem {
        armor: item.armor * k,
        max_hp: item.max_hp * k,
        max_energy: item.max_energy * k,
        damage: item.damage * k,
        speed: item.speed * k,
        ..*item
    }
}

/// A full armour set and its bonus (`ArmorSet`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArmorSet {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub effects: &'static [(&'static str, f64)],
}

impl ArmorSet {
    /// The catalogue ids of this set's pieces.
    pub fn pieces(&self) -> Vec<&'static str> {
        ARMOR_CATALOG.iter().filter(|i| i.set_id == self.id).map(|i| i.id).collect()
    }
}

/// `ARMOR_SETS`, in the Python dict's order.
pub const ARMOR_SETS: [ArmorSet; 5] = [
    ArmorSet {
        id: "forager",
        name: "Forager set",
        description: "Wear all five pieces: light enough to hunt all day.",
        effects: &[("max_energy", 8.0), ("energy_regen", 0.5)],
    },
    ArmorSet {
        id: "warden",
        name: "Warden set",
        description: "Wear all five pieces: the plates brace one another.",
        effects: &[("max_hp", 10.0), ("armor", 0.5), ("energy_regen", 1.5)],
    },
    ArmorSet {
        id: "frost",
        name: "Frostglass set",
        description: "Wear all five pieces: the crystal hums and the spider flies over the ground.",
        effects: &[("speed", 0.06), ("max_energy", 10.0), ("energy_regen", 2.0)],
    },
    ArmorSet {
        id: "brood",
        name: "Obsidian Brood set",
        description: "Wear all five pieces: every rune wakes at once.",
        effects: &[("damage", 3.0), ("max_hp", 15.0)],
    },
    ArmorSet {
        id: "sun",
        name: "Sunforged regalia",
        description: "Wear all five pieces: the embers join into one fire.",
        effects: &[("max_hp", 20.0), ("armor", 1.0), ("damage", 2.0), ("energy_regen", 2.0)],
    },
];

pub fn armor_set(id: &str) -> Option<&'static ArmorSet> {
    ARMOR_SETS.iter().find(|s| s.id == id)
}

/// `set_pieces_worn`.
pub fn set_pieces_worn(state: &ProgressionState, set_id: &str) -> usize {
    let Some(set) = armor_set(set_id) else { return 0 };
    let worn: Vec<&str> = state.equipped.values().map(String::as_str).collect();
    set.pieces().iter().filter(|p| worn.contains(p)).count()
}

/// `set_bonus_effects`: the bonuses of every set worn complete.
pub fn set_bonus_effects(state: &ProgressionState) -> OrderedMap<f64> {
    let mut totals = OrderedMap::new();
    for set in ARMOR_SETS.iter() {
        let pieces = set.pieces();
        if !pieces.is_empty() && set_pieces_worn(state, set.id) == pieces.len() {
            for (key, value) in set.effects {
                *totals.entry_or_insert_with(key, || 0.0) += value;
            }
        }
    }
    totals
}

// ---------------------------------------------------------------------------
// Curve
// ---------------------------------------------------------------------------

/// `xp_to_next_level`: XP needed to advance from `level` to `level + 1`.
pub fn xp_to_next_level(level: i64) -> i64 {
    let level = level.clamp(1, MAX_LEVEL) as f64;
    py_round(92.0 + level * 21.0 + level * level * 2.1) as i64
}

/// `growth_multipliers`: bounded (size, speed) multipliers for a level.
pub fn growth_multipliers(level: i64) -> (f64, f64) {
    let step = (level.clamp(1, MAX_LEVEL) - 1) as f64;
    (1.0 + step * 0.012, 1.0 + step * 0.018)
}

// ---------------------------------------------------------------------------
// Saved state
// ---------------------------------------------------------------------------

/// One spider's saved progression (`ProgressionState`).
#[derive(Clone, Debug, PartialEq)]
pub struct ProgressionState {
    pub level: i64,
    pub xp: i64,
    pub total_xp: i64,
    pub skill_points: i64,
    pub unlocked_abilities: Vec<String>,
    pub inventory: Vec<String>,
    /// slot -> item id, in equip order.
    pub equipped: OrderedMap<String>,
    pub team_id: String,
    /// other spider's progression id -> relation.
    pub relation_overrides: OrderedMap<String>,
    pub pin_level: bool,
    pub pin_health: bool,
    /// Level of each owned armour piece (1..MAX_ITEM_LEVEL); missing means 1.
    pub item_levels: OrderedMap<i64>,
}

impl Default for ProgressionState {
    fn default() -> Self {
        Self {
            level: 1,
            xp: 0,
            total_xp: 0,
            skill_points: 0,
            unlocked_abilities: Vec::new(),
            inventory: vec!["silk_carapace".into(), "leg_guard_set".into()],
            equipped: OrderedMap::new(),
            team_id: "neutral".into(),
            relation_overrides: OrderedMap::new(),
            pin_level: false,
            pin_health: false,
            item_levels: OrderedMap::new(),
        }
    }
}

impl ProgressionState {
    /// `ProgressionState.from_dict`: forgiving, never fails.
    pub fn from_value(value: &Value) -> Self {
        let Some(value) = value.as_object() else { return Self::default() };
        let mut state = Self::default();
        let get = |k: &str| value.get(k);
        state.level = match get("level") {
            None => 1,
            Some(v) => py_int(v).map(|l| l.clamp(1, MAX_LEVEL)).unwrap_or(1),
        };
        let counters = (|| -> Option<(i64, i64, i64)> {
            let xp = match get("xp") {
                None => 0,
                Some(v) => py_int(v)?,
            }
            .max(0);
            let total = match get("total_xp") {
                None => xp,
                Some(v) => py_int(v)?,
            };
            let total = xp.max(total);
            let points = match get("skill_points") {
                None => 0,
                Some(v) => py_int(v)?,
            }
            .max(0);
            Some((xp, total, points))
        })();
        (state.xp, state.total_xp, state.skill_points) = counters.unwrap_or((0, 0, 0));
        if let Some(Value::Array(raw)) = get("unlocked_abilities") {
            state.unlocked_abilities =
                raw.iter().filter_map(Value::as_str).filter(|a| ability_by_id(a).is_some()).map(String::from).collect();
        }
        if let Some(Value::Array(raw)) = get("inventory") {
            state.inventory =
                raw.iter().filter_map(Value::as_str).filter(|i| armor_by_id(i).is_some()).map(String::from).collect();
        }
        if let Some(Value::Object(raw)) = get("equipped") {
            for (slot, item) in raw {
                let item = py_str(item);
                if armor_by_id(&item).is_some() && state.inventory.contains(&item) {
                    state.equipped.insert(slot.clone(), item);
                }
            }
        }
        state.team_id = normalize_team_id_value(get("team_id").unwrap_or(&json!("neutral")));
        if let Some(Value::Object(raw)) = get("relation_overrides") {
            for (key, rel) in raw {
                let rel = py_str(rel).to_lowercase();
                if RELATIONS.contains(&rel.as_str()) {
                    state.relation_overrides.insert(key.clone(), rel);
                }
            }
        }
        state.pin_level = get("pin_level").is_some_and(py_truthy);
        state.pin_health = get("pin_health").is_some_and(py_truthy);
        if let Some(Value::Object(raw)) = get("item_levels") {
            for (item_id, level) in raw {
                let Some(level) = py_int(level) else { continue };
                if armor_by_id(item_id).is_some() {
                    state.item_levels.insert(item_id.clone(), level.clamp(1, MAX_ITEM_LEVEL));
                }
            }
        }
        state
    }

    /// `ProgressionState.to_dict`.
    pub fn to_value(&self) -> Value {
        let mut m = JsonMap::new();
        m.insert("level".into(), json!(self.level));
        m.insert("xp".into(), json!(self.xp));
        m.insert("total_xp".into(), json!(self.total_xp));
        m.insert("skill_points".into(), json!(self.skill_points));
        m.insert("unlocked_abilities".into(), json!(self.unlocked_abilities));
        m.insert("inventory".into(), json!(self.inventory));
        m.insert("equipped".into(), serde_json::to_value(&self.equipped).unwrap());
        m.insert("team_id".into(), json!(self.team_id));
        m.insert("relation_overrides".into(), serde_json::to_value(&self.relation_overrides).unwrap());
        m.insert("pin_level".into(), json!(self.pin_level));
        m.insert("pin_health".into(), json!(self.pin_health));
        m.insert("item_levels".into(), serde_json::to_value(&self.item_levels).unwrap());
        Value::Object(m)
    }

    /// `add_item`: false for an unknown or already-owned piece.
    pub fn add_item(&mut self, item_id: &str) -> bool {
        if armor_by_id(item_id).is_none() || self.inventory.iter().any(|i| i == item_id) {
            return false;
        }
        self.inventory.push(item_id.to_string());
        true
    }

    /// `equip`: wear an owned piece in its slot.
    pub fn equip(&mut self, item_id: &str) -> bool {
        let Some(item) = armor_by_id(item_id) else { return false };
        if !self.inventory.iter().any(|i| i == item_id) {
            return false;
        }
        self.equipped.insert(item.slot, item_id.to_string());
        true
    }

    /// `unequip`.
    pub fn unequip(&mut self, slot: &str) -> bool {
        self.equipped.remove(slot).is_some()
    }

    /// `can_unlock`: level, points and prerequisites all satisfied.
    pub fn can_unlock(&self, ability_id: &str) -> bool {
        let Some(node) = ability_by_id(ability_id) else { return false };
        if self.unlocked_abilities.iter().any(|a| a == ability_id) {
            return false;
        }
        self.level >= node.level_required
            && self.skill_points >= node.cost
            && node.prerequisites.iter().all(|req| self.unlocked_abilities.iter().any(|a| a == req))
    }

    /// The level of an owned piece (`item_levels.get(id, 1)`).
    pub fn item_level(&self, item_id: &str) -> i64 {
        self.item_levels.get(item_id).copied().unwrap_or(1)
    }
}

impl Serialize for ProgressionState {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_value().serialize(s)
    }
}

impl<'de> Deserialize<'de> for ProgressionState {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::from_value(&Value::deserialize(d)?))
    }
}

/// `equipped_items`: worn pieces, each scaled to its level.
pub fn equipped_items(state: &ProgressionState) -> Vec<ArmorItem> {
    state
        .equipped
        .values()
        .filter_map(|id| armor_by_id(id).map(|item| item_at_level(item, state.item_level(id))))
        .collect()
}

// ---------------------------------------------------------------------------
// Teams and relations
// ---------------------------------------------------------------------------

/// `normalize_team_id` for text: case-folded, whitespace runs to `_`, 32 chars.
pub fn normalize_team_id(value: &str) -> String {
    let source = if value.is_empty() { "neutral" } else { value };
    let text = source.trim().to_lowercase().split_whitespace().collect::<Vec<_>>().join("_");
    let text: String = text.chars().take(32).collect();
    if text.is_empty() { "neutral".into() } else { text }
}

/// `normalize_team_id` for any JSON value (`str(value or "neutral")`).
pub fn normalize_team_id_value(value: &Value) -> String {
    if !py_truthy(value) {
        return "neutral".into();
    }
    normalize_team_id(&py_str(value))
}

/// Declared stances: `{team: {team: relation}}`, mirrored both ways.
pub type Stances = OrderedMap<OrderedMap<String>>;

/// `normalize_team_stances`: clean a preset's `team_relations` block.
pub fn normalize_team_stances(raw: Option<&Value>) -> Stances {
    let mut stances = Stances::new();
    let Some(Value::Object(raw)) = raw else { return stances };
    for (left, row) in raw {
        let Some(row) = row.as_object() else { continue };
        let left_id = normalize_team_id(left);
        for (right, relation) in row {
            let rel = if py_truthy(relation) { py_str(relation) } else { String::new() }.trim().to_lowercase();
            if !RELATIONS.contains(&rel.as_str()) {
                continue;
            }
            let right_id = normalize_team_id(right);
            stances.entry_or_insert_with(&left_id, OrderedMap::new).insert(right_id.clone(), rel.clone());
            stances.entry_or_insert_with(&right_id, OrderedMap::new).insert(left_id.clone(), rel);
        }
    }
    stances
}

/// `normalize_team_stances` for an already-typed block.
pub fn normalize_team_stances_typed(raw: &OrderedMap<OrderedMap<String>>) -> Stances {
    normalize_team_stances(Some(&serde_json::to_value(raw).expect("stances serialise")))
}

/// `team_stance`: the relation declared between two teams, or `None`.
pub fn team_stance(left_team: &str, right_team: &str, stances: Option<&Stances>) -> Option<String> {
    let left = normalize_team_id(left_team);
    let right = normalize_team_id(right_team);
    if left == "neutral" || right == "neutral" {
        return None;
    }
    if let Some(stances) = stances.filter(|s| !s.is_empty())
        && let Some(rel) = stances.get(&left).and_then(|row| row.get(&right))
    {
        return Some(rel.clone());
    }
    if left == right {
        return Some("friend".into());
    }
    if DEFAULT_HOSTILE_TEAMS.contains(&left.as_str()) || DEFAULT_HOSTILE_TEAMS.contains(&right.as_str()) {
        return Some("foe".into());
    }
    None
}

/// `relation_between`: a per-pair override, then the team stance, else neutral.
pub fn relation_between(
    left: &ProgressionState,
    right: &ProgressionState,
    right_key: Option<&str>,
    stances: Option<&Stances>,
) -> String {
    if let Some(key) = right_key.filter(|k| !k.is_empty())
        && let Some(rel) = left.relation_overrides.get(key)
    {
        return rel.clone();
    }
    team_stance(&left.team_id, &right.team_id, stances).unwrap_or_else(|| "neutral".into())
}
