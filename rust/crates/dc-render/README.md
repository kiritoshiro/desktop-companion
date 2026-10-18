# dc-render

The QPainter replacement for the Rust port. Painting code records into a
retained `Scene` through a `Canvas` with QPainter-shaped methods; a scene is
rasterised by `CpuRenderer` (tiny-skia → `Image`) or `GpuRenderer` (wgpu →
any `TextureView`, batched into a few draw calls, 4× MSAA, premultiplied).

```rust
use dc_render::*;

let mut scene = Scene::new();
{
    let mut c = scene.canvas();                      // QPainter p(device)
    c.set_render_hint(RenderHint::Antialiasing, true);
    c.save();
    c.translate(x, y);
    c.rotate(heading.to_degrees());
    c.set_pen(Pen::NONE);
    c.set_brush(RadialGradient::new(0.0, 0.0, r).with_stop(0.0, light).with_stop(1.0, dark));
    c.draw_ellipse_at(0.0, 0.0, rx, ry);
    c.restore();
}                                                     // p.end()
let img = CpuRenderer::new().render(&scene, w, h, Some(Color::TRANSPARENT));
// overlay window: gpu.render(&scene, &view, (w, h), Some(Color::TRANSPARENT));
scene.clear();                                        // next frame, buffers kept
```

## Driving `GpuRenderer` from a window

```rust
let mut renderer = GpuRenderer::new(&device, &queue, surface_format, 4); // MSAA x4
// each frame:
let frame = surface.get_current_texture()?;
let view = frame.texture.create_view(&Default::default());
renderer.render(&scene, &view, (width, height), Some(Color::TRANSPARENT));
frame.present();
```

* Use a **non-sRGB** format (`Bgra8Unorm` / `Rgba8Unorm`) and a
  premultiplied-alpha composite mode for the layered window.
* `clear: None` keeps the previous frame (the MSAA buffer is persistent), so
  Qt-style partial repaints work: clear dirty areas with
  `CompositionMode::Clear` + `fill_rect`, set a clip region, redraw.
* `renderer.stats()` reports draw calls, vertices and CPU prepare time.
* `Headless::new(4)` builds its own device; `render(...)` reads back to an
  `Image`, `render_wait(...)` only waits (for timing). `None` = no adapter.

## Porting QPainter code

General rules:

* Names are QPainter's in snake_case. Overloads get suffixes: `_at` (centre /
  point forms), `_p` (takes `impl Into<Vec2>`, e.g. `(x, y)` or `Vec2`),
  `_rect` / `_in` (rect forms), `_op` (takes a `ClipOp`).
* `QPointF(x, y)` → `(x, y)` or `Vec2::new(x, y)`; `QRectF` → `dc_core::Rect`
  (`Rect::new(x, y, w, h)`, `adjusted`, `translated`, `center`, …).
* `QColor(r, g, b[, a])` → `Color::rgb` / `Color::rgba`; `QColor("#hex")` →
  `parse_color("#hex").unwrap()`; `Qt.transparent` → `Color::TRANSPARENT`.
* Pens, brushes, gradients and fonts are `Copy`; pass them by value.
* Defaults are Qt's: pen black width 1 square cap bevel join; brush none;
  **antialiasing off** until `set_render_hint(Antialiasing, true)` (the GPU
  always antialiases); `QPainterPath` fill rule is odd-even.
