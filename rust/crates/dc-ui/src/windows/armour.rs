//! Port of app/armour_ui.py: a tarantula anatomy doll and a bag of pieces
//! to drag onto it.
//!
//! The doll is a top-down red-knee drawn to the tarantula model's
//! proportions. Drop a piece anywhere on the doll and it goes to its part,
//! which lights up while it is dragged; drag a worn piece back to the bag, or
//! double-click it, to take it off. A click without a drag picks the piece
//! for the item panel.
//!
//! TODO(content): the worn pieces are painted as tier-coloured overlays here;
//! dc-paint's `armour_art` painters (`paint_carapace`, `paint_leg_armour`,
//! ...) and `armour_icon` take over through [`ArmourIconFn`] once they land.

use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

use crate::art;
use crate::model::{ARMOR_TIERS, ArmourIconFn, ArmourItem, Progression, ProgressionCatalog, SLOT_ORDER};
use crate::paint;
use crate::probe;
use crate::stat_text;
use crate::theme::{self, *};
use crate::widgets;

// The tarantula model's own proportions, in body sizes.
const ABDOMEN: (f32, f32, f32) = (-0.60, 1.06, 0.86);
const CEPH: (f32, f32, f32) = (0.22, 0.76, 0.66);
const HEAD: (f32, f32, f32) = (0.64, 0.26, 0.34);
const LEGS: [(f32, f32, f32); 4] = [(38.0, 32.0, 1.0), (72.0, 70.0, 0.92), (104.0, 112.0, 0.9), (134.0, 150.0, 1.04)];
const LEG_SEGMENTS: [f32; 5] = [0.62, 0.24, 0.48, 0.44, 0.28];
const LEG_WIDTHS: [f32; 5] = [0.13, 0.12, 0.1, 0.08, 0.062];
const PALP_SEGMENTS: [f32; 5] = [0.12, 0.22, 0.12, 0.2, 0.15];
const PALP_WIDTHS: [f32; 5] = [0.09, 0.09, 0.085, 0.075, 0.06];

const BODY: Color32 = rgb(28, 22, 21);
const BODY_LIGHT: Color32 = rgb(70, 60, 54);
const SETAE: Color32 = rgba(120, 108, 96, 150);
const BAND: Color32 = rgb(226, 110, 40);
const RIM: Color32 = rgb(200, 124, 66);

/// What dragging and clicking on the doll or bag asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum ArmourEvent {
    Equip(String),
    Unequip(String),
    Picked(String),
}

/// The drag payload (`MIME`): an item from the bag, or a worn slot.
#[derive(Clone, Debug, PartialEq)]
pub enum ArmourDrag {
    Item(String),
    Worn(String),
}

fn polar(x: f32, y: f32, angle_deg: f32, length: f32, side: f32) -> (f32, f32) {
    let a = angle_deg.to_radians();
    (x + a.cos() * length, y + side * a.sin() * length)
}

fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    (a + ab * t - p).length()
}

/// `SpiderDoll`.
pub struct SpiderDoll {
    pub compact: bool,
    pub size: Vec2,
    unit: f32,
    plaque_w: f32,
    legs: Vec<Vec<(f32, f32)>>,
    palps: Vec<Vec<(f32, f32)>>,
    hairs: Vec<(f32, f32, f32)>,
    pub hover_slot: Option<String>,
    pub drag_slot: Option<String>,
}

impl SpiderDoll {
    pub const PLAQUE_H: f32 = 54.0;

