//! Port of app/stat_text.py: what a skill or an armour piece gives, in words
//! a player can use. The Python returned rich-text HTML for Qt tooltips; here
//! a tooltip is a list of `(text, colour, strong)` lines.

use egui::Color32;

use crate::model::{ArmourItem, MAX_ITEM_LEVEL, Progression, ProgressionCatalog, SkillNode};
use crate::theme::hex;

pub const GOOD: Color32 = Color32::from_rgb(0x3f, 0x6b, 0x1f);
pub const BAD: Color32 = Color32::from_rgb(0xa2, 0x37, 0x1f);
pub const DIM: Color32 = Color32::from_rgb(0x6a, 0x4a, 0x2c);
pub const INK: Color32 = crate::theme::INK;

/// One tooltip line.
pub type Line = (String, Color32, bool);

/// `SLOT_NAMES`.
pub fn slot_name(slot: &str) -> String {
    match slot {
        "head" => "Head".into(),
        "carapace" => "Carapace".into(),
        "abdomen" => "Abdomen".into(),
        "legs" => "Legs".into(),
        "pedipalps" => "Pedipalps".into(),
        other => other.into(),
    }
}

/// `TIER_COLORS`: the colours players know from other games.
pub fn tier_color(tier: &str) -> Color32 {
    match tier {
        "uncommon" => hex("#6fc24a"),
        "rare" => hex("#4f9ae8"),
        "epic" => hex("#b066f0"),
        "legendary" => hex("#f5a431"),
        _ => hex("#cfc8b8"),
    }
}

/// `TIER_INK`: darker versions, readable on the parchment tooltip.
pub fn tier_ink(tier: &str) -> Color32 {
    match tier {
        "common" => hex("#6a5f4c"),
        "uncommon" => hex("#2f7a18"),
        "rare" => hex("#1f5fae"),
        "epic" => hex("#7a2fc0"),
        "legendary" => hex("#b0620a"),
        _ => DIM,
    }
}

