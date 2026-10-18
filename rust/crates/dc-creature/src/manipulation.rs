//! Port of the direct-manipulation and runtime-settings methods of
//! `creature/core.py`: drag and throw, hit testing, size and screen changes,
//! skills, mood mode, social switch and gait style.

use crate::constants::*;
use crate::creature::{Creature, LabelStyle};
use crate::pymath::{angle_lerp, angle_to, clamp, clamp_point, hypot, py_float};
use crate::state::State;
use crate::world::{CreatureId, ShotKind, WebWorldApi};
use dc_kinematics::{ContactState, Kinematics};
use serde_json::Value;

impl Creature {
    /// `start_drag(mx, my)`: picked up by the pointer.
    pub fn start_drag(&mut self, mx: f64, my: f64) {
        self.cursor_pressure = (self.cursor_pressure + CURSOR_PRESSURE_PER_GRAB).min(1.0);
        self.dragging = true;
        self.phase_scheduler.cancel();
        self.state = State::Dragged;
        self.kin.motion_paused = true;
        self.kin.speed = 0.0;
        self.kin.current_speed = 0.0;
        self.kin.inertia_vx = 0.0;
        self.kin.inertia_vy = 0.0;
        self.kin.inertia_timer = 0.0;
        self.startled_timer = 1.2;
        self.startle_highlight_suppression = 1.35;
        self.held_leg_relax = 0.0;
        self.held_pose_clock = 0.0;
        self.held_drag_response = 0.0;
        self.held_drag_sway_x = 0.0;
        self.held_drag_sway_y = 0.0;
        self.held_prev_drag_vx = 0.0;
        self.held_prev_drag_vy = 0.0;
        self.held_drag_accel_x = 0.0;
        self.held_drag_accel_y = 0.0;
        self.held_release_timer = 0.0;
        self.grab_offset_x = self.kin.x - mx;
        self.grab_offset_y = self.kin.y - my;
        self.drag_vel_x = 0.0;
        self.drag_vel_y = 0.0;
        self.state_timer = 0.25;
        self.register_camouflage_touch(None);
        for leg in &mut self.kin.legs {
            leg.stepping = false;
            leg.pending_step = false;
            leg.lift = 0.0;
            leg.held_spring_x = 0.0;
            leg.held_spring_y = 0.0;
            leg.held_spring_vx = 0.0;
            leg.held_spring_vy = 0.0;
            // Python writes "air"; the gait only ever tests `== "stance"`,
            // so `Swing` behaves identically (dc-kinematics has no `Air`).
            leg.contact_state = ContactState::Swing;
            leg.support_weight = 0.0;
            leg.contact_age = 0.0;
            leg.step_cooldown = 0.0;
        }
    }

    /// `drag_to(dt, mx, my)`
    pub fn drag_to(&mut self, dt: f64, mx: f64, my: f64) {
        let dt = clamp(dt, 0.001, 0.05);
        if !self.dragging {
            return;
        }
        let (target_x, target_y) = clamp_point(
            mx + self.grab_offset_x,
            my + self.grab_offset_y,
            self.margin * 0.25,
            self.screen_w,
            self.screen_h,
        );
        let dx = target_x - self.kin.x;
        let dy = target_y - self.kin.y;
        let raw_vx = dx / dt;
        let raw_vy = dy / dt;
        self.drag_vel_x = self.drag_vel_x * 0.58 + raw_vx * 0.42;
        self.drag_vel_y = self.drag_vel_y * 0.58 + raw_vy * 0.42;
        self.kin.x = target_x;
        self.kin.y = target_y;
        self.kin.target_x = target_x;
        self.kin.target_y = target_y;
        let drag_speed = hypot(self.drag_vel_x, self.drag_vel_y);
        if drag_speed > 18.0 {
            self.kin.target_heading = self.drag_vel_y.atan2(self.drag_vel_x);
            if self.acts_as_drifter() {
                self.prime_drift(clamp(drag_speed / 700.0, 0.25, 1.15), None);
                self.kin.target_heading += self.kin.drift.drift_dir * clamp(drag_speed / 900.0, 0.0, 0.56);
            }
        }
        self.kin.heading = angle_lerp(self.kin.heading, self.kin.target_heading, self.kin.turn_rate * 2.2 * dt);
        if let Some(prey) = self.prey
            && prey.alive
            && !prey.eaten
        {
            self.kin.target_heading = angle_to(self.kin.x, self.kin.y, prey.x, prey.y);
            self.kin.heading = angle_lerp(self.kin.heading, self.kin.target_heading, self.kin.turn_rate * 2.6 * dt);
            self.set_focus(prey.x, prey.y, 1.0);
        }
        self.kin.vel_x = self.drag_vel_x;
        self.kin.vel_y = self.drag_vel_y;
        self.kin.current_speed = clamp(drag_speed, 0.0, 260.0);
        self.startled_timer = 1.2;
        self.register_camouflage_touch(Some(5.0));
        let carry = if drag_speed < 520.0 { 0.68 } else { 0.56 };
        self.kin.translate_leg_world_points(dx, dy, carry);
        if drag_speed > 45.0 && self.rng.random() < 0.82 {
            self.panic_rehome_legs(clamp(drag_speed / 800.0, 0.35, 1.0));
        }
    }

