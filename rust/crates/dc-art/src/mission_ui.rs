//! Port of app/mission_ui.py (mission overlays), the HUD half of
//! app/adventure_ui.py (`hud_rect`, `_board`, `_well`, `short_binding`,
//! `draw_hud`) and the engine's own small overlays (`_draw_released_banner`,
//! the take-control hint and the aim markers in `paintEvent`).
//!
//! Everything paints from plain input structs, so the engine fills them from
//! whatever owns the state (the mission, the player controllers, the Reclaim
//! surface) and nothing here reaches into the simulation:
//!
//! * [`HudWindow`] -- the overlay's size, the main monitor and the dragged
//!   HUD position (`_home_area`, `hud_rect`).
//! * [`HudPlayer`] -- one controlled spider and its controller (`draw_hud`).
//! * [`MissionView`] -- what `draw_buildings` and `draw_mission_hud` read
//!   from the mission (sites, actors, loot, acid, the screen layout, the
//!   objective, the end state, the Reclaim desktop's health).
//!
//! Colours, sizes, fonts and texts are the Python's.

use std::f64::consts::TAU;
use std::sync::OnceLock;

use dc_core::{Color, IRect, Rect};
use dc_render::{
    Brush, Canvas, ClipOp, ElideMode, Font, Image, LinearGradient, Path, Pen, PenStyle, RadialGradient, RenderHint,
    TextFlags, Weight,
};

use crate::common::{alpha, round_cap_pen, round_pen};
use crate::mission_art::building_art;
use crate::qt::{darker, hex, lighter};

// -- wood_theme -------------------------------------------------------------------

pub const WALNUT: &str = "#3f2616";
pub const BRASS: &str = "#d6a448";
pub const CREAM: &str = "#f6e2b8";
pub const HEALTH: &str = "#b8472f";
pub const STAMINA: &str = "#f5c518";
pub const UI_FONT: &str = "Segoe UI";

/// `stat_text.TIER_COLORS`.
pub fn tier_colour(tier: &str) -> &'static str {
    match tier {
        "common" => "#cfc8b8",
        "uncommon" => "#6fc24a",
        "rare" => "#4f9ae8",
        "epic" => "#b066f0",
        "legendary" => "#f5a431",
        _ => "#cfc8b8",
    }
}

/// `wood_theme.texture("wood_dark_tile.png")`, loaded once (None when the
/// asset is missing).
pub fn dark_wood_texture() -> Option<&'static Image> {
    static T: OnceLock<Option<Image>> = OnceLock::new();
    T.get_or_init(|| {
        let path = dc_content::discovery::find_data_file(&["assets", "ui", "wood_dark_tile.png"], None);
        Image::load_png(&path).ok().filter(|i| !i.is_null())
    })
    .as_ref()
}

/// `fillPath(path, QBrush(texture))`: the texture tiled from the painter's
/// origin, inside `path`.
pub fn fill_path_tiled(p: &mut Canvas, path: &Path, texture: &Image) {
    let b = path.bounding_rect();
    let (tw, th) = texture.logical_size();
    if tw <= 0.0 || th <= 0.0 || b.is_empty() {
        return;
    }
    p.save();
    p.set_clip_path_op(path, ClipOp::Intersect);
    let mut y = (b.y / th).floor() * th;
    while y < b.bottom() {
        let mut x = (b.x / tw).floor() * tw;
        while x < b.right() {
            p.draw_image_at(x, y, texture);
            x += tw;
        }
        y += th;
    }
    p.restore();
}

// -- QRect helpers -------------------------------------------------------------------

/// `QRect::right()` (inclusive).
pub fn qright(r: IRect) -> i32 {
    r.x + r.w - 1
}

/// `QRect::bottom()` (inclusive).
pub fn qbottom(r: IRect) -> i32 {
    r.y + r.h - 1
}

/// `QRect::center().x()`.
pub fn qcenter_x(r: IRect) -> i32 {
    ((r.x as i64 + qright(r) as i64) / 2) as i32
}

/// `QRect::intersected`.
pub fn qintersected(a: IRect, b: IRect) -> IRect {
    let l = a.x.max(b.x);
    let t = a.y.max(b.y);
    let r = a.right().min(b.right());
    let bo = a.bottom().min(b.bottom());
    if r <= l || bo <= t { IRect::new(0, 0, 0, 0) } else { IRect::new(l, t, r - l, bo - t) }
}

/// `QRect::adjusted`.
pub fn qadjusted(r: IRect, dx1: i32, dy1: i32, dx2: i32, dy2: i32) -> IRect {
    IRect::new(r.x + dx1, r.y + dy1, r.w + dx2 - dx1, r.h + dy2 - dy1)
}

/// `QRectF(rect)`.
pub fn rf(r: IRect) -> Rect {
    r.to_rect()
}

/// `QRectF::toAlignedRect`.
pub fn aligned(r: Rect) -> IRect {
    let a = crate::qt::to_aligned_rect(r);
    IRect::new(a.x as i32, a.y as i32, a.w as i32, a.h as i32)
}

// -- the overlay window -------------------------------------------------------------

/// What the HUD needs of the overlay window.
#[derive(Clone, Debug, Default)]
pub struct HudWindow {
    pub width: i32,
    pub height: i32,
    /// The main monitor's area in overlay pixels (`primary_rect_local`), when known.
    pub primary: Option<IRect>,
    /// Where the player dragged the HUD (`_adventure_hud_position`).
    pub hud_position: Option<(i32, i32)>,
    /// Players in the mission (`len(mission.players)`); 0 outside a mission.
    pub players: usize,
}

impl HudWindow {
    pub fn new(width: i32, height: i32) -> Self {
        HudWindow { width, height, ..Default::default() }
    }

    fn whole(&self) -> IRect {
        IRect::new(0, 0, self.width, self.height)
    }
}

/// `_home_area(window)`: the main monitor in overlay pixels, or the whole
/// window when that is unknown or too small.
pub fn home_area(window: &HudWindow) -> IRect {
    let whole = window.whole();
    let Some(primary) = window.primary else { return whole };
    let area = qintersected(primary, whole);
    if area.w >= 200 && area.h >= 200 { area } else { whole }
}

pub const HUD_WIDTH: i32 = 430;
pub const HUD_HEIGHT: i32 = 210;

/// `hud_rect(window, player_index)`.
pub fn hud_rect(window: &HudWindow, player_index: usize) -> IRect {
    if window.players > 1 {
        let area = home_area(window);
        let width = HUD_WIDTH.min(200.max((area.w - 32).div_euclid(2)));
        return IRect::new(
            area.x + 10 + player_index as i32 * (width + 12),
            qbottom(area) - HUD_HEIGHT - 14,
            width,
            HUD_HEIGHT,
        );
    }
    let width = HUD_WIDTH.min(1.max(window.width - 16));
    let height = HUD_HEIGHT.min(1.max(window.height - 16));
    match window.hud_position {
        None => {
            let area = home_area(window);
            IRect::new(
                8.max(area.x + (area.w - width).div_euclid(2)),
                8.max(qbottom(area) - height - 14),
                width,
                height,
            )
        }
        Some((x, y)) => {
            IRect::new(8.max(x.min(window.width - width - 8)), 8.max(y.min(window.height - height - 8)), width, height)
        }
    }
}

/// `_board(painter, rect, radius)`: walnut with a raised, lit rim and a shadow under it.
pub fn board(p: &mut Canvas, rect: IRect, radius: f64) {
    let outer = rf(rect);
    let mut path = Path::new();
    path.add_rounded_rect(outer, radius, radius);
    p.set_pen(Pen::NONE);
    for i in (1..=4).rev() {
        p.set_brush(Color::rgba(10, 4, 0, 30));
        p.draw_path(&path.translated(i as f64 * 0.6, i as f64 * 1.0));
    }
    match dark_wood_texture() {
        Some(texture) => fill_path_tiled(p, &path, texture),
        None => {
            p.set_brush(hex(WALNUT));
            p.draw_path(&path);
        }
    }
    p.set_pen(Pen::NONE);
    p.set_brush(Color::rgba(20, 10, 3, 40));
    p.draw_path(&path);
    p.set_brush(Brush::None);
    p.set_pen(Pen::new(Color::rgba(210, 160, 100, 120), 1.4));
    p.draw_rounded_rect(outer.adjusted(1.5, 1.5, -1.5, -1.5), radius - 1.0, radius - 1.0);
    p.set_pen(Pen::new(Color::rgba(18, 8, 2, 230), 2.0));
    p.draw_rounded_rect(outer, radius, radius);
    p.set_pen(Pen::new(Color::rgba(18, 8, 2, 150), 1.2));
    p.draw_rounded_rect(outer.adjusted(5.0, 5.0, -5.0, -5.0), radius - 4.0, radius - 4.0);
}

