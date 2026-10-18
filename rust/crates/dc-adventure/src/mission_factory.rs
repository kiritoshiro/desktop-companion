//! Port of the Qt-free part of app/mission_factory.py.
//!
//! Which mission the Adventure overlay plays, on which screens. The map
//! chosen on the Adventure page decides the kind; the monitors decide the
//! arena: every screen when the profile's `all_screens` is on and there is
//! more than one, else the main screen.
//!
//! `create_mission` builds `TerritoryMission`, `FlySwarmMission` or
//! `ReclaimMission`, which are wave 2; [`plan_mission`] makes every decision
//! it makes, so wave 2 only has to construct the mission from the plan.

use std::path::Path;

use dc_world::{ScreenLayout, ScreenRect};

use crate::adventure_profile::AdventureProfile;
use crate::campaign::{MapInfo, MapKind, chosen_map};

/// The arena for a raid or a swarm. `primary` out of range picks screen 0.
/// Panics on an empty `rects` (Python raises `IndexError`).
pub fn build_layout(rects: &[ScreenRect], primary: usize, all_screens: bool) -> ScreenLayout {
    let primary = if primary < rects.len() { primary } else { 0 };
    let layout = if all_screens && rects.len() > 1 {
        ScreenLayout::new(rects, primary)
    } else {
        ScreenLayout::single(rects[primary])
    };
    layout.expect("at least one usable screen")
}

/// What `create_mission` decides before it builds anything.
#[derive(Debug, Clone, PartialEq)]
pub struct MissionPlan {
    pub info: MapInfo,
    /// The arena for a raid or swarm. None for Reclaim the desktop, whose
    /// layout comes from the desktop snapshot's screens
    /// (`ScreenLayout::new(&shot_rects, snapshot.primary)`; a failed capture
    /// falls back to `synthetic_snapshot(rects, primary)`).
    pub layout: Option<ScreenLayout>,
}

pub fn plan_mission(profile: &AdventureProfile, state_dir: &Path, rects: &[ScreenRect], primary: usize) -> MissionPlan {
    let info = chosen_map(profile, state_dir);
    let layout = match info.kind {
        MapKind::Reclaim => None,
        _ => Some(build_layout(rects, primary, profile.all_screens)),
    };
    MissionPlan { info, layout }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side() -> Vec<ScreenRect> {
        vec![ScreenRect::new(0.0, 0.0, 1600.0, 1000.0), ScreenRect::new(1600.0, 0.0, 1600.0, 1000.0)]
    }

    #[test]
    fn the_layout_follows_the_setting_and_the_monitors() {
        assert!(!build_layout(&side(), 0, false).multi());
        assert_eq!(build_layout(&side(), 1, true).primary().rect, side()[1]);
        assert!(!build_layout(&side()[..1], 0, true).multi());
        assert_eq!(build_layout(&side(), 7, false).primary().rect, side()[0]);
    }

    #[test]
    fn reclaim_takes_its_screens_from_the_snapshot() {
        let dir = std::env::temp_dir().join(format!("dc-factory-{}", std::process::id()));
        let profile = AdventureProfile { selected_map: "reclaim".into(), admin: true, ..Default::default() };
        let plan = plan_mission(&profile, &dir, &side(), 0);
        assert_eq!(plan.info.kind, MapKind::Reclaim);
        assert!(plan.layout.is_none());
        let raid = plan_mission(&AdventureProfile::default(), &dir, &side(), 0);
        assert!(raid.layout.unwrap().multi());
    }
}

