//! Port of `world/aimed_silk.py`: visible straight silk from the player
//! controller, with a swept hit test so a fast shot cannot tunnel through
//! a target between frames.
//!
//! Python subclasses `WebShotProjectile` and is handed two callables for the
//! live creature and fly lists; here [`AimedSilk::update`] takes both slices
//! and the fly world's stream (a hit fly is wrapped with
//! `pin_with_web_shot`, which rolls its splat pegs). It lives in
//! `FlyWorld::projectiles` as [`crate::flies::Projectile::Aimed`].
//!
//! If the shooter is no longer among `creatures`, no spider counts as its
//! foe (Python would still have asked the removed object); flies can still
//! be hit.

use dc_core::PyRandom;
use dc_core::math::clamp;

use crate::Point;
use crate::creature_view::{CreatureId, Relation, WorldCreature, find_index};
use crate::flies::{Fly, ShooterPose, WEB_SHOT_SPEED, pin_fly_with_shot, push_trail};
use crate::pymath::hypot;

/// A straight, swept silk line.
#[derive(Clone, Debug)]
pub struct AimedSilk {
    pub shooter: CreatureId,
    pub kind: String,
    pub homing: f64,
    pub origin: Point,
    pub pos: Point,
    pub vel: Point,
    pub travelled: f64,
    pub max_travel: f64,
    /// Last up to 9 positions, oldest first.
    pub trail: Vec<Point>,
    pub done: bool,
    pub hit: bool,
}

impl AimedSilk {
    pub fn new(shooter: ShooterPose, angle: f64, reach: f64) -> AimedSilk {
        let pos = (shooter.x + angle.cos() * shooter.size * 0.7, shooter.y + angle.sin() * shooter.size * 0.7);
        AimedSilk {
            shooter: shooter.id,
            kind: "trap".to_string(),
            homing: 0.0,
            origin: pos,
            pos,
            vel: (angle.cos() * WEB_SHOT_SPEED, angle.sin() * WEB_SHOT_SPEED),
            travelled: 0.0,
            max_travel: reach,
            trail: vec![pos],
            done: false,
            hit: false,
        }
    }

    pub fn update<C: WorldCreature>(&mut self, dt: f64, creatures: &mut [C], flies: &mut [Fly], rng: &mut PyRandom) {
        if self.done {
            return;
        }
        let (ox, oy) = self.pos;
        let step = (WEB_SHOT_SPEED * dt).min(self.max_travel - self.travelled);
        let dx = self.vel.0 / WEB_SHOT_SPEED * step;
        let dy = self.vel.1 / WEB_SHOT_SPEED * step;
        self.pos = (ox + dx, oy + dy);
        self.travelled += step;
        push_trail(&mut self.trail, self.pos, 9);

        let denom = 0.001f64.max(dx * dx + dy * dy);
        let swept = |tx: f64, ty: f64, size: f64| -> Option<f64> {
            let t = clamp(((tx - ox) * dx + (ty - oy) * dy) / denom, 0.0, 1.0);
            let radius = 13.0f64.max(size * 0.7);
            (hypot(tx - ox - dx * t, ty - oy - dy * t) <= radius).then_some(t)
        };

        // (t, is_spider, index): the earliest along the step wins, spiders
        // before flies on a tie, list order within each.
        let mut best: Option<(f64, bool, usize)> = None;
        let shooter = find_index(creatures, self.shooter);
        for (i, target) in creatures.iter().enumerate() {
            let Some(s) = shooter else {
                break;
            };
            if i == s
                || target.dead()
                || target.webbed()
                || creatures[s].relation_to(target) != Relation::Foe
                || target.airborne()
            {
                continue;
            }
            if let Some(t) = swept(target.x(), target.y(), target.size())
                && best.is_none_or(|b| t < b.0)
            {
                best = Some((t, true, i));
            }
        }
        for (i, fly) in flies.iter().enumerate() {
            if !fly.alive() || fly.eaten() || fly.trapped() {
                continue;
            }
            if let Some(t) = swept(fly.x, fly.y, fly.size)
                && best.is_none_or(|b| t < b.0)
            {
                best = Some((t, false, i));
            }
        }
        if let Some((_t, is_spider, i)) = best {
            if is_spider {
                creatures[i].web_pinned("trap", self.shooter);
            } else {
                pin_fly_with_shot(&mut flies[i], self.origin, "trap", self.shooter, rng);
            }
            self.done = true;
            self.hit = true;
        } else if self.travelled >= self.max_travel {
            self.done = true;
        }
    }
}