/// `_well(painter, rect, radius, lit)`: a recess cut into the board.
pub fn well(p: &mut Canvas, rect: IRect, radius: f64, lit: bool) {
    let r = rf(rect);
    p.set_pen(Pen::NONE);
    p.set_brush(Color::rgba(14, 6, 1, if lit { 150 } else { 185 }));
    p.draw_rounded_rect(r, radius, radius);
    p.set_pen(Pen::new(Color::rgba(8, 3, 0, 200), 1.6));
    p.draw_line(r.x + radius, r.y + 0.8, r.right() - radius, r.y + 0.8);
    p.set_pen(Pen::new(Color::rgba(220, 170, 110, 90), 1.2));
    p.draw_line(r.x + radius, r.bottom() + 0.6, r.right() - radius, r.bottom() + 0.6);
}

/// `short_binding(name)`: "Mouse Left" -> "L CLICK".
pub fn short_binding(name: &str) -> String {
    match name {
        "Mouse Left" => "L CLICK".into(),
        "Mouse Right" => "R CLICK".into(),
        "Mouse Middle" => "M CLICK".into(),
        "Mouse Back" => "M4".into(),
        "Mouse Forward" => "M5".into(),
        _ => name.to_uppercase(),
    }
}

// -- the player HUD (adventure_ui.draw_hud) ---------------------------------------------

/// One controlled spider and its controller, as `draw_hud` reads them.
#[derive(Clone, Debug, Default)]
pub struct HudPlayer {
    pub display_name: String,
    pub level: i64,
    pub dead: bool,
    pub hp: f64,
    pub max_hp: f64,
    pub energy: f64,
    pub max_energy: f64,
    pub attack_cooldown: f64,
    pub silk: f64,
    pub silk_capacity: f64,
    pub jump_cooldown: f64,
    pub web_cooldown: f64,
    /// `controller.JUMP_ENERGY` / `WEB_ENERGY`.
    pub jump_energy: f64,
    pub web_energy: f64,
    pub feedback: String,
    pub feedback_time: f64,
    /// The bindings of "jump", "shoot", "bite" and "skills" (raw names,
    /// e.g. "Mouse Left"); None when the controller has no controls.
    pub bindings: Option<[String; 4]>,
}

/// `spider._label_font()`.
pub fn label_font(level: i64) -> Font {
    let mut font = Font::new(UI_FONT, 9.0);
    font.set_point_size_f(10.0 + (level - 1) as f64 * 0.1);
    font.set_weight(Weight::DEMI_BOLD);
    font
}

/// `draw_hud(painter, window, controller, player_index)`.
pub fn draw_hud(p: &mut Canvas, window: &HudWindow, player: &HudPlayer, player_index: usize) {
    let rect = hud_rect(window, player_index);
    let brass = hex(BRASS);
    let cream = hex(CREAM);
    let dim = Color::rgb(150, 124, 92);
    p.save();
    p.set_clipping(false);
    p.set_render_hint(RenderHint::Antialiasing, true);
    board(p, rect, 14.0);

    p.set_pen(brass);
    let body_font = p.font();
    p.set_font(label_font(player.level));
    let mut title = format!("{}  \u{b7}  Level {}", player.display_name, player.level);
    if window.players > 1 {
        title = format!("P{} \u{b7} {title}", player_index + 1);
    }
    if player.dead {
        title += " \u{b7} FALLEN";
    }
    let title = p.font_metrics().elided_text(&title, ElideMode::Right, 1.max(rect.w - 64) as f64);
    let title_rect = IRect::new(rect.x + 16, rect.y + 5, rect.w - 64, 26);
    let flags = TextFlags::ALIGN_LEFT | TextFlags::ALIGN_VCENTER;
    p.set_pen(Color::rgba(10, 4, 0, 200));
    p.draw_text_in(rf(title_rect).translated(0.0, 1.0), flags, &title);
    p.set_pen(brass);
    p.draw_text_in(rf(title_rect), flags, &title);
    p.set_font(body_font);
    // The drag handle: three grooves.
    let right = qright(rect) as f64;
    for y in [13.0, 18.0, 23.0] {
        let top = rect.y as f64;
        p.set_pen(round_cap_pen(Color::rgba(8, 3, 0, 220), 2.0));
        p.draw_line(right - 31.0, top + y, right - 16.0, top + y);
        p.set_pen(round_cap_pen(Color::rgba(210, 160, 100, 110), 1.0));
        p.draw_line(right - 31.0, top + y + 1.5, right - 16.0, top + y + 1.5);
    }

    let bar = |p: &mut Canvas, y: i32, label: &str, value: f64, maximum: f64, color: Color| {
        let x = rect.x + 16;
        let width = rect.w - 32;
        p.set_pen(cream);
        p.draw_text(x as f64, y as f64, &format!("{label}  {value:.0}/{maximum:.0}"));
        let track = IRect::new(x, y + 4, width, 9);
        well(p, track, 4.0, true);
        let fraction = (value / maximum.max(1.0)).clamp(0.0, 1.0);
        if fraction > 0.0 {
            let fill = Rect::new(x as f64 + 1.5, y as f64 + 5.5, 2.0f64.max((width - 3) as f64 * fraction), 6.0);
            let grad = LinearGradient::new(fill.x, fill.y, fill.x, fill.bottom())
                .with_stop(0.0, lighter(color, 135))
                .with_stop(1.0, darker(color, 125));
            p.set_pen(Pen::NONE);
            p.set_brush(grad);
            p.draw_rounded_rect(fill, 3.0, 3.0);
        }
    };
    bar(p, rect.y + 44, "HEALTH", player.hp, player.max_hp, hex(HEALTH));
    bar(p, rect.y + 68, "STAMINA", player.energy, player.max_energy, hex(STAMINA));
    bar(p, rect.y + 92, "SILK", player.silk, player.silk_capacity, hex("#c6dfd6"));

    let key = |i: usize, default: &str| match &player.bindings {
        Some(b) => short_binding(&b[i]),
        None => default.to_string(),
    };
    let slots = [
        ("jump", key(0, "SPACE"), "Jump", player.jump_cooldown, player.energy >= player.jump_energy),
        (
            "web",
            key(1, "L CLICK"),
            "Web",
            player.web_cooldown,
            player.energy >= player.web_energy && player.silk >= 1.0,
        ),
        ("bite", key(2, "R CLICK"), "Bite", player.attack_cooldown, true),
        ("skills", key(3, "K"), "Gear", 0.0, true),
    ];
    let card_width = (rect.w - 38).div_euclid(4);
    for (index, (kind, key, label, cooldown, enough_energy)) in slots.iter().enumerate() {
        let card = IRect::new(rect.x + 12 + index as i32 * (card_width + 5), rect.y + 108, card_width, 40);
        let ready = *cooldown <= 0.0 && *enough_energy;
        well(p, card, 7.0, ready);
        if ready {
            p.set_pen(Pen::new(Color::rgba(214, 164, 72, 150), 1.2));
            p.set_brush(Brush::None);
            p.draw_rounded_rect(rf(card).adjusted(0.5, 0.5, -0.5, -0.5), 7.0, 7.0);
        }
        let color = if ready { brass } else { dim };
        p.set_pen(Pen::new(color, 2.0));
        p.set_brush(Brush::None);
        let (cx, cy) = ((card.x + 17) as f64, (card.y + 19) as f64);
        match *kind {
            "jump" => {
                p.draw_line(cx - 8.0, cy + 7.0, cx, cy - 6.0);
                p.draw_line(cx, cy - 6.0, cx + 8.0, cy + 7.0);
                p.draw_line(cx - 6.0, cy + 7.0, cx + 6.0, cy + 7.0);
            }
            "web" => {
                p.draw_ellipse(Rect::new(cx - 8.0, cy - 8.0, 16.0, 16.0));
                for (x1, y1, x2, y2) in
                    [(-10.0, 0.0, 10.0, 0.0), (0.0, -10.0, 0.0, 10.0), (-7.0, -7.0, 7.0, 7.0), (-7.0, 7.0, 7.0, -7.0)]
                {
                    p.draw_line(cx + x1, cy + y1, cx + x2, cy + y2);
                }
            }
            "bite" => {
                p.draw_line(cx - 9.0, cy - 7.0, cx - 4.0, cy + 7.0);
                p.draw_line(cx - 4.0, cy + 7.0, cx, cy - 3.0);
                p.draw_line(cx + 9.0, cy - 7.0, cx + 4.0, cy + 7.0);
                p.draw_line(cx + 4.0, cy + 7.0, cx, cy - 3.0);
            }
            _ => {
                p.draw_line(cx, cy - 7.0, cx - 7.0, cy + 6.0);
                p.draw_line(cx, cy - 7.0, cx + 7.0, cy + 6.0);
                for (px, py) in [(cx, cy - 7.0), (cx - 7.0, cy + 6.0), (cx + 7.0, cy + 6.0)] {
                    p.draw_ellipse(Rect::new(px - 2.0, py - 2.0, 4.0, 4.0));
                }
            }
        }
        p.set_pen(if ready { cream } else { dim });
        p.draw_text((card.x + 33) as f64, (card.y + 16) as f64, key);
        p.set_pen(color);
        let text = if *cooldown > 0.0 { format!("{cooldown:.1}s") } else { label.to_string() };
        p.draw_text((card.x + 33) as f64, (card.y + 31) as f64, &text);
    }
    if player.feedback_time > 0.0 {
        p.set_pen(hex("#ffe2a5"));
        p.draw_text_in(rf(qadjusted(rect, 12, -25, -12, -rect.h)), TextFlags::ALIGN_CENTER, &player.feedback);
    }
    p.restore();
}

