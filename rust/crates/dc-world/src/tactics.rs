//! Port of world/tactics.py
//!
//! Squad tactics: several spiders fighting as one team.
//!
//! Nothing here knows about missions or which side is which. A squad is a
//! list of [`Member`] (where each spider is, how hurt, what it is good at);
//! the foes are a list of [`Foe`]. [`plan`] picks a **doctrine** for that
//! squad -- from what its members can do, how many there are, the foes in
//! reach, and the squad's skill level -- and gives every member an
//! [`Order`]. Whoever drives the spiders follows the orders.
//!
//! The member/foe key is generic (`K`): the mission uses actor ids, the
//! strategy mode may use anything cloneable and comparable.
//!
//! Skill levels (a map's tier decides it in the Adventure):
//!
//! - 0 -- no teamwork: every spider fights on its own.
//! - 1 -- focus fire: the squad picks one foe and all go for it; the badly
//!   hurt fall back to heal.
//! - 2 -- formations: *hammer and anvil*, *screen*, *regroup*, *perimeter*.
//! - 3 -- advanced: *encircle*, *bait and ambush*.

use std::collections::BTreeSet;
use std::f64::consts::{PI, TAU};

/// What a member is good at. Python stores the name as a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum Role {
    Tank,
    #[default]
    Melee,
    Fast,
    Ranged,
    Support,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Tank => "tank",
            Role::Melee => "melee",
            Role::Fast => "fast",
            Role::Ranged => "ranged",
            Role::Support => "support",
        }
    }

    pub fn parse(name: &str) -> Option<Role> {
        Some(match name {
            "tank" => Role::Tank,
            "melee" => Role::Melee,
            "fast" => Role::Fast,
            "ranged" => Role::Ranged,
            "support" => Role::Support,
            _ => return None,
        })
    }

    /// In `MELEE_ROLES`.
    pub fn is_melee(self) -> bool {
        matches!(self, Role::Tank | Role::Melee | Role::Fast)
    }

    /// In `RANGED_ROLES`.
    pub fn is_ranged(self) -> bool {
        matches!(self, Role::Ranged | Role::Support)
    }
}

pub const MELEE_ROLES: [Role; 3] = [Role::Tank, Role::Melee, Role::Fast];
pub const RANGED_ROLES: [Role; 2] = [Role::Ranged, Role::Support];
/// A squad's members stop fighting and go back to heal below this share.
pub const RETREAT_HEALTH: f64 = 0.3;
/// How far a defending squad reaches from its post before it lets a foe be.
pub const DEFEND_RADIUS: f64 = 330.0;
/// Spread beyond which an attacking squad gathers before it goes in.
pub const REGROUP_SPREAD: f64 = 260.0;
/// `plan`'s default `notice`.
pub const DEFAULT_NOTICE: f64 = 360.0;

/// One spider of the squad, as plain data.
#[derive(Debug, Clone, PartialEq)]
pub struct Member<K> {
    pub key: K,
    pub x: f64,
    pub y: f64,
    /// Share of full health, 0..1.
    pub health: f64,
    pub role: Role,
    /// How close it fights from.
    pub reach: f64,
    pub abilities: BTreeSet<String>,
    /// Its post, or its rally point.
    pub anchor: Option<(f64, f64)>,
}

impl<K> Member<K> {
    /// The Python defaults: full health, melee, reach 34, no anchor.
    pub fn new(key: K, x: f64, y: f64) -> Self {
        Self { key, x, y, health: 1.0, role: Role::Melee, reach: 34.0, abilities: BTreeSet::new(), anchor: None }
    }
}

/// One spider on the other side.
#[derive(Debug, Clone, PartialEq)]
pub struct Foe<K> {
    pub key: K,
    pub x: f64,
    pub y: f64,
    pub health: f64,
    /// How much it matters to stop this one.
    pub threat: f64,
}

impl<K> Foe<K> {
    pub fn new(key: K, x: f64, y: f64) -> Self {
        Self { key, x, y, health: 1.0, threat: 1.0 }
    }
}

