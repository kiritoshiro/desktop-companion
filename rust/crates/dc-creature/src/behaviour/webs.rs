//! Port of `creature/behaviour/webs.py`: weaving, repairing, walking and
//! shooting silk. Every web call goes through [`WebWorldApi`] and every
//! shot through [`crate::world::FlyWorldApi`] / [`crate::world::MouseWebApi`].

use crate::bump;
use crate::creature::Creature;
use crate::pymath::{angle_to, clamp, distance, rand_range};
use crate::state::State;
use crate::world::{CreatureId, Env, FlyId, PreyView, Shooter, ShotKind, WebId, WebWorldApi};

/// `Option<&mut dyn WebWorldApi>` re-borrowed from an [`Env`].
pub type WebsMut<'b> = Option<&'b mut dyn WebWorldApi>;

impl Creature {
    /// `_begin_weave()`: claim a fresh site and walk to it.
    pub fn begin_weave(&mut self, env: &mut Env<'_>) -> bool {
        if env.webs.is_none() || self.cage.is_some() {
            return false;
        }
        let prefer_corner = self.rng.random() < self.pf("web_corner_bias", 0.55);
        let size = self.kin.size;
        let web = env.webs_mut().unwrap().claim_site(self.id, size, prefer_corner);
        let Some(web) = web else {
            self.weave_cooldown = rand_range(self.personality.get("weave_retry_cooldown"), 5.0, 10.0, &mut self.rng);
            return false;
        };
        self.weaving_web = Some(web);
        self.enter_weave_approach(env.webs.as_deref());
        true
    }

    /// `_begin_adopt(web)`
    pub fn begin_adopt(&mut self, web: WebId, env: &mut Env<'_>) -> bool {
        let Some(webs) = env.webs_mut() else { return false };
        if !webs.adopt(web, self.id) {
            return false;
        }
        self.weaving_web = Some(web);
        self.enter_weave_approach(env.webs.as_deref());
        true
    }

    /// `_begin_web_walk(web)`
    pub fn begin_web_walk(&mut self, web: WebId, env: &mut Env<'_>) -> bool {
        let Some(webs) = env.webs.as_deref() else { return false };
        if !webs.is_complete(web) {
            return false;
        }
        self.web_target = Some(web);
        self.enter_web_approach(env.webs_mut());
        true
    }

    /// `_abandon_weaving()`
    pub fn abandon_weaving(&mut self, webs: WebsMut<'_>) {
        if let (Some(web), Some(w)) = (self.weaving_web, webs) {
            w.abandon(web);
        }
        self.weaving_web = None;
        self.weave_drawing = false;
        let retry = rand_range(self.personality.get("weave_retry_cooldown"), 3.5, 7.5, &mut self.rng);
        self.weave_cooldown = self.weave_cooldown.max(retry);
    }

    /// `_finish_weave()`
    pub fn finish_weave(&mut self) {
        self.weaving_web = None;
        self.weave_drawing = false;
        let base = rand_range(self.personality.get("weave_cooldown"), 8.0, 16.0, &mut self.rng);
        self.weave_cooldown = base / if self.acts_as_webber() { 4.0 } else { 1.0 };
        bump!(self.mood, arousal = -0.05, valence = 0.18, affection = 0.04);
        self.enter_idle();
    }

    /// `_begin_repair(web)`
    pub fn begin_repair(&mut self, web: WebId, env: &mut Env<'_>) -> bool {
        let Some(webs) = env.webs_mut() else { return false };
        if !webs.claim_repair(web, self.id) {
            return false;
        }
        self.repairing_web = Some(web);
        self.enter_repair_approach(env.webs_mut());
        true
    }

    /// `_abandon_repair()`
    pub fn abandon_repair(&mut self, webs: WebsMut<'_>) {
        if let (Some(web), Some(w)) = (self.repairing_web, webs) {
            w.release_repair(web);
        }
        self.repairing_web = None;
        let retry = rand_range(self.personality.get("weave_retry_cooldown"), 2.5, 5.0, &mut self.rng);
        self.weave_cooldown = self.weave_cooldown.max(retry);
    }

