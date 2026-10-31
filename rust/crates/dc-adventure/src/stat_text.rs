//! Port of app/stat_text.py
//!
//! What a skill or an armour piece gives, in words a player can use. Every
//! number is the one the game applies. Tooltips are the same Qt rich-text
//! HTML the Python writes, so a port of the UI can show them as they are (or
//! use [`effect_lines`] for plain text).

use crate::adventure::PlayerController;
use crate::progression::{
    ABILITY_TREE, ARMOR_TIERS, AbilityNode, ArmorItem, ITEM_STATS, MAX_ITEM_LEVEL, ProgressionState, ability_by_id,
    armor_set, item_at_level, set_pieces_worn,
};

pub const GOOD: &str = "#3f6b1f";
pub const BAD: &str = "#a2371f";
pub const DIM: &str = "#6a4a2c";

pub const SLOT_NAMES: [(&str, &str); 5] =
    [("head", "Head"), ("carapace", "Carapace"), ("abdomen", "Abdomen"), ("legs", "Legs"), ("pedipalps", "Pedipalps")];

/// The colours players know from other games, lowest quality first.
pub const TIER_COLORS: [(&str, &str); 5] = [
    ("common", "#cfc8b8"),
    ("uncommon", "#6fc24a"),
    ("rare", "#4f9ae8"),
    ("epic", "#b066f0"),
    ("legendary", "#f5a431"),
];

/// Darker versions, readable on the parchment tooltip.
pub const TIER_INK: [(&str, &str); 5] = [
    ("common", "#6a5f4c"),
    ("uncommon", "#2f7a18"),
    ("rare", "#1f5fae"),
    ("epic", "#7a2fc0"),
    ("legendary", "#b0620a"),
];

fn lookup<'a>(table: &'a [(&str, &'a str)], key: &str) -> Option<&'a str> {
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

pub fn slot_name(slot: &str) -> String {
    lookup(&SLOT_NAMES, slot).unwrap_or(slot).to_string()
}

pub fn tier_color(tier: &str) -> Option<&'static str> {
    TIER_COLORS.iter().find(|(k, _)| *k == tier).map(|(_, v)| *v)
}

pub fn tier_ink(tier: &str) -> Option<&'static str> {
    TIER_INK.iter().find(|(k, _)| *k == tier).map(|(_, v)| *v)
}

/// `TIER_NAMES.get(tier, tier)`: "Common", "Uncommon", ...
pub fn tier_name(tier: &str) -> String {
    if !ARMOR_TIERS.contains(&tier) {
        return tier.to_string();
    }
    let mut chars = tier.chars();
    chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
}

/// Python's `html.escape(text)`.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
    out
}

