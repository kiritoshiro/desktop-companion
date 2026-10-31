//! Sample data with the Python values, for the tests, the example runner,
//! and as the stand-in catalogue until dc-content and dc-state are wired in.
//!
//! The skills registry, jobs, ability tree, armour catalogue and sets below
//! are copies of content/skills.py, world/jobs.py and state/progression.py
//! for tests; `bridge`'s tests check they equal the real crates' tables.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::model::*;

fn rgbmap(pairs: &[(&str, Rgb)]) -> BTreeMap<String, Rgb> {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect()
}

/// `content/skills.SKILLS` with `SHORT_LABELS`.
pub fn skills() -> Vec<SkillInfo> {
    let rows: &[(&str, &str, &str, &str, &str)] = &[
        (
            "approach",
            "Approach / stalk",
            "Interaction",
            "approach",
            "Allows cautious movement toward the cursor before chasing or inspecting.",
        ),
        ("wander", "Wander / roam", "Movement", "wander", "Allows idle roaming between cursor interactions."),
        (
            "drift",
            "Drift / slide",
            "Ability",
            "drift",
            "Allows momentum-based drift sliding: builds speed first, breaks traction, leans/counter-steers through wide circle or corner skids, and keeps sliding after throws.",
        ),
        ("jump", "Jump / hop", "Movement", "jump", "Allows small hops and the airborne part of pounces."),
        ("roll", "Roll / tumble", "Movement", "roll", "Allows happy curl-up tumbles and rolling flourishes."),
        ("chase", "Chase", "Interaction", "chase", "Allows burst chasing of the cursor or a playmate."),
        (
            "observe",
            "Observe / orbit",
            "Interaction",
            "observe",
            "Allows observer spiders to circle and watch a cursor or another spider.",
        ),
        (
            "run_away",
            "Run away / retreat",
            "Survival",
            "run away",
            "Allows threat-triggered retreats away from fast cursor movement.",
        ),
        (
            "prepare_jump_attack",
            "Prepare jump attack",
            "Interaction",
            "jump attack",
            "Allows the crouch, rangefinder feeler pose, and pounce wind-up.",
        ),
        ("inspect", "Inspect", "Interaction", "inspect", "Allows curious close inspection and studying behaviour."),
        ("cuddle", "Cuddle", "Social", "cuddle", "Allows affectionate approach, snuggling, and boop behaviour."),
        (
            "social_play",
            "Social play",
            "Social",
            "play",
            "Allows direct play bouts between spiders when social play is enabled.",
        ),
        ("zoomies", "Zoomies", "Movement", "zoomies", "Allows short playful sprint bursts."),
        (
            "weave_web",
            "Weave web",
            "Ability",
            "weave",
            "Allows building silk webs thread by thread in screen corners, and finishing an abandoned unfinished web even if another spider began it.",
        ),
        (
            "web_walk",
            "Walk on webs",
            "Ability",
            "web walk",
            "Allows walking onto a finished web and plucking it to test its bounce.",
        ),
        (
            "shoot_web",
            "Shoot trapping web",
            "Ability",
            "web trap",
            "Allows aiming and firing a glob of sticky silk that pins its target in place, whether the mouse pointer or a fly. Wiggle a trapped pointer to break it free.",
        ),
        (
            "wall_web",
            "Web-shove to wall",
            "Ability",
            "wall pin",
            "Allows firing a web that shoves its target to the nearest wall and pins it there, whether the mouse pointer or a fly. Wiggle a trapped pointer to peel it off.",
        ),
    ];
    rows.iter()
        .map(|(id, name, cat, short, desc)| SkillInfo {
            id: (*id).into(),
            display_name: (*name).into(),
            category: (*cat).into(),
            short_label: (*short).into(),
            description: (*desc).into(),
        })
        .collect()
}

/// `COMMON_SKILL_IDS`.
pub fn common_skill_ids() -> Vec<String> {
    [
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
        "web_walk",
    ]
    .iter()
    .map(|s| (*s).into())
    .collect()
}