    pub fn new(compact: bool) -> Self {
        let unit = if compact { 58.0 } else { 80.0 };
        let s = unit;
        let mut legs = Vec::new();
        for side in [-1.0f32, 1.0] {
            for (attach, aim, scale) in LEGS {
                let a = attach.to_radians();
                let root = (CEPH.0 * s + a.cos() * CEPH.1 * s * 0.36, side * a.sin() * CEPH.2 * s * 0.40);
                let femur = aim * 0.55 + 90.0 * 0.45;
                let angles = [femur, (femur + aim) / 2.0, aim, aim + 6.0 * (aim - 90.0) / 60.0, aim];
                let mut points = vec![root];
                for (length, angle) in LEG_SEGMENTS.iter().zip(angles) {
                    let last = *points.last().unwrap();
                    points.push(polar(last.0, last.1, angle, length * s * scale, side));
                }
                legs.push(points);
            }
        }
        let mut palps = Vec::new();
        for side in [-1.0f32, 1.0] {
            let mut points = vec![(0.54 * s, side * 0.13 * s)];
            for (length, angle) in PALP_SEGMENTS.iter().zip([60.0, 34.0, 18.0, 8.0, -4.0]) {
                let last = *points.last().unwrap();
                points.push(polar(last.0, last.1, angle, length * s, side));
            }
            palps.push(points);
        }
        // random.Random(7), three uniforms per hair.
        let mut rng = dc_core::PyRandom::from_int(7);
        let hairs = (0..150)
            .map(|_| {
                let a = rng.uniform(-1.0, 1.0) as f32;
                let b = rng.uniform(-1.0, 1.0) as f32;
                let c = rng.uniform(-0.6, 0.6) as f32;
                (a, b, c)
            })
            .collect();
        Self {
            compact,
            size: if compact { vec2(440.0, 340.0) } else { vec2(620.0, 440.0) },
            unit,
            plaque_w: if compact { 120.0 } else { 156.0 },
            legs,
            palps,
            hairs,
            hover_slot: None,
            drag_slot: None,
        }
    }

    /// Spider coordinates (front along +x) to the widget: front points up.
    fn map(&self, origin: Pos2, p: (f32, f32)) -> Pos2 {
        let c = origin + vec2(self.size.x / 2.0, self.size.y / 2.0 + 8.0);
        pos2(c.x + p.1, c.y - p.0)
    }

    pub fn plaques(&self, origin: Pos2) -> Vec<(&'static str, Rect)> {
        let (w, h, pw, ph) = (self.size.x, self.size.y, self.plaque_w, Self::PLAQUE_H);
        let r = |x: f32, y: f32| Rect::from_min_size(origin + vec2(x, y), vec2(pw, ph));
        vec![
            ("head", r(8.0, 10.0)),
            ("pedipalps", r(w - pw - 8.0, 10.0)),
            ("carapace", r(w - pw - 8.0, h * 0.44)),
            ("abdomen", r(w - pw - 8.0, h - ph - 10.0)),
            ("legs", r(8.0, h * 0.5)),
        ]
    }

    fn anchor(&self, origin: Pos2, slot: &str) -> Pos2 {
        let s = self.unit;
        let p = match slot {
            "head" => (0.72 * s, 0.0),
            "pedipalps" => self.palps[1][3],
            "carapace" => (CEPH.0 * s, 0.18 * s),
            "abdomen" => (ABDOMEN.0 * s - 0.1 * s, 0.22 * s),
            _ => self.legs[1][2],
        };
        self.map(origin, p)
    }

    fn in_ellipse(&self, origin: Pos2, point: Pos2, cx: f32, length: f32, width: f32) -> bool {
        let c = self.map(origin, (cx, 0.0));
        // Rotated: the length runs up the screen.
        let dx = (point.x - c.x) / (width / 2.0);
        let dy = (point.y - c.y) / (length / 2.0);
        dx * dx + dy * dy <= 1.0
    }

    fn near_chains(&self, origin: Pos2, point: Pos2, chains: &[Vec<(f32, f32)>], width: f32) -> bool {
        chains.iter().any(|chain| {
            chain
                .windows(2)
                .any(|w| dist_to_segment(point, self.map(origin, w[0]), self.map(origin, w[1])) <= width / 2.0)
        })
    }

