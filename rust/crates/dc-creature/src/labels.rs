//! Port of the label logic and `bounding_rect` of `creature/core.py`.
//!
//! What a label *says* and *whether* it shows is decided here; drawing it is
//! `dc-paint`'s (it reads [`Creature::label_spec`]). `bounding_rect` needed
//! `QFontMetrics` of the label font, so it takes a [`LabelMetrics`]; a caller
//! without fonts can pass [`ApproxLabelMetrics`]. `wants_mouse` does not
//! depend on it: the manager's `creature_at` uses [`Creature::hit_test`].

use crate::creature::{Creature, LabelStyle};
use crate::pymath::clamp;
use dc_state::progression::{MAX_LEVEL, xp_to_next_level};
use dc_state::teams::team_color;

/// `Creature.LABEL_BAR_WIDTH`: every spider's bars are this wide (DC-87).
pub const LABEL_BAR_WIDTH: f64 = 44.0;
/// `Creature.XP_BAR_COLOR`
pub const XP_BAR_COLOR: [u8; 3] = [242, 212, 128];
/// `Creature.STAMINA_BAR_COLOR`
pub const STAMINA_BAR_COLOR: [u8; 3] = [250, 204, 20];
/// The label family (`QFont("Segoe UI")`).
pub const LABEL_FONT_FAMILY: &str = "Segoe UI";
/// The quiet (mission) label's point size.
pub const QUIET_LABEL_POINT_SIZE: f64 = 7.5;

/// The label font's metrics, as `QFontMetrics` gave them.
pub trait LabelMetrics {
    /// `fm.horizontalAdvance(text)` for the label font at `point_size`
    /// (DemiBold for the full label, regular for the quiet one).
    fn horizontal_advance(&self, text: &str, point_size: f64, demibold: bool) -> f64;
    /// `fm.height()`
    fn height(&self, point_size: f64, demibold: bool) -> f64;
}

/// A font-less estimate for headless use (tests, a manager without text
/// shaping): 0.56 em per character, 1.35 em line height at 96 dpi.
#[derive(Clone, Copy, Debug, Default)]
pub struct ApproxLabelMetrics;

impl LabelMetrics for ApproxLabelMetrics {
    fn horizontal_advance(&self, text: &str, point_size: f64, demibold: bool) -> f64 {
        let px = point_size * 96.0 / 72.0;
        let em = if demibold { 0.58 } else { 0.55 };
        (text.chars().count() as f64 * px * em).round()
    }

    fn height(&self, point_size: f64, _demibold: bool) -> f64 {
        (point_size * 96.0 / 72.0 * 1.35).ceil()
    }
}

/// Everything the painter needs to draw a creature's label.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelSpec {
    pub style: LabelStyle,
    /// The name line is drawn (`name_visible`); otherwise only pinned bars.
    pub name_visible: bool,
    /// `_label_text()` (`display_name + "  ·  Lv N"` when pinned).
    pub text: String,
    /// `display_name`, the quiet label's text.
    pub display_name: String,
    /// `_level_suffix()`
    pub level_suffix: String,
    /// The label font's point size (`10 + (level - 1) * 0.1`, DemiBold).
    pub point_size: f64,
    /// `_label_top()`: just above the body and legs.
    pub top: f64,
    /// The border colour, `[r, g, b, a]` (`_label_border_color`).
    pub border_rgba: [u8; 4],
    pub health_pinned: bool,
    pub stamina_pinned: bool,
    pub xp_pinned: bool,
    pub health_fraction: f64,
    pub stamina_fraction: f64,
    pub xp_fraction: f64,
    /// `_health_bar_height()`, `_xp_bar_height()`, `_label_bars_height()`.
    pub health_bar_height: f64,
    pub xp_bar_height: f64,
    pub bars_height: f64,
}

impl Creature {
    /// `display_name`
    pub fn display_name(&self) -> String {
        if !self.name.is_empty() {
            return self.name.clone();
        }
        self.model.get("display_name").map(crate::pymath::py_str).unwrap_or_else(|| "Spider".to_string())
    }

    /// `level_label_pinned`
    pub fn level_label_pinned(&self) -> bool {
        self.progression.pin_level || self.force_show_level
    }

    pub fn set_level_label_pinned(&mut self, enabled: bool) {
        self.progression.pin_level = enabled;
    }

    /// `health_label_pinned`
    pub fn health_label_pinned(&self) -> bool {
        self.progression.pin_health || self.force_show_health
    }

    pub fn set_health_label_pinned(&mut self, enabled: bool) {
        self.progression.pin_health = enabled;
    }

    /// `xp_label_pinned`
    pub fn xp_label_pinned(&self) -> bool {
        self.force_show_xp
    }

    /// `stamina_label_pinned`
    pub fn stamina_label_pinned(&self) -> bool {
        self.force_show_stamina
    }

    /// `_label_font()` point size.
    pub fn label_point_size(&self) -> f64 {
        10.0 + (self.level() - 1) as f64 * 0.1
    }

    /// `label_visible(always_show)`: is the label drawn at all?
    pub fn label_visible(&self, always_show: bool) -> bool {
        let pinned = self.level_label_pinned()
            || self.health_label_pinned()
            || self.xp_label_pinned()
            || self.stamina_label_pinned();
        if always_show || pinned {
            return true;
        }
        !self.name.is_empty() && self.hovered
    }

    /// `name_visible(always_show)`: is the name drawn, not just the bars?
    pub fn name_visible(&self, always_show: bool) -> bool {
        if always_show || self.level_label_pinned() || self.xp_label_pinned() || self.label_style == LabelStyle::Hero {
            return true;
        }
        !self.name.is_empty() && self.hovered
    }

