//! Port of app/mission_art.py: raster artwork for the Adventure mission's
//! woodland buildings.
//!
//! Transparent PNG sprites (`assets/mission_buildings/`) keep the detailed
//! game-art look; each kind and captured state is composed once into the old
//! 2x frame (240 x 190 logical, ground anchor (120, 139)) and cached.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use dc_core::Rect;
use dc_render::{Image, RenderHint};

pub const ART_W: u32 = 240;
pub const ART_H: u32 = 190;
pub const GROUND_Y: f64 = 139.0;
pub const SCALE: u32 = 2;
pub const SPRITE_H: f64 = 137.0;
pub const SPRITE_MAX_W: f64 = 218.0;

pub const BUILDING_KINDS: [&str; 12] = [
    "home",
    "food",
    "silk",
    "hatchery",
    "nest",
    "outpost",
    "infestation",
    "venom",
    "lookout",
    "amber",
    "nursery",
    "flynest",
];

/// The sprite a captured site switches to, where it has one.
fn captured_asset(kind: &str, owned: bool) -> Option<&'static str> {
    match (kind, owned) {
        ("hatchery", true) => Some("hatchery_claimed"),
        ("nest", true) => Some("nest_claimed"),
        ("infestation", true) => Some("infestation_destroyed"),
        _ => None,
    }
}

fn asset_cache() -> &'static Mutex<HashMap<String, Image>> {
    static C: OnceLock<Mutex<HashMap<String, Image>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

fn art_cache() -> &'static Mutex<HashMap<(String, bool), Image>> {
    static C: OnceLock<Mutex<HashMap<(String, bool), Image>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

/// `_load_asset`: one PNG from `assets/mission_buildings/`, cached.
pub fn load_asset(name: &str) -> Result<Image, String> {
    if let Some(img) = asset_cache().lock().unwrap().get(name) {
        return Ok(img.clone());
    }
    let file = format!("{name}.png");
    let path = dc_content::discovery::find_data_file(&["assets", "mission_buildings", &file], None);
    let image = Image::load_png(&path)
        .map_err(|e| format!("Mission building art could not be loaded: {}: {e}", path.display()))?;
    if image.is_null() {
        return Err(format!("Mission building art could not be loaded: {}", path.display()));
    }
    asset_cache().lock().unwrap().insert(name.to_string(), image.clone());
    Ok(image)
}

/// The 2x transparent sprite for a building and its captured state
/// (480 x 380 pixels, device pixel ratio 2). Cached: the same kind and state
/// returns the same image (equal `id()`). Unknown kinds fall back to the
/// hatchery. A missing PNG gives an empty transparent frame (logged).
pub fn building_art(kind: &str, owned: bool) -> Image {
    let kind = if BUILDING_KINDS.contains(&kind) { kind } else { "hatchery" };
    let key = (kind.to_string(), owned);
    if let Some(img) = art_cache().lock().unwrap().get(&key) {
        return img.clone();
    }
    let asset_name = captured_asset(kind, owned).unwrap_or(kind);
    let mut image = Image::new(ART_W * SCALE, ART_H * SCALE);
    match load_asset(asset_name) {
        Ok(source) => {
            let (sw, sh) = (source.width() as f64, source.height() as f64);
            let mut width = SPRITE_H * sw / sh;
            let mut height = SPRITE_H;
            if width > SPRITE_MAX_W {
                width = SPRITE_MAX_W;
                height = width * sh / sw;
            }
            image.paint(|p| {
                p.set_render_hint(RenderHint::Antialiasing, true);
                p.set_render_hint(RenderHint::SmoothPixmapTransform, true);
                p.scale(SCALE as f64, SCALE as f64);
                let rect = Rect::new((ART_W as f64 - width) / 2.0, GROUND_Y - height, width, height);
                p.draw_image_rect(rect, &source);
            });
        }
        Err(e) => log::warn!("{e}"),
    }
    image.set_device_pixel_ratio(SCALE as f32);
    art_cache().lock().unwrap().entry(key).or_insert(image).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opaque(image: &Image) -> usize {
        let mut n = 0;
        for y in (0..image.height()).step_by(3) {
            for x in (0..image.width()).step_by(3) {
                n += (image.pixel(x, y).a > 40) as usize;
            }
        }
        n
    }

    fn differs(a: &Image, b: &Image) -> usize {
        let mut n = 0;
        for y in (0..a.height()).step_by(3) {
            for x in (0..a.width()).step_by(3) {
                n += (a.pixel_premultiplied(x, y) != b.pixel_premultiplied(x, y)) as usize;
            }
        }
        n
    }

    fn last_opaque_row(art: &Image) -> Option<u32> {
        (0..art.height()).rev().find(|&y| (0..art.width()).any(|x| art.pixel(x, y).a > 8))
    }

    #[test]
    fn every_building_loads_its_own_cached_transparent_png() {
        for kind in BUILDING_KINDS {
            let art = building_art(kind, false);
            assert!(!art.is_null());
            assert!(opaque(&art) > 1000, "{kind} should fill a useful part of its frame");
            assert_eq!(building_art(kind, false).id(), art.id(), "composed art is cached");
            assert_eq!(art.pixel(0, 0).a, 0);
            assert_eq!(art.pixel(ART_W * SCALE - 1, ART_H * SCALE - 1).a, 0);
        }
    }

    #[test]
    fn building_sprites_share_the_ground_anchor_and_hidpi_canvas() {
        for kind in BUILDING_KINDS {
            let art = building_art(kind, false);
            assert_eq!(art.device_pixel_ratio(), SCALE as f32);
            assert_eq!((art.width(), art.height()), (ART_W * SCALE, ART_H * SCALE));
            let last = last_opaque_row(&art).expect(kind);
            assert!(((last + 1) as f64 - GROUND_Y * SCALE as f64).abs() <= 2.0, "{kind} {last}");
        }
    }

    #[test]
    fn only_sites_with_capture_art_change_their_sprite_state() {
        for kind in BUILDING_KINDS {
            let changed = differs(&building_art(kind, false), &building_art(kind, true));
            if ["hatchery", "nest", "infestation"].contains(&kind) {
                assert!(changed > 100, "{kind} {changed}");
            } else {
                assert_eq!(changed, 0, "{kind}");
            }
        }
    }

    #[test]
    fn buildings_differ_from_one_another() {
        for (i, a) in BUILDING_KINDS.iter().enumerate() {
            for b in &BUILDING_KINDS[i + 1..] {
                let d = differs(&building_art(a, false), &building_art(b, false));
                assert!(d > 800, "{a} {b} {d}");
            }
        }
    }

    #[test]
    fn unknown_building_kind_keeps_the_existing_hatchery_fallback() {
        assert_eq!(differs(&building_art("unknown", false), &building_art("hatchery", false)), 0);
        let a = building_art("silk", true);
        assert_eq!(building_art("silk", true).id(), a.id());
    }
}