/// `world/jobs.JOB_DEFINITIONS`.
pub fn jobs() -> Vec<JobInfo> {
    let rows: &[(&str, &str, &[&str], &str)] = &[
        ("none", "No job", &[], "Acts only from temperament and selected abilities."),
        ("hunter", "Hunter", &[], "Tracks nearby prey and threats with focused pursuit."),
        ("builder", "Builder", &[], "Establishes and upgrades a shared colony base."),
        ("guard", "Guard", &["shoot_web"], "Patrols a friendly base, and pins declared foes with silk."),
        ("scout", "Scout", &[], "Ranges beyond the base and reports points of interest."),
        (
            "webber",
            "Web tender",
            &["weave_web", "shoot_web"],
            "Maintains the colony's silk structures, and throws silk.",
        ),
    ];
    rows.iter()
        .map(|(id, name, abilities, desc)| JobInfo {
            id: (*id).into(),
            display_name: (*name).into(),
            description: (*desc).into(),
            ability_ids: abilities.iter().map(|a| (*a).into()).collect(),
        })
        .collect()
}

/// The six compact temperaments.
pub fn personalities() -> BTreeMap<String, PersonalityInfo> {
    let traits = |v: [f64; 6]| -> Vec<(String, f64)> {
        ["energy", "curiosity", "boldness", "sociability", "patience", "caution"]
            .iter()
            .zip(v)
            .map(|(k, x)| ((*k).to_owned(), x))
            .collect()
    };
    let rows: [(&str, &str, &str, [f64; 6]); 6] = [
        ("balanced", "Balanced", "A steady mix of roaming, curiosity, and caution.", [5.0, 5.0, 5.0, 5.0, 5.0, 5.0]),
        ("playful", "Playful", "", [8.0, 6.0, 6.0, 8.0, 3.0, 3.0]),
        ("curious", "Curious", "", [6.0, 10.0, 5.0, 5.0, 7.0, 4.0]),
        ("bold", "Bold", "", [8.0, 6.0, 10.0, 4.0, 2.0, 1.0]),
        (
            "cautious",
            "Cautious",
            "Patient and observant, with a strong threat response.",
            [4.0, 5.0, 2.0, 3.0, 8.0, 10.0],
        ),
        ("social", "Social", "Friendly and connection-seeking without being frantic.", [5.0, 6.0, 4.0, 10.0, 7.0, 4.0]),
    ];
    rows.iter()
        .map(|(id, name, desc, t)| {
            (
                (*id).to_owned(),
                PersonalityInfo {
                    id: (*id).into(),
                    display_name: (*name).into(),
                    canonical: true,
                    description: (*desc).into(),
                    temperament: traits(*t),
                    default_skills: common_skill_ids(),
                },
            )
        })
        .collect()
}

