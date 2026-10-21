//! Port of world/screen_layout.py
//!
//! The monitors as a map a mission can travel: which screens exist, how they
//! join. [`Playfield`] answers *where may a spider stand*; this module answers
//! the questions a mission asks on top of that:
//!
//! - which screen is a point on, and which is the main one;
//! - how two screens connect -- along a shared edge (a **door**: walk
//!   across), or, when monitors do not touch, through a **tunnel** the layout
//!   adds so every screen can still be reached;
//! - which way a spider should walk to reach a point on another screen
//!   ([`ScreenLayout::route`]);
//! - where on a screen to put things ([`Screen::anchor`]).
//!
//! Pure data: coordinates are overlay-local.

use std::collections::{BTreeSet, VecDeque};
use std::f64::consts::PI;

use crate::playfield::{Playfield, ScreenRect};

/// A shared edge shorter than this is a corner touch, not a door.
pub const MIN_DOOR: f64 = 80.0;
/// How near a tunnel mouth a spider must be to go through.
pub const TUNNEL_REACH: f64 = 34.0;
/// How near a door's near side a spider must be to be sent through it.
pub const DOOR_REACH: f64 = 40.0;
/// Distance from the screen edge at which a tunnel mouth sits.
pub const TUNNEL_INSET: f64 = 70.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Screen {
    pub index: usize,
    pub rect: ScreenRect,
    pub primary: bool,
}

impl Screen {
    pub fn name(&self) -> String {
        if self.primary { "Main screen".to_string() } else { format!("Screen {}", self.index + 1) }
    }

    pub fn centre(&self) -> (f64, f64) {
        (self.rect.x + self.rect.w / 2.0, self.rect.y + self.rect.h / 2.0)
    }

    /// The point at fractions `fx`, `fy` of this screen, kept `margin` inside.
    pub fn anchor(&self, fx: f64, fy: f64, margin: f64) -> (f64, f64) {
        let r = &self.rect;
        r.clamp(r.x + r.w * fx, r.y + r.h * fy, margin)
    }
}

/// `"door"` or `"tunnel"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkKind {
    Door,
    Tunnel,
}

impl LinkKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LinkKind::Door => "door",
            LinkKind::Tunnel => "tunnel",
        }
    }
}

/// A way between two screens. `a_point` is on screen `a`, `b_point` on `b`.
///
/// A door's two points are the middle of the shared edge, just either side
/// of it; a tunnel's are its two mouths, well inside each screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenLink {
    pub a: usize,
    pub b: usize,
    pub kind: LinkKind,
    pub a_point: (f64, f64),
    pub b_point: (f64, f64),
}

impl ScreenLink {
    pub fn point_on(&self, screen: usize) -> (f64, f64) {
        if screen == self.a { self.a_point } else { self.b_point }
    }

    pub fn other(&self, screen: usize) -> usize {
        if screen == self.a { self.b } else { self.a }
    }
}

/// Error for a layout built from no usable rectangle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptyLayout;

impl std::fmt::Display for EmptyLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a screen layout needs at least one screen")
    }
}

impl std::error::Error for EmptyLayout {}

/// Every monitor, how they connect, and a way between any two of them.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenLayout {
    pub screens: Vec<Screen>,
    primary: usize,
    pub playfield: Playfield,
    pub links: Vec<ScreenLink>,
}

impl ScreenLayout {
    /// Python raises `ValueError` for no usable screen; this returns `Err`.
    pub fn new(rects: &[ScreenRect], primary: usize) -> Result<Self, EmptyLayout> {
        let cleaned: Vec<ScreenRect> = rects.iter().copied().filter(|r| r.w > 1.0 && r.h > 1.0).collect();
        if cleaned.is_empty() {
            return Err(EmptyLayout);
        }
        let primary = if primary < cleaned.len() { primary } else { 0 };
        let screens: Vec<Screen> =
            cleaned.iter().enumerate().map(|(i, r)| Screen { index: i, rect: *r, primary: i == primary }).collect();
        let right = cleaned.iter().map(|r| r.right()).fold(f64::NEG_INFINITY, f64::max);
        let bottom = cleaned.iter().map(|r| r.bottom()).fold(f64::NEG_INFINITY, f64::max);
        let playfield = Playfield::new(right, bottom, &cleaned);
        let links = find_links(&screens);
        Ok(Self { screens, primary, playfield, links })
    }

