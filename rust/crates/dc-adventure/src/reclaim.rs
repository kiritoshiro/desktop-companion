//! Port of app/reclaim.py: frozen desktop damage, words, Devourer and rollback.
use crate::{
    adventure::ControlledSpider,
    adventure_profile::{AdventureProfile, save_profile},
    controls::ControlSettings,
    mission::*,
};
use dc_art::desktop_surface::{DesktopSurface, Hit};
use dc_core::PyRandom;
use dc_world::ScreenLayout;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SilkThread {
    pub x: f64,
    pub y: f64,
    pub eater: SpiderId,
    pub age: f64,
}
pub struct ReclaimMission {
    pub mission: TerritoryMission,
    pub surface: DesktopSurface,
    pub words_eaten: usize,
    pub threads: Vec<SilkThread>,
    pub intro_time: f64,
    pub guardian_called: bool,
    pub lost_desktop: bool,
    pub start_profile: AdventureProfile,
}
impl ReclaimMission {
    pub const FREEZES_DESKTOP: bool = true;
    pub const CRASH_GLASS_SECONDS: f64 = 1.2;
    pub const CRASH_SCREEN_SECONDS: f64 = 7.0;
    pub const MELT_RADIUS: f64 = 30.0;
    pub const EAT_SECONDS: f64 = 1.1;
    pub const WORD_HEAL: f64 = 3.0;
    pub const WORD_LEASH: f64 = 280.0;
    pub const WORD_ROAM: f64 = 600.0;
    pub const INTRO_SECONDS: f64 = 7.0;
    pub fn new(
        host: &mut impl MissionHost,
        controls: ControlSettings,
        layout: ScreenLayout,
        surface: DesktopSurface,
        state_dir: &Path,
        rng: &mut PyRandom,
    ) -> Result<Self, String> {
        let mut mission = TerritoryMission::new(host, controls, layout, state_dir, rng)?;
        let start_profile = mission.profile.clone();
        mission.announce("The desktop is frozen. Esc and Leave gives it back at once.");
        Ok(Self {
            mission,
            surface,
            words_eaten: 0,
            threads: vec![],
            intro_time: Self::INTRO_SECONDS,
            guardian_called: false,
            lost_desktop: false,
            start_profile,
        })
    }
    pub fn objective(&self) -> String {
        match self.mission.state.as_str() {
            "victory" => "VICTORY - the desktop is yours again".into(),
            "crashed" => "DESKTOP BROKEN - nothing gained, nothing lost".into(),
            "defeat" => "RAID ENDED - your spider has fallen".into(),
            _ => {
                let taken = self.mission.outposts.iter().filter(|&&i| self.mission.sites[i].owned).count();
                if taken < self.mission.outposts.len() {
                    format!("01 / Destroy the nests  ({taken}/{})", self.mission.outposts.len())
                } else {
                    "02 / Defeat the Devourer".into()
                }
            }
        }
    }
    pub fn update(&mut self, host: &mut impl MissionHost, dt: f64, rng: &mut PyRandom) {
        self.surface.update(dt);
        self.intro_time = (self.intro_time - dt).max(0.0);
        for t in &mut self.threads {
            t.age += dt;
        }
        self.threads.retain(|t| t.age < 0.7);
        self.mission.update_with(host, dt, rng, &mut ReclaimEnvironment { surface: &mut self.surface });
        if self.mission.ended() {
            return;
        }
        self.eat_words(host, dt.clamp(0.0, 0.05));
        if self.surface.desktop_hp() <= 0.0 {
            self.crash();
            return;
        }
        if self.mission.guardian.is_some_and(|id| host.creature(id).dead()) {
            self.mission.state = "victory".into();
            for p in &self.mission.players {
                host.creature_mut(p.creature).gain_experience(100, "desktop reclaimed");
            }
            self.mission.clear_player_keys();
            self.mission.finish(host, true);
        }
    }
    pub fn crash(&mut self) {
        if self.mission.state == "crashed" {
            return;
        }
        self.surface.break_desktop(None);
        self.lost_desktop = true;
        self.mission.state = "crashed".into();
        self.mission.end_clock = 0.0;
        self.mission.clear_player_keys();
        self.mission.profile = self.start_profile.clone();
        if save_profile(&self.mission.profile, &self.mission.progress_path) {
            self.mission.saved = true;
            self.mission.save_error.clear();
        } else {
            self.mission.save_error = "The profile could not be restored.".into();
        }
    }
    pub fn blue_screen(&self) -> f64 {
        if self.mission.state != "crashed" {
            0.0
        } else {
            (self.mission.end_clock - Self::CRASH_GLASS_SECONDS).max(0.0)
        }
    }
    pub fn end_screen_done(&self) -> bool {
        if self.mission.state == "crashed" {
            self.mission.end_clock >= Self::CRASH_GLASS_SECONDS + Self::CRASH_SCREEN_SECONDS
        } else {
            self.mission.end_screen_done()
        }
    }
    pub fn eat_words(&mut self, host: &mut impl MissionHost, dt: f64) {
        for i in 0..self.mission.actors.len() {
            let a = &self.mission.actors[i];
            let c = host.creature_mut(a.creature);
            if a.role == "ally" || c.dead() || c.airborne() {
                continue;
            }
            let Some(word) = self.surface.word_near(c.x(), c.y(), c.size() * 0.9) else {
                continue;
            };
            if self.surface.eat(word, dt / Self::EAT_SECONDS) {
                self.words_eaten += 1;
                c.heal(Self::WORD_HEAL);
                let (x, y) = self.surface.words[word].centre();
                self.threads.push(SilkThread { x, y, eater: a.creature, age: 0.0 });
                if [1, 10, 25, 50, 100].contains(&self.words_eaten) {
                    self.mission.announce(format!(
                        "The enemy has devoured {} word{} of your desktop",
                        self.words_eaten,
                        if self.words_eaten != 1 { "s" } else { "" }
                    ));
                }
            }
        }
    }
    pub fn on_acid<H: MissionHost>(&mut self, x: f64, y: f64, rng: &mut PyRandom) {
        <ReclaimEnvironment<'_> as MissionEnvironment<H>>::on_acid(
            &mut ReclaimEnvironment { surface: &mut self.surface },
            &mut self.mission,
            x,
            y,
            rng,
        );
    }
    pub fn restarted(&self, host: &mut impl MissionHost, rng: &mut PyRandom) -> Result<Self, String> {
        Self::new(
            host,
            self.mission.controls.clone(),
            self.mission.layout.clone(),
            self.surface.fresh(None),
            self.mission.progress_path.parent().unwrap(),
            rng,
        )
    }
}
pub struct ReclaimEnvironment<'a> {
    pub surface: &'a mut DesktopSurface,
}
impl ReclaimEnvironment<'_> {
    fn cracked(&self, m: &mut TerritoryMission, result: Option<Hit>) {
        match result {
            Some(Hit::Window) => {
                let left = self.surface.intact_panes().count() as i64 - 1;
                let more = if left > 0 {
                    format!(" - {left} window{} left", if left != 1 { "s" } else { "" })
                } else {
                    " - the desktop is next".into()
                };
                m.announce(format!("A window's glass gave way{more}"));
            }
            Some(Hit::Desktop) => m.announce("The desktop's glass is breaking"),
            _ => {}
        }
    }
}
impl<H: MissionHost> MissionEnvironment<H> for ReclaimEnvironment<'_> {
    fn actor_goal(&self, _m: &TerritoryMission, host: &H, actor: &MissionActor) -> Option<Point> {
        if actor.role == "ally" {
            return None;
        }
        let c = host.creature(actor.creature);
        let (anchor, reach) = if actor.raider {
            (point(c), ReclaimMission::WORD_ROAM)
        } else {
            (actor.defend_point, ReclaimMission::WORD_LEASH)
        };
        let mut best = None;
        let mut best_d = reach;
        for i in self.surface.words_within(anchor.0, anchor.1, reach) {
            let pos = self.surface.words[i].centre();
            let d = distance(pos, point(c));
            if d < best_d {
                best = Some(pos);
                best_d = d;
            }
        }
        best
    }
    fn spawning(&mut self, m: &mut TerritoryMission, host: &mut H, dt: f64, rng: &mut PyRandom) {
        if m.guardian.is_some()
            || m.outposts.is_empty()
            || !m.outposts.iter().all(|&i| m.sites[i].owned)
            || m.actors.iter().any(|a| !host.creature(a.creature).dead() && a.role != "ally")
        {
            return;
        }
        if m.guardian_warning.is_none() {
            m.guardian_warning = Some(4.0);
            m.announce("Something vast is crawling up from under the desktop. 4 seconds.");
        }
        let warning = (m.guardian_warning.unwrap() - dt).max(0.0);
        m.guardian_warning = Some(warning);
        if warning > 0.0 {
            return;
        }
        let hero = point(host.creature(m.hero));
        let here = m.layout.screen_at(hero.0, hero.1).index;
        let lair = *m
            .layout
            .screens
            .iter()
            .reduce(|a, b| {
                let ka = (a.index != here, distance(a.centre(), hero));
                let kb = (b.index != here, distance(b.centre(), hero));
                if kb > ka { b } else { a }
            })
            .unwrap();
        let mut pos = lair.anchor(0.5, 0.45, 120.0);
        if distance(hero, pos) < 180.0 {
            pos = lair.anchor(0.8, 0.3, 120.0);
        }
        m.guardian = m.spawn(host, "guardian", pos, true, None, None, rng);
        if let Some(id) = m.guardian {
            for a in &mut m.actors {
                if a.creature == id {
                    a.style = "hunter".into();
                }
            }
            m.announce(format!(
                "{} has risen on {}. Its landings crack the glass.",
                host.creature(id).display_name(),
                lair.name()
            ));
        }
    }
    fn on_acid(&mut self, m: &mut TerritoryMission, x: f64, y: f64, rng: &mut PyRandom) {
        if self.surface.melt(x, y, ReclaimMission::MELT_RADIUS) == Some(Hit::Window) && rng.random() < 0.25 {
            m.announce("Acid is melting through the windows");
        }
    }
    fn on_landing(&mut self, m: &mut TerritoryMission, c: &H::Spider) {
        if c.size_scale() >= TerritoryMission::HEAVY_LANDING {
            let result = self.surface.crack(c.x(), c.y(), (0.25 + (c.size_scale() - 1.0) * 0.9).min(1.0));
            self.cracked(m, result);
        }
    }
    fn on_heavy_step(&mut self, m: &mut TerritoryMission, c: &H::Spider) {
        let result = self.surface.crack(c.x(), c.y(), 0.16);
        self.cracked(m, result);
    }
}
