//! Port of app/map_layouts.py
//!
//! Where each map's buildings stand, and which buildings it has. A building
//! has a place on the main screen as fractions of it, or on the **far**
//! screen -- the other monitor -- with a fallback place on the main screen
//! for players with only one. Some buildings exist only when there is a
//! second screen.
//!
//! Building kinds and what holding one does, for whichever side holds it:
//!
//! - `home`: heals and refills silk (yours from the start);
//! - `food`: heals from a supply; `silk`: more silk and fast refills;
//! - `hatchery`: sends enemy reinforcements until you seal it;
//! - `nest`: the enemy's home; claim it last, after its guardian;
//! - `venom`: the holder's side deals 20% more damage;
//! - `lookout`: the holder's side shoots silk and acid 30% farther;
//! - `amber`: mines amber for you while you hold it;
//! - `nursery`: hatches spiderlings for its holder, two at a time.

use std::sync::LazyLock;

use crate::custom_maps::SpiderSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BuildingKind {
    Home,
    Food,
    Silk,
    Hatchery,
    Nest,
    Venom,
    Lookout,
    Amber,
    Nursery,
}

impl BuildingKind {
    pub const ALL: [BuildingKind; 9] = [
        BuildingKind::Home,
        BuildingKind::Food,
        BuildingKind::Silk,
        BuildingKind::Hatchery,
        BuildingKind::Nest,
        BuildingKind::Venom,
        BuildingKind::Lookout,
        BuildingKind::Amber,
        BuildingKind::Nursery,
    ];

    pub fn id(self) -> &'static str {
        match self {
            BuildingKind::Home => "home",
            BuildingKind::Food => "food",
            BuildingKind::Silk => "silk",
            BuildingKind::Hatchery => "hatchery",
            BuildingKind::Nest => "nest",
            BuildingKind::Venom => "venom",
            BuildingKind::Lookout => "lookout",
            BuildingKind::Amber => "amber",
            BuildingKind::Nursery => "nursery",
        }
    }

    pub fn from_id(id: &str) -> Option<BuildingKind> {
        Self::ALL.iter().copied().find(|k| k.id() == id)
    }

    /// What holding a building gives, for the status line under it.
    pub fn effect(self) -> Option<&'static str> {
        EFFECTS.iter().find(|(k, _)| *k == self).map(|(_, t)| *t)
    }
}

/// Who guards a building: a role name on the built-in maps, a full spider
/// spec on a map made in the editor.
#[derive(Debug, Clone, PartialEq)]
pub enum Guard {
    Role(String),
    Spec(SpiderSpec),
}