// -- the engine's small overlays -----------------------------------------------------------

/// `_adventure_hint_rect`: top middle of the main monitor.
pub fn adventure_hint_rect(window: &HudWindow) -> IRect {
    let whole = window.whole();
    let mut area = match window.primary {
        Some(pr) if pr.w > 0 && pr.h > 0 => qintersected(pr, whole),
        _ => whole,
    };
    if area.w <= 0 || area.h <= 0 {
        area = whole;
    }
    IRect::new(qcenter_x(area) - 220, area.y + 24, 440, 30)
}

/// "Click a spider to take control" (adventure mode without a player).
pub fn draw_adventure_hint(p: &mut Canvas, window: &HudWindow) {
    p.set_pen(hex("#ffffff"));
    p.draw_text_in(
        rf(adventure_hint_rect(window)),
        TextFlags::ALIGN_CENTER,
        "Click a spider to take control \u{b7} Esc for menu",
    );
}

/// A local co-op player's aim: `(x, y, heading, size, dead)`.
#[derive(Clone, Copy, Debug)]
pub struct PlayerAim {
    pub x: f64,
    pub y: f64,
    pub heading: f64,
    pub size: f64,
    pub dead: bool,
}

/// The P1 / P2 aim rings ahead of each co-op spider.
pub fn draw_player_aims(p: &mut Canvas, players: &[PlayerAim]) {
    for (index, c) in players.iter().enumerate() {
        if c.dead {
            continue;
        }
        let ax = dc_core::math::py_round(c.x + c.heading.cos() * (c.size + 28.0));
        let ay = dc_core::math::py_round(c.y + c.heading.sin() * (c.size + 28.0));
        p.set_pen(hex(if index == 0 { "#f7df93" } else { "#9fe7fa" }));
        p.draw_ellipse(Rect::new(ax - 5.0, ay - 5.0, 10.0, 10.0));
        p.draw_text(ax + 8.0, ay, &format!("P{}", index + 1));
    }
}

/// The single player's cursor cross-hair at `aim` (overlay pixels).
pub fn draw_cursor_aim(p: &mut Canvas, aim: (i32, i32)) {
    let (x, y) = (aim.0 as f64, aim.1 as f64);
    p.set_pen(hex("#f7df93"));
    p.draw_ellipse(Rect::new(x - 7.0, y - 7.0, 14.0, 14.0));
    p.draw_line(x - 12.0, y, x - 4.0, y);
    p.draw_line(x + 4.0, y, x + 12.0, y);
}

/// `_draw_released_banner`: `key` is the release binding; `area` the
/// mission area when there is a mission. The painter's font is the base.
pub fn draw_released_banner(p: &mut Canvas, window: &HudWindow, key: &str, area: Option<Rect>) {
    let text = format!("Controls released  \u{b7}  the raid is paused  \u{b7}  press {key} to play again");
    let mut font = p.font();
    font.set_point_size_f(11.0);
    font.set_bold(true);
    p.set_font(font);
    let width = (p.font_metrics().horizontal_advance(&text) + 40) as f64;
    let cx = area.map(|a| a.x + a.w / 2.0).unwrap_or(window.width as f64 / 2.0);
    let top = area.map(|a| a.y + a.h * 0.42).unwrap_or(window.height as f64 * 0.42);
    let bx = IRect::new((cx - width / 2.0) as i32, top as i32, width as i32, 34);
    p.set_pen(Pen::new(hex("#e8c170"), 2.0));
    p.set_brush(Color::rgba(30, 18, 8, 225));
    p.draw_rounded_rect(rf(bx), 10.0, 10.0);
    p.set_pen(hex("#fff2d6"));
    p.draw_text_in(rf(bx), TextFlags::ALIGN_CENTER, &text);
}

// -- the mission ------------------------------------------------------------------------

/// One screen of the mission's layout.
#[derive(Clone, Debug)]
pub struct UiScreen {
    pub index: usize,
    pub name: String,
    pub rect: Rect,
}

/// A way between two screens (`screen_layout.ScreenLink`).
#[derive(Clone, Debug)]
pub struct UiLink {
    pub a: usize,
    pub b: usize,
    /// "tunnel" (true) or "door".
    pub tunnel: bool,
    pub a_point: (f64, f64),
    pub b_point: (f64, f64),
}

impl UiLink {
    pub fn point_on(&self, screen: usize) -> (f64, f64) {
        if screen == self.a { self.a_point } else { self.b_point }
    }
    pub fn other(&self, screen: usize) -> usize {
        if screen == self.a { self.b } else { self.a }
    }
}

/// The mission's screens and how they join.
#[derive(Clone, Debug, Default)]
pub struct UiLayout {
    pub screens: Vec<UiScreen>,
    pub links: Vec<UiLink>,
}

impl UiLayout {
    pub fn multi(&self) -> bool {
        self.screens.len() > 1
    }
    fn screen(&self, index: usize) -> Option<&UiScreen> {
        self.screens.iter().find(|s| s.index == index).or_else(|| self.screens.get(index))
    }
}

/// A building (`mission.sites`).
#[derive(Clone, Debug, Default)]
pub struct UiSite {
    pub x: f64,
    pub y: f64,
    pub kind: String,
    pub name: String,
    pub owned: bool,
    pub contested: bool,
    pub warning: f64,
    pub supply: f64,
    pub reserves: i64,
    pub progress: f64,
}

/// An enemy winding up a charge (`mission.actors` with `windup > 0`).
#[derive(Clone, Copy, Debug, Default)]
pub struct UiActor {
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub windup: f64,
}

/// Dropped armour. `icon` is `armour_icon(item.id, item.slot, 64)` (dc-paint).
#[derive(Clone, Debug, Default)]
pub struct UiLoot {
    pub x: f64,
    pub y: f64,
    pub age: f64,
    pub tier: String,
    pub icon: Option<Image>,
}

/// Acid in flight: ground position and height above it.
#[derive(Clone, Copy, Debug, Default)]
pub struct UiGlob {
    pub x: f64,
    pub y: f64,
    pub height: f64,
}

/// Acid landing.
#[derive(Clone, Copy, Debug, Default)]
pub struct UiSplash {
    pub x: f64,
    pub y: f64,
    pub age: f64,
}

/// A word torn away by an enemy: from (x0, y0) toward the spider.
#[derive(Clone, Copy, Debug, Default)]
pub struct UiThread {
    pub x0: f64,
    pub y0: f64,
    pub spider_x: f64,
    pub spider_y: f64,
    pub age: f64,
}

/// How a creature on the minimap relates to the hero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation {
    Hero,
    Foe,
    Friend,
}

#[derive(Clone, Copy, Debug)]
pub struct UiCreature {
    pub x: f64,
    pub y: f64,
    pub dead: bool,
    pub relation: Relation,
}

/// A companion in the party line.
#[derive(Clone, Debug, Default)]
pub struct UiAlly {
    pub display_name: String,
    pub hp: f64,
    pub max_hp: f64,
    pub dead: bool,
}

/// The Reclaim desktop as the HUD shows it (`DesktopSurface::hud`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SurfaceHud {
    pub desktop_hp: f64,
    pub intact_windows: usize,
    pub hit_flash: f64,
}

