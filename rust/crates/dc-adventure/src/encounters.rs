//! Port of app/encounters.py
//!
//! Where extra enemies come from, by the kind of mission and the screens
//! there are. A mission names an **encounter profile** ("raid", "reclaim",
//! ...). The director reads it with the monitor layout and the map's tier and
//! says:
//!
//! - which **outposts** to place -- one on each extra screen, and for some
//!   mission kinds on the main screen too -- with their defenders;
//! - when an outpost sends **reinforcements** and who they are.
//!
//! The mission owns the spiders; the director only plans. A new scenario is a
//! new [`EncounterProfile`], not new spawning code.
//!
//! Python's director also takes an `rng` it never draws from; the port leaves
//! it out.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use dc_world::{Screen, ScreenLayout};

/// Screen area, in pixels, that earns one more defender.
pub const AREA_PER_EXTRA_DEFENDER: f64 = 2_600_000.0;

#[derive(Debug, Clone, PartialEq)]
pub struct EncounterProfile {
    pub kind: String,
    /// The site kind (its art and rules).
    pub outpost_kind: String,
    /// One per outpost, cycled.
    pub outpost_names: Vec<String>,
    /// Roles guarding every outpost.
    pub defenders: Vec<String>,
    /// One more per map tier above the first, cycled.
    pub tier_defenders: Vec<String>,
    /// Who an outpost sends, cycled.
    pub wave_roles: Vec<String>,
    /// Seconds between an outpost's waves.
    pub wave_every: f64,
    /// Waves an outpost holds.
    pub reserves: i64,
    /// Seconds of "emerging" before a wave.
    pub warning: f64,
    /// Outposts on the main screen as well.
    pub on_main_screen: usize,
    /// Where on a screen outposts stand, as fractions of it, tried in order.
    pub spots: Vec<(f64, f64)>,
}

pub const DEFAULT_SPOTS: [(f64, f64); 4] = [(0.55, 0.45), (0.30, 0.70), (0.78, 0.68), (0.25, 0.30)];

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

impl EncounterProfile {
    /// A profile with Python's defaults for `warning`, `on_main_screen` and
    /// `spots`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kind: &str,
        outpost_kind: &str,
        outpost_names: &[&str],
        defenders: &[&str],
        tier_defenders: &[&str],
        wave_roles: &[&str],
        wave_every: f64,
        reserves: i64,
    ) -> Self {
        Self {
            kind: kind.into(),
            outpost_kind: outpost_kind.into(),
            outpost_names: strings(outpost_names),
            defenders: strings(defenders),
            tier_defenders: strings(tier_defenders),
            wave_roles: strings(wave_roles),
            wave_every,
            reserves,
            warning: 3.0,
            on_main_screen: 0,
            spots: DEFAULT_SPOTS.to_vec(),
        }
    }
}

/// `PROFILES`, in Python's order.
pub static PROFILES: LazyLock<Vec<EncounterProfile>> = LazyLock::new(|| {
    vec![
        // The territory maps: the main screen keeps its five buildings; every
        // other screen gets an optional outpost that raids you until taken.
        EncounterProfile::new(
            "raid",
            "outpost",
            &["Far burrow", "Border web", "Distant den"],
            &["guard", "weaver"],
            &["hunter", "guard", "weaver"],
            &["hunter", "guard"],
            38.0,
            3,
        ),
        // Reclaim the desktop: every screen is infested, the main one included.
        EncounterProfile {
            on_main_screen: 1,
            spots: vec![(0.72, 0.42), (0.40, 0.62), (0.62, 0.72), (0.30, 0.35)],
            ..EncounterProfile::new(
                "reclaim",
                "infestation",
                &["Acid nest", "Glass burrow", "Pixel hive", "Void den"],
                &["spitter", "hunter"],
                &["spitter", "guard", "hunter"],
                &["spitter", "hunter", "guard"],
                30.0,
                4,
            )
        },
        // Fly swarms: rival dens on the other screens send more fly hunters.
        EncounterProfile {
            spots: vec![(0.30, 0.30), (0.70, 0.30), (0.50, 0.80)],
            ..EncounterProfile::new(
                "swarm",
                "outpost",
                &["Rival den", "Hunters' hole", "Far den"],
                &["hunter"],
                &["hunter", "weaver"],
                &["hunter"],
                34.0,
                2,
            )
        },
    ]
});