/// `TIER_NAMES`.
pub fn tier_name(tier: &str) -> String {
    let mut c = tier.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Python's `%g`.
fn g(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        let s = format!("{value:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

/// `_num`: "+2", "+0.5", "-1.5".
fn num(value: f64) -> String {
    if value.fract() != 0.0 {
        let s = format!("{value:+.1}");
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        format!("{value:+.0}")
    }
}

/// `effect_lines`: (text, is_good) in the order a player cares about.
pub fn effect_lines(effects: &[(String, f64)], catalog: &ProgressionCatalog) -> Vec<(String, bool)> {
    let get = |key: &str| effects.iter().filter(|(k, _)| k == key).map(|(_, v)| *v).sum::<f64>();
    let mut lines = Vec::new();
    for key in ["max_hp", "armor", "damage", "max_energy", "energy_regen", "speed", "web_homing"] {
        let value = get(key);
        if value.abs() < 1e-9 {
            continue;
        }
        let good = value > 0.0;
        let text = match key {
            "max_hp" => format!("{} max health", num(value)),
            "armor" => format!("{} armour: every hit you take is {} lower", num(value), g(value.abs())),
            "damage" => format!("{} damage on every bite", num(value)),
            "max_energy" => format!(
                "{} max stamina (a web costs {}, a pounce {})",
                num(value),
                g(catalog.web_energy),
                g(catalog.jump_energy)
            ),
            "energy_regen" => format!("{} stamina per second (base 8)", num(value)),
            "speed" => format!("{}% speed", num(value * 100.0)),
            _ => "Thrown silk steers after a moving target".into(),
        };
        lines.push((text, good));
    }
    lines
}

/// `short_effect`: "+2 armour: every hit ..." -> "+2 armour".
pub fn short_effect(text: &str) -> String {
    let head = text.split(':').next().unwrap_or(text);
    head.split(" (").next().unwrap_or(head).to_owned()
}

fn effects_lines(effects: &[(String, f64)], catalog: &ProgressionCatalog) -> Vec<Line> {
    effect_lines(effects, catalog)
        .into_iter()
        .map(|(text, good)| {
            (format!("{} {}", if good { "\u{25b2}" } else { "\u{25bc}" }, text), if good { GOOD } else { BAD }, false)
        })
        .collect()
}

/// `skill_tooltip`: name, cost, what it needs, what it does, what it gives.
pub fn skill_tooltip(node: &SkillNode, state: Option<&Progression>, catalog: &ProgressionCatalog) -> Vec<Line> {
    let mut lines: Vec<Line> = vec![
        (node.name.clone(), INK, true),
        (
            format!(
                "Level {} \u{b7} {} skill point{}",
                node.level_required,
                node.cost,
                if node.cost != 1 { "s" } else { "" }
            ),
            DIM,
            false,
        ),
    ];
    if !node.prerequisites.is_empty() {
        let names: Vec<String> = node
            .prerequisites
            .iter()
            .map(|r| catalog.node(r).map(|n| n.name.clone()).unwrap_or_else(|| r.clone()))
            .collect();
        lines.push((format!("Needs {}", names.join(", ")), DIM, false));
    }
    lines.push((node.description.clone(), INK, false));
    lines.push(("Gives".into(), INK, true));
    lines.extend(effects_lines(&node.effects, catalog));
    let opens: Vec<String> =
        catalog.tree.iter().filter(|n| n.prerequisites.contains(&node.id)).map(|n| n.name.clone()).collect();
    if !opens.is_empty() {
        lines.push((format!("Opens {}", opens.join(", ")), DIM, false));
    }
    if let Some(state) = state {
        if state.unlocked_abilities.contains(&node.id) {
            lines.push(("Learned".into(), GOOD, false));
        } else if state.can_unlock(catalog, &node.id) {
            lines.push(("Click to learn".into(), GOOD, false));
        }
    }
    lines
}

/// `set_line`: "Warden set 2/5 · all 5: +10 max health, ...".
pub fn set_line(
    set_id: &str,
    state: Option<&Progression>,
    catalog: &ProgressionCatalog,
    dim: Color32,
    good: Color32,
) -> Line {
    let Some(set) = catalog.set(set_id) else { return (String::new(), dim, false) };
    let worn = state.map(|s| s.set_pieces_worn(catalog, set_id)).unwrap_or(0);
    let total = catalog.set_pieces(set_id).len();
    let bonus: Vec<String> = effect_lines(&set.effects, catalog).into_iter().map(|(t, _)| short_effect(&t)).collect();
    let colour = if worn == total { good } else { dim };
    (format!("{} {}/{} \u{b7} all {}: {}", set.name, worn, total, total, bonus.join(", ")), colour, false)
}

/// `item_tooltip`: name, quality, slot, what it is, and its stats at `level`.
pub fn item_tooltip(
    item: &ArmourItem,
    state: Option<&Progression>,
    level: u32,
    catalog: &ProgressionCatalog,
) -> Vec<Line> {
    let ink = tier_ink(&item.tier);
    let shown = item.at_level(level);
    let rank = if level > 1 { format!(" \u{b7} Level {level}/{MAX_ITEM_LEVEL}") } else { String::new() };
    let mut lines: Vec<Line> = vec![
        (item.name.clone(), ink, true),
        (format!("{} \u{b7} {}{}", tier_name(&item.tier), slot_name(&item.slot), rank), ink, false),
        (item.description.clone(), INK, false),
    ];
    lines.extend(effects_lines(&shown.effects(), catalog));
    if catalog.set(&item.set_id).is_some() {
        lines.push(set_line(&item.set_id, state, catalog, DIM, GOOD));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_like_the_python() {
        assert_eq!(num(2.0), "+2");
        assert_eq!(num(0.5), "+0.5");
        assert_eq!(num(-1.5), "-1.5");
        assert_eq!(num(6.0000001), "+6");
        assert_eq!(g(2.5), "2.5");
        assert_eq!(g(3.0), "3");
    }

    #[test]
    fn effects_are_ordered_and_explained() {
        let c = crate::sample::progression_catalog();
        let lines = effect_lines(&[("speed".into(), 0.06), ("armor".into(), 2.0)], &c);
        assert_eq!(lines[0].0, "+2 armour: every hit you take is 2 lower");
        assert_eq!(lines[1].0, "+6% speed");
        assert_eq!(short_effect(&lines[0].0), "+2 armour");
        let stamina = effect_lines(&[("max_energy".into(), 12.0)], &c);
        assert_eq!(short_effect(&stamina[0].0), "+12 max stamina");
    }

    #[test]
    fn a_skill_tooltip_says_what_it_opens() {
        let c = crate::sample::progression_catalog();
        let lines = skill_tooltip(c.node("vitality").unwrap(), None, &c);
        assert!(lines.iter().any(|(t, _, _)| t == "Opens Hardened carapace"));
        assert!(lines.iter().any(|(t, _, _)| t.contains("+18 max health")));
    }

    #[test]
    fn an_item_tooltip_names_its_set() {
        let c = crate::sample::progression_catalog();
        let item = c.item("warden_crest").unwrap();
        let lines = item_tooltip(item, None, 3, &c);
        assert_eq!(lines[1].0, "Rare \u{b7} Head \u{b7} Level 3/5");
        assert!(lines.last().unwrap().0.starts_with("Warden set 0/5"));
    }
}
