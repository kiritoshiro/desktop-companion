//! Port of `content/enemy_kinds.py`: the visual enemy catalogue.
//!
//! Tiers are minimum map tiers; boss queries are separate. The caller applies
//! `size_scale` after spawning.

use std::sync::LazyLock;

use dc_core::{Color, PyRandom};

use crate::palettes::palette_from_hue;
use crate::util::ColorMap;

#[derive(Clone, Debug, PartialEq)]
pub struct EnemyKind {
    pub id: &'static str,
    pub name: &'static str,
    pub model_id: &'static str,
    pub skins: Vec<ColorMap>,
    pub size_scale: f64,
    pub boss: bool,
    pub tier: i64,
    pub tags: Vec<&'static str>,
}

fn scale(c: Color, k: f64) -> Color {
    // int(c * k): truncation toward zero.
    let f = |x: u8| (x as f64 * k).trunc() as u8;
    Color::rgb(f(c.r), f(c.g), f(c.b))
}

fn lift(c: Color, add: u16) -> Color {
    let f = |x: u8| (x as u16 + add).min(255) as u8;
    Color::rgb(f(c.r), f(c.g), f(c.b))
}

/// `_skin(hue, saturation, value, pale)`.
pub fn skin(hue: f64, saturation: f64, value: f64, pale: bool) -> ColorMap {
    let mut palette = palette_from_hue(hue, saturation, value);
    if pale {
        let body = lift(palette.get("body").copied().unwrap(), 145);
        let legs = lift(palette.get("legs").copied().unwrap(), 100);
        palette.insert("body", body);
        palette.insert("legs", legs);
        let band = scale(palette.get("leg_band").copied().unwrap(), 0.45);
        let highlight = scale(palette.get("highlight").copied().unwrap(), 0.48);
        let eyes = scale(palette.get("eyes").copied().unwrap(), 0.30);
        palette.insert("leg_band", band);
        palette.insert("highlight", highlight);
        palette.insert("eyes", eyes);
    }
    let highlight = palette.get("highlight").copied().unwrap();
    let band = palette.get("leg_band").copied().unwrap();
    palette.insert("rim", highlight);
    palette.insert("marking", band);
    palette.insert("fluff_color", highlight);
    palette
}

type SkinSpec = (f64, f64, f64, bool);

