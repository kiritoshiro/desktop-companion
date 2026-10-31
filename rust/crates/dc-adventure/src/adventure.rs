//! Port of app/adventure.py
//!
//! One spider's Adventure rules: aim cone, stamina, silk, cooldowns, reach.
//! The player's spider and every mission spider act through the same
//! `shoot`, `bite` and `jump`, so no spider can do what the player's cannot.
//!
//! Python's `PlayerController` holds the creature and writes straight into
//! it. Here the controller holds only its own state, and every method takes
//! the spider as a [`ControlledSpider`] -- the narrow surface the rules need,
//! which `dc_creature::Creature` implements. The two calls that need the rest
//! of the world are split:
//!
//! - [`PlayerController::bite`] gets the candidate foes (already filtered to
//!   live foes other than the spider) and returns which one to trade a blow
//!   with; the caller runs `manager._trade_blow`.
//! - [`PlayerController::shoot`] returns the [`SilkShot`] to fire; the caller
//!   builds the `AimedSilk` projectile from it.

use std::collections::BTreeSet;
use std::f64::consts::{PI, TAU};

use crate::controls::{Action, ControlSettings, Key, clamp_to_cone};

/// What the controller reads and writes on the spider it drives.
pub trait ControlledSpider {
    fn x(&self) -> f64;
    fn y(&self) -> f64;
    fn heading(&self) -> f64;
    fn size(&self) -> f64;
    fn dead(&self) -> bool;
    fn airborne(&self) -> bool;
    fn webbed(&self) -> bool;
    /// Held fast by silk (it cannot bite, shoot or jump).
    fn webbed_held(&self) -> bool;
    fn energy(&self) -> f64;
    fn set_energy(&mut self, energy: f64);
    fn max_energy(&self) -> f64;
    /// Spend stamina if there is enough; false (and nothing spent) otherwise.
    fn spend_energy(&mut self, amount: f64) -> bool;
    fn attack_cooldown(&self) -> f64;
    fn set_attack_cooldown(&mut self, seconds: f64);
    fn target(&self) -> (f64, f64);
    fn set_target(&mut self, x: f64, y: f64);
    fn set_target_heading(&mut self, heading: f64);
    fn speed(&self) -> f64;
    fn set_speed(&mut self, speed: f64);
    /// `creature._speed_mult()`: level, armour and skill speed together.
    fn speed_mult(&self) -> f64;
    fn set_motion_paused(&mut self, paused: bool);
    fn reverse_walk(&self) -> bool;
    fn set_reverse_walk(&mut self, reverse: bool);
    /// `creature.state = "Player"`.
    fn set_player_state(&mut self);
    fn has_skill(&self, skill: &str) -> bool;
    /// `creature._launch_jump(x, y, kind=..., after=...)`.
    fn launch_jump(&mut self, x: f64, y: f64, kind: &str, after: &str);
    /// `creature.begin_strike(x, y)`: the bite animation towards a point.
    fn begin_strike(&mut self, x: f64, y: f64);
    /// Taken over by a controller: Python sets `player_control`, clears
    /// prey, foe and hunting, sets `job_mode = "idle"` and calls `enter_idle()`.
    fn take_player_control(&mut self);
    /// Handed back: `player_control = None` and `enter_idle()`.
    fn release_player_control(&mut self);
    /// `PlayerController.advance_pose`: one step of the creature's own
    /// pipeline while a controller drives it -- the jump when airborne, else
    /// aim at a real screen and the grounded frame or `_move_body`; then the
    /// cage bounds, roll, legs, mood, posture and antennae.
    fn advance_pose(&mut self, dt: f64);
}

/// Something that can be aimed at: its position and body size (Python's
/// `getattr(target, "size", 10.0)`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AimTarget {
    pub x: f64,
    pub y: f64,
    pub size: f64,
}

impl AimTarget {
    pub fn new(x: f64, y: f64, size: f64) -> Self {
        Self { x, y, size }
    }
}

