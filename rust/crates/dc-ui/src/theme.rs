//! Port of app/wood_theme.py: the carved-wood look shared by every window.
//!
//! The Python module produced Qt style sheets (`dialog_qss`, `companion_qss`,
//! `shell_qss`, `app_qss`, `controls_qss`, `scrollbar_qss`). In egui the same
//! looks are [`egui::Visuals`] presets plus reusable [`egui::Frame`]s, and the
//! painted pieces (carved buttons, pill switches, key caps, bars) live in
//! [`crate::widgets`]. The pictures and textures are PNGs in `assets/ui/`,
//! loaded by [`crate::assets`]; when an asset is missing everything falls back
//! to the plain colours below, so the interface still reads as wood.

use std::sync::{Arc, OnceLock};

use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke, Visuals, epaint::Shadow,
};

/// `#rrggbb` as a colour. Panics on malformed input only in debug builds;
/// release builds fall back to magenta so a typo is visible, not fatal.
pub const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// Parse `#rrggbb` (or `rrggbb`).
pub fn hex(text: &str) -> Color32 {
    let t = text.trim_start_matches('#');
    if t.len() != 6 {
        debug_assert!(false, "bad colour {text}");
        return Color32::from_rgb(255, 0, 255);
    }
    let p = |i: usize| u8::from_str_radix(&t[i..i + 2], 16).unwrap_or(0);
    Color32::from_rgb(p(0), p(2), p(4))
}

// Palette. Light text sits on walnut, dark text on parchment.
pub const WALNUT: Color32 = rgb(0x3f, 0x26, 0x16);
pub const WALNUT_DEEP: Color32 = rgb(0x2a, 0x18, 0x0c);
pub const OAK: Color32 = rgb(0xa8, 0x74, 0x3f);
pub const OAK_LIGHT: Color32 = rgb(0xc9, 0x9a, 0x5e);
pub const PARCHMENT: Color32 = rgb(0xf4, 0xe7, 0xcc);
pub const PARCHMENT_DEEP: Color32 = rgb(0xe8, 0xd3, 0xab);
pub const INK: Color32 = rgb(0x2e, 0x1d, 0x10);
pub const INK_SOFT: Color32 = rgb(0x6a, 0x4a, 0x2c);
pub const CREAM: Color32 = rgb(0xf6, 0xe2, 0xb8);
pub const CREAM_SOFT: Color32 = rgb(0xdc, 0xc3, 0x9a);
pub const BRASS: Color32 = rgb(0xd6, 0xa4, 0x48);
pub const UI_FONT: &str = "Segoe UI";
pub const BRASS_DEEP: Color32 = rgb(0x9c, 0x6d, 0x22);
pub const HEALTH: Color32 = rgb(0xb8, 0x47, 0x2f);
/// Yellow, as under the spiders.
pub const STAMINA: Color32 = rgb(0xf5, 0xc5, 0x18);

// Colours the style sheets spelled inline.
/// The bottom stop of a wood button's gradient.
pub const OAK_BOTTOM: Color32 = rgb(0x8e, 0x5d, 0x30);
/// Text on a wood button.
pub const BUTTON_TEXT: Color32 = rgb(0xff, 0xf5, 0xe0);
/// The top stop of a brass button / selected tab.
pub const BRASS_TOP: Color32 = rgb(0xe8, 0xbd, 0x62);
/// A brass button's border.
pub const BRASS_BORDER: Color32 = rgb(0x5a, 0x3a, 0x10);
/// `:disabled` button background and text.
pub const DISABLED_BG: Color32 = rgb(0x5a, 0x43, 0x31);
pub const DISABLED_TEXT: Color32 = rgb(0x9c, 0x8a, 0x70);
/// Inputs on the Companion editor.
pub const INPUT_BG: Color32 = rgb(0xff, 0xfa, 0xf0);
pub const INPUT_BORDER: Color32 = rgb(0xb0, 0x86, 0x58);
/// The Companion editor's group boxes.
pub const GROUP_BORDER: Color32 = rgb(0x8a, 0x5a, 0x2e);
/// The stop button's hover.
pub const STOP_HOVER: Color32 = rgb(0x8c, 0x3a, 0x24);
pub const STOP_HOVER_TEXT: Color32 = rgb(0xff, 0xf0, 0xe0);
/// Alternate table rows and grid lines.
pub const ROW_ALT: Color32 = rgb(0xfa, 0xf1, 0xde);
pub const GRID_LINE: Color32 = rgb(0xea, 0xd9, 0xb8);
pub const TABLE_SELECTION: Color32 = rgb(0xe4, 0xc6, 0x90);
pub const HEADER_TEXT: Color32 = rgb(0x4a, 0x2c, 0x14);
pub const HEADER_RULE: Color32 = rgb(0xd3, 0xb9, 0x8d);
/// Second player's badge.
pub const P2_BLUE: Color32 = rgb(0x7e, 0xc4, 0xe0);
/// A key cap's face, ink and rim.
pub const KEYCAP_FACE: Color32 = rgb(0xf3, 0xdf, 0xb4);
pub const KEYCAP_INK: Color32 = rgb(0x2e, 0x1d, 0x10);
pub const KEYCAP_RIM: Color32 = rgb(0xb0, 0x8a, 0x4a);
/// "Won" in green on a mission card.
pub const MISSION_DONE: Color32 = rgb(0x9f, 0xdc, 0xbf);

