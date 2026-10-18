//! Port of the name-label drawing in `creature/core.py`: `_label_font`,
//! `label_visible`, `name_visible`, `_label_border_color`, `_label_text`,
//! `_level_suffix`, the bar heights, `health_bar_color`, `_draw_label_bar`,
//! `_draw_quiet_label`, `_draw_name_label`, `_label_top` and
//! `_draw_label_bars`, plus the label part of `bounding_rect`.

use crate::draw::CreatureDraw;
use dc_core::math::clamp;
use dc_core::{Color, Rect};
use dc_render::{Brush, Canvas, Font, FontMetrics, Pen, TextFlags, Weight};
use dc_state::progression::{MAX_LEVEL, xp_to_next_level};

/// `Creature.LABEL_BAR_WIDTH`.
pub const LABEL_BAR_WIDTH: f64 = 44.0;
/// `Creature.XP_BAR_COLOR`.
pub const XP_BAR_COLOR: (u8, u8, u8) = (242, 212, 128);
/// `Creature.STAMINA_BAR_COLOR`.
pub const STAMINA_BAR_COLOR: (u8, u8, u8) = (250, 204, 20);

impl CreatureDraw {
    /// `health_fraction()`.
    pub fn health_fraction(&self) -> f64 {
        clamp(self.label.hp / self.label.max_hp.max(1.0), 0.0, 1.0)
    }

    /// `stamina_fraction()`.
    pub fn stamina_fraction(&self) -> f64 {
        clamp(self.label.energy / self.label.max_energy.max(1.0), 0.0, 1.0)
    }

    /// `xp_fraction()`: full at the level cap.
    pub fn xp_fraction(&self) -> f64 {
        if self.label.level >= MAX_LEVEL {
            return 1.0;
        }
        clamp(self.label.xp / (xp_to_next_level(self.label.level) as f64).max(1.0), 0.0, 1.0)
    }

    /// `_label_font()`.
    pub fn label_font(&self) -> Font {
        let mut font = Font::new("Segoe UI", 9.0);
        font.set_point_size_f(10.0 + (self.label.level - 1) as f64 * 0.1);
        font.set_weight(Weight::DEMI_BOLD);
        font
    }

    /// `label_visible(always_show)`.
    pub fn label_visible(&self, always_show: bool) -> bool {
        let l = &self.label;
        let pinned = l.level_label_pinned || l.health_label_pinned || l.xp_label_pinned || l.stamina_label_pinned;
        if always_show || pinned {
            return true;
        }
        !l.name.is_empty() && l.hovered
    }

    /// `name_visible(always_show)`.
    pub fn name_visible(&self, always_show: bool) -> bool {
        let l = &self.label;
        if always_show || l.level_label_pinned || l.xp_label_pinned || l.label_style == "hero" {
            return true;
        }
        !l.name.is_empty() && l.hovered
    }

    /// `_label_border_color()`: the team colour, white for no team.
    pub fn label_border_color(&self) -> Color {
        let team = self.label.team_id.trim().to_lowercase();
        if team.is_empty() || team == "neutral" {
            return Color::rgba(255, 255, 255, 215);
        }
        let (r, g, b) = self.label.team_color;
        Color::rgba(r, g, b, 235)
    }

    /// `_level_suffix()`.
    pub fn level_suffix(&self) -> String {
        if self.label.level_label_pinned || self.label.xp_label_pinned {
            format!("  \u{00b7}  Lv {}", self.label.level)
        } else {
            String::new()
        }
    }

    /// `_label_text()`.
    pub fn label_text(&self) -> String {
        format!("{}{}", self.label.display_name, self.level_suffix())
    }

    /// `_health_bar_height()`.
    pub fn health_bar_height(&self) -> f64 {
        4.0f64.max(7.0f64.min(self.size * 0.20))
    }

    /// `_xp_bar_height()`.
    pub fn xp_bar_height(&self) -> f64 {
        3.0f64.max(self.health_bar_height() - 1.0)
    }

    /// `_label_bars_height()`.
    pub fn label_bars_height(&self) -> f64 {
        let mut height = 0.0;
        if self.label.health_label_pinned {
            height += self.health_bar_height() + 3.0;
        }
        if self.label.stamina_label_pinned {
            height += self.xp_bar_height() + 2.0;
        }
        height
    }

    /// `_label_top()`.
    pub fn label_top(&self) -> f64 {
        let mut top_extent = self.y;
        for leg in &self.legs {
            if leg.leg_foot_y < top_extent {
                top_extent = leg.leg_foot_y;
            }
        }
        top_extent - (self.size * 0.6 + self.jump_z)
    }