/// One sample model: id, name, body plan, colours, enemy, when it was added.
type ModelRow = (&'static str, &'static str, &'static str, BTreeMap<String, Rgb>, bool, f64);

/// A handful of the shipped models.
pub fn models() -> BTreeMap<String, ModelInfo> {
    let rows: Vec<ModelRow> = vec![
        (
            "tarantula",
            "Tarantula",
            "tarantula",
            rgbmap(&[
                ("body", [24, 20, 20]),
                ("legs", [30, 26, 26]),
                ("leg_band", [226, 110, 40]),
                ("leg_dark", [18, 16, 16]),
                ("leg_tip", [20, 18, 18]),
                ("highlight", [150, 138, 126]),
                ("eyes", [70, 60, 54]),
                ("rim", [170, 108, 62]),
            ]),
            false,
            9.0,
        ),
        (
            "plush_snow_hybrid_2",
            "Snowpuff-2",
            "tarantula",
            rgbmap(&[
                ("body", [236, 232, 226]),
                ("legs", [212, 204, 194]),
                ("highlight", [255, 255, 252]),
                ("eyes", [70, 66, 74]),
            ]),
            false,
            8.0,
        ),
        (
            "spider",
            "Spider",
            "bug",
            rgbmap(&[
                ("body", [34, 30, 25]),
                ("legs", [20, 17, 14]),
                ("highlight", [78, 66, 52]),
                ("eyes", [170, 40, 30]),
            ]),
            false,
            1.0,
        ),
        (
            "spider_fluffy_snow",
            "Fluffy Snow Spider",
            "bug",
            rgbmap(&[
                ("body", [215, 208, 190]),
                ("legs", [152, 146, 132]),
                ("highlight", [245, 240, 222]),
                ("eyes", [42, 50, 58]),
            ]),
            false,
            2.0,
        ),
        (
            "coppercurl_soft_tarantula",
            "Coppercurl Soft Tarantula",
            "bug",
            rgbmap(&[
                ("body", [158, 94, 46]),
                ("legs", [126, 76, 38]),
                ("highlight", [238, 176, 92]),
                ("eyes", [68, 44, 26]),
            ]),
            false,
            3.0,
        ),
        (
            "legendary_seraph_cloudweaver",
            "Seraph Cloudweaver",
            "bug",
            rgbmap(&[
                ("body", [238, 202, 128]),
                ("legs", [236, 214, 171]),
                ("highlight", [255, 244, 204]),
                ("eyes", [255, 252, 235]),
            ]),
            false,
            4.0,
        ),
        (
            "velvet_cloud",
            "Velvet Cloud Tarantula",
            "segmented",
            rgbmap(&[
                ("body", [62, 48, 62]),
                ("legs", [76, 59, 79]),
                ("leg_band", [170, 111, 151]),
                ("leg_tip", [36, 28, 43]),
                ("highlight", [210, 155, 196]),
                ("eyes", [248, 218, 255]),
            ]),
            false,
            5.0,
        ),
        (
            "giant_copper",
            "Giant Copper Tarantula",
            "segmented",
            rgbmap(&[
                ("body", [48, 28, 24]),
                ("legs", [92, 48, 34]),
                ("leg_band", [221, 132, 55]),
                ("leg_tip", [31, 18, 18]),
                ("highlight", [239, 174, 92]),
                ("eyes", [255, 224, 151]),
            ]),
            false,
            6.0,
        ),
        (
            "chosen_one",
            "Chosen One",
            "jumper",
            rgbmap(&[
                ("body", [28, 24, 43]),
                ("legs", [46, 37, 68]),
                ("highlight", [225, 181, 71]),
                ("eyes", [255, 239, 161]),
            ]),
            false,
            7.0,
        ),
        (
            "silverwool_little_jumper",
            "Silverwool Little Jumper",
            "jumper",
            rgbmap(&[
                ("body", [128, 134, 144]),
                ("legs", [108, 114, 124]),
                ("highlight", [230, 234, 242]),
                ("eyes", [252, 252, 255]),
            ]),
            false,
            6.5,
        ),
        ("enemy_crab_reaver", "Crab reaver", "enemy_crab", rgbmap(&[("body", [24, 20, 20])]), true, 10.0),
        ("enemy_ash_wolf", "Ash wolf spider", "tarantula", rgbmap(&[("body", [24, 20, 20])]), true, 10.0),
    ];
    rows.into_iter()
        .map(|(id, name, plan, colors, enemy, added)| {
            (
                id.to_owned(),
                ModelInfo { id: id.into(), display_name: name.into(), body_plan: plan.into(), colors, enemy, added },
            )
        })
        .collect()
}

/// The settings window's catalogue with sample models.
pub fn settings_catalog() -> SettingsCatalog {
    SettingsCatalog {
        models: models(),
        personalities: personalities(),
        compact_temperaments: ["balanced", "playful", "curious", "bold", "cautious", "social"]
            .iter()
            .map(|s| (*s).into())
            .collect(),
        skills: skills(),
        common_skill_ids: common_skill_ids(),
        jobs: jobs(),
        palettes: named_palettes(),
        body_plans: ["bug", "segmented", "tarantula", "jumper", "enemy_crab", "enemy_orb"]
            .iter()
            .map(|s| (*s).into())
            .collect(),
        enemy_body_plans: vec!["enemy_crab".into(), "enemy_orb".into()],
        label_switch_defaults: [
            ("always_show_names", true),
            ("always_show_levels", true),
            ("always_show_health", true),
            ("always_show_xp", false),
            ("always_show_stamina", true),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), *v))
        .collect(),
        warnings: Vec::new(),
    }
}

