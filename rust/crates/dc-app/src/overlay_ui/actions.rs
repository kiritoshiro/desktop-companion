//! What the overlay's menus do to the scene: `app/engine.py`'s menu
//! handlers (`_show_context_menu`, `create_tray`, `_add_label_switches`)
//! and the inspector's (`app/inspector_ui.py`), as calls on
//! [`CreatureManager`].
//!
//! Every handler that ended in `self._announce(message)` (or the tray's
//! `announce`) returns [`EngineCommand::Announce`]; the engine shows the
//! balloon and puts the text on the tray's status line.
//!
//! Action names. Spider actions ([`apply_spider_action`]) take the spider's
//! progression id and an optional argument; overlay actions
//! ([`apply_overlay_action`]) are one string, `name` or `name=argument`. A
//! switch without an argument flips the manager's current setting (the tray
//! reports a click, not always the new state).

use dc_core::PyRandom;
use dc_manager::CreatureManager;
use dc_platform::tray::TrayMenu;

use super::EngineCommand;

/// `_add_label_switches`: (text, action, emblem, tooltip), in menu order.
pub const LABEL_SWITCHES: [(&str, &str, &str, &str); 5] = [
    ("Always show names", "always_show_names", "name", "Show every spider's name without having to hover it."),
    ("Always show levels", "always_show_levels", "pin", "Show every spider's level beside its name."),
    (
        "Always show health bars",
        "always_show_health",
        "health",
        "Show every spider's health bar, not just ones pinned individually.",
    ),
    (
        "Always show XP bars",
        "always_show_xp",
        "xp",
        "Show every spider's progress to its next level as a line under its level.",
    ),
    (
        "Always show stamina bars",
        "always_show_stamina",
        "stamina",
        "Show every spider's stamina (energy) under its health bar.",
    ),
];

/// `fps_options` of the tray's Performance menu.
pub const FPS_OPTIONS: [(&str, u32); 4] =
    [("Smooth 60 FPS", 60), ("Still smooth 45 FPS", 45), ("Balanced 30 FPS", 30), ("Battery 20 FPS", 20)];

/// `size_options` of "Creature size".
pub const SIZE_OPTIONS: [(&str, f64); 5] = [
    ("Tiny (60%)", 0.60),
    ("Small (80%)", 0.80),
    ("Normal (100%)", 1.00),
    ("Large (125%)", 1.25),
    ("Huge (160%)", 1.60),
];

/// `rate_options` of Flies > Spawn rate.
pub const RATE_OPTIONS: [(&str, &str); 3] = [
    ("Sparse (every 9-18s, up to 3)", "sparse"),
    ("Normal (every 4-9s, up to 6)", "normal"),
    ("Swarm (every 1-3s, up to 14)", "swarm"),
];

/// The tray's on/off entries (checkable, not in a group).
pub const TRAY_SWITCHES: [&str; 9] = [
    "interferable",
    "mouse_web",
    "naming",
    "always_show_names",
    "always_show_levels",
    "always_show_health",
    "always_show_xp",
    "always_show_stamina",
    "flies_on",
];

/// A label switch's current value.
pub fn label_switch(manager: &CreatureManager, action: &str) -> bool {
    match action {
        "always_show_names" => manager.always_show_names,
        "always_show_levels" => manager.always_show_levels,
        "always_show_health" => manager.always_show_health,
        "always_show_xp" => manager.always_show_xp,
        "always_show_stamina" => manager.always_show_stamina,
        _ => false,
    }
}

/// `"on"` / `"off"` for an argument.
pub fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

/// An on/off argument; `None` (no argument) flips `current`.
fn switch_value(arg: Option<&str>, current: bool) -> bool {
    match arg.map(|a| a.trim().to_lowercase()) {
        None => !current,
        Some(a) => matches!(a.as_str(), "on" | "1" | "true" | "yes"),
    }
}

/// `"x,y"` as numbers.
pub fn parse_point(arg: &str) -> Option<(f64, f64)> {
    let (x, y) = arg.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

fn announce(message: String) -> Vec<EngineCommand> {
    vec![EngineCommand::Announce(message)]
}

/// The status line the tray menu opens with (`initial_status`).
pub fn initial_status(manager: &CreatureManager) -> &'static str {
    if manager.interferable { "Running - dragging is on." } else { "Running - clicks pass through spiders." }
}