    /// `slot_at`: the part under `point`, plaques included.
    pub fn slot_at(&self, origin: Pos2, point: Pos2) -> Option<&'static str> {
        let s = self.unit;
        let plaques = self.plaques(origin);
        for slot in SLOT_ORDER {
            if plaques.iter().any(|(n, r)| *n == slot && r.contains(point)) {
                return Some(slot);
            }
            let hit = match slot {
                "abdomen" => self.in_ellipse(origin, point, ABDOMEN.0 * s, ABDOMEN.1 * s, ABDOMEN.2 * s),
                "carapace" => self.in_ellipse(origin, point, CEPH.0 * s, CEPH.1 * s, CEPH.2 * s),
                "head" => self.in_ellipse(origin, point, 0.7 * s, 0.4 * s, 0.42 * s),
                "legs" => self.near_chains(origin, point, &self.legs, 0.2 * s),
                _ => self.near_chains(origin, point, &self.palps, 0.18 * s),
            };
            if hit {
                return Some(slot);
            }
        }
        None
    }

    /// Draw the doll for `state`.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        state: &Progression,
        catalog: &ProgressionCatalog,
        icons: Option<&ArmourIconFn>,
    ) -> Vec<ArmourEvent> {
        let mut events = Vec::new();
        let (rect, response) = ui.allocate_exact_size(self.size, Sense::click_and_drag());
        probe::record(ui.ctx(), if self.compact { "doll:compact" } else { "doll" }, rect);
        let origin = rect.min;
        let worn = |slot: &str| state.equipped.get(slot).and_then(|id| catalog.item(id));
        let pointer = ui.input(|i| i.pointer.hover_pos());
        self.hover_slot =
            pointer.filter(|p| rect.contains(*p)).and_then(|p| self.slot_at(origin, p)).map(str::to_owned);
        // A piece dragged from the bag lights up its part.
        self.drag_slot = match egui::DragAndDrop::payload::<ArmourDrag>(ui.ctx()).as_deref() {
            Some(ArmourDrag::Item(id)) if response.contains_pointer() => catalog.item(id).map(|i| i.slot.clone()),
            _ => None,
        };
        if let Some(payload) = response.dnd_release_payload::<ArmourDrag>()
            && let ArmourDrag::Item(id) = payload.as_ref()
        {
            events.push(ArmourEvent::Equip(id.clone()));
        }
        // Dragging a worn piece off the doll.
        if response.drag_started()
            && let Some(slot) = ui.input(|i| i.pointer.press_origin()).and_then(|p| self.slot_at(origin, p))
            && worn(slot).is_some()
        {
            response.dnd_set_drag_payload(ArmourDrag::Worn(slot.to_owned()));
        }
        if response.double_clicked()
            && let Some(slot) = response.interact_pointer_pos().and_then(|p| self.slot_at(origin, p))
            && worn(slot).is_some()
        {
            events.push(ArmourEvent::Unequip(slot.to_owned()));
        } else if response.clicked()
            && let Some(slot) = response.interact_pointer_pos().and_then(|p| self.slot_at(origin, p))
            && let Some(item) = worn(slot)
        {
            events.push(ArmourEvent::Picked(item.id.clone()));
        }
        let hover_slot = self.hover_slot.clone();
        let hovered_item = hover_slot.as_deref().and_then(worn).cloned();
        let response = match (hover_slot, hovered_item) {
            (_, Some(item)) => {
                let mut lines = stat_text::item_tooltip(&item, Some(state), state.item_level(&item.id), catalog);
                lines.push(("Drag to the bag or double-click to take off".into(), stat_text::DIM, false));
                widgets::rich_tooltip(response, lines)
            }
            (Some(slot), None) => widgets::rich_tooltip(
                response,
                vec![(stat_text::slot_name(&slot), INK, true), ("Nothing worn. Drag a piece here.".into(), INK, false)],
            ),
            _ => response,
        };
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::accesskit::Role::Group, true, "Spider anatomy: drag armour onto it")
        });
        if ui.is_rect_visible(rect) {
            self.paint(ui, rect, state, catalog, icons);
        }
        events
    }

    fn paint(
        &self,
        ui: &Ui,
        rect: Rect,
        state: &Progression,
        catalog: &ProgressionCatalog,
        icons: Option<&ArmourIconFn>,
    ) {
        let painter = ui.painter_at(rect);
        let origin = rect.min;
        let s = self.unit;
        // The stage: a soft glow and a faint orb web behind the doll.
        paint::glow(&painter, rect.center(), rect.width() * 0.46, 0.0, rgba(96, 70, 40, 150));
        let c = origin + vec2(self.size.x / 2.0, self.size.y / 2.0 + 8.0);
        let web = Stroke::new(1.0, rgba(246, 226, 184, 30));
        for i in 0..12 {
            let a = i as f32 * std::f32::consts::TAU / 12.0;
            painter.line_segment([c, c + vec2(a.cos(), a.sin()) * 205.0], web);
        }
        let mut r = 40.0;
        while r < 210.0 {
            let ring: Vec<Pos2> = (0..13)
                .map(|i| {
                    let a = i as f32 * std::f32::consts::TAU / 12.0;
                    c + vec2(a.cos(), a.sin()) * r
                })
                .collect();
            paint::polyline(&painter, ring, web);
            r += 28.0;
        }
        let m = |p: (f32, f32)| self.map(origin, p);
        // Shadow.
        painter.add(egui::Shape::ellipse_filled(m((-0.2 * s + 6.0, 6.0)), vec2(0.7 * s, 1.1 * s), rgba(0, 0, 0, 60)));
        let limb = |points: &[(f32, f32)], widths: &[f32; 5]| {
            for i in 0..points.len() - 1 {
                let (a, b) = (m(points[i]), m(points[i + 1]));
                let width = widths[i] * s;
                painter.line_segment([a, b], Stroke::new(width * 1.35, SETAE));
                painter.line_segment([a, b], Stroke::new(width, if i == 1 { BAND } else { BODY }));
                painter.circle_filled(a, width / 2.0, if i == 1 { BAND } else { BODY });
                if i == 2 || i == 3 {
                    let d = b - a;
                    painter
                        .line_segment([a + d * 0.02, a + d * 0.1], Stroke::new(width * 0.9, rgba(226, 150, 90, 170)));
                }
                // Setae standing off the segment.
                let d = b - a;
                let len = d.length().max(1.0);
                let n = vec2(-d.y, d.x) / len;
                for k in 1..5 {
                    let base = a + d * (k as f32 / 5.0);
                    for sign in [-1.0f32, 1.0] {
                        painter.line_segment(
                            [base + n * sign * width * 0.45, base + n * sign * width * 0.9 + d / len * width * 0.3],
                            Stroke::new(1.0, SETAE),
                        );
                    }
                }
            }
            painter.circle_filled(m(*points.last().unwrap()), widths[4] * s * 0.6, BODY);
        };
        for points in &self.legs {
            limb(points, &LEG_WIDTHS);
        }
        let tint = |item: &ArmourItem, alpha: u8| theme::with_alpha(stat_text::tier_color(&item.tier), alpha);
        let worn = |slot: &str| state.equipped.get(slot).and_then(|id| catalog.item(id));
        if let Some(item) = worn("legs") {
            for points in &self.legs {
                for i in 0..4 {
                    painter.line_segment(
                        [m(points[i]), m(points[i + 1])],
                        Stroke::new(LEG_WIDTHS[i] * s * 0.8, tint(item, 200)),
                    );
                }
            }
        }
        for points in &self.palps {
            limb(points, &PALP_WIDTHS);
        }
        if let Some(item) = worn("pedipalps") {
            for points in &self.palps {
                painter
                    .line_segment([m(points[2]), m(points[3])], Stroke::new(PALP_WIDTHS[2] * s * 1.2, tint(item, 220)));
            }
        }
        // Abdomen: black, densely haired.
        let (ax, aw, ah) = (ABDOMEN.0 * s, ABDOMEN.1 * s, ABDOMEN.2 * s);
        paint::radial_ellipse(
            &painter,
            m((ax, 0.0)),
            vec2(ah / 2.0, aw / 2.0),
            m((ax + aw * 0.12, -ah * 0.15)),
            BODY_LIGHT,
            BODY,
        );
        painter.add(egui::Shape::ellipse_stroke(
            m((ax, 0.0)),
            vec2(ah / 2.0, aw / 2.0),
            Stroke::new(1.5, rgb(12, 10, 10)),
        ));
        for (hx, hy, tilt) in &self.hairs {
            if hx * hx + hy * hy > 0.9 {
                continue;
            }
            let (x, y) = (ax + hx * aw * 0.5, hy * ah * 0.5);
            painter.line_segment([m((x, y)), m((x - aw * 0.05, y + tilt * ah * 0.04))], Stroke::new(1.0, SETAE));
        }
        if let Some(item) = worn("abdomen") {
            for k in 0..4 {
                let x = ax - aw * 0.3 + k as f32 * aw * 0.18;
                painter.add(egui::Shape::ellipse_filled(m((x, 0.0)), vec2(ah * 0.42, aw * 0.08), tint(item, 170)));
            }
        }
        // Pedicel and carapace with the red-knee's orange rim.
        painter.add(egui::Shape::ellipse_filled(m((-0.06 * s, 0.0)), vec2(0.06 * s, 0.09 * s), BODY));
        let (cx, cw, ch) = (CEPH.0 * s, CEPH.1 * s, CEPH.2 * s);
        painter.add(egui::Shape::ellipse_filled(m((cx, 0.0)), vec2(ch / 2.0, cw / 2.0), RIM));
        paint::radial_ellipse(
            &painter,
            m((cx, 0.0)),
            vec2(ch / 2.0 - 0.1 * s, cw / 2.0 - 0.1 * s),
            m((cx + cw * 0.1, -ch * 0.1)),
            BODY_LIGHT,
            BODY,
        );
        for i in 0..8 {
            let a = (i as f32 + 0.5) * std::f32::consts::PI / 4.0;
            painter.line_segment(
                [
                    m((cx - cw * 0.05 + a.cos() * cw * 0.08, a.sin() * ch * 0.08)),
                    m((cx - cw * 0.05 + a.cos() * cw * 0.26, a.sin() * ch * 0.26)),
                ],
                Stroke::new(1.2, rgba(90, 76, 66, 160)),
            );
        }
        if let Some(item) = worn("carapace") {
            painter.add(egui::Shape::ellipse_filled(m((cx, 0.0)), vec2(ch * 0.42, cw * 0.42), tint(item, 150)));
            painter.add(egui::Shape::ellipse_stroke(
                m((cx, 0.0)),
                vec2(ch * 0.42, cw * 0.42),
                Stroke::new(2.0, tint(item, 255)),
            ));
        }
        // Head lobe and chelicerae.
        let (hx, hw, hh) = (HEAD.0 * s, HEAD.1 * s, HEAD.2 * s);
        for side in [-1.0f32, 1.0] {
            painter.add(egui::Shape::ellipse_filled(
                m((hx + hw * 0.5, side * hh * 0.2)),
                vec2(hh * 0.22, hw * 0.3),
                BODY,
            ));
        }
        painter.add(egui::Shape::ellipse_filled(m((hx, 0.0)), vec2(hh / 2.0, hw / 2.0), BODY));
        if let Some(item) = worn("head") {
            painter.add(egui::Shape::ellipse_stroke(
                m((hx, 0.0)),
                vec2(hh / 2.0 + 2.0, hw / 2.0 + 2.0),
                Stroke::new(3.0, tint(item, 230)),
            ));
        }
        // Eight eyes, drawn last so every visor leaves them clear.
        painter.add(egui::Shape::ellipse_filled(m((hx - hw * 0.1, 0.0)), vec2(hh * 0.3, hw * 0.34), rgb(10, 8, 8)));
        for (ex, ey, r) in [
            (0.02, -0.09, 0.05),
            (0.02, 0.09, 0.05),
            (-0.05, -0.04, 0.035),
            (-0.05, 0.04, 0.035),
            (0.08, -0.15, 0.03),
            (0.08, 0.15, 0.03),
            (-0.1, -0.14, 0.028),
            (-0.1, 0.14, 0.028),
        ] {
            painter.circle_filled(m((hx - hw * 0.1 + ex * s, ey * s)), r * s * 0.6, rgb(150, 140, 128));
        }
        // Highlights.
        for (slot, fill) in [
            (self.hover_slot.as_deref(), rgba(246, 226, 184, 60)),
            (self.drag_slot.as_deref(), rgba(214, 164, 72, 110)),
        ] {
            let Some(slot) = slot else { continue };
            let outline = Stroke::new(2.0, BRASS);
            match slot {
                "abdomen" => {
                    painter.add(egui::Shape::ellipse_filled(m((ax, 0.0)), vec2(ah / 2.0, aw / 2.0), fill));
                    painter.add(egui::Shape::ellipse_stroke(m((ax, 0.0)), vec2(ah / 2.0, aw / 2.0), outline));
                }
                "carapace" => {
                    painter.add(egui::Shape::ellipse_filled(m((cx, 0.0)), vec2(ch / 2.0, cw / 2.0), fill));
                    painter.add(egui::Shape::ellipse_stroke(m((cx, 0.0)), vec2(ch / 2.0, cw / 2.0), outline));
                }
                "head" => {
                    painter.add(egui::Shape::ellipse_filled(m((0.7 * s, 0.0)), vec2(0.21 * s, 0.2 * s), fill));
                    painter.add(egui::Shape::ellipse_stroke(m((0.7 * s, 0.0)), vec2(0.21 * s, 0.2 * s), outline));
                }
                "legs" | "pedipalps" => {
                    let (chains, width) = if slot == "legs" { (&self.legs, 0.2 * s) } else { (&self.palps, 0.18 * s) };
                    for chain in chains {
                        paint::polyline(&painter, chain.iter().map(|p| m(*p)).collect(), Stroke::new(width, fill));
                    }
                }
                _ => {}
            }
        }
        // Callouts: a plaque per part, linked to it.
        for (slot, plaque) in self.plaques(origin) {
            let item = worn(slot);
            let lit = self.drag_slot.as_deref() == Some(slot) || self.hover_slot.as_deref() == Some(slot);
            let anchor = self.anchor(origin, slot);
            let edge = if plaque.center().x < rect.center().x { plaque.right_center() } else { plaque.left_center() };
            painter
                .line_segment([edge, anchor], Stroke::new(1.4, if lit || item.is_some() { BRASS } else { INK_SOFT }));
            painter.circle_filled(anchor, 3.0, BRASS);
            let border = match item {
                Some(i) => stat_text::tier_color(&i.tier),
                None if lit => BRASS,
                None => BRASS_DEEP,
            };
            painter.rect(
                plaque,
                CornerRadius::same(8),
                rgba(42, 24, 12, 235),
                Stroke::new(if lit { 2.0 } else { 1.2 }, border),
                StrokeKind::Inside,
            );
            let socket = Rect::from_min_size(plaque.min + vec2(6.0, 6.0), Vec2::splat(plaque.height() - 12.0));
            painter.rect_filled(socket, CornerRadius::same(6), rgba(20, 12, 6, 220));
            if item.is_some() {
                painter.rect_stroke(socket, CornerRadius::same(6), Stroke::new(1.0, INK_SOFT), StrokeKind::Inside);
            } else {
                paint::dashed_rounded_rect(&painter, socket, 6.0, Stroke::new(1.0, INK_SOFT));
            }
            if let Some(item) = item {
                armour_picture(&painter, socket, item, icons);
            }
            let text_x = socket.right() + 8.0;
            painter.text(
                pos2(text_x, plaque.top() + 13.0),
                Align2::LEFT_CENTER,
                stat_text::slot_name(slot),
                theme::bold(8.25),
                BRASS,
            );
            let name = item.map(ArmourItem::short_name).unwrap_or_else(|| "Drag armour here".into());
            let galley = painter.layout(
                name,
                theme::bold(9.0),
                if item.is_some() { CREAM } else { CREAM_SOFT },
                plaque.right() - text_x - 4.0,
            );
            painter.galley(pos2(text_x, plaque.top() + 20.0), galley, CREAM);
        }
    }
}

