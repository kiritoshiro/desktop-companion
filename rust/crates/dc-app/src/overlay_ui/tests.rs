//! Headless tests of the UI layer: real `CreatureManager`s built from the
//! shipped preset with a throw-away state folder, every surface driven
//! through `egui::Context::run_pass` with `RawInput` clicks and keys, and
//! read back through the accesskit tree. No window is opened.

use std::path::{Path, PathBuf};

use dc_core::PyRandom;
use dc_manager::CreatureManager;
use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect};

use super::context_menu::{BaseInfo, MenuContext};
use super::surfaces::{Surface, SurfaceId, UiCore, View};
use super::*;

// -- fixtures ------------------------------------------------------------------

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("dc-overlay-ui-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// The default preset's spiders on a 1920x1080 desktop, saving into `state`.
fn manager(state: &Path) -> (CreatureManager, PyRandom) {
    let mut rng = PyRandom::from_int(7);
    let preset = root().join("presets/default.json");
    let m = CreatureManager::new(&root(), &preset, (1920.0, 1080.0), Some(7), state, false, &mut rng)
        .expect("the default preset loads");
    assert!(m.creatures.len() >= 2, "the default preset has spiders");
    (m, rng)
}

fn first_key(m: &CreatureManager) -> String {
    m.creatures[0].progression_id.clone()
}

fn creature<'a>(m: &'a CreatureManager, key: &str) -> &'a dc_creature::Creature {
    m.creatures.iter().find(|c| c.progression_id == key).expect("spider")
}

fn core(state: &Path) -> UiCore {
    let mut core = UiCore::new(state.to_path_buf());
    core.paint_portraits = false;
    core
}

// -- a headless driver -----------------------------------------------------------

/// Frames, clicks and keys on one surface's own egui context.
#[derive(Default)]
struct Driver {
    time: f64,
    pending: Vec<Event>,
    labels: Vec<String>,
    nodes: Vec<(egui::accesskit::Role, String, bool, Option<bool>)>,
}

impl Driver {
    fn frame(&mut self, core: &mut UiCore, id: SurfaceId) {
        let Some(surface) = core.surface(id) else {
            self.pending.clear();
            return;
        };
        let ctx = surface.ctx.clone();
        let size = surface.size;
        ctx.enable_accesskit();
        dc_ui::probe::enable(&ctx);
        self.time += 1.0 / 30.0;
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            time: Some(self.time),
            events: std::mem::take(&mut self.pending),
            ..Default::default()
        };
        let mut output = ctx.run_pass(input, |c| core.draw(id, c));
        output.textures_delta.clear();
        self.labels.clear();
        self.nodes.clear();
        if let Some(update) = &output.platform_output.accesskit_update {
            for (_, node) in &update.nodes {
                let label = node.label().or(node.value()).unwrap_or("").to_owned();
                let toggled = node.toggled().map(|t| t == egui::accesskit::Toggled::True);
                if !label.is_empty() {
                    self.labels.push(label.clone());
                }
                self.nodes.push((node.role(), label, node.is_disabled(), toggled));
            }
        }
    }

    fn settle(&mut self, core: &mut UiCore, id: SurfaceId) {
        for _ in 0..3 {
            self.frame(core, id);
        }
    }

    fn probe(&self, core: &UiCore, id: SurfaceId, key: &str) -> Rect {
        let ctx = &core.surface(id).expect("open").ctx;
        dc_ui::probe::rect(ctx, key).unwrap_or_else(|| panic!("no {key:?}; recorded: {:?}", dc_ui::probe::keys(ctx)))
    }

    fn click_at(&mut self, core: &mut UiCore, id: SurfaceId, pos: Pos2) {
        self.pending.push(Event::PointerMoved(pos));
        self.frame(core, id);
        self.pending.push(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        self.frame(core, id);
        self.pending.push(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        self.frame(core, id);
    }

    fn click(&mut self, core: &mut UiCore, id: SurfaceId, key: &str) {
        let rect = self.probe(core, id, key);
        self.click_at(core, id, rect.center());
    }

    fn hover(&mut self, core: &mut UiCore, id: SurfaceId, key: &str) {
        let rect = self.probe(core, id, key);
        self.pending.push(Event::PointerMoved(rect.center()));
        self.frame(core, id);
        self.frame(core, id);
    }

    fn key(&mut self, core: &mut UiCore, id: SurfaceId, key: egui::Key) {
        for pressed in [true, false] {
            self.pending.push(Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Modifiers::NONE,
            });
            self.frame(core, id);
        }
    }

    fn type_text(&mut self, core: &mut UiCore, id: SurfaceId, text: &str) {
        self.pending.push(Event::Text(text.to_owned()));
        self.frame(core, id);
    }

    fn has_label(&self, text: &str) -> bool {
        self.labels.iter().any(|l| l == text)
    }

    fn label_containing(&self, text: &str) -> Option<&String> {
        self.labels.iter().find(|l| l.contains(text))
    }

    fn toggled(&self, label: &str) -> Option<bool> {
        self.nodes.iter().find(|n| n.1 == label).and_then(|n| n.3)
    }
}

