//! Port of `world/carcass.py`: what a beaten spider leaves behind (DC-47).
//!
//! A carcass lies still for a settle phase, then is consumed — faster while a
//! living spider stands over it. The killing blow's damage number rides over
//! from the spider; [`DamageNumber`] is the plain-data half of
//! `creature/damage_numbers.py` that a carcass needs (dc-creature may reuse
//! it for its own numbers).
//!
//! Drawing is out of scope. Painter state: `x, y, size, colors.body, legs`
//! (`(along, across, curl)` each), [`Carcass::spent`], `damage_numbers`,
//! [`Carcass::damage_top`] and [`Carcass::damage_color`].

use dc_core::math::clamp;

use crate::Xywh;
use crate::creature_view::WorldCreature;
use crate::pymath::hypot;

pub const CARCASS_LIFETIME: f64 = 16.0;
pub const EATEN_SPEED_UP: f64 = 3.2;
pub const CARCASS_SETTLE_SECONDS: f64 = 4.0;
pub const FEEDING_REACH: f64 = 1.9;
pub const CARCASS_FOOD_AMOUNT: f64 = 9.0;

// `creature/damage_numbers.py` constants the carcass uses.
pub const DAMAGE_NUMBER_SECONDS: f64 = 1.1;
pub const DAMAGE_MERGE_SECONDS: f64 = 0.15;
pub const DAMAGE_RISE_PX: f64 = 30.0;
pub const DAMAGE_NUMBER_MARGIN: f64 = 56.0;
pub const ALLY_DAMAGE_COLOR: (u8, u8, u8) = (255, 86, 64);
pub const ENEMY_DAMAGE_COLOR: (u8, u8, u8) = (110, 200, 255);

/// One floating `-N`: `[amount, age, nudge]` in Python.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageNumber {
    pub amount: f64,
    pub age: f64,
    pub nudge: f64,
}

/// `_note_damage`: add a hit, merging blows that land close together.
pub fn note_damage(numbers: &mut Vec<DamageNumber>, dealt: f64) {
    if dealt <= 0.0 {
        return;
    }
    if let Some(newest) = numbers.last_mut()
        && newest.age < DAMAGE_MERGE_SECONDS
    {
        newest.amount += dealt;
        return;
    }
    let nudge = 8.0 * (numbers.len() % 3) as f64;
    numbers.push(DamageNumber { amount: dealt, age: 0.0, nudge });
}

/// `_update_damage_numbers`.
pub fn update_damage_numbers(numbers: &mut Vec<DamageNumber>, dt: f64) {
    if numbers.is_empty() {
        return;
    }
    for n in numbers.iter_mut() {
        n.age += dt;
    }
    numbers.retain(|n| n.age < DAMAGE_NUMBER_SECONDS);
}

/// `DamageNumbersMixin.damage_text`.
pub fn damage_text(amount: f64) -> String {
    if amount >= 1.0 { format!("-{amount:.0}") } else { format!("-{amount:.1}") }
}

/// A dead spider's remains.
#[derive(Clone, Debug, PartialEq)]
pub struct Carcass {
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub team_id: String,
    /// `colors["body"]`, when the model had one.
    pub body_color: Option<(u8, u8, u8)>,
    pub t: f64,
    pub settle_t: f64,
    pub being_eaten: bool,
    pub damage_numbers: Vec<DamageNumber>,
    /// The side the spider was on, for the number colour.
    pub damage_team: Option<String>,
    /// `(along, across, curl)` per folded leg, fixed at death.
    pub legs: [(f64, f64, f64); 8],
}

impl Carcass {
    pub fn new(x: f64, y: f64, size: f64, team_id: &str, body_color: Option<(u8, u8, u8)>) -> Carcass {
        let size = 4.0f64.max(size);
        let mut legs = [(0.0, 0.0, 0.0); 8];
        for (index, leg) in legs.iter_mut().enumerate() {
            let side = if index % 2 == 1 { -1.0 } else { 1.0 };
            let along = (index / 2) as f64 - 1.5;
            *leg = (along * size * 0.26, side * size * 0.30, side * (0.7 + 0.25 * along.abs()));
        }
        Carcass {
            x,
            y,
            size,
            team_id: if team_id.is_empty() { "neutral".to_string() } else { team_id.to_string() },
            body_color,
            t: 0.0,
            settle_t: 0.0,
            being_eaten: false,
            damage_numbers: Vec::new(),
            damage_team: None,
            legs,
        }
    }

    /// Still lying intact, before anything has begun on it.
    pub fn settling(&self) -> bool {
        self.settle_t < CARCASS_SETTLE_SECONDS
    }

    /// How far through being eaten, 0..1.
    pub fn spent(&self) -> f64 {
        clamp(self.t / CARCASS_LIFETIME, 0.0, 1.0)
    }

    /// Advance; true once nothing is left. Feeding does not stack.
    pub fn update(&mut self, dt: f64, eaters: usize) -> bool {
        self.being_eaten = eaters > 0;
        let step = 0.0f64.max(dt);
        update_damage_numbers(&mut self.damage_numbers, step);
        if self.settling() && !self.being_eaten {
            self.settle_t += step;
            return false;
        }
        let rate = if self.being_eaten { EATEN_SPEED_UP } else { 1.0 };
        self.settle_t = CARCASS_SETTLE_SECONDS;
        self.t += step * rate;
        self.t >= CARCASS_LIFETIME
    }