/// What the mission overlays read from the mission.
#[derive(Clone, Debug)]
pub struct MissionView {
    /// `mission.area` (overlay pixels).
    pub area: Rect,
    pub layout: Option<UiLayout>,
    /// The screen the hero stands on (`layout.screen_at(hero.x, hero.y).index`).
    pub here: usize,
    pub sites: Vec<UiSite>,
    pub actors: Vec<UiActor>,
    pub loot: Vec<UiLoot>,
    pub hazards: Vec<UiGlob>,
    pub splashes: Vec<UiSplash>,
    pub threads: Vec<UiThread>,
    pub creatures: Vec<UiCreature>,
    pub objective: String,
    pub notice: String,
    pub notice_time: f64,
    /// "active", "victory", "defeat" or "crashed".
    pub state: String,
    pub save_error: String,
    pub ended: bool,
    pub end_clock: f64,
    pub end_screen_seconds: f64,
    /// "follow", "defend" or "attack".
    pub command: String,
    pub allies: Vec<UiAlly>,
    pub amber_mined: i64,
    pub found: usize,
    pub caught: i64,
    pub reward_text: String,
    pub words_eaten: i64,
    pub hero_name: String,
    pub hero_level: i64,
    /// Present in Reclaim the desktop.
    pub surface: Option<SurfaceHud>,
    pub intro_time: f64,
    pub blue_screen: f64,
    pub crash_screen_seconds: f64,
    /// The player's bindings of companion_follow / _defend / _attack; None
    /// when no spider is controlled (`window.player is None`).
    pub command_bindings: Option<[String; 3]>,
}

impl Default for MissionView {
    fn default() -> Self {
        MissionView {
            area: Rect::new(0.0, 0.0, 1600.0, 1000.0),
            layout: None,
            here: 0,
            sites: Vec::new(),
            actors: Vec::new(),
            loot: Vec::new(),
            hazards: Vec::new(),
            splashes: Vec::new(),
            threads: Vec::new(),
            creatures: Vec::new(),
            objective: String::new(),
            notice: String::new(),
            notice_time: 0.0,
            state: "active".into(),
            save_error: String::new(),
            ended: false,
            end_clock: 0.0,
            end_screen_seconds: 6.0,
            command: "follow".into(),
            allies: Vec::new(),
            amber_mined: 0,
            found: 0,
            caught: 0,
            reward_text: String::new(),
            words_eaten: 0,
            hero_name: String::new(),
            hero_level: 1,
            surface: None,
            intro_time: 0.0,
            blue_screen: 0.0,
            crash_screen_seconds: 7.0,
            command_bindings: None,
        }
    }
}

/// The status line under a building's name.
pub fn site_status(s: &UiSite, amber_mined: i64) -> String {
    if s.warning > 0.0 {
        return format!("EMERGING IN {:.1}s", s.warning);
    }
    if s.contested && !s.owned {
        return "Clear the defenders".into();
    }
    if s.owned {
        return match s.kind.as_str() {
            "home" => "Heal + refill silk".into(),
            "food" => format!("Healing supply {:.0}", s.supply),
            "silk" => "12 silk capacity + fast refill".into(),
            "hatchery" => "SEALED".into(),
            "nest" => "CLAIMED".into(),
            "outpost" => "TAKEN".into(),
            "infestation" => "DESTROYED".into(),
            "flynest" => "Flies swarm from here".into(),
            "venom" => "+20% damage for you".into(),
            "lookout" => "+30% range for you".into(),
            "amber" => format!("Mining amber ({amber_mined})"),
            "nursery" => "Hatches your spiderlings".into(),
            _ => "HELD".into(),
        };
    }
    match s.kind.as_str() {
        "venom" => "Enemy +20% damage - take it".into(),
        "lookout" => "Enemy +30% range - take it".into(),
        "nursery" => "Hatches enemy spiderlings".into(),
        "amber" => "Hold nearby to mine amber".into(),
        "outpost" => format!("{} raids left / hold to take", s.reserves),
        "infestation" => "Hold nearby to burn it out".into(),
        "hatchery" => format!("{} reserves / hold to seal", s.reserves),
        "nest" => "Seal hatchery + defeat guardian".into(),
        _ => "Hold nearby to capture".into(),
    }
}

/// `draw_buildings(painter, mission)`: screen links, every building with its
/// territory glow, name card and progress, enemy wind-ups, loot and acid.
pub fn draw_buildings(p: &mut Canvas, m: &MissionView) {
    draw_screen_links(p, m);
    p.save();
    p.set_font(Font::new(UI_FONT, 9.0));
    let scale = if m.area.w < 1100.0 || m.area.h < 800.0 { 0.72 } else { 1.0 };
    for s in &m.sites {
        let art = building_art(&s.kind, s.owned);
        p.draw_image_rect(Rect::new(s.x - 120.0 * scale, s.y - 139.0 * scale, 240.0 * scale, 190.0 * scale), &art);
        let color = hex(if s.owned { "#8ce0be" } else { "#efb28c" });
        // The territory: a soft glow in the owner's colour, not a dashed ring.
        let inner = alpha(color, 0);
        let edge = alpha(color, 70);
        let glow = RadialGradient::new(s.x, s.y, 92.0 * scale)
            .with_stop(0.0, inner)
            .with_stop(0.78, inner)
            .with_stop(0.95, edge)
            .with_stop(1.0, inner);
        p.save();
        p.translate(s.x, s.y);
        p.scale(1.0, 39.0 / 87.0);
        p.translate(-s.x, -s.y);
        p.set_pen(Pen::NONE);
        p.set_brush(glow);
        p.draw_ellipse_at(s.x, s.y, 92.0 * scale, 92.0 * scale);
        p.restore();
        let label = Rect::new(s.x - 95.0, s.y + 29.0, 190.0, 44.0);
        p.set_pen(Pen::new(hex("#9a7951"), 1.0));
        p.set_brush(Color::rgba(30, 24, 20, 235));
        p.draw_rounded_rect(label, 7.0, 7.0);
        p.set_pen(hex("#fff0cd"));
        p.draw_text_in(label.adjusted(4.0, 1.0, -4.0, -21.0), TextFlags::ALIGN_CENTER, &s.name);
        let status = site_status(s, m.amber_mined);
        p.set_pen(color);
        p.draw_text_in(label.adjusted(4.0, 21.0, -4.0, -1.0), TextFlags::ALIGN_CENTER, &status);
        if 0.0 < s.progress && s.progress < 1.0 {
            p.fill_rect(Rect::new(s.x - 83.0, s.y + 77.0, 166.0, 5.0), hex("#31291f"));
            p.fill_rect(Rect::new(s.x - 83.0, s.y + 77.0, 166.0 * s.progress, 5.0), hex("#eacb7f"));
        }
    }
    for a in &m.actors {
        if a.windup > 0.0 {
            p.set_brush(Brush::None);
            p.set_pen(Pen::new(hex("#ffb77a"), 2.5).style(PenStyle::Dash));
            p.draw_ellipse_at(a.x, a.y, a.size * 2.8, a.size * 2.8);
        }
    }
    p.restore();
    draw_loot(p, m);
    draw_hazards(p, m);
}

/// `draw_screen_links`: a chevron at each door, a web-lined hole at each
/// tunnel mouth, labelled with the screen it leads to.
pub fn draw_screen_links(p: &mut Canvas, m: &MissionView) {
    let Some(layout) = m.layout.as_ref().filter(|l| l.multi()) else { return };
    p.save();
    p.set_font(Font::with_weight(UI_FONT, 9.0, Weight::BOLD));
    for link in &layout.links {
        for here in [link.a, link.b] {
            let (mut x, mut y) = link.point_on(here);
            let Some(there) = layout.screen(link.other(here)) else { continue };
            let label;
            if link.tunnel {
                let glow = RadialGradient::new(x, y, 40.0)
                    .with_stop(0.0, Color::rgba(10, 6, 16, 250))
                    .with_stop(0.55, Color::rgba(40, 20, 60, 220))
                    .with_stop(1.0, Color::rgba(150, 110, 220, 0));
                p.set_pen(Pen::NONE);
                p.set_brush(glow);
                p.draw_ellipse_at(x, y, 40.0, 40.0);
                p.set_pen(Pen::new(Color::rgba(230, 220, 255, 150), 1.0));
                for i in 0..8 {
                    let a = i as f64 / 8.0 * TAU;
                    p.draw_line(x, y, x + a.cos() * 30.0, y + a.sin() * 30.0);
                }
                label = format!("Tunnel to {}", there.name);
            } else {
                let (ox, oy) = link.point_on(there.index);
                let angle = (oy - y).atan2(ox - x);
                p.set_pen(round_pen(Color::rgba(247, 223, 147, 200), 3.0));
                for step in [0.0, 12.0] {
                    let (cx, cy) = (x - angle.cos() * (30.0 - step), y - angle.sin() * (30.0 - step));
                    for side in [2.5, -2.5f64] {
                        p.draw_line(cx + (angle + side).cos() * 10.0, cy + (angle + side).sin() * 10.0, cx, cy);
                    }
                }
                label = format!("To {}", there.name);
                // The label stands on this side of the seam, clear of the other's.
                x -= angle.cos() * 95.0;
                y -= angle.sin() * 95.0;
            }
            let bx = Rect::new(x - 70.0, y + 30.0, 140.0, 20.0);
            p.set_pen(Pen::NONE);
            p.set_brush(Color::rgba(20, 16, 12, 190));
            p.draw_rounded_rect(bx, 6.0, 6.0);
            p.set_pen(hex("#f7df93"));
            p.draw_text_in(bx, TextFlags::ALIGN_CENTER, &label);
        }
    }
    p.restore();
}