* Angles: `rotate` takes degrees (clockwise on screen), path arcs use Qt's
  degrees (counter-clockwise, 0° = 3 o'clock), `draw_arc/chord/pie` take
  1/16 degree ints like Qt (`draw_arc_deg` takes degrees).

| QPainter / Qt | dc-render |
|---|---|
| `QPainter p(&image)` … `p.end()` | `image.paint(\|c\| { … })` (CPU) |
| `QPainter p(widget)` in `paintEvent` | `let mut c = scene.canvas();` then render the scene |
| `p.save()` / `p.restore()` | `c.save()` / `c.restore()` |
| `p.translate(dx, dy)` | `c.translate(dx, dy)` |
| `p.rotate(deg)` | `c.rotate(deg)` (alias `rotate_deg`; `rotate_rad`) |
| `p.scale(sx, sy)` / `p.shear(h, v)` | `c.scale(sx, sy)` / `c.shear(h, v)` |
| `p.setTransform(t, combine)` | `c.set_transform(&t, combine)` |
| `p.worldTransform()` / `p.resetTransform()` | `c.world_transform()` / `c.reset_transform()` |
| `QTransform().translate(..).rotate(..)` | `let mut t = Transform::IDENTITY; t.translate(..).rotate(..);` |
| `t.map(x, y)` / `t.mapRect(r)` / `t.inverted()` | `t.map(x, y)` / `t.map_rect(r)` / `t.inverted()` |
| `p.setPen(Qt.NoPen)` | `c.set_pen(Pen::NONE)` |
| `p.setPen(QColor(..))` | `c.set_pen(color)` |
| `QPen(color, w)` | `Pen::new(color, w)` |
| `QPen(c, w, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin)` | `Pen::full(c, w, PenStyle::Solid, Cap::Round, Join::Round)` |
| `pen.setCapStyle / setJoinStyle / setWidthF` | `pen.set_cap_style / set_join_style / set_width_f` (or builders `.cap() .join() .width()`) |
| `Qt.DashLine`, `pen.setDashPattern([2, 3])` | `PenStyle::Dash`, `pen.set_dash_pattern(&[2.0, 3.0])` |
| `pen.setCosmetic(True)` / width 0 | `pen.set_cosmetic(true)` / width 0 (1 device px) |
| `QPen(QBrush(gradient), w)` | `Pen::with_brush(gradient, w)` |
| `p.setBrush(Qt.NoBrush)` | `c.set_brush(Brush::None)` |
| `p.setBrush(QColor(..))` / `QBrush(color)` | `c.set_brush(color)` |
| `QLinearGradient(x1, y1, x2, y2)` / `(p1, p2)` | `LinearGradient::new(x1, y1, x2, y2)` / `LinearGradient::from_points(p1, p2)` |
| `QRadialGradient(cx, cy, r)` / `(QPointF, r)` | `RadialGradient::new(cx, cy, r)` / `RadialGradient::from_center(p, r)` |
| `QRadialGradient(c, r, focal)` | `RadialGradient::with_focal(cx, cy, r, fx, fy)` |
| `QConicalGradient(cx, cy, angle)` | `ConicalGradient::new(cx, cy, angle)` |
| `g.setColorAt(pos, color)` | `g.set_color_at(pos, color)` (or chain `.with_stop(pos, color)`) |
| `g.setSpread(QGradient.ReflectSpread)` | `g.set_spread(Spread::Reflect)` |
| `p.setOpacity(o)` | `c.set_opacity(o)` |
| `p.setRenderHint(QPainter.Antialiasing, on)` | `c.set_render_hint(RenderHint::Antialiasing, on)` |
| `p.setRenderHint(QPainter.SmoothPixmapTransform)` | `c.set_render_hint(RenderHint::SmoothPixmapTransform, true)` |
| `p.setCompositionMode(QPainter.CompositionMode_Clear)` | `c.set_composition_mode(CompositionMode::Clear)` |
| `CompositionMode_SourceAtop`, `_SourceOver`, `_Difference`, … | `CompositionMode::SourceAtop`, `::SourceOver`, `::Difference`, … |
| `p.drawPath(path)` | `c.draw_path(&path)` |
| `p.fillPath(path, brush)` / `p.strokePath(path, pen)` | `c.fill_path(&path, brush)` / `c.stroke_path(&path, pen)` |
| `p.drawEllipse(QRectF)` | `c.draw_ellipse(rect)` |
| `p.drawEllipse(QPointF(x, y), rx, ry)` | `c.draw_ellipse_at(x, y, rx, ry)` (`draw_ellipse_p(p, rx, ry)`) |
| `p.drawLine(x1, y1, x2, y2)` / `(p1, p2)` | `c.draw_line(x1, y1, x2, y2)` / `c.draw_line_p(p1, p2)` |
| `p.drawLines(lines)` | `c.draw_lines(&[(p1, p2), …])` |
| `p.drawPolyline(*pts)` | `c.draw_polyline(&pts)` |
| `p.drawPolygon(QPolygonF(pts))` | `c.draw_polygon(&pts)` (`draw_polygon_rule(&pts, FillRule::Winding)`) |
| `p.drawConvexPolygon(..)` | `c.draw_convex_polygon(&pts)` |
| `p.drawRect(r)` | `c.draw_rect(r)` |
| `p.fillRect(r, brush_or_color)` | `c.fill_rect(r, brush_or_color)` |
| `p.eraseRect(r)` | `c.erase_rect(r)` |
| `p.drawRoundedRect(r, xr, yr)` | `c.draw_rounded_rect(r, xr, yr)` (`_relative` for `Qt.RelativeSize`) |
| `p.drawArc(r, a16, span16)` | `c.draw_arc(r, a16, span16)` / `c.draw_arc_deg(r, a, span)` |
| `p.drawChord(..)` / `p.drawPie(..)` | `c.draw_chord(..)` / `c.draw_pie(..)` |
| `p.drawPoint(x, y)` / `p.drawPoints(*pts)` | `c.draw_point(x, y)` / `c.draw_points(&pts)` |
| `p.drawImage(target, img, source)` | `c.draw_image(target, &img, source)` |
| `p.drawImage(target_rect, img)` | `c.draw_image_rect(target, &img)` |
| `p.drawImage(QPointF(x, y), img)` / `drawImage(x, y, img)` | `c.draw_image_at(x, y, &img)` |
| `p.drawPixmap(..)` | `c.draw_pixmap(..)` / `c.draw_pixmap_at(..)` (pixmaps are `Image`s) |
| `p.setClipRect(r)` | `c.set_clip_rect(r)` (`set_clip_rect_op(r, ClipOp::Intersect)`) |
| `p.setClipPath(path)` | `c.set_clip_path(&path)` (`set_clip_path_op`) |
| `p.setClipRegion(region[, Qt.IntersectClip])` | `c.set_clip_region(&region)` / `set_clip_region_op(&region, ClipOp::Intersect)` |
| `p.setClipping(False)` | `c.set_clipping(false)` |
| `p.setFont(QFont("Segoe UI", 9, QFont.Bold))` | `c.set_font(Font::with_weight("Segoe UI", 9.0, Weight::BOLD))` |
| `font.setPixelSize(n)` / `setPointSizeF` / `setBold` / `setWeight` | `font.set_pixel_size(n)` / `set_point_size_f` / `set_bold` / `set_weight(Weight::DEMI_BOLD)` |
| `font.setFamily("Consolas")` | `font.set_family("Consolas")` |
| `font.setLetterSpacing(QFont.AbsoluteSpacing, 6)` | `font.set_letter_spacing_absolute(6.0)` |
| `p.drawText(x, y, text)` | `c.draw_text(x, y, text)` (baseline at `y`) |
| `p.drawText(rect, Qt.AlignCenter, text)` | `c.draw_text_in(rect, TextFlags::ALIGN_CENTER, text)` (returns the bounding rect) |
| `Qt.AlignLeft \| Qt.AlignVCenter \| Qt.TextWordWrap` | `TextFlags::ALIGN_LEFT \| TextFlags::ALIGN_VCENTER \| TextFlags::TEXT_WORD_WRAP` |
| `QFontMetrics(font)` / `p.fontMetrics()` | `FontMetrics::new(&font)` / `font.metrics()` / `c.font_metrics()` |
| `fm.horizontalAdvance(t)` / `fm.height()` / `fm.ascent()` | `fm.horizontal_advance(t)` / `fm.height()` / `fm.ascent()` (`_f` variants are float) |
| `fm.boundingRect(t)` / `fm.boundingRect(r, flags, t)` | `fm.bounding_rect(t)` / `fm.bounding_rect_in(r, flags, t)` |
| `fm.elidedText(t, Qt.ElideRight, w)` | `fm.elided_text(t, ElideMode::Right, w)` |
| `QPainterPath()` | `Path::new()` (or `PathBuilder::new()…build()`) |
| `path.moveTo / lineTo / quadTo / cubicTo` | `path.move_to / line_to / quad_to / cubic_to` (`_p` variants take points) |
| `path.arcMoveTo(r, a)` / `path.arcTo(r, a, sweep)` | `path.arc_move_to(r, a)` / `path.arc_to(r, a, sweep)` |
| `path.addEllipse(r)` / `addEllipse(QPointF, rx, ry)` | `path.add_ellipse(r)` / `path.add_ellipse_c(cx, cy, rx, ry)` |
| `path.addRect / addRoundedRect / addPolygon / addPath` | `path.add_rect / add_rounded_rect / add_polygon(&pts) / add_path(&other)` |
| `path.addText(QPointF(x, y), font, text)` | `path.add_text(x, y, &font, text)` |
| `path.closeSubpath()` | `path.close_subpath()` |
| `path.setFillRule(Qt.WindingFill)` | `path.set_fill_rule(FillRule::Winding)` |
| `path.boundingRect()` / `controlPointRect()` | `path.bounding_rect()` / `path.control_point_rect()` |
| `path.contains(QPointF)` / `contains(QRectF)` | `path.contains(x, y)` (`contains_p`) / `path.contains_rect(r)` |
| `path.intersects(QRectF)` / `intersects(path)` | `path.intersects_rect(r)` / `path.intersects_path(&other)` |
| `path.translated(dx, dy)` / `t.map(path)` | `path.translated(dx, dy)` / `path.transformed(&t)` |
| `QPainterPathStroker` + `createStroke(path)` | `path.stroked(&pen)` |
| `path.subtracted(rect_path)` (rects only in the app) | `Region::from_rect(a)` + `subtract_rect(&b)` … `.to_path()` |
| `QRegion(rect)`, `+=`, `-=`, `.rects()` | `Region::from_rect`, `unite_rect`, `subtract_rect`, `rects()` |
| `QImage(w, h, Format_ARGB32_Premultiplied)` + `fill(Qt.transparent)` | `Image::new(w, h)` |
| `QImage(path)` / `QPixmap(path)` | `Image::load_png(path)?` |
| `img.save(path)` | `img.save_png(path)?` |
| `img.width() / height() / isNull()` | `img.width() / height() / is_null()` |
| `img.pixelColor(x, y)` / `img.pixel(x, y)` (premultiplied) | `img.pixel(x, y)` / `img.pixel_premultiplied(x, y)` |
| `img.setPixelColor(x, y, c)` / `img.fill(c)` | `img.set_pixel(x, y, c)` / `img.fill(c)` |
| `img.bits()` (byte access) | `img.data()` / `img.data_mut()` (RGBA premultiplied; Qt is BGRA in memory) |
| `img.copy(x, y, w, h)` | `img.copy(x, y, w, h)` (alias `sub_image`) |
| `img.scaled(w, h, IgnoreAspectRatio, SmoothTransformation)` | `img.smooth_scaled(w, h)` (`scaled(w, h, TransformMode::Fast)`) |
| `img.convertToFormat(Format_Grayscale8)` | `img.to_gray8()` (Qt's `qGray`) / `Image::from_gray8` |
| `img.devicePixelRatio()` / `setDevicePixelRatio` | `img.device_pixel_ratio()` / `set_device_pixel_ratio` |
| `QPixmap.fromImage(img)` / `pixmap.toImage()` | no-op: use the `Image` |

## Notes and differences

* Text metrics emulate Qt 5 on Windows (GDI engine, 96 DPI): integer
  ascent/descent, glyph advances rounded to whole pixels, `kern`-table
  kerning (no GPOS shaping). Segoe UI 9 pt gives ascent 13 / descent 3 /
  height 16, as in Qt.
* The GPU backend supports the Porter-Duff modes (SourceOver … Plus, Clear);
  `Multiply`, `Screen`, `Difference` etc. are CPU-only (the GPU draws them as
  SourceOver and logs once). The Python code uses `Difference` only inside
  `screen_text.find_text_boxes`, which is image processing; do that on the CPU.
* `QPainterPath` boolean ops are not implemented in general: the app only
  subtracts rectangles, which `Region` covers exactly.
* The GPU ignores the antialiasing hint (always MSAA); the CPU honours it.

## Tests and benchmark

```
CARGO_TARGET_DIR=target-dc-render cargo test -p dc-render
CARGO_TARGET_DIR=target-dc-render cargo run --release -p dc-render --example bench [spiders] [frames]
CARGO_TARGET_DIR=target-dc-render cargo run --release -p dc-render --example smoke [out_dir]
```

`DC_RENDER_BLESS=1` rewrites `tests/fixtures/cpu_features.png`; GPU tests skip
themselves when no adapter is available.