/// The group tabs of the Companion editor (`companion_qss`'s `tabs`).
pub const GROUP_TABS: &[(&str, Color32)] = &[
    ("presetGroup", rgb(0x6b, 0x3f, 0x1f)),
    ("creaturesGroup", rgb(0x7a, 0x4a, 0x26)),
    ("teamsGroup", rgb(0x5e, 0x35, 0x22)),
    ("behaviorGroup", rgb(0x71, 0x50, 0x2b)),
    ("fliesGroup", rgb(0x8a, 0x5a, 0x22)),
    ("launchGroup", rgb(0x4f, 0x5f, 0x25)),
];

/// The tab colour of one Companion group, by its Qt object name.
pub fn group_tab_color(name: &str) -> Color32 {
    GROUP_TABS.iter().find(|(n, _)| *n == name).map(|(_, c)| *c).unwrap_or(WALNUT)
}

/// Which picture each mode card shows (`MODE_ART`).
pub const MODE_ART: &[(&str, &str)] =
    &[("companion", "mode_companion.png"), ("skirmish", "mode_adventure.png"), ("strategy", "mode_strategy.png")];

pub fn mode_art_name(kind: &str) -> Option<&'static str> {
    MODE_ART.iter().find(|(k, _)| *k == kind).map(|(_, n)| *n)
}

/// `_lift`: every channel raised by `amount`, clamped (a hovered gradient stop).
pub fn lift(color: Color32, amount: u8) -> Color32 {
    let up = |c: u8| c.saturating_add(amount);
    Color32::from_rgba_unmultiplied(up(color.r()), up(color.g()), up(color.b()), color.a())
}

/// `_lift` with the style sheets' default of 22.
pub fn lift22(color: Color32) -> Color32 {
    lift(color, 22)
}

/// Qt's `QColor::lighter(factor)` (percent), in HSV value, as Qt does it.
pub fn lighter(color: Color32, factor: u32) -> Color32 {
    let (h, s, v) = rgb_to_hsv(color);
    let mut v = v * factor as f32 / 100.0;
    let mut s = s;
    if v > 1.0 {
        // Qt moves the overflow into lower saturation.
        s = (s - (v - 1.0)).max(0.0);
        v = 1.0;
    }
    hsv_to_rgb(h, s, v, color.a())
}

/// Qt's `QColor::darker(factor)` (percent).
pub fn darker(color: Color32, factor: u32) -> Color32 {
    let (h, s, v) = rgb_to_hsv(color);
    hsv_to_rgb(h, s, v * 100.0 / factor.max(1) as f32, color.a())
}