/// The shipped `presets/default.json`, as the window sees it.
pub fn default_preset() -> Value {
    json!({
        "name": "Default",
        "slots": [
            {"model": "tarantula", "personality": "bold", "count": 1, "count_random": false, "slot_id": "slot-0",
             "team": "neutral", "job": "scout", "abilities": ["web_walk"],
             "colors": {"highlight": [185, 110, 112], "leg_dark": [2, 29, 9], "leg_tip": [0, 255, 255]}},
            {"model": "plush_snow_hybrid_2", "personality": "curious", "count": 1, "count_random": false,
             "slot_id": "slot-1", "team": "neutral", "job": "webber", "abilities": ["web_walk"]},
            {"model": "spider_fluffy_snow", "personality": "balanced", "count": 1, "count_random": false,
             "slot_id": "slot-3fe031e00ed1", "team": "neutral", "job": "hunter"},
            {"model": "legendary_seraph_cloudweaver", "personality": "balanced", "count": 1, "count_random": false,
             "slot_id": "slot-f8390820264f", "team": "neutral", "job": "guard"},
            {"model": "coppercurl_soft_tarantula", "personality": "balanced", "count": 1, "count_random": false,
             "slot_id": "slot-179e8d8c0663", "team": "neutral", "job": "builder"}
        ],
        "settings": {
            "size_scale": 1.0, "interferable": true, "mood_mode": "auto", "social_play": true, "gait_style": "skitter",
            "flies": {"enabled": false, "min_interval": 4.0, "max_interval": 9.0, "max_flies": 6, "spawner": false},
            "mouse_capture": true
        }
    })
}

/// A preset with two named teams and a declared stance (presets/colony.json-like).
pub fn colony_preset() -> Value {
    json!({
        "name": "Colony",
        "slots": [
            {"model": "tarantula", "personality": "balanced", "count": 3, "slot_id": "slot-a", "team": "hunters", "job": "guard"},
            {"model": "spider", "personality": "bold", "count": 2, "slot_id": "slot-b", "team": "rivals", "job": "hunter"},
            {"model": "chosen_one", "personality": "social", "count": 1, "count_random": true, "slot_id": "slot-c", "team": "neutral", "job": "none"}
        ],
        "settings": {
            "size_scale": 1.25, "interferable": false,
            "teams": {"hunters": {"name": "Porch guard", "color": "#e2a64a"}},
            "team_relations": {"hunters": {"rivals": "foe"}},
            "always_show_names": false, "always_show_xp": true,
            "flies": {"enabled": true, "min_interval": 2.5, "max_interval": 6.0, "max_flies": 10, "spawner": true}
        }
    })
}