/// A silk shot to fire: build `AimedSilk(spider, angle, reach, ...)` from it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SilkShot {
    pub angle: f64,
    pub reach: f64,
}

/// What a shot aims by: Python's `shoot(manager, toward_pointer)` and the
/// mission spider's own `shot_angle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShotAim {
    /// A key: straight where the spider faces.
    Facing,
    /// A mouse button: exactly at the pointer, whatever the cone.
    Pointer,
    /// A mission spider: at its wound-up point, inside its view cone.
    Cone,
}

/// What a bite press did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bite {
    /// Nothing happened (paused, dead, airborne, on cooldown, webbed).
    Blocked,
    /// Bit the air: the strike animation and a short recovery.
    Whiff,
    /// Trade a blow with `foes[index]` (`manager._trade_blow(spider, target)`).
    Hit(usize),
}

impl Bite {
    /// Python's return value.
    pub fn landed(self) -> bool {
        matches!(self, Bite::Hit(_))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerController {
    pub controls: ControlSettings,
    /// Actions held down, not raw keys, so a rebinding never reaches in here.
    pub held: BTreeSet<Action>,
    pub aim: (f64, f64),
    pub web_cooldown: f64,
    pub jump_cooldown: f64,
    pub paused: bool,
    pub keyboard_aim: bool,
    pub sprint_exhausted: bool,
    pub silk_capacity: i64,
    pub silk: f64,
    pub feedback: String,
    pub feedback_time: f64,
    /// How far a shot carries. Python's mission sets `control.WEB_RANGE` per
    /// spider for the Lookout bonus; [`Self::WEB_RANGE`] is the base.
    pub web_range: f64,
    was_webbed: bool,
}

impl PlayerController {
    pub const WEB_RANGE: f64 = 320.0;
    pub const WEB_COOLDOWN: f64 = 1.2;
    pub const WEB_ENERGY: f64 = 12.0;
    pub const JUMP_ENERGY: f64 = 18.0;
    pub const JUMP_COOLDOWN: f64 = 0.65;
    pub const SPRINT_DRAIN: f64 = 24.0;
    /// Body sizes.
    pub const BITE_REACH: f64 = 3.0;
    /// Px a pounce carries.
    pub const POUNCE_DISTANCE: f64 = 75.0;
    pub const SILK_CAPACITY: i64 = 8;
    /// While your side holds the Silk loom.
    pub const LOOM_SILK_CAPACITY: i64 = 12;
    /// How far off the aim line a target may be and still be hit, at least.
    pub const AIM_TOLERANCE: f64 = 28.0;
    /// A bite that finds nothing still has to be recovered from.
    pub const WHIFF_RECOVERY: f64 = 0.38;
    /// Of the walking speed.
    pub const BACK_UP_SPEED: f64 = 0.40;
    /// Radians asked ahead of the body while turning.
    pub const TURN_LEAD: f64 = 0.6;

    /// Take control of `spider` (Python's constructor).
    pub fn new<S: ControlledSpider + ?Sized>(spider: &mut S, controls: Option<ControlSettings>) -> Self {
        let silk_capacity = Self::SILK_CAPACITY;
        spider.take_player_control();
        Self {
            controls: controls.unwrap_or_default(),
            held: BTreeSet::new(),
            aim: (spider.x(), spider.y()),
            web_cooldown: 0.0,
            jump_cooldown: 0.0,
            paused: false,
            keyboard_aim: false,
            sprint_exhausted: false,
            silk_capacity,
            silk: silk_capacity as f64,
            feedback: String::new(),
            feedback_time: 0.0,
            web_range: Self::WEB_RANGE,
            was_webbed: false,
        }
    }

    /// Hand the spider back. Python checks that the creature's
    /// `player_control` is this controller; the caller owns that link here.
    pub fn release<S: ControlledSpider + ?Sized>(&mut self, spider: &mut S) {
        self.held.clear();
        spider.set_reverse_walk(false);
        spider.release_player_control();
    }

    /// Hold or release an action.
    pub fn set_held(&mut self, action: Action, down: bool) {
        if down {
            self.held.insert(action);
        } else {
            self.held.remove(&action);
        }
    }

    /// Hold or release whatever action `key` is bound to.
    pub fn set_held_key(&mut self, key: Key, down: bool) {
        if let Some(action) = self.controls.action_for_key(key) {
            self.set_held(action, down);
        }
    }

    fn holds(&self, action: Action) -> bool {
        self.held.contains(&action)
    }

    // -- aiming -----------------------------------------------------------

    /// Where a shot goes: towards the pointer, inside the view cone.
    pub fn aim_angle<S: ControlledSpider + ?Sized>(&self, spider: &S) -> f64 {
        let (ax, ay) = self.aim;
        if self.keyboard_aim {
            return spider.heading();
        }
        if (ax - spider.x()).hypot(ay - spider.y()) < 1.0 {
            return spider.heading();
        }
        let wanted = (ay - spider.y()).atan2(ax - spider.x());
        clamp_to_cone(spider.heading(), wanted, self.controls.half_cone())
    }

    /// Where a silk shot goes (the owner: "when I'm using F to shoot it
    /// should shoot in the direction the spider is facing, not based on the
    /// mouse. If shooting with the mouse, then the mouse dictates the precise
    /// direction"): a key shoots straight ahead, a mouse button exactly at
    /// the pointer, and a mission spider at its wound-up point inside its cone.
    pub fn shot_angle<S: ControlledSpider + ?Sized>(&self, spider: &S, aim: ShotAim) -> f64 {
        match aim {
            ShotAim::Cone => self.aim_angle(spider),
            ShotAim::Facing => spider.heading(),
            ShotAim::Pointer => {
                let (ax, ay) = self.aim;
                if self.keyboard_aim || (ax - spider.x()).hypot(ay - spider.y()) < 1.0 {
                    spider.heading()
                } else {
                    (ay - spider.y()).atan2(ax - spider.x())
                }
            }
        }
    }

    /// Is `target` inside the view cone (allowing for its own size)?
    pub fn in_cone<S: ControlledSpider + ?Sized>(&self, spider: &S, target: &AimTarget) -> bool {
        let (dx, dy) = (target.x - spider.x(), target.y - spider.y());
        let dist = dx.hypot(dy);
        if dist < 1e-6 {
            return true;
        }
        let body = 8.0_f64.max(target.size);
        let off_heading = (dc_core::math::py_mod(dy.atan2(dx) - spider.heading() + PI, TAU) - PI).abs();
        off_heading <= self.controls.half_cone() + body.atan2(dist)
    }

    pub fn bite_reach<S: ControlledSpider + ?Sized>(&self, spider: &S) -> f64 {
        spider.size() * Self::BITE_REACH
    }

    /// (offset from the aim line, distance) if `target` can be hit: within
    /// `reach`, inside the cone, ahead, and within `tolerance` of the aim line.
    pub fn in_front<S: ControlledSpider + ?Sized>(
        &self,
        spider: &S,
        target: &AimTarget,
        reach: f64,
        tolerance: f64,
    ) -> Option<(f64, f64)> {
        let (dx, dy) = (target.x - spider.x(), target.y - spider.y());
        let dist = dx.hypot(dy);
        if dist > reach || dist < 1e-6 {
            return None;
        }
        if !self.in_cone(spider, target) {
            return None;
        }
        let angle = self.aim_angle(spider);
        let along = dx * angle.cos() + dy * angle.sin();
        if along <= 0.0 {
            return None;
        }
        let across = (-dx * angle.sin() + dy * angle.cos()).abs();
        if across > tolerance {
            return None;
        }
        Some((across, dist))
    }

    /// Webbed, the player fights the silk by holding a movement key.
    pub fn struggling(&self) -> bool {
        self.held.iter().any(|a| a.is_movement())
    }

    fn webbed_feedback(&mut self) {
        self.feedback = "Webbed - struggle free before you can fight.".into();
        self.feedback_time = 1.6;
    }

    pub fn clear_keys(&mut self) {
        self.held.clear();
    }

    /// No movement key held.
    pub(crate) fn motion_idle(&self) -> bool {
        !self.held.iter().any(|a| a.is_movement())
    }

    /// The sprint key (or a companion's wish to run) is held.
    pub(crate) fn sprint_held(&self) -> bool {
        self.holds(Action::Sprint)
    }

    pub fn update<S: ControlledSpider + ?Sized>(&mut self, spider: &mut S, dt: f64) {
        self.feedback_time = (self.feedback_time - dt).max(0.0);
        if spider.webbed() && !self.was_webbed {
            self.feedback = "Webbed! Hold a direction to struggle free.".into();
            self.feedback_time = 2.5;
        }
        self.was_webbed = spider.webbed();
        self.web_cooldown = (self.web_cooldown - dt).max(0.0);
        self.jump_cooldown = (self.jump_cooldown - dt).max(0.0);
        if self.paused {
            spider.set_target(spider.x(), spider.y());
            spider.set_speed(0.0);
            spider.set_motion_paused(true);
            spider.set_reverse_walk(false);
        } else if self.controls.turn_movement() {
            self.update_turn_movement(spider, dt);
        } else {
            spider.set_reverse_walk(false);
            let dx = i32::from(self.holds(Action::MoveRight)) - i32::from(self.holds(Action::MoveLeft));
            let dy = i32::from(self.holds(Action::MoveDown)) - i32::from(self.holds(Action::MoveUp));
            let (dx, dy) = (f64::from(dx), f64::from(dy));
            let length = dx.hypot(dy);
            let moving = length > 0.0;
            let sprinting = self.sprinting(spider, moving, dt);
            if !spider.airborne() {
                spider.set_player_state();
            }
            spider.set_motion_paused(!moving);
            spider.set_speed((if sprinting { 190.0 } else { 115.0 }) * spider.speed_mult());
            if moving {
                spider.set_target(spider.x() + dx / length * 100.0, spider.y() + dy / length * 100.0);
            } else {
                spider.set_target(spider.x(), spider.y());
                // The pointer only aims; turning to face it while standing is
                // an option in the controls settings.
                let (ax, ay) = self.aim;
                if !self.keyboard_aim
                    && self.controls.face_mouse_when_still
                    && (ax - spider.x()).hypot(ay - spider.y()) > spider.size()
                {
                    spider.set_target_heading((ay - spider.y()).atan2(ax - spider.x()));
                }
            }
        }
        if spider.webbed() {
            spider.set_speed(spider.speed() * 0.25);
        }
        spider.advance_pose(dt);
    }

    pub(crate) fn sprinting<S: ControlledSpider + ?Sized>(&mut self, spider: &mut S, moving: bool, dt: f64) -> bool {
        if self.sprint_exhausted && spider.energy() >= spider.max_energy() * 0.20 {
            self.sprint_exhausted = false;
        }
        let sprinting = moving && self.holds(Action::Sprint) && !self.sprint_exhausted;
        if sprinting {
            spider.set_energy((spider.energy() - Self::SPRINT_DRAIN * dt).max(0.0));
            if spider.energy() <= 0.0 {
                self.sprint_exhausted = true;
            }
        }
        sprinting
    }

    /// W forward, S back up (still facing forward), A/D turn.
    fn update_turn_movement<S: ControlledSpider + ?Sized>(&mut self, spider: &mut S, dt: f64) {
        let forward = i32::from(self.holds(Action::MoveUp)) - i32::from(self.holds(Action::MoveDown));
        let turn = f64::from(i32::from(self.holds(Action::MoveRight)) - i32::from(self.holds(Action::MoveLeft)));
        let backing = forward < 0;
        let sprinting = self.sprinting(spider, forward > 0, dt);
        if !spider.airborne() {
            spider.set_player_state();
        }
        spider.set_reverse_walk(backing);
        spider.set_motion_paused(forward == 0);
        spider.set_speed((if sprinting { 190.0 } else { 115.0 }) * spider.speed_mult());
        if backing {
            spider.set_speed(spider.speed() * Self::BACK_UP_SPEED);
        }
        // Where the body should face: ahead of it by a fixed lead while a
        // turn is held, so the gait keeps turning at its own rate.
        let facing = spider.heading() + turn * Self::TURN_LEAD;
        if forward > 0 {
            spider.set_target(spider.x() + facing.cos() * 100.0, spider.y() + facing.sin() * 100.0);
        } else if backing {
            // Walk away from the way it faces, and keep facing it.
            spider.set_target_heading(facing);
            let h = spider.heading();
            spider.set_target(spider.x() - h.cos() * 100.0, spider.y() - h.sin() * 100.0);
        } else {
            spider.set_target(spider.x(), spider.y());
            if turn != 0.0 {
                spider.set_target_heading(facing);
            }
        }
    }

    pub fn jump<S: ControlledSpider + ?Sized>(&mut self, spider: &mut S) -> bool {
        if self.paused
            || spider.dead()
            || spider.airborne()
            || spider.webbed_held()
            || self.jump_cooldown > 0.0
            || !spider.has_skill("jump")
        {
            return false;
        }
        if !spider.spend_energy(Self::JUMP_ENERGY) {
            return false;
        }
        let (tx, ty) = spider.target();
        let (mut dx, mut dy) = (tx - spider.x(), ty - spider.y());
        let mut length = dx.hypot(dy);
        if length < 1.0 || spider.reverse_walk() {
            // Standing, or backing up: pounce the way the spider faces.
            dx = spider.heading().cos();
            dy = spider.heading().sin();
            length = 1.0;
        }
        spider.launch_jump(
            spider.x() + dx / length * Self::POUNCE_DISTANCE,
            spider.y() + dy / length * Self::POUNCE_DISTANCE,
            "pounce",
            "idle",
        );
        self.jump_cooldown = Self::JUMP_COOLDOWN;
        true
    }

    /// Bite the nearest foe on the aim line. `foes`: every live creature that
    /// is a foe of the spider (Python filters `manager.creatures`), in the
    /// manager's order.
    pub fn bite<S: ControlledSpider + ?Sized>(
        &mut self,
        spider: &mut S,
        foes: &[AimTarget],
        conflict_enabled: bool,
    ) -> Bite {
        if self.paused || spider.dead() || spider.airborne() || spider.attack_cooldown() > 0.0 {
            return Bite::Blocked;
        }
        if spider.webbed_held() {
            self.webbed_feedback();
            return Bite::Blocked;
        }
        let reach = self.bite_reach(spider);
        let tolerance = Self::AIM_TOLERANCE.max(spider.size() * 1.5);
        let mut best: Option<((f64, f64), usize)> = None;
        for (index, target) in foes.iter().enumerate() {
            if let Some(hit) = self.in_front(spider, target, reach, tolerance) {
                // min over (across, dist) tuples, first wins on a tie.
                let better = match best {
                    None => true,
                    Some((b, _)) => hit.0 < b.0 || (hit.0 == b.0 && hit.1 < b.1),
                };
                if better {
                    best = Some((hit, index));
                }
            }
        }
        match best {
            Some((_, index)) if conflict_enabled => Bite::Hit(index),
            _ => {
                // A miss still bites the air, so every press shows, and costs
                // the same short recovery as the animation.
                let angle = self.aim_angle(spider);
                spider.begin_strike(spider.x() + angle.cos() * 100.0, spider.y() + angle.sin() * 100.0);
                spider.set_attack_cooldown(spider.attack_cooldown().max(Self::WHIFF_RECOVERY));
                Bite::Whiff
            }
        }
    }

    /// Fire one silk charge along [`Self::shot_angle`]: the shot to build, or
    /// None (with feedback when silk or stamina ran out).
    pub fn shoot<S: ControlledSpider + ?Sized>(&mut self, spider: &mut S, aim: ShotAim) -> Option<SilkShot> {
        if self.paused || spider.dead() || spider.airborne() || self.web_cooldown > 0.0 {
            return None;
        }
        if spider.webbed_held() {
            self.webbed_feedback();
            return None;
        }
        if self.silk < 1.0 || spider.energy() < Self::WEB_ENERGY {
            self.feedback = if self.silk < 1.0 {
                "Silk empty - refill at home or a captured loom".into()
            } else {
                "Not enough stamina".into()
            };
            self.feedback_time = 2.5;
            return None;
        }
        let shot = SilkShot { angle: self.shot_angle(spider, aim), reach: self.web_range };
        spider.spend_energy(Self::WEB_ENERGY);
        self.silk -= 1.0;
        self.web_cooldown = Self::WEB_COOLDOWN;
        Some(shot)
    }
}

#[cfg(test)]
mod tests {
    //! The aim parts of tests/test_adventure_controls.py and the movement
    //! rules of tests/test_turn_movement.py, against a stand-in spider.
    use super::*;
    use crate::controls::Movement;

    #[derive(Debug, Default)]
    pub(crate) struct Dummy {
        pub x: f64,
        pub y: f64,
        pub heading: f64,
        pub target_heading: f64,
        pub size: f64,
        pub energy: f64,
        pub max_energy: f64,
        pub target: (f64, f64),
        pub speed: f64,
        pub paused: bool,
        pub reverse: bool,
        pub webbed: bool,
        pub airborne: bool,
        pub attack_cooldown: f64,
        pub jumps: Vec<(f64, f64)>,
        pub strikes: Vec<(f64, f64)>,
        pub controlled: bool,
    }

    impl ControlledSpider for Dummy {
        fn x(&self) -> f64 {
            self.x
        }
        fn y(&self) -> f64 {
            self.y
        }
        fn heading(&self) -> f64 {
            self.heading
        }
        fn size(&self) -> f64 {
            self.size
        }
        fn dead(&self) -> bool {
            false
        }
        fn airborne(&self) -> bool {
            self.airborne
        }
        fn webbed(&self) -> bool {
            self.webbed
        }
        fn webbed_held(&self) -> bool {
            self.webbed
        }
        fn energy(&self) -> f64 {
            self.energy
        }
        fn set_energy(&mut self, energy: f64) {
            self.energy = energy;
        }
        fn max_energy(&self) -> f64 {
            self.max_energy
        }
        fn spend_energy(&mut self, amount: f64) -> bool {
            if self.energy < amount {
                return false;
            }
            self.energy -= amount;
            true
        }
        fn attack_cooldown(&self) -> f64 {
            self.attack_cooldown
        }
        fn set_attack_cooldown(&mut self, seconds: f64) {
            self.attack_cooldown = seconds;
        }
        fn target(&self) -> (f64, f64) {
            self.target
        }
        fn set_target(&mut self, x: f64, y: f64) {
            self.target = (x, y);
        }
        fn set_target_heading(&mut self, heading: f64) {
            self.target_heading = heading;
        }
        fn speed(&self) -> f64 {
            self.speed
        }
        fn set_speed(&mut self, speed: f64) {
            self.speed = speed;
        }
        fn speed_mult(&self) -> f64 {
            1.0
        }
        fn set_motion_paused(&mut self, paused: bool) {
            self.paused = paused;
        }
        fn reverse_walk(&self) -> bool {
            self.reverse
        }
        fn set_reverse_walk(&mut self, reverse: bool) {
            self.reverse = reverse;
        }
        fn set_player_state(&mut self) {}
        fn has_skill(&self, _skill: &str) -> bool {
            true
        }
        fn launch_jump(&mut self, x: f64, y: f64, _kind: &str, _after: &str) {
            self.jumps.push((x, y));
        }
        fn begin_strike(&mut self, x: f64, y: f64) {
            self.strikes.push((x, y));
        }
        fn take_player_control(&mut self) {
            self.controlled = true;
        }
        fn release_player_control(&mut self) {
            self.controlled = false;
        }
        fn advance_pose(&mut self, _dt: f64) {}
    }

    fn spider(heading: f64) -> Dummy {
        Dummy { x: 450.0, y: 350.0, heading, size: 12.0, energy: 60.0, max_energy: 60.0, ..Dummy::default() }
    }

    fn target(s: &Dummy, angle_deg: f64, dist: f64) -> AimTarget {
        let a = angle_deg.to_radians();
        AimTarget::new(s.x + a.cos() * dist, s.y + a.sin() * dist, 12.0)
    }

    #[test]
    fn the_aim_is_held_to_the_cone() {
        let half = 45f64.to_radians();
        let mut s = spider(0.0);
        let mut player = PlayerController::new(&mut s, None);
        assert!(s.controlled);
        player.aim = (450.0, 100.0); // straight up
        assert!((player.aim_angle(&s) + half).abs() < 1e-9);
        player.controls.aim_cone = 360;
        assert!((player.aim_angle(&s) + PI / 2.0).abs() < 1e-9);
        player.release(&mut s);
        assert!(!s.controlled);
    }

    /// test_mission.py's `test_a_key_shoots_where_the_spider_faces_and_a_click_at_the_pointer`
    /// (#114). The owner: "when I'm using F to shoot it should shoot in the
    /// direction the spider is facing, not based on the mouse. If shooting
    /// with the mouse then the mouse dictates the precise direction."
    #[test]
    fn a_key_shoots_where_the_spider_faces_and_a_click_at_the_pointer() {
        let mut s = spider(0.4);
        let mut player = PlayerController::new(&mut s, None);
        player.aim = (s.x, s.y + 300.0); // the pointer straight below
        let shot = player.shoot(&mut s, ShotAim::Facing).expect("a key shot");
        assert!((shot.angle - 0.4).abs() < 1e-9, "a key shot follows the heading, not the pointer");
        player.web_cooldown = 0.0;
        player.aim = (s.x - 300.0, s.y); // behind it: a click still goes there
        let shot = player.shoot(&mut s, ShotAim::Pointer).expect("a click shot");
        assert!((shot.angle.abs() - PI).abs() < 1e-9, "a click goes exactly at the pointer, outside the cone");
        // A mission spider's shot stays inside its cone.
        assert!((player.shot_angle(&s, ShotAim::Cone) - player.aim_angle(&s)).abs() < 1e-12);
        assert!((player.shot_angle(&s, ShotAim::Cone).abs() - PI).abs() > 0.5);
        // Aiming by keyboard (two players), a click shoots straight ahead too.
        player.keyboard_aim = true;
        assert!((player.shot_angle(&s, ShotAim::Pointer) - 0.4).abs() < 1e-9);
    }

    #[test]
    fn only_what_is_in_front_can_be_hit() {
        let mut s = spider(0.0);
        let mut player = PlayerController::new(&mut s, None);
        let (ahead, off, behind) = (target(&s, 0.0, 150.0), target(&s, 65.0, 150.0), target(&s, 180.0, 150.0));
        for (t, hits) in [(ahead, true), (off, false), (behind, false)] {
            player.aim = (t.x, t.y);
            assert_eq!(player.in_front(&s, &t, PlayerController::WEB_RANGE, 30.0).is_some(), hits, "{t:?}");
        }
        player.controls.aim_cone = 180;
        player.aim = (off.x, off.y);
        assert!(player.in_front(&s, &off, PlayerController::WEB_RANGE, 30.0).is_some());
    }

    #[test]
    fn rebound_keys_move_the_spider() {
        let mut settings = ControlSettings { movement: Movement::Screen, ..ControlSettings::default() };
        settings.rebind(Action::MoveRight, "L");
        let mut s = spider(0.0);
        let mut player = PlayerController::new(&mut s, Some(settings));
        player.set_held_key(Key::Char('D'), true);
        assert!(player.held.is_empty());
        player.set_held_key(Key::Char('L'), true);
        player.update(&mut s, 1.0 / 60.0);
        assert!(s.target.0 > s.x + 50.0 && !s.paused);
        assert_eq!(s.speed, 115.0);
    }

    #[test]
    fn turn_movement_walks_forward_backs_up_and_turns() {
        let mut s = spider(0.0);
        let mut player = PlayerController::new(&mut s, None);
        player.held = [Action::MoveUp].into();
        player.update(&mut s, 1.0 / 60.0);
        assert!(s.target.0 > s.x && !s.reverse);
        player.held = [Action::MoveDown, Action::Sprint].into();
        let energy = s.energy;
        player.update(&mut s, 1.0 / 60.0);
        assert!(s.reverse && s.target.0 < s.x);
        assert!((s.speed - 115.0 * PlayerController::BACK_UP_SPEED).abs() < 1e-9);
        assert_eq!(s.energy, energy, "backing up cannot sprint");
        player.held = [Action::MoveLeft].into();
        player.update(&mut s, 1.0 / 60.0);
        assert_eq!(s.target, (s.x, s.y));
        assert!((s.target_heading + PlayerController::TURN_LEAD).abs() < 1e-9, "A turns anticlockwise");
        player.held.clear();
        player.update(&mut s, 1.0 / 60.0);
        assert!(!s.reverse && s.paused);
    }

    #[test]
    fn sprinting_drains_stamina_until_exhausted() {
        let mut s = spider(0.0);
        let mut player = PlayerController::new(&mut s, None);
        player.held = [Action::MoveUp, Action::Sprint].into();
        for _ in 0..200 {
            player.update(&mut s, 0.05);
        }
        assert!(player.sprint_exhausted);
        assert_eq!(s.speed, 115.0);
    }

    #[test]
    fn jump_bite_and_shoot_follow_the_rules() {
        let mut s = spider(0.0);
        let mut player = PlayerController::new(&mut s, None);
        s.target = (s.x, s.y);
        assert!(player.jump(&mut s));
        assert_eq!(s.jumps, vec![(450.0 + PlayerController::POUNCE_DISTANCE, 350.0)]);
        assert!(!player.jump(&mut s), "cooldown");
        let foe = target(&s, 0.0, 30.0);
        player.aim = (foe.x, foe.y);
        assert_eq!(player.bite(&mut s, &[foe], true), Bite::Hit(0));
        assert_eq!(player.bite(&mut s, &[foe], false), Bite::Whiff);
        assert!(s.attack_cooldown >= PlayerController::WHIFF_RECOVERY);
        assert_eq!(player.bite(&mut s, &[foe], true), Bite::Blocked);
        let shot = player.shoot(&mut s, ShotAim::Facing).expect("a shot");
        assert_eq!(shot.reach, PlayerController::WEB_RANGE);
        assert_eq!(player.silk, 7.0);
        assert!(player.shoot(&mut s, ShotAim::Facing).is_none(), "cooldown");
        player.web_cooldown = 0.0;
        player.silk = 0.0;
        assert!(player.shoot(&mut s, ShotAim::Facing).is_none());
        assert!(player.feedback.starts_with("Silk empty"));
        s.webbed = true;
        player.update(&mut s, 0.1);
        assert!(player.feedback.starts_with("Webbed!"));
        assert_eq!(s.speed, 115.0 * 0.25, "webbed spiders crawl");
    }
}