#[allow(clippy::too_many_arguments)]
fn kind(
    id: &'static str,
    name: &'static str,
    model_id: &'static str,
    skins: [SkinSpec; 3],
    size_scale: f64,
    boss: bool,
    tier: i64,
    tags: &[&'static str],
) -> EnemyKind {
    EnemyKind {
        id,
        name,
        model_id,
        skins: skins.iter().map(|(h, s, v, p)| skin(*h, *s, *v, *p)).collect(),
        size_scale,
        boss,
        tier,
        tags: tags.to_vec(),
    }
}

static ENEMY_KINDS: LazyLock<Vec<EnemyKind>> = LazyLock::new(|| {
    const F: bool = false;
    const T: bool = true;
    vec![
        kind(
            "redback_raider",
            "Redback raider",
            "enemy_redback_raider",
            [(0.0, 0.8, 0.9, F), (0.08, 0.8, 0.9, F), (0.92, 0.6, 0.9, F)],
            1.0,
            false,
            1,
            &["fast"],
        ),
        kind(
            "ash_wolf",
            "Ash wolf spider",
            "enemy_ash_wolf",
            [(0.6, 0.12, 0.8, F), (0.09, 0.45, 0.8, F), (0.46, 0.3, 0.8, F)],
            1.0,
            false,
            1,
            &["hunter"],
        ),
        kind(
            "jumping_skirmisher",
            "Jumping skirmisher",
            "enemy_jumping_skirmisher",
            [(0.48, 0.65, 0.95, F), (0.79, 0.6, 0.95, F), (0.12, 0.7, 0.95, F)],
            0.9,
            false,
            1,
            &["fast"],
        ),
        kind(
            "harvest_stalker",
            "Harvest stalker",
            "enemy_harvest_stalker",
            [(0.08, 0.5, 0.8, F), (0.35, 0.45, 0.8, F), (0.65, 0.4, 0.9, F)],
            1.0,
            false,
            2,
            &["ranged"],
        ),
        kind(
            "trapdoor_brute",
            "Trapdoor brute",
            "enemy_trapdoor_brute",
            [(0.08, 0.65, 0.8, F), (0.62, 0.45, 0.8, F), (0.01, 0.6, 0.8, F)],
            1.15,
            false,
            2,
            &["heavy"],
        ),
        kind(
            "pale_cave",
            "Pale cave spider",
            "enemy_pale_cave",
            [(0.13, 0.12, 0.98, T), (0.52, 0.2, 0.98, T), (0.78, 0.18, 0.98, T)],
            1.0,
            false,
            2,
            &["cave"],
        ),
        kind(
            "crab_reaver",
            "Crab reaver",
            "enemy_crab_reaver",
            [(0.1, 0.7, 0.95, F), (0.46, 0.65, 0.85, F), (0.94, 0.6, 0.9, F)],
            1.0,
            false,
            3,
            &["heavy"],
        ),
        kind(
            "thorn_weaver",
            "Thorn orb-weaver",
            "enemy_thorn_weaver",
            [(0.32, 0.65, 0.9, F), (0.78, 0.6, 0.9, F), (0.05, 0.75, 0.95, F)],
            1.05,
            false,
            3,
            &["ranged"],
        ),
        kind(
            "ember_matriarch",
            "Ember matriarch",
            "enemy_ember_matriarch",
            [(0.01, 0.8, 0.95, F), (0.1, 0.8, 0.95, F), (0.85, 0.65, 0.95, F)],
            1.65,
            true,
            1,
            &["heavy"],
        ),
        kind(
            "ivory_regent",
            "Ivory crab regent",
            "enemy_ivory_regent",
            [(0.12, 0.16, 0.98, T), (0.51, 0.24, 0.98, T), (0.94, 0.2, 0.98, T)],
            1.7,
            true,
            2,
            &["heavy"],
        ),
        kind(
            "thorn_crown",
            "Thorn crown",
            "enemy_thorn_crown",
            [(0.76, 0.75, 0.95, F), (0.4, 0.7, 0.95, F), (0.02, 0.8, 0.95, F)],
            1.8,
            true,
            3,
            &["ranged"],
        ),
    ]
});

/// `ENEMY_KINDS`, in catalogue order.
pub fn enemy_kinds() -> &'static [EnemyKind] {
    &ENEMY_KINDS
}

pub fn enemy_kind(id: &str) -> Option<&'static EnemyKind> {
    ENEMY_KINDS.iter().find(|k| k.id == id)
}

/// `ENEMY_MODEL_IDS`.
pub fn enemy_model_ids() -> Vec<&'static str> {
    ENEMY_KINDS.iter().map(|k| k.model_id).collect()
}

/// `ENEMY_BODY_PLANS`.
pub const ENEMY_BODY_PLANS: [&str; 2] = ["enemy_crab", "enemy_orb"];

/// `is_enemy_model`.
pub fn is_enemy_model(model_id: &str) -> bool {
    ENEMY_KINDS.iter().any(|k| k.model_id == model_id)
}

/// `kinds_for_tier`: kinds unlocked at or before `tier`, bosses or not.
pub fn kinds_for_tier(tier: i64, boss: bool) -> Vec<&'static EnemyKind> {
    ENEMY_KINDS.iter().filter(|k| k.tier <= tier && k.boss == boss).collect()
}

/// `pick_skin`: `random.Random(seed).choice(kind.skins)`, a fresh copy.
pub fn pick_skin(kind: &EnemyKind, seed: i64) -> ColorMap {
    let mut rng = PyRandom::from_int(seed as i128);
    rng.choice(&kind.skins).clone()
}
