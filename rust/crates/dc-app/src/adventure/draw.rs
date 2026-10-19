//! Painting an Adventure frame: `OverlayWindow._paint` with a mission.
//!
//! The order is the Python's: the mission's painted map (map view) or the
//! frozen Reclaim desktop, the buildings, the manager's frame (spiders,
//! webs, flies, carcasses), the player HUDs and aim marks, the mission HUD
//! (objective, commands, minimap, desktop health, blue screen or end
//! title), and the released banner on top. The dc-art painters read plain
//! structs, filled here from the mission and the manager.

use super::{AdventureGame, spider};
use dc_adventure::PlayerController;
use dc_adventure::adventure::ControlledSpider;
use dc_adventure::controls::Action;
use dc_adventure::mission::{MissionHost, MissionSpider, TerritoryMission};
use dc_adventure::mission_factory::Mission;
use dc_adventure::reclaim::ReclaimMission;
use dc_art::mission_backdrop::{BackdropScreen, BackdropSite, MissionBackdrop};
use dc_art::mission_ui::{
    HudPlayer, MissionView, PlayerAim, Relation, UiActor, UiAlly, UiCreature, UiGlob, UiLayout, UiLink, UiLoot,
    UiScreen, UiSite, UiSplash, UiThread, draw_buildings, draw_cursor_aim, draw_hud, draw_mission_hud,
    draw_player_aims, draw_released_banner,
};
use dc_core::Rect;
use dc_creature::Creature;
use dc_manager::paint::ScenePainter;
use dc_paint::PaintSwitches;
use dc_render::{Canvas, Image, RenderHint};
use dc_world::screen_layout::LinkKind;
use std::collections::HashMap;

/// `draw_hud`'s view of one controlled spider and its controller.
pub fn hud_player(c: &Creature, control: &PlayerController) -> HudPlayer {
    let b = |a: Action| control.controls.binding(a).to_string();
    HudPlayer {
        display_name: MissionSpider::display_name(c),
        level: MissionSpider::progression(c).level,
        dead: ControlledSpider::dead(c),
        hp: c.hp,
        max_hp: c.max_hp,
        energy: c.energy,
        max_energy: c.max_energy,
        attack_cooldown: c.attack_cooldown,
        silk: control.silk,
        silk_capacity: control.silk_capacity as f64,
        jump_cooldown: control.jump_cooldown,
        web_cooldown: control.web_cooldown,
        jump_energy: PlayerController::JUMP_ENERGY,
        web_energy: PlayerController::WEB_ENERGY,
        feedback: control.feedback.clone(),
        feedback_time: control.feedback_time,
        bindings: Some([b(Action::Jump), b(Action::Shoot), b(Action::Bite), b(Action::Skills)]),
    }
}

/// Paints Adventure frames; keeps the backdrop pictures and loot icons.
pub struct AdventureDrawer {
    pub backdrop: MissionBackdrop,
    pub painter: ScenePainter,
    icons: HashMap<String, Image>,
}

impl AdventureDrawer {
    pub fn new(switches: PaintSwitches) -> Self {
        Self { backdrop: MissionBackdrop::new(), painter: ScenePainter::new(switches), icons: HashMap::new() }
    }

    fn loot_icon(&mut self, item_id: &str) -> (String, Option<Image>) {
        let Some(item) = dc_state::progression::armor_by_id(item_id) else {
            return (String::new(), None);
        };
        let icon = self
            .icons
            .entry(item_id.to_string())
            .or_insert_with(|| dc_paint::armour_art::armour_icon(item.id, item.slot, 64))
            .clone();
        (item.tier.to_string(), Some(icon))
    }