/// `state/progression` ability tree, armour catalogue and sets.
pub fn progression_catalog() -> ProgressionCatalog {
    let e = |pairs: &[(&str, f64)]| pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect::<Vec<_>>();
    let node = |id: &str, name: &str, desc: &str, level: u32, effects: Vec<(String, f64)>, prereq: &[&str]| SkillNode {
        id: id.into(),
        name: name.into(),
        description: desc.into(),
        level_required: level,
        cost: 1,
        prerequisites: prereq.iter().map(|p| (*p).into()).collect(),
        effects,
    };
    let tree = vec![
        node(
            "vitality",
            "Vitality",
            "A sturdier body: you can take more hits before you fall.",
            2,
            e(&[("max_hp", 18.0)]),
            &[],
        ),
        node("quick_step", "Quick step", "Quicker legs: you walk and turn faster.", 3, e(&[("speed", 0.06)]), &[]),
        node(
            "silk_sense",
            "Silk sense",
            "A deeper silk reserve: more stamina for webs and pounces, and it refills faster.",
            4,
            e(&[("max_energy", 12.0), ("energy_regen", 0.8)]),
            &[],
        ),
        node(
            "carapace_harden",
            "Hardened carapace",
            "A denser shell: every hit you take lands softer.",
            5,
            e(&[("armor", 2.0)]),
            &["vitality"],
        ),
        node(
            "power_strike",
            "Power strike",
            "Stronger chelicerae: every bite hits harder.",
            6,
            e(&[("damage", 2.0)]),
            &[],
        ),
        node(
            "web_crafter",
            "Web crafter",
            "Practised spinning: more stamina, and it opens Silk tracking.",
            8,
            e(&[("max_energy", 8.0)]),
            &["silk_sense"],
        ),
        node(
            "silk_tracking",
            "Silk tracking",
            "Thrown silk corrects itself in flight and follows a moving target.",
            9,
            e(&[("web_homing", 1.0)]),
            &["web_crafter"],
        ),
        node(
            "long_stride",
            "Long stride",
            "Each step reaches further: you move faster still.",
            10,
            e(&[("speed", 0.08)]),
            &["quick_step"],
        ),
        node(
            "apex_predator",
            "Apex predator",
            "The peak of the hunt: much harder bites and a deep stamina reserve.",
            15,
            e(&[("damage", 4.0), ("max_energy", 16.0)]),
            &["power_strike", "carapace_harden"],
        ),
    ];
    #[allow(clippy::too_many_arguments)]
    fn item(id: &str, name: &str, slot: &str, desc: &str, stats: [f64; 5], set_id: &str, tier: &str) -> ArmourItem {
        ArmourItem {
            id: id.into(),
            name: name.into(),
            slot: slot.into(),
            description: desc.into(),
            armor: stats[0],
            max_hp: stats[1],
            max_energy: stats[2],
            damage: stats[3],
            speed: stats[4],
            set_id: set_id.into(),
            tier: tier.into(),
        }
    }
    // stats: armor, max_hp, max_energy, damage, speed
    let armour = vec![
        item(
            "silk_carapace",
            "Woven carapace",
            "carapace",
            "A flexible silk shell over the carapace.",
            [2.0, 8.0, 0.0, 0.0, -0.015],
            "",
            "common",
        ),
        item(
            "fluffy_mantle",
            "Soft setae mantle",
            "abdomen",
            "A warm, light mantle that cushions impacts.",
            [1.0, 12.0, 4.0, 0.0, -0.02],
            "",
            "common",
        ),
        item(
            "leg_guard_set",
            "Leg guard set",
            "legs",
            "Small guards fitted around the leg joints.",
            [1.0, 0.0, 0.0, 0.0, 0.025],
            "",
            "common",
        ),
        item(
            "pedipalp_cuffs",
            "Pedipalp cuffs",
            "pedipalps",
            "Tiny protective cuffs that leave the hands free.",
            [0.5, 0.0, 5.0, 0.5, -0.01],
            "",
            "common",
        ),
        item(
            "chelicerae_cap",
            "Fang cap",
            "head",
            "A narrow guard around the small head and chelicerae.",
            [1.0, 0.0, 0.0, 2.0, -0.008],
            "",
            "common",
        ),
        item(
            "warden_crest",
            "Warden ocular crest",
            "head",
            "A ridged visor over the eye mound; the eight eyes stay clear and the chelicerae get capped tips.",
            [1.0, 0.0, 0.0, 2.5, 0.0],
            "warden",
            "rare",
        ),
        item(
            "warden_carapace",
            "Warden carapace plate",
            "carapace",
            "A shield moulded to the prosoma, the fovea left open; the orange rim still shows around it.",
            [2.5, 12.0, 0.0, 0.0, -0.02],
            "warden",
            "rare",
        ),
        item(
            "warden_tergites",
            "Warden tergite bands",
            "abdomen",
            "Overlapping bands over the soft abdomen, the most vulnerable part; they flex as it breathes and leave the spinnerets free.",
            [1.5, 20.0, 0.0, 0.0, -0.02],
            "warden",
            "rare",
        ),
        item(
            "warden_greaves",
            "Warden leg plates",
            "legs",
            "Plates down all eight legs, femur to metatarsus, with a cop over each knee; the clawed tarsi stay bare to grip.",
            [1.0, 6.0, 0.0, 0.0, -0.01],
            "warden",
            "rare",
        ),
        item(
            "warden_bracers",
            "Warden palp bracers",
            "pedipalps",
            "Bracers on both pedipalps; the palp tips stay free to feel and to hold prey.",
            [0.5, 0.0, 6.0, 1.0, 0.0],
            "warden",
            "rare",
        ),
        item(
            "forager_hood",
            "Forager elytra hood",
            "head",
            "A beetle's wing case bent over the eye mound and lashed with silk.",
            [0.5, 0.0, 0.0, 1.0, 0.0],
            "forager",
            "uncommon",
        ),
        item(
            "forager_shell",
            "Forager shell",
            "carapace",
            "Two elytra halves over the carapace, still green-bronze where the light hits.",
            [1.2, 8.0, 0.0, 0.0, 0.0],
            "forager",
            "uncommon",
        ),
        item(
            "forager_mantle",
            "Forager mantle",
            "abdomen",
            "Wing-case scales sewn on silk over the abdomen.",
            [0.8, 12.0, 0.0, 0.0, -0.01],
            "forager",
            "uncommon",
        ),
        item(
            "forager_leggings",
            "Forager leggings",
            "legs",
            "Light chitin splints down each leg; barely heavier than bare legs.",
            [0.6, 0.0, 0.0, 0.0, 0.02],
            "forager",
            "uncommon",
        ),
        item(
            "forager_wraps",
            "Forager palp wraps",
            "pedipalps",
            "Chitin slivers wrapped round the palps.",
            [0.3, 0.0, 4.0, 0.0, 0.0],
            "forager",
            "uncommon",
        ),
        item(
            "frost_diadem",
            "Frostglass diadem",
            "head",
            "A ring of ice-clear crystal round the eye mound; the eyes look out through it.",
            [1.0, 0.0, 0.0, 2.0, 0.01],
            "frost",
            "epic",
        ),
        item(
            "frost_aegis",
            "Frostglass aegis",
            "carapace",
            "A faceted crystal shield over the carapace, light as frost.",
            [2.0, 12.0, 0.0, 0.0, 0.01],
            "frost",
            "epic",
        ),
        item(
            "frost_scales",
            "Frostglass scales",
            "abdomen",
            "Overlapping crystal scales that ring faintly as the spider walks.",
            [1.5, 18.0, 0.0, 0.0, 0.0],
            "frost",
            "epic",
        ),
        item(
            "frost_shards",
            "Frostglass leg shards",
            "legs",
            "Crystal sheaths down every leg; they shed the air like ice on a blade.",
            [1.2, 0.0, 0.0, 0.0, 0.04],
            "frost",
            "epic",
        ),
        item(
            "frost_talons",
            "Frostglass talons",
            "pedipalps",
            "Crystal points over the palps, cold enough to numb what they touch.",
            [0.5, 0.0, 8.0, 1.5, 0.0],
            "frost",
            "epic",
        ),
        item(
            "brood_crown",
            "Brood obsidian crown",
            "head",
            "A spiked crown of black glass; the runes on it glow when the spider bites.",
            [1.2, 0.0, 0.0, 4.0, 0.0],
            "brood",
            "epic",
        ),
        item(
            "brood_bulwark",
            "Brood obsidian bulwark",
            "carapace",
            "A heavy plate of black glass over the carapace, veined with violet runes.",
            [2.8, 16.0, 0.0, 0.0, -0.02],
            "brood",
            "epic",
        ),
        item(
            "brood_plates",
            "Brood obsidian plates",
            "abdomen",
            "Rune-cut obsidian bands over the abdomen.",
            [2.0, 24.0, 0.0, 0.0, -0.02],
            "brood",
            "epic",
        ),
        item(
            "brood_spurs",
            "Brood obsidian leg spurs",
            "legs",
            "Black-glass plates down each leg, spurred at the knee.",
            [1.2, 0.0, 0.0, 1.0, -0.01],
            "brood",
            "epic",
        ),
        item(
            "brood_claws",
            "Brood obsidian claws",
            "pedipalps",
            "Glass-edged palp claws that cut as they grip.",
            [0.6, 0.0, 0.0, 2.0, 0.0],
            "brood",
            "epic",
        ),
        item(
            "sun_crown",
            "Sunforged crown",
            "head",
            "A gold crown over the eye mound set with an ember-red stone.",
            [1.5, 0.0, 6.0, 3.5, 0.0],
            "sun",
            "legendary",
        ),
        item(
            "sun_aegis",
            "Sunforged aegis",
            "carapace",
            "A gold shield over the carapace with ember veins that never cool.",
            [3.0, 20.0, 0.0, 0.0, 0.0],
            "sun",
            "legendary",
        ),
        item(
            "sun_scales",
            "Sunforged scales",
            "abdomen",
            "Gold bands over the abdomen, each lit from within.",
            [2.2, 30.0, 0.0, 0.0, 0.0],
            "sun",
            "legendary",
        ),
        item(
            "sun_legs",
            "Sunforged leg plates",
            "legs",
            "Gold plates down every leg; they hold their warmth, and the legs stay quick.",
            [1.5, 8.0, 0.0, 0.0, 0.02],
            "sun",
            "legendary",
        ),
        item(
            "sun_gauntlets",
            "Sunforged palp gauntlets",
            "pedipalps",
            "Gold gauntlets over the palps, ember-hot at the tips.",
            [0.8, 0.0, 10.0, 2.5, 0.0],
            "sun",
            "legendary",
        ),
    ];
    let set = |id: &str, name: &str, desc: &str, effects: Vec<(String, f64)>| ArmourSet {
        id: id.into(),
        name: name.into(),
        description: desc.into(),
        effects,
    };
    let sets = vec![
        set(
            "forager",
            "Forager set",
            "Wear all five pieces: light enough to hunt all day.",
            e(&[("max_energy", 8.0), ("energy_regen", 0.5)]),
        ),
        set(
            "warden",
            "Warden set",
            "Wear all five pieces: the plates brace one another.",
            e(&[("max_hp", 10.0), ("armor", 0.5), ("energy_regen", 1.5)]),
        ),
        set(
            "frost",
            "Frostglass set",
            "Wear all five pieces: the crystal hums and the spider flies over the ground.",
            e(&[("speed", 0.06), ("max_energy", 10.0), ("energy_regen", 2.0)]),
        ),
        set(
            "brood",
            "Obsidian Brood set",
            "Wear all five pieces: every rune wakes at once.",
            e(&[("damage", 3.0), ("max_hp", 15.0)]),
        ),
        set(
            "sun",
            "Sunforged regalia",
            "Wear all five pieces: the embers join into one fire.",
            e(&[("max_hp", 20.0), ("armor", 1.0), ("damage", 2.0), ("energy_regen", 2.0)]),
        ),
    ];
    ProgressionCatalog { tree, armour, sets, web_energy: 12.0, jump_energy: 18.0 }
}