fn open_id(core: &UiCore, kind: &str) -> SurfaceId {
    core.find(kind).unwrap_or_else(|| panic!("no {kind} open; open: {:?}", kinds(core)))
}

fn kinds(core: &UiCore) -> Vec<&'static str> {
    core.surfaces.iter().map(|s: &Surface| s.view.kind()).collect()
}

fn menu_ids(core: &UiCore, id: SurfaceId) -> Vec<String> {
    match &core.surface(id).unwrap().view {
        View::Menu { menu, .. } => menu.ids(),
        _ => panic!("not a menu"),
    }
}

const LABELS: [&str; 5] =
    ["always_show_names", "always_show_levels", "always_show_health", "always_show_xp", "always_show_stamina"];

// -- the right-click menu ----------------------------------------------------------

#[test]
fn the_spider_menu_lists_pythons_actions_in_order() {
    let dir = TempDir::new("spider-menu");
    let (m, _) = manager(&dir.0);
    let key = first_key(&m);
    let mut core = core(&dir.0);
    core.request(UiRequest::SpiderMenu { creature: key.clone(), at: (400.0, 300.0) });
    assert!(core.is_open());
    let id = open_id(&core, "menu");
    assert!(!core.surface(id).unwrap().ready, "a menu waits for the scene");
    core.sync(&m);
    assert!(core.surface(id).unwrap().ready);

    let name_entries: Vec<String> = if creature(&m, &key).name.is_empty() {
        vec!["name".into()]
    } else {
        vec!["rename".into(), "clear_name".into()]
    };
    let mut expected: Vec<String> = vec!["inspect".into(), "pin_level".into()];
    expected.extend(name_entries);
    expected.push("skills".into());
    expected.extend(dc_content::skills::SKILLS.iter().map(|s| format!("skill:{}", s.id)));
    expected.push("add_cage".into());
    expected.extend(LABELS.iter().map(|s| (*s).to_owned()));
    assert_eq!(menu_ids(&core, id), expected);

    let mut d = Driver::default();
    d.settle(&mut core, id);
    let title = format!("SPIDER  \u{b7}  {}", creature(&m, &key).display_name());
    assert!(d.has_label(&title), "{:?}", d.labels);
    assert!(d.has_label("Inspect progression, inventory, and stats\u{2026}"));
    assert!(d.has_label("Add a cage here"));
    assert_eq!(d.toggled("Always show names"), Some(m.always_show_names));
    assert_eq!(d.toggled("Always show XP bars"), Some(m.always_show_xp));
    // The size is the content's, taller than the starting guess.
    assert!(core.surface(id).unwrap().size.y > 300.0, "{:?}", core.surface(id).unwrap().size);
}

#[test]
fn the_overlay_menu_has_cages_and_label_switches_only() {
    let dir = TempDir::new("overlay-menu");
    let (mut m, _) = manager(&dir.0);
    m.add_cage(None, None, 360.0, 260.0);
    let mut core = core(&dir.0);
    core.request(UiRequest::OverlayMenu { at: (5.0, 5.0) });
    core.sync(&m);
    let id = open_id(&core, "menu");
    let mut expected = vec!["add_cage".to_owned(), "remove_cages".to_owned()];
    expected.extend(LABELS.iter().map(|s| (*s).to_owned()));
    assert_eq!(menu_ids(&core, id), expected);
}

#[test]
fn bases_come_first_and_a_carried_base_puts_its_destination_on_top() {
    let base = BaseInfo { id: "base-1".into(), team: "Pack A".into() };
    let other = BaseInfo { id: "base-2".into(), team: "Rivals".into() };
    let ctx = MenuContext {
        local: (10.0, 20.0),
        base_here: Some(base.clone()),
        carried: Some(other.clone()),
        any_bases: true,
        ..Default::default()
    };
    let ids: Vec<String> = ctx.entries().iter().filter_map(|e| e.id().map(str::to_owned)).collect();
    assert_eq!(
        ids[..6],
        ["drop_base", "cancel_move", "move_base", "remove_base", "remove_bases", "add_cage"].map(String::from)
    );
    let texts: Vec<String> = ctx.entries().iter().filter_map(|e| e.text().map(str::to_owned)).collect();
    assert!(texts.contains(&"Put the Rivals base down here".to_owned()), "{texts:?}");
    assert!(texts.contains(&"Move the Pack A base\u{2026}".to_owned()));
    // Right-clicking the carried base itself offers to leave it.
    let here = MenuContext { carried: Some(base.clone()), ..ctx.clone() };
    let ids: Vec<String> = here.entries().iter().filter_map(|e| e.id().map(str::to_owned)).collect();
    assert_eq!(ids[..3], ["cancel_move_here", "remove_base", "remove_bases"].map(String::from));
    // Choosing the drop moves it to the click.
    assert_eq!(
        ctx.choose("drop_base", None),
        super::context_menu::Choice::DropBase(vec![EngineCommand::OverlayAction {
            action: "move_base=base-2@10,20".into()
        }])
    );
}