    /// `release_drag(mx, my)`: let go, maybe with a throw.
    pub fn release_drag(&mut self, mx: f64, my: f64) {
        if !self.dragging {
            return;
        }
        self.dragging = false;
        self.held_release_timer = 0.24;
        self.kin.motion_paused = false;
        let speed = hypot(self.drag_vel_x, self.drag_vel_y);
        if speed > 1.0 {
            let max_throw: f64 = 980.0;
            let scale = max_throw.min(speed) / speed;
            self.kin.inertia_vx = self.drag_vel_x * scale * 0.78;
            self.kin.inertia_vy = self.drag_vel_y * scale * 0.78;
            self.kin.inertia_timer = clamp(speed / 760.0, 0.25, 1.25);
            self.kin.target_heading = self.kin.inertia_vy.atan2(self.kin.inertia_vx);
        } else {
            self.kin.inertia_vx = 0.0;
            self.kin.inertia_vy = 0.0;
            self.kin.inertia_timer = 0.0;
        }
        self.register_camouflage_touch(None);
        self.enter_startled(mx, my);
        self.startle_highlight_suppression = self.startle_highlight_suppression.max(1.35);
        for i in 0..self.kin.legs.len() {
            self.kin.soft_limit_leg_state(i, 0.45);
        }
        self.panic_rehome_legs(clamp(speed / 850.0, 0.55, 1.0));
    }

    /// `hit_test(mx, my)`: is a press close enough to grab this spider?
    pub fn hit_test(&self, mx: f64, my: f64) -> bool {
        let (local_f, local_s) = self.kin.world_to_body_local(mx, my);
        let ap = self.appearance();
        let pair = |key: &str, default: [f64; 2]| -> [f64; 2] {
            match ap.get(key).and_then(Value::as_array) {
                Some(a) if a.len() >= 2 => [py_float(&a[0]).unwrap_or(default[0]), py_float(&a[1]).unwrap_or(default[1])],
                _ => default,
            }
        };
        let size = self.kin.size;
        let abdomen_scale = pair("abdomen_scale", [0.96, 0.86]);
        let ceph_scale = pair("cephalothorax_scale", [0.76, 0.70]);
        let abdomen_offset_x = ap.f64_or("abdomen_offset_x", -0.18) * size;
        let ceph_offset_x = ap.f64_or("cephalothorax_offset_x", 0.38) * size;
        for (cx, rx, ry) in [
            (abdomen_offset_x, size * abdomen_scale[0] * 0.62, size * abdomen_scale[1] * 0.58),
            (ceph_offset_x, size * ceph_scale[0] * 0.62, size * ceph_scale[1] * 0.58),
        ] {
            let a = (local_f - cx) / rx.max(1.0);
            let b = local_s / ry.max(1.0);
            if a * a + b * b <= 1.25 {
                return true;
            }
        }
        let leg_pick_radius = (size * 0.16).max(5.0);
        let pf = self.split_playfield();
        let ctx = self.kctx(pf.as_ref().map(|p| p as &dyn dc_kinematics::PlayfieldClamp));
        for i in 0..self.kin.legs.len() {
            let (ax, ay) = self.kin.leg_attach(i);
            let (fx, fy) = self.kin.visual_foot_for_render(i, &ctx);
            let (kx, ky) = self.kin.solve_knee(i, ax, ay, fx, fy, &ctx);
            if Kinematics::point_segment_distance(mx, my, ax, ay, kx, ky) <= leg_pick_radius {
                return true;
            }
            if Kinematics::point_segment_distance(mx, my, kx, ky, fx, fy) <= leg_pick_radius {
                return true;
            }
        }
        false
    }

