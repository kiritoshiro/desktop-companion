//! Port of `Creature.update` (`creature/core.py:1285`): the per-frame entry
//! point, and `_apply_cage_bounds`.
//!
//! The manager's half of the contract (what `CreatureManager.update` did
//! around this call) is documented on [`Creature::update`] and in
//! `PORT_STATUS.md`.

use crate::bump;
use crate::creature::Creature;
use crate::kin_ctx;
use crate::pymath::{clamp, cursor_is_threatening, hypot};
use crate::state::State;
use crate::world::{Command, Env, Population};

/// What one [`Creature::update`] did, for the manager.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateOutcome {
    /// The ordinary frame ran.
    Ran,
    /// The creature is dead: nothing ran past the timers. The manager
    /// removes it at the end of the tick and leaves remains.
    Dead,
    /// A player controller drives this creature (`player_control is not
    /// None`): energy regenerated and nothing else ran. The manager must now
    /// call the controller's `update(dt)` with the same (clamped) `dt`, as
    /// Python's `self.player_control.update(dt)` did.
    PlayerControlled,
}

/// States in which a fast cursor has to be faster and closer to count as a
/// threat (`calm_states` in `update`).
const CALM_STATES: [State; 17] = [
    State::Cuddle,
    State::Play,
    State::Inspect,
    State::Aim,
    State::Coil,
    State::Catch,
    State::Land,
    State::Roll,
    State::Feed,
    State::Weave,
    State::WeaveApproach,
    State::WebWalk,
    State::WebApproach,
    State::WebAim,
    State::WebShot,
    State::RepairApproach,
    State::Repair,
];