    /// `_finish_repair()`
    pub fn finish_repair(&mut self, webs: WebsMut<'_>) {
        if let (Some(web), Some(w)) = (self.repairing_web, webs) {
            w.release_repair(web);
        }
        self.repairing_web = None;
        let base = rand_range(self.personality.get("weave_cooldown"), 8.0, 16.0, &mut self.rng);
        self.weave_cooldown = base / if self.acts_as_webber() { 6.0 } else { 1.5 };
        bump!(self.mood, valence = 0.14, affection = 0.05, arousal = -0.04);
        self.enter_idle();
    }

    /// `_repair_point()`
    pub fn repair_point(&self, webs: &dyn WebWorldApi) -> Option<(f64, f64)> {
        let web = self.repairing_web?;
        let (cx, cy) = webs.damaged_centroid(web)?;
        let ins = webs.reach_inset(web);
        Some((clamp(cx, ins, self.screen_w - ins), clamp(cy, ins, self.screen_h - ins)))
    }

    /// `enter_repair_approach()`
    pub fn enter_repair_approach(&mut self, webs: WebsMut<'_>) {
        if !self.has_skill("weave_web") || self.repairing_web.is_none() {
            self.enter_idle();
            return;
        }
        self.state = State::RepairApproach;
        self.kin.motion_paused = false;
        let point = webs.as_deref().and_then(|w| self.repair_point(w));
        let Some((px, py)) = point else {
            self.finish_repair(webs);
            return;
        };
        self.kin.target_x = px;
        self.kin.target_y = py;
        self.kin.speed = 48.0f64.max(self.weave_speed * 0.95) * self.speed_mult();
        self.state_timer = 7.0;
        bump!(self.mood, arousal = 0.06, valence = 0.03);
    }

    fn web_alive(&self, web: Option<WebId>, webs: Option<&dyn WebWorldApi>) -> Option<WebId> {
        let web = web?;
        let w = webs?;
        if w.contains(web) { Some(web) } else { None }
    }

    /// `_update_repair_approach(dt, mx, my)`
    pub fn update_repair_approach(&mut self, env: &mut Env<'_>) {
        let Some(web) = self.web_alive(self.repairing_web, env.webs.as_deref()) else {
            self.abandon_repair(env.webs_mut());
            self.enter_idle();
            return;
        };
        let webs = env.webs_mut().unwrap();
        if !webs.is_damaged(web) {
            self.finish_repair(Some(webs));
            return;
        }
        let Some((px, py)) = self.repair_point(webs) else {
            self.finish_repair(Some(webs));
            return;
        };
        self.kin.target_x = px;
        self.kin.target_y = py;
        self.set_focus(px, py, 0.6);
        if distance(self.kin.x, self.kin.y, px, py) < 18.0 || self.state_timer <= 0.0 {
            self.enter_repair();
        }
    }

    /// `enter_repair()`
    pub fn enter_repair(&mut self) {
        if self.repairing_web.is_none() {
            self.enter_idle();
            return;
        }
        self.state = State::Repair;
        self.kin.motion_paused = false;
        self.kin.speed = self.weave_speed * 0.8 * self.speed_mult();
        self.state_timer = 20.0;
        bump!(self.mood, arousal = 0.03, valence = 0.05);
    }