    /// `set_size_scale(size_scale)`: rescale and rehome the legs.
    pub fn set_size_scale(&mut self, size_scale: f64) {
        let new_scale = clamp(size_scale, 0.45, 2.25);
        if (new_scale - self.size_scale).abs() < 1e-4 {
            return;
        }
        self.size_scale = new_scale;
        self.apply_progression_stats(false, false);
        (self.kin.x, self.kin.y) =
            clamp_point(self.kin.x, self.kin.y, self.margin * 0.4, self.screen_w, self.screen_h);
        (self.kin.target_x, self.kin.target_y) =
            clamp_point(self.kin.target_x, self.kin.target_y, self.margin * 0.4, self.screen_w, self.screen_h);
        self.initialize_legs();
        self.startled_timer = self.startled_timer.max(0.45);
    }

    /// `resize_screen(w, h)`
    pub fn resize_screen(&mut self, w: f64, h: f64) {
        self.screen_w = w.trunc().max(200.0);
        self.screen_h = h.trunc().max(200.0);
        (self.kin.x, self.kin.y) = clamp_point(self.kin.x, self.kin.y, self.margin, self.screen_w, self.screen_h);
        (self.kin.target_x, self.kin.target_y) =
            clamp_point(self.kin.target_x, self.kin.target_y, self.margin, self.screen_w, self.screen_h);
    }

    /// `set_neighbors(neighbors)`
    pub fn set_neighbors(&mut self, neighbors: Vec<CreatureId>) {
        self.neighbors = neighbors;
    }

    /// `set_mood_mode(mode)`
    pub fn set_mood_mode(&mut self, mode: &str) {
        let mode = if mode.is_empty() { "auto".to_string() } else { mode.to_lowercase() };
        let baseline = if mode != "auto" { mode.clone() } else { self.p().str_or("id", "auto") };
        self.mood_mode = mode;
        self.mood.set_baseline_named(&baseline);
    }

    /// `set_allow_social(enabled)`
    pub fn set_allow_social(&mut self, enabled: bool) {
        self.allow_social = enabled;
        if !self.allow_social && self.state == State::Play {
            self.social_target = None;
            self.enter_idle();
        }
    }

    /// `set_gait_style(style)`
    pub fn set_gait_style(&mut self, style: &str) {
        self.kin.gait_style = normalize_gait_style(Some(style));
    }

    /// `set_name(name)`
    pub fn set_name(&mut self, name: &str) {
        self.name = name.trim().to_string();
    }

    /// `set_label_style`: `"full"`, `"quiet"` or `"hero"`.
    pub fn set_label_style(&mut self, style: LabelStyle) {
        self.label_style = style;
    }

    /// `set_skill_enabled(skill_id, enabled)`: switching off a skill in use
    /// leaves that state at once. `webs` is the web world, which an abandoned
    /// weave or repair is handed back to.
    pub fn set_skill_enabled(&mut self, skill_id: &str, enabled: bool, webs: Option<&mut dyn WebWorldApi>) {
        self.skills.set_enabled(skill_id, enabled);
        let sid = skill_id.trim().to_lowercase();
        if matches!(self.state, State::WebAim | State::WebShot) && !enabled {
            let needed = if self.web_shot_kind == ShotKind::Wall { "wall_web" } else { "shoot_web" };
            if sid == needed {
                self.finish_web_shot(false);
                return;
            }
        }
        let active: &[&str] = match self.state {
            State::Approach => &["approach"],
            State::Wander => &["wander"],
            State::Chase => &["chase"],
            State::Retreat => &["run_away"],
            State::Observe => &["observe"],
            State::Inspect => &["inspect"],
            State::Cuddle => &["cuddle"],
            State::Aim => &["prepare_jump_attack", "jump"],
            State::Coil | State::Jump | State::Land => &["jump"],
            State::Catch => &["prepare_jump_attack"],
            State::Play => &["social_play"],
            State::Zoom => &["zoomies"],
            State::DriftRun => &["drift"],
            State::Roll => &["roll"],
            State::WeaveApproach | State::Weave | State::RepairApproach | State::Repair => &["weave_web"],
            State::WebApproach | State::WebWalk => &["web_walk"],
            _ => &[],
        };
        if active.contains(&sid.as_str()) && !enabled {
            self.social_target = None;
            self.airborne = false;
            self.jump_z = 0.0;
            self.kin.roll_spin = 0.0;
            self.kin.roll_tuck = 0.0;
            // The two states are exclusive, so at most one hand-back runs.
            if matches!(self.state, State::WeaveApproach | State::Weave) {
                self.abandon_weaving(webs);
            } else if matches!(self.state, State::RepairApproach | State::Repair) {
                self.abandon_repair(webs);
            }
            self.web_target = None;
            self.enter_idle();
        }
    }
}
