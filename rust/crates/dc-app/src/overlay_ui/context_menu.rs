//! The overlay's right-click menu (`OverlayWindow._show_context_menu` in
//! `app/engine.py`): the spider's own entries when one was clicked, then
//! the base under the cursor (or the base being carried), the cages, and the
//! label switches. The scene is read once, when the menu opens, into a
//! [`MenuContext`]; the entries and what choosing one means are pure
//! functions of it.

use dc_manager::CreatureManager;
use dc_ui::windows::menu::{MenuEntry, MenuIcon};

use super::actions::{LABEL_SWITCHES, label_switch, on_off};
use super::{CreatureKey, EngineCommand};

/// A skill row of "Skills for this spider".
#[derive(Clone, Debug, PartialEq)]
pub struct SkillRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub on: bool,
}

/// The right-clicked spider.
#[derive(Clone, Debug, PartialEq)]
pub struct SpiderInfo {
    pub key: CreatureKey,
    /// `creature.name or creature.display_name`.
    pub title: String,
    /// `creature.name` (empty when it has none).
    pub name: String,
    pub level_pinned: bool,
    pub skills: Vec<SkillRow>,
}

/// A base, by id, with its team's name.
#[derive(Clone, Debug, PartialEq)]
pub struct BaseInfo {
    pub id: String,
    pub team: String,
}

/// What the menu needs to know about the scene.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MenuContext {
    /// The click, in the manager's (overlay-local) coordinates.
    pub local: (f64, f64),
    pub spider: Option<SpiderInfo>,
    /// The base under the cursor (`manager.base_at`).
    pub base_here: Option<BaseInfo>,
    /// The base picked up with "Move the … base…" (`_carried_base`).
    pub carried: Option<BaseInfo>,
    pub any_bases: bool,
    pub any_cages: bool,
    /// `always_show_*`, in [`LABEL_SWITCHES`] order.
    pub labels: [bool; 5],
}

fn base_info(manager: &CreatureManager, site: &dc_world::jobs::BaseSite) -> BaseInfo {
    BaseInfo { id: site.id.clone(), team: dc_state::teams::team_label(&site.team_id, Some(&manager.team_profiles)) }
}

impl MenuContext {
    /// Read the scene. `creature` is the clicked spider's progression id,
    /// `carried` the id of a base being moved.
    pub fn from_manager(
        manager: &CreatureManager,
        creature: Option<&str>,
        local: (f64, f64),
        carried: Option<&str>,
    ) -> Self {
        let spider =
            creature.and_then(|key| manager.creatures.iter().find(|c| c.progression_id == key)).map(|c| SpiderInfo {
                key: c.progression_id.clone(),
                title: c.display_name(),
                name: c.name.clone(),
                level_pinned: c.level_label_pinned(),
                skills: dc_content::skills::SKILLS
                    .iter()
                    .map(|s| SkillRow {
                        id: s.id.to_owned(),
                        name: s.display_name.to_owned(),
                        description: s.description.to_owned(),
                        on: c.has_skill(s.id),
                    })
                    .collect(),
            });
        let base_here = manager.base_at(local.0, local.1).map(|s| base_info(manager, s));
        // Removed while it was being carried: forget it.
        let carried =
            carried.and_then(|id| manager.base_world.bases.iter().find(|s| s.id == id)).map(|s| base_info(manager, s));
        let mut labels = [false; 5];
        for (i, (_, action, _, _)) in LABEL_SWITCHES.iter().enumerate() {
            labels[i] = label_switch(manager, action);
        }
        Self {
            local,
            spider,
            base_here,
            carried,
            any_bases: !manager.base_world.bases.is_empty(),
            any_cages: !manager.cages.is_empty(),
            labels,
        }
    }