/// Carry out one spider action on the spider whose progression id is
/// `creature`, as the Python menu and inspector handlers did:
///
/// | action | argument | Python |
/// |---|---|---|
/// | `pin_level` | on/off | "Pin level above name", inspector check |
/// | `pin_health` | on/off | inspector "Health bar" check |
/// | `rename` | the name (`""` clears) | `_apply_name` |
/// | `clear_name` | — | "Clear name" |
/// | `skill` | `<skill id>=on/off` | "Skills for this spider" |
/// | `team` | team id | inspector team picker |
/// | `relation` | `<other spider's id>=friend/neutral/foe` | inspector relations |
/// | `unlock` | ability id | inspector skill tree |
/// | `equip` / `unequip` | item id / slot | inspector doll |
/// | `add_item` | item id | inspector "Add to bag" |
/// | `cage` | optional `x,y` | "Add a cage here" over the spider |
///
/// A spider that has left the desktop, or an unknown action, does nothing.
pub fn apply_spider_action(
    manager: &mut CreatureManager,
    creature: &str,
    action: &str,
    arg: Option<&str>,
) -> Vec<EngineCommand> {
    let Some(c) = manager.creatures.iter().find(|c| c.progression_id == creature) else {
        log::debug!("spider action {action} for a spider that is gone ({creature})");
        return Vec::new();
    };
    let id = c.id;
    let (x, y) = (c.x(), c.y());
    let level_pinned = c.progression.pin_level;
    let health_pinned = c.progression.pin_health;
    match action {
        "pin_level" => announce(manager.set_creature_level_pin(id, switch_value(arg, level_pinned))),
        "pin_health" => announce(manager.set_creature_health_pin(id, switch_value(arg, health_pinned))),
        "rename" => announce(manager.name_creature(id, arg.unwrap_or(""))),
        "clear_name" => announce(manager.name_creature(id, "")),
        "skill" => {
            let Some(arg) = arg else { return Vec::new() };
            let (skill, state) = arg.rsplit_once('=').unwrap_or((arg, ""));
            let has = manager.creature(id).is_some_and(|c| c.has_skill(skill));
            let on = if state.is_empty() { !has } else { switch_value(Some(state), has) };
            announce(manager.set_creature_skill(id, skill, on))
        }
        "team" => match arg {
            Some(team) => announce(manager.set_creature_team(id, team)),
            None => Vec::new(),
        },
        "relation" => {
            let Some((other, relation)) = arg.and_then(|a| a.rsplit_once('=')) else { return Vec::new() };
            let Some(other_id) = manager.creatures.iter().find(|o| o.progression_id == other).map(|o| o.id) else {
                return Vec::new();
            };
            announce(manager.set_creature_relation(id, other_id, relation))
        }
        "unlock" => match arg {
            Some(ability) => announce(manager.unlock_creature_ability(id, ability)),
            None => Vec::new(),
        },
        "equip" => match arg {
            Some(item) => announce(manager.equip_creature_item(id, item)),
            None => Vec::new(),
        },
        "unequip" => match arg {
            Some(slot) => announce(manager.unequip_creature_item(id, slot)),
            None => Vec::new(),
        },
        "add_item" => {
            let Some(item) = arg else { return Vec::new() };
            let added = manager.creature_mut(id).is_some_and(|c| c.add_inventory_item(item));
            if !added {
                return Vec::new();
            }
            if let Err(e) = manager.save_runtime_state() {
                log::warn!("Could not save after adding armour: {e}");
            }
            announce("Added armour to this spider's bag.".into())
        }
        "cage" => {
            let (cx, cy) = arg.and_then(parse_point).unwrap_or((x, y));
            announce(manager.add_cage(Some(cx), Some(cy), 360.0, 260.0))
        }
        other => {
            log::warn!("Unknown spider action {other:?}");
            Vec::new()
        }
    }
}