impl Guard {
    /// The role it fights as.
    pub fn role(&self) -> &str {
        match self {
            Guard::Role(r) => r,
            Guard::Spec(s) => &s.role,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub kind: BuildingKind,
    pub name: String,
    pub fx: f64,
    pub fy: f64,
    pub guards: Vec<Guard>,
    pub reserves: i64,
    pub owned: bool,
    /// On the other screen when there is one.
    pub far: bool,
    /// Its main-screen place without one; None: skipped.
    pub alt: Option<(f64, f64)>,
    /// Healing a Food cache holds.
    pub supply: f64,
}

impl Placement {
    /// The Python defaults: no guards, no reserves, not owned, main screen,
    /// supply 180.
    pub fn new(kind: BuildingKind, name: &str, fx: f64, fy: f64) -> Self {
        Self {
            kind,
            name: name.to_string(),
            fx,
            fy,
            guards: Vec::new(),
            reserves: 0,
            owned: false,
            far: false,
            alt: None,
            supply: 180.0,
        }
    }

    fn guards(mut self, roles: &[&str]) -> Self {
        self.guards = roles.iter().map(|r| Guard::Role(r.to_string())).collect();
        self
    }

    fn reserves(mut self, n: i64) -> Self {
        self.reserves = n;
        self
    }

    fn owned(mut self) -> Self {
        self.owned = true;
        self
    }

    fn far(mut self, alt: Option<(f64, f64)>) -> Self {
        self.far = true;
        self.alt = alt;
        self
    }
}

/// What holding a building gives, for the status line under it.
pub const EFFECTS: [(BuildingKind, &str); 4] = [
    (BuildingKind::Venom, "+20% damage"),
    (BuildingKind::Lookout, "+30% silk and acid range"),
    (BuildingKind::Amber, "Mines amber"),
    (BuildingKind::Nursery, "Hatches spiderlings"),
];

pub const KINDS_WITH_EFFECTS: [BuildingKind; 4] =
    [BuildingKind::Venom, BuildingKind::Lookout, BuildingKind::Amber, BuildingKind::Nursery];

/// Every raid map's buildings, by map id, in Python's order.
pub static LAYOUTS: LazyLock<Vec<(&'static str, Vec<Placement>)>> = LazyLock::new(|| {
    use BuildingKind::*;
    let p = Placement::new;
    vec![
        // The first map keeps its familiar layout; a second screen adds a mine.
        (
            "territory",
            vec![
                p(Home, "Home burrow", 0.16, 0.57).owned(),
                p(Food, "Food cache", 0.39, 0.29).guards(&["guard"]),
                p(Silk, "Silk loom", 0.39, 0.70),
                p(Hatchery, "Hatchery", 0.64, 0.47).guards(&["hunter"]).reserves(4),
                p(Nest, "Thorn nest", 0.83, 0.30),
                p(Amber, "Amber mine", 0.50, 0.55).far(None),
            ],
        ),
        // Ember hollow: home high on the left, a venom den to fight over, the
        // nest across on the other screen.
        (
            "ember",
            vec![
                p(Home, "Home burrow", 0.10, 0.22).owned(),
                p(Venom, "Venom den", 0.28, 0.78).guards(&["guard"]),
                p(Food, "Food cache", 0.46, 0.20).guards(&["hunter"]),
                p(Hatchery, "Hatchery", 0.60, 0.66).guards(&["hunter", "guard"]).reserves(7),
                p(Lookout, "Lookout", 0.80, 0.80).guards(&["weaver"]),
                p(Nest, "Hollow nest", 0.60, 0.40).far(Some((0.88, 0.26))),
                p(Amber, "Amber mine", 0.25, 0.70).guards(&["guard"]).far(None),
            ],
        ),
        // Obsidian deep: home low on the left; the hatchery and nest both on
        // the far screen, a nursery in the middle.
        (
            "obsidian",
            vec![
                p(Home, "Home burrow", 0.12, 0.82).owned(),
                p(Silk, "Silk loom", 0.28, 0.30).guards(&["weaver", "guard"]),
                p(Nursery, "Nursery", 0.50, 0.68).guards(&["guard", "hunter"]),
                p(Amber, "Amber mine", 0.66, 0.20).guards(&["guard"]),
                p(Hatchery, "Hatchery", 0.30, 0.60)
                    .guards(&["guard", "hunter", "weaver"])
                    .reserves(8)
                    .far(Some((0.74, 0.58))),
                p(Nest, "Obsidian nest", 0.75, 0.30).far(Some((0.90, 0.84))),
                p(Lookout, "Lookout", 0.86, 0.45).guards(&["weaver", "hunter"]),
            ],
        ),
        // Queen of thorns: home at the bottom middle, every kind of building
        // round it, the queen's nest across on the far screen.
        (
            "queen",
            vec![
                p(Home, "Home burrow", 0.50, 0.92).owned(),
                p(Food, "Food cache", 0.18, 0.60).guards(&["guard", "hunter"]),
                p(Silk, "Silk loom", 0.82, 0.60).guards(&["weaver", "guard"]),
                p(Venom, "Venom den", 0.30, 0.16).guards(&["hunter", "hunter", "guard"]),
                p(Lookout, "Lookout", 0.70, 0.16).guards(&["weaver", "weaver", "guard"]),
                p(Nursery, "Nursery", 0.50, 0.45).guards(&["guard", "hunter"]),
                p(Hatchery, "Hatchery", 0.30, 0.62)
                    .guards(&["hunter", "guard", "weaver"])
                    .reserves(9)
                    .far(Some((0.10, 0.22))),
                p(Nest, "Queen's nest", 0.66, 0.38).far(Some((0.90, 0.22))),
                p(Amber, "Amber mine", 0.20, 0.25).guards(&["guard"]).far(None),
            ],
        ),
    ]
});

/// `LAYOUTS[map_id]`, if the map has a layout of its own.
pub fn layout(map_id: &str) -> Option<&'static [Placement]> {
    LAYOUTS.iter().find(|(id, _)| *id == map_id).map(|(_, l)| l.as_slice())
}

/// The buildings of `map_id`; the first map's when it has none of its own.
pub fn layout_for(map_id: &str) -> &'static [Placement] {
    layout(map_id).unwrap_or_else(|| layout("territory").expect("the first map has a layout"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_layout_has_a_home_hatchery_and_nest() {
        for (id, places) in LAYOUTS.iter() {
            for kind in [BuildingKind::Home, BuildingKind::Hatchery, BuildingKind::Nest] {
                assert_eq!(places.iter().filter(|p| p.kind == kind).count(), 1, "{id} {kind:?}");
            }
            let home = places.iter().find(|p| p.kind == BuildingKind::Home).unwrap();
            assert!(home.owned && !home.far);
        }
        assert_eq!(layout_for("swarm"), layout_for("territory"));
        assert_eq!(BuildingKind::Venom.effect(), Some("+20% damage"));
        assert_eq!(BuildingKind::Home.effect(), None);
    }
}