    /// `bounding_rect(always_show_names)`: a padded `(x0, y0, x1, y1)`
    /// covering everything this spider draws.
    pub fn bounding_rect(&self, always_show_names: bool) -> (f64, f64, f64, f64) {
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (self.x, self.x, self.y, self.y);
        for leg in &self.legs {
            let (fx, fy) = (leg.leg_foot_x, leg.leg_foot_y);
            let (ax, ay) = (leg.attach_x, leg.attach_y);
            if fx < min_x {
                min_x = fx;
            } else if fx > max_x {
                max_x = fx;
            }
            if fy < min_y {
                min_y = fy;
            } else if fy > max_y {
                max_y = fy;
            }
            if ax < min_x {
                min_x = ax;
            } else if ax > max_x {
                max_x = ax;
            }
            if ay < min_y {
                min_y = ay;
            } else if ay > max_y {
                max_y = ay;
            }
        }
        let pad = self.size * 0.85 + 12.0;
        min_x -= pad;
        max_x += pad;
        min_y -= pad;
        max_y += pad;
        min_y -= self.jump_z;
        if self.state == "Roll" || self.roll_spin.abs() > 1e-4 {
            let radius = self.size * 2.2 + pad;
            min_x = min_x.min(self.x - radius);
            max_x = max_x.max(self.x + radius);
            min_y = min_y.min(self.y - radius);
            max_y = max_y.max(self.y + radius);
        }
        if self.label_visible(always_show_names) {
            let fm = FontMetrics::new(&self.label_font());
            let text = self.label_text();
            let half_w = (fm.horizontal_advance(&text) as f64 * 0.5 + 12.0).max(LABEL_BAR_WIDTH * 0.5 + 2.0);
            let label_h = fm.height() as f64 + 12.0 + self.label_bars_height();
            min_x = min_x.min(self.x - half_w);
            max_x = max_x.max(self.x + half_w);
            min_y -= label_h + 6.0;
        }
        if !self.web_anchors.is_empty() {
            let reach = 4.0;
            for &(ax, ay) in &self.web_anchors {
                min_x = min_x.min(ax - reach);
                max_x = max_x.max(ax + reach);
                min_y = min_y.min(ay - reach);
                max_y = max_y.max(ay + reach);
            }
        }
        crate::damage::damage_number_bounds(
            &self.damage_numbers,
            self.damage_top(),
            self.x,
            self.size,
            (min_x, min_y, max_x, max_y),
        )
    }
}

/// `health_bar_color(fraction)`.
pub fn health_bar_color(fraction: f64) -> Color {
    let fraction = clamp(fraction, 0.0, 1.0);
    if fraction > 0.6 {
        Color::rgba(104, 194, 108, 235)
    } else if fraction > 0.3 {
        Color::rgba(226, 176, 74, 235)
    } else {
        Color::rgba(214, 84, 76, 235)
    }
}

/// `_draw_label_bar(painter, left, top, width, height, fraction, fill)`.
pub fn draw_label_bar(p: &mut Canvas, left: f64, top: f64, width: f64, height: f64, fraction: f64, fill: Color) {
    p.set_pen(Pen::NONE);
    p.set_brush(Color::rgba(18, 18, 22, 215));
    p.draw_rounded_rect(Rect::new(left, top, width, height), 2.0, 2.0);
    if fraction > 0.0 {
        p.set_brush(fill);
        p.draw_rounded_rect(
            Rect::new(left + 1.0, top + 1.0, ((width - 2.0) * fraction).max(1.0), height - 2.0),
            1.5,
            1.5,
        );
    }
    p.set_brush(Brush::None);
    p.set_pen(Pen::new(Color::rgba(255, 255, 255, 70), 1.0));
    p.draw_rounded_rect(Rect::new(left, top, width, height), 2.0, 2.0);
}

/// `_draw_health_bar(painter, left, top, width)`.
fn draw_health_bar(p: &mut Canvas, d: &CreatureDraw, left: f64, top: f64, width: f64) {
    let fraction = d.health_fraction();
    draw_label_bar(p, left, top, width, d.health_bar_height(), fraction, health_bar_color(fraction));
}

/// `_draw_label_bars(painter, bar_y)`.
pub fn draw_label_bars(p: &mut Canvas, d: &CreatureDraw, mut bar_y: f64) {
    let bar_w = LABEL_BAR_WIDTH;
    let bar_x = d.x - bar_w * 0.5;
    if d.label.health_label_pinned {
        draw_health_bar(p, d, bar_x, bar_y, bar_w);
        bar_y += d.health_bar_height() + 2.0;
    }
    if d.label.stamina_label_pinned {
        let (r, g, b) = STAMINA_BAR_COLOR;
        draw_label_bar(p, bar_x, bar_y, bar_w, d.xp_bar_height(), d.stamina_fraction(), Color::rgba(r, g, b, 235));
    }
}