    /// `_update_repair(dt, mx, my)`
    pub fn update_repair(&mut self, dt: f64, env: &mut Env<'_>) {
        let Some(web) = self.web_alive(self.repairing_web, env.webs.as_deref()) else {
            self.abandon_repair(env.webs_mut());
            self.enter_idle();
            return;
        };
        let webs = env.webs_mut().unwrap();
        if !webs.is_damaged(web) {
            self.finish_repair(Some(webs));
            return;
        }
        let close = match self.repair_point(webs) {
            Some((px, py)) => {
                self.kin.target_x = px;
                self.kin.target_y = py;
                self.set_focus(px, py, 0.5);
                distance(self.kin.x, self.kin.y, px, py) < self.kin.size * 2.2
            }
            None => true,
        };
        self.weave_drawing = true;
        if close {
            self.kin.speed = 0.0;
        }
        webs.repair_near(web, self.kin.x, self.kin.y, dt);
        if !webs.is_damaged(web) {
            self.finish_repair(Some(webs));
            return;
        }
        if self.state_timer <= 0.0 {
            self.abandon_repair(Some(webs));
            self.enter_idle();
        }
    }

    /// `enter_weave_approach()`
    pub fn enter_weave_approach(&mut self, webs: Option<&dyn WebWorldApi>) {
        if !self.has_skill("weave_web") || self.weaving_web.is_none() {
            self.enter_idle();
            return;
        }
        self.state = State::WeaveApproach;
        self.kin.motion_paused = false;
        if let Some(w) = webs {
            let (point, _) = w.working_point(self.weaving_web.unwrap(), self.screen_w, self.screen_h);
            self.kin.target_x = point.0;
            self.kin.target_y = point.1;
        }
        self.kin.speed = 46.0f64.max(self.weave_speed * 0.95) * self.speed_mult();
        self.state_timer = 7.0;
        bump!(self.mood, arousal = 0.05, valence = 0.05);
    }

    /// `_update_weave_approach(dt, mx, my)`
    pub fn update_weave_approach(&mut self, env: &mut Env<'_>) {
        let Some(web) = self.web_alive(self.weaving_web, env.webs.as_deref()) else {
            self.weaving_web = None;
            self.enter_idle();
            return;
        };
        let webs = env.webs.as_deref().unwrap();
        if webs.is_complete(web) {
            self.weaving_web = None;
            self.weave_cooldown = rand_range(self.personality.get("weave_cooldown"), 6.0, 14.0, &mut self.rng);
            self.enter_idle();
            return;
        }
        let (point, _) = webs.working_point(web, self.screen_w, self.screen_h);
        self.kin.target_x = point.0;
        self.kin.target_y = point.1;
        self.set_focus(point.0, point.1, 0.6);
        if distance(self.kin.x, self.kin.y, point.0, point.1) < 16.0 || self.state_timer <= 0.0 {
            self.enter_weave();
        }
    }

    /// `enter_weave()`
    pub fn enter_weave(&mut self) {
        if !self.has_skill("weave_web") || self.weaving_web.is_none() {
            self.enter_idle();
            return;
        }
        self.state = State::Weave;
        self.kin.motion_paused = false;
        self.kin.speed = self.weave_speed * self.speed_mult();
        self.state_timer = 34.0;
        bump!(self.mood, arousal = 0.04, valence = 0.06);
    }

    /// `_update_weave(dt, mx, my)`
    pub fn update_weave(&mut self, dt: f64, env: &mut Env<'_>) {
        let Some(web) = self.web_alive(self.weaving_web, env.webs.as_deref()) else {
            self.weaving_web = None;
            self.enter_idle();
            return;
        };
        let webs = env.webs_mut().unwrap();
        if webs.is_complete(web) {
            self.finish_weave();
            return;
        }
        let (point, drawing) = webs.working_point(web, self.screen_w, self.screen_h);
        self.weave_drawing = drawing;
        self.kin.target_x = point.0;
        self.kin.target_y = point.1;
        self.kin.speed = self.weave_speed * if drawing { 1.0 } else { 2.4 } * self.speed_mult();
        self.set_focus(point.0, point.1, 0.5);
        webs.advance(web, dt, self.weave_speed, (self.kin.x, self.kin.y), 58.0);
        if webs.is_complete(web) {
            self.finish_weave();
            return;
        }
        if self.state_timer <= 0.0 {
            self.abandon_weaving(Some(webs));
            self.enter_idle();
        }
    }