    /// `_label_border_color()`: the team colour, white for no team.
    pub fn label_border_color(&self) -> [u8; 4] {
        let team_id = if self.progression.team_id.is_empty() { "neutral" } else { self.progression.team_id.as_str() };
        if matches!(team_id.trim().to_lowercase().as_str(), "" | "neutral") {
            return [255, 255, 255, 215];
        }
        let c = team_color(team_id, self.team_profiles.as_deref());
        [c.r, c.g, c.b, 235]
    }

    /// `_label_text()`
    pub fn label_text(&self) -> String {
        self.display_name() + &self.level_suffix()
    }

    /// `_level_suffix()`
    pub fn level_suffix(&self) -> String {
        if self.level_label_pinned() || self.xp_label_pinned() {
            format!("  \u{00b7}  Lv {}", self.level())
        } else {
            String::new()
        }
    }

    /// `_health_bar_height()`
    pub fn health_bar_height(&self) -> f64 {
        4.0f64.max(7.0f64.min(self.kin.size * 0.20))
    }

    /// `_xp_bar_height()`
    pub fn xp_bar_height(&self) -> f64 {
        3.0f64.max(self.health_bar_height() - 1.0)
    }

    /// `_label_bars_height()`
    pub fn label_bars_height(&self) -> f64 {
        let mut height = 0.0;
        if self.health_label_pinned() {
            height += self.health_bar_height() + 3.0;
        }
        if self.stamina_label_pinned() {
            height += self.xp_bar_height() + 2.0;
        }
        height
    }

    /// `health_bar_color(fraction)`: green, amber, red.
    pub fn health_bar_color(fraction: f64) -> [u8; 4] {
        let fraction = clamp(fraction, 0.0, 1.0);
        if fraction > 0.6 {
            [104, 194, 108, 235]
        } else if fraction > 0.3 {
            [226, 176, 74, 235]
        } else {
            [214, 84, 76, 235]
        }
    }

    /// `_label_top()`
    pub fn label_top(&self) -> f64 {
        let mut top_extent = self.kin.y;
        for leg in &self.kin.legs {
            if leg.foot_y < top_extent {
                top_extent = leg.foot_y;
            }
        }
        top_extent - (self.kin.size * 0.6 + self.jump_z)
    }

    /// Everything `_draw_name_label` needs, or `None` when no label shows.
    pub fn label_spec(&self, always_show_names: bool) -> Option<LabelSpec> {
        if !self.label_visible(always_show_names) {
            return None;
        }
        Some(LabelSpec {
            style: self.label_style,
            name_visible: self.name_visible(always_show_names),
            text: self.label_text(),
            display_name: self.display_name(),
            level_suffix: self.level_suffix(),
            point_size: self.label_point_size(),
            top: self.label_top(),
            border_rgba: self.label_border_color(),
            health_pinned: self.health_label_pinned(),
            stamina_pinned: self.stamina_label_pinned(),
            xp_pinned: self.xp_label_pinned(),
            health_fraction: self.health_fraction(),
            stamina_fraction: self.stamina_fraction(),
            xp_fraction: self.xp_fraction(),
            health_bar_height: self.health_bar_height(),
            xp_bar_height: self.xp_bar_height(),
            bars_height: self.label_bars_height(),
        })
    }

    /// `bounding_rect(always_show_names)`: a padded (x0, y0, x1, y1) covering
    /// everything this spider draws. Also stores it as `bbox`.
    pub fn bounding_rect(&mut self, always_show_names: bool, metrics: &dyn LabelMetrics) -> (f64, f64, f64, f64) {
        let (mut min_x, mut max_x) = (self.kin.x, self.kin.x);
        let (mut min_y, mut max_y) = (self.kin.y, self.kin.y);
        for i in 0..self.kin.legs.len() {
            let (fx, fy) = (self.kin.legs[i].foot_x, self.kin.legs[i].foot_y);
            let (ax, ay) = self.kin.leg_attach(i);
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
        let pad = self.kin.size * 0.85 + 12.0;
        min_x -= pad;
        max_x += pad;
        min_y -= pad;
        max_y += pad;
        min_y -= self.jump_z;
        if self.state == crate::state::State::Roll || self.kin.roll_spin.abs() > 1e-4 {
            let radius = self.kin.size * 2.2 + pad;
            min_x = min_x.min(self.kin.x - radius);
            max_x = max_x.max(self.kin.x + radius);
            min_y = min_y.min(self.kin.y - radius);
            max_y = max_y.max(self.kin.y + radius);
        }
        if self.label_visible(always_show_names) {
            let size = self.label_point_size();
            let text = self.label_text();
            let half_w = (metrics.horizontal_advance(&text, size, true) * 0.5 + 12.0).max(LABEL_BAR_WIDTH * 0.5 + 2.0);
            let label_h = metrics.height(size, true) + 12.0 + self.label_bars_height();
            min_x = min_x.min(self.kin.x - half_w);
            max_x = max_x.max(self.kin.x + half_w);
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
        let bbox = self.damage_number_bounds((min_x, min_y, max_x, max_y));
        self.bbox = Some(bbox);
        bbox
    }

    /// `xp_fraction` at the level cap is full (re-stated for the painter).
    pub fn at_level_cap(&self) -> bool {
        self.level() >= MAX_LEVEL
    }

    /// `xp_to_next_level(self.level)`
    pub fn xp_to_next(&self) -> i64 {
        xp_to_next_level(self.level())
    }
}
