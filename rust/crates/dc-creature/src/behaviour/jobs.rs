//! Port of `creature/behaviour/jobs.py`: pursuing prey and foes, running,
//! limping home, and how job state yields to temperament.

use crate::constants::*;
use crate::creature::{After, Creature};
use crate::progression::Relation;
use crate::pymath::{angle_to, clamp, distance, hypot, py_mod};
use crate::state::State;
use crate::world::{CreatureView, Env, Population, ShotKind};
use std::f64::consts::{PI, TAU};

impl Creature {
    /// `_pursue_prey(dt, mx, my)`: act on the published prey candidate.
    pub fn pursue_prey(&mut self, mx: f64, my: f64, env: &mut Env<'_>) {
        let Some(prey) = self.prey else { return };
        if self.state.is_in(&HUNT_COMMITTED_STATES) || self.airborne {
            return;
        }
        let shooter = self.has_skill("shoot_web") || self.has_skill("wall_web");
        if self.dragging {
            if !prey.trapped && shooter {
                self.maybe_shoot_web_at_prey(env);
            }
            return;
        }
        if !prey.trapped && shooter && self.maybe_shoot_web_at_prey(env) {
            return;
        }
        let d = distance(self.kin.x, self.kin.y, mx, my);
        let strike = self.kin.size * 5.0 + prey.size;
        let hunter = self.acts_as_hunter();
        if hunter && !prey.is_moving && !prey.trapped {
            self.kin.target_x = mx;
            self.kin.target_y = my;
            self.kin.target_heading = (my - self.kin.y).atan2(mx - self.kin.x);
            self.kin.motion_paused = true;
            self.kin.speed = 0.0;
            return;
        }
        if !prey.trapped
            && d <= strike
            && self.has_skill("jump")
            && self.has_skill("prepare_jump_attack")
            && self.pounce_cooldown <= 0.0
            && self.rng.random() < if hunter { 0.5 } else { 0.4 }
        {
            self.pounce_cooldown = self.rng.uniform(0.8, 1.5);
            self.enter_aim(mx, my, None, After::Outcome, (0.4, 0.85), 0.06, env.population);
            return;
        }
        if hunter && self.has_skill("approach") {
            if self.state != State::Approach {
                self.enter_approach(mx, my);
            }
        } else if self.has_skill("chase") {
            if self.state != State::Chase {
                self.enter_chase(mx, my);
            }
        } else if self.has_skill("approach") && self.state != State::Approach {
            self.enter_approach(mx, my);
        }
    }

    /// `_flee_from_danger(dt)`: home if there is a base to run to, away if not.
    pub fn flee_from_danger(&mut self, env: &Env<'_>) -> bool {
        let threat = self.flee_from.and_then(|id| env.population.get(id)).map(|t| (t.x, t.y));
        let mut home = self.own_base_point(env);
        let (x, y) = (self.kin.x, self.kin.y);
        if let (Some((hx, hy)), Some((tx, ty))) = (home, threat) {
            let to_home = (hy - y).atan2(hx - x);
            let to_threat = (ty - y).atan2(tx - x);
            let gap = (py_mod(to_home - to_threat + PI, TAU) - PI).abs();
            if gap < 0.7 {
                home = None;
            }
        }
        let (target_x, target_y) = match home {
            Some(h) => h,
            None => {
                let Some((tx, ty)) = threat else { return false };
                let away = (y - ty).atan2(x - tx);
                let reach = self.pf("reaction_radius", 360.0) * 0.9;
                (x + away.cos() * reach, y + away.sin() * reach)
            }
        };
        let target_x = clamp(target_x, 12.0, self.screen_w - 12.0);
        let target_y = clamp(target_y, 12.0, self.screen_h - 12.0);
        if self.has_skill("chase") {
            if self.state != State::Chase {
                self.enter_chase(target_x, target_y);
            } else {
                self.kin.target_x = target_x;
                self.kin.target_y = target_y;
            }
            return true;
        }
        if self.has_skill("approach") {
            if self.state != State::Approach {
                self.enter_approach(target_x, target_y);
            } else {
                self.kin.target_x = target_x;
                self.kin.target_y = target_y;
            }
            return true;
        }
        if let Some((tx, ty)) = threat {
            self.kin.target_heading = (ty - y).atan2(tx - x);
        }
        false
    }