/// Carry out one overlay action (`name` or `name=argument`): the tray menu's
/// handlers and the base, cage and label entries of the right-click menu.
///
/// `fps=N`, `size=S`, `randomize_models`, `randomize_personalities`,
/// `randomize_count`, `randomize_everything`, `interferable`, `mouse_web`,
/// `naming`, `always_show_{names,levels,health,xp,stamina}`, `flies_on`,
/// `fly_rate=sparse|normal|swarm`, `release_fly`, `fly_nest`, `add_nest`,
/// `reset_nest`, `add_cage[=x,y]`, `remove_cages`, `remove_base=<id>`,
/// `move_base=<id>@x,y`, `remove_bases`, `quit`. Switches take `=on`/`=off`.
pub fn apply_overlay_action(manager: &mut CreatureManager, rng: &mut PyRandom, action: &str) -> Vec<EngineCommand> {
    let (name, arg) = match action.split_once('=') {
        Some((n, a)) => (n.trim(), Some(a.trim())),
        None => (action.trim(), None),
    };
    match name {
        "fps" => {
            let Some(fps) = arg.and_then(|a| a.parse::<u32>().ok()) else { return Vec::new() };
            let label = FPS_OPTIONS.iter().find(|(_, f)| *f == fps).map(|(l, _)| *l).unwrap_or("custom");
            vec![
                EngineCommand::SetTargetFps(fps as f64),
                EngineCommand::Announce(format!("Performance set to {label}.")),
            ]
        }
        "size" => {
            let Some(scale) = arg.and_then(|a| a.parse::<f64>().ok()) else { return Vec::new() };
            manager.set_size_scale(scale);
            let label = SIZE_OPTIONS
                .iter()
                .find(|(_, s)| (s - scale).abs() < 1e-6)
                .map(|(l, _)| (*l).to_owned())
                .unwrap_or_else(|| format!("{:.0}%", scale * 100.0));
            announce(format!("Size set to {label}."))
        }
        "randomize_models" => announce(manager.randomize_models(rng)),
        "randomize_personalities" => announce(manager.randomize_personalities(rng)),
        "randomize_count" => announce(manager.randomize_count(1, 10, rng)),
        "randomize_everything" => announce(manager.randomize_everything(rng)),
        "interferable" => {
            let on = switch_value(arg, manager.interferable);
            manager.set_interferable(on);
            announce(
                if on {
                    "Dragging enabled; spiders can be grabbed."
                } else {
                    "Dragging disabled; clicks pass through spiders."
                }
                .into(),
            )
        }
        "mouse_web" => {
            let on = switch_value(arg, manager.allow_mouse_capture);
            announce(manager.set_allow_mouse_capture(on))
        }
        "naming" => {
            let on = switch_value(arg, manager.naming_enabled);
            announce(manager.set_naming_enabled(on))
        }
        "always_show_names"
        | "always_show_levels"
        | "always_show_health"
        | "always_show_xp"
        | "always_show_stamina" => {
            let on = switch_value(arg, label_switch(manager, name));
            let message = match name {
                "always_show_names" => manager.set_always_show_names(on),
                "always_show_levels" => manager.set_always_show_levels(on),
                "always_show_health" => manager.set_always_show_health(on),
                "always_show_xp" => manager.set_always_show_xp(on),
                _ => manager.set_always_show_stamina(on),
            };
            announce(message)
        }
        "flies_on" => {
            let on = switch_value(arg, manager.flies_enabled);
            announce(manager.set_flies_enabled(on))
        }
        "fly_rate" => match arg {
            Some(rate) => announce(manager.set_fly_rate_preset(rate)),
            None => Vec::new(),
        },
        "release_fly" => announce(manager.release_fly()),
        "fly_nest" => {
            let on = switch_value(arg, manager.flies_spawner);
            announce(manager.set_fly_spawner_enabled(on))
        }
        "add_nest" => announce(manager.add_fly_spawner()),
        "reset_nest" => announce(manager.reset_fly_spawner()),
        "add_cage" => {
            let at = arg.and_then(parse_point);
            announce(manager.add_cage(at.map(|p| p.0), at.map(|p| p.1), 360.0, 260.0))
        }
        "remove_cages" => announce(manager.remove_cages()),
        "remove_base" => match arg {
            Some(id) => announce(manager.remove_base(id)),
            None => Vec::new(),
        },
        "move_base" => {
            let Some((id, point)) = arg.and_then(|a| a.rsplit_once('@')) else { return Vec::new() };
            let Some((x, y)) = parse_point(point) else { return Vec::new() };
            announce(manager.move_base(id, x, y))
        }
        "remove_bases" => announce(manager.remove_bases()),
        "quit" => vec![EngineCommand::Quit],
        other => {
            log::warn!("Unknown overlay action {other:?}");
            Vec::new()
        }
    }
}