/// `draw_hazards`: acid in flight, acid landing, and words torn away.
pub fn draw_hazards(p: &mut Canvas, m: &MissionView) {
    p.save();
    for g in &m.hazards {
        p.set_pen(Pen::NONE);
        p.set_brush(Color::rgba(0, 0, 0, 60));
        p.draw_ellipse_at(g.x, g.y, 7.0, 3.5);
        let glow = RadialGradient::new(g.x, g.y - g.height, 13.0)
            .with_stop(0.0, Color::rgba(235, 255, 150, 255))
            .with_stop(0.45, Color::rgba(140, 230, 40, 230))
            .with_stop(1.0, Color::rgba(90, 180, 20, 0));
        p.set_brush(glow);
        p.draw_ellipse_at(g.x, g.y - g.height, 13.0, 13.0);
    }
    for s in &m.splashes {
        let fade = (1.0 - s.age / 0.9).max(0.0);
        p.set_pen(Pen::new(Color::rgba(170, 255, 60, (220.0 * fade) as u8), 2.5));
        p.set_brush(Color::rgba(120, 220, 30, (90.0 * fade) as u8));
        let r = 14.0 + s.age * 40.0;
        p.draw_ellipse_at(s.x, s.y, r, r * 0.55);
    }
    for t in &m.threads {
        let fade = (1.0 - t.age / 0.7).max(0.0);
        let k = (t.age / 0.45).min(1.0);
        p.set_pen(Pen::new(Color::rgba(200, 40, 48, (230.0 * fade) as u8), 1.4));
        for i in 0..3 {
            let wobble = (t.age * 20.0 + i as f64 * 2.0).sin() * 6.0;
            let mx = t.x0 + (t.spider_x - t.x0) * k;
            let my = t.y0 + (t.spider_y - t.y0) * k + wobble;
            p.draw_line(t.x0 + i as f64 * 4.0, t.y0, mx, my);
        }
    }
    p.restore();
}

/// `minimap_rect(window, mission)`: (box, scale, x0, y0).
pub fn minimap_rect(window: &HudWindow, layout: &UiLayout) -> (Rect, f64, f64, f64) {
    let area = home_area(window);
    let xs = layout.screens.iter().flat_map(|s| [s.rect.x, s.rect.right()]);
    let ys = layout.screens.iter().flat_map(|s| [s.rect.y, s.rect.bottom()]);
    let (min_x, max_x) = xs.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)));
    let (min_y, max_y) = ys.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)));
    let (span_w, span_h) = (max_x - min_x, max_y - min_y);
    let scale = (230.0 / span_w.max(1.0)).min(120.0 / span_h.max(1.0));
    let (w, h) = (span_w * scale, span_h * scale);
    (Rect::new(qright(area) as f64 - w - 34.0, area.y as f64 + 16.0, w + 20.0, h + 36.0), scale, min_x, min_y)
}

/// `draw_minimap`: every screen, where you are, companions, enemies and outposts.
pub fn draw_minimap(p: &mut Canvas, window: &HudWindow, m: &MissionView) {
    let Some(layout) = m.layout.as_ref().filter(|l| l.multi()) else { return };
    let (bx, scale, x0, y0) = minimap_rect(window, layout);
    board(p, aligned(bx), 8.0);
    let at = |x: f64, y: f64| (bx.x + 10.0 + (x - x0) * scale, bx.y + 26.0 + (y - y0) * scale);
    p.set_font(Font::with_weight(UI_FONT, 8.0, Weight::BOLD));
    p.set_pen(hex("#f9df9d"));
    let here = layout.screen(m.here).or(layout.screens.first());
    let here_name = here.map(|s| s.name.as_str()).unwrap_or("");
    let here_index = here.map(|s| s.index);
    p.draw_text_in(
        Rect::new(bx.x + 8.0, bx.y + 4.0, bx.w - 16.0, 18.0),
        TextFlags::ALIGN_LEFT | TextFlags::ALIGN_VCENTER,
        &format!("Screens  \u{b7}  you: {here_name}"),
    );
    for screen in &layout.screens {
        let r = screen.rect;
        let (ax, ay) = at(r.x, r.y);
        let (bx2, by2) = at(r.right(), r.bottom());
        let rect = Rect::from_ltrb(ax, ay, bx2, by2).adjusted(1.0, 1.0, -1.0, -1.0);
        p.set_pen(Pen::new(hex(if Some(screen.index) == here_index { "#f7df93" } else { "#9a7951" }), 1.4));
        p.set_brush(Color::rgba(20, 30, 38, 200));
        p.draw_rect(rect);
    }
    p.set_pen(Pen::NONE);
    for site in &m.sites {
        let colour = if site.owned {
            "#8ce0be"
        } else if site.kind == "outpost" || site.kind == "infestation" {
            "#ff9a3c"
        } else {
            "#c9a36a"
        };
        p.set_brush(hex(colour));
        let (sx, sy) = at(site.x, site.y);
        p.draw_rect(Rect::new(sx - 3.0, sy - 3.0, 6.0, 6.0));
    }
    for link in layout.links.iter().filter(|l| l.tunnel) {
        p.set_brush(hex("#b58cff"));
        for point in [link.a_point, link.b_point] {
            let (px, py) = at(point.0, point.1);
            p.draw_ellipse_at(px, py, 2.5, 2.5);
        }
    }
    for c in &m.creatures {
        if c.dead {
            continue;
        }
        let (colour, r) = match c.relation {
            Relation::Hero => ("#ffd76a", 3.5),
            Relation::Foe => ("#ff5a4a", 2.2),
            Relation::Friend => ("#7fe0a0", 2.5),
        };
        p.set_brush(hex(colour));
        let (cx, cy) = at(c.x, c.y);
        p.draw_ellipse_at(cx, cy, r, r);
    }
}

/// `draw_reclaim_extras`: the desktop's health, and the opening card.
pub fn draw_reclaim_extras(p: &mut Canvas, window: &HudWindow, m: &MissionView) {
    let Some(surface) = m.surface else { return };
    draw_desktop_health(p, window, m, &surface);
    if m.intro_time <= 0.0 || m.ended {
        return;
    }
    let fade = (m.intro_time / 0.8).min(1.0);
    let area = m.area;
    let card = Rect::new(area.x + area.w / 2.0 - 330.0, area.y + area.h / 2.0 - 110.0, 660.0, 220.0);
    p.set_pen(Pen::new(Color::rgba(214, 164, 72, (255.0 * fade) as u8), 2.0));
    p.set_brush(Color::rgba(16, 10, 8, (235.0 * fade) as u8));
    p.draw_rounded_rect(card, 12.0, 12.0);
    p.set_font(Font::with_weight(UI_FONT, 24.0, Weight::BLACK));
    p.set_pen(Color::rgba(246, 207, 106, (255.0 * fade) as u8));
    p.draw_text_in(card.adjusted(20.0, 16.0, -20.0, -140.0), TextFlags::ALIGN_CENTER, "Reclaim the desktop");
    p.set_font(Font::new(UI_FONT, 11.0));
    p.set_pen(Color::rgba(242, 231, 211, (255.0 * fade) as u8));
    let screens = m.layout.as_ref().map(|l| l.screens.len()).unwrap_or(1);
    let many = if screens > 1 { format!(" - all {screens} of them") } else { String::new() };
    let text = format!(
        "Your desktop has been taken. You have no control of it now: every screen is frozen and infested{many}.\n\
         Destroy the nests before they devour its words and their acid melts it.\n\
         Esc and Leave gives you the desktop back at once. Nothing real is harmed."
    );
    p.draw_text_in(
        card.adjusted(26.0, 78.0, -26.0, -14.0),
        TextFlags::ALIGN_HCENTER | TextFlags::ALIGN_TOP | TextFlags::TEXT_WORD_WRAP,
        &text,
    );
}