/// The constructed mode. State is plain data; the caller supplies its host and
/// the mission RNG on every tick. Screen capture itself belongs to dc-app.
pub enum Mission {
    Territory(crate::mission::TerritoryMission),
    Swarm(crate::swarm::FlySwarmMission),
    Reclaim(crate::reclaim::ReclaimMission),
}
impl Mission {
    pub fn territory(&self) -> &crate::mission::TerritoryMission {
        match self {
            Self::Territory(m) => m,
            Self::Swarm(m) => &m.mission,
            Self::Reclaim(m) => &m.mission,
        }
    }
    pub fn territory_mut(&mut self) -> &mut crate::mission::TerritoryMission {
        match self {
            Self::Territory(m) => m,
            Self::Swarm(m) => &mut m.mission,
            Self::Reclaim(m) => &mut m.mission,
        }
    }
    pub fn update(&mut self, host: &mut impl crate::mission::MissionHost, dt: f64, rng: &mut dc_core::PyRandom) {
        match self {
            Self::Territory(m) => m.update(host, dt, rng),
            Self::Swarm(m) => m.update(host, dt, rng),
            Self::Reclaim(m) => m.update(host, dt, rng),
        }
    }
    pub fn objective(&self, host: &impl crate::mission::MissionHost) -> String {
        match self {
            Self::Territory(m) => m.objective(host),
            Self::Swarm(m) => m.objective(host),
            Self::Reclaim(m) => m.objective(),
        }
    }
    pub fn end_screen_done(&self) -> bool {
        match self {
            Self::Reclaim(m) => m.end_screen_done(),
            _ => self.territory().end_screen_done(),
        }
    }
    pub fn restarted(
        &self,
        host: &mut impl crate::mission::MissionHost,
        rng: &mut dc_core::PyRandom,
    ) -> Result<Self, String> {
        match self {
            Self::Territory(m) => m.restarted(host, rng).map(Self::Territory),
            Self::Swarm(m) => m.restarted(host, rng).map(Self::Swarm),
            Self::Reclaim(m) => m.restarted(host, rng).map(Self::Reclaim),
        }
    }
}

/// `capture` is invoked only for Reclaim. None/empty snapshots use the same
/// synthetic desktop as Python. Reclaim always uses the snapshot's screens,
/// including when the profile's all_screens setting is false.
#[allow(clippy::too_many_arguments)]
pub fn create_mission(
    host: &mut impl crate::mission::MissionHost,
    controls: crate::controls::ControlSettings,
    rects: &[ScreenRect],
    primary: usize,
    state_dir: &Path,
    capture: Option<&mut dyn FnMut() -> Option<dc_art::desktop_surface::SurfaceSnapshot>>,
    rng: &mut dc_core::PyRandom,
) -> Result<Mission, String> {
    use dc_art::desktop_surface::{DesktopSurface, synthetic_snapshot};
    let profile = crate::adventure_profile::load_profile(&crate::adventure_profile::profile_path(state_dir));
    let plan = plan_mission(&profile, state_dir, rects, primary);
    if plan.info.kind == MapKind::Reclaim {
        let snapshot = capture.and_then(|capture| capture()).filter(|s| !s.screens.is_empty()).unwrap_or_else(|| {
            let rects = rects.iter().map(|r| dc_core::Rect::new(r.x, r.y, r.w, r.h)).collect::<Vec<_>>();
            synthetic_snapshot(&rects, &[], primary, 0)
        });
        let screens = snapshot
            .screens
            .iter()
            .map(|s| ScreenRect::new(s.rect.x, s.rect.y, s.rect.w, s.rect.h))
            .collect::<Vec<_>>();
        let layout = ScreenLayout::new(&screens, snapshot.primary).map_err(|e| format!("{e:?}"))?;
        return crate::reclaim::ReclaimMission::new(
            host,
            controls,
            layout,
            DesktopSurface::new(snapshot, None, true),
            state_dir,
            rng,
        )
        .map(Mission::Reclaim);
    }
    let layout = plan.layout.unwrap();
    if plan.info.kind == MapKind::Swarm {
        crate::swarm::FlySwarmMission::new(host, controls, layout, state_dir, rng).map(Mission::Swarm)
    } else {
        crate::mission::TerritoryMission::new(host, controls, layout, state_dir, rng).map(Mission::Territory)
    }
}