// -- the tray menu (`create_tray`) -------------------------------------------

/// The tray menu with the manager's defaults (`CreatureManager::empty`) and
/// 60 FPS. Prefer [`tray_menu_for`] once the manager exists.
pub fn tray_menu() -> TrayMenu {
    build_tray_menu(&TrayState::default())
}

/// The tray menu with every check mark as the scene is now.
pub fn tray_menu_for(manager: &CreatureManager, target_fps: f64) -> TrayMenu {
    build_tray_menu(&TrayState::of(manager, target_fps))
}

/// The tray's check marks as the scene is now, `(id, checked)`: call after
/// applying any command, and give each to `Tray::set_checked` (the
/// right-click menu changes the same settings).
pub fn tray_checks(manager: &CreatureManager, target_fps: f64) -> Vec<(String, bool)> {
    let s = TrayState::of(manager, target_fps);
    let mut out: Vec<(String, bool)> =
        FPS_OPTIONS.iter().map(|(_, f)| (format!("fps={f}"), (s.fps - *f as f64).abs() <= 2.0)).collect();
    out.extend(SIZE_OPTIONS.iter().map(|(_, v)| (format!("size={v}"), (v - s.size).abs() < 1e-6)));
    out.extend(TRAY_SWITCHES.iter().map(|id| ((*id).to_owned(), s.switch(id))));
    out.push(("fly_nest".into(), s.fly_nest));
    out
}

/// What a tray click means. `id` is a [`tray_menu`] id; check items flip the
/// current setting.
pub fn tray_event(id: &str) -> Vec<EngineCommand> {
    tray_event_checked(id, None)
}

/// [`tray_event`] with the check item's new state when the tray reports it
/// (`TrayEvent::Menu { checked }`).
pub fn tray_event_checked(id: &str, checked: Option<bool>) -> Vec<EngineCommand> {
    let switch = TRAY_SWITCHES.contains(&id) || id == "fly_nest";
    let action = match (switch, checked) {
        (true, Some(c)) => format!("{id}={}", on_off(c)),
        _ => id.to_owned(),
    };
    let known = switch
        || id.starts_with("fps=")
        || id.starts_with("size=")
        || id.starts_with("fly_rate=")
        || matches!(
            id,
            "randomize_models"
                | "randomize_personalities"
                | "randomize_count"
                | "randomize_everything"
                | "release_fly"
                | "add_nest"
                | "reset_nest"
                | "add_cage"
                | "remove_cages"
        );
    if id == "quit" {
        vec![EngineCommand::Quit]
    } else if known {
        vec![EngineCommand::OverlayAction { action }]
    } else {
        // Notes, submenu headings and the status line.
        Vec::new()
    }
}

struct TrayState {
    fps: f64,
    size: f64,
    interferable: bool,
    mouse_web: bool,
    naming: bool,
    labels: [bool; 5],
    flies_on: bool,
    fly_nest: bool,
}

impl Default for TrayState {
    fn default() -> Self {
        // `CreatureManager::empty`'s settings.
        Self {
            fps: 60.0,
            size: 1.0,
            interferable: true,
            mouse_web: true,
            naming: true,
            labels: [true, true, true, false, true],
            flies_on: true,
            fly_nest: true,
        }
    }
}