/// `PROFILES[kind]`.
pub fn profile(kind: &str) -> Option<&'static EncounterProfile> {
    PROFILES.iter().find(|p| p.kind == kind)
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutpostPlan {
    pub screen: usize,
    pub x: f64,
    pub y: f64,
    pub kind: String,
    pub name: String,
    pub defenders: Vec<String>,
    pub reserves: i64,
}

/// A live outpost site as the director sees it: Python reads `owned` and
/// `reserves` and writes `warning`.
pub trait OutpostSite {
    fn owned(&self) -> bool;
    fn reserves(&self) -> i64;
    fn set_warning(&mut self, warning: f64);
}

impl<T: OutpostSite + ?Sized> OutpostSite for &mut T {
    fn owned(&self) -> bool {
        (**self).owned()
    }
    fn reserves(&self) -> i64 {
        (**self).reserves()
    }
    fn set_warning(&mut self, warning: f64) {
        (**self).set_warning(warning)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Clock {
    time: f64,
    warning: f64,
}

impl Clock {
    fn new(time: f64) -> Self {
        Self { time, warning: 0.0 }
    }
}

#[derive(Debug, Clone)]
pub struct EncounterDirector {
    pub profile: EncounterProfile,
    pub layout: ScreenLayout,
    pub tier: i64,
    clocks: BTreeMap<usize, Clock>,
    waves_sent: BTreeMap<usize, usize>,
}

impl EncounterDirector {
    pub fn new(profile: EncounterProfile, layout: ScreenLayout, tier: i64) -> Self {
        Self { profile, layout, tier, clocks: BTreeMap::new(), waves_sent: BTreeMap::new() }
    }

    // -- placing -----------------------------------------------------------

    /// The outposts for this layout: one per extra screen, plus the main
    /// screen's share. `avoid` holds points (the mission's own buildings) an
    /// outpost must keep `clearance` from (Python's default 240).
    pub fn outposts(&mut self, avoid: &[(f64, f64)], clearance: f64) -> Vec<OutpostPlan> {
        let p = &self.profile;
        let mut plans: Vec<OutpostPlan> = Vec::new();
        let mut screens: Vec<(Screen, usize)> = self.layout.others().into_iter().map(|s| (s, 1)).collect();
        if p.on_main_screen > 0 {
            screens.insert(0, (*self.layout.primary(), p.on_main_screen));
        }
        let mut taken: Vec<(f64, f64)> = avoid.to_vec();
        for (screen, count) in &screens {
            for _ in 0..*count {
                let Some(spot) = self.free_spot(screen, &taken, clearance) else {
                    continue;
                };
                taken.push(spot);
                let name = p.outpost_names[plans.len() % p.outpost_names.len()].clone();
                plans.push(OutpostPlan {
                    screen: screen.index,
                    x: spot.0,
                    y: spot.1,
                    kind: p.outpost_kind.clone(),
                    name,
                    defenders: self.defenders_for(screen),
                    reserves: p.reserves,
                });
            }
        }
        for index in 0..plans.len() {
            // Staggered, so outposts do not all send their waves at once.
            self.clocks.insert(index, Clock::new(self.profile.wave_every * (0.6 + 0.25 * index as f64)));
        }
        plans
    }

    /// `outposts()` with Python's defaults (nothing to avoid, clearance 240).
    pub fn outposts_default(&mut self) -> Vec<OutpostPlan> {
        self.outposts(&[], 240.0)
    }

    fn free_spot(&self, screen: &Screen, taken: &[(f64, f64)], clearance: f64) -> Option<(f64, f64)> {
        let margin = 110.0;
        for &(fx, fy) in &self.profile.spots {
            let (x, y) = screen.anchor(fx, fy, margin);
            if taken.iter().all(|&(tx, ty)| (x - tx).powi(2) + (y - ty).powi(2) >= clearance.powi(2)) {
                return Some((x, y));
            }
        }
        None
    }

    /// The roles guarding an outpost on `screen`: one on the first maps,
    /// one more per tier later, and a big screen at most one more per tier
    /// above the first.
    pub fn defenders_for(&self, screen: &Screen) -> Vec<String> {
        let p = &self.profile;
        let mut roles: Vec<String> =
            if self.tier <= 1 { p.defenders.iter().take(1).cloned().collect() } else { p.defenders.clone() };
        let above = (self.tier - 1).max(0) as usize;
        for i in 0..above {
            roles.push(p.tier_defenders[i % p.tier_defenders.len()].clone());
        }
        let by_area = dc_core::math::py_floordiv(screen.rect.w * screen.rect.h, AREA_PER_EXTRA_DEFENDER) as i64;
        let extra = (above as i64).min(by_area).max(0) as usize;
        for i in 0..extra {
            roles.push(p.tier_defenders[(self.tier as usize + i) % p.tier_defenders.len()].clone());
        }
        roles
    }

    // -- reinforcements ----------------------------------------------------

    /// `sites`: the mission's live outpost sites, in plan order. Returns
    /// (outpost index, role) for every wave due now, and sets the warnings
    /// that precede them. The mission spawns them (and may decline, see
    /// [`retry`](Self::retry)).
    pub fn update<S: OutpostSite>(&mut self, dt: f64, sites: &mut [S]) -> Vec<(usize, String)> {
        let p = &self.profile;
        let mut due = Vec::new();
        for (index, site) in sites.iter_mut().enumerate() {
            let clock = self.clocks.entry(index).or_insert_with(|| Clock::new(p.wave_every));
            if site.owned() || site.reserves() <= 0 {
                continue;
            }
            if clock.warning > 0.0 {
                clock.warning = (clock.warning - dt).max(0.0);
                site.set_warning(clock.warning);
                if clock.warning <= 0.0 {
                    let sent = self.waves_sent.get(&index).copied().unwrap_or(0);
                    due.push((index, p.wave_roles[sent % p.wave_roles.len()].clone()));
                    self.waves_sent.insert(index, sent + 1);
                    clock.time = p.wave_every;
                }
                continue;
            }
            clock.time -= dt;
            if clock.time <= 0.0 {
                clock.warning = p.warning;
                site.set_warning(p.warning);
            }
        }
        due
    }

    /// A wave the mission could not spawn is tried again shortly.
    pub fn retry(&mut self, index: usize) {
        if let Some(clock) = self.clocks.get_mut(&index) {
            clock.time = 2.0;
            let sent = self.waves_sent.get(&index).copied().unwrap_or(1);
            self.waves_sent.insert(index, sent.saturating_sub(1));
        }
    }
}

#[cfg(test)]
mod tests {
    //! Port of the encounter part of tests/test_screens_and_reclaim.py.
    use super::*;
    use dc_world::ScreenRect;

    fn owner() -> Vec<ScreenRect> {
        vec![ScreenRect::new(0.0, 718.0, 2560.0, 1440.0), ScreenRect::new(2560.0, 0.0, 3840.0, 2160.0)]
    }
    fn side() -> Vec<ScreenRect> {
        vec![ScreenRect::new(0.0, 0.0, 1600.0, 1000.0), ScreenRect::new(1600.0, 0.0, 1600.0, 1000.0)]
    }

    fn raid() -> EncounterProfile {
        profile("raid").unwrap().clone()
    }

    #[test]
    fn a_raid_puts_an_outpost_on_every_other_screen_and_none_on_one_screen() {
        let mut one = EncounterDirector::new(raid(), ScreenLayout::new(&side()[..1], 0).unwrap(), 1);
        assert!(one.outposts_default().is_empty());
        let mut two = EncounterDirector::new(raid(), ScreenLayout::new(&side(), 0).unwrap(), 1);
        let plans = two.outposts_default();
        assert_eq!(plans.iter().map(|p| p.screen).collect::<Vec<_>>(), vec![1]);
        assert_eq!(plans[0].kind, "outpost");
        assert!(side()[1].contains(plans[0].x, plans[0].y, 0.0));
    }

    #[test]
    fn reclaim_infests_every_screen_including_the_main_one() {
        let mut director =
            EncounterDirector::new(profile("reclaim").unwrap().clone(), ScreenLayout::new(&side(), 0).unwrap(), 2);
        let plans = director.outposts_default();
        let mut screens: Vec<usize> = plans.iter().map(|p| p.screen).collect();
        screens.sort();
        assert_eq!(screens, vec![0, 1]);
        assert!(plans.iter().all(|p| p.kind == "infestation" && p.defenders.iter().any(|d| d == "spitter")));
    }

    #[test]
    fn later_maps_and_bigger_screens_bring_more_defenders() {
        let layout = ScreenLayout::new(&owner(), 0).unwrap();
        let small_layout = ScreenLayout::new(&side(), 0).unwrap();
        let first = EncounterDirector::new(raid(), layout.clone(), 1).defenders_for(&layout.screens[1]);
        let later = EncounterDirector::new(raid(), layout.clone(), 3).defenders_for(&layout.screens[1]);
        let small = EncounterDirector::new(raid(), small_layout.clone(), 1).defenders_for(&small_layout.screens[1]);
        assert!(later.len() > first.len());
        assert_eq!(first.len(), 1);
        assert_eq!(small.len(), 1);
        let big2 = EncounterDirector::new(raid(), layout.clone(), 2).defenders_for(&layout.screens[1]);
        let small2 = EncounterDirector::new(raid(), small_layout.clone(), 2).defenders_for(&small_layout.screens[1]);
        assert!(big2.len() > small2.len(), "a 4K screen holds more than a 1600x1000 one");
    }

    struct Site {
        owned: bool,
        reserves: i64,
        warning: f64,
    }

    impl OutpostSite for Site {
        fn owned(&self) -> bool {
            self.owned
        }
        fn reserves(&self) -> i64 {
            self.reserves
        }
        fn set_warning(&mut self, warning: f64) {
            self.warning = warning;
        }
    }

    #[test]
    fn outposts_warn_then_send_waves_until_taken() {
        let mut director = EncounterDirector::new(raid(), ScreenLayout::new(&side(), 0).unwrap(), 1);
        director.outposts_default();
        let mut sites = [Site { owned: false, reserves: 2, warning: 0.0 }];
        let mut due = Vec::new();
        let mut warned = false;
        for _ in 0..(80.0_f64 / 0.1) as usize {
            let got = director.update(0.1, &mut sites);
            warned = warned || sites[0].warning > 0.0;
            if !got.is_empty() {
                sites[0].reserves -= 1;
            }
            due.extend(got);
        }
        assert!(warned);
        assert_eq!(due.iter().map(|(_, r)| r.as_str()).collect::<Vec<_>>(), vec!["hunter", "guard"]);
        sites[0].reserves = 5;
        sites[0].owned = true;
        assert!((0..200).all(|_| director.update(0.5, &mut sites).is_empty()), "a taken outpost sends nothing");
    }

    #[test]
    fn a_declined_wave_is_retried_and_not_counted() {
        let mut director = EncounterDirector::new(raid(), ScreenLayout::new(&side(), 0).unwrap(), 1);
        director.outposts_default();
        let mut sites = [Site { owned: false, reserves: 3, warning: 0.0 }];
        let mut first = Vec::new();
        while first.is_empty() {
            first = director.update(0.1, &mut sites);
        }
        director.retry(first[0].0);
        let mut again = Vec::new();
        while again.is_empty() {
            again = director.update(0.1, &mut sites);
        }
        assert_eq!(first[0].1, again[0].1, "the same role comes again");
    }
}
