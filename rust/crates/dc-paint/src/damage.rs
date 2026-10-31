//! Port of the drawing half of `creature/damage_numbers.py`
//! (`_damage_top`, `damage_color`, `damage_text`, `_draw_damage_numbers`,
//! `_damage_number_bounds`) and `Creature.render_damage_numbers`.
//!
//! The numbers are shared by spiders and carcasses, so the painter takes the
//! few values it reads rather than a whole creature.

use crate::draw::{CreatureDraw, DamageNumber};
use dc_core::Color;
use dc_core::math::clamp;
use dc_render::{Brush, Canvas, Cap, Join, Path, Pen, PenStyle};

pub const DAMAGE_NUMBER_SECONDS: f64 = 1.1;
pub const DAMAGE_RISE_PX: f64 = 30.0;
pub const DAMAGE_NUMBER_MARGIN: f64 = 56.0;
pub const ALLY_DAMAGE_COLOR: (u8, u8, u8) = (255, 86, 64);
pub const ENEMY_DAMAGE_COLOR: (u8, u8, u8) = (110, 200, 255);

/// `damage_color()`: ice blue over a mission's enemy, red otherwise.
pub fn damage_color(team: Option<&str>) -> (u8, u8, u8) {
    if team == Some("rivals") { ENEMY_DAMAGE_COLOR } else { ALLY_DAMAGE_COLOR }
}

/// `damage_text(amount)`: `f"-{amount:.0f}"` or `f"-{amount:.1f}"`.
pub fn damage_text(amount: f64) -> String {
    // Python's format rounds half to even, as Rust's `{:.N}` does.
    if amount >= 1.0 { format!("-{amount:.0}") } else { format!("-{amount:.1}") }
}

/// `_draw_damage_numbers(painter)` for any owner: `top` is its
/// `_damage_top()`, `x`/`size` its position and size.
pub fn draw_damage_numbers(p: &mut Canvas, numbers: &[DamageNumber], top: f64, x: f64, size: f64, team: Option<&str>) {
    if numbers.is_empty() {
        return;
    }
    let mut font = p.font();
    font.set_bold(true);
    font.set_pixel_size(clamp(size * 0.55, 12.0, 20.0) as i32);
    let (r, g, b) = damage_color(team);
    p.save();
    for &(amount, age, nudge) in numbers {
        let t = clamp(age / DAMAGE_NUMBER_SECONDS, 0.0, 1.0);
        let alpha = (255.0 * (1.0 - t * t)) as i64;
        let alpha = alpha.clamp(0, 255) as u8;
        let nx = x + size * 0.8 + nudge;
        let ny = top - DAMAGE_RISE_PX * t;
        let mut path = Path::new();
        path.add_text(nx, ny, &font, &damage_text(amount));
        p.set_pen(Pen::full(Color::rgba(20, 10, 10, alpha), 3.0, PenStyle::Solid, Cap::Round, Join::Round));
        p.set_brush(Brush::None);
        p.draw_path(&path);
        p.set_pen(Pen::NONE);
        p.set_brush(Color::rgba(r, g, b, alpha));
        p.draw_path(&path);
    }
    p.restore();
}

/// `_damage_number_bounds(bbox)`: grown to cover the numbers.
pub fn damage_number_bounds(
    numbers: &[DamageNumber],
    top: f64,
    x: f64,
    size: f64,
    bbox: (f64, f64, f64, f64),
) -> (f64, f64, f64, f64) {
    if numbers.is_empty() {
        return bbox;
    }
    let (min_x, min_y, max_x, max_y) = bbox;
    (
        min_x,
        min_y.min(top - DAMAGE_RISE_PX - DAMAGE_NUMBER_MARGIN),
        max_x.max(x + size * 0.8 + DAMAGE_NUMBER_MARGIN * 2.0),
        max_y,
    )
}

impl CreatureDraw {
    /// `_damage_top()`: the top of the drawn spider.
    pub fn damage_top(&self) -> f64 {
        self.legs.iter().map(|l| l.leg_foot_y).fold(self.y, f64::min) - self.jump_z
    }

    /// `damage_team or progression.team_id`.
    pub fn damage_team_id(&self) -> Option<&str> {
        match self.damage_team.as_deref() {
            Some(t) if !t.is_empty() => Some(t),
            _ => Some(self.label.team_id.as_str()),
        }
    }

    /// `_draw_damage_numbers(painter)`.
    pub fn draw_damage_numbers(&self, p: &mut Canvas) {
        draw_damage_numbers(p, &self.damage_numbers, self.damage_top(), self.x, self.size, self.damage_team_id());
    }

    /// `render_damage_numbers(painter)`: the numbers alone, faded with the
    /// spider's camouflage, as the manager draws them last.
    pub fn render_damage_numbers(&self, p: &mut Canvas) {
        if self.damage_numbers.is_empty() {
            return;
        }
        let opacity = self.camouflage_opacity();
        p.save();
        let o = p.opacity();
        p.set_opacity(o * opacity);
        self.draw_damage_numbers(p);
        p.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_text_matches_python_format() {
        assert_eq!(damage_text(12.4), "-12");
        assert_eq!(damage_text(2.5), "-2");
        assert_eq!(damage_text(0.45), "-0.5");
        assert_eq!(damage_text(0.25), "-0.2");
        assert_eq!(damage_color(Some("rivals")), ENEMY_DAMAGE_COLOR);
        assert_eq!(damage_color(Some("neutral")), ALLY_DAMAGE_COLOR);
    }
}
