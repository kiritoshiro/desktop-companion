# dc-art port status

Mission and menu art for the Rust port, painting onto `dc-render`.
Depends on `dc-core`, `dc-content`, `dc-render` (and `serde_json` for tests only).

```
CARGO_TARGET_DIR=target-dc-art cargo test -p dc-art
<venv>/python.exe -I rust/crates/dc-art/tools/qt_reference.py      # Qt reference renders (local only)
CARGO_TARGET_DIR=target-dc-art cargo test -p dc-art --release --test qt_parity -- --nocapture
CARGO_TARGET_DIR=target-dc-art cargo run --release -p dc-art --example bench_reclaim
CARGO_TARGET_DIR=target-dc-art cargo run --release -p dc-art --example generate_ui_art [out_dir]
```

## Modules

| Python | Rust | Status |
|---|---|---|
| `app/mission_art.py` | `mission_art` | ported: PNG sprites from `assets/mission_buildings/` composed into the 2x 240x190 frame, ground anchor (120, 139), captured variants, cache |
| `app/mission_backdrop.py` | `mission_backdrop` | ported: all 7 looks, `paint_backdrop` at DETAIL 0.5 then smooth-scaled, `PyRandom` with the same string seed and draw order, `MissionBackdrop` cache keyed by (map, screen, size, sites) |
| `app/map_art.py` | `map_art` | ported: `map_picture(map_id, kind, locked)`, seeded `card-{map_id}`, cache |
| `app/skill_art.py` | `skill_art` | ported: medallions per state, branch crests, stat icons |
| `app/mission_ui.py` + HUD half of `app/adventure_ui.py` + engine banners | `mission_ui` | ported: buildings, screen links, hazards, loot, minimap, Reclaim intro card, `draw_desktop_health` with flash, `draw_blue_screen`, mission banner and companion commands, end titles; `hud_rect`, `_board`, `_well`, `short_binding`, `draw_hud` (P1/P2, FALLEN, key cards); `_draw_released_banner`, the take-control hint, aim markers |
| `app/desktop_surface.py` | `desktop_surface` | ported: panes, stress, breaks, falls, `_reveal_behind`, void/wallpaper layers, acid, `desktop_hp`, `break_desktop`, `WORD_CELL` buckets, `words_within` / `word_near`, `CRACK_REACH`, region-sized scratch; plus a Qt-exact `synthetic_snapshot` |
| `app/screen_text.py` | `screen_text` | ported: same boxes as Python (fixture test) |
| `tools/generate_ui_art.py` | `ui_art` + `examples/generate_ui_art.rs` | partial: wood tiles, card frame, all 29 `menu_*.png` emblems. **Not ported:** `mode_*.png` and `app_icon.*` (they burn the live procedural tarantula in; need dc-creature/dc-paint) |

Not here on purpose: `draw_loot`'s armour picture comes from `dc-paint`'s
`armour_icon` (pass it in `UiLoot::icon`). The map-view "banner" is a tray
toast in the Python (`_announce`); the map view itself is `MissionBackdrop::paint`.

## API for the engine, the mission and Reclaim

### Reclaim (`dc-adventure`'s port of `reclaim.py`)

```rust
use dc_art::desktop_surface::*;
// From dc-platform's capture: Image::from_straight_rgba(w, h, pixels) + set_device_pixel_ratio.
let snap = SurfaceSnapshot { screens: vec![SurfaceShot { rect, image, wallpaper }], windows, primary, window_images };
let mut s = DesktopSurface::new(snap, Some(seed), /*find_text*/ true);
s.melt(x, y, radius)        -> Option<Hit>       // Hit::Window | Hit::Desktop; None = only void left
s.crack(x, y, force)        -> Option<Hit>       // Window: that window starts breaking; Desktop: hp ran out
s.break_desktop(Some((x, y)));
s.update(dt);                                    // flash, racing cracks, shards
s.busy() -> bool
s.what_is_at(x, y) -> Seen                       // Window | Desktop | Void
s.desktop_hp() -> f64;  s.integrity() -> f64;  s.desktop_broken;  s.hit_flash
s.intact_panes();  s.pane_at(x, y);  s.windows   // unbroken, front to back
s.words: Vec<Word>                               // rect, colour, screen, eaten; refer to words by index
s.living_words() -> Vec<usize>
s.words_within(x, y, reach) -> Vec<usize>;  s.word_near(x, y, reach) -> Option<usize>
s.eat(word_index, amount) -> bool                // true when the word is gone
s.paint(&mut canvas);  s.version                 // version bumps whenever a frame image changes
s.hud() -> SurfaceHud                            // for draw_desktop_health
s.fresh(seed)                                    // Retry: a new surface from the same snapshot
```