    /// `enter_web_approach()`
    pub fn enter_web_approach(&mut self, webs: WebsMut<'_>) {
        if !self.has_skill("web_walk") || self.web_target.is_none() {
            self.enter_idle();
            return;
        }
        self.state = State::WebApproach;
        self.kin.motion_paused = false;
        if let Some(w) = webs {
            let (px, py) = w.walkable_point(self.web_target.unwrap(), self.screen_w, self.screen_h);
            self.web_walk_point = Some((px, py));
            self.kin.target_x = px;
            self.kin.target_y = py;
        }
        self.kin.speed = 48.0f64.max(self.weave_speed * 0.85) * self.speed_mult();
        self.state_timer = 7.0;
    }

    /// `_update_web_approach(dt, mx, my)`
    pub fn update_web_approach(&mut self, env: &mut Env<'_>) {
        let alive = self
            .web_alive(self.web_target, env.webs.as_deref())
            .filter(|w| env.webs.as_deref().unwrap().is_complete(*w));
        let Some(web) = alive else {
            self.web_target = None;
            self.enter_idle();
            return;
        };
        let (px, py) = self.web_walk_point.unwrap_or_else(|| env.webs.as_deref().unwrap().hub(web));
        self.kin.target_x = px;
        self.kin.target_y = py;
        self.set_focus(px, py, 0.5);
        if distance(self.kin.x, self.kin.y, px, py) < 16.0 || self.state_timer <= 0.0 {
            self.enter_web_walk();
        }
    }

    /// `enter_web_walk()`
    pub fn enter_web_walk(&mut self) {
        if self.web_target.is_none() {
            self.enter_idle();
            return;
        }
        self.state = State::WebWalk;
        self.kin.motion_paused = true;
        self.kin.speed = 0.0;
        self.web_pluck_count = self.rng.randint(2, 4);
        self.web_pluck_timer = self.rng.uniform(0.25, 0.5);
        self.state_timer = 6.0;
        bump!(self.mood, arousal = 0.06, valence = 0.05, curiosity = 0.05);
    }

    /// `_update_web_walk(dt, mx, my)`
    pub fn update_web_walk(&mut self, dt: f64, env: &mut Env<'_>) {
        let Some(web) = self.web_alive(self.web_target, env.webs.as_deref()) else {
            self.web_target = None;
            self.kin.motion_paused = false;
            self.enter_idle();
            return;
        };
        let webs = env.webs_mut().unwrap();
        self.kin.motion_paused = true;
        self.kin.speed = 0.0;
        self.kin.target_x = self.kin.x;
        self.kin.target_y = self.kin.y;
        let hub = webs.hub(web);
        self.set_focus(hub.0, hub.1, 0.4);
        self.web_pluck_timer -= dt;
        if self.web_pluck_timer <= 0.0 {
            if self.web_pluck_count > 0 {
                let strength = self.rng.uniform(0.6, 1.3);
                webs.pluck(web, (self.kin.x, self.kin.y), strength);
                self.web_pluck_count -= 1;
                self.web_pluck_timer = self.rng.uniform(0.5, 0.95);
                bump!(self.mood, arousal = 0.03, valence = 0.04, curiosity = 0.03);
            } else {
                self.leave_web_walk();
                return;
            }
        }
        if self.state_timer <= 0.0 {
            self.leave_web_walk();
        }
    }

    /// `_leave_web_walk()`
    pub fn leave_web_walk(&mut self) {
        self.web_target = None;
        self.web_walk_cooldown = rand_range(self.personality.get("web_walk_cooldown"), 8.0, 20.0, &mut self.rng);
        self.kin.motion_paused = false;
        self.enter_idle();
    }