impl TrayState {
    fn of(m: &CreatureManager, fps: f64) -> Self {
        Self {
            fps,
            size: m.size_scale,
            interferable: m.interferable,
            mouse_web: m.allow_mouse_capture,
            naming: m.naming_enabled,
            labels: [
                m.always_show_names,
                m.always_show_levels,
                m.always_show_health,
                m.always_show_xp,
                m.always_show_stamina,
            ],
            flies_on: m.flies_enabled,
            fly_nest: m.flies_spawner,
        }
    }

    fn switch(&self, id: &str) -> bool {
        match id {
            "interferable" => self.interferable,
            "mouse_web" => self.mouse_web,
            "naming" => self.naming,
            "flies_on" => self.flies_on,
            "fly_nest" => self.fly_nest,
            other => LABEL_SWITCHES.iter().position(|(_, a, _, _)| *a == other).is_some_and(|i| self.labels[i]),
        }
    }
}

fn build_tray_menu(s: &TrayState) -> TrayMenu {
    let status = if s.interferable { "Running - dragging is on." } else { "Running - clicks pass through spiders." };
    let mut performance = TrayMenu::new();
    for (label, fps) in FPS_OPTIONS {
        performance = performance.radio(&format!("fps={fps}"), label, (s.fps - fps as f64).abs() <= 2.0, "fps");
    }
    let performance = performance.note("note:performance", "Tip: fewer/lower-size spiders matter more than FPS.");

    let mut size = TrayMenu::new();
    for (label, scale) in SIZE_OPTIONS {
        size = size.radio(&format!("size={scale}"), label, (scale - s.size).abs() < 1e-6, "size");
    }
    let appearance = TrayMenu::new().submenu("size", "Creature size", size);

    let randomize = TrayMenu::new()
        .note("note:randomize", "Changes apply immediately to the running overlay.")
        .separator()
        .item("randomize_models", "Randomize models")
        .item("randomize_personalities", "Randomize personalities")
        .item("randomize_count", "Randomize count (1-10)")
        .separator()
        .item("randomize_everything", "Surprise me: randomize everything");

    let mut interaction = TrayMenu::new()
        .check("interferable", "Allow dragging spiders", s.interferable)
        .check("mouse_web", "Let spiders web-trap the mouse", s.mouse_web)
        .check("naming", "Right-click a spider to name it", s.naming);
    for (i, (text, action, _, _)) in LABEL_SWITCHES.iter().enumerate() {
        interaction = interaction.check(action, text, s.labels[i]);
    }

    let mut rate = TrayMenu::new();
    for (label, key) in RATE_OPTIONS {
        rate = rate.radio(&format!("fly_rate={key}"), label, false, "fly_rate");
    }
    let rate = rate.note("note:fly_rate", "Fine-grained spawn timing lives in the settings window.");
    let flies = TrayMenu::new()
        .note("note:flies", "Flies buzz around for the spiders to hunt, trap, and eat.")
        .separator()
        .check("flies_on", "Flies on", s.flies_on)
        .separator()
        .submenu("fly_rate", "Spawn rate", rate)
        .item("release_fly", "Release a fly now")
        .separator()
        .check("fly_nest", "Flies emerge from a nest", s.fly_nest)
        .item("add_nest", "Add another nest")
        .item("reset_nest", "Reset nest position");

    let cages = TrayMenu::new()
        .note("note:cages", "A cage keeps any spider you drop inside from wandering out.")
        .separator()
        .item("add_cage", "Add a cage")
        .item("remove_cages", "Remove all cages");

    let help = TrayMenu::new()
        .note("help:1", "Right-click this tray icon for controls.")
        .note("help:2", "Empty overlay space stays click-through.")
        .note("help:3", "Enable dragging to grab a spider by its body.")
        .note("help:4", "Right-click a spider to name it; hover to see the name.")
        .note("help:5", "Add a cage, then drop spiders inside to keep them there.");

    TrayMenu::new()
        .note("status", status)
        .separator()
        .submenu("performance", "Performance", performance)
        .submenu("appearance", "Appearance and mood", appearance)
        .submenu("randomize", "Randomize creatures", randomize)
        .submenu("interaction", "Interaction", interaction)
        .submenu("flies", "Flies", flies)
        .submenu("cages", "Cages", cages)
        .submenu("help", "Help", help)
        .separator()
        .item("quit", "Quit overlay")
}