impl Creature {
    /// `update(dt, mx, my, sw, sh)`.
    ///
    /// `(mx, my)` is the creature's focus point: the real cursor, or the
    /// prey's position while hunting (the manager's `_creature_focus`).
    /// Before calling, the manager keeps these fields current, as Python's
    /// manager wrote them onto the creature: `neighbors`, `prey`/
    /// `hunting_prey` (prey view refreshed every frame), `foe`, `flee_timer`/
    /// `flee_from`/`recovering`/`escaped_timer`, `job_mode`/`job_target`/
    /// `job_alert_target`/`job_base_id`/`job_facing`, `cage`, `playfield`,
    /// `force_show_*`, `team_stances`/`team_profiles`, `allow_social`.
    /// `env.population` must hold every creature's [`crate::world::CreatureView`];
    /// refresh this creature's entry right after the call (Python updated
    /// creatures in sequence, so later ones saw earlier ones' new state), and
    /// apply `env.commands` with [`Creature::apply_command`] on their targets.
    pub fn update(&mut self, dt: f64, mx: f64, my: f64, sw: f64, sh: f64, env: &mut Env<'_>) -> UpdateOutcome {
        let dt = clamp(dt, 0.001, 0.05);
        self.update_regal(dt, mx, my, env.population);
        self.resize_screen(sw, sh);
        self.hurt_flash = (self.hurt_flash - dt * 1.6).max(0.0);
        self.attack_cooldown = (self.attack_cooldown - dt).max(0.0);
        self.webbed_timer = (self.webbed_timer - dt).max(0.0);
        self.update_web_struggle(dt);
        self.update_damage_numbers(dt);
        self.flee_timer = (self.flee_timer - dt).max(0.0);
        if self.flee_timer <= 0.0 {
            self.flee_from = None;
        }
        if !self.webbed() {
            self.webbed_by = None;
        }
        if self.dead {
            self.kin.motion_paused = true;
            self.kin.current_speed = 0.0;
            return UpdateOutcome::Dead;
        }
        if self.player_control {
            self.regenerate_energy(dt);
            return UpdateOutcome::PlayerControlled;
        }
        if self.hunting_prey {
            self.pursue_prey(mx, my, env);
        } else if self.foe.is_some() {
            self.pursue_foe(env);
        }

        let (mut cursor_vx, mut cursor_vy) = match (self.prev_mx, self.prev_my) {
            (Some(pmx), Some(pmy)) => ((mx - pmx) / dt, (my - pmy) / dt),
            _ => (0.0, 0.0),
        };
        cursor_vx = cursor_vx * 0.7 + self.prev_cursor_vx * 0.3;
        cursor_vy = cursor_vy * 0.7 + self.prev_cursor_vy * 0.3;
        self.prev_mx = Some(mx);
        self.prev_my = Some(my);
        self.prev_cursor_vx = cursor_vx;
        self.prev_cursor_vy = cursor_vy;
        self.nope_cooldown = (self.nope_cooldown - dt).max(0.0);
        self.weave_cooldown = (self.weave_cooldown - dt).max(0.0);
        self.web_walk_cooldown = (self.web_walk_cooldown - dt).max(0.0);
        self.web_shot_cooldown = (self.web_shot_cooldown - dt).max(0.0);
        self.feed_cooldown = (self.feed_cooldown - dt).max(0.0);
        self.pounce_cooldown = (self.pounce_cooldown - dt).max(0.0);
        self.trap_shot_cooldown = (self.trap_shot_cooldown - dt).max(0.0);
        self.regenerate_energy(dt);
        if !self.dragging {
            self.phase_scheduler.tick(dt);
        }

        // Release webs the live state no longer works on.
        if self.weaving_web.is_some() && !matches!(self.state, State::WeaveApproach | State::Weave) {
            self.abandon_weaving(env.webs_mut());
        }
        if self.repairing_web.is_some() && !matches!(self.state, State::RepairApproach | State::Repair) {
            self.abandon_repair(env.webs_mut());
        }
        if self.web_target.is_some() && !matches!(self.state, State::WebApproach | State::WebWalk) {
            self.web_target = None;
        }
        self.reconcile_roll();

        if self.desktop_fully_hidden {
            self.social_target = None;
        } else if let Some(t) = self.social_target
            && env.population.get(t).is_some_and(|v| v.desktop_fully_hidden)
        {
            self.social_target = None;
        }

        if self.social_target.is_none() {
            self.set_focus(mx, my, self.focus_strength.max(0.25));
        }

        if self.dragging {
            self.update_held(dt, mx, my, env);
            return UpdateOutcome::Ran;
        }

        self.startled_timer = (self.startled_timer - dt).max(0.0);
        self.held_release_timer = (self.held_release_timer - dt).max(0.0);
        self.startle_highlight_suppression = (self.startle_highlight_suppression - dt).max(0.0);

        let m = &self.mood;
        let bravery =
            clamp(0.5 + m.happy() * 0.4 + m.affection * 0.5 + m.valence.max(0.0) * 0.2 - m.sad() * 0.5, 0.0, 1.6);
        let base_radius = self.pf("threat_radius", 180.0);
        let base_speed = self.pf("threat_cursor_speed", 900.0);
        let mut eff_radius = base_radius * clamp(1.0 - bravery * 0.32, 0.45, 1.0);
        let mut eff_speed = base_speed * clamp(1.0 + bravery * 0.55, 1.0, 2.2);
        if self.state.is_in(&CALM_STATES) {
            eff_speed *= 1.9;
            eff_radius *= 0.6;
        }
        if self.cursor_triggers_nope_escape(mx, my, cursor_vx, cursor_vy) && !self.hunting_prey {
            self.social_target = None;
            self.enter_nope_escape(mx, my, false);
        }

        if self.has_skill("run_away")
            && !self.hunting_prey
            && !self.airborne
            && !matches!(self.state, State::Retreat | State::Startled | State::Jump | State::Land)
            && cursor_is_threatening(self.kin.x, self.kin.y, mx, my, cursor_vx, cursor_vy, eff_radius, eff_speed)
        {
            self.social_target = None;
            bump!(self.mood, arousal = 0.25, valence = -0.1);
            self.enter_retreat(mx, my);
        }

        if self.airborne {
            self.update_jump(dt);
        } else {
            self.update_state(dt, mx, my, env);
            kin_ctx!(self, |kin, ctx, rng| {
                kin.aim_at_a_real_screen(&ctx);
                if kin.spider_grounded_mode_allowed(&ctx) {
                    kin.update_spider_grounded_frame(dt, &ctx, rng);
                } else {
                    if kin.spider_locomotion_active {
                        kin.spider_locomotion_active = false;
                    }
                    kin.move_body(dt, &ctx, rng);
                }
            });
        }
        self.reconcile_roll();
        if self.cage.is_some() && !self.dragging {
            self.apply_cage_bounds();
        }
        self.update_legs(dt);
        self.update_mood(dt);
        let foe = self.foe.and_then(|id| env.population.get(id));
        self.update_posture(dt, foe);
        self.update_antennae(dt);
        let focus = self.phase_focus_context(mx, my, env.population);
        self.sync_scheduled_phase(focus);
        UpdateOutcome::Ran
    }