    /// `_walk_home_to_heal(dt)` (DC-64)
    pub fn walk_home_to_heal(&mut self, env: &Env<'_>) -> bool {
        let Some((target_x, target_y)) = self.own_base_point(env) else {
            self.recovering = false;
            return false;
        };
        if self.foe_near(target_x, target_y, ESCAPED_RADIUS, env.population) {
            return false;
        }
        if hypot(target_x - self.kin.x, target_y - self.kin.y) <= RECOVER_ARRIVE_RADIUS {
            self.kin.motion_paused = true;
            self.kin.speed = 0.0;
            self.kin.target_heading = angle_to(self.kin.x, self.kin.y, target_x, target_y);
            return true;
        }
        if self.has_skill("approach") {
            if self.state != State::Approach {
                self.enter_approach(target_x, target_y);
            } else {
                self.kin.target_x = target_x;
                self.kin.target_y = target_y;
            }
            return true;
        }
        if self.has_skill("chase") {
            if self.state != State::Chase {
                self.enter_chase(target_x, target_y);
            } else {
                self.kin.target_x = target_x;
                self.kin.target_y = target_y;
            }
            return true;
        }
        false
    }

    /// `_foe_near(x, y, radius)`
    pub fn foe_near(&self, x: f64, y: f64, radius: f64, pop: &Population) -> bool {
        for id in &self.neighbors {
            if *id == self.id {
                continue;
            }
            let Some(other) = pop.get(*id) else { continue };
            if other.dead || self.relation_to(other) != Relation::Foe {
                continue;
            }
            if hypot(other.x - x, other.y - y) <= radius {
                return true;
            }
        }
        false
    }

    /// `_own_base_point()`
    pub fn own_base_point(&self, env: &Env<'_>) -> Option<(f64, f64)> {
        let bases = env.bases?;
        let team = if self.progression.team_id.is_empty() { "neutral" } else { self.progression.team_id.as_str() };
        if team == "neutral" {
            return None;
        }
        bases.base_point(&format!("team:{team}"))
    }

    /// `_pursue_foe(dt)`: fight the published foe with this spider's kit.
    pub fn pursue_foe(&mut self, env: &mut Env<'_>) -> bool {
        let pop: &Population = env.population;
        let Some(foe) = self.foe.and_then(|id| pop.get(id)) else { return false };
        if foe.dead || self.dead {
            return false;
        }
        if self.dragging || self.airborne || self.state.is_in(&HUNT_COMMITTED_STATES) {
            return false;
        }
        let (fx, fy) = (foe.x, foe.y);
        let (x, y, size) = (self.kin.x, self.kin.y, self.kin.size);
        let d = distance(x, y, fx, fy);
        if !foe.webbed
            && (self.has_skill("shoot_web") || self.has_skill("wall_web"))
            && self.maybe_shoot_web_at_foe(foe, d, env)
        {
            return true;
        }
        let standoff = (size + foe.size) * COMBAT_SPACING;
        let strike = size * 4.5 + foe.size;
        if standoff * 0.95 <= d
            && d <= strike
            && self.has_skill("jump")
            && self.has_skill("prepare_jump_attack")
            && self.pounce_cooldown <= 0.0
            && self.rng.random() < if foe.webbed { 0.65 } else { 0.32 }
        {
            self.pounce_cooldown = self.rng.uniform(0.9, 1.6);
            self.enter_aim(fx, fy, None, After::Strike, (0.4, 0.85), 0.05, pop);
            return true;
        }
        let span = 1e-4f64.max(d);
        let (away_x, away_y) = ((x - fx) / span, (y - fy) / span);
        if d < standoff * (1.0 - COMBAT_SPACING_SLACK) {
            self.walk_towards_in_fight(fx + away_x * standoff, fy + away_y * standoff);
            self.focus_x = fx;
            self.focus_y = fy;
            return true;
        }
        if d <= standoff * (1.0 + COMBAT_SPACING_SLACK) {
            self.kin.target_x = x;
            self.kin.target_y = y;
            self.focus_x = fx;
            self.focus_y = fy;
            return true;
        }
        let (stop_x, stop_y) = (fx + away_x * standoff, fy + away_y * standoff);
        self.focus_x = fx;
        self.focus_y = fy;
        self.walk_towards_in_fight(stop_x, stop_y)
    }

    /// `_walk_towards_in_fight(x, y)`
    pub fn walk_towards_in_fight(&mut self, x: f64, y: f64) -> bool {
        if self.has_skill("chase") {
            if self.state != State::Chase {
                self.enter_chase(x, y);
            } else {
                self.kin.target_x = x;
                self.kin.target_y = y;
            }
            return true;
        }
        if self.has_skill("approach") {
            if self.state != State::Approach {
                self.enter_approach(x, y);
            } else {
                self.kin.target_x = x;
                self.kin.target_y = y;
            }
            return true;
        }
        false
    }

