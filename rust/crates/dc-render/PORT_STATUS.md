# dc-render port status

`dc-render` replaces QPainter / QPainterPath / QPen / QBrush / gradients /
QTransform / QImage / QPixmap / QFont / QFontMetrics / QRegion. It ports no
single Python module; the painter modules themselves belong to `dc-paint`.
Status: **ported** for everything the Python tree uses, with the exceptions
listed under "Not covered".

## Survey of the Python tree (`src/desktop_bug`, 44.5k lines)

Counts are grep hits. Files with the most painter calls: `app/config_ui.py`,
`app/mission_ui.py`, `app/engine.py`, `creature/armour_art.py`,
`app/character_ui.py`, `app/armour_ui.py`, `app/inspector_ui.py`,
`creature/render_procedural.py`, `app/mission_backdrop.py`, `app/skill_art.py`,
`app/map_editor.py`, `app/desktop_surface.py`, `app/adventure_ui.py`,
`world/flies.py`, `app/map_art.py`, `creature/core.py`, `creature/regal.py`,
`world/mouse_webs.py`, `creature/render_sprite.py`, `world/cage.py`,
`world/jobs.py`, `creature/render_batch.py`, `world/webs.py`,
`creature/web_net.py`, `app/desktop_capture.py`, `creature/damage_numbers.py`,
`world/carcass.py`, `app/mission_art.py`, `app/screen_text.py`.