/// `campaign.MAPS` plus the "Hold the burrow" placeholder.
pub fn campaign_maps() -> Vec<MapCard> {
    let rows: &[(&str, &str, &str, &str, Option<&str>)] = &[
        (
            "territory",
            "Take back the desktop",
            "Capture Food or Silk, seal the Hatchery, survive the counterattack and claim Thorn nest.",
            "raid",
            None,
        ),
        (
            "swarm",
            "Fly swarm",
            "Flies pour from nests on every screen. Pin them with silk and eat them before the rivals do.",
            "swarm",
            None,
        ),
        (
            "ember",
            "Ember hollow",
            "Raiders in bronze hold the hollow. Their warden wears the full Warden plate.",
            "raid",
            Some("territory"),
        ),
        (
            "reclaim",
            "Reclaim the desktop",
            "Your desktop is frozen and infested. Destroy the nest on every screen before the acid eats it. Esc gives it back at once.",
            "reclaim",
            Some("territory"),
        ),
        (
            "storm",
            "Fly storm",
            "A storm of fast flies, more rivals, less time. Rival dens wait on your other screens.",
            "swarm",
            Some("ember"),
        ),
        ("obsidian", "Obsidian deep", "Rune-cut raiders and a brood matriarch in black glass.", "raid", Some("ember")),
        (
            "queen",
            "Queen of thorns",
            "The nest mother herself, in Sunforged regalia. Her guard wear crystal and obsidian.",
            "raid",
            Some("obsidian"),
        ),
    ];
    let mut maps: Vec<MapCard> = rows
        .iter()
        .map(|(id, title, blurb, kind, after)| MapCard {
            id: (*id).into(),
            title: (*title).into(),
            blurb: (*blurb).into(),
            kind: (*kind).into(),
            unlock_after: after.map(|a| a.into()),
            playable: true,
            record: MissionRecord::default(),
        })
        .collect();
    maps.push(MapCard {
        id: "burrow".into(),
        title: "Hold the burrow".into(),
        blurb: "Wave after wave comes for your home. Keep it.".into(),
        kind: "raid".into(),
        unlock_after: None,
        playable: false,
        record: MissionRecord::default(),
    });
    maps
}