    /// Python's `ScreenLayout(rects, primary)` with a signed primary index:
    /// anything out of range picks screen 0.
    pub fn with_primary(rects: &[ScreenRect], primary: i64) -> Result<Self, EmptyLayout> {
        Self::new(rects, usize::try_from(primary).unwrap_or(usize::MAX))
    }

    pub fn single(rect: ScreenRect) -> Result<Self, EmptyLayout> {
        Self::new(&[rect], 0)
    }

    // -- queries ------------------------------------------------------------

    pub fn primary(&self) -> &Screen {
        &self.screens[self.primary]
    }

    pub fn count(&self) -> usize {
        self.screens.len()
    }

    pub fn multi(&self) -> bool {
        self.screens.len() > 1
    }

    /// Every screen but the main one.
    pub fn others(&self) -> Vec<Screen> {
        self.screens.iter().filter(|s| !s.primary).copied().collect()
    }

    /// The screen a point is on; off every screen, the nearest one.
    pub fn screen_at(&self, x: f64, y: f64) -> &Screen {
        if let Some(s) = self.screens.iter().find(|s| s.rect.contains(x, y, 0.0)) {
            return s;
        }
        let mut best = &self.screens[0];
        let mut best_d = distance_to_rect(&best.rect, x, y);
        for s in &self.screens[1..] {
            let d = distance_to_rect(&s.rect, x, y);
            if d < best_d {
                best = s;
                best_d = d;
            }
        }
        best
    }

    pub fn clamp(&self, x: f64, y: f64, margin: f64) -> (f64, f64) {
        self.playfield.clamp(x, y, margin)
    }

    pub fn links_of(&self, screen: usize) -> Vec<ScreenLink> {
        self.links.iter().filter(|l| l.a == screen || l.b == screen).copied().collect()
    }

    pub fn tunnels(&self) -> Vec<ScreenLink> {
        self.links.iter().filter(|l| l.kind == LinkKind::Tunnel).copied().collect()
    }

    /// The links to cross, in order, from screen `start` to `goal`.
    pub fn path(&self, start: usize, goal: usize) -> Vec<ScreenLink> {
        if start == goal {
            return Vec::new();
        }
        let n = self.screens.len();
        let mut came: Vec<Option<Option<(usize, ScreenLink)>>> = vec![None; n.max(start + 1).max(goal + 1)];
        came[start] = Some(None);
        let mut queue = VecDeque::from([start]);
        while let Some(here) = queue.pop_front() {
            if here == goal {
                break;
            }
            for link in self.links_of(here) {
                let there = link.other(here);
                if came[there].is_none() {
                    came[there] = Some(Some((here, link)));
                    queue.push_back(there);
                }
            }
        }
        if came[goal].is_none() {
            return Vec::new();
        }
        let mut steps = Vec::new();
        let mut node = goal;
        while let Some(Some((here, link))) = came[node] {
            steps.push(link);
            node = here;
        }
        steps.reverse();
        steps
    }

    /// Where to walk next to reach `(tx, ty)` from `(x, y)`: on the same
    /// screen the target itself; otherwise the near side of the first door
    /// or tunnel on the way.
    pub fn route(&self, x: f64, y: f64, tx: f64, ty: f64) -> (f64, f64) {
        let here = self.screen_at(x, y).index;
        let there = self.screen_at(tx, ty).index;
        let steps = self.path(here, there);
        let Some(link) = steps.first() else {
            return (tx, ty);
        };
        let near = link.point_on(here);
        if link.kind == LinkKind::Door && (x - near.0).hypot(y - near.1) < DOOR_REACH {
            // At the door: step through it.
            return link.point_on(link.other(here));
        }
        near
    }

    /// (tunnel, far mouth) when `(x, y)` stands in a tunnel mouth.
    pub fn tunnel_at(&self, x: f64, y: f64, reach: f64) -> Option<(ScreenLink, (f64, f64))> {
        for link in self.links.iter().filter(|l| l.kind == LinkKind::Tunnel) {
            for (near, far) in [(link.a_point, link.b_point), (link.b_point, link.a_point)] {
                if (x - near.0).hypot(y - near.1) <= reach {
                    return Some((*link, far));
                }
            }
        }
        None
    }
}

// -- building -----------------------------------------------------------------