    /// The menu, entry by entry, as Python built it.
    pub fn entries(&self) -> Vec<MenuEntry> {
        let mut out = Vec::new();
        if let Some(s) = &self.spider {
            out.push(MenuEntry::title(format!("SPIDER  \u{b7}  {}", s.title), MenuIcon::emblem("companion")));
            out.push(MenuEntry::Separator);
            out.push(
                MenuEntry::item(
                    "inspect",
                    "Inspect progression, inventory, and stats\u{2026}",
                    MenuIcon::emblem("inspect"),
                )
                .tip("View level, XP, skill tree, armor, health, energy, and team relations."),
            );
            out.push(MenuEntry::check("pin_level", "Pin level above name", MenuIcon::emblem("pin"), s.level_pinned));
            out.push(MenuEntry::Separator);
            if !s.name.is_empty() {
                out.push(
                    MenuEntry::item(
                        "rename",
                        format!("Rename \u{201c}{}\u{201d}\u{2026}", s.name),
                        MenuIcon::emblem("rename"),
                    )
                    .tip("Give this spider a name shown above it on the desktop."),
                );
                out.push(
                    MenuEntry::item("clear_name", "Clear name", MenuIcon::emblem("remove"))
                        .tip("Remove its custom name; the model name can still be shown."),
                );
            } else {
                out.push(
                    MenuEntry::item("name", "Name this spider\u{2026}", MenuIcon::emblem("name"))
                        .tip("Give this spider a name shown above it on the desktop."),
                );
            }
            let skills = s
                .skills
                .iter()
                .map(|k| {
                    MenuEntry::check(
                        format!("skill:{}", k.id),
                        k.name.clone(),
                        Some(MenuIcon::Skill { id: k.id.clone(), learned: k.on }),
                        k.on,
                    )
                    .tip(k.description.clone())
                })
                .collect();
            out.push(MenuEntry::submenu("skills", "Skills for this spider", MenuIcon::emblem("skills"), skills));
            out.push(MenuEntry::Separator);
        }
        let carried_elsewhere = self.carried.as_ref().filter(|c| self.base_here.as_ref() != Some(*c));
        if let Some(carried) = carried_elsewhere {
            out.push(
                MenuEntry::item(
                    "drop_base",
                    format!("Put the {} base down here", carried.team),
                    MenuIcon::emblem("move"),
                )
                .tip("Move the base you picked up to this spot, earth and all."),
            );
            out.push(MenuEntry::item("cancel_move", "Leave it where it is", MenuIcon::emblem("remove")));
            out.push(MenuEntry::Separator);
        }
        if let Some(base) = &self.base_here {
            if self.carried.as_ref() == Some(base) {
                out.push(MenuEntry::item(
                    "cancel_move_here",
                    format!("Leave the {} base where it is", base.team),
                    MenuIcon::emblem("remove"),
                ));
            } else {
                out.push(
                    MenuEntry::item(
                        "move_base",
                        format!("Move the {} base\u{2026}", base.team),
                        MenuIcon::emblem("move"),
                    )
                    .tip(
                        "Pick this base up, then right-click where it should go. \
                             It keeps its level, its food and the earth already dug. \
                             You can also just drag the earth with the left button, \
                             when dragging is switched on.",
                    ),
                );
            }
            out.push(
                MenuEntry::item("remove_base", format!("Remove the {} base here", base.team), MenuIcon::emblem("base"))
                    .tip("Delete this base. Its team keeps its spiders and its food; a Builder will found a new one."),
            );
            out.push(MenuEntry::Separator);
        }
        if self.any_bases {
            out.push(
                MenuEntry::item("remove_bases", "Remove every base", MenuIcon::emblem("remove"))
                    .tip("Clear the desktop of colonies. Builders start again from nothing."),
            );
            out.push(MenuEntry::Separator);
        }
        out.push(
            MenuEntry::item("add_cage", "Add a cage here", MenuIcon::emblem("cage"))
                .tip("Place a roomy boundary here; spiders inside stay on this part of the desktop."),
        );
        if self.any_cages {
            out.push(
                MenuEntry::item("remove_cages", "Remove all cages", MenuIcon::emblem("remove"))
                    .tip("Remove every boundary so the spiders can roam freely again."),
            );
        }
        out.push(MenuEntry::Separator);
        for (i, (text, action, icon, tip)) in LABEL_SWITCHES.iter().enumerate() {
            out.push(MenuEntry::check(*action, *text, MenuIcon::emblem(icon), self.labels[i]).tip(*tip));
        }
        out
    }

