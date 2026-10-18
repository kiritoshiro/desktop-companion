//! `CreatureManager.render` as a function: layer order, the clip skip and
//! the occluding-window region clip.

use dc_paint::scene::{CreatureEntry, SceneDraw, render_scene, surface_visible_region};
use dc_paint::world::{BaseDraw, CageDraw, FlyWorldDraw, MouseWebDraw};
use dc_paint::{CreatureDraw, CreatureLook, CreaturePainter, PaintSwitches, Worn};
use dc_render::{Color, CpuRenderer, Image, RenderHint, Scene};
use serde_json::Value;

/// The tarantula of the reference fixture, bare or wearing the stress
/// case's armour on every slot.
fn tarantula(armour: bool) -> (CreatureDraw, CreatureLook) {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/reference/rest__tarantula.json");
    let case: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    let mut draw: CreatureDraw = serde_json::from_value(case["draw"].clone()).unwrap();
    draw.worn = if armour {
        Worn {
            abdomen: Some("warden_abdomen".into()),
            carapace: Some("sun_carapace".into()),
            head: Some("brood_helm".into()),
            legs: Some("frost_greaves".into()),
            pedipalps: Some("pedipalp_cuffs".into()),
        }
    } else {
        Worn::default()
    };
    (draw, CreatureLook::from_model_json(&case["model"], true))
}

fn frame(
    armour: bool,
    hidden: Vec<dc_render::Region>,
    fully_covered: bool,
    clip: Option<(f64, f64, f64, f64)>,
) -> Image {
    let (draw, look) = tarantula(armour);
    let cages = [CageDraw { x: 700.0, y: 40.0, w: 200.0, h: 120.0, ..Default::default() }];
    let bases = [BaseDraw { id: "b".into(), x: 200.0, y: 200.0, build_progress: 80.0, level: 1, ..Default::default() }];
    let (mouse, flies) = (MouseWebDraw::default(), FlyWorldDraw::default());
    let creatures = [CreatureEntry { draw: &draw, look: &look, fully_covered, hidden_regions: hidden }];
    let s = SceneDraw {
        screen_w: 960.0,
        screen_h: 640.0,
        always_show_names: false,
        clip,
        cages: &cages,
        bases: &bases,
        webs: &[],
        mouse_webs: &mouse,
        flies: &flies,
        carcasses: &[],
        creatures: &creatures,
    };
    let mut scene = Scene::new();
    {
        let mut c = scene.canvas();
        c.set_render_hint(RenderHint::Antialiasing, true);
        render_scene(&mut c, &mut CreaturePainter::new(PaintSwitches::default()), &s);
    }
    CpuRenderer::new().render(&scene, 960, 640, Some(Color::TRANSPARENT))
}

fn alpha(img: &Image, x0: u32, y0: u32, x1: u32, y1: u32) -> u64 {
    let mut a = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            a += img.pixel_premultiplied(x, y)[3] as u64;
        }
    }
    a
}

#[test]
fn creature_is_drawn_over_the_props_and_clipped_by_occluders() {
    let (draw, _) = tarantula(false);
    let (cx, cy) = (draw.x as u32, draw.y as u32);
    let full = frame(false, Vec::new(), false, None);
    let body = alpha(&full, cx - 6, cy - 6, cx + 6, cy + 6);
    assert!(body > 0, "the spider is drawn");
    assert!(alpha(&full, 190, 190, 210, 210) > 0, "the base is drawn");
    assert!(alpha(&full, 695, 35, 705, 45) > 0, "the cage is drawn");

    // A window over the left half of the spider, with nothing above it.
    let hidden = || vec![surface_visible_region((0.0, 0.0, draw.x, 640.0), &[])];
    let clipped = frame(false, hidden(), false, None);
    let edge = draw.x.ceil() as u32;
    // Everything of the spider left of the window edge (the props lie further left).
    assert_eq!(alpha(&clipped, cx - 70, cy - 70, edge, cy + 70), 0, "covered half is clipped");
    assert!(alpha(&clipped, cx + 1, cy - 6, cx + 12, cy + 6) > 0, "uncovered half is drawn");

    let covered = frame(false, Vec::new(), true, None);
    assert_eq!(alpha(&covered, cx - 6, cy - 6, cx + 6, cy + 6), 0, "fully covered spiders are skipped");
    let outside = frame(false, Vec::new(), false, Some((0.0, 0.0, 10.0, 10.0)));
    assert_eq!(alpha(&outside, cx - 6, cy - 6, cx + 6, cy + 6), 0, "spiders outside the clip are skipped");
}

/// Plate armour on the abdomen (`armour_art.paint_abdomen`) calls
/// `setClipPath(outline)` with Qt's default `ReplaceClip` and then
/// `setClipping(False)`, so inside its `save()`/`restore()` the occluder clip
/// set by `CreatureManager.render` is gone and the plates draw over the
/// covering window. Qt does exactly this: the same frame rendered with PyQt5
/// leaves alpha 6853 in the probe below (bare spider: 0). The port keeps it.
#[test]
fn plate_abdomen_armour_escapes_the_occluder_clip_as_in_qt() {
    let (draw, _) = tarantula(true);
    let (cx, cy) = (draw.x as u32, draw.y as u32);
    let hidden = vec![surface_visible_region((0.0, 0.0, draw.x, 640.0), &[])];
    let clipped = frame(true, hidden, false, None);
    let leaked = alpha(&clipped, cx - 12, cy - 6, cx - 1, cy + 6);
    assert!((5500..8200).contains(&leaked), "plate armour leaks like Qt's 6853, got {leaked}");
}