/// `_draw_quiet_label(painter)`.
pub fn draw_quiet_label(p: &mut Canvas, d: &CreatureDraw) {
    let mut font = Font::new("Segoe UI", 9.0);
    font.set_point_size_f(7.5);
    p.set_font(font);
    let fm = FontMetrics::new(&font);
    let text = d.label.display_name.as_str();
    let tw = fm.horizontal_advance(text) as f64;
    let top_extent = d.legs.iter().map(|l| l.leg_foot_y).fold(d.y, f64::min) - d.size * 0.45 - d.jump_z;
    let bars = d.label_bars_height() * 0.6;
    let base_y = top_extent - 3.0 - bars;
    let x = d.x - (tw + 8.0) * 0.5;
    let dot = d.label_border_color();
    p.set_pen(Pen::NONE);
    p.set_brush(Color::rgba(dot.r, dot.g, dot.b, 200));
    p.draw_ellipse(Rect::new(x, base_y - fm.ascent() as f64 * 0.62, 5.0, 5.0));
    p.set_pen(Color::rgba(0, 0, 0, 110));
    p.draw_text(x + 8.6, base_y + 0.6, text);
    p.set_pen(Color::rgba(240, 240, 236, 165));
    p.draw_text(x + 8.0, base_y, text);
    let width = LABEL_BAR_WIDTH * 0.6;
    let bar_x = d.x - width * 0.5;
    let bar_y = base_y + 3.0;
    if d.label.health_label_pinned {
        let fraction = d.health_fraction();
        draw_label_bar(p, bar_x, bar_y, width, 3.0, fraction, health_bar_color(fraction));
    }
}

/// `_draw_name_label(painter, always_show_names)`.
pub fn draw_name_label(p: &mut Canvas, d: &CreatureDraw, always_show_names: bool) {
    if !d.label_visible(always_show_names) {
        return;
    }
    if d.label.label_style == "quiet" {
        draw_quiet_label(p, d);
        return;
    }
    if !d.name_visible(always_show_names) {
        draw_label_bars(p, d, d.label_top() - d.label_bars_height() - 1.0);
        return;
    }
    let font = d.label_font();
    p.set_font(font);
    let fm = FontMetrics::new(&font);
    let text = d.label_text();
    let tw = fm.horizontal_advance(&text) as f64;
    let th = fm.height() as f64;
    let pad_x = 8.0;
    let pad_y = 4.0;
    let box_w = tw + pad_x * 2.0;
    let box_h = th + pad_y * 2.0;
    let top_extent = d.label_top();
    let box_x = d.x - box_w * 0.5;
    let box_y = top_extent - box_h - 4.0 - d.label_bars_height();
    let hero = d.label.label_style == "hero";
    let gold = Color::rgba(246, 207, 106, 255);

    p.set_pen(Pen::NONE);
    p.set_brush(Color::rgba(18, 18, 22, 205));
    p.draw_rounded_rect(Rect::new(box_x, box_y, box_w, box_h), 6.0, 6.0);
    p.set_brush(Brush::None);
    if hero {
        p.set_pen(Pen::new(gold, 2.4));
    } else {
        p.set_pen(Pen::new(d.label_border_color(), 2.0));
    }
    p.draw_rounded_rect(Rect::new(box_x, box_y, box_w, box_h), 6.0, 6.0);
    if hero {
        let (tx, ty) = (d.x, box_y - 3.0);
        p.set_pen(Pen::NONE);
        p.set_brush(gold);
        p.draw_polygon(&[(tx - 6.0, ty - 7.0), (tx + 6.0, ty - 7.0), (tx, ty)]);
    }
    p.set_pen(Color::rgba(245, 247, 250, 255));
    p.draw_text_in(Rect::new(box_x, box_y, box_w, box_h), TextFlags::ALIGN_CENTER, &text);
    if d.label.xp_label_pinned {
        let suffix = d.level_suffix();
        let suffix = suffix.trim_start_matches([' ', '\u{00b7}']);
        let level_w = fm.horizontal_advance(suffix) as f64;
        let level_x = box_x + pad_x + tw - level_w;
        let line_y = box_y + box_h - 3.5;
        p.set_pen(Pen::NONE);
        p.set_brush(Color::rgba(0, 0, 0, 150));
        p.draw_rounded_rect(Rect::new(level_x, line_y, level_w, 2.5), 1.2, 1.2);
        let fill = level_w * d.xp_fraction();
        if fill > 0.5 {
            let (r, g, b) = XP_BAR_COLOR;
            p.set_brush(Color::rgba(r, g, b, 255));
            p.draw_rounded_rect(Rect::new(level_x, line_y, fill, 2.5), 1.2, 1.2);
        }
        p.set_brush(Brush::None);
    }
    draw_label_bars(p, d, box_y + box_h + 3.0);
}