/// Python's `f"{value:g}"`.
pub fn fmt_g(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    if !value.is_finite() {
        return if value.is_nan() {
            "nan".into()
        } else if value > 0.0 {
            "inf".into()
        } else {
            "-inf".into()
        };
    }
    let sci = format!("{value:.5e}");
    let (mantissa, exp) = sci.split_once('e').expect("scientific format");
    let exp: i32 = exp.parse().expect("exponent");
    if (-4..6).contains(&exp) {
        let decimals = (5 - exp).max(0) as usize;
        strip_zeros(&format!("{value:.decimals$}"))
    } else {
        let m = strip_zeros(mantissa);
        format!("{m}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
    }
}

fn strip_zeros(text: &str) -> String {
    if text.contains('.') { text.trim_end_matches('0').trim_end_matches('.').to_string() } else { text.to_string() }
}

/// `_num`: a signed number without a needless ".0".
fn num(value: f64) -> String {
    if dc_core::math::py_mod(value, 1.0) != 0.0 { strip_zeros(&format!("{value:+.1}")) } else { format!("{value:+.0}") }
}

/// (text, is_good) for each effect, in the order a player cares about.
pub fn effect_lines(effects: &[(&str, f64)]) -> Vec<(String, bool)> {
    let (web, pounce) = (PlayerController::WEB_ENERGY, PlayerController::JUMP_ENERGY);
    let mut lines = Vec::new();
    for key in ["max_hp", "armor", "damage", "max_energy", "energy_regen", "speed", "web_homing"] {
        let value = effects.iter().find(|(k, _)| *k == key).map_or(0.0, |(_, v)| *v);
        if value.abs() < 1e-9 {
            continue;
        }
        let good = value > 0.0;
        let text = match key {
            "max_hp" => format!("{} max health", num(value)),
            "armor" => format!("{} armour: every hit you take is {} lower", num(value), fmt_g(value.abs())),
            "damage" => format!("{} damage on every bite", num(value)),
            "max_energy" => {
                format!("{} max stamina (a web costs {}, a pounce {})", num(value), fmt_g(web), fmt_g(pounce))
            }
            "energy_regen" => format!("{} stamina per second (base 8)", num(value)),
            "speed" => format!("{}% speed", num(value * 100.0)),
            _ => "Thrown silk steers after a moving target".to_string(),
        };
        lines.push((text, good));
    }
    lines
}

pub fn effects_html(effects: &[(&str, f64)]) -> String {
    effect_lines(effects)
        .iter()
        .map(|(text, good)| {
            format!(
                "<span style='color:{}'>{} {}</span>",
                if *good { GOOD } else { BAD },
                if *good { "\u{25b2}" } else { "\u{25bc}" },
                escape(text)
            )
        })
        .collect::<Vec<_>>()
        .join("<br>")
}

/// The piece's non-zero stats, in `ITEM_STATS` order.
pub fn item_effects(item: &ArmorItem) -> Vec<(&'static str, f64)> {
    ITEM_STATS.iter().map(|k| (*k, item.stat(k))).filter(|(_, v)| *v != 0.0).collect()
}

/// Name, cost, what it needs, what it does and exactly what it gives.
pub fn skill_tooltip(node: &AbilityNode, state: Option<&ProgressionState>) -> String {
    let mut parts = vec![
        format!("<b style='font-size:11pt'>{}</b>", escape(node.name)),
        format!(
            "<span style='color:{DIM}'>Level {} \u{b7} {} skill point{}</span>",
            node.level_required,
            node.cost,
            if node.cost != 1 { "s" } else { "" }
        ),
    ];
    if !node.prerequisites.is_empty() {
        let names: Vec<&str> = node.prerequisites.iter().map(|r| ability_by_id(r).map_or(*r, |n| n.name)).collect();
        parts.push(format!("<span style='color:{DIM}'>Needs {}</span>", escape(&names.join(", "))));
    }
    parts.push(escape(node.description));
    parts.push(format!("<b>Gives</b><br>{}", effects_html(node.effects)));
    let opens: Vec<&str> = ABILITY_TREE.iter().filter(|n| n.prerequisites.contains(&node.id)).map(|n| n.name).collect();
    if !opens.is_empty() {
        parts.push(format!("<span style='color:{DIM}'>Opens {}</span>", escape(&opens.join(", "))));
    }
    if let Some(state) = state {
        if state.unlocked_abilities.iter().any(|a| a == node.id) {
            parts.push(format!("<i style='color:{GOOD}'>Learned</i>"));
        } else if state.can_unlock(node.id) {
            parts.push(format!("<i style='color:{GOOD}'>Click to learn</i>"));
        }
    }
    format!("<p>{}</p>", parts.join("<br>"))
}

/// "+2 armour: every hit ..." -> "+2 armour", for one-line summaries.
pub fn short_effect(text: &str) -> &str {
    let head = text.split(':').next().unwrap_or(text);
    head.split(" (").next().unwrap_or(head)
}

pub fn set_line(set_id: &str, state: Option<&ProgressionState>, dim: &str, good: &str) -> String {
    let Some(armor_set) = armor_set(set_id) else { return String::new() };
    let worn = state.map_or(0, |s| set_pieces_worn(s, set_id));
    let total = armor_set.pieces().len();
    let bonus: Vec<String> =
        effect_lines(armor_set.effects).iter().map(|(text, _)| short_effect(text).to_string()).collect();
    let colour = if worn == total { good } else { dim };
    format!(
        "<span style='color:{colour}'><b>{} {worn}/{total}</b> \u{b7} all {total}: {}</span>",
        escape(armor_set.name),
        escape(&bonus.join(", "))
    )
}

/// Name, quality, slot, what it is, and its stats at `level`.
pub fn item_tooltip(item: &ArmorItem, state: Option<&ProgressionState>, level: i64) -> String {
    let ink = tier_ink(item.tier).unwrap_or(DIM);
    let shown = item_at_level(item, level);
    let rank = if level > 1 { format!(" \u{b7} Level {level}/{MAX_ITEM_LEVEL}") } else { String::new() };
    let mut parts = vec![
        format!("<b style='font-size:11pt; color:{ink}'>{}</b>", escape(item.name)),
        format!(
            "<span style='color:{ink}'><b>{}</b></span><span style='color:{DIM}'> \u{b7} {}{rank}</span>",
            tier_name(item.tier),
            slot_name(item.slot)
        ),
        escape(item.description),
        effects_html(&item_effects(&shown)),
    ];
    if armor_set(item.set_id).is_some() {
        parts.push(set_line(item.set_id, state, DIM, GOOD));
    }
    format!("<p>{}</p>", parts.join("<br>"))
}

#[cfg(test)]
mod tests {
    //! Port of tests/test_armour_set.py's tooltip test, plus the Python
    //! output of every tooltip as a fixture (tools/dump_fixtures.py).
    use super::*;
    use crate::progression::{ARMOR_CATALOG, armor_by_id};

    #[test]
    fn numbers_format_like_python() {
        assert_eq!(fmt_g(2.0), "2");
        assert_eq!(fmt_g(0.5), "0.5");
        assert_eq!(fmt_g(12.0), "12");
        assert_eq!(fmt_g(1.8000000000000003), "1.8");
        assert_eq!(fmt_g(1234567.0), "1.23457e+06");
        assert_eq!(fmt_g(0.00001), "1e-05");
        assert_eq!(num(2.0), "+2");
        assert_eq!(num(-0.015 * 100.0), "-1.5");
        assert_eq!(num(6.000000000000001), "+6");
        assert_eq!(tier_name("legendary"), "Legendary");
    }

    #[test]
    fn skill_and_armour_tooltips_say_exactly_what_they_give() {
        let tip = skill_tooltip(ability_by_id("carapace_harden").unwrap(), None);
        assert!(tip.contains("+2 armour") && tip.contains("Needs Vitality"), "{tip}");
        let item = armor_by_id("warden_crest").unwrap();
        let tip = item_tooltip(item, None, 3);
        assert!(tip.contains("Level 3/5") && tip.contains("Warden set 0/5"), "{tip}");
        assert!(tip.contains("+3.5 damage on every bite"), "{tip}");
        assert!(effect_lines(&[("speed", -0.02)])[0] == ("-2% speed".to_string(), false));
    }

    #[test]
    fn every_tooltip_matches_python() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/stat_text.json");
        let fixture: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for case in fixture["skills"].as_array().unwrap() {
            let node = ability_by_id(case["id"].as_str().unwrap()).unwrap();
            assert_eq!(skill_tooltip(node, None), case["html"].as_str().unwrap(), "{}", node.id);
        }
        for case in fixture["items"].as_array().unwrap() {
            let item = armor_by_id(case["id"].as_str().unwrap()).unwrap();
            let level = case["level"].as_i64().unwrap();
            assert_eq!(item_tooltip(item, None, level), case["html"].as_str().unwrap(), "{} {level}", item.id);
        }
        assert_eq!(fixture["items"].as_array().unwrap().len(), ARMOR_CATALOG.len() * MAX_ITEM_LEVEL as usize);
    }
}
