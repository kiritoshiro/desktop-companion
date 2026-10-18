//! Port of the drawing half of `world/carcass.py`: `Carcass.spent`,
//! `Carcass.footprint`, `Carcass._damage_top` and `Carcass.draw`.

use crate::damage::{damage_number_bounds, draw_damage_numbers};
use crate::draw::DamageNumber;
use dc_core::{Color, Rect};
use dc_render::{Canvas, Cap, Pen};
use serde::{Deserialize, Serialize};

pub const CARCASS_LIFETIME: f64 = 16.0;

/// `Carcass`: what `draw` reads.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct CarcassDraw {
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub t: f64,
    /// `colors.get("body")`.
    pub body: Option<(u8, u8, u8)>,
    /// `legs`: `(along, across, curl)`, fixed at death. Empty means "derive
    /// from `size`" exactly as `Carcass.__init__` does.
    pub legs: Vec<(f64, f64, f64)>,
    pub damage_numbers: Vec<DamageNumber>,
    pub damage_team: Option<String>,
    pub team_id: String,
}

impl Default for CarcassDraw {
    fn default() -> Self {
        CarcassDraw {
            x: 0.0,
            y: 0.0,
            size: 18.0,
            t: 0.0,
            body: None,
            legs: Vec::new(),
            damage_numbers: Vec::new(),
            damage_team: None,
            team_id: "neutral".into(),
        }
    }
}

impl CarcassDraw {
    /// The leg layout of `Carcass.__init__`.
    pub fn default_legs(size: f64) -> Vec<(f64, f64, f64)> {
        (0..8)
            .map(|index| {
                let side = if index % 2 == 1 { -1.0 } else { 1.0 };
                let along = (index / 2) as f64 - 1.5;
                (along * size * 0.26, side * size * 0.30, side * (0.7 + 0.25 * along.abs()))
            })
            .collect()
    }

    /// `spent`.
    pub fn spent(&self) -> f64 {
        (self.t / CARCASS_LIFETIME).clamp(0.0, 1.0)
    }

    /// `_damage_top()`.
    pub fn damage_top(&self) -> f64 {
        self.y - self.size * 0.6
    }

    fn team(&self) -> Option<&str> {
        // `damage_team or progression.team_id`: a carcass has no progression.
        self.damage_team.as_deref().filter(|t| !t.is_empty())
    }

    /// `footprint()` as `(x, y, w, h)`.
    pub fn footprint(&self) -> (f64, f64, f64, f64) {
        let reach = self.size * 1.6;
        let (x0, y0, x1, y1) = damage_number_bounds(
            &self.damage_numbers,
            self.damage_top(),
            self.x,
            self.size,
            (self.x - reach, self.y - reach, self.x + reach, self.y + reach),
        );
        (x0, y0, x1 - x0, y1 - y0)
    }

    /// `_draw_damage_numbers(painter)`.
    pub fn draw_damage_numbers(&self, p: &mut Canvas) {
        draw_damage_numbers(p, &self.damage_numbers, self.damage_top(), self.x, self.size, self.team());
    }
}

/// `Carcass.draw(painter, numbers)`.
pub fn draw_carcass(p: &mut Canvas, c: &CarcassDraw, numbers: bool) {
    let left = 1.0 - c.spent();
    if left <= 0.0 {
        return;
    }
    let alpha = (225.0 * left) as i64;
    let body = c.body.unwrap_or((132, 120, 136));
    let ch = |v: u8, add: i64| ((v as f64 * 0.78) as i64 + add).clamp(0, 255) as u8;
    let shade = Color::rgba(ch(body.0, 26), ch(body.1, 24), ch(body.2, 28), alpha.clamp(0, 255) as u8);
    let scale = c.size * (0.55 + 0.45 * left);
    let derived;
    let legs = if c.legs.is_empty() {
        derived = CarcassDraw::default_legs(c.size);
        &derived
    } else {
        &c.legs
    };
    p.save();
    p.set_pen(crate::procedural::pen_cap(shade, (scale * 0.10).max(1.0), Cap::Round));
    for &(along, across, curl) in legs {
        let knee = (c.x + along * left, c.y + across * left);
        let toe = (knee.0 + along * 0.5 * left, knee.1 + across * curl * 0.45 * left);
        p.draw_line(knee.0, knee.1, toe.0, toe.1);
    }
    p.set_pen(Pen::NONE);
    p.set_brush(shade);
    p.draw_ellipse(Rect::new(c.x - scale * 0.36, c.y - scale * 0.28, scale * 0.72, scale * 0.56));
    p.restore();
    if numbers {
        c.draw_damage_numbers(p);
    }
}