`synthetic_snapshot(rects, windows, primary, text_rows)` builds the test
desktop (Qt's fill rule exactly, so word finding matches the Python).

### Mission and engine overlays (`mission_ui`)

Fill plain structs each frame and call the painters in the engine's order:

* `HudWindow { width, height, primary, hud_position, players }`
* `HudPlayer` (spider + controller fields, raw `bindings` for jump/shoot/bite/skills)
* `MissionView` (area, `UiLayout` screens/links, `here`, `UiSite`s, `UiActor`s,
  `UiLoot`, `UiGlob`/`UiSplash`/`UiThread`, `UiCreature`s for the minimap,
  objective/notice/state/end fields, allies, counters, `surface: Option<SurfaceHud>`,
  intro/blue-screen timers, `command_bindings`)

Painters: `draw_buildings`, `draw_hud(p, window, player, index)`,
`draw_adventure_hint`, `draw_player_aims`, `draw_cursor_aim`,
`draw_mission_hud` (which calls `draw_minimap`, `draw_reclaim_extras`,
`draw_blue_screen` or `draw_end_title`), `draw_released_banner`.
Hit testing: `hud_rect`, `command_rects`, `mission_banner_rect`,
`minimap_rect`, `adventure_hint_rect`. Text: `site_status`,
`end_title_text`, `short_binding`.

### Backdrop, cards and icons

`MissionBackdrop::new().paint(canvas, map_id, kind, &screens, &sites)`,
`paint_backdrop(look, w, h, &sites, seed)`, `map_picture(id, kind, locked)`,
`building_art(kind, owned)`, `skill_icon(id, SkillLook, size)`,
`branch_crest(branch, size)`, `stat_icon(kind, size)`,
`ui_art::menu_emblem(name)`, `ui_art::generate_all()`.

## Verification

* 41 unit tests + `tests/word_parity.rs` (2) + `tests/qt_parity.rs` (1, skips
  without local Qt renders). Ported: the surface tests of
  `test_screens_and_reclaim` (acid, cracks, window break, desktop hp, words,
  fast word lookup), backdrop tests of `test_map_view_and_release`, map
  picture / building tests of `test_ingame_polish` and `test_mission_art`.
* **Word finder:** identical boxes to Python on 4 synthetic desktops (incl.
  an offset second screen, a crop, long rows split by `MAX_W`), and exactly
  the same word list and background colours on the seeded Reclaim desktop (`tests/fixtures/word_boxes.json`, 20 KB).
* **Qt parity** (124 scenes, mean channel difference 0..255 / worst SSIM):
  backdrops 0.88 / 0.996, maps 0.52 / 0.998, skills 0.94 / 0.990,
  buildings 0.30 / 1.000, HUDs 0.53 / 0.962, Reclaim desktop 0.19 / 0.999
  (same RNG stream end to end), emblems 1.14 / 0.986. Sheets in
  `target-review/sheets/` (gitignored).

## Performance (owner layout 2560x1440 + 3840x2160, release, CPU)

* Backdrop: 53–116 ms (1440p) and 115–255 ms (4K) per look on a quiet
  machine; both screens first time ~440 ms, cached 0.005 ms. Painted once.
* Surface: new 177 ms with word finding (1,400 words), 160 ms without;
  melt 0.6 ms; crack 4–5 ms; a pair of word queries 3 µs; update while a
  window breaks 0.5 ms mean / p95 3 ms; while the whole desktop breaks
  1.4 ms mean / p95 9 ms; worst single frame ~80 ms (the moment both
  screens' glass falls away at once, as in the Python).

## Rust differences and workarounds

* Words are indices into `DesktopSurface::words` (Python compared objects).
* `_compose`, the region-clipped copies and `eat`'s SourceAtop fill are
  integer pixel loops with Qt's source-over arithmetic, clipped by pixel
  centre like Qt's aliased rect clips. A break's new web lines are drawn
  straight onto the glass inside the region, and only each line's own box
  is recomposed (Python composed the box round all of them; same pixels,
  p95 51 ms → 9 ms).
* `screen_text` on a HiDPI image scales the grey picture to its logical
  size (the Qt code kept the device pixel ratio and covered a quarter of the
  area). Parity is checked at ratio 1.
* **dc-render font gaps (worked around here, dc-render unchanged):**
  - Georgia is not in dc-render's font table, so the `menu_help` "?" fell
    back to Segoe UI; `ui_art` registers `%WINDIR%\Fonts\georgiab.ttf`
    through `dc_render::text::register_font`.
  - Segoe UI metrics at small pixel sizes differ from Qt's GDI values by
    1–2 px at scattered sizes (10 px: Qt line spacing 12, dc-render 14).
    The blue screen's wrapped paragraphs are laid out line by line with a
    table of Qt's measured line spacing for 6–60 px
    (`mission_ui::QT_SEGOE_LINE_SPACING`). Ascent still differs by a pixel
    at some sizes; worth fixing in dc-render's metrics.
* Qt references must be rendered with the native `windows` platform:
  under `offscreen` Qt finds no fonts on Windows (all text is boxes).