    /// Where the numbers start.
    pub fn damage_top(&self) -> f64 {
        self.y - self.size * 0.6
    }

    /// Ice blue over a mission's enemy, red otherwise.
    pub fn damage_color(&self) -> (u8, u8, u8) {
        let team = self.damage_team.as_deref().filter(|t| !t.is_empty()).unwrap_or("");
        if team == "rivals" { ENEMY_DAMAGE_COLOR } else { ALLY_DAMAGE_COLOR }
    }

    pub fn footprint(&self) -> Xywh {
        let reach = self.size * 1.6;
        let (x0, mut y0, mut x1, y1) = (self.x - reach, self.y - reach, self.x + reach, self.y + reach);
        if !self.damage_numbers.is_empty() {
            let top = self.damage_top();
            y0 = y0.min(top - DAMAGE_RISE_PX - DAMAGE_NUMBER_MARGIN);
            x1 = x1.max(self.x + self.size * 0.8 + DAMAGE_NUMBER_MARGIN * 2.0);
        }
        (x0, y0, x1 - x0, y1 - y0)
    }
}

/// `curled_size`: the size a carcass takes from its spider.
pub fn curled_size(size: f64) -> f64 {
    4.0f64.max(size)
}

/// `carcass_for`: the remains of a spider that has just died. `body` is the
/// model's `colors.body` (or `abdomen`); `numbers` its damage numbers.
pub fn carcass_for<C: WorldCreature>(creature: &C, body: Option<(u8, u8, u8)>, numbers: &[DamageNumber]) -> Carcass {
    let mut c = Carcass::new(creature.x(), creature.y(), curled_size(creature.size()), creature.team_id(), body);
    c.damage_numbers = numbers.to_vec();
    c.damage_team = Some(creature.team_id().to_string());
    c
}

/// How many living spiders are close enough to be feeding.
pub fn eaters_near<C: WorldCreature>(carcass: &Carcass, creatures: &[C]) -> usize {
    let reach = carcass.size * FEEDING_REACH;
    creatures
        .iter()
        .filter(|c| !c.dead() && !c.dragging() && hypot(c.x() - carcass.x, c.y() - carcass.y) <= reach)
        .count()
}

/// Whether one creature is feeding on the carcass (`eaters_near(c, [one])`).
pub fn is_eating<C: WorldCreature>(carcass: &Carcass, creature: &C) -> bool {
    !creature.dead()
        && !creature.dragging()
        && hypot(creature.x() - carcass.x, creature.y() - carcass.y) <= carcass.size * FEEDING_REACH
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f64 = 1.0 / 60.0;

    #[test]
    fn being_eaten_is_faster_and_a_body_settles_first() {
        let mut eaten = Carcass::new(100.0, 100.0, 18.0, "pack_a", None);
        let mut ignored = Carcass::new(300.0, 300.0, 18.0, "pack_a", None);
        for _ in 0..((CARCASS_SETTLE_SECONDS + 1.0) * 60.0) as usize {
            eaten.update(DT, 1);
            ignored.update(DT, 0);
        }
        assert!(eaten.spent() > ignored.spent() * 2.0);
        assert!(ignored.spent() > 0.0);

        let mut body = Carcass::new(100.0, 100.0, 18.0, "pack_a", None);
        for _ in 0..((CARCASS_SETTLE_SECONDS - 0.5) * 60.0) as usize {
            assert!(!body.update(DT, 0));
        }
        assert!(body.settling());
        assert_eq!(body.spent(), 0.0);
        for _ in 0..60 {
            body.update(DT, 0);
        }
        assert!(!body.settling());
        assert!(body.spent() > 0.0);
    }

    #[test]
    fn scavengers_start_the_clock_and_crowds_do_not_stack() {
        let mut one = Carcass::new(0.0, 0.0, 18.0, "pack_a", None);
        let mut many = Carcass::new(0.0, 0.0, 18.0, "pack_a", None);
        for _ in 0..30 {
            one.update(DT, 1);
            many.update(DT, 6);
        }
        assert!(one.spent() > 0.0);
        assert_eq!(many.spent(), one.spent());
    }

    #[test]
    fn numbers_merge_age_and_widen_the_footprint() {
        let mut n = Vec::new();
        note_damage(&mut n, 3.0);
        note_damage(&mut n, 2.0);
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].amount, 5.0);
        let mut c = Carcass::new(100.0, 100.0, 20.0, "", None);
        let plain = c.footprint();
        c.damage_numbers = n;
        assert!(c.footprint().1 < plain.1);
        for _ in 0..70 {
            c.update(DT, 0);
        }
        assert!(c.damage_numbers.is_empty());
        assert_eq!(damage_text(5.0), "-5");
        assert_eq!(damage_text(0.25), "-0.2");
        assert_eq!(c.team_id, "neutral");
    }
}