fn rgb_to_hsv(c: Color32) -> (f32, f32, f32) {
    let (r, g, b) = (c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max == 0.0 { 0.0 } else { d / max };
    (h, s, max)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32, a: u8) -> Color32 {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to = |f: f32| ((f + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(to(r), to(g), to(b), a)
}

/// Mix two colours: `t` = 0 gives `a`, 1 gives `b`.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// The same colour with a new alpha (`QColor` with `setAlpha`).
pub fn with_alpha(c: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
}

/// `rgba(r, g, b, a)` from a style sheet.
pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied_const(r, g, b, a)
}

// -- type --------------------------------------------------------------------

/// Qt point sizes as egui points: the windows were laid out at 96 DPI, where
/// one Qt point is 4/3 of a pixel.
pub fn pt(points: f32) -> f32 {
    points * 96.0 / 72.0
}

/// The family name of the bold UI face, when it could be loaded.
pub const BOLD: &str = "segoe-bold";
/// The family name of the bold monospace face (key caps, bindings).
pub const MONO_BOLD: &str = "mono-bold";

/// The body font at a Qt point size.
pub fn font(points: f32) -> FontId {
    FontId::new(pt(points), FontFamily::Proportional)
}

/// The bold UI font at a Qt point size (`font-weight: 700..900`).
pub fn bold(points: f32) -> FontId {
    FontId::new(pt(points), FontFamily::Name(BOLD.into()))
}

/// Consolas, bold, for key caps (`font-family: Consolas, monospace; font-weight: 800`).
pub fn mono_bold(points: f32) -> FontId {
    FontId::new(pt(points), FontFamily::Name(MONO_BOLD.into()))
}

/// Where the Windows fonts live.
fn font_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(windir) = std::env::var("WINDIR") {
        dirs.push(std::path::Path::new(&windir).join("Fonts"));
    }
    dirs.push(std::path::PathBuf::from(r"C:\Windows\Fonts"));
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        dirs.push(std::path::Path::new(&local).join("Microsoft").join("Windows").join("Fonts"));
    }
    dirs.push(std::path::PathBuf::from("/usr/share/fonts/truetype/dejavu"));
    dirs
}

fn read_font(names: &[&str]) -> Option<Arc<FontData>> {
    for dir in font_dirs() {
        for name in names {
            let path = dir.join(name);
            if let Ok(bytes) = std::fs::read(&path) {
                return Some(Arc::new(FontData::from_owned(bytes)));
            }
        }
    }
    None
}

/// Which faces were found, for logging and tests.
#[derive(Clone, Debug, Default)]
pub struct FontReport {
    pub regular: bool,
    pub bold: bool,
    pub mono: bool,
    pub symbols: bool,
}

fn font_definitions() -> &'static (FontDefinitions, FontReport) {
    static FONTS: OnceLock<(FontDefinitions, FontReport)> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut defs = FontDefinitions::empty();
        let mut report = FontReport::default();
        let regular = read_font(&["segoeui.ttf", "DejaVuSans.ttf"]);
        let bold = read_font(&["segoeuib.ttf", "DejaVuSans-Bold.ttf"]);
        let mono = read_font(&["consola.ttf", "DejaVuSansMono.ttf"]);
        let mono_bold = read_font(&["consolab.ttf", "DejaVuSansMono-Bold.ttf"]);
        // Arrows, ticks, crowns and the middle dot the windows use.
        let symbols = read_font(&["seguisym.ttf"]);
        let mut add = |name: &str, data: &Option<Arc<FontData>>| {
            if let Some(data) = data {
                defs.font_data.insert(name.to_owned(), data.clone());
                true
            } else {
                false
            }
        };
        report.regular = add("segoe", &regular);
        report.bold = add("segoe-b", &bold);
        report.mono = add("consolas", &mono);
        let mono_bold_ok = add("consolas-b", &mono_bold);
        report.symbols = add("symbols", &symbols);
        let chain = |first: &[&str]| -> Vec<String> {
            let mut v: Vec<String> = Vec::new();
            for name in first.iter().chain(["segoe", "segoe-b", "consolas", "symbols"].iter()) {
                if defs.font_data.contains_key(*name) && !v.iter().any(|x| x == name) {
                    v.push((*name).to_owned());
                }
            }
            v
        };
        let proportional = chain(&["segoe"]);
        let bold_chain = chain(&["segoe-b"]);
        let mono_chain = chain(&["consolas"]);
        let mono_bold_chain = if mono_bold_ok { chain(&["consolas-b"]) } else { mono_chain.clone() };
        defs.families.insert(FontFamily::Proportional, proportional);
        defs.families.insert(FontFamily::Monospace, mono_chain);
        defs.families.insert(FontFamily::Name(BOLD.into()), bold_chain);
        defs.families.insert(FontFamily::Name(MONO_BOLD.into()), mono_bold_chain);
        (defs, report)
    })
}