#[test]
fn clicking_menu_entries_yields_engine_commands() {
    let dir = TempDir::new("menu-clicks");
    let (m, _) = manager(&dir.0);
    let key = first_key(&m);
    let pinned = creature(&m, &key).level_label_pinned();
    let mut core = core(&dir.0);
    core.origin = (-1920, 0);
    let mut d = Driver::default();

    // "Pin level above name" toggles the pin.
    core.request(UiRequest::SpiderMenu { creature: key.clone(), at: (-1500.0, 300.0) });
    core.sync(&m);
    let id = open_id(&core, "menu");
    d.settle(&mut core, id);
    d.click(&mut core, id, "menu:pin_level");
    assert!(core.find("menu").is_none(), "choosing closes the menu");
    assert_eq!(
        core.take_commands(),
        vec![EngineCommand::SpiderAction {
            creature: key.clone(),
            action: "pin_level".into(),
            arg: Some(actions::on_off(!pinned).into())
        }]
    );

    // "Add a cage here" at the click, in the overlay's own coordinates.
    core.request(UiRequest::SpiderMenu { creature: key.clone(), at: (-1500.0, 300.0) });
    core.sync(&m);
    let id = open_id(&core, "menu");
    d.settle(&mut core, id);
    d.click(&mut core, id, "menu:add_cage");
    assert_eq!(core.take_commands(), vec![EngineCommand::OverlayAction { action: "add_cage=420,300".into() }]);

    // A label switch.
    core.request(UiRequest::OverlayMenu { at: (0.0, 0.0) });
    core.sync(&m);
    let id = open_id(&core, "menu");
    d.settle(&mut core, id);
    d.click(&mut core, id, "menu:always_show_xp");
    assert_eq!(
        core.take_commands(),
        vec![EngineCommand::OverlayAction { action: format!("always_show_xp={}", actions::on_off(!m.always_show_xp)) }]
    );

    // The skills submenu opens in place; a skill toggles.
    core.request(UiRequest::SpiderMenu { creature: key.clone(), at: (-1500.0, 300.0) });
    core.sync(&m);
    let id = open_id(&core, "menu");
    d.settle(&mut core, id);
    let closed = core.surface(id).unwrap().size.y;
    d.click(&mut core, id, "menu:skills");
    assert!(core.surface(id).unwrap().size.y > closed + 100.0, "the submenu grows the menu");
    d.frame(&mut core, id);
    let wander = creature(&m, &key).has_skill("wander");
    d.click(&mut core, id, "menu:skill:wander");
    assert_eq!(
        core.take_commands(),
        vec![EngineCommand::SpiderAction {
            creature: key.clone(),
            action: "skill".into(),
            arg: Some(format!("wander={}", actions::on_off(!wander)))
        }]
    );

    // Esc closes without a command.
    core.request(UiRequest::OverlayMenu { at: (0.0, 0.0) });
    core.sync(&m);
    let id = open_id(&core, "menu");
    d.settle(&mut core, id);
    d.key(&mut core, id, egui::Key::Escape);
    assert!(!core.is_open());
    assert!(core.take_commands().is_empty());
}

/// A right-click on a 150% monitor left of a 100% one: the menu opens at
/// the physical desktop point, its actions use the manager's logical one.
#[test]
fn a_click_on_a_scaled_monitor_reaches_the_manager_in_logical_pixels() {
    use dc_platform::dpi::{DpiMap, ScaledScreen};
    let dir = TempDir::new("menu-dpi");
    let (m, _) = manager(&dir.0);
    let mut core = core(&dir.0);
    core.origin = (-3840, 0);
    core.dpi = DpiMap::new([
        ScaledScreen::new(dc_core::IRect::new(0, 0, 1920, 1080), 1.0),
        ScaledScreen::new(dc_core::IRect::new(-3840, 0, 3840, 2160), 1.5),
    ])
    .translated(3840, 0);
    let mut d = Driver::default();
    // The main monitor keeps its corner (3840 local) in both spaces.
    for (at, cage) in [((-3840.0 + 600.0, 300.0), "add_cage=400,200"), ((100.0, 50.0), "add_cage=3940,50")] {
        core.request(UiRequest::OverlayMenu { at });
        core.sync(&m);
        let id = open_id(&core, "menu");
        d.settle(&mut core, id);
        d.click(&mut core, id, "menu:add_cage");
        assert_eq!(core.take_commands(), vec![EngineCommand::OverlayAction { action: cage.into() }]);
    }
}