    /// The dragged branch of `update`: the held pose, and an in-progress web
    /// shot carried on from the hand.
    fn update_held(&mut self, dt: f64, mx: f64, my: f64, env: &mut Env<'_>) {
        self.startled_timer = 1.2;
        self.set_focus(mx, my, 1.0);
        let speed01 = clamp(self.kin.current_speed / 260.0, 0.0, 1.0);
        self.held_leg_relax = (self.held_leg_relax + dt * 4.5).min(1.0);
        let response_target = clamp(hypot(self.drag_vel_x, self.drag_vel_y) / 420.0, 0.0, 1.0);
        let response_blend = 1.0 - (-dt * 7.0).exp();
        self.held_drag_response += (response_target - self.held_drag_response) * response_blend;
        self.held_pose_clock += dt * (1.15 + self.held_drag_response * 2.35);
        let drag_accel_x = (self.drag_vel_x - self.held_prev_drag_vx) / dt;
        let drag_accel_y = (self.drag_vel_y - self.held_prev_drag_vy) / dt;
        self.held_prev_drag_vx = self.drag_vel_x;
        self.held_prev_drag_vy = self.drag_vel_y;
        let accel_blend = 1.0 - (-dt * 10.0).exp();
        self.held_drag_accel_x += (drag_accel_x - self.held_drag_accel_x) * accel_blend;
        self.held_drag_accel_y += (drag_accel_y - self.held_drag_accel_y) * accel_blend;
        let sway_target_x = clamp(self.drag_vel_x / 520.0, -1.0, 1.0) * self.kin.size * 0.15;
        let sway_target_y = clamp(self.drag_vel_y / 520.0, -1.0, 1.0) * self.kin.size * 0.07;
        let sway_blend = 1.0 - (-dt * 8.0).exp();
        self.held_drag_sway_x += (sway_target_x - self.held_drag_sway_x) * sway_blend;
        self.held_drag_sway_y += (sway_target_y - self.held_drag_sway_y) * sway_blend;
        kin_ctx!(self, |kin, ctx, _rng| kin.update_held_leg_springs(dt, &ctx));
        self.kin.bob_phase += dt * 1.2;
        self.kin.breath_phase += dt * (1.45 + speed01 * 0.20);
        self.kin.body_bob = 0.0;
        self.kin.body_sway = 0.0;
        self.kin.abdomen_pulse = (self.kin.breath_phase * 1.05).sin() * 0.032;
        self.kin.ceph_pulse = -(self.kin.breath_phase * 0.92 + 0.85).sin() * 0.016;
        bump!(self.mood, arousal = dt * 0.7, valence = -dt * 0.25);
        if matches!(self.state, State::WebAim | State::WebShot) {
            self.state_timer -= dt;
            if self.state == State::WebAim {
                self.update_web_aim(dt, mx, my, env);
            } else {
                self.update_web_shot(dt, mx, my);
            }
        }
        self.update_legs(dt);
        self.update_mood(dt);
        let foe = self.foe.and_then(|id| env.population.get(id));
        self.update_posture(dt, foe);
        self.update_antennae(dt);
    }

    /// `_reconcile_roll()`, with its `squash = 1.0` side effect.
    pub fn reconcile_roll(&mut self) {
        if kin_ctx!(self, |kin, ctx, _rng| kin.reconcile_roll(&ctx)) {
            self.squash = 1.0;
        }
    }

    /// `_update_legs(dt)`
    pub fn update_legs(&mut self, dt: f64) {
        kin_ctx!(self, |kin, ctx, rng| kin.update_legs(dt, &ctx, rng));
    }

    /// `_apply_cage_bounds()`: keep a caged spider inside its enclosure.
    pub fn apply_cage_bounds(&mut self) {
        let Some(cage) = self.cage else { return };
        let inset = self.kin.size * 0.85;
        let (new_x, new_y) = cage.clamp_center(self.kin.x, self.kin.y, inset);
        let dx = new_x - self.kin.x;
        let dy = new_y - self.kin.y;
        if dx != 0.0 || dy != 0.0 {
            let k = &mut self.kin;
            k.x = new_x;
            k.y = new_y;
            if dx > 0.0 {
                k.inertia_vx = k.inertia_vx.max(0.0);
                k.vel_x = k.vel_x.max(0.0);
            } else if dx < 0.0 {
                k.inertia_vx = k.inertia_vx.min(0.0);
                k.vel_x = k.vel_x.min(0.0);
            }
            if dy > 0.0 {
                k.inertia_vy = k.inertia_vy.max(0.0);
                k.vel_y = k.vel_y.max(0.0);
            } else if dy < 0.0 {
                k.inertia_vy = k.inertia_vy.min(0.0);
                k.vel_y = k.vel_y.min(0.0);
            }
            k.translate_leg_world_points(dx, dy, 0.6);
        }
        (self.kin.target_x, self.kin.target_y) = cage.clamp_center(self.kin.target_x, self.kin.target_y, inset);
    }

    /// One step of the creature's own pipeline while a player controller
    /// drives it (`PlayerController.advance_pose`).
    pub fn advance_pose(&mut self, dt: f64) {
        if self.airborne {
            self.update_jump(dt);
        } else {
            kin_ctx!(self, |kin, ctx, rng| {
                kin.aim_at_a_real_screen(&ctx);
                if kin.spider_grounded_mode_allowed(&ctx) {
                    kin.update_spider_grounded_frame(dt, &ctx, rng);
                } else {
                    kin.move_body(dt, &ctx, rng);
                }
            });
        }
        if self.cage.is_some() {
            self.apply_cage_bounds();
        }
        self.reconcile_roll();
        self.update_legs(dt);
        self.update_mood(dt);
        self.update_posture(dt, None);
        self.update_antennae(dt);
    }
}

impl Creature {
    /// Apply a [`Command`] another creature issued during its update (Python
    /// wrote straight onto this creature: `target.mood.bump(...)`,
    /// `target.enter_play(self, role=...)`). Commands addressed to someone
    /// else are ignored.
    pub fn apply_command(&mut self, cmd: &Command, pop: &Population) {
        match *cmd {
            Command::BumpMood { target, bump } if target == self.id => self.mood.bump(bump),
            Command::EnterPlay { target, partner, role } if target == self.id && self.state != State::Play => {
                self.enter_play(partner, Some(role), pop);
            }
            _ => {}
        }
    }
}