/// Install Segoe UI (the face the Qt windows asked for: "text is difficult to
/// read" with Qt's fallback) and Consolas. A glyph no face has draws as a box
/// rather than panicking.
pub fn install_fonts(ctx: &egui::Context) -> FontReport {
    let (defs, report) = font_definitions();
    ctx.set_missing_glyph_policy(egui::MissingGlyphPolicy::Tofu);
    ctx.set_fonts(defs.clone());
    if !report.regular {
        log::warn!("Segoe UI not found; text falls back to whatever face loaded");
    }
    report.clone()
}

// -- visuals -----------------------------------------------------------------

fn widget(bg: Color32, stroke: Stroke, fg: Color32, radius: u8) -> egui::style::WidgetVisuals {
    egui::style::WidgetVisuals {
        bg_fill: bg,
        weak_bg_fill: bg,
        bg_stroke: stroke,
        corner_radius: CornerRadius::same(radius),
        fg_stroke: Stroke::new(1.0, fg),
        expansion: 0.0,
    }
}

/// `dialog_qss` / `app_qss`: a dark walnut board, cream labels, parchment
/// inputs with ink text, oak selection.
pub fn dialog_visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.dark_mode = true;
    v.override_text_color = None;
    v.panel_fill = WALNUT;
    v.window_fill = PARCHMENT;
    v.window_stroke = Stroke::new(2.0, WALNUT_DEEP);
    v.window_corner_radius = CornerRadius::same(6);
    v.window_shadow = Shadow { offset: [2, 4], blur: 10, spread: 0, color: rgba(10, 4, 0, 120) };
    v.popup_shadow = Shadow { offset: [2, 3], blur: 6, spread: 0, color: rgba(10, 4, 0, 90) };
    v.faint_bg_color = rgba(20, 10, 3, 110);
    v.extreme_bg_color = PARCHMENT;
    v.code_bg_color = WALNUT_DEEP;
    v.hyperlink_color = BRASS;
    v.selection.bg_fill = OAK;
    v.selection.stroke = Stroke::new(1.0, BUTTON_TEXT);
    v.widgets.noninteractive = widget(WALNUT, Stroke::new(1.0, WALNUT_DEEP), CREAM, 6);
    v.widgets.inactive = widget(PARCHMENT, Stroke::new(1.0, WALNUT_DEEP), INK, 6);
    v.widgets.hovered = widget(lift22(PARCHMENT), Stroke::new(1.0, BRASS), INK, 6);
    v.widgets.active = widget(PARCHMENT_DEEP, Stroke::new(1.0, BRASS_DEEP), INK, 6);
    v.widgets.open = widget(PARCHMENT_DEEP, Stroke::new(1.0, WALNUT_DEEP), INK, 6);
    v.text_cursor.stroke = Stroke::new(2.0, INK);
    v
}

/// The global style used by egui's floating areas (tooltips and popups).
///
/// Egui creates those areas from the context's global style rather than the
/// scoped visuals of the widget that opened them.  The dialog style has cream
/// non-interactive text for walnut panels, but floating areas use its light
/// parchment `window_fill`; keeping that combination made ordinary
/// `Response::on_hover_text` tooltips look empty.  Floating labels use ink,
/// matching the Python `QToolTip` rule, while window contents continue to opt
/// into [`dialog_visuals`] or [`companion_visuals`] in their own scope.
pub fn floating_visuals() -> Visuals {
    let mut v = dialog_visuals();
    v.widgets.noninteractive.fg_stroke.color = TOOLTIP_TEXT;
    v
}