    /// `_can_shoot_web(kind, prey=, foe=)`
    pub fn can_shoot_web(&self, kind: ShotKind, prey: bool, foe: bool, env: &Env<'_>) -> bool {
        let skill = if kind == ShotKind::Wall { "wall_web" } else { "shoot_web" };
        if !self.has_skill(skill) || self.airborne {
            return false;
        }
        if foe {
            return true;
        }
        if prey {
            return env.flies.is_some();
        }
        if self.dragging {
            return false;
        }
        match env.mouse_webs.as_deref() {
            Some(w) => w.enabled() && !w.busy(),
            None => false,
        }
    }

    /// `_maybe_shoot_web_at_cursor(dist, mx, my)`
    pub fn maybe_shoot_web_at_cursor(&mut self, dist_to_cursor: f64, mx: f64, my: f64, env: &mut Env<'_>) -> bool {
        if self.webbed_held() || self.hunting_prey || self.web_shot_cooldown > 0.0 {
            return false;
        }
        let can_trap = self.can_shoot_web(ShotKind::Trap, false, false, env);
        let can_wall = self.can_shoot_web(ShotKind::Wall, false, false, env);
        if !(can_trap || can_wall) {
            return false;
        }
        let web_shooter = self.acts_as_web_shooter();
        let reaction = self.pf("reaction_radius", 360.0);
        let range_mult = self.pf("web_shot_range_mult", if web_shooter { 0.85 } else { 0.5 });
        let shot_range = (self.kin.size * 4.0).max(reaction * range_mult);
        if dist_to_cursor > shot_range {
            return false;
        }
        let m = self.mood;
        let boldness = clamp(self.pf("boldness", 0.5), 0.0, 1.0);
        let mut chance = self.pf("web_shot_chance", if web_shooter { 0.6 } else { 0.05 });
        if self.cursor_is_still_for_observe() {
            chance *= 1.55;
        }
        let chance = clamp(chance + m.arousal * 0.15 + boldness * 0.1, 0.0, 0.97);
        if self.rng.random() >= chance {
            return false;
        }
        let mut kind = ShotKind::Trap;
        if can_wall && (!can_trap || self.rng.random() < self.pf("wall_web_bias", 0.3)) {
            kind = ShotKind::Wall;
        }
        self.enter_web_aim(mx, my, kind, None, None, env);
        true
    }

    /// `_maybe_shoot_web_at_prey(prey)`
    pub fn maybe_shoot_web_at_prey(&mut self, env: &mut Env<'_>) -> bool {
        if self.webbed_held() {
            return false;
        }
        let Some(prey) = self.prey else { return false };
        if self.web_shot_cooldown > 0.0 {
            return false;
        }
        if prey.trapped || prey.dragging || !prey.alive {
            return false;
        }
        let can_trap = self.can_shoot_web(ShotKind::Trap, true, false, env);
        let can_wall = self.can_shoot_web(ShotKind::Wall, true, false, env);
        if !(can_trap || can_wall) {
            return false;
        }
        let web_shooter = self.acts_as_web_shooter();
        let reaction = self.pf("reaction_radius", 360.0);
        let range_mult = self.pf("web_shot_range_mult", if web_shooter { 0.85 } else { 0.5 });
        let shot_range = (self.kin.size * 4.0).max(reaction * range_mult);
        let d = distance(self.kin.x, self.kin.y, prey.x, prey.y);
        if d > shot_range || d < self.kin.size * 1.4 {
            return false;
        }
        let boldness = clamp(self.pf("boldness", 0.5), 0.0, 1.0);
        let chance = self.pf("web_shot_chance", if web_shooter { 0.6 } else { 0.12 });
        let chance = clamp(chance + self.mood.arousal * 0.15 + boldness * 0.1, 0.0, 0.97);
        if self.rng.random() >= chance {
            return false;
        }
        let mut kind = ShotKind::Trap;
        if can_wall && (!can_trap || self.rng.random() < self.pf("wall_web_bias", 0.3)) {
            kind = ShotKind::Wall;
        }
        self.enter_web_aim(prey.x, prey.y, kind, Some(prey.id), None, env);
        true
    }