fn find_links(screens: &[Screen]) -> Vec<ScreenLink> {
    let mut doors = Vec::new();
    for a in screens {
        for b in screens {
            if b.index <= a.index {
                continue;
            }
            if let Some(door) = door(a, b) {
                doors.push(door);
            }
        }
    }
    let mut links = doors.clone();
    // Screens that share no edge with the rest are joined by tunnels,
    // nearest pair first, until all connect.
    let mut groups = components(screens.len(), &doors);
    while groups.len() > 1 {
        let mut best: Option<(f64, usize, usize)> = None;
        for (ia, ga) in groups.iter().enumerate() {
            for gb in &groups[ia + 1..] {
                for &i in ga {
                    for &j in gb {
                        let d = rect_gap(&screens[i].rect, &screens[j].rect);
                        if best.is_none_or(|(bd, _, _)| d < bd) {
                            best = Some((d, i, j));
                        }
                    }
                }
            }
        }
        let (_, i, j) = best.expect("at least two groups");
        links.push(tunnel(&screens[i], &screens[j]));
        let gi = groups.iter().find(|g| g.contains(&i)).cloned().unwrap_or_default();
        let gj = groups.iter().find(|g| g.contains(&j)).cloned().unwrap_or_default();
        let merged: BTreeSet<usize> = gi.union(&gj).copied().collect();
        groups.retain(|g| !g.contains(&i) && !g.contains(&j));
        groups.push(merged);
    }
    links
}

fn door(a: &Screen, b: &Screen) -> Option<ScreenLink> {
    let (ra, rb) = (&a.rect, &b.rect);
    let step = 24.0;
    let link = |ap, bp| Some(ScreenLink { a: a.index, b: b.index, kind: LinkKind::Door, a_point: ap, b_point: bp });
    // Side by side.
    let (lo, hi) = (ra.y.max(rb.y), ra.bottom().min(rb.bottom()));
    if hi - lo >= MIN_DOOR {
        let mid = (lo + hi) / 2.0;
        if (ra.right() - rb.x).abs() <= 1.5 {
            return link((ra.right() - step, mid), (rb.x + step, mid));
        }
        if (rb.right() - ra.x).abs() <= 1.5 {
            return link((ra.x + step, mid), (rb.right() - step, mid));
        }
    }
    // Stacked.
    let (lo, hi) = (ra.x.max(rb.x), ra.right().min(rb.right()));
    if hi - lo >= MIN_DOOR {
        let mid = (lo + hi) / 2.0;
        if (ra.bottom() - rb.y).abs() <= 1.5 {
            return link((mid, ra.bottom() - step), (mid, rb.y + step));
        }
        if (rb.bottom() - ra.y).abs() <= 1.5 {
            return link((mid, ra.y + step), (mid, rb.bottom() - step));
        }
    }
    None
}

/// Mouths on the sides of each screen that face the other one.
fn tunnel(a: &Screen, b: &Screen) -> ScreenLink {
    let ((ax, ay), (bx, by)) = (a.centre(), b.centre());
    let angle = (by - ay).atan2(bx - ax);

    let mouth = |screen: &Screen, towards: f64| {
        let r = &screen.rect;
        let (cx, cy) = screen.centre();
        // Walk from the centre towards the other screen until the inset edge.
        let reach_x = (r.w / 2.0 - TUNNEL_INSET) / 1e-6_f64.max(towards.cos().abs());
        let reach_y = (r.h / 2.0 - TUNNEL_INSET) / 1e-6_f64.max(towards.sin().abs());
        let reach = 0.0_f64.max(reach_x.min(reach_y));
        r.clamp(cx + towards.cos() * reach, cy + towards.sin() * reach, TUNNEL_INSET)
    };

    ScreenLink {
        a: a.index,
        b: b.index,
        kind: LinkKind::Tunnel,
        a_point: mouth(a, angle),
        b_point: mouth(b, angle + PI),
    }
}

fn components(count: usize, links: &[ScreenLink]) -> Vec<BTreeSet<usize>> {
    let mut groups: Vec<BTreeSet<usize>> = (0..count).map(|i| BTreeSet::from([i])).collect();
    for link in links {
        let ga = groups.iter().position(|g| g.contains(&link.a)).expect("screen in a group");
        let gb = groups.iter().position(|g| g.contains(&link.b)).expect("screen in a group");
        if ga != gb {
            let merged: BTreeSet<usize> = groups[ga].union(&groups[gb]).copied().collect();
            let (ga, gb) = (ga.min(gb), ga.max(gb));
            groups.remove(gb);
            groups.remove(ga);
            groups.push(merged);
        }
    }
    groups
}