    /// What `draw_buildings` and `draw_mission_hud` read from the mission.
    pub fn mission_view(&mut self, game: &AdventureGame) -> MissionView {
        let host = &game.manager;
        let m = game.territory();
        let hero = spider(host, m.hero);
        let layout = UiLayout {
            screens: m
                .layout
                .screens
                .iter()
                .map(|s| UiScreen {
                    index: s.index,
                    name: s.name(),
                    rect: Rect::new(s.rect.x, s.rect.y, s.rect.w, s.rect.h),
                })
                .collect(),
            links: m
                .layout
                .links
                .iter()
                .map(|l| UiLink {
                    a: l.a,
                    b: l.b,
                    tunnel: l.kind == LinkKind::Tunnel,
                    a_point: l.a_point,
                    b_point: l.b_point,
                })
                .collect(),
        };
        let here = m.layout.screen_at(ControlledSpider::x(hero), ControlledSpider::y(hero)).index;
        let sites = m
            .sites
            .iter()
            .map(|s| UiSite {
                x: s.x,
                y: s.y,
                kind: s.kind.clone(),
                name: s.name.clone(),
                owned: s.owned,
                contested: s.contested,
                warning: s.warning,
                supply: s.supply,
                reserves: s.reserves,
                progress: s.progress,
            })
            .collect();
        let actors = m
            .actors
            .iter()
            .map(|a| {
                let c = spider(host, a.creature);
                UiActor {
                    x: ControlledSpider::x(c),
                    y: ControlledSpider::y(c),
                    size: ControlledSpider::size(c),
                    windup: a.windup,
                }
            })
            .collect();
        let loot = m
            .loot
            .iter()
            .map(|l| {
                let (tier, icon) = self.loot_icon(&l.item_id);
                UiLoot { x: l.x, y: l.y, age: l.age, tier, icon }
            })
            .collect();
        let hazards = m
            .hazards
            .iter()
            .map(|g| {
                let (x, y, height) = g.position();
                UiGlob { x, y, height }
            })
            .collect();
        let splashes = m.splashes.iter().map(|s| UiSplash { x: s.x, y: s.y, age: s.age }).collect();
        let creatures = host
            .creatures()
            .into_iter()
            .map(|id| {
                let c = spider(host, id);
                let relation = if id == m.hero {
                    Relation::Hero
                } else if TerritoryMission::foes(host, m.hero, id) {
                    Relation::Foe
                } else {
                    Relation::Friend
                };
                UiCreature {
                    x: ControlledSpider::x(c),
                    y: ControlledSpider::y(c),
                    dead: ControlledSpider::dead(c),
                    relation,
                }
            })
            .collect();
        let allies = m
            .allies
            .iter()
            .map(|&id| {
                let c = spider(host, id);
                UiAlly {
                    display_name: MissionSpider::display_name(c),
                    hp: c.hp,
                    max_hp: c.max_hp,
                    dead: ControlledSpider::dead(c),
                }
            })
            .collect();
        let controls = &m.players[0].control.controls;
        let mut view = MissionView {
            area: Rect::new(m.area.x, m.area.y, m.area.w, m.area.h),
            layout: Some(layout),
            here,
            sites,
            actors,
            loot,
            hazards,
            splashes,
            threads: Vec::new(),
            creatures,
            objective: game.mission.objective(host),
            notice: m.notice.clone(),
            notice_time: m.notice_time,
            state: m.state.clone(),
            save_error: m.save_error.clone(),
            ended: m.ended(),
            end_clock: m.end_clock,
            end_screen_seconds: TerritoryMission::END_SCREEN_SECONDS,
            command: m.command.clone(),
            allies,
            amber_mined: m.amber_mined,
            found: m.found.len(),
            caught: 0,
            reward_text: m.reward_text.clone(),
            words_eaten: 0,
            hero_name: MissionSpider::display_name(hero),
            hero_level: MissionSpider::progression(hero).level,
            surface: None,
            intro_time: 0.0,
            blue_screen: 0.0,
            crash_screen_seconds: ReclaimMission::CRASH_SCREEN_SECONDS,
            command_bindings: Some([
                controls.binding(Action::CompanionFollow).to_string(),
                controls.binding(Action::CompanionDefend).to_string(),
                controls.binding(Action::CompanionAttack).to_string(),
            ]),
        };
        match &game.mission {
            Mission::Swarm(s) => view.caught = s.caught,
            Mission::Reclaim(r) => {
                view.words_eaten = r.words_eaten as i64;
                view.surface = Some(r.surface.hud());
                view.intro_time = r.intro_time;
                view.blue_screen = r.blue_screen();
                view.threads = r
                    .threads
                    .iter()
                    .map(|t| {
                        let c = spider(host, t.eater);
                        UiThread {
                            x0: t.x,
                            y0: t.y,
                            spider_x: ControlledSpider::x(c),
                            spider_y: ControlledSpider::y(c),
                            age: t.age,
                        }
                    })
                    .collect();
            }
            Mission::Territory(_) => {}
        }
        view
    }

    /// The mission's painted map over every screen (the map view).
    pub fn paint_backdrop(&mut self, game: &AdventureGame, p: &mut Canvas) {
        let m = game.territory();
        let screens: Vec<BackdropScreen> = m
            .layout
            .screens
            .iter()
            .map(|s| BackdropScreen { index: s.index, rect: Rect::new(s.rect.x, s.rect.y, s.rect.w, s.rect.h) })
            .collect();
        let sites: Vec<BackdropSite> =
            m.sites.iter().map(|s| BackdropSite { x: s.x, y: s.y, kind: s.kind.clone(), screen: s.screen }).collect();
        self.backdrop.paint(p, &m.map_info.id, m.map_info.kind.id(), &screens, &sites);
    }

    /// One whole Adventure frame onto `p` (overlay pixels, cleared to
    /// transparent by the caller).
    pub fn paint(&mut self, game: &mut AdventureGame, p: &mut Canvas) {
        p.set_render_hint(RenderHint::Antialiasing, true);
        let view = self.mission_view(game);
        let window = game.hud_window();
        let show_map = game.map_view && !game.controls_released;
        if show_map {
            // The mission's own painted ground instead of the desktop.
            self.paint_backdrop(game, p);
        }
        if let Mission::Reclaim(r) = &game.mission
            && !show_map
            && !game.controls_released
        {
            // The frozen desktop under everything (the live desktop shows
            // while the controls are released).
            r.surface.paint(p);
        }
        draw_buildings(p, &view);
        self.painter.paint(&mut game.manager, p, None);
        let m = game.territory();
        let host = &game.manager;
        for (index, player) in m.players.iter().enumerate().take(2) {
            draw_hud(p, &window, &hud_player(spider(host, player.creature), &player.control), index);
        }
        if m.players.len() > 1 {
            let aims: Vec<PlayerAim> = m
                .players
                .iter()
                .map(|player| {
                    let c = spider(host, player.creature);
                    PlayerAim {
                        x: ControlledSpider::x(c),
                        y: ControlledSpider::y(c),
                        heading: ControlledSpider::heading(c),
                        size: ControlledSpider::size(c),
                        dead: ControlledSpider::dead(c),
                    }
                })
                .collect();
            draw_player_aims(p, &aims);
        } else {
            draw_cursor_aim(p, (game.cursor.0.round() as i32, game.cursor.1.round() as i32));
        }
        draw_mission_hud(p, &window, &view);
        if game.controls_released {
            let key = game.controls.binding(Action::Release).to_string();
            draw_released_banner(p, &window, &key, Some(view.area));
        }
    }
}