    /// `enter_web_aim(mx, my, kind, prey=, foe=)`
    pub fn enter_web_aim(
        &mut self,
        mx: f64,
        my: f64,
        kind: ShotKind,
        prey: Option<FlyId>,
        foe: Option<CreatureId>,
        env: &Env<'_>,
    ) {
        if !self.can_shoot_web(kind, prey.is_some(), foe.is_some(), env) {
            self.end_web_state();
            return;
        }
        self.web_shot_prey = prey;
        self.web_shot_foe = foe;
        self.state = State::WebAim;
        self.kin.motion_paused = true;
        self.kin.speed = 0.0;
        self.social_target = None;
        self.web_shot_kind = kind;
        self.set_focus(mx, my, 1.0);
        self.kin.target_heading = angle_to(self.kin.x, self.kin.y, mx, my);
        self.state_timer = rand_range(self.personality.get("web_aim_time"), 0.3, 0.58, &mut self.rng);
        self.range_clock = 0.0;
        self.range_mode = crate::creature::RangeMode::Waggle;
        self.range_switch = self.rng.uniform(0.14, 0.28);
        bump!(self.mood, arousal = 0.2, curiosity = 0.05);
    }

    /// The fly a web shot is aimed at: the manager's prey view when it is
    /// the same fly, else the fly world's.
    fn web_shot_prey_view(&self, env: &Env<'_>) -> Option<PreyView> {
        let id = self.web_shot_prey?;
        if let Some(p) = self.prey
            && p.id == id
        {
            return Some(p);
        }
        env.fly(Some(id))
    }

    /// `_update_web_aim(dt, mx, my)`
    pub fn update_web_aim(&mut self, dt: f64, mx: f64, my: f64, env: &mut Env<'_>) {
        if self.webbed_held() {
            self.finish_web_shot(false);
            return;
        }
        let (mut mx, mut my) = (mx, my);
        if let Some(foe_id) = self.web_shot_foe {
            match env.population.get(foe_id) {
                Some(foe) if !(foe.dead || foe.webbed || foe.dragging) => {
                    mx = foe.x;
                    my = foe.y;
                }
                _ => {
                    self.finish_web_shot(false);
                    return;
                }
            }
        } else if self.web_shot_prey.is_some() {
            match self.web_shot_prey_view(env) {
                Some(p) if p.alive && !p.eaten && !p.dragging && !p.trapped => {
                    mx = p.x;
                    my = p.y;
                }
                _ => {
                    self.finish_web_shot(false);
                    return;
                }
            }
        } else {
            let ok = env.mouse_webs.as_deref().is_some_and(|w| w.enabled() && !w.busy());
            if !ok {
                self.finish_web_shot(false);
                return;
            }
        }
        self.kin.motion_paused = true;
        self.kin.speed = 0.0;
        self.kin.target_x = self.kin.x;
        self.kin.target_y = self.kin.y;
        self.set_focus(mx, my, 1.0);
        self.kin.target_heading = angle_to(self.kin.x, self.kin.y, mx, my);
        self.aim_intent = (self.aim_intent + dt * 3.2).min(1.0);
        self.crouch = (self.crouch + dt * 4.0).min(1.0);
        self.range_clock += dt;
        if self.range_clock >= self.range_switch {
            use crate::creature::RangeMode;
            self.range_clock = 0.0;
            self.range_switch = self.rng.uniform(0.14, 0.28);
            self.range_mode = if self.range_mode == RangeMode::Waggle { RangeMode::Nod } else { RangeMode::Waggle };
            if self.range_mode == RangeMode::Waggle {
                self.raise_wiggle_burst(0.6);
            }
        }
        if self.state_timer <= 0.0 {
            self.fire_web_shot(mx, my, env);
        }
    }

