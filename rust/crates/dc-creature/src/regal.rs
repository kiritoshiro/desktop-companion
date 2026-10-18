//! Port of the logic of `creature/regal.py`: the regal jumper's bounded
//! attention gestures. `_render_regal` is `dc-paint`'s; it reads
//! [`Creature::regal`] (or a default [`RegalExpression`] while
//! [`Creature::regal_blocked`]) and [`Creature::regal_face_forward`].

use crate::creature::Creature;
use crate::progression::Relation;
use crate::pymath::{clamp, hypot, py_pow};
use crate::state::State;
use crate::world::Population;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegalGesture {
    #[default]
    Rest,
    Greet,
    Dance,
    Question,
    Peek,
    Reach,
    Groom,
}

impl RegalGesture {
    pub fn as_str(self) -> &'static str {
        match self {
            RegalGesture::Rest => "rest",
            RegalGesture::Greet => "greet",
            RegalGesture::Dance => "dance",
            RegalGesture::Question => "question",
            RegalGesture::Peek => "peek",
            RegalGesture::Reach => "reach",
            RegalGesture::Groom => "groom",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegalTarget {
    #[default]
    None,
    Mouse,
    Spider,
}

/// `RegalExpression`
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RegalExpression {
    pub clock: f64,
    pub cooldown: f64,
    pub age: f64,
    pub duration: f64,
    pub gesture: RegalGesture,
    pub target: RegalTarget,
    pub yaw: f64,
    pub pitch: f64,
    pub tilt: f64,
    pub left: f64,
    pub right: f64,
    pub bounce: f64,
    pub palps: f64,
}

impl Default for RegalExpression {
    fn default() -> Self {
        RegalExpression {
            clock: 0.0,
            cooldown: 1.2,
            age: 0.0,
            duration: 0.0,
            gesture: RegalGesture::Rest,
            target: RegalTarget::None,
            yaw: 0.0,
            pitch: 0.0,
            tilt: 0.0,
            left: 0.0,
            right: 0.0,
            bounce: 0.0,
            palps: 0.0,
        }
    }
}

impl Creature {
    fn species_profile(&self) -> Option<&str> {
        self.appearance().get("species_profile").and_then(|v| v.as_str())
    }

    /// `_is_regal()`
    pub fn is_regal(&self) -> bool {
        matches!(self.species_profile(), Some("regal_fluff" | "regal_scout"))
    }

    /// `_regal_face_forward()`
    pub fn regal_face_forward(&self) -> bool {
        self.species_profile() == Some("regal_scout")
    }

    /// `_regal_blocked()`
    pub fn regal_blocked(&self) -> bool {
        self.dead
            || self.dragging
            || self.webbed()
            || self.airborne
            || self.player_control
            || self.hunting_prey
            || self.foe.is_some()
            || self.desktop_fully_hidden
            || !matches!(self.state, State::Idle | State::Wander | State::Approach | State::Play | State::Observe)
            || self.kin.current_speed > 45.0
            || self.startled_timer > 0.0
    }

    /// `_update_regal(dt, mx, my)`
    pub fn update_regal(&mut self, dt: f64, mx: f64, my: f64, population: &Population) {
        if !self.is_regal() {
            return;
        }
        if self.regal.is_none() {
            self.regal = Some(RegalExpression { cooldown: 1.2 + self.index as f64 * 0.37, ..Default::default() });
        }
        let blocked = self.regal_blocked();
        let (x, y, heading) = (self.kin.x, self.kin.y, self.kin.heading);
        let mut r = self.regal.unwrap();
        r.clock += dt;
        r.cooldown = (r.cooldown - dt).max(0.0);
        let (mut tx, mut ty) = (mx, my);
        let mut target = if hypot(mx - x, my - y) < 230.0 { RegalTarget::Mouse } else { RegalTarget::None };
        if self.allow_social {
            let mut best: Option<(f64, f64, f64)> = None;
            for id in &self.neighbors {
                if *id == self.id {
                    continue;
                }
                let Some(other) = population.get(*id) else { continue };
                if other.dead || other.dragging || other.desktop_fully_hidden {
                    continue;
                }
                if self.relation_to(other) == Relation::Foe {
                    continue;
                }
                let d = hypot(other.x - x, other.y - y);
                if d >= 180.0 {
                    continue;
                }
                if best.is_none_or(|(bd, _, _)| d < bd) {
                    best = Some((d, other.x, other.y));
                }
            }
            if let Some((_, ox, oy)) = best {
                tx = ox;
                ty = oy;
                target = RegalTarget::Spider;
            }
        }
        if blocked {
            r.gesture = RegalGesture::Rest;
            r.target = RegalTarget::None;
            r.duration = 0.0;
            r.age = 0.0;
            r.cooldown = r.cooldown.max(1.0);
            r.left = 0.0;
            r.right = 0.0;
            r.bounce = 0.0;
            r.palps = 0.0;
            r.tilt = 0.0;
            r.yaw *= (-dt * 7.0).exp();
            r.pitch *= (-dt * 7.0).exp();
            self.regal = Some(r);
            return;
        }
        let (dx, dy) = (tx - x, ty - y);
        let side = -heading.sin() * dx + heading.cos() * dy;
        let forward = heading.cos() * dx + heading.sin() * dy;
        let yaw = if target != RegalTarget::None { clamp(side / 160.0, -0.65, 0.65) } else { (r.clock * 0.55).sin() * 0.22 };
        let pitch = if target != RegalTarget::None { clamp(forward / 220.0, -0.2, 0.65) } else { 0.12 };
        let blend = 1.0 - (-dt * 5.0).exp();
        r.yaw += (yaw - r.yaw) * blend;
        r.pitch += (pitch - r.pitch) * blend;
        r.target = target;
        if r.duration == 0.0 && r.cooldown == 0.0 {
            use RegalGesture::*;
            let choices: [RegalGesture; 3] = match target {
                RegalTarget::Spider => [Greet, Dance, Question],
                RegalTarget::Mouse => [Question, Peek, Reach],
                RegalTarget::None => [Groom, Peek, Dance],
            };
            r.gesture = *self.rng.choice(&choices);
            r.duration = if r.gesture == Dance { 3.4 } else { 2.5 };
            r.age = 0.0;
        }
        r.left = 0.0;
        r.right = 0.0;
        r.bounce = 0.0;
        r.tilt = 0.0;
        r.palps = 0.0;
        if r.duration != 0.0 {
            r.age += dt;
            let u = (r.age / r.duration).min(1.0);
            let envelope = py_pow((PI * u).sin(), 2.0);
            let beat = (r.age * 8.0).sin();
            match r.gesture {
                RegalGesture::Question | RegalGesture::Reach => {
                    r.left = envelope * if r.yaw <= 0.0 { 0.85 } else { 0.1 };
                    r.right = envelope * if r.yaw > 0.0 { 0.85 } else { 0.1 };
                    r.tilt = envelope * if r.yaw <= 0.0 { -0.14 } else { 0.14 };
                    r.palps = envelope * 0.25;
                }
                RegalGesture::Greet | RegalGesture::Dance => {
                    r.left = envelope * (0.76 + 0.20 * beat);
                    r.right = envelope * (0.76 - 0.20 * beat);
                    r.tilt = envelope * (r.age * 4.0).sin() * 0.12;
                    r.bounce = envelope * beat.abs() * 0.09;
                    r.palps = envelope * beat * 0.5;
                }
                RegalGesture::Peek => {
                    r.tilt = envelope * (r.age * 2.7).sin() * 0.19;
                    r.bounce = envelope * 0.045;
                    r.palps = envelope * 0.22;
                }
                _ => {
                    r.palps = envelope * (r.age * 10.0).sin();
                }
            }
            if u >= 1.0 {
                r.gesture = RegalGesture::Rest;
                r.duration = 0.0;
                r.cooldown = self.rng.uniform(3.5, 7.5);
            }
        }
        self.regal = Some(r);
    }
}
