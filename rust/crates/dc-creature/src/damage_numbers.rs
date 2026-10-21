//! Port of the logic of `creature/damage_numbers.py`: recording, ageing and
//! bounding the floating damage numbers. Drawing them is `dc-paint`'s; the
//! painter reads [`Creature::damage_numbers`], [`Creature::damage_top`],
//! [`Creature::damage_color`] and [`damage_text`].

use crate::creature::Creature;
use serde::{Deserialize, Serialize};

pub const DAMAGE_NUMBER_SECONDS: f64 = 1.1;
pub const DAMAGE_MERGE_SECONDS: f64 = 0.15;
pub const DAMAGE_RISE_PX: f64 = 30.0;
pub const DAMAGE_NUMBER_MARGIN: f64 = 56.0;
pub const ALLY_DAMAGE_COLOR: [u8; 3] = [255, 86, 64];
pub const ENEMY_DAMAGE_COLOR: [u8; 3] = [110, 200, 255];

/// `[amount, age, sideways nudge]`
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DamageNumber {
    pub amount: f64,
    pub age: f64,
    pub nudge: f64,
}

/// `damage_text(amount)`: `-12` for a whole hit, `-0.4` for a scratch.
pub fn damage_text(amount: f64) -> String {
    if amount >= 1.0 {
        // f"{amount:.0f}" rounds half to even, like `round`.
        format!("-{}", py_format_0f(amount))
    } else {
        format!("-{:.1}", amount)
    }
}

/// `f"{x:.0f}"` (correctly rounded, ties to even).
fn py_format_0f(x: f64) -> String {
    format!("{:.0}", x)
}

impl Creature {
    /// `_note_damage(dealt)`
    pub fn note_damage(&mut self, dealt: f64) {
        if dealt <= 0.0 {
            return;
        }
        if let Some(newest) = self.damage_numbers.last_mut()
            && newest.age < DAMAGE_MERGE_SECONDS
        {
            newest.amount += dealt;
            return;
        }
        let nudge = 8.0 * (self.damage_numbers.len() % 3) as f64;
        self.damage_numbers.push(DamageNumber { amount: dealt, age: 0.0, nudge });
    }

    /// `_update_damage_numbers(dt)`
    pub fn update_damage_numbers(&mut self, dt: f64) {
        if self.damage_numbers.is_empty() {
            return;
        }
        for n in &mut self.damage_numbers {
            n.age += dt;
        }
        self.damage_numbers.retain(|n| n.age < DAMAGE_NUMBER_SECONDS);
    }

    /// `_damage_top()`: where the numbers start.
    pub fn damage_top(&self) -> f64 {
        let mut top = self.kin.y;
        for leg in &self.kin.legs {
            top = top.min(leg.foot_y);
        }
        top - self.jump_z
    }

    /// `damage_color()`: ice blue over a mission's enemy, red otherwise.
    pub fn damage_color(&self) -> [u8; 3] {
        let team = match &self.damage_team {
            Some(t) if !t.is_empty() => t.as_str(),
            _ => self.progression.team_id.as_str(),
        };
        if team == "rivals" { ENEMY_DAMAGE_COLOR } else { ALLY_DAMAGE_COLOR }
    }

    /// `_damage_number_bounds(bbox)`
    pub fn damage_number_bounds(&self, bbox: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
        if self.damage_numbers.is_empty() {
            return bbox;
        }
        let (min_x, min_y, max_x, max_y) = bbox;
        let top = self.damage_top();
        (
            min_x,
            min_y.min(top - DAMAGE_RISE_PX - DAMAGE_NUMBER_MARGIN),
            max_x.max(self.kin.x + self.kin.size * 0.8 + DAMAGE_NUMBER_MARGIN * 2.0),
            max_y,
        )
    }
}