    /// `_fire_web_shot(mx, my)`
    pub fn fire_web_shot(&mut self, mx: f64, my: f64, env: &mut Env<'_>) {
        let kind = self.web_shot_kind;
        let [fx, fy, _, _] = self.kin.basis();
        let size = self.kin.size;
        let origin = (self.kin.x + fx * size * 0.6, self.kin.y + fy * size * 0.6);
        let shooter = Shooter {
            id: self.id,
            x: self.kin.x,
            y: self.kin.y,
            heading: self.kin.heading,
            size,
            homing: self.progression_effect("web_homing"),
        };
        let mut launched = false;
        if let Some(foe_id) = self.web_shot_foe {
            if env.flies.is_some()
                && self.can_shoot_web(kind, false, true, env)
                && let Some(target) = env.population.get(foe_id)
            {
                launched = env.flies.as_deref_mut().unwrap().launch_web_shot_at_creature(&shooter, target, kind);
            }
        } else if let Some(prey) = self.web_shot_prey {
            if env.flies.is_some() && self.can_shoot_web(kind, true, false, env) {
                launched = env.flies.as_deref_mut().unwrap().launch_web_shot(&shooter, prey, kind);
            }
        } else if env.mouse_webs.is_some() && self.can_shoot_web(kind, false, false, env) {
            let lead = (self.prev_cursor_vx, self.prev_cursor_vy);
            launched = env.mouse_webs.as_deref_mut().unwrap().shoot(origin, (mx, my), kind, shooter.homing, lead);
        }
        if launched {
            bump!(self.mood, arousal = 0.12, valence = 0.12, curiosity = 0.05);
            self.enter_web_shot();
        } else {
            self.finish_web_shot(false);
        }
    }

    /// `enter_web_shot()`: the recoil hold after letting the silk fly.
    pub fn enter_web_shot(&mut self) {
        self.state = State::WebShot;
        self.kin.motion_paused = true;
        self.kin.speed = 0.0;
        self.state_timer = rand_range(self.personality.get("web_recoil_time"), 0.18, 0.3, &mut self.rng);
        self.rear = (self.rear + 0.4).min(1.0);
        self.raise_wiggle_burst(0.5);
    }

    /// `_update_web_shot(dt, mx, my)`
    pub fn update_web_shot(&mut self, dt: f64, mx: f64, my: f64) {
        self.kin.motion_paused = true;
        self.kin.speed = 0.0;
        self.kin.target_x = self.kin.x;
        self.kin.target_y = self.kin.y;
        self.set_focus(mx, my, 0.7);
        self.crouch = (self.crouch - dt * 3.5).max(0.0);
        if self.state_timer <= 0.0 {
            self.finish_web_shot(true);
        }
    }

    /// `_finish_web_shot(fired)`
    pub fn finish_web_shot(&mut self, fired: bool) {
        let web_shooter = self.acts_as_web_shooter();
        let prey_shot = self.web_shot_prey.is_some() || self.web_shot_foe.is_some();
        self.web_shot_prey = None;
        self.web_shot_foe = None;
        let base = if fired {
            rand_range(self.personality.get("web_shot_cooldown"), 8.0, 18.0, &mut self.rng)
        } else {
            rand_range(self.personality.get("web_shot_retry_cooldown"), 2.0, 4.5, &mut self.rng)
        };
        self.web_shot_cooldown = base / if web_shooter { 3.5 } else { 1.0 };
        if prey_shot {
            self.web_shot_cooldown = self.web_shot_cooldown.min(1.2);
        }
        self.end_web_state();
    }

    /// `_end_web_state()`
    pub fn end_web_state(&mut self) {
        self.kin.motion_paused = false;
        if self.dragging {
            self.state = State::Dragged;
            self.kin.motion_paused = true;
            self.kin.speed = 0.0;
        } else {
            self.enter_idle();
        }
    }
}