/// `companion_qss`: parchment panels on an oak board, ink text, cream inputs.
pub fn companion_visuals() -> Visuals {
    let mut v = Visuals::light();
    v.dark_mode = false;
    v.panel_fill = OAK;
    v.window_fill = INPUT_BG;
    v.window_stroke = Stroke::new(1.0, GROUP_BORDER);
    v.window_corner_radius = CornerRadius::same(6);
    v.faint_bg_color = ROW_ALT;
    v.extreme_bg_color = INPUT_BG;
    v.code_bg_color = PARCHMENT_DEEP;
    v.hyperlink_color = WALNUT;
    v.selection.bg_fill = OAK;
    v.selection.stroke = Stroke::new(1.0, BUTTON_TEXT);
    v.widgets.noninteractive = widget(PARCHMENT, Stroke::new(1.0, GROUP_BORDER), INK, 6);
    v.widgets.inactive = widget(INPUT_BG, Stroke::new(1.0, INPUT_BORDER), INK, 6);
    v.widgets.hovered = widget(INPUT_BG, Stroke::new(1.0, WALNUT), INK, 6);
    v.widgets.active = widget(PARCHMENT_DEEP, Stroke::new(1.0, WALNUT), INK, 6);
    v.widgets.open = widget(INPUT_BG, Stroke::new(1.0, WALNUT), INK, 6);
    v.text_cursor.stroke = Stroke::new(2.0, INK);
    v
}

/// `shell_qss`: the mode menu, cream text on oak, walnut cards.
pub fn shell_visuals() -> Visuals {
    let mut v = dialog_visuals();
    v.panel_fill = OAK;
    v
}

/// Run `add` with `visuals` for that part of the window only.
pub fn with_visuals<R>(ui: &mut egui::Ui, visuals: Visuals, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        *ui.visuals_mut() = visuals;
        add(ui)
    })
    .inner
}

/// `apply_app_theme`: the fonts, the walnut look for dialogs and menus, and
/// the spacing the Qt windows used.
pub fn apply_app_theme(ctx: &egui::Context) -> FontReport {
    let report = install_fonts(ctx);
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        style.visuals = floating_visuals();
        style.spacing.item_spacing = egui::vec2(6.0, 5.0);
        style.spacing.button_padding = egui::vec2(12.0, 5.0);
        style.spacing.interact_size = egui::vec2(36.0, 24.0);
        style.spacing.scroll.bar_width = 12.0;
        style.spacing.combo_width = 120.0;
        style.text_styles.insert(egui::TextStyle::Body, font(9.0));
        style.text_styles.insert(egui::TextStyle::Button, font(9.0));
        style.text_styles.insert(egui::TextStyle::Small, font(8.0));
        style.text_styles.insert(egui::TextStyle::Heading, bold(16.0));
        style.text_styles.insert(egui::TextStyle::Monospace, FontId::new(pt(9.0), FontFamily::Monospace));
    });
    report
}

// -- frames --------------------------------------------------------------------

/// `QDialog`'s walnut board (`wood_dark_tile.png` when present).
pub fn dialog_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(WALNUT)
        .stroke(Stroke::new(2.0, WALNUT_DEEP))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin { left: 22, right: 22, top: 18, bottom: 20 })
}

/// A Companion `QGroupBox`: parchment, a 2px brown border, rounded 10.
pub fn parchment_group() -> egui::Frame {
    egui::Frame::new()
        .fill(PARCHMENT)
        .stroke(Stroke::new(2.0, GROUP_BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin { left: 8, right: 8, top: 10, bottom: 6 })
}

/// `QLabel#summaryLabel, #statusLabel`: deep parchment note.
pub fn status_note() -> egui::Frame {
    egui::Frame::new()
        .fill(PARCHMENT_DEEP)
        .stroke(Stroke::new(1.0, INPUT_BORDER))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(8))
}

/// `QFrame#pageHeader`: a walnut bar across the top of a mode page.
pub fn page_header() -> egui::Frame {
    egui::Frame::new()
        .fill(WALNUT)
        .stroke(Stroke::new(2.0, WALNUT_DEEP))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin { left: 10, right: 14, top: 6, bottom: 6 })
}

/// `QFrame#missionCard`; `locked` uses the faded, dashed variant (the dash is
/// painted by [`crate::paint::dashed_rounded_rect`]).
pub fn mission_card(locked: bool) -> egui::Frame {
    egui::Frame::new()
        .fill(if locked { rgba(42, 24, 12, 130) } else { rgba(42, 24, 12, 215) })
        .stroke(if locked { Stroke::NONE } else { Stroke::new(2.0, BRASS_DEEP) })
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin { left: 12, right: 12, top: 10, bottom: 10 })
}