/// `draw_desktop_health`: "DESKTOP" and its health bar under the mission
/// banner; it flashes when a crack lands on the bare desktop.
pub fn draw_desktop_health(p: &mut Canvas, window: &HudWindow, m: &MissionView, surface: &SurfaceHud) {
    let banner = mission_banner_rect(window);
    let hp = surface.desktop_hp;
    let windows = surface.intact_windows;
    let bx = Rect::new(banner.x as f64 + 14.0, qbottom(banner) as f64 + 6.0, banner.w as f64 - 28.0, 30.0);
    let flash = (surface.hit_flash / 0.5).min(1.0);
    let rim = if flash != 0.0 { Color::rgba(255, 120, 90, (120.0 + 135.0 * flash) as u8) } else { hex("#9a7951") };
    p.set_pen(Pen::new(rim, 1.5));
    p.set_brush(Color::rgba(24, 18, 14, 230));
    p.draw_rounded_rect(bx, 7.0, 7.0);
    p.set_font(Font::with_weight(UI_FONT, 9.0, Weight::BLACK));
    p.set_pen(hex("#9fd3ff"));
    let label = Rect::new(bx.x + 10.0, bx.y, 78.0, bx.h);
    p.draw_text_in(label, TextFlags::ALIGN_LEFT | TextFlags::ALIGN_VCENTER, "DESKTOP");
    let bar = Rect::new(label.right() + 4.0, bx.y + 8.0, bx.w - label.w - 24.0, bx.h - 16.0);
    p.set_pen(Pen::NONE);
    p.set_brush(Color::rgba(8, 6, 4, 230));
    p.draw_rounded_rect(bar, 4.0, 4.0);
    let mut fill = if hp > 0.5 {
        hex("#6ccf6a")
    } else if hp > 0.25 {
        hex("#e0b24a")
    } else {
        hex("#e0503e")
    };
    if flash != 0.0 {
        fill = lighter(fill, 100 + (60.0 * flash) as i32);
    }
    p.set_brush(fill);
    p.draw_rounded_rect(Rect::new(bar.x + 1.0, bar.y + 1.0, (bar.w - 2.0) * hp, bar.h - 2.0), 3.0, 3.0);
    p.set_font(Font::with_weight(UI_FONT, 8.0, Weight::BOLD));
    p.set_pen(hex("#fff6de"));
    let shield = if windows > 0 {
        format!("{windows} window{} guarding it", if windows != 1 { "s" } else { "" })
    } else {
        "no windows left".to_string()
    };
    p.draw_text_in(
        bar,
        TextFlags::ALIGN_CENTER,
        &format!("HP {:.0} / 100  \u{b7}  {shield}  \u{b7}  {} words devoured", hp * 100.0, m.words_eaten),
    );
}

/// `QFontMetrics(Segoe UI at pixel size px).lineSpacing()` as Qt 5.15 on
/// Windows (GDI, hinted) reports it, px 6..=60, Normal and Light alike.
/// dc-render's emulated metrics are off by one or two pixels at scattered
/// sizes here (e.g. 10 px: Qt 12, dc-render 14), which spread the blue
/// screen's wrapped paragraphs; see PORT_STATUS.md.
const QT_SEGOE_LINE_SPACING: [i32; 55] = [
    8, 10, 11, 12, 12, 13, 15, 17, 19, 20, 21, 23, 25, 25, 28, 30, 30, 31, 32, 35, 36, 37, 38, 40, 41, 42, 45, 45, 46,
    47, 48, 50, 51, 52, 54, 55, 57, 59, 60, 61, 62, 62, 65, 66, 67, 68, 70, 71, 72, 74, 74, 76, 77, 78, 81,
];

/// Qt's line spacing for the painter's Segoe UI pixel-size font, else dc-render's.
fn qt_line_spacing(font: &Font) -> f64 {
    let px = font.pixel_size_f().round() as i32;
    if font.family().eq_ignore_ascii_case(UI_FONT) && (6..=60).contains(&px) {
        return QT_SEGOE_LINE_SPACING[(px - 6) as usize] as f64;
    }
    font.metrics().line_spacing() as f64
}

/// `drawText(rect, AlignLeft | AlignTop | TextWordWrap, text)`, wrapped at
/// spaces and laid out line by line at Qt's line spacing (the workaround
/// above), clipped to `rect` as Qt clips.
fn draw_wrapped_qt(p: &mut Canvas, rect: Rect, text: &str) {
    let font = p.font();
    let fm = font.metrics();
    let step = qt_line_spacing(&font);
    let mut lines: Vec<String> = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if !line.is_empty() && fm.horizontal_advance_f(&candidate) > rect.w {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = candidate;
            }
        }
        lines.push(line);
    }
    p.save();
    p.set_clip_rect_op(rect, ClipOp::Intersect);
    for (i, line) in lines.iter().enumerate() {
        let r = Rect::new(rect.x, rect.y + i as f64 * step, rect.w, step);
        if !line.is_empty() {
            p.draw_text_in(r, TextFlags::ALIGN_LEFT | TextFlags::ALIGN_TOP | TextFlags::TEXT_DONT_CLIP, line);
        }
    }
    p.restore();
}

/// The blue screen's progress, 0..100 (`draw_blue_screen`'s `percent`).
pub fn blue_screen_percent(shown: f64, total: f64) -> i64 {
    100.min((shown / (total - 0.8).max(0.1) * 100.0) as i64)
}

/// `draw_blue_screen`: the desktop broke; a blue screen on every screen.
pub fn draw_blue_screen(p: &mut Canvas, m: &MissionView) {
    let shown = m.blue_screen;
    if shown <= 0.0 {
        return;
    }
    let fade = (shown / 0.25).min(1.0);
    let percent = blue_screen_percent(shown, m.crash_screen_seconds);
    let rects: Vec<Rect> = match &m.layout {
        Some(l) => l.screens.iter().map(|s| s.rect).collect(),
        None => vec![m.area],
    };
    let blue = Color::rgba(0, 120, 215, (255.0 * fade) as u8);
    p.save();
    p.set_clipping(false);
    for r in rects {
        p.set_pen(Pen::NONE);
        p.set_brush(blue);
        p.draw_rect(r);
        let unit = 0.6f64.max((r.w / 1920.0).min(r.h / 1080.0));
        let left = r.x + r.w * 0.1;
        let mut top = r.y + r.h * 0.16;
        let white = Color::rgba(255, 255, 255, (255.0 * fade) as u8);
        p.set_pen(white);
        let mut face = Font::with_weight(UI_FONT, 12.0, Weight::LIGHT);
        face.set_pixel_size((150.0 * unit) as i32);
        p.set_font(face);
        p.draw_text_in(
            Rect::new(left, top, r.w * 0.8, 190.0 * unit),
            TextFlags::ALIGN_LEFT | TextFlags::ALIGN_TOP,
            ":(",
        );
        top += 210.0 * unit;
        let mut text = Font::with_weight(UI_FONT, 12.0, Weight::LIGHT);
        text.set_pixel_size((34.0 * unit) as i32);
        p.set_font(text);
        draw_wrapped_qt(
            p,
            Rect::new(left, top, r.w * 0.78, 110.0 * unit),
            "Your desktop ran into a problem: the spiders broke it. We're just collecting the pieces, and then \
             we'll take you back.",
        );
        top += 120.0 * unit;
        p.draw_text_in(
            Rect::new(left, top, r.w * 0.7, 50.0 * unit),
            TextFlags::ALIGN_LEFT | TextFlags::ALIGN_TOP,
            &format!("{percent}% complete"),
        );
        top += 80.0 * unit;
        let code = Rect::new(left, top, 118.0 * unit, 118.0 * unit);
        p.set_brush(white);
        p.set_pen(Pen::NONE);
        p.draw_rect(code);
        let cell = code.w / 11.0;
        p.set_brush(blue);
        let mut seed: i64 = 7;
        for j in 0..11 {
            for i in 0..11 {
                let corner = (i < 3 && j < 3) || (i > 7 && j < 3) || (i < 3 && j > 7);
                seed = (seed * 1103515245 + 12345) & 0x7FFF_FFFF;
                if (corner && (i == 1 || i == 9 || j == 1 || j == 9)) || (!corner && seed % 3 == 0) {
                    p.draw_rect(Rect::new(code.x + i as f64 * cell, code.y + j as f64 * cell, cell, cell));
                }
            }
        }
        let mut small = Font::new(UI_FONT, 12.0);
        small.set_pixel_size((17.0 * unit) as i32);
        p.set_font(small);
        p.set_pen(white);
        let info = Rect::new(code.right() + 24.0 * unit, code.y, r.w * 0.6, code.h * 1.6);
        draw_wrapped_qt(
            p,
            info,
            "The raid is over. Nothing you found or earned in it is kept, and nothing was lost:\n\
             your spider is just as it was before the raid.\n\n\
             Your real desktop was never touched. Back to Adventure in a moment.\n\n\
             Stop code: DESKTOP_DEVOURED",
        );
    }
    p.restore();
}

