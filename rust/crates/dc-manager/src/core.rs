//! Port of `manager/core.py`: construction, input and the sequential frame loop.
use crate::{cages::CageDrag, surfaces::DesktopSurface};
use dc_content::OrderedMap;
use dc_core::PyRandom;
use dc_creature::{
    ApproxLabelMetrics, Creature, CreatureConfig, CreatureId, Env, LabelMetrics, Population, UpdateOutcome,
};
use dc_state::{progression::Stances, teams::TeamProfiles};
use dc_world::{
    BaseWorld, FlyWorld, MouseWebWorld, Playfield, WebWorld, cage::Cage, carcass::Carcass, flies::FlyWorldConfig,
};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct MouseInput {
    pub down: bool,
    pub pressed: bool,
    pub released: bool,
}

pub struct CreatureManager {
    pub screen_w: f64,
    pub screen_h: f64,
    pub seed: Option<i64>,
    pub creatures: Vec<Creature>,
    pub playfield: Playfield,
    pub models: OrderedMap<Value>,
    pub personalities: OrderedMap<Value>,
    pub warnings: Vec<String>,
    pub web_world: WebWorld,
    pub fly_world: FlyWorld,
    pub mouse_web_world: MouseWebWorld,
    pub base_world: BaseWorld,
    pub cages: Vec<Cage>,
    pub carcasses: Vec<Carcass>,
    pub size_scale: f64,
    pub interferable: bool,
    pub mood_mode: String,
    pub social_play: bool,
    pub conflict_enabled: bool,
    pub gait_style: Option<String>,
    pub naming_enabled: bool,
    pub always_show_names: bool,
    pub always_show_levels: bool,
    pub always_show_health: bool,
    pub always_show_xp: bool,
    pub always_show_stamina: bool,
    pub team_stances: Arc<Stances>,
    pub team_profiles: Arc<TeamProfiles>,
    pub flies_enabled: bool,
    pub fly_min_interval: f64,
    pub fly_max_interval: f64,
    pub fly_max: i64,
    pub flies_spawner: bool,
    pub allow_mouse_capture: bool,
    pub desktop_icons_enabled: bool,
    pub desktop_surfaces: Vec<DesktopSurface>,
    pub dragged_creature: Option<CreatureId>,
    pub desired_cursor: Option<(f64, f64)>,
    pub runtime_state_dirty: bool,
    pub(crate) cage_drag: Option<CageDrag>,
    pub(crate) dragged_fly: Option<dc_world::FlyId>,
    pub(crate) dragged_spawner: Option<usize>,
    pub(crate) dragged_base: Option<String>,
    pub(crate) base_drag_offset: (f64, f64),
    pub(crate) render_order: Vec<CreatureId>,
    pub(crate) render_sort_accum: f64,
    pub(crate) neighbor_refresh_accum: f64,
    pub(crate) mouse_x: f64,
    pub(crate) mouse_y: f64,
    pub(crate) mouse_down: bool,
    pub(crate) raise_timers: HashMap<String, f64>,
    pub(crate) slot_ids: HashMap<CreatureId, String>,
    pub(crate) progression_states: serde_json::Map<String, Value>,
    pub(crate) state_launch: i64,
    pub(crate) progression_namespace: String,
    pub(crate) state_folder: PathBuf,
    pub(crate) preset_data: Value,
    pub(crate) runtime_state_flush_accum: f64,
    pub(crate) id_rng: PyRandom,
    pub(crate) next_id: u64,
    /// Label text measurement (Qt's `QFontMetrics`). The binary sets real font
    /// metrics; headless use keeps the estimate. Labels widen the bounding box
    /// that decides hiding behind windows.
    pub label_metrics: Box<dyn LabelMetrics>,
    /// The model and personality every mission hero is built from: the first
    /// live preset spider when the first mission began (`mission.py` took the
    /// previous mission's hero, which was built from the same pair).
    pub mission_source: Option<(Value, Value)>,
    /// Mission spiders taken out by `bury_the_dead`, kept so the mission can
    /// still read the ids it holds (its hero, guardian, targets).
    pub mission_graveyard: Vec<Creature>,
}
impl CreatureManager {
    /// Empty scene, useful for missions and tests. All OS capabilities are inputs.
    pub fn empty(
        screen_w: f64,
        screen_h: f64,
        seed: Option<i64>,
        state_folder: &Path,
        can_control_cursor: bool,
    ) -> Self {
        let world_rng =
            |name: &str| seed.map_or_else(PyRandom::from_entropy, |s| PyRandom::from_str_seed(&format!("{s}:{name}")));
        let playfield = Playfield::whole(screen_w, screen_h);
        let mut fly_world = FlyWorld::new(screen_w, screen_h, FlyWorldConfig::default(), world_rng("flies"));
        fly_world.playfield = Some(playfield.clone());
        let mut base_world = BaseWorld::new(screen_w, screen_h, &[]);
        base_world.playfield = Some(playfield.clone());
        Self {
            screen_w,
            screen_h,
            seed,
            creatures: vec![],
            playfield,
            models: OrderedMap::new(),
            personalities: OrderedMap::new(),
            warnings: vec![],
            web_world: WebWorld::new(screen_w, screen_h, world_rng("webs")),
            fly_world,
            mouse_web_world: MouseWebWorld::new(screen_w, screen_h, can_control_cursor, world_rng("mouse-webs")),
            base_world,
            cages: vec![],
            carcasses: vec![],
            size_scale: 1.0,
            interferable: true,
            mood_mode: "auto".into(),
            social_play: true,
            conflict_enabled: true,
            gait_style: None,
            naming_enabled: true,
            always_show_names: true,
            always_show_levels: true,
            always_show_health: true,
            always_show_xp: false,
            always_show_stamina: true,
            team_stances: Arc::new(Stances::new()),
            team_profiles: Arc::new(TeamProfiles::new()),
            flies_enabled: true,
            fly_min_interval: 4.0,
            fly_max_interval: 9.0,
            fly_max: 6,
            flies_spawner: true,
            allow_mouse_capture: true,
            desktop_icons_enabled: false,
            desktop_surfaces: vec![],
            dragged_creature: None,
            desired_cursor: None,
            runtime_state_dirty: false,
            cage_drag: None,
            dragged_fly: None,
            dragged_spawner: None,
            dragged_base: None,
            base_drag_offset: (0.0, 0.0),
            render_order: vec![],
            render_sort_accum: 0.0,
            neighbor_refresh_accum: 0.0,
            mouse_x: -100000.0,
            mouse_y: -100000.0,
            mouse_down: false,
            raise_timers: HashMap::new(),
            slot_ids: HashMap::new(),
            progression_states: serde_json::Map::new(),
            state_launch: 1,
            progression_namespace: "default".into(),
            state_folder: state_folder.into(),
            preset_data: Value::Null,
            runtime_state_flush_accum: 0.0,
            id_rng: world_rng("creature-ids"),
            next_id: 1,
            label_metrics: Box::new(ApproxLabelMetrics),
            mission_source: None,
            mission_graveyard: Vec::new(),
        }
    }
    /// Discover content from an explicit root and restore `state_folder/creatures.json`.
    pub fn new(
        root: &Path,
        preset: &Path,
        screen: (f64, f64),
        seed: Option<i64>,
        state_folder: &Path,
        can_control_cursor: bool,
        rng: &mut PyRandom,
    ) -> Result<Self, dc_content::ContentError> {
        let mut m = Self::empty(screen.0, screen.1, seed, state_folder, can_control_cursor);
        let (models, mut warnings) = dc_content::discovery::discover_models(Some(root));
        let (personalities, pwarnings) = dc_content::discovery::discover_personalities(Some(root));
        warnings.extend(pwarnings);
        for (k, v) in models.iter() {
            m.models.insert(k, v.to_value());
        }
        for (k, v) in personalities.iter() {
            m.personalities.insert(k, v.to_value());
        }
        m.warnings = warnings;
        let scene = m.load_runtime_state();
        m.load_preset(preset, rng)?;
        m.restore_scene(&scene);
        Ok(m)
    }
    /// Replace the label text metrics (the binary passes its font shaper).
    pub fn set_label_metrics(&mut self, metrics: Box<dyn LabelMetrics>) {
        self.label_metrics = metrics;
    }
    pub fn creature(&self, id: CreatureId) -> Option<&Creature> {
        self.creatures.iter().find(|c| c.id == id)
    }
    pub fn creature_mut(&mut self, id: CreatureId) -> Option<&mut Creature> {
        self.creatures.iter_mut().find(|c| c.id == id)
    }
    pub fn create_creature(
        &mut self,
        mut cfg: CreatureConfig,
        pos: Option<(f64, f64)>,
        team: Option<&str>,
        slot: &str,
    ) -> Creature {
        cfg.id = self.next_id;
        self.next_id += 1;
        cfg.screen_w = self.screen_w;
        cfg.screen_h = self.screen_h;
        cfg.size_scale = self.size_scale;
        cfg.seed = self.seed;
        let key = cfg.progression_id.clone().unwrap_or_else(|| self.mint_progression_id());
        cfg.progression_id = Some(key.clone());
        let stored = self.progression_states.get(&key).cloned().unwrap_or(Value::Null);
        if cfg.progression_state.is_none() {
            cfg.progression_state =
                stored.get("progression").cloned().or_else(|| stored.is_object().then(|| stored.clone()));
        }
        let pd = dc_creature::creature::typed_personality(&cfg.personality);
        cfg.gait_style = self
            .gait_style
            .clone()
            .unwrap_or_else(|| dc_content::personality_profiles::personality_gait_style(&pd).into());
        cfg.job_id = dc_content::jobs::normalize_job_id(Some(&cfg.job_id)).into();
        let mut c = Creature::new(cfg);
        self.slot_ids.insert(c.id, slot.into());
        if let Some(name) = stored.get("name").and_then(Value::as_str) {
            c.set_name(name);
        }
        if stored.get("progression").and_then(|p| p.get("team_id")).is_none() {
            if let Some(t) = team {
                c.set_team(t);
            }
        }
        c.team_stances = Some(self.team_stances.clone());
        c.team_profiles = Some(self.team_profiles.clone());
        c.force_show_level = self.always_show_levels;
        c.force_show_health = self.always_show_health;
        c.force_show_xp = self.always_show_xp;
        c.force_show_stamina = self.always_show_stamina;
        c.playfield = Some(Arc::new(self.playfield.clone()));
        if let Some((x, y)) = pos {
            c.kin.x = x;
            c.kin.y = y;
            c.kin.target_x = x;
            c.kin.target_y = y;
            c.resize_screen(self.screen_w, self.screen_h);
            c.initialize_legs();
        }
        if self.mood_mode != "auto" {
            c.set_mood_mode(&self.mood_mode);
        }
        c.set_allow_social(self.social_play);
        c
    }
    pub fn refresh_neighbor_links(&mut self) {
        let visible: Vec<_> = self.creatures.iter().filter(|c| !c.desktop_fully_hidden).map(|c| c.id).collect();
        for c in &mut self.creatures {
            if !self.social_play || visible.len() <= 1 || c.desktop_fully_hidden {
                if c.desktop_fully_hidden {
                    c.social_target = None;
                }
                c.set_neighbors(vec![]);
            } else {
                c.set_neighbors(visible.clone());
            }
        }
    }
    pub fn refresh_render_order(&mut self) {
        let mut order: Vec<_> = self.creatures.iter().map(|c| (c.id, c.kin.y)).collect();
        order.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        self.render_order = order.into_iter().map(|x| x.0).collect();
    }
    pub fn set_screen_rects(&mut self, rects: &[dc_world::playfield::ScreenRect]) {
        self.playfield.set_rects(rects);
        self.publish_playfield();
        self.base_world.keep_all_on_screen();
    }
    fn publish_playfield(&mut self) {
        self.base_world.playfield = Some(self.playfield.clone());
        self.fly_world.playfield = Some(self.playfield.clone());
        for c in &mut self.creatures {
            c.playfield = Some(Arc::new(self.playfield.clone()));
        }
    }
    pub fn resize(&mut self, w: f64, h: f64) {
        self.screen_w = w;
        self.screen_h = h;
        self.playfield.set_size(w, h);
        self.publish_playfield();
        for c in &mut self.creatures {
            c.resize_screen(w, h);
        }
        for c in &mut self.cages {
            c.clamp_to_screen(w, h, 8.0);
        }
        self.sync_cages();
        self.web_world.set_screen(w, h);
        self.fly_world.set_screen(w, h);
        self.mouse_web_world.set_screen(w, h);
        self.base_world.set_screen(w, h);
    }
    pub fn creature_at(&self, x: f64, y: f64) -> Option<CreatureId> {
        let order: Vec<_> = if self.render_order.is_empty() {
            self.creatures.iter().map(|c| c.id).collect()
        } else {
            self.render_order.clone()
        };
        order
            .iter()
            .rev()
            .filter_map(|id| self.creature(*id))
            .find(|c| (c.dragging || (!c.desktop_fully_hidden && !self.point_occluded(c, x, y))) && c.hit_test(x, y))
            .map(|c| c.id)
    }
    pub fn wants_mouse(&self, x: f64, y: f64) -> bool {
        self.cage_handle_at(x, y).is_some()
            || self.cage_drag.is_some()
            || self.dragged_creature.is_some()
            || self.dragged_fly.is_some()
            || self.dragged_spawner.is_some()
            || self.dragged_base.is_some()
            || ((self.interferable || self.naming_enabled) && self.creature_at(x, y).is_some())
            || (self.interferable
                && (self.fly_world.hit_fly_at(x, y).is_some() || self.fly_world.hit_spawner_at(x, y).is_some()))
            || self.base_grab_at(x, y).is_some()
    }
    pub fn update(&mut self, dt: f64, mx: f64, my: f64, input: MouseInput, rng: &mut PyRandom) -> Option<(f64, f64)> {
        self.update_with_controller(dt, mx, my, input, rng, |_, _| {})
    }
    /// Controller runs immediately at the creature's turn, before publishing its view.
    pub fn update_with_controller(
        &mut self,
        dt: f64,
        mx: f64,
        my: f64,
        input: MouseInput,
        rng: &mut PyRandom,
        mut controller: impl FnMut(f64, &mut Creature),
    ) -> Option<(f64, f64)> {
        self.mouse_x = mx;
        self.mouse_y = my;
        self.mouse_down = input.down;
        self.handle_input(dt, mx, my, input);
        self.update_camouflage_touch_state(mx, my);
        self.neighbor_refresh_accum += dt;
        if self.neighbor_refresh_accum >= 0.15 {
            self.neighbor_refresh_accum = 0.0;
            self.refresh_neighbor_links();
        }
        self.update_prey_targets(dt, rng);
        self.base_world.update(dt, &mut self.creatures, Some(&mut self.web_world), rng);
        self.update_colony_growth(dt, rng);
        if self.creatures.iter().any(|c| c.job_mode == "build") {
            self.mark_runtime_state_dirty();
        }
        self.flush_runtime_state(dt);
        let mut pop = Population::from_views(self.creatures.iter().map(Creature::view));
        let mut commands = vec![];
        for i in 0..self.creatures.len() {
            if self.creatures[i].prey.is_none() && !self.creatures[i].player_control {
                self.maybe_seek_desktop_cover(i, dt, rng);
            }
            let (fx, fy, hunting) = if self.creatures[i].desktop_fully_hidden && !self.creatures[i].dragging {
                self.creatures[i].social_target = None;
                (-100000.0, -100000.0, false)
            } else {
                self.creature_focus(i, mx, my)
            };
            let c = &mut self.creatures[i];
            c.hunting_prey = hunting;
            let mut env = Env {
                population: &pop,
                webs: Some(&mut self.web_world),
                flies: Some(&mut self.fly_world),
                mouse_webs: Some(&mut self.mouse_web_world),
                bases: Some(&self.base_world),
                commands: &mut commands,
            };
            if c.update(dt, fx, fy, self.screen_w, self.screen_h, &mut env) == UpdateOutcome::PlayerControlled {
                controller(dt, c);
            }
            pop.set(c.view());
            for command in commands.drain(..) {
                let id = match command {
                    dc_creature::Command::BumpMood { target, .. } | dc_creature::Command::EnterPlay { target, .. } => {
                        target
                    }
                };
                if let Some(target) = self.creature_mut(id) {
                    target.apply_command(&command, &pop);
                    pop.set(target.view());
                }
            }
            if !self.playfield.simple() {
                let c = &mut self.creatures[i];
                let margin = 8.0f64.max(c.kin.size * 0.5);
                (c.kin.x, c.kin.y) = self.playfield.clamp(c.kin.x, c.kin.y, margin);
                (c.kin.target_x, c.kin.target_y) = self.playfield.clamp(c.kin.target_x, c.kin.target_y, margin);
            }
            self.update_desktop_awareness(i, dt, rng);
            pop.set(self.creatures[i].view());
        }
        self.web_world.update(dt, Some((mx, my)));
        self.mouse_web_world.enabled = self.allow_mouse_capture;
        self.desired_cursor = self.mouse_web_world.update(dt, mx, my);
        self.fly_world.update(dt, &mut self.creatures, Some(&mut self.web_world));
        self.resolve_fly_catches(rng);
        self.resolve_combat(dt);
        self.render_sort_accum += dt;
        if self.dragged_creature.is_some() || self.render_sort_accum >= 0.10 {
            self.render_sort_accum = 0.0;
            self.refresh_render_order();
        }
        self.desired_cursor
    }
    fn update_camouflage_touch_state(&mut self, mx: f64, my: f64) {
        let touched: Vec<_> = self
            .creatures
            .iter()
            .map(|c| {
                if crate::surfaces::personality_float(c, "camouflage_strength", 0.0) <= 0.0 {
                    return false;
                }
                if c.dragging || Some(c.id) == self.dragged_creature {
                    return true;
                }
                if c.desktop_fully_hidden {
                    return false;
                }
                let padding = 4.0f64
                    .max(c.kin.size * crate::surfaces::personality_float(c, "camouflage_cursor_touch_padding", 0.1));
                if (c.kin.x - mx).hypot(c.kin.y - my) <= c.kin.size * 2.25 + padding && c.hit_test(mx, my) {
                    return true;
                }
                let mult = crate::surfaces::personality_float(c, "camouflage_social_touch_radius_mult", 1.45);
                self.creatures.iter().any(|o| {
                    o.id != c.id
                        && !o.desktop_fully_hidden
                        && (c.kin.x - o.kin.x).hypot(c.kin.y - o.kin.y)
                            <= 8.0f64.max((c.kin.size + o.kin.size) * 0.5 * mult)
                })
            })
            .collect();
        for (c, t) in self.creatures.iter_mut().zip(touched) {
            if t {
                c.register_camouflage_touch(None);
            }
        }
    }
}
