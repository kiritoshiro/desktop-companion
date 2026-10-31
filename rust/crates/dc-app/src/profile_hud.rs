//! The profiling HUD (`_hud_rect`, `_hud_header`, `_draw_profile_hud` in
//! `app/engine.py`). A debugging aid, not a feature: it only exists while
//! `DESKTOP_BUG_PROFILE` is set (and `DESKTOP_BUG_PROFILE_HUD` is not off),
//! and it is what makes a claim about frame cost checkable while you watch
//! the overlay rather than only in a benchmark. The numbers come from
//! `dc_core::profiling`.

use dc_core::Rect;
use dc_render::{Canvas, Color, Font};

pub const HUD_ORIGIN: (f64, f64) = (24.0, 24.0);
pub const HUD_LINE_HEIGHT: f64 = 15.0;
pub const HUD_WIDTH: f64 = 260.0;
/// Room for the header line plus every section the HUD will show.
pub const HUD_MAX_LINES: usize = 9;

/// `_hud_rect`: the panel, in overlay coordinates. The panel changes every
/// frame, so a partial repaint has to expose all of it every frame.
pub fn hud_rect() -> Rect {
    let (left, top) = HUD_ORIGIN;
    Rect::new(left - 8.0, top - 8.0, HUD_WIDTH, HUD_LINE_HEIGHT * HUD_MAX_LINES as f64 + 14.0)
}

/// `_hud_header`: the spider count, the running frame rate and why.
pub fn hud_header(spiders: usize, fps: f64, reason: &str) -> String {
    format!("{spiders} spiders @ {fps:.0} FPS ({reason})")
}

/// `_draw_profile_hud`: nothing until the profiler has numbers.
pub fn draw_profile_hud(canvas: &mut Canvas, header: &str, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    let (left, top) = HUD_ORIGIN;
    canvas.save();
    canvas.set_clipping(false);
    canvas.fill_rect(hud_rect(), Color::rgba(0, 0, 0, 170));
    canvas.set_font(Font::new("Consolas", 8.5));
    canvas.set_pen(Color::rgb(210, 255, 210));
    for (i, line) in std::iter::once(header).chain(lines.iter().map(String::as_str)).enumerate() {
        canvas.draw_text(left, top + 4.0 + HUD_LINE_HEIGHT * (i + 1) as f64, line);
    }
    canvas.restore();
}

#[cfg(test)]
mod tests {
    use super::*;
    use dc_render::{CpuRenderer, Scene};

    #[test]
    fn rect_and_header_match_python() {
        let r = hud_rect();
        assert_eq!((r.x, r.y, r.w, r.h), (16.0, 16.0, 260.0, 149.0));
        assert_eq!(hud_header(5, 59.6, "target"), "5 spiders @ 60 FPS (target)");
        assert_eq!(hud_header(1, 15.0, "fullscreen app in front"), "1 spiders @ 15 FPS (fullscreen app in front)");
    }

    fn draw(lines: &[String]) -> dc_render::Image {
        let mut scene = Scene::new();
        draw_profile_hud(&mut scene.canvas(), "4 spiders @ 60 FPS (target)", lines);
        CpuRenderer::new().render(&scene, 400, 300, None)
    }

    #[test]
    fn the_panel_is_drawn_and_nothing_else() {
        let image = draw(&["frame           1.00 ms  p95  1.00".to_string()]);
        let r = hud_rect();
        let (mut seen, mut solid) = (0, 0);
        for y in (r.y as u32 + 2..(r.y + r.h) as u32 - 2).step_by(3) {
            for x in (r.x as u32 + 2..(r.x + r.w) as u32 - 2).step_by(3) {
                seen += 1;
                if image.pixel(x, y).a > 100 {
                    solid += 1;
                }
            }
        }
        assert!(seen > 100 && solid == seen, "the HUD panel was not drawn: {solid} of {seen} samples were opaque");
        // The overlay stays transparent away from the panel.
        for (x, y) in [(300, 250), (399, 299), (290, 20), (20, 200)] {
            assert_eq!(image.pixel(x, y).a, 0, "painted outside the HUD at {x},{y}");
        }
    }

    #[test]
    fn no_numbers_no_panel() {
        let image = draw(&[]);
        assert_eq!(image.pixel(30, 30).a, 0);
    }
}