#[test]
fn hovering_an_entry_explains_it_in_the_strip() {
    let dir = TempDir::new("menu-tips");
    let (m, _) = manager(&dir.0);
    let mut core = core(&dir.0);
    core.request(UiRequest::SpiderMenu { creature: first_key(&m), at: (100.0, 100.0) });
    core.sync(&m);
    let id = open_id(&core, "menu");
    let mut d = Driver::default();
    d.settle(&mut core, id);
    d.hover(&mut core, id, "menu:add_cage");
    assert!(
        d.has_label("Place a roomy boundary here; spiders inside stay on this part of the desktop."),
        "{:?}",
        d.labels
    );
}

#[test]
fn naming_a_spider_goes_through_the_prompt() {
    let dir = TempDir::new("name-prompt");
    let (m, _) = manager(&dir.0);
    let key = first_key(&m);
    let mut core = core(&dir.0);
    let mut d = Driver::default();
    core.request(UiRequest::SpiderMenu { creature: key.clone(), at: (100.0, 100.0) });
    core.sync(&m);
    let id = open_id(&core, "menu");
    d.settle(&mut core, id);
    let entry = if creature(&m, &key).name.is_empty() { "menu:name" } else { "menu:rename" };
    d.click(&mut core, id, entry);
    let prompt = open_id(&core, "prompt");
    d.settle(&mut core, prompt);
    assert!(d.has_label("Spider name:"), "{:?}", d.labels);
    // The line edit has the focus: select what is there and type over it.
    d.pending.push(Event::Key {
        key: egui::Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    d.frame(&mut core, prompt);
    d.type_text(&mut core, prompt, "Webster");
    d.click(&mut core, prompt, "button:OK");
    assert!(!core.is_open());
    assert_eq!(
        core.take_commands(),
        vec![EngineCommand::SpiderAction { creature: key, action: "rename".into(), arg: Some("Webster".into()) }]
    );
}

#[test]
fn inspect_opens_the_inspector_from_the_menu() {
    let dir = TempDir::new("menu-inspect");
    let (m, _) = manager(&dir.0);
    let mut core = core(&dir.0);
    let mut d = Driver::default();
    core.request(UiRequest::SpiderMenu { creature: first_key(&m), at: (100.0, 100.0) });
    core.sync(&m);
    let id = open_id(&core, "menu");
    d.settle(&mut core, id);
    d.click(&mut core, id, "menu:inspect");
    let inspector = open_id(&core, "inspector");
    assert!(!core.surface(inspector).unwrap().ready, "waits for the scene");
    core.sync(&m);
    assert!(core.surface(inspector).unwrap().ready);
    assert!(core.take_commands().is_empty());
}

// -- spider actions on a real manager -----------------------------------------------------------

#[test]
fn spider_actions_change_the_real_manager() {
    let dir = TempDir::new("spider-actions");
    let (mut m, _) = manager(&dir.0);
    let key = first_key(&m);
    let other = m.creatures[1].progression_id.clone();

    let out = apply_spider_action(&mut m, &key, "rename", Some("  Webster "));
    assert_eq!(creature(&m, &key).name, "Webster");
    assert_eq!(out, vec![EngineCommand::Announce("Named this spider \u{201c}Webster\u{201d}.".into())]);
    apply_spider_action(&mut m, &key, "clear_name", None);
    assert_eq!(creature(&m, &key).name, "");

    apply_spider_action(&mut m, &key, "pin_level", Some("on"));
    assert!(creature(&m, &key).progression.pin_level);
    apply_spider_action(&mut m, &key, "pin_level", None);
    assert!(!creature(&m, &key).progression.pin_level, "no argument flips it");
    apply_spider_action(&mut m, &key, "pin_health", Some("on"));
    assert!(creature(&m, &key).progression.pin_health);

    let had = creature(&m, &key).has_skill("wander");
    apply_spider_action(&mut m, &key, "skill", Some(&format!("wander={}", actions::on_off(!had))));
    assert_eq!(creature(&m, &key).has_skill("wander"), !had);

    let cages = m.cages.len();
    let out = apply_spider_action(&mut m, &key, "cage", None);
    assert_eq!(m.cages.len(), cages + 1);
    assert!(creature(&m, &key).cage.is_some(), "a cage put over the spider holds it: {out:?}");

    apply_spider_action(&mut m, &key, "team", Some("pack_a"));
    assert_eq!(creature(&m, &key).progression.team_id, "pack_a");
    apply_spider_action(&mut m, &key, "relation", Some(&format!("{other}=foe")));
    assert_eq!(creature(&m, &key).progression.relation_overrides.get(&other).map(String::as_str), Some("foe"));

    let piece = dc_state::progression::ARMOR_CATALOG
        .iter()
        .find(|i| !creature(&m, &key).progression.inventory.iter().any(|o| o == i.id))
        .expect("a piece the spider does not own");
    let (item, slot) = (piece.id, piece.slot);
    let out = apply_spider_action(&mut m, &key, "add_item", Some(item));
    assert_eq!(out, vec![EngineCommand::Announce("Added armour to this spider's bag.".into())]);
    assert!(creature(&m, &key).progression.inventory.iter().any(|i| i == item));
    apply_spider_action(&mut m, &key, "equip", Some(item));
    assert_eq!(creature(&m, &key).progression.equipped.get(slot).map(String::as_str), Some(item));
    apply_spider_action(&mut m, &key, "unequip", Some(slot));
    assert!(creature(&m, &key).progression.equipped.get(slot).is_none());

    // Saved in the isolated state folder, nowhere else.
    assert!(dir.0.join("creatures.json").is_file());
    // A spider that has gone does nothing.
    assert!(apply_spider_action(&mut m, "nobody", "rename", Some("x")).is_empty());
}

#[test]
fn overlay_actions_change_the_real_manager() {
    let dir = TempDir::new("overlay-actions");
    let (mut m, mut rng) = manager(&dir.0);
    let out = apply_overlay_action(&mut m, &mut rng, "add_cage=600,400");
    assert_eq!(m.cages.len(), 1);
    assert!(matches!(&out[..], [EngineCommand::Announce(_)]));
    let cage = &m.cages[0];
    assert!((cage.x + cage.w / 2.0 - 600.0).abs() < 1.0 && (cage.y + cage.h / 2.0 - 400.0).abs() < 1.0);
    apply_overlay_action(&mut m, &mut rng, "remove_cages");
    assert!(m.cages.is_empty());

    apply_overlay_action(&mut m, &mut rng, "always_show_names=off");
    assert!(!m.always_show_names);
    apply_overlay_action(&mut m, &mut rng, "always_show_names");
    assert!(m.always_show_names, "no argument flips it");
    apply_overlay_action(&mut m, &mut rng, "naming=off");
    assert!(!m.naming_enabled);
    apply_overlay_action(&mut m, &mut rng, "interferable=off");
    assert!(!m.interferable);
    apply_overlay_action(&mut m, &mut rng, "size=1.25");
    assert_eq!(m.size_scale, 1.25);
    apply_overlay_action(&mut m, &mut rng, "flies_on=off");
    assert!(!m.flies_enabled);
    apply_overlay_action(&mut m, &mut rng, "fly_rate=swarm");
    assert_eq!(m.fly_max, 14);
    assert_eq!(
        apply_overlay_action(&mut m, &mut rng, "fps=30"),
        vec![EngineCommand::SetTargetFps(30.0), EngineCommand::Announce("Performance set to Balanced 30 FPS.".into())]
    );
    let count = m.creatures.len();
    let out = apply_overlay_action(&mut m, &mut rng, "randomize_count");
    assert!(matches!(&out[..], [EngineCommand::Announce(_)]));
    assert!((1..=10).contains(&m.creatures.len()), "{count} -> {}", m.creatures.len());
    assert_eq!(apply_overlay_action(&mut m, &mut rng, "quit"), vec![EngineCommand::Quit]);
    assert!(apply_overlay_action(&mut m, &mut rng, "no_such_thing").is_empty());
}

// -- the tray menu -----------------------------------------------------------------------

#[test]
fn the_tray_menu_mirrors_create_tray() {
    let menu = tray_menu();
    let top: Vec<(String, String)> = menu
        .entries
        .iter()
        .filter_map(|e| match e {
            dc_platform::tray::MenuEntry::Submenu { id, label, .. }
            | dc_platform::tray::MenuEntry::Item { id, label, .. }
            | dc_platform::tray::MenuEntry::Note { id, label } => Some((id.clone(), label.clone())),
            _ => None,
        })
        .collect();
    let labels: Vec<&str> = top.iter().map(|(_, l)| l.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Running - dragging is on.",
            "Performance",
            "Appearance and mood",
            "Randomize creatures",
            "Interaction",
            "Flies",
            "Cages",
            "Help",
            "Quit overlay"
        ]
    );
    let ids = menu.ids();
    for id in [
        "fps=60",
        "fps=45",
        "fps=30",
        "fps=20",
        "size=0.6",
        "size=1",
        "size=1.6",
        "randomize_models",
        "randomize_personalities",
        "randomize_count",
        "randomize_everything",
        "interferable",
        "mouse_web",
        "naming",
        "always_show_names",
        "always_show_stamina",
        "flies_on",
        "fly_rate=sparse",
        "fly_rate=swarm",
        "release_fly",
        "fly_nest",
        "add_nest",
        "reset_nest",
        "add_cage",
        "remove_cages",
        "quit",
    ] {
        assert!(ids.contains(&id.to_owned()), "{id} missing from {ids:?}");
    }
    assert_eq!(menu.groups()["fps"], ["fps=60", "fps=45", "fps=30", "fps=20"]);
    assert_eq!(menu.groups()["size"].len(), 5);
}

#[test]
fn tray_ids_map_to_the_right_commands() {
    assert_eq!(tray_event("quit"), vec![EngineCommand::Quit]);
    assert_eq!(tray_event("fps=45"), vec![EngineCommand::OverlayAction { action: "fps=45".into() }]);
    assert_eq!(tray_event("size=1.25"), vec![EngineCommand::OverlayAction { action: "size=1.25".into() }]);
    assert_eq!(tray_event("add_cage"), vec![EngineCommand::OverlayAction { action: "add_cage".into() }]);
    assert_eq!(tray_event("naming"), vec![EngineCommand::OverlayAction { action: "naming".into() }]);
    assert_eq!(
        tray_event_checked("naming", Some(false)),
        vec![EngineCommand::OverlayAction { action: "naming=off".into() }]
    );
    assert_eq!(
        tray_event_checked("always_show_xp", Some(true)),
        vec![EngineCommand::OverlayAction { action: "always_show_xp=on".into() }]
    );
    for silent in ["status", "help:1", "note:flies", "performance", "fly_rate"] {
        assert!(tray_event(silent).is_empty(), "{silent}");
    }

    // Every actionable tray id does something on a real manager.
    let dir = TempDir::new("tray");
    let (mut m, mut rng) = manager(&dir.0);
    for id in tray_menu().ids() {
        for command in tray_event(&id) {
            match command {
                EngineCommand::OverlayAction { action } => {
                    let out = apply_overlay_action(&mut m, &mut rng, &action);
                    assert!(!out.is_empty(), "{id} did nothing");
                }
                EngineCommand::Quit => assert_eq!(id, "quit"),
                other => panic!("{id}: {other:?}"),
            }
        }
    }
    // And the check marks follow the scene.
    let checks = tray_checks(&m, 45.0);
    assert!(checks.contains(&("fps=45".to_owned(), true)));
    assert!(checks.contains(&("fps=60".to_owned(), false)));
    assert!(checks.contains(&("naming".to_owned(), m.naming_enabled)));
    assert_eq!(tray_menu_for(&m, 45.0).ids(), tray_menu().ids());
}

// -- the inspector ---------------------------------------------------------------------------

#[test]
fn the_inspector_shows_a_live_spiders_level_health_and_xp() {
    let dir = TempDir::new("inspector");
    let (mut m, _) = manager(&dir.0);
    let key = first_key(&m);
    // So the check box shows the spider's own pin.
    m.set_always_show_health(false);
    {
        let c = m.creatures.iter_mut().find(|c| c.progression_id == key).unwrap();
        c.gain_experience(15.0);
        c.hp = (c.max_hp * 0.5).round();
    }
    let c = creature(&m, &key);
    let (level, xp, hp, max_hp) = (c.level(), c.xp(), c.hp, c.max_hp);
    let (name, health_pinned) = (c.display_name(), c.health_label_pinned());
    let other_name = m.creatures[1].display_name();
    let need = dc_state::progression::xp_to_next_level(level);
    let mut core = core(&dir.0);
    core.paint_portraits = true;
    core.request(UiRequest::Inspector { creature: key.clone() });
    core.sync(&m);
    let id = open_id(&core, "inspector");
    assert_eq!(core.surface(id).unwrap().title, format!("Inspect {name}"));
    let mut d = Driver::default();
    d.settle(&mut core, id);
    assert!(d.has_label(&format!("Level {level}  \u{b7}  {xp} / {need} XP")), "{:?}", d.labels);
    assert!(d.has_label(&format!("Health {hp:.0} / {max_hp:.0}")), "{:?}", d.labels);
    assert!(d.has_label(&format!("Portrait of {name}")));
    assert!(d.has_label(&format!("Level {level}")), "the level badge");
    // The other spider is listed for relations.
    assert!(d.has_label(&other_name));

    // A change made in the window goes to the engine as a spider action.
    d.click(&mut core, id, "check:health-pin");
    let commands = core.take_commands();
    assert_eq!(
        commands,
        vec![EngineCommand::SpiderAction {
            creature: key.clone(),
            action: "pin_health".into(),
            arg: Some(actions::on_off(!health_pinned).into())
        }]
    );
    for command in commands {
        if let EngineCommand::SpiderAction { creature, action, arg } = command {
            apply_spider_action(&mut m, &creature, &action, arg.as_deref());
        }
    }
    assert_eq!(creature(&m, &key).progression.pin_health, !health_pinned);

    // Done closes it; a spider leaving the desktop closes it too.
    d.click(&mut core, id, "button:Done");
    assert!(!core.is_open());
    core.request(UiRequest::Inspector { creature: key.clone() });
    core.sync(&m);
    m.creatures.retain(|c| c.progression_id != key);
    let id = open_id(&core, "inspector");
    std::thread::sleep(surfaces::INSPECTOR_REFRESH);
    core.sync(&m);
    d.frame(&mut core, id);
    assert!(!core.is_open(), "the inspector of a spider that left closes");
}

#[test]
fn portraits_are_painted_from_the_live_spider() {
    let dir = TempDir::new("portrait");
    let (m, _) = manager(&dir.0);
    let mut painter = live::PortraitPainter::new();
    let image = painter.portrait(&m.creatures[0], live::PORTRAIT);
    assert_eq!(image.size(), (2 * live::PORTRAIT, 2 * live::PORTRAIT));
    let opaque = image.data().chunks(4).filter(|p| p[3] > 128).count();
    assert!(opaque > 500, "the spider is drawn ({opaque} opaque pixels)");
    // The middle holds the body; the corners are empty.
    assert_eq!(image.pixel(0, 0).a, 0);
}

// -- the pause menu --------------------------------------------------------------------------

fn pause(core: &mut UiCore, d: &mut Driver) -> SurfaceId {
    core.request(UiRequest::Pause);
    let id = open_id(core, "pause");
    d.settle(core, id);
    id
}

#[test]
fn pause_resume_exit_and_restart() {
    let dir = TempDir::new("pause");
    let mut core = core(&dir.0);
    let mut d = Driver::default();

    let id = pause(&mut core, &mut d);
    for label in ["Resume", "Character & armour", "Settings", "Restart raid", "Retry saving victory", "Exit raid"] {
        assert!(d.has_label(label), "{label} in {:?}", d.labels);
    }
    // FitHeight: the dialog measured its own height.
    assert!(core.surface(id).unwrap().size.y > 250.0);
    d.click(&mut core, id, "pause:resume");
    assert!(!core.is_open());
    assert_eq!(core.take_commands(), vec![EngineCommand::Resume]);

    let id = pause(&mut core, &mut d);
    d.click(&mut core, id, "pause:restart");
    assert_eq!(core.take_commands(), vec![EngineCommand::RestartMission, EngineCommand::Resume]);

    let id = pause(&mut core, &mut d);
    d.click(&mut core, id, "pause:exit");
    assert_eq!(core.take_commands(), vec![EngineCommand::SaveProgress, EngineCommand::ExitMission]);

    // "Retry saving victory" saves and the menu stays.
    let id = pause(&mut core, &mut d);
    d.click(&mut core, id, "pause:save");
    assert_eq!(core.take_commands(), vec![EngineCommand::SaveProgress]);
    assert_eq!(core.find("pause"), Some(id));
    // Esc is "resume"; a second Esc request while paused is ignored.
    core.request(UiRequest::Pause);
    assert_eq!(core.surfaces.len(), 1);
    d.key(&mut core, id, egui::Key::Escape);
    assert_eq!(core.take_commands(), vec![EngineCommand::Resume]);
    assert!(!core.is_open());
}

#[test]
fn pause_outside_a_raid_offers_release() {
    let dir = TempDir::new("pause-free");
    let mut core = core(&dir.0);
    core.adventure.in_mission = false;
    let mut d = Driver::default();
    let id = pause(&mut core, &mut d);
    assert!(d.has_label("Release spider") && d.has_label("Save and exit"), "{:?}", d.labels);
    d.click(&mut core, id, "pause:release");
    assert_eq!(core.take_commands(), vec![EngineCommand::Resume]);
}

#[test]
fn pause_settings_apply_the_frame_rate_and_come_back() {
    let dir = TempDir::new("pause-settings");
    let mut core = core(&dir.0);
    core.adventure.target_fps = 60.0;
    let mut d = Driver::default();
    let id = pause(&mut core, &mut d);
    d.click(&mut core, id, "pause:settings");
    let settings = open_id(&core, "settings");
    d.settle(&mut core, settings);
    assert!(d.has_label("Frame rate"), "{:?}", d.labels);
    if let Some(View::Settings { dialog, .. }) = core.surface_mut(settings).map(|s| &mut s.view) {
        dialog.fps = 30;
    }
    d.click(&mut core, settings, "button:Apply and return");
    assert_eq!(core.take_commands(), vec![EngineCommand::SetTargetFps(30.0), EngineCommand::ControlsChanged]);
    assert!(core.find("pause").is_some(), "the pause menu comes back");

    // Closed with Esc: no frame-rate change, controls still re-read.
    let id = open_id(&core, "pause");
    d.settle(&mut core, id);
    d.click(&mut core, id, "pause:settings");
    let settings = open_id(&core, "settings");
    d.settle(&mut core, settings);
    d.key(&mut core, settings, egui::Key::Escape);
    assert_eq!(core.take_commands(), vec![EngineCommand::ControlsChanged]);
}

// -- character & armour ------------------------------------------------------------------------

#[test]
fn the_character_window_saves_first_and_reports_profile_changed() {
    let dir = TempDir::new("character");
    let mut core = core(&dir.0);
    let mut d = Driver::default();
    core.request(UiRequest::Character { player: 0 });
    assert!(core.is_open(), "waiting counts as open");
    assert!(core.find("character").is_none(), "not before the progress is saved");
    core.promote_pending();
    assert!(core.find("character").is_none());
    assert_eq!(core.take_commands(), vec![EngineCommand::SaveProgress]);
    core.promote_pending();
    let id = open_id(&core, "character");
    d.settle(&mut core, id);
    assert!(d.label_containing("XP").is_some(), "{:?}", d.labels);

    // A rename is written to the profile in the state folder at once.
    let name_rect = d.probe(&core, id, "name:hero");
    d.click_at(&mut core, id, name_rect.center());
    d.pending.push(Event::Key {
        key: egui::Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    d.frame(&mut core, id);
    d.type_text(&mut core, id, "Bramble");
    d.key(&mut core, id, egui::Key::Enter);
    let profile = dc_adventure::adventure_profile::load_profile(&dc_adventure::adventure_profile::profile_path(&dir.0));
    assert_eq!(profile.name, "Bramble");

    d.click(&mut core, id, "button:Done");
    assert!(!core.is_open());
    assert_eq!(core.take_commands(), vec![EngineCommand::ProfileChanged]);
}

#[test]
fn two_players_get_the_party_window() {
    let dir = TempDir::new("party");
    let mut core = core(&dir.0);
    core.adventure.players = 2;
    core.request(UiRequest::Character { player: 1 });
    core.take_commands();
    core.promote_pending();
    let id = open_id(&core, "character");
    assert!(matches!(
        &core.surface(id).unwrap().view,
        View::Character { window, .. } if matches!(**window, dc_ui::windows::CharacterWindow::Party(_))
    ));
    let mut d = Driver::default();
    d.settle(&mut core, id);
    d.key(&mut core, id, egui::Key::Escape);
    assert_eq!(core.take_commands(), vec![EngineCommand::ProfileChanged]);
}

#[test]
fn character_from_the_pause_menu_returns_to_it() {
    let dir = TempDir::new("pause-character");
    let mut core = core(&dir.0);
    let mut d = Driver::default();
    let id = pause(&mut core, &mut d);
    d.click(&mut core, id, "pause:skills");
    assert_eq!(core.take_commands(), vec![EngineCommand::SaveProgress]);
    assert!(core.is_open());
    core.promote_pending();
    let id = open_id(&core, "character");
    d.settle(&mut core, id);
    d.click(&mut core, id, "button:Done");
    assert_eq!(core.take_commands(), vec![EngineCommand::ProfileChanged]);
    assert!(core.find("pause").is_some(), "back to the pause menu");
}

#[test]
fn character_outside_a_raid_is_the_inspectors_skill_tree() {
    let dir = TempDir::new("character-free");
    let (m, _) = manager(&dir.0);
    let mut core = core(&dir.0);
    core.adventure =
        AdventureContext { in_mission: false, player_creatures: vec![first_key(&m)], ..Default::default() };
    core.request(UiRequest::Character { player: 0 });
    let id = open_id(&core, "inspector");
    assert!(matches!(
        &core.surface(id).unwrap().view,
        View::Inspector { inspector, .. } if inspector.tab == dc_ui::windows::inspector::InspectorTab::Skills
    ));
    assert!(core.take_commands().is_empty(), "no raid progress to save");
}

// -- controls ------------------------------------------------------------------------------

#[test]
fn the_controls_editor_reports_controls_changed() {
    let dir = TempDir::new("controls");
    let mut core = core(&dir.0);
    let mut d = Driver::default();
    core.request(UiRequest::Controls);
    let id = open_id(&core, "controls");
    d.settle(&mut core, id);
    assert!(d.label_containing("Movement").is_some(), "{:?}", d.labels);
    d.key(&mut core, id, egui::Key::Escape);
    assert!(!core.is_open());
    assert_eq!(core.take_commands(), vec![EngineCommand::ControlsChanged]);
}

#[test]
fn close_all_closes_everything() {
    let dir = TempDir::new("close-all");
    let mut core = core(&dir.0);
    core.request(UiRequest::Controls);
    core.request(UiRequest::Pause);
    core.request(UiRequest::Character { player: 0 });
    assert!(core.is_open());
    core.request(UiRequest::CloseAll);
    assert!(!core.is_open());
}

#[test]
fn a_dismissed_window_counts_as_esc() {
    let dir = TempDir::new("dismiss");
    let mut core = core(&dir.0);
    core.request(UiRequest::Pause);
    let id = open_id(&core, "pause");
    core.dismiss(id);
    assert_eq!(core.take_commands(), vec![EngineCommand::Resume]);
}