    /// `_maybe_shoot_web_at_foe(foe, d)`
    pub fn maybe_shoot_web_at_foe(&mut self, foe: &CreatureView, d: f64, env: &mut Env<'_>) -> bool {
        if self.webbed_held() {
            return false;
        }
        if self.web_shot_cooldown > 0.0 || env.flies.is_none() {
            return false;
        }
        let can_trap = self.has_skill("shoot_web");
        let can_wall = self.has_skill("wall_web");
        if !(can_trap || can_wall) {
            return false;
        }
        let reaction = self.pf("reaction_radius", 360.0);
        let web_shooter = self.acts_as_web_shooter();
        let range_mult = self.pf("web_shot_range_mult", if web_shooter { 0.85 } else { 0.5 });
        let shot_range = (self.kin.size * 4.0).max(reaction * range_mult);
        if d > shot_range || d < self.kin.size * 1.5 {
            return false;
        }
        let chance = self.pf("web_shot_chance", if web_shooter { 0.6 } else { 0.12 });
        let chance = clamp(chance + self.mood.arousal * 0.15, 0.0, 0.97);
        if self.rng.random() >= chance {
            return false;
        }
        let mut kind = ShotKind::Trap;
        if can_wall && (!can_trap || self.rng.random() < self.pf("wall_web_bias", 0.3)) {
            kind = ShotKind::Wall;
        }
        self.enter_web_aim(foe.x, foe.y, kind, None, Some(foe.id), env);
        true
    }

    /// `_update_job_state(dt, mx, my)`: apply a job's work intent.
    pub fn update_job_state(&mut self, dt: f64) -> bool {
        let state_for_mode = job_mode_state(if self.job_mode.is_empty() { "idle" } else { &self.job_mode });
        self.job_busy = self.job_outranked_by_personality();
        let target = self.job_target;
        let Some(new_state) = state_for_mode.filter(|s| !self.job_busy && (*s == State::JobBuild || target.is_some()))
        else {
            self.release_job_state();
            return false;
        };
        self.state = new_state;
        let (x, y) = (self.kin.x, self.kin.y);
        let travel = |c: &mut Creature, speed: f64| {
            let (tx, ty) = target.unwrap();
            c.kin.motion_paused = false;
            c.kin.target_x = tx;
            c.kin.target_y = ty;
            c.kin.target_heading = angle_to(x, y, tx, ty);
            c.kin.speed = speed * c.speed_mult();
        };
        let hold = |c: &mut Creature, aim_rate: f64| {
            c.kin.motion_paused = true;
            c.kin.speed = 0.0;
            c.kin.target_x = x;
            c.kin.target_y = y;
            c.aim_intent = (c.aim_intent + dt.max(0.0) * aim_rate).min(1.0);
        };
        match new_state {
            State::JobTravel => travel(self, 52.0),
            State::JobBuild => hold(self, 0.9),
            State::JobPatrol => {
                let (tx, ty) = target.unwrap();
                self.kin.motion_paused = false;
                self.kin.target_x = tx;
                self.kin.target_y = ty;
                self.kin.target_heading = match self.job_facing {
                    Some(f) => f,
                    None => angle_to(x, y, tx, ty),
                };
                self.kin.speed = 38.0 * self.speed_mult();
            }
            State::JobGuardAlert => {
                travel(self, 72.0);
                self.aim_intent = (self.aim_intent + dt.max(0.0) * 2.0).min(1.0);
            }
            State::JobScoutTravel => travel(self, 44.0),
            State::JobScoutReport => hold(self, 0.7),
            State::JobWebTravel => travel(self, 50.0),
            State::JobWebRepair | State::JobWebWeave => hold(self, 0.9),
            State::JobHuntPatrol => travel(self, 60.0),
            State::JobHuntReturn => travel(self, 85.0),
            _ => {}
        }
        true
    }

    /// `_job_outranked_by_personality(mode)`
    pub fn job_outranked_by_personality(&self) -> bool {
        if self.state.is_in(&JOB_PREEMPTING_STATES) {
            return true;
        }
        self.hunting_prey && !matches!(self.job_mode.as_str(), "guard_alert" | "hunt_return")
    }

    /// `_release_job_state()`
    pub fn release_job_state(&mut self) -> bool {
        if !self.state.is_in(&JOB_STATES) {
            return false;
        }
        self.aim_intent = 0.0;
        self.enter_idle();
        true
    }
}
