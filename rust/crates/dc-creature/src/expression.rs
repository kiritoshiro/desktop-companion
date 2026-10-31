//! Port of `creature/expression.py`: mood coupling, posture, combat pose and
//! the pedipalp chain.

use crate::constants::*;
use crate::creature::Creature;
use crate::mood::antenna_drive_from_mood;
use crate::pymath::{Dict, clamp, hypot, py_float, py_int};
use crate::state::State;
use crate::world::CreatureView;
use serde_json::Value;

/// `ease(u)` inside `strike_phase`.
fn ease(u: f64) -> f64 {
    let u = clamp(u, 0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// A list of finite numbers, `segments` long, or `None` (malformed).
fn float_list(raw: Option<&Value>, segments: usize, nonneg: bool) -> Option<Vec<f64>> {
    let items = raw?.as_array()?;
    let mut out = Vec::with_capacity(segments);
    for v in items.iter().take(segments) {
        out.push(py_float(v)?);
    }
    if out.len() != segments || out.iter().any(|v| !v.is_finite() || (nonneg && *v < 0.0)) {
        return None;
    }
    Some(out)
}

impl Creature {
    /// `_update_mood(dt)`
    pub fn update_mood(&mut self, dt: f64) {
        self.social_cooldown = (self.social_cooldown - dt).max(0.0);
        let speed01 = clamp(self.kin.current_speed / 160.0, 0.0, 1.0);
        if speed01 > 0.25 {
            crate::bump!(self.mood, arousal = dt * 0.12 * speed01);
        }
        let relax_rate = if matches!(self.state, State::Idle | State::Wander) { 0.6 } else { 0.35 };
        self.mood.relax(dt, relax_rate);

        let decay = (-dt * 3.2).exp();
        if !matches!(self.state, State::Aim | State::WebAim) {
            self.aim_intent *= decay;
        }
        if self.state != State::Inspect {
            self.kin.inspect_intent *= decay;
        }
        if self.state != State::Cuddle {
            self.cuddle_intent *= decay;
        }
        if self.state != State::Catch {
            self.kin.catch_blend *= (-dt * 5.0).exp();
        }

        self.blink_timer -= dt;
        if self.expression_blink > 0.0 {
            self.expression_blink = (self.expression_blink - dt * 7.0).max(0.0);
        } else if self.blink_timer <= 0.0 {
            self.expression_blink = 1.0;
            self.blink_timer = self.rng.uniform(2.0, 6.0);
        }
    }

    /// `_update_combat_pose(dt)`; `foe` is the published foe's view.
    pub fn update_combat_pose(&mut self, dt: f64, foe: Option<&CreatureView>) {
        let engaged = foe.is_some_and(|f| !f.dead) && !self.dead && !self.dragging && !self.fleeing();
        if engaged {
            let f = foe.unwrap();
            let (dx, dy) = (f.x - self.kin.x, f.y - self.kin.y);
            let mut span = hypot(dx, dy);
            if span == 0.0 {
                span = 1.0;
            }
            self.combat_face_x = dx / span;
            self.combat_face_y = dy / span;
            self.combat_stance = (self.combat_stance + dt * STANCE_RISE_PER_SECOND).min(1.0);
            self.rear = self.rear.max(self.combat_stance * 0.85);
        } else {
            self.combat_stance = (self.combat_stance - dt * STANCE_FALL_PER_SECOND).max(0.0);
        }
        self.lunge -= self.lunge * (dt * LUNGE_DECAY_PER_SECOND).min(1.0);
        if self.lunge.abs() < 0.002 {
            self.lunge = 0.0;
        }
        if let Some(clock) = self.strike_clock {
            let clock = clock + dt;
            self.strike_clock = if clock < STRIKE_DURATION { Some(clock) } else { None };
            let (_, lift) = self.strike_phase();
            self.rear = self.rear.max(lift * 0.9);
        }
    }

    /// `strike_landed(foe)`: bite at a foe at `(fx, fy)`.
    pub fn strike_landed(&mut self, fx: f64, fy: f64) {
        self.face_for_combat(fx, fy);
        self.begin_strike(fx, fy);
    }

    /// `begin_strike(tx, ty)`: the bite animation towards a point.
    pub fn begin_strike(&mut self, tx: f64, ty: f64) {
        let (mut dx, mut dy) = (tx - self.kin.x, ty - self.kin.y);
        let mut span = hypot(dx, dy);
        if span < 1e-6 {
            dx = self.kin.heading.cos();
            dy = self.kin.heading.sin();
            span = 1.0;
        }
        self.strike_face = (dx / span, dy / span);
        self.strike_clock = Some(1.0 / 60.0);
    }

    /// `strike_phase()`: (push in LUNGE_REACH units, lift 0..1).
    pub fn strike_phase(&self) -> (f64, f64) {
        let Some(clock) = self.strike_clock else { return (0.0, 0.0) };
        if clock < STRIKE_WINDUP {
            let u = ease(clock / STRIKE_WINDUP);
            return (-STRIKE_DRAW_BACK * u, u);
        }
        if clock < STRIKE_WINDUP + STRIKE_SNAP {
            let u = ease((clock - STRIKE_WINDUP) / STRIKE_SNAP);
            return (-STRIKE_DRAW_BACK + (STRIKE_OVERSHOOT + STRIKE_DRAW_BACK) * u, 1.0);
        }
        let rest = STRIKE_DURATION - STRIKE_WINDUP - STRIKE_SNAP;
        let u = ease((clock - STRIKE_WINDUP - STRIKE_SNAP) / rest);
        (STRIKE_OVERSHOOT * (1.0 - u), 1.0 - u)
    }

    /// `blow_taken(foe)`: knock the body back off its feet.
    pub fn blow_taken(&mut self, fx: f64, fy: f64) {
        self.face_for_combat(fx, fy);
        self.lunge = -1.0;
    }

    /// `_face_for_combat(foe)`
    pub fn face_for_combat(&mut self, fx: f64, fy: f64) {
        let (dx, dy) = (fx - self.kin.x, fy - self.kin.y);
        let mut span = hypot(dx, dy);
        if span == 0.0 {
            span = 1.0;
        }
        self.combat_face_x = dx / span;
        self.combat_face_y = dy / span;
    }

    /// `combat_body_offset()`: the body relative to its planted feet.
    pub fn combat_body_offset(&self) -> (f64, f64) {
        let (mut ox, mut oy) = (0.0, 0.0);
        let (push, _) = self.strike_phase();
        if push != 0.0 {
            let (fx, fy) = self.strike_face;
            ox += fx * push * LUNGE_REACH * self.kin.size;
            oy += fy * push * LUNGE_REACH * self.kin.size;
        }
        if self.lunge != 0.0 {
            let reach = if self.lunge > 0.0 { LUNGE_REACH } else { RECOIL_REACH };
            let back = self.lunge * reach * self.kin.size;
            ox += self.combat_face_x * back;
            oy += self.combat_face_y * back;
        }
        (ox, oy)
    }

    /// `_update_posture(dt)`
    pub fn update_posture(&mut self, dt: f64, foe: Option<&CreatureView>) {
        let m = self.mood;
        self.kin.drift.wiggle_burst = (self.kin.drift.wiggle_burst - dt * 1.6).max(0.0);
        self.head_tilt *= (-dt * 2.5).exp();
        if !matches!(self.state, State::Aim | State::Coil | State::WebAim) {
            self.crouch = (self.crouch - dt * 3.0).max(0.0);
        }
        if !matches!(self.state, State::Cuddle | State::Aim) {
            self.rear = (self.rear - dt * 3.5).max(0.0);
        }
        self.update_combat_pose(dt, foe);
        self.kin.update_gait_spell(dt);
        if !self.dragging {
            self.cursor_pressure = (self.cursor_pressure - dt * CURSOR_PRESSURE_DECAY_PER_SECOND).max(0.0);
        }
        self.squash += (1.0 - self.squash) * (1.0 - (-dt * 10.0).exp());
        if self.land_recover > 0.0 {
            self.land_recover = (self.land_recover - dt).max(0.0);
        }

        let wiggle_burst = self.kin.drift.wiggle_burst;
        let mut base_wiggle = 0.02 + m.arousal * 0.05 + m.valence.max(0.0) * 0.04;
        if self.kin.current_speed > 30.0 {
            base_wiggle *= 0.5;
        }
        self.wiggle_amp = base_wiggle + wiggle_burst * 0.22;
        self.wiggle_phase += dt * (2.2 + m.arousal * 3.5 + wiggle_burst * 6.0);
        self.body_wiggle = self.wiggle_phase.sin() * self.wiggle_amp;
        let wag_drive = 0.03 + m.happy() * 0.07 + m.affection * 0.05 + wiggle_burst * 0.10;
        self.abdomen_wag = (self.wiggle_phase * 1.35 + 0.5).sin() * wag_drive;

        let gaze = clamp(self.focus_strength, 0.0, 1.0);
        let (tgt_f, tgt_s) = if gaze > 0.05 {
            let (lf, ls) = self.kin.world_to_body_local(self.focus_x, self.focus_y);
            let mag = 1e-3f64.max(hypot(lf, ls));
            (lf / mag, ls / mag)
        } else {
            (1.0, 0.0)
        };
        self.look_fwd += (tgt_f - self.look_fwd) * (1.0 - (-dt * 8.0).exp());
        self.look_side += (tgt_s - self.look_side) * (1.0 - (-dt * 8.0).exp());
    }

    /// `_antenna_aim_angle()`
    pub fn antenna_aim_angle(&self) -> f64 {
        let (lf, ls) = self.kin.world_to_body_local(self.focus_x, self.focus_y);
        ls.atan2(lf)
    }

    /// `_update_antennae(dt)`: the antenna clocks, and for hand-palp models
    /// the whole serial pedipalp chain.
    pub fn update_antennae(&mut self, dt: f64) {
        let m = self.mood;
        let speed01 = clamp(self.kin.current_speed / 160.0, 0.0, 1.0);
        let model = self.model.clone();
        let cfg = Dict::of(&model).dict("appearance").dict("antennae");
        let style = match cfg.0 {
            Some(_) => cfg.str_or("style", "").trim().to_lowercase(),
            None => String::new(),
        };
        let custom_hand_palps = matches!(style.as_str(), "front_leg" | "tarantula_front_legs" | "tarantula_hand_palps");
        let phase_rate = if self.dragging {
            0.0
        } else if custom_hand_palps {
            1.08 + m.arousal * 0.86 + m.curiosity * 0.58 + speed01 * 0.30
        } else {
            1.8 + m.arousal * 3.2 + m.curiosity * 1.6 + speed01 * 2.0
        };
        for i in 0..2 {
            self.antenna_phase[i] += dt * phase_rate;
        }
        if !custom_hand_palps {
            return;
        }
        let segments = match cfg.get("segments") {
            None => 5usize,
            Some(v) => py_int(v).map(|s| s.max(3) as usize).unwrap_or(5),
        };
        let dragging = self.dragging;
        let drive = antenna_drive_from_mood(
            &self.mood,
            if dragging { 0.0 } else { clamp(self.aim_intent, 0.0, 1.0) },
            if dragging { 0.0 } else { clamp(self.kin.inspect_intent, 0.0, 1.0) },
            if dragging { 0.0 } else { clamp(self.cuddle_intent.max(self.kin.catch_blend), 0.0, 1.0) },
            self.antenna_aim_angle(),
        );
        let default_angle_profile = [0.90, 0.56, 0.22, -0.13, -0.46];
        let mut angle_profile: Vec<f64> = match cfg.get("rest_angles") {
            Some(Value::Array(_)) => match float_list(cfg.get("rest_angles"), segments, false) {
                Some(v) => v,
                None => default_angle_profile.iter().take(segments).copied().collect(),
            },
            _ => {
                let v: Vec<f64> = default_angle_profile.iter().take(segments).copied().collect();
                if v.len() == segments { v } else { default_angle_profile.iter().take(segments).copied().collect() }
            }
        };
        let mut lift_profile: Vec<f64> = [1.00, 0.55, 0.20, 0.06, 0.0].iter().take(segments).copied().collect();
        let mut phase_offsets: Vec<f64> = [0.00, 0.12, 0.24, 0.34, 0.44].iter().take(segments).copied().collect();
        angle_profile.truncate(segments);
        while angle_profile.len() < segments {
            let last = *angle_profile.last().unwrap();
            angle_profile.push(last * 0.72);
        }
        while lift_profile.len() < segments {
            let last = *lift_profile.last().unwrap();
            lift_profile.push((last * 0.55).max(0.0));
        }
        while phase_offsets.len() < segments {
            let last = *phase_offsets.last().unwrap();
            phase_offsets.push(last + 0.14);
        }
        let joint_profile = |name: &str, fallback: [f64; 5]| -> Vec<f64> {
            let raw = cfg.get(name);
            let mut values = match raw {
                Some(Value::Array(_)) => match float_list(raw, segments, true) {
                    Some(v) => v,
                    None => fallback.iter().take(segments).copied().collect(),
                },
                _ => match float_list(Some(&Value::from(fallback.to_vec())), segments, true) {
                    Some(v) => v,
                    None => fallback.iter().take(segments).copied().collect(),
                },
            };
            while values.len() < segments {
                values.push(values.last().copied().unwrap_or(0.0));
            }
            values
        };
        let joint_steering = joint_profile("joint_steering", [0.24, 0.40, 0.34, 0.24, 0.14]);
        let mut joint_steering_limits = joint_profile("joint_steering_limits", [0.42, 0.66, 0.58, 0.44, 0.30]);
        let serial_follow = clamp(cfg.f64_or("serial_follow", 0.68), 0.0, 0.94);
        let mut joint_angle_limits = joint_profile("joint_angle_limits", [0.38, 0.60, 0.54, 0.40, 0.26]);
        let free_rom = joint_profile("free_range_of_motion", [0.48, 0.78, 0.68, 0.54, 0.38]);
        for (i, l) in joint_angle_limits.iter_mut().enumerate() {
            *l = l.max(free_rom[i]);
        }
        for (i, l) in joint_steering_limits.iter_mut().enumerate() {
            *l = l.max(free_rom[i] * 1.12);
        }

        let probing =
            if dragging { 0.0 } else { clamp(self.kin.feeler_pulse.max(self.kin.inspect_intent * 0.55), 0.0, 1.0) };
        let catch_blend = self.kin.catch_blend;
        let inspect_intent = self.kin.inspect_intent;
        let explicit_hand_target = !dragging && (catch_blend > 0.08 || inspect_intent > 0.40 || self.aim_intent > 0.55);
        let target_world = if explicit_hand_target {
            Some(if catch_blend > 0.08 { self.kin.catch_point } else { (self.focus_x, self.focus_y) })
        } else {
            None
        };
        let mut hand_activity = if dragging {
            0.0
        } else {
            clamp(
                (probing * 0.55).max(inspect_intent * 0.35).max(self.aim_intent * 0.25).max(catch_blend * 0.70),
                0.0,
                1.0,
            )
        };
        if explicit_hand_target {
            hand_activity = hand_activity.max(self.focus_strength * 0.25);
        }
        let proximal_rise = clamp(cfg.f64_or("proximal_rise", 0.12), 0.0, 0.28);
        let rest_forward = clamp(cfg.f64_or("hand_rest_forward", 1.02), 0.28, 1.45);
        let rest_lateral = clamp(cfg.f64_or("hand_rest_lateral", 0.52), 0.14, 1.10);
        let min_forward = clamp(cfg.f64_or("hand_min_forward", 0.34), 0.16, rest_forward);
        let max_forward = clamp(cfg.f64_or("hand_max_forward", 1.52), rest_forward, 2.20);
        let min_lateral = clamp(cfg.f64_or("hand_min_lateral", 0.20), 0.10, rest_lateral);
        let max_lateral = clamp(cfg.f64_or("hand_max_lateral", 0.86), rest_lateral, 1.30);
        let rub_frequency = clamp(cfg.f64_or("rub_frequency", 0.92), 0.45, 1.80);
        let rub_lateral_amount = clamp(cfg.f64_or("rub_lateral_amount", 0.18), 0.0, 0.34);
        let rub_forward_amount = clamp(cfg.f64_or("rub_forward_amount", 0.09), 0.0, 0.20);
        let rub_joint_angle = clamp(cfg.f64_or("rub_joint_angle", 0.42), 0.02, 0.72);
        let rub_lift_amount = clamp(cfg.f64_or("rub_lift_amount", 0.24), 0.0, 0.42);
        let knuckle_wave_delay = clamp(cfg.f64_or("knuckle_wave_delay", 0.18), 0.08, 0.36);
        let knuckle_lift_delay = clamp(cfg.f64_or("knuckle_lift_delay", 0.24), 0.10, 0.44);
        let appearance = Dict::of(&model).dict("appearance");
        let (base_forward, base_lateral) = (|| -> Option<(f64, f64)> {
            let ceph_scale = match appearance.get("cephalothorax_scale") {
                None => vec![0.70, 0.66],
                Some(Value::Array(items)) => items.iter().map(py_float).collect::<Option<Vec<f64>>>()?,
                Some(_) => return None,
            };
            let ceph_forward = match appearance.get("cephalothorax_offset_x") {
                None => 0.40,
                Some(v) => py_float(v)?,
            };
            let bf = match cfg.get("base_forward") {
                None => 0.34,
                Some(v) => py_float(v)?,
            };
            let bs = match cfg.get("base_side") {
                None => 0.26,
                Some(v) => py_float(v)?,
            };
            Some((ceph_forward + ceph_scale.first()? * bf, ceph_scale.get(1)? * bs))
        })()
        .unwrap_or((0.64, 0.18));

        let shared_phase = (self.antenna_phase[0] + self.antenna_phase[1]) * 0.5;
        let rub_phase = shared_phase * rub_frequency;
        let observing =
            matches!(self.state, State::Observe | State::Inspect | State::Aim | State::Catch | State::Alert);
        let attention_level = clamp(
            inspect_intent.max(self.aim_intent).max(catch_blend).max(if observing && self.focus_strength > 0.15 {
                0.82
            } else {
                0.0
            }),
            0.0,
            1.0,
        );
        let idle_rub_gate = 0.5 + 0.5 * (rub_phase * 0.52 - 1.15).sin();
        let mut gesture_level =
            clamp(attention_level.max(idle_rub_gate * if observing { 0.52 } else { 0.48 }), 0.0, 1.0);
        if explicit_hand_target {
            gesture_level = gesture_level.max(0.72 * hand_activity);
        }
        let joint_rub_profile = [0.32, 0.70, 1.00, 0.82, 0.56];
        let symmetric_rest = cfg.get("symmetric_rest").is_some_and(crate::pymath::py_truthy);
        let size = self.kin.size;

        for (side_index, side_sign) in [(0usize, -1.0f64), (1, 1.0)] {
            if self.antenna_segment_angles[side_index].len() != segments {
                self.antenna_segment_angles[side_index] = angle_profile.iter().map(|a| side_sign * a).collect();
            }
            if self.antenna_joint_lifts[side_index].len() != segments {
                self.antenna_joint_lifts[side_index] = lift_profile.iter().map(|l| proximal_rise * l).collect();
            }
            let side_phase = self.antenna_phase[side_index];
            let mut phase = side_phase;
            if symmetric_rest {
                phase = (self.antenna_phase[0] + self.antenna_phase[1]) * 0.5;
            }
            let mut desired_f = rest_forward;
            let mut desired_s = side_sign * rest_lateral;
            if let Some((wx, wy)) = target_world {
                let (tf, ts) = self.kin.world_to_body_local(wx, wy);
                let mut target_f = tf / 1.0f64.max(size) - base_forward;
                let mut target_s = ts / 1.0f64.max(size) - side_sign * base_lateral;
                target_f = clamp(target_f, min_forward, max_forward);
                target_s = side_sign * clamp(side_sign * target_s, min_lateral, max_lateral);
                desired_f += (target_f - desired_f) * hand_activity;
                desired_s += (target_s - desired_s) * hand_activity;
            } else if !dragging {
                desired_f += 0.006 * phase.sin();
                desired_s += side_sign * 0.004 * (phase * 0.91).cos();
            }
            if !dragging {
                let rub_wave = 0.5 + 0.5 * rub_phase.sin();
                desired_f += rub_phase.cos() * rub_forward_amount * gesture_level;
                desired_s = side_sign * (desired_s.abs() - rub_lateral_amount * gesture_level * rub_wave);
            }
            desired_f = clamp(desired_f, min_forward, max_forward);
            desired_s = side_sign * clamp(side_sign * desired_s, min_lateral, max_lateral);
            self.antenna_hand_targets[side_index] = (desired_f * size, desired_s * size);
            let hand_reach = hypot(desired_f, desired_s);
            let extension_target = clamp(
                (hand_reach - hypot(rest_forward, rest_lateral)).max(0.0) * 0.10
                    + hand_activity * 0.025
                    + phase.sin().max(0.0) * 0.006,
                0.0,
                0.10,
            );
            self.antenna_extension[side_index] +=
                (extension_target - self.antenna_extension[side_index]) * (1.0 - (-dt * 4.5).exp());
            let grip_target = if dragging {
                0.0
            } else {
                clamp(
                    catch_blend * 0.72 + inspect_intent * 0.24 + hand_activity * 0.12 + phase.sin().max(0.0) * 0.03,
                    0.0,
                    1.0,
                )
            };
            self.antenna_hand_grips[side_index] +=
                (grip_target - self.antenna_hand_grips[side_index]) * (1.0 - (-dt * 9.0).exp());

            let mut inherited_delta = 0.0;
            for segment_index in 0..segments {
                let si = segment_index as f64;
                let delayed_phase = phase - phase_offsets[segment_index];
                let joint_wave = delayed_phase.sin() * (0.018 + m.curiosity * 0.022);
                let profile_angle = side_sign * angle_profile[segment_index];
                let mut target_angle = profile_angle;
                target_angle += side_sign * (drive.curl * 0.045 + (drive.base_angle - 0.62) * 0.18);
                target_angle += side_sign * joint_wave;
                if target_world.is_some() {
                    let desired_angle = desired_s.atan2(0.05f64.max(desired_f));
                    let aim_delta = (desired_angle - profile_angle).sin().atan2((desired_angle - profile_angle).cos());
                    target_angle += clamp(
                        aim_delta * joint_steering[segment_index],
                        -joint_steering_limits[segment_index],
                        joint_steering_limits[segment_index],
                    );
                }
                let stroke = 0.5 + 0.5 * delayed_phase.sin();
                if probing > 0.01 {
                    target_angle += side_sign * probing * (if segment_index < 2 { 0.025 } else { -0.018 }) * stroke;
                }
                if hand_activity > 0.01 {
                    target_angle += side_sign * hand_activity * 0.010 * (delayed_phase * 0.85).sin();
                }
                if !dragging {
                    let joint_wave_scale = joint_rub_profile[segment_index.min(joint_rub_profile.len() - 1)];
                    let knuckle_phase = rub_phase - si * knuckle_wave_delay + side_index as f64 * 0.08;
                    target_angle +=
                        side_sign * knuckle_phase.sin() * rub_joint_angle * gesture_level * joint_wave_scale;
                    let side_wave = (side_phase * 0.82 + 0.65 + si * 0.04).sin();
                    target_angle += side_wave * rub_joint_angle * gesture_level * 0.24;
                    if attention_level > 0.01 {
                        let probe_fold =
                            (rub_phase * 0.62 - si * knuckle_wave_delay * 1.25 + side_index as f64 * 0.11).sin();
                        target_angle += side_sign
                            * attention_level
                            * rub_joint_angle
                            * (if segment_index < 2 { 0.18 } else { -0.11 })
                            * probe_fold;
                    }
                }
                let mut angle_delta = (target_angle - profile_angle).sin().atan2((target_angle - profile_angle).cos());
                if segment_index > 0 {
                    angle_delta += inherited_delta * serial_follow * 0.56;
                    angle_delta = angle_delta.sin().atan2(angle_delta.cos());
                }
                target_angle = profile_angle
                    + clamp(angle_delta, -joint_angle_limits[segment_index], joint_angle_limits[segment_index]);
                inherited_delta = inherited_delta * 0.34 + (target_angle - profile_angle) * 0.66;

                let mut lift_target = proximal_rise * lift_profile[segment_index];
                lift_target *= 0.90 + hand_activity * 0.34 + self.antenna_hand_grips[side_index] * 0.16;
                lift_target += proximal_rise * 0.08 * delayed_phase.sin().max(0.0);
                if !dragging {
                    let lift_phase = rub_phase - si * knuckle_lift_delay + side_index as f64 * 0.09;
                    let lift_scale = if segment_index < 2 {
                        1.0
                    } else if segment_index < 4 {
                        0.68
                    } else {
                        0.42
                    };
                    lift_target += proximal_rise * rub_lift_amount * gesture_level * lift_phase.sin() * lift_scale;
                    if attention_level > 0.01 {
                        lift_target += proximal_rise
                            * attention_level
                            * (if segment_index < 2 { 0.12 } else { 0.06 })
                            * (rub_phase * 0.62 - si * knuckle_lift_delay + side_index as f64 * 0.13).sin();
                    }
                }
                lift_target = clamp(lift_target, 0.0, 0.48);
                let angle_rate = 4.6f64.max(8.2 - si * 0.50 + hand_activity * 1.4);
                let lift_rate = 4.8f64.max(8.0 - si * 0.56 + hand_activity * 1.0);
                let angle_alpha = 1.0 - (-dt * angle_rate).exp();
                let lift_alpha = 1.0 - (-dt * lift_rate).exp();
                let a = &mut self.antenna_segment_angles[side_index][segment_index];
                *a += (target_angle - *a) * angle_alpha;
                let l = &mut self.antenna_joint_lifts[side_index][segment_index];
                *l += (lift_target - *l) * lift_alpha;
            }
        }
    }
}