    /// What choosing entry `id` does (`checked`: a check entry's new state).
    pub fn choose(&self, id: &str, checked: Option<bool>) -> Choice {
        let spider = |action: &str, arg: Option<String>| -> Choice {
            match &self.spider {
                Some(s) => Choice::Commands(vec![EngineCommand::SpiderAction {
                    creature: s.key.clone(),
                    action: action.to_owned(),
                    arg,
                }]),
                None => Choice::Nothing,
            }
        };
        let overlay = |action: String| Choice::Commands(vec![EngineCommand::OverlayAction { action }]);
        let (x, y) = self.local;
        match id {
            "inspect" => match &self.spider {
                Some(s) => Choice::Inspect(s.key.clone()),
                None => Choice::Nothing,
            },
            "pin_level" => {
                let on = checked.unwrap_or(!self.spider.as_ref().is_some_and(|s| s.level_pinned));
                spider("pin_level", Some(on_off(on).to_owned()))
            }
            "rename" | "name" => match &self.spider {
                Some(s) => Choice::PromptName { creature: s.key.clone(), current: s.name.clone() },
                None => Choice::Nothing,
            },
            "clear_name" => spider("rename", Some(String::new())),
            "drop_base" => match &self.carried {
                Some(b) => Choice::DropBase(vec![EngineCommand::OverlayAction {
                    action: format!("move_base={}@{x},{y}", b.id),
                }]),
                None => Choice::Nothing,
            },
            "cancel_move" | "cancel_move_here" => Choice::CancelBaseMove,
            "move_base" => match &self.base_here {
                Some(b) => Choice::PickUpBase(b.clone()),
                None => Choice::Nothing,
            },
            "remove_base" => match &self.base_here {
                Some(b) => overlay(format!("remove_base={}", b.id)),
                None => Choice::Nothing,
            },
            "remove_bases" => overlay("remove_bases".into()),
            "add_cage" => overlay(format!("add_cage={x},{y}")),
            "remove_cages" => overlay("remove_cages".into()),
            other => {
                if let Some(skill) = other.strip_prefix("skill:") {
                    let current = self
                        .spider
                        .as_ref()
                        .and_then(|s| s.skills.iter().find(|k| k.id == skill))
                        .is_some_and(|k| k.on);
                    let on = checked.unwrap_or(!current);
                    return spider("skill", Some(format!("{skill}={}", on_off(on))));
                }
                if let Some(i) = LABEL_SWITCHES.iter().position(|(_, a, _, _)| *a == other) {
                    let on = checked.unwrap_or(!self.labels[i]);
                    return overlay(format!("{other}={}", on_off(on)));
                }
                Choice::Nothing
            }
        }
    }
}

/// What a menu choice asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Choice {
    Nothing,
    /// Commands for the engine.
    Commands(Vec<EngineCommand>),
    /// Open the inspector (`_show_inspector`).
    Inspect(CreatureKey),
    /// Ask for a name (`_prompt_name`).
    PromptName {
        creature: CreatureKey,
        current: String,
    },
    /// "Move the … base…" (`_pick_up_base`).
    PickUpBase(BaseInfo),
    /// "Leave it where it is" (`_cancel_base_move`).
    CancelBaseMove,
    /// "Put the … base down here" (`_drop_base`): forget the carried base
    /// and move it.
    DropBase(Vec<EngineCommand>),
}