/// A fresh Adventure page with two screens.
pub fn adventure_view() -> AdventureView {
    AdventureView {
        hero_name: DEFAULT_HERO_NAME.into(),
        level: 1,
        skill_points: 0,
        companions: 0,
        amber: 0,
        admin: false,
        two_player: false,
        all_screens: true,
        disabled_screens: Vec::new(),
        maps: campaign_maps(),
        custom_maps: Vec::new(),
        screens: vec![
            ScreenInfo {
                name: "\\\\.\\DISPLAY1".into(),
                number: 1,
                width: 1920,
                height: 1080,
                primary: true,
                state: "active".into(),
            },
            ScreenInfo {
                name: "\\\\.\\DISPLAY2".into(),
                number: 2,
                width: 2560,
                height: 1440,
                primary: false,
                state: "active".into(),
            },
        ],
        controls: ControlSettings::default(),
    }
}

/// `SELL_PRICES`.
pub fn sell_prices() -> BTreeMap<String, i64> {
    [("common", 5), ("uncommon", 12), ("rare", 30), ("epic", 70), ("legendary", 160)]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), *v))
        .collect()
}

/// The hero, player two and a companion, sharing one armoury.
pub fn party_view() -> PartyView {
    let hero = Progression {
        level: 6,
        xp: 40,
        skill_points: 2,
        unlocked_abilities: vec!["vitality".into()],
        inventory: Vec::new(),
        equipped: [("carapace".to_owned(), "silk_carapace".to_owned())].into_iter().collect(),
        ..Default::default()
    };
    let two = Progression { level: 3, skill_points: 1, inventory: Vec::new(), ..Default::default() };
    PartyView {
        members: vec![
            PartyMember { who: HERO.into(), name: "Trailblazer".into(), progression: hero },
            PartyMember { who: PLAYER_TWO.into(), name: "Second".into(), progression: two },
            PartyMember {
                who: "scout".into(),
                name: "Scout".into(),
                progression: Progression { inventory: Vec::new(), ..Default::default() },
            },
        ],
        two_player: false,
        amber: 120,
        owned: vec![
            "silk_carapace".into(),
            "leg_guard_set".into(),
            "warden_crest".into(),
            "sun_aegis".into(),
            "forager_hood".into(),
        ],
        spares: [("warden_crest".to_owned(), 2u32)].into_iter().collect(),
        levels: [("warden_crest".to_owned(), 2u32)].into_iter().collect(),
        sell_prices: sell_prices(),
    }
}