/// `QFrame#heroStrip`.
pub fn hero_strip() -> egui::Frame {
    egui::Frame::new()
        .fill(rgba(42, 24, 12, 215))
        .stroke(Stroke::new(2.0, BRASS))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin { left: 14, right: 10, top: 8, bottom: 8 })
}

/// `QFrame#setupCard` of the raid setup.
pub fn setup_card() -> egui::Frame {
    egui::Frame::new()
        .fill(rgba(42, 24, 12, 220))
        .stroke(Stroke::new(2.0, BRASS_DEEP))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin { left: 20, right: 20, top: 14, bottom: 16 })
}

/// `QFrame#sheetPanel` (character, inspector).
pub fn sheet_panel() -> egui::Frame {
    egui::Frame::new()
        .fill(rgba(20, 12, 6, 150))
        .stroke(Stroke::new(1.0, BRASS_DEEP))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin { left: 14, right: 14, top: 10, bottom: 14 })
}

/// `QFrame#itemPanel`.
pub fn item_panel() -> egui::Frame {
    egui::Frame::new()
        .fill(rgba(63, 38, 22, 210))
        .stroke(Stroke::new(1.0, BRASS_DEEP))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin { left: 10, right: 10, top: 6, bottom: 6 })
}

/// `QScrollArea#inventoryBag`.
pub fn bag_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(rgba(20, 12, 6, 200))
        .stroke(Stroke::new(1.0, INK_SOFT))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(8))
}

/// `QScrollArea#bindingsScroll` of the controls editor.
pub fn bindings_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(rgba(28, 16, 8, 215))
        .stroke(Stroke::new(1.0, BRASS_DEEP))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(8))
}

/// `QFrame#statTile`.
pub fn stat_tile() -> egui::Frame {
    egui::Frame::new()
        .fill(rgba(63, 38, 22, 200))
        .stroke(Stroke::new(1.0, BRASS_DEEP))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin { left: 10, right: 12, top: 8, bottom: 8 })
}

/// `QTabWidget::pane` of the inspector and character windows.
pub fn tab_pane() -> egui::Frame {
    egui::Frame::new()
        .fill(rgba(20, 10, 3, 110))
        .stroke(Stroke::new(2.0, WALNUT_DEEP))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(8))
}

/// `QToolTip`: parchment, ink, a walnut rim.
pub fn tooltip_frame() -> egui::Frame {
    egui::Frame::new().fill(PARCHMENT).stroke(Stroke::new(1.0, WALNUT_DEEP)).inner_margin(Margin::same(4))
}

/// The text colour of a tooltip, which sits on parchment in every window.
pub const TOOLTIP_TEXT: Color32 = INK;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_matches_the_style_sheets() {
        assert_eq!(hex("#f5c518"), STAMINA);
        assert_eq!(hex("#3f2616"), WALNUT);
        assert_eq!(hex("d6a448"), BRASS);
    }

    #[test]
    fn lift_clamps_like_the_python() {
        assert_eq!(lift22(hex("#c99a5e")), hex("#dfb074"));
        assert_eq!(lift(hex("#f0f0f0"), 22), hex("#ffffff"));
    }

    #[test]
    fn qt_lighter_and_darker() {
        let base = hex("#808080");
        assert_eq!(darker(base, 200), hex("#404040"));
        let light = lighter(base, 150);
        assert_eq!(light, hex("#c0c0c0"));
    }

    #[test]
    fn group_tabs_have_their_colours() {
        assert_eq!(group_tab_color("launchGroup"), hex("#4f5f25"));
        assert_eq!(group_tab_color("nope"), WALNUT);
    }

    #[test]
    fn floating_tooltips_use_parchment_and_ink() {
        let ctx = egui::Context::default();
        apply_app_theme(&ctx);
        let mut seen = None;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let response = ui.label("tooltip owner");
            response.show_tooltip_ui(|ui| {
                let frame = egui::Frame::popup(ui.style());
                seen = Some((frame.fill, frame.stroke.color, ui.visuals().text_color()));
            });
        });
        output.textures_delta.clear();

        assert_eq!(seen, Some((PARCHMENT, WALNUT_DEEP, TOOLTIP_TEXT)));
    }
}