fn distance_to_rect(r: &ScreenRect, x: f64, y: f64) -> f64 {
    let dx = (r.x - x).max(0.0).max(x - r.right());
    let dy = (r.y - y).max(0.0).max(y - r.bottom());
    dx.hypot(dy)
}

fn rect_gap(a: &ScreenRect, b: &ScreenRect) -> f64 {
    let dx = (b.x - a.right()).max(0.0).max(a.x - b.right());
    let dy = (b.y - a.bottom()).max(0.0).max(a.y - b.bottom());
    dx.hypot(dy)
}

#[cfg(test)]
mod tests {
    //! Port of the screen part of tests/test_screens_and_reclaim.py.
    use super::*;

    pub(crate) fn owner() -> Vec<ScreenRect> {
        vec![ScreenRect::new(0.0, 718.0, 2560.0, 1440.0), ScreenRect::new(2560.0, 0.0, 3840.0, 2160.0)]
    }
    pub(crate) fn side() -> Vec<ScreenRect> {
        vec![ScreenRect::new(0.0, 0.0, 1600.0, 1000.0), ScreenRect::new(1600.0, 0.0, 1600.0, 1000.0)]
    }
    fn apart() -> Vec<ScreenRect> {
        vec![ScreenRect::new(0.0, 0.0, 1600.0, 1000.0), ScreenRect::new(1900.0, 200.0, 1280.0, 800.0)]
    }

    #[test]
    fn the_owners_monitors_join_by_a_door_on_their_shared_edge() {
        let layout = ScreenLayout::new(&owner(), 0).unwrap();
        assert!(layout.multi() && layout.primary().index == 0);
        assert_eq!(layout.links.iter().map(|l| l.kind).collect::<Vec<_>>(), vec![LinkKind::Door]);
        let door = layout.links[0];
        assert!(door.a_point.0 < 2560.0 && 2560.0 < door.b_point.0);
        assert!((718.0..=2158.0).contains(&door.a_point.1));
        assert_eq!(layout.screen_at(100.0, 900.0).index, 0);
        assert_eq!(layout.screen_at(4000.0, 100.0).index, 1);
        assert_eq!(layout.route(300.0, 1400.0, 5000.0, 300.0), door.a_point);
        assert_eq!(layout.route(door.a_point.0, door.a_point.1, 5000.0, 300.0), door.b_point);
        assert_eq!(layout.route(300.0, 1400.0, 900.0, 1500.0), (900.0, 1500.0));
    }

    #[test]
    fn screens_that_do_not_touch_are_joined_by_a_tunnel() {
        let layout = ScreenLayout::new(&apart(), 0).unwrap();
        assert_eq!(layout.links.iter().map(|l| l.kind).collect::<Vec<_>>(), vec![LinkKind::Tunnel]);
        let t = layout.links[0];
        assert_eq!(layout.screen_at(t.a_point.0, t.a_point.1).index, 0);
        assert_eq!(layout.screen_at(t.b_point.0, t.b_point.1).index, 1);
        let found = layout.tunnel_at(t.a_point.0, t.a_point.1, TUNNEL_REACH);
        assert_eq!(found.map(|f| f.1), Some(t.b_point));
        assert_eq!(layout.route(200.0, 200.0, 2500.0, 600.0), t.a_point);
    }

    #[test]
    fn three_screens_all_connect_and_a_path_crosses_them_in_order() {
        let mut rects = side();
        rects.push(ScreenRect::new(4000.0, 0.0, 1200.0, 900.0));
        let layout = ScreenLayout::new(&rects, 1).unwrap();
        assert_eq!(layout.primary().index, 1);
        assert_eq!(layout.primary().name(), "Main screen");
        let steps = layout.path(0, 2);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].kind, LinkKind::Door);
        assert_eq!(steps[1].kind, LinkKind::Tunnel);
    }

    #[test]
    fn an_empty_layout_is_an_error() {
        assert_eq!(ScreenLayout::new(&[ScreenRect::new(0.0, 0.0, 1.0, 1.0)], 0), Err(EmptyLayout));
    }
}