/// `campaign.COMPANIONS`.
pub fn companions() -> Vec<(String, String)> {
    [("scout", "Scout"), ("weaver", "Silk weaver"), ("hunter", "Hunter"), ("sentinel", "Sentinel")]
        .iter()
        .map(|(a, b)| ((*a).into(), (*b).into()))
        .collect()
}

/// `ENEMY_KINDS`: (id, name, boss).
pub fn enemy_kinds() -> Vec<(String, String, bool)> {
    [
        ("redback_raider", "Redback raider", false),
        ("ash_wolf", "Ash wolf spider", false),
        ("jumping_skirmisher", "Jumping skirmisher", false),
        ("harvest_stalker", "Harvest stalker", false),
        ("trapdoor_brute", "Trapdoor brute", false),
        ("pale_cave", "Pale cave spider", false),
        ("crab_reaver", "Crab reaver", false),
        ("thorn_weaver", "Thorn orb-weaver", false),
        ("ember_matriarch", "Ember matriarch", true),
        ("ivory_regent", "Ivory crab regent", true),
        ("thorn_crown", "Thorn crown", true),
    ]
    .iter()
    .map(|(a, b, c)| ((*a).into(), (*b).into(), *c))
    .collect()
}

/// `custom_maps.new_map()` as Python writes it.
pub fn new_custom_map() -> Value {
    json!({"id": "", "title": "My map", "blurb": "A map of your own.", "tier": 1, "enemy_tiers": ["common", "uncommon"],
           "reward_companion": null, "cap": 12, "wave_every": 24.0, "tactics": -1,
           "buildings": [
               {"kind": "home", "name": "Home burrow", "fx": 0.16, "fy": 0.57, "far": false, "owned": true, "reserves": 0, "supply": 180.0, "guards": []},
               {"kind": "food", "name": "Food cache", "fx": 0.39, "fy": 0.29, "far": false, "owned": false, "reserves": 0, "supply": 180.0,
                "guards": [{"role": "guard", "kind": "auto", "level_bonus": 0, "hp": 1.0, "armor": "auto", "item_level": 1}]},
               {"kind": "silk", "name": "Silk loom", "fx": 0.39, "fy": 0.7, "far": false, "owned": false, "reserves": 0, "supply": 180.0, "guards": []},
               {"kind": "hatchery", "name": "Hatchery", "fx": 0.64, "fy": 0.47, "far": false, "owned": false, "reserves": 4, "supply": 180.0,
                "guards": [{"role": "hunter", "kind": "auto", "level_bonus": 0, "hp": 1.0, "armor": "auto", "item_level": 1}]},
               {"kind": "nest", "name": "Thorn nest", "fx": 0.83, "fy": 0.3, "far": false, "owned": false, "reserves": 0, "supply": 180.0, "guards": []}
           ],
           "enemies": [],
           "boss": {"role": "guardian", "kind": "auto", "level_bonus": 0, "hp": 1.0,
                    "armor": ["forager_hood", "forager_shell", "forager_mantle", "forager_leggings", "forager_wraps"],
                    "item_level": 2, "name": "Guardian"}})
}