/// `draw_loot`: dropped armour bobbing in a glow of its quality's colour.
pub fn draw_loot(p: &mut Canvas, m: &MissionView) {
    if m.loot.is_empty() {
        return;
    }
    p.save();
    for loot in &m.loot {
        let colour = hex(tier_colour(&loot.tier));
        let pulse = 0.5 + 0.5 * (loot.age * 4.0).sin();
        let bob = (loot.age * 3.0).sin() * 3.0;
        let glow = RadialGradient::new(loot.x, loot.y + bob, 30.0)
            .with_stop(0.0, alpha(colour, (120.0 + 80.0 * pulse) as i64))
            .with_stop(1.0, alpha(colour, 0));
        p.set_pen(Pen::NONE);
        p.set_brush(glow);
        p.draw_ellipse_at(loot.x, loot.y + bob, 30.0, 30.0);
        if let Some(icon) = &loot.icon {
            p.draw_image_rect(Rect::new(loot.x - 17.0, loot.y - 17.0 + bob, 34.0, 34.0), icon);
        }
    }
    p.restore();
}

/// `command_rects(window)`: the follow / defend / attack buttons.
pub fn command_rects(window: &HudWindow) -> [(&'static str, IRect); 3] {
    let r = hud_rect(window, 0);
    let width = (r.w - 32).div_euclid(3);
    let at = |i: i32| IRect::new(r.x + 12 + i * (width + 4), r.y + 152, width, 30);
    [("follow", at(0)), ("defend", at(1)), ("attack", at(2))]
}

/// `mission_banner_rect(window)`.
pub fn mission_banner_rect(window: &HudWindow) -> IRect {
    let area = home_area(window);
    let width = 640.min(area.w - 24);
    IRect::new(qcenter_x(area) - width.div_euclid(2), area.y + 16, width, 91)
}

fn title_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

/// `draw_mission_hud(painter, window, mission)`: the objective banner, the
/// companion commands, the minimap, Reclaim's extras and the end screens.
pub fn draw_mission_hud(p: &mut Canvas, window: &HudWindow, m: &MissionView) {
    p.save();
    p.set_clipping(false);
    let r = mission_banner_rect(window);
    board(p, r, 10.0);
    p.set_font(Font::with_weight(UI_FONT, 12.0, Weight::BOLD));
    p.set_pen(hex("#f9df9d"));
    let text = p.font_metrics().elided_text(&m.objective, ElideMode::Right, (r.w - 28) as f64);
    p.draw_text_in(rf(qadjusted(r, 14, 7, -14, -57)), TextFlags::ALIGN_LEFT | TextFlags::ALIGN_VCENTER, &text);
    p.set_font(Font::new(UI_FONT, 9.0));
    p.set_pen(hex("#f2e7d3"));
    let mut detail = if m.notice_time > 0.0 {
        m.notice.clone()
    } else {
        "Clear defenders, then stand by the entrance. Retreat home to recover.".to_string()
    };
    if m.state != "active" {
        detail = if !m.save_error.is_empty() {
            m.save_error.clone()
        } else if m.state == "victory" {
            "Victory progress saved. Esc: replay or exit.".into()
        } else {
            "Esc: retry or exit. Your Companion colony is unchanged.".into()
        };
    }
    p.draw_text_in(rf(qadjusted(r, 14, 33, -14, -9)), TextFlags::TEXT_WORD_WRAP | TextFlags::ALIGN_LEFT, &detail);
    if let Some(bindings) = &m.command_bindings {
        let hr = hud_rect(window, 0);
        let living: Vec<&UiAlly> = m.allies.iter().filter(|a| !a.dead).collect();
        for (i, (command, rect)) in command_rects(window).into_iter().enumerate() {
            let active = command == m.command && !living.is_empty();
            p.set_pen(Pen::new(hex(if active { "#8bd9bd" } else { "#95794e" }), 1.5));
            p.set_brush(hex(if active { "#29463d" } else { "#312820" }));
            p.draw_rounded_rect(rf(rect), 5.0, 5.0);
            p.set_pen(hex("#f9edce"));
            let key = short_binding(&bindings[i]);
            p.draw_text_in(rf(rect), TextFlags::ALIGN_CENTER, &format!("{key}  {}", title_case(command)));
        }
        p.set_pen(hex("#e7d4ac"));
        let party = living
            .iter()
            .map(|a| format!("{} {:.0}/{:.0}", a.display_name, a.hp, a.max_hp))
            .collect::<Vec<_>>()
            .join("  ");
        let party = if party.is_empty() { "Companions fallen".to_string() } else { party };
        let elided = p.font_metrics().elided_text(&party, ElideMode::Right, (hr.w - 28) as f64);
        p.draw_text_in(rf(qadjusted(hr, 14, 186, -14, -3)), TextFlags::ALIGN_LEFT | TextFlags::ALIGN_VCENTER, &elided);
    }
    p.restore();
    p.save();
    p.set_clipping(false);
    draw_minimap(p, window, m);
    draw_reclaim_extras(p, window, m);
    p.restore();
    if m.state == "crashed" {
        draw_blue_screen(p, m);
    } else if m.ended {
        draw_end_title(p, m);
    }
}

/// `end_title_text(mission)`: (title, subtitle).
pub fn end_title_text(m: &MissionView) -> (String, String) {
    let mut extras: Vec<String> = Vec::new();
    if m.found > 0 {
        extras.push(format!("found {} piece{} of armour", m.found, if m.found != 1 { "s" } else { "" }));
    }
    if m.amber_mined != 0 {
        extras.push(format!("mined {} amber", m.amber_mined));
    }
    if m.caught != 0 {
        extras.push(format!("caught {} flies", m.caught));
    }
    if !m.reward_text.is_empty() {
        extras.push(m.reward_text.clone());
    }
    let mut tail: String = extras.iter().map(|e| format!("  \u{b7}  {e}")).collect();
    if m.words_eaten != 0 {
        let plural = if m.words_eaten != 1 { "s" } else { "" };
        tail += &format!("  \u{b7}  {} word{plural} devoured", m.words_eaten);
    }
    if m.state == "victory" {
        return (
            "VICTORY".into(),
            format!("The desktop is yours  \u{b7}  {} reached level {}{tail}", m.hero_name, m.hero_level),
        );
    }
    ("DEFEAT".into(), format!("{} has fallen  \u{b7}  level, XP and loot are kept{tail}", m.hero_name))
}

/// `draw_end_title`: a large VICTORY or DEFEAT across the middle of the arena.
pub fn draw_end_title(p: &mut Canvas, m: &MissionView) {
    let area = m.area;
    let fade = (m.end_clock / 0.4).min(1.0);
    let won = m.state == "victory";
    let (title, subtitle) = end_title_text(m);
    let (cx, cy) = (area.x + area.w / 2.0, area.y + area.h / 2.0);
    let band = Rect::new(area.x, cy - 120.0, area.w, 240.0);
    p.save();
    p.set_clipping(false);
    p.set_pen(Pen::NONE);
    p.set_brush(Color::rgba(18, 12, 8, (200.0 * fade) as u8));
    p.draw_rect(band);
    let edge = alpha(hex(if won { "#d6a448" } else { "#b8472f" }), (255.0 * fade) as i64);
    p.set_pen(Pen::new(edge, 3.0));
    p.draw_line(band.x, band.y, band.right(), band.y);
    p.draw_line(band.x, band.bottom(), band.right(), band.bottom());
    let mut font = Font::with_weight(UI_FONT, 12.0, Weight::BLACK);
    font.set_pixel_size(110.0f64.min(56.0f64.max(area.w * 0.075)) as i32);
    font.set_letter_spacing_absolute(6.0);
    let mut path = Path::new();
    path.add_text(0.0, 0.0, &font, &title);
    let bxr = path.bounding_rect();
    let (bcx, bcy) = bxr.center();
    path.translate(cx - bcx, cy - 18.0 - bcy);
    p.set_pen(round_pen(Color::rgba(10, 6, 4, (230.0 * fade) as u8), 6.0));
    p.set_brush(Brush::None);
    p.draw_path(&path);
    p.set_pen(Pen::NONE);
    p.set_brush(alpha(hex(if won { "#f6cf6a" } else { "#e0604a" }), (255.0 * fade) as i64));
    p.draw_path(&path);
    p.set_font(Font::with_weight(UI_FONT, 15.0, Weight::BOLD));
    p.set_pen(Color::rgba(246, 226, 184, (255.0 * fade) as u8));
    p.draw_text_in(Rect::new(area.x, cy + 38.0, area.w, 30.0), TextFlags::ALIGN_CENTER, &subtitle);
    let left = (m.end_screen_seconds - m.end_clock).max(0.0);
    p.set_font(Font::new(UI_FONT, 10.0));
    p.set_pen(Color::rgba(220, 195, 154, (220.0 * fade) as u8));
    p.draw_text_in(
        Rect::new(area.x, cy + 72.0, area.w, 24.0),
        TextFlags::ALIGN_CENTER,
        &format!("Back to Adventure in {left:.0}s"),
    );
    p.restore();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window() -> HudWindow {
        HudWindow { width: 1600, height: 1000, primary: Some(IRect::new(0, 0, 1600, 1000)), ..Default::default() }
    }

    #[test]
    fn hud_and_banner_rects_follow_the_main_monitor() {
        let w = window();
        assert_eq!(hud_rect(&w, 0), IRect::new(585, 775, 430, 210));
        assert_eq!(mission_banner_rect(&w), IRect::new(479, 16, 640, 91));
        let coop = HudWindow { players: 2, ..w.clone() };
        assert_eq!(hud_rect(&coop, 0), IRect::new(10, 775, 430, 210));
        assert_eq!(hud_rect(&coop, 1), IRect::new(452, 775, 430, 210));
        let dragged = HudWindow { hud_position: Some((5000, -40)), ..w.clone() };
        assert_eq!(hud_rect(&dragged, 0), IRect::new(1162, 8, 430, 210));
        let cmds = command_rects(&w);
        assert_eq!(cmds[0], ("follow", IRect::new(597, 927, 132, 30)));
        assert_eq!(cmds[2].1.x, 597 + 2 * 136);
        // A main monitor too small to hold the HUD: the whole window.
        let tiny = HudWindow { primary: Some(IRect::new(0, 0, 100, 100)), ..w };
        assert_eq!(home_area(&tiny), IRect::new(0, 0, 1600, 1000));
    }

    #[test]
    fn short_bindings_fit_a_slot() {
        assert_eq!(short_binding("Mouse Left"), "L CLICK");
        assert_eq!(short_binding("space"), "SPACE");
    }

    #[test]
    fn site_status_lines() {
        let mut s = UiSite { kind: "outpost".into(), reserves: 3, ..Default::default() };
        assert_eq!(site_status(&s, 0), "3 raids left / hold to take");
        s.warning = 2.25;
        assert_eq!(site_status(&s, 0), "EMERGING IN 2.2s");
        s.warning = 0.0;
        s.owned = true;
        assert_eq!(site_status(&s, 0), "TAKEN");
        s.kind = "amber".into();
        assert_eq!(site_status(&s, 4), "Mining amber (4)");
    }

    #[test]
    fn end_titles_count_what_was_won() {
        let mut m =
            MissionView { hero_name: "Webby".into(), hero_level: 4, state: "victory".into(), ..Default::default() };
        m.found = 1;
        m.words_eaten = 2;
        let (t, s) = end_title_text(&m);
        assert_eq!(t, "VICTORY");
        assert_eq!(
            s,
            "The desktop is yours  \u{b7}  Webby reached level 4  \u{b7}  found 1 piece of armour  \u{b7}  2 words devoured"
        );
        m.state = "defeat".into();
        m.found = 0;
        m.words_eaten = 1;
        assert_eq!(
            end_title_text(&m).1,
            "Webby has fallen  \u{b7}  level, XP and loot are kept  \u{b7}  1 word devoured"
        );
    }

    #[test]
    fn qt_line_spacing_replaces_dc_renders_at_small_pixel_sizes() {
        let mut f = Font::new(UI_FONT, 12.0);
        f.set_pixel_size(10);
        assert_eq!(qt_line_spacing(&f), 12.0);
        f.set_pixel_size(20);
        assert_eq!(qt_line_spacing(&f), 28.0);
    }

    #[test]
    fn the_blue_screen_counts_up_to_complete() {
        assert_eq!(blue_screen_percent(0.1, 7.0), 1);
        assert_eq!(blue_screen_percent(6.2, 7.0), 100);
        assert_eq!(blue_screen_percent(30.0, 7.0), 100);
    }

    fn painted(f: impl FnOnce(&mut Canvas)) -> Image {
        let mut image = Image::new(1600, 1000);
        image.paint(|p| {
            p.set_render_hint(RenderHint::Antialiasing, true);
            f(p)
        });
        image
    }

    #[test]
    fn the_desktop_health_bar_flashes_and_shortens() {
        let w = window();
        let m = MissionView::default();
        let full = painted(|p| {
            draw_desktop_health(p, &w, &m, &SurfaceHud { desktop_hp: 1.0, intact_windows: 2, hit_flash: 0.0 })
        });
        let low = painted(|p| {
            draw_desktop_health(p, &w, &m, &SurfaceHud { desktop_hp: 0.2, intact_windows: 0, hit_flash: 0.0 })
        });
        let hit = painted(|p| {
            draw_desktop_health(p, &w, &m, &SurfaceHud { desktop_hp: 1.0, intact_windows: 2, hit_flash: 0.5 })
        });
        let banner = mission_banner_rect(&w);
        // Bar from x = banner.x + 14 + 10 + 78 + 4 ..; sample near its right end.
        let (x, y) = ((banner.x + banner.w - 40) as u32, (qbottom(banner) + 6 + 15) as u32);
        assert!(full.pixel(x, y).g > 150, "a full bar is green");
        assert!(low.pixel(x, y).g < 40, "a low bar is short");
        let rim = (banner.x as u32 + 20, (qbottom(banner) + 6) as u32);
        assert_ne!(full.pixel(rim.0, rim.1), hit.pixel(rim.0, rim.1), "a crack flashes the rim");
    }

    #[test]
    fn the_blue_screen_covers_every_screen() {
        let layout = UiLayout {
            screens: vec![
                UiScreen { index: 0, name: "Main".into(), rect: Rect::new(0.0, 0.0, 800.0, 1000.0) },
                UiScreen { index: 1, name: "Right".into(), rect: Rect::new(800.0, 0.0, 800.0, 1000.0) },
            ],
            links: vec![],
        };
        let m = MissionView { layout: Some(layout), blue_screen: 1.0, state: "crashed".into(), ..Default::default() };
        let image = painted(|p| draw_blue_screen(p, &m));
        assert_eq!(image.pixel(5, 990), Color::rgb(0, 120, 215));
        assert_eq!(image.pixel(1590, 5), Color::rgb(0, 120, 215));
    }

    #[test]
    fn the_hud_paints_its_board_bars_and_cards() {
        let w = window();
        let player = HudPlayer {
            display_name: "Webby".into(),
            level: 3,
            hp: 50.0,
            max_hp: 100.0,
            energy: 80.0,
            max_energy: 100.0,
            silk: 5.0,
            silk_capacity: 10.0,
            jump_energy: 10.0,
            web_energy: 5.0,
            dead: true,
            ..Default::default()
        };
        let image = painted(|p| draw_hud(p, &w, &player, 0));
        let r = hud_rect(&w, 0);
        assert_eq!(image.pixel(r.x as u32 + 100, r.y as u32 + 100).a, 255, "the board is opaque");
        assert_eq!(image.pixel(r.x as u32 - 20, r.y as u32 + 100).a, 0);
        // Health bar: red, filled to half.
        let y = (r.y + 44 + 8) as u32;
        let left = image.pixel(r.x as u32 + 40, y);
        let right = image.pixel((r.x + r.w - 40) as u32, y);
        assert!(left.r > left.b + 40, "{left:?}");
        assert!(right.r < 80, "{right:?}");
    }
}