/// An armour piece's picture: dc-paint's icon when available, else a stand-in.
pub fn armour_picture(painter: &egui::Painter, rect: Rect, item: &ArmourItem, icons: Option<&ArmourIconFn>) {
    if let Some(texture) = icons.and_then(|f| f(&item.id, &item.slot, 64)) {
        painter.image(texture.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    } else {
        art::armour_placeholder(painter, rect, &item.slot, stat_text::tier_color(&item.tier));
    }
}

/// `InventoryBag`: the pieces owned and not worn, best first, in a grid. A
/// worn piece dropped here comes off.
pub struct InventoryBag {
    pub columns: usize,
}

pub const TILE_SIZE: Vec2 = Vec2 { x: 100.0, y: 124.0 };

impl InventoryBag {
    pub fn new(columns: usize) -> Self {
        Self { columns: columns.max(1) }
    }

    pub fn width(&self) -> f32 {
        self.columns as f32 * 108.0 + 34.0
    }

    /// The pieces in the bag, in order: tier (best first), set, slot.
    pub fn contents<'a>(state: &Progression, catalog: &'a ProgressionCatalog, worn: &[String]) -> Vec<&'a ArmourItem> {
        let mut owned: Vec<&ArmourItem> =
            state.inventory.iter().filter(|i| !worn.contains(i)).filter_map(|i| catalog.item(i)).collect();
        owned.sort_by_key(|item| {
            (
                std::cmp::Reverse(ARMOR_TIERS.iter().position(|t| *t == item.tier).unwrap_or(0)),
                item.set_id.clone(),
                SLOT_ORDER.iter().position(|s| *s == item.slot).unwrap_or(9),
            )
        });
        owned
    }

    /// `show_state`. `worn`: pieces worn by anyone (default: this spider's);
    /// `spares`: item id -> spare count.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut Ui,
        id_salt: &str,
        state: &Progression,
        catalog: &ProgressionCatalog,
        worn: Option<&[String]>,
        spares: &std::collections::BTreeMap<String, u32>,
        icons: Option<&ArmourIconFn>,
        max_height: f32,
    ) -> Vec<ArmourEvent> {
        let mut events = Vec::new();
        let own: Vec<String> = state.equipped.values().cloned().collect();
        let worn = worn.map(|w| w.to_vec()).unwrap_or(own);
        let items = Self::contents(state, catalog, &worn);
        let frame = theme::bag_frame().show(ui, |ui| {
            ui.set_width(self.width() - 16.0);
            egui::ScrollArea::vertical()
                .id_salt(("bag", id_salt))
                .max_height(max_height)
                .min_scrolled_height(150.0)
                .show(ui, |ui| {
                    if items.is_empty() {
                        widgets::wrapped(
                            ui,
                            "Everything you own is worn. Drag a piece off the spider to put it back.",
                            theme::font(9.0),
                            CREAM_SOFT,
                        );
                    }
                    egui::Grid::new(("bag-grid", id_salt)).spacing(vec2(8.0, 8.0)).show(ui, |ui| {
                        for (index, item) in items.iter().enumerate() {
                            if let Some(e) = Self::tile(
                                ui,
                                id_salt,
                                item,
                                state,
                                catalog,
                                spares.get(&item.id).copied().unwrap_or(0),
                                icons,
                            ) {
                                events.push(e);
                            }
                            if index % self.columns == self.columns - 1 {
                                ui.end_row();
                            }
                        }
                    });
                });
        });
        probe::record(ui.ctx(), format!("bag:{id_salt}"), frame.response.rect);
        // A worn piece dropped on the bag comes off.
        let bag = ui.interact(frame.response.rect, ui.make_persistent_id(("bag-drop", id_salt)), Sense::hover());
        if let Some(payload) = bag.dnd_release_payload::<ArmourDrag>()
            && let ArmourDrag::Worn(slot) = payload.as_ref()
        {
            events.push(ArmourEvent::Unequip(slot.clone()));
        }
        events
    }

    fn tile(
        ui: &mut Ui,
        id_salt: &str,
        item: &ArmourItem,
        state: &Progression,
        catalog: &ProgressionCatalog,
        spares: u32,
        icons: Option<&ArmourIconFn>,
    ) -> Option<ArmourEvent> {
        let (rect, response) = ui.allocate_exact_size(TILE_SIZE, Sense::click_and_drag());
        probe::record(ui.ctx(), format!("tile:{id_salt}:{}", item.id), rect);
        let level = state.item_level(&item.id);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::ListItem, true, &item.name));
        if response.drag_started() {
            response.dnd_set_drag_payload(ArmourDrag::Item(item.id.clone()));
        }
        let event = if response.double_clicked() {
            Some(ArmourEvent::Equip(item.id.clone()))
        } else if response.clicked() {
            Some(ArmourEvent::Picked(item.id.clone()))
        } else {
            None
        };
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            paint::vertical_gradient(painter, rect, 8.0, rgba(90, 58, 32, 230), rgba(52, 32, 18, 230));
            let border = if response.hovered() {
                Stroke::new(2.0, BRASS)
            } else if item.tier == "common" {
                Stroke::new(1.0, BRASS_DEEP)
            } else {
                Stroke::new(2.0, stat_text::tier_color(&item.tier))
            };
            painter.rect_stroke(rect, CornerRadius::same(8), border, StrokeKind::Inside);
            let picture = Rect::from_center_size(pos2(rect.center().x, rect.top() + 38.0), Vec2::splat(64.0));
            armour_picture(painter, picture, item, icons);
            let badge = |at: Pos2, text: String| {
                let galley = painter.layout_no_wrap(text, theme::bold(7.0), WALNUT_DEEP);
                let r = Rect::from_min_size(at, galley.size() + vec2(8.0, 2.0));
                painter.rect_filled(r, CornerRadius::same(6), BRASS);
                painter.galley(r.min + vec2(4.0, 1.0), galley, WALNUT_DEEP);
            };
            if level > 1 {
                badge(picture.min, format!("Lv {level}"));
            }
            if spares > 0 {
                badge(picture.right_top() - vec2(24.0, 0.0), format!("+{spares}"));
            }
            let name = painter.layout(item.short_name(), theme::bold(8.0), CREAM, rect.width() - 8.0);
            let name_pos = pos2(rect.center().x - name.size().x / 2.0, rect.top() + 74.0);
            painter.galley(name_pos, name, CREAM);
            painter.text(
                pos2(rect.center().x, rect.bottom() - 9.0),
                Align2::CENTER_CENTER,
                stat_text::tier_name(&item.tier),
                theme::bold(7.0),
                stat_text::tier_color(&item.tier),
            );
        }
        let mut lines = stat_text::item_tooltip(item, Some(state), level, catalog);
        lines.push((
            "Drag onto the spider, or double-click. Click for upgrade and sell.".into(),
            stat_text::DIM,
            false,
        ));
        let _ = widgets::rich_tooltip(response, lines);
        event
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_part_of_the_doll_is_found_where_it_is_drawn() {
        let doll = SpiderDoll::new(false);
        let o = Pos2::ZERO;
        let s = doll.unit;
        assert_eq!(doll.slot_at(o, doll.map(o, (ABDOMEN.0 * s, 0.0))), Some("abdomen"));
        assert_eq!(doll.slot_at(o, doll.map(o, (CEPH.0 * s, 0.0))), Some("carapace"));
        assert_eq!(doll.slot_at(o, doll.map(o, (0.72 * s, 0.0))), Some("head"));
        assert_eq!(doll.slot_at(o, doll.map(o, doll.legs[2][3])), Some("legs"));
        for (slot, rect) in doll.plaques(o) {
            assert_eq!(doll.slot_at(o, rect.center()), Some(slot));
        }
        assert_eq!(doll.slot_at(o, pos2(300.0, 2.0)), None);
    }

    #[test]
    fn the_bag_lists_the_best_first_and_hides_what_is_worn() {
        let catalog = crate::sample::progression_catalog();
        let state = Progression {
            inventory: vec!["silk_carapace".into(), "sun_aegis".into(), "warden_crest".into(), "leg_guard_set".into()],
            equipped: [("legs".to_owned(), "leg_guard_set".to_owned())].into_iter().collect(),
            ..Default::default()
        };
        let worn: Vec<String> = state.equipped.values().cloned().collect();
        let ids: Vec<&str> = InventoryBag::contents(&state, &catalog, &worn).iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["sun_aegis", "warden_crest", "silk_carapace"]);
    }
}