| Qt feature | Uses | dc-render | Status |
|---|---|---|---|
| `drawEllipse` (rect and centre forms) | 162 | `draw_ellipse`, `draw_ellipse_at`, `draw_ellipse_p` | ported (fast path on GPU) |
| `drawLine` | 103 | `draw_line`, `draw_line_p` | ported |
| `drawLines` | 3 | `draw_lines` (one stroke, like Qt) | ported |
| `drawPath` | 59 | `draw_path` (fill with path rule, then stroke) | ported |
| `drawRoundedRect` (absolute) | 42 | `draw_rounded_rect` (+ `_relative`) | ported |
| `drawText` (point and rect + flags) | 42 | `draw_text`, `draw_text_in` (clips overflow unless `TEXT_DONT_CLIP`) | ported |
| `fillRect` | 28 | `fill_rect` | ported |
| `drawImage` | 20 | `draw_image`, `draw_image_rect`, `draw_image_at`, `draw_image_at_src` (DPR aware) | ported |
| `drawRect` | 13 | `draw_rect` | ported |
| `drawPoint` / `drawPoints` | 7 / 2 | `draw_point(s)`: round dot, square, or one device pixel for ≤1 px pens (render_batch relies on this) | ported |
| `drawPixmap` | 6 | `draw_pixmap`, `draw_pixmap_at` | ported |
| `drawPolyline` / `drawPolygon` | 3 / 2 | `draw_polyline`, `draw_polygon`(odd-even), `draw_polygon_rule`, `draw_convex_polygon` | ported |
| `drawArc` / `drawChord` | 1 / 1 | `draw_arc` (1/16°), `draw_arc_deg`, `draw_chord`, `draw_pie` | ported |
| `setPen` / `QPen` | 315 / 180 | `Pen` (`new`, `full` = 5-arg ctor, builders, setters), `Pen::NONE`, `From<Color>` | ported |
| Cap / join styles (`RoundCap` 72, `RoundJoin` 21, `FlatCap` 4) | — | `Cap::{Flat,Square,Round}`, `Join::{Miter,Bevel,Round,SvgMiter}`, miter limit | ported |
| `Qt.DashLine` (7), `setDashPattern` (1) | — | `PenStyle::*`, `DashPattern`, dash offset (dashed at record time, same on both backends) | ported |
| Cosmetic pens / width 0 | — | `Pen::cosmetic`, width 0 = 1 device px | ported |
| `setBrush` / `QBrush` | 242 / 97 | `Brush::{None,Solid,Linear,Radial,Conical}` | ported |
| `QLinearGradient` | 28 | `LinearGradient` | ported |
| `QRadialGradient` (centre, radius; no focal use found) | 32 | `RadialGradient` incl. focal point and focal radius | ported |
| `QConicalGradient` | 0 | `ConicalGradient` | ported (unused by app) |
| `setColorAt` | 107 | `set_color_at` / `with_stop` (Qt sort/replace semantics, ≤ 12 stops inline) | ported |
| Spread modes | 0 | `Spread::{Pad,Reflect,Repeat}` | ported |
| Premultiplied gradient interpolation | (implicit) | GPU exact; CPU emulates by sub-stopping alpha-changing intervals | ported |
| `save` / `restore` | 58 / 52 | `save` / `restore` | ported |
| `translate` / `rotate` / `scale` | 57 / 37 / 19 | same names (degrees, local order as Qt) | ported |
| `setTransform(t, combine)` / `QTransform` | 1 / 2 | `set_transform(&t, combine)`, `Transform` (map, map_rect, inverted, then/`*`) | ported |
| `setOpacity` | 8 | `set_opacity` | ported |
| `setRenderHint` (Antialiasing 21, SmoothPixmapTransform 5) | 27 | `set_render_hint(RenderHint::…)` | ported (GPU always MSAA) |
| `setCompositionMode` Clear (3), SourceAtop (4), SourceOver (1), Difference (1) | 9 | `CompositionMode::*` (24 modes) | ported; GPU covers Porter-Duff modes, Difference etc. CPU-only |
| `setClipRect` / `setClipPath` / `setClipRegion` / `setClipping` | 1 / 7 / 2 / 8 | `set_clip_rect(_op)`, `set_clip_path(_op)`, `set_clip_region(_op)`, `set_clipping`; Replace / Intersect | ported (scissor + stencil on GPU, AA masks on CPU) |
| `QRegion` (+=, -=, rects) | 13 | `Region` | ported |
| `QPainterPath` build: moveTo, lineTo (41), cubicTo (15), quadTo (15), closeSubpath (14), addRect (6), addEllipse (4), addRoundedRect, addPolygon, addPath, addText (2), arcTo/arcMoveTo | 90 | `Path` (+ `PathBuilder`) with Qt semantics (implicit `MoveTo(0,0)`, move replacement, zero-length line skip, odd-even default) | ported |
| `boundingRect`, `contains` (18), `intersects` (5), `translated` (12) | — | `bounding_rect` (exact extrema), `control_point_rect`, `contains`, `contains_rect`, `intersects_rect`, `intersects_path`, `translated`, `transformed` | ported |
| `path.subtracted` (2, rect − rect in `desktop_surface`) | 2 | `Region` ops + `Region::to_path` | ported for the rect case; no general path booleans |
| `QPainterPathStroker.createStroke` (armour_ui hit areas) | 1 | `Path::stroked(&pen)` (winding triangles; `contains` works) | ported |
| `QPolygonF` | 7 | slices of `(f64, f64)` / `Vec2` | ported |
| `QImage` / `QPixmap` creation, `fromImage`, file load | 62 / 35 | `Image` (premultiplied RGBA8, `Arc` copy-on-write, id + version for GPU caching), `load_png`, `decode_png`, `save_png` | ported |
| `pixel` (4), `setPixel` (7), `bits`/`constBits` (4, sprite_tint / screen_text) | — | `pixel`, `pixel_premultiplied`, `set_pixel`, `data`, `data_mut`, `to_straight_rgba`, BGRA helpers | ported (byte order is RGBA, not Qt's BGRA) |
| `copy` (16) | 16 | `copy` / `sub_image` (outside area transparent) | ported |
| `scaled(..., SmoothTransformation)` (4) | 4 | `smooth_scaled` (box filter down, bilinear up), `scaled_nearest`, `scaled` | ported |
| `convertToFormat` (Grayscale8, ARGB32, RGB32) | 11 | `to_gray8` (`qGray`), `from_gray8`, `to_straight_rgba` | ported |
| `devicePixelRatio` | — | `device_pixel_ratio`, honoured by `draw_image_at` | ported |
| `QPainter(image)` offscreen painting | many | `Image::paint(\|c\| …)` (CPU backend on the image) | ported |
| `QFont(family, pt[, weight])`, setPixelSize, setPointSizeF, setBold, setWeight (Light/DemiBold/Bold/Black), setFamily("Consolas"), setLetterSpacing(Absolute) | 28 + 30 `setFont` | `Font` (`Copy`, interned family), `Weight` (Qt 5 values) | ported |
| `QFontMetrics`: horizontalAdvance (6), height, ascent, boundingRect, elidedText (3) | 6 | `FontMetrics` (int and `_f` float variants), `bounding_rect_in`, `elided_text` | ported |
| Alignment / text flags (AlignCenter 31, Left 18, HCenter 15, VCenter 12, Top 12, WordWrap 5, Right 1, ElideRight 3) | — | `TextFlags` (Qt bit values), `ElideMode` | ported |

## Architecture as built

* `Canvas` records into `Scene`: flattened commands (final transform, paint,
  opacity, composition, clip id), with geometry in shared arenas. `clear()`
  keeps capacity; solid colours are stored inline and gradients are recorded
  once per `set_brush`/`set_pen`.
* CPU (`CpuRenderer`): tiny-skia; exact fill rules, AA clip masks (cached per
  clip), patterns for images, glyph outlines (cached) for text.
* GPU (`GpuRenderer`): one dynamic vertex/index buffer and a paint storage
  buffer per frame (reused); one uber-shader (solid / linear / radial with
  focal / conical / image linear / image nearest, × glyph coverage);
  per-command depth with a strict test so overlapping stroke triangles never
  double-blend; scissor clip rects; stencil clip paths (one increment per
  intersected path); 4× MSAA with a persistent multisampled buffer;
  glyph atlas (R8, shelf-packed, grows 1024 → 4096, subpixel-x bins);
  image atlas (2048² RGBA pages with 1 px replicated borders, large images get
  their own texture, re-uploaded on version change); glyphs above 160 device
  px are tessellated outlines. Fills: convex contours as fans, others through
  lyon's `FillTessellator`; strokes through the crate's own polyline stroker
  (segment quads + join/cap fans), which with the depth test is exact for
  round joins and much faster than lyon's stroker.
* `Headless` creates its own device for offscreen rendering and readback.

## Tests

* 39 unit tests: transforms, path semantics/geometry/arcs/contains/stroker,
  regions, gradients and stops, pens, images (COW, copy, scaling, PNG), text
  metrics (Segoe UI 9 pt = 13/3/16 like Qt; skip without fonts), layout,
  canvas recording, CPU compositing / clips / gradients / images, atlas packer.
* `tests/render.rs`: CPU fixture regression (`tests/fixtures/cpu_features.png`,
  re-bless with `DC_RENDER_BLESS=1`), CPU vs GPU agreement on the feature
  scene (mean channel diff 0.51 on RX 9070 XT), `Scene::append_transformed`
  (matches recording under the transform, mean diff 0.014 from the dashes;
  stays inside its clip; GPU agrees with CPU), spiders + text CPU vs GPU and
  batching (≤ 3 draw calls), persistent contents with `clear: None`. GPU tests
  skip when no adapter exists.
* `examples/bench.rs` (300 spiders, 1920×1080): record 0.76 ms, CPU 47 ms,
  GPU 4.5 ms per frame including the wait (3.8 ms of it CPU-side
  tessellation and upload), 1 draw call, 23,400 commands.
* `examples/smoke.rs`: writes `spider_cpu.png` / `spider_gpu.png`.

## Not covered / known differences

* General `QPainterPath` booleans (`united`, `intersected`, `subtracted` of
  arbitrary paths): only rectangles (via `Region`), which is all the app uses.
* GPU: non-Porter-Duff blend modes (`Difference`, `Multiply`, …) fall back to
  SourceOver with a one-time warning; the antialiasing hint is ignored
  (MSAA always on). Use the CPU backend for `screen_text`'s Difference pass.
* Text: no complex shaping (GPOS, ligatures, bidi); kerning from the `kern`
  table only. Advances are rounded to pixels to match Qt's GDI engine; small
  label widths may still differ from Qt by about 1 px.
* `QPixmap`-specific APIs (`mask`, `QBitmap`) and `drawTiledPixmap` are unused
  in Python and not provided.
* GPU tessellation runs on one thread (3.8 ms for 300 spiders); split it
  across threads if profiles ever need it.