/// What an order tells a member to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrderMode {
    Engage,
    Flank,
    Screen,
    Kite,
    Encircle,
    Hold,
    Regroup,
    Retreat,
    Bait,
    Ambush,
}

impl OrderMode {
    pub fn as_str(self) -> &'static str {
        match self {
            OrderMode::Engage => "engage",
            OrderMode::Flank => "flank",
            OrderMode::Screen => "screen",
            OrderMode::Kite => "kite",
            OrderMode::Encircle => "encircle",
            OrderMode::Hold => "hold",
            OrderMode::Regroup => "regroup",
            OrderMode::Retreat => "retreat",
            OrderMode::Bait => "bait",
            OrderMode::Ambush => "ambush",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Order<K> {
    pub mode: OrderMode,
    /// A foe key, or None.
    pub target: Option<K>,
    /// Where to stand; None: close on the target.
    pub point: Option<(f64, f64)>,
    pub note: String,
}

impl<K> Order<K> {
    fn new(mode: OrderMode, target: Option<K>, point: Option<(f64, f64)>, note: &str) -> Self {
        Self { mode, target, point, note: note.to_string() }
    }
}

/// The squad's doctrine. Python stores the name as a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Doctrine {
    #[default]
    None,
    FallBack,
    Regroup,
    Perimeter,
    BaitAndAmbush,
    FocusFire,
    HammerAndAnvil,
    Screen,
    Encircle,
}

impl Doctrine {
    pub fn as_str(self) -> &'static str {
        match self {
            Doctrine::None => "none",
            Doctrine::FallBack => "fall back",
            Doctrine::Regroup => "regroup",
            Doctrine::Perimeter => "perimeter",
            Doctrine::BaitAndAmbush => "bait and ambush",
            Doctrine::FocusFire => "focus fire",
            Doctrine::HammerAndAnvil => "hammer and anvil",
            Doctrine::Screen => "screen",
            Doctrine::Encircle => "encircle",
        }
    }
}

/// `"defend"` a post at the anchor, or `"attack"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Stance {
    #[default]
    Defend,
    Attack,
}

impl Stance {
    pub fn as_str(self) -> &'static str {
        match self {
            Stance::Defend => "defend",
            Stance::Attack => "attack",
        }
    }

    pub fn parse(name: &str) -> Option<Stance> {
        match name {
            "defend" => Some(Stance::Defend),
            "attack" => Some(Stance::Attack),
            _ => None,
        }
    }
}

/// A squad's doctrine and every member's order. `orders` keeps Python's
/// dict insertion order; members given no order fight on their own.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan<K> {
    pub doctrine: Doctrine,
    pub orders: Vec<(K, Order<K>)>,
    pub focus: Option<K>,
}

impl<K> Default for Plan<K> {
    fn default() -> Self {
        Self { doctrine: Doctrine::None, orders: Vec::new(), focus: None }
    }
}

impl<K: PartialEq> Plan<K> {
    /// The order for `key`, if it has one.
    pub fn order(&self, key: &K) -> Option<&Order<K>> {
        self.orders.iter().find(|(k, _)| k == key).map(|(_, o)| o)
    }

    /// `orders[key] = order`: replaces in place, else appends.
    fn set(&mut self, key: K, order: Order<K>) {
        if let Some(slot) = self.orders.iter_mut().find(|(k, _)| *k == key) {
            slot.1 = order;
        } else {
            self.orders.push((key, order));
        }
    }
}

fn centroid(points: impl IntoIterator<Item = (f64, f64)>) -> (f64, f64) {
    let points: Vec<(f64, f64)> = points.into_iter().collect();
    if points.is_empty() {
        return (0.0, 0.0);
    }
    let n = points.len() as f64;
    (points.iter().map(|p| p.0).sum::<f64>() / n, points.iter().map(|p| p.1).sum::<f64>() / n)
}

fn unit(dx: f64, dy: f64) -> (f64, f64) {
    let d = dx.hypot(dy);
    if d > 1e-6 { (dx / d, dy / d) } else { (1.0, 0.0) }
}

/// Index of the first minimum of `key` (Python's `min(seq, key=...)`).
fn argmin<T>(items: &[T], key: impl Fn(&T) -> f64) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (i, item) in items.iter().enumerate() {
        let k = key(item);
        if best.is_none_or(|(_, bk)| k < bk) {
            best = Some((i, k));
        }
    }
    best.map(|(i, _)| i)
}

/// The foe the squad should all go for: close to it, already hurt, and
/// dangerous. A hero counts double through its threat.
pub fn choose_focus<'a, K>(members: &[&Member<K>], foes: &'a [&'a Foe<K>]) -> Option<&'a Foe<K>> {
    let (cx, cy) = centroid(members.iter().map(|m| (m.x, m.y)));
    let score = |foe: &&Foe<K>| {
        let distance = (foe.x - cx).hypot(foe.y - cy);
        distance * (0.55 + foe.health) / 0.2_f64.max(foe.threat)
    };
    argmin(foes, score).map(|i| foes[i])
}

/// Orders for one squad. `stance`: defend a post at `anchor`, or attack.
/// `notice`: how far a defending squad watches beyond its post (for the bait
/// at level 3). `anchor` None: the members' centroid.
pub fn plan<K: Clone + PartialEq>(
    members: &[Member<K>],
    foes: &[Foe<K>],
    level: i32,
    stance: Stance,
    anchor: Option<(f64, f64)>,
    notice: f64,
) -> Plan<K> {
    let mut result = Plan::default();
    if level <= 0 || members.len() < 2 {
        return result;
    }
    let anchor = anchor.unwrap_or_else(|| centroid(members.iter().map(|m| (m.x, m.y))));

    // Level 1 and up: the badly hurt fall back to heal, unless they are the
    // squad's wall.
    let mut fighting: Vec<&Member<K>> = Vec::new();
    for m in members {
        match m.anchor {
            Some(post) if m.health < RETREAT_HEALTH && m.role != Role::Tank => {
                result.set(m.key.clone(), Order::new(OrderMode::Retreat, None, Some(post), "falls back to heal"));
            }
            _ => fighting.push(m),
        }
    }
    if fighting.len() < 2 {
        result.doctrine = if result.orders.is_empty() { Doctrine::None } else { Doctrine::FallBack };
        return result;
    }

    let from_anchor = |f: &Foe<K>| (f.x - anchor.0).hypot(f.y - anchor.1);
    let engaged: Vec<&Foe<K>> = match stance {
        Stance::Defend => foes.iter().filter(|f| from_anchor(f) < DEFEND_RADIUS).collect(),
        Stance::Attack => foes.iter().collect(),
    };

    if engaged.is_empty() {
        if stance == Stance::Defend && level >= 3 {
            let lure: Vec<&Foe<K>> = foes.iter().filter(|f| from_anchor(f) < DEFEND_RADIUS + notice).collect();
            let fast = fighting.iter().position(|m| m.role == Role::Fast);
            if let Some(bait) = fast
                && !lure.is_empty()
                && fighting.len() >= 3
            {
                bait_and_ambush(&mut result, &fighting, &lure, bait, anchor);
                return result;
            }
        }
        if stance == Stance::Defend && level >= 2 {
            perimeter(&mut result, &fighting, anchor, foes);
            return result;
        }
        if stance == Stance::Attack && level >= 2 {
            let mut spread = f64::NEG_INFINITY;
            for m in &fighting {
                for n in &fighting {
                    spread = spread.max((m.x - n.x).hypot(m.y - n.y));
                }
            }
            if spread > REGROUP_SPREAD {
                let rally = centroid(fighting.iter().map(|m| (m.x, m.y)));
                for m in &fighting {
                    result.set(
                        m.key.clone(),
                        Order::new(OrderMode::Regroup, None, Some(rally), "gathers before the attack"),
                    );
                }
                result.doctrine = Doctrine::Regroup;
            }
        }
        return result;
    }

    let focus = choose_focus(&fighting, &engaged).expect("engaged is not empty");
    result.focus = Some(focus.key.clone());
    let melee: Vec<&Member<K>> = fighting.iter().copied().filter(|m| m.role.is_melee()).collect();
    let ranged: Vec<&Member<K>> = fighting.iter().copied().filter(|m| m.role.is_ranged()).collect();
    let has_tank = melee.iter().any(|m| m.role == Role::Tank);
    let has_fast = melee.iter().any(|m| m.role == Role::Fast);

    if level >= 3 && melee.len() >= 3 {
        encircle(&mut result, &melee, focus);
        kite(&mut result, &ranged, focus, &melee);
        result.doctrine = Doctrine::Encircle;
    } else if level >= 2 && has_tank && has_fast {
        let tanks: Vec<&Member<K>> = melee.iter().copied().filter(|m| m.role == Role::Tank).collect();
        let others: Vec<&Member<K>> = melee.iter().copied().filter(|m| m.role != Role::Tank).collect();
        hammer_and_anvil(&mut result, &tanks, &others, focus);
        kite(&mut result, &ranged, focus, &melee);
        result.doctrine = Doctrine::HammerAndAnvil;
    } else if level >= 2 && !ranged.is_empty() && !melee.is_empty() {
        screen(&mut result, &melee, &ranged, focus);
        result.doctrine = Doctrine::Screen;
    } else {
        for m in &fighting {
            result.set(m.key.clone(), Order::new(OrderMode::Engage, Some(focus.key.clone()), None, "focus fire"));
        }
        result.doctrine = Doctrine::FocusFire;
    }
    result
}

// -- the doctrines ------------------------------------------------------------

/// Close-fighters take evenly spaced places round the foe, starting from the
/// side they come from, so it cannot face them all.
fn encircle<K: Clone + PartialEq>(result: &mut Plan<K>, melee: &[&Member<K>], focus: &Foe<K>) {
    let (cx, cy) = centroid(melee.iter().map(|m| (m.x, m.y)));
    let start = (cy - focus.y).atan2(cx - focus.x);
    let count = melee.len();
    let mut free: Vec<&Member<K>> = melee.to_vec();
    for i in 0..count {
        let angle = start + i as f64 * TAU / count as f64;
        // The nearest free member takes each place.
        let (px, py) = (focus.x + angle.cos() * 60.0, focus.y + angle.sin() * 60.0);
        let pick = argmin(&free, |m| (m.x - px).hypot(m.y - py)).expect("one free member per slot");
        let m = free.remove(pick);
        result
            .set(m.key.clone(), Order::new(OrderMode::Encircle, Some(focus.key.clone()), Some((px, py)), "surrounds"));
    }
}

/// The tank meets the foe head on; the rest swing round to its sides and
/// back while it is busy.
fn hammer_and_anvil<K: Clone + PartialEq>(
    result: &mut Plan<K>,
    tanks: &[&Member<K>],
    others: &[&Member<K>],
    focus: &Foe<K>,
) {
    for t in tanks {
        result.set(t.key.clone(), Order::new(OrderMode::Engage, Some(focus.key.clone()), None, "holds the front"));
    }
    let (tx, ty) = centroid(tanks.iter().map(|t| (t.x, t.y)));
    let (fx, fy) = unit(focus.x - tx, focus.y - ty); // from the tank through the foe
    for (index, m) in others.iter().enumerate() {
        let side = if index % 2 == 0 { 1.0 } else { -1.0 };
        // Beside and behind the foe, as seen from the tank.
        let px = focus.x + fx * 38.0 + (-fy) * side * 58.0;
        let py = focus.y + fy * 38.0 + fx * side * 58.0;
        result.set(m.key.clone(), Order::new(OrderMode::Flank, Some(focus.key.clone()), Some((px, py)), "flanks"));
    }
}

/// Close-fighters stand between the foe and the shooters, who shoot from
/// behind them.
fn screen<K: Clone + PartialEq>(result: &mut Plan<K>, melee: &[&Member<K>], ranged: &[&Member<K>], focus: &Foe<K>) {
    let (rx, ry) = centroid(ranged.iter().map(|m| (m.x, m.y)));
    let (ux, uy) = unit(rx - focus.x, ry - focus.y); // from the foe towards the shooters
    for (index, m) in melee.iter().enumerate() {
        let offset = (index as f64 - (melee.len() as f64 - 1.0) / 2.0) * 34.0;
        let px = focus.x + ux * 46.0 + (-uy) * offset;
        let py = focus.y + uy * 46.0 + ux * offset;
        result.set(
            m.key.clone(),
            Order::new(OrderMode::Screen, Some(focus.key.clone()), Some((px, py)), "screens the shooters"),
        );
    }
    kite(result, ranged, focus, melee);
}

/// Shooters keep their range from the foe, on the far side of the
/// close-fighters.
fn kite<K: Clone + PartialEq>(result: &mut Plan<K>, ranged: &[&Member<K>], focus: &Foe<K>, melee: &[&Member<K>]) {
    if ranged.is_empty() {
        return;
    }
    let (ux, uy) = if !melee.is_empty() {
        let (mx, my) = centroid(melee.iter().map(|m| (m.x, m.y)));
        unit(mx - focus.x, my - focus.y)
    } else {
        let (rx, ry) = centroid(ranged.iter().map(|m| (m.x, m.y)));
        unit(rx - focus.x, ry - focus.y)
    };
    for (index, m) in ranged.iter().enumerate() {
        let spread = (index as f64 - (ranged.len() as f64 - 1.0) / 2.0) * 0.5;
        let (c, s) = (spread.cos(), spread.sin());
        let (dx, dy) = (ux * c - uy * s, ux * s + uy * c);
        let distance = 110.0_f64.max(m.reach * 0.8);
        result.set(
            m.key.clone(),
            Order::new(
                OrderMode::Kite,
                Some(focus.key.clone()),
                Some((focus.x + dx * distance, focus.y + dy * distance)),
                "shoots from range",
            ),
        );
    }
}

/// Nobody close: stand round the post, the wall on the side the foes are.
fn perimeter<K: Clone + PartialEq>(result: &mut Plan<K>, members: &[&Member<K>], anchor: (f64, f64), foes: &[Foe<K>]) {
    let (ax, ay) = anchor;
    let facing = match argmin(foes, |f| (f.x - ax).hypot(f.y - ay)) {
        Some(i) => (foes[i].y - ay).atan2(foes[i].x - ax),
        None => PI,
    };
    let mut ordered: Vec<&Member<K>> = members.to_vec();
    // Stable, like Python's sorted: tanks first, shooters last.
    ordered.sort_by_key(|m| (m.role != Role::Tank, m.role.is_ranged()));
    let n = ordered.len() as f64;
    for (index, m) in ordered.iter().enumerate() {
        let i = index as f64;
        let (angle, radius) = if m.role.is_ranged() {
            (facing + PI + (i - n / 2.0) * 0.5, 55.0)
        } else {
            (facing + (i - (n - 1.0) / 2.0) * 0.7, 95.0)
        };
        result.set(
            m.key.clone(),
            Order::new(
                OrderMode::Hold,
                None,
                Some((ax + angle.cos() * radius, ay + angle.sin() * radius)),
                "holds the perimeter",
            ),
        );
    }
    result.doctrine = Doctrine::Perimeter;
}

/// A fast one goes out to draw the foe in; the rest wait round the post.
fn bait_and_ambush<K: Clone + PartialEq>(
    result: &mut Plan<K>,
    members: &[&Member<K>],
    lure: &[&Foe<K>],
    bait: usize,
    anchor: (f64, f64),
) {
    let (ax, ay) = anchor;
    let foe = lure[argmin(lure, |f| (f.x - ax).hypot(f.y - ay)).expect("lure is not empty")];
    result.focus = Some(foe.key.clone());
    let (ux, uy) = unit(foe.x - ax, foe.y - ay);
    // Close enough to be seen and chased, not so close it is caught.
    result.set(
        members[bait].key.clone(),
        Order::new(
            OrderMode::Bait,
            Some(foe.key.clone()),
            Some((foe.x - ux * 120.0, foe.y - uy * 120.0)),
            "lures a foe in",
        ),
    );
    let rest = members.iter().enumerate().filter(|(i, _)| *i != bait).map(|(_, m)| *m);
    for (index, m) in rest.enumerate() {
        let side = if index % 2 == 0 { 1.0 } else { -1.0 };
        let angle = uy.atan2(ux) + side * (0.9 + 0.3 * (index / 2) as f64);
        result.set(
            m.key.clone(),
            Order::new(
                OrderMode::Ambush,
                Some(foe.key.clone()),
                Some((ax + angle.cos() * 80.0, ay + angle.sin() * 80.0)),
                "lies in wait",
            ),
        );
    }
    result.doctrine = Doctrine::BaitAndAmbush;
}

/// A map's squad skill from its tier: none on the first maps.
pub fn tactics_level(tier: i32) -> i32 {
    (tier - 1).clamp(0, 3)
}

#[cfg(test)]
mod tests {
    //! Port of the squad-tactics part of tests/test_enemy_tactics.py.
    use super::*;

    fn members(specs: &[(&'static str, f64, f64, Role)]) -> Vec<Member<&'static str>> {
        specs
            .iter()
            .map(|&(name, x, y, role)| Member {
                health: 1.0,
                role,
                reach: if role.is_ranged() { 180.0 } else { 34.0 },
                anchor: Some((0.0, 0.0)),
                ..Member::new(name, x, y)
            })
            .collect()
    }

    fn hero() -> Foe<&'static str> {
        Foe { key: "hero", x: 200.0, y: 0.0, health: 1.0, threat: 1.6 }
    }

    fn plan1(squad: &[Member<&'static str>], foes: &[Foe<&'static str>], level: i32) -> Plan<&'static str> {
        plan(squad, foes, level, Stance::Defend, None, DEFAULT_NOTICE)
    }

    #[test]
    fn no_teamwork_on_the_first_maps() {
        let squad = members(&[("a", 0.0, 0.0, Role::Melee), ("b", 10.0, 10.0, Role::Melee)]);
        assert!(plan1(&squad, &[hero()], 0).orders.is_empty());
        assert_eq!(tactics_level(1), 0);
        assert_eq!(tactics_level(4), 3);
    }

    #[test]
    fn focus_fire_sends_everyone_at_the_same_foe() {
        let squad = members(&[("a", 0.0, 0.0, Role::Melee), ("b", 10.0, 10.0, Role::Melee)]);
        let weak = Foe { key: "scout", x: 150.0, y: 40.0, health: 0.2, threat: 1.0 };
        let result = plan1(&squad, &[hero(), weak], 1);
        assert_eq!(result.doctrine, Doctrine::FocusFire);
        assert!(result.orders.iter().all(|(_, o)| o.target == result.focus));
    }

    #[test]
    fn hammer_and_anvil_tank_in_front_fast_ones_on_the_flanks() {
        let squad =
            members(&[("tank", 0.0, 0.0, Role::Tank), ("f1", 0.0, 30.0, Role::Fast), ("f2", 0.0, -30.0, Role::Fast)]);
        let result = plan1(&squad, &[hero()], 2);
        assert_eq!(result.doctrine, Doctrine::HammerAndAnvil);
        assert_eq!(result.order(&"tank").unwrap().mode, OrderMode::Engage);
        let p1 = result.order(&"f1").unwrap().point.unwrap();
        let p2 = result.order(&"f2").unwrap().point.unwrap();
        assert_eq!(result.order(&"f1").unwrap().mode, OrderMode::Flank);
        assert!(p1.1 * p2.1 < 0.0, "one on each side");
        assert!(p1.0 > hero().x - 10.0 && p2.0 > hero().x - 10.0);
    }

    #[test]
    fn screen_keeps_the_shooters_behind_the_close_fighters() {
        let squad = members(&[("m", 60.0, 0.0, Role::Melee), ("r", 0.0, 0.0, Role::Ranged)]);
        let result = plan1(&squad, &[hero()], 2);
        assert_eq!(result.doctrine, Doctrine::Screen);
        let h = hero();
        let front = result.order(&"m").unwrap().point.unwrap();
        let back = result.order(&"r").unwrap().point.unwrap();
        assert!((front.0 - h.x).hypot(front.1 - h.y) < (back.0 - h.x).hypot(back.1 - h.y));
        assert_eq!(result.order(&"r").unwrap().mode, OrderMode::Kite);
    }

    #[test]
    fn encircle_puts_three_close_fighters_round_a_foe() {
        let squad =
            members(&[("a", 0.0, 0.0, Role::Melee), ("b", 0.0, 40.0, Role::Melee), ("c", 0.0, -40.0, Role::Tank)]);
        let result = plan1(&squad, &[hero()], 3);
        assert_eq!(result.doctrine, Doctrine::Encircle);
        let h = hero();
        let mut angles: Vec<f64> = result
            .orders
            .iter()
            .map(|(_, o)| {
                let p = o.point.unwrap();
                (p.1 - h.y).atan2(p.0 - h.x)
            })
            .collect();
        angles.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = angles.len();
        let min_gap =
            (0..n).map(|i| dc_core::math::py_mod(angles[(i + 1) % n] - angles[i], TAU)).fold(f64::INFINITY, f64::min);
        assert!(min_gap > 1.5, "spread round the foe");
    }

    #[test]
    fn defenders_hold_a_perimeter_and_the_advanced_ones_bait_and_ambush() {
        let far_foe = Foe::new("hero", 600.0, 0.0);
        let squad = members(&[
            ("tank", 0.0, 0.0, Role::Tank),
            ("fast", 20.0, 0.0, Role::Fast),
            ("r", -20.0, 0.0, Role::Ranged),
        ]);
        let held = plan(&squad, std::slice::from_ref(&far_foe), 2, Stance::Defend, Some((0.0, 0.0)), DEFAULT_NOTICE);
        assert_eq!(held.doctrine, Doctrine::Perimeter);
        assert!(held.orders.iter().all(|(_, o)| o.mode == OrderMode::Hold));
        let lure = plan(&squad, &[far_foe], 3, Stance::Defend, Some((0.0, 0.0)), DEFAULT_NOTICE);
        assert_eq!(lure.doctrine, Doctrine::BaitAndAmbush);
        assert_eq!(lure.order(&"fast").unwrap().mode, OrderMode::Bait);
        assert_eq!(lure.order(&"tank").unwrap().mode, OrderMode::Ambush);
    }

    #[test]
    fn the_badly_hurt_fall_back_and_a_scattered_attack_regroups() {
        let mut squad =
            members(&[("a", 0.0, 0.0, Role::Melee), ("b", 10.0, 0.0, Role::Melee), ("c", 20.0, 0.0, Role::Melee)]);
        squad[0].health = 0.1;
        let result = plan1(&squad, &[hero()], 1);
        assert_eq!(result.order(&"a").unwrap().mode, OrderMode::Retreat);
        let spread = members(&[("a", 0.0, 0.0, Role::Melee), ("b", 600.0, 0.0, Role::Melee)]);
        let regroup = plan(&spread, &[], 2, Stance::Attack, None, DEFAULT_NOTICE);
        assert_eq!(regroup.doctrine, Doctrine::Regroup);
    }
}
