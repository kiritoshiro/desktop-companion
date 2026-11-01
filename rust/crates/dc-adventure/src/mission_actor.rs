//! Port of app/mission.py's MissionActor: the player's rules with AI decisions.
use crate::{
    adventure::{ControlledSpider, PlayerController, ShotAim},
    controls::{Action, ControlSettings},
    mission::*,
};
use dc_content::enemy_traits::{EnemyTraits, traits_for};
use dc_world::tactics::{Doctrine, Order, OrderMode};
use std::f64::consts::{PI, TAU};

#[derive(Debug, Clone)]
pub struct MissionActor {
    pub creature: SpiderId,
    pub control: PlayerController,
    pub role: String,
    pub style: String,
    pub slot: usize,
    pub defend_point: Point,
    pub companion_id: Option<String>,
    pub home: Point,
    pub raider: bool,
    pub windup: f64,
    pub strike_target: Option<SpiderId>,
    pub strike_point: Point,
    pub strike_kind: Option<String>,
    pub target: Option<SpiderId>,
    pub think: f64,
    pub traits: Option<EnemyTraits>,
    pub order: Option<Order<SpiderId>>,
    pub doctrine: Doctrine,
    pub hunting: bool,
    pub out_of_reach: f64,
    pub charge_time: f64,
    pub charge_cooldown: f64,
    pub retreat_time: f64,
    pub from_outpost: bool,
    pub aggro_range: Option<f64>,
    pub spit_range: f64,
}
impl MissionActor {
    pub const SPIT_RANGE: f64 = 290.0;
    pub const SPIT_ENERGY: f64 = 14.0;
    pub const SPIT_COOLDOWN: f64 = 2.2;
    pub const REGEN_PER_SECOND: f64 = 0.9;
    pub const CHARGE_SECONDS: f64 = 1.1;
    pub const CHARGE_COOLDOWN: f64 = 7.0;
    pub const CHARGE_SPEED: f64 = 1.7;
    pub const HIT_AND_RUN_SECONDS: f64 = 0.9;
    pub const RALLY_REACH: f64 = 230.0;
    // A companion keeps up with you (the owner: "my companion spiders seem
    // to not have ability to run just like me ... they should be able to
    // shift run, to follow me and do other of my commands"): it walks at
    // your pace, and runs, on its own stamina, when it has fallen this far
    // behind where it should be -- or whenever you run.
    pub const ALLY_WALK: f64 = 115.0;
    pub const ALLY_RUN: f64 = 190.0;
    pub const ALLY_RUN_GAP: f64 = 170.0;
    pub const ALLY_CALM_GAP: f64 = 90.0;
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        creature: SpiderId,
        c: &mut impl MissionSpider,
        controls: ControlSettings,
        role: &str,
        home: Point,
        raider: bool,
        style: Option<&str>,
        slot: usize,
        capacity: i64,
        kind: Option<&str>,
        companion: Option<&str>,
    ) -> Self {
        let mut control = PlayerController::new(c, Some(controls));
        control.silk_capacity = capacity;
        control.silk = capacity as f64;
        Self {
            creature,
            control,
            role: role.into(),
            style: style.unwrap_or(role).into(),
            slot,
            defend_point: home,
            companion_id: companion.map(String::from),
            home,
            raider,
            windup: 0.0,
            strike_target: None,
            strike_point: home,
            strike_kind: None,
            target: None,
            think: c.index() as f64 * 0.013,
            traits: if role == "ally" { None } else { Some(*traits_for(kind, role)) },
            order: None,
            doctrine: Doctrine::None,
            hunting: false,
            out_of_reach: 0.0,
            charge_time: 0.0,
            charge_cooldown: 0.0,
            retreat_time: 0.0,
            from_outpost: false,
            aggro_range: None,
            spit_range: Self::SPIT_RANGE,
        }
    }
    pub fn has(&self, ability: &str) -> bool {
        self.traits.is_some_and(|t| t.has(ability))
    }
    pub fn fighting_reach(&self, c: &impl ControlledSpider) -> f64 {
        if self.style == "weaver" || self.has("web") {
            self.control.web_range * 0.6
        } else if self.style == "spitter" || self.has("spit") {
            self.spit_range * 0.7
        } else {
            self.control.bite_reach(c)
        }
    }
    pub fn keeps_distance(&self) -> bool {
        ["weaver", "spitter"].contains(&self.style.as_str())
            || self.traits.is_some_and(|t| ["ranged", "support"].contains(&t.squad_role))
    }
    pub fn enraged(&self, c: &impl MissionSpider) -> bool {
        self.has("enrage") && c.hp() < c.max_hp() * 0.4
    }
    pub fn rallied(&self, m: &TerritoryMission, host: &impl MissionHost) -> bool {
        self.role != "ally"
            && m.actors.iter().any(|a| {
                a.creature != self.creature
                    && a.has("rally")
                    && !host.creature(a.creature).dead()
                    && a.role != "ally"
                    && distance(point(host.creature(a.creature)), point(host.creature(self.creature)))
                        < Self::RALLY_REACH
            })
    }
    pub fn choose_strike(&self, c: &impl MissionSpider, target: &impl MissionSpider, gap: f64) -> Option<&'static str> {
        if !self.control.in_cone(c, &aim_target(target)) {
            return None;
        }
        let bite = self.control.bite_reach(c);
        if self.style == "spitter" || self.has("spit") {
            if gap < self.spit_range && self.control.web_cooldown <= 0.0 && c.energy() >= Self::SPIT_ENERGY {
                return Some("spit");
            }
            return (gap < bite).then_some("bite");
        }
        if self.style == "weaver" || self.has("web") {
            if gap < self.control.web_range
                && self.control.web_cooldown <= 0.0
                && self.control.silk >= 1.0
                && c.energy() >= PlayerController::WEB_ENERGY
            {
                return Some("shoot");
            }
            return (gap < bite).then_some("bite");
        }
        if gap < bite {
            return Some("bite");
        }
        if (self.style == "hunter" || self.has("pounce"))
            && gap < bite + PlayerController::POUNCE_DISTANCE
            && self.control.jump_cooldown <= 0.0
            && c.energy() >= PlayerController::JUMP_ENERGY
            && c.has_skill("jump")
        {
            Some("pounce")
        } else {
            None
        }
    }
    pub fn by_temperament(
        &mut self,
        m: &TerritoryMission,
        host: &impl MissionHost,
        targets: Vec<SpiderId>,
    ) -> Vec<SpiderId> {
        let c = host.creature(self.creature);
        let temper = self.traits.unwrap().temper();
        if temper.chase_forever {
            if !self.hunting && targets.iter().any(|&id| distance(point(host.creature(id)), point(c)) < temper.notice) {
                self.hunting = true;
            }
            if self.hunting {
                return if targets.contains(&m.hero) { vec![m.hero] } else { targets };
            }
        }
        let fair = targets
            .into_iter()
            .filter(|&id| distance(point(host.creature(id)), m.sites[0].point()) > 130.0)
            .collect::<Vec<_>>();
        let mut chosen = fair
            .iter()
            .copied()
            .filter(|&id| {
                distance(point(host.creature(id)), self.home) < temper.leash
                    && (distance(point(host.creature(id)), point(c)) < temper.notice || Some(id) == self.target)
            })
            .collect::<Vec<_>>();
        if let Some(keep) =
            self.target.filter(|id| !host.creature(*id).dead() && !chosen.contains(id) && fair.contains(id))
        {
            self.out_of_reach += 0.15;
            if self.out_of_reach < temper.give_up {
                chosen.push(keep);
            }
        } else {
            self.out_of_reach = 0.0;
        }
        chosen
    }
    pub fn follow_order(&mut self, host: &impl MissionHost) {
        let Some(order) = &self.order else {
            return;
        };
        if self.role == "ally" {
            return;
        }
        if order.mode == OrderMode::Retreat {
            self.target = None;
            return;
        }
        let Some(wanted) = order.target.filter(|&id| !host.creature(id).dead()) else {
            return;
        };
        if matches!(order.mode, OrderMode::Hold | OrderMode::Ambush) {
            let notice = self.traits.map_or(120.0, |t| t.temper().notice * 0.6);
            if self
                .target
                .is_some_and(|id| distance(point(host.creature(id)), point(host.creature(self.creature))) > notice)
            {
                self.target = None;
            }
            return;
        }
        self.target = Some(wanted);
    }
    pub fn update<H: MissionHost>(
        &mut self,
        m: &mut TerritoryMission,
        host: &mut H,
        dt: f64,
        env: &impl MissionEnvironment<H>,
    ) {
        self.think -= dt;
        self.control.web_cooldown = (self.control.web_cooldown - dt).max(0.0);
        self.control.jump_cooldown = (self.control.jump_cooldown - dt).max(0.0);
        self.charge_time = (self.charge_time - dt).max(0.0);
        self.charge_cooldown = (self.charge_cooldown - dt).max(0.0);
        self.retreat_time = (self.retreat_time - dt).max(0.0);
        let c = host.creature_mut(self.creature);
        if self.has("regen") && !c.dead() && c.hp() < c.max_hp() {
            c.heal(Self::REGEN_PER_SECOND * dt);
        }
        let previous = self.target;
        if self.think <= 0.0 {
            self.think = 0.15;
            let c = host.creature(self.creature);
            let mut targets = host
                .creatures()
                .into_iter()
                .filter(|&id| !host.creature(id).dead() && TerritoryMission::foes(host, self.creature, id))
                .collect::<Vec<_>>();
            if self.role == "ally" {
                let leader = host.creature(
                    m.players.iter().find(|p| !host.creature(p.creature).dead()).map_or(m.hero, |p| p.creature),
                );
                let anchor = if m.command == "defend" { self.defend_point } else { point(leader) };
                let spread = (self.slot as f64 - (m.allies.len() as f64 - 1.0) / 2.0) * 0.7;
                let behind = leader.heading() + PI + spread;
                self.home = if m.command == "defend" {
                    anchor
                } else {
                    m.layout.clamp(anchor.0 + behind.cos() * 85.0, anchor.1 + behind.sin() * 85.0, c.size() * 2.0)
                };
                targets.retain(|&id| distance(point(host.creature(id)), anchor) < 220.0);
                if m.command == "attack"
                    && let Some(id) = m.attack_target.filter(|&id| !host.creature(id).dead())
                {
                    targets = vec![id];
                }
            } else if let Some(reach) = self.aggro_range {
                targets.retain(|&id| distance(point(host.creature(id)), point(c)) < reach);
            } else if !self.raider {
                targets = self.by_temperament(m, host, targets);
            }
            self.target = targets.into_iter().min_by(|&a, &b| {
                distance(point(host.creature(a)), point(host.creature(self.creature)))
                    .total_cmp(&distance(point(host.creature(b)), point(host.creature(self.creature))))
            });
            self.follow_order(host);
        }
        if self.target.is_some_and(|id| host.creature(id).dead()) {
            self.target = None;
        }
        if self.target.is_some() && previous.is_none() && self.has("charge") && self.charge_cooldown <= 0.0 {
            self.charge_time = Self::CHARGE_SECONDS;
            self.charge_cooldown = Self::CHARGE_COOLDOWN;
        }
        let mut dest = self.target.map(|id| point(host.creature(id))).unwrap_or(self.home);
        let order_point = self.order.as_ref().and_then(|o| o.point);
        if self.target.is_none()
            && let Some(goal) = env.actor_goal(m, host, self)
        {
            dest = goal;
        }
        let c = host.creature(self.creature);
        let gap = distance(dest, point(c));
        let reach = self.control.bite_reach(c);
        let mut paused = gap < if self.target.is_some() { reach * 0.72 } else { 22.0 };
        let mut speed = if self.style == "hunter" { 95.0 } else { 68.0 } * c.speed_mult();
        if self.role == "ally" {
            speed = self.ally_pace(m, host, gap, !paused, dt);
        }
        let c = host.creature(self.creature);
        if let Some(traits) = self.traits {
            speed *= traits.speed;
            if self.charge_time > 0.0 {
                speed *= Self::CHARGE_SPEED;
            }
            if self.enraged(c) {
                speed *= 1.2;
            }
        }
        if self.target.is_some() && self.keeps_distance() && order_point.is_none() {
            paused = 135.0 < gap && gap < 230.0;
            if gap < 135.0 {
                dest = (c.x() + (c.x() - dest.0), c.y() + (c.y() - dest.1));
            }
        }
        if let Some(p) = order_point {
            let to_point = distance(p, point(c));
            let closing = self.target.is_some()
                && self.order.as_ref().is_some_and(|o| {
                    matches!(o.mode, OrderMode::Flank | OrderMode::Encircle | OrderMode::Screen | OrderMode::Bait)
                })
                && to_point < 34.0
                && !self.keeps_distance();
            if !closing {
                dest = p;
                paused = to_point < 18.0;
            }
        }
        if let Some(id) = self.target.filter(|_| self.retreat_time > 0.0) {
            let t = host.creature(id);
            dest = (c.x() + (c.x() - t.x()), c.y() + (c.y() - t.y()));
            paused = false;
        }
        let webbed = c.webbed();
        let airborne = c.airborne();
        let attack_cooldown = c.attack_cooldown();
        host.creature_mut(self.creature).set_speed(speed);
        if self.windup > 0.0 {
            paused = true;
            self.windup -= dt;
            if webbed {
                self.windup = 0.0;
                host.creature_mut(self.creature).set_attack_cooldown(0.9);
            } else if self.windup <= 0.0 {
                self.release_strike(m, host);
            }
        } else if let Some(id) = self.target.filter(|_| !webbed && !airborne && attack_cooldown <= 0.0) {
            let kind = self.choose_strike(host.creature(self.creature), host.creature(id), gap);
            let busy = m.actors.iter().filter(|a| a.windup > 0.0 && a.role != "ally").count();
            if let Some(kind) = kind.filter(|_| self.role == "ally" || busy < 2) {
                self.windup = if self.role == "guardian" { 0.7 } else { 0.55 };
                self.strike_kind = Some(kind.into());
                self.strike_target = Some(id);
                self.strike_point = point(host.creature(id));
                host.creature_mut(self.creature).begin_strike(self.strike_point.0, self.strike_point.1);
            }
        }
        let target_point = self.target.map(|id| point(host.creature(id)));
        let c = host.creature_mut(self.creature);
        c.set_motion_paused(paused);
        let (tx, ty) = m.layout.route(c.x(), c.y(), dest.0, dest.1);
        let (tx, ty) = m.layout.clamp(tx, ty, c.size());
        c.set_target(tx, ty);
        if let Some(p) = target_point {
            c.set_target_heading((p.1 - c.y()).atan2(p.0 - c.x()));
        }
        if c.webbed() {
            c.set_speed(c.speed() * 0.25);
        }
        if !c.airborne() {
            c.set_player_state();
        }
        c.advance_pose(dt);
    }
    /// `_ally_pace`: a companion's speed, holding its own sprint key when it
    /// has fallen behind or the leading player runs. `moving` is whether it
    /// has somewhere to go this frame.
    fn ally_pace(&mut self, m: &TerritoryMission, host: &mut impl MissionHost, gap: f64, moving: bool, dt: f64) -> f64 {
        let leader_runs = m
            .players
            .iter()
            .find(|p| !host.creature(p.creature).dead())
            .is_some_and(|p| p.control.sprint_held() && !p.control.motion_idle());
        if gap > Self::ALLY_RUN_GAP || leader_runs {
            self.control.set_held(Action::Sprint, true);
        } else if gap < Self::ALLY_CALM_GAP {
            self.control.set_held(Action::Sprint, false);
        }
        let c = host.creature_mut(self.creature);
        let running = self.control.sprinting(c, moving, dt);
        (if running { Self::ALLY_RUN } else { Self::ALLY_WALK }) * c.speed_mult()
    }
    pub fn release_strike(&mut self, m: &mut TerritoryMission, host: &mut impl MissionHost) {
        self.control.aim = self.strike_point;
        match self.strike_kind.as_deref() {
            Some("shoot") => {
                TerritoryMission::shoot(host, self.creature, &mut self.control, ShotAim::Cone);
            }
            Some("spit") => {
                self.spit(m, host, self.strike_point);
            }
            Some("pounce") => {
                let c = host.creature_mut(self.creature);
                let pos = m.layout.clamp(self.strike_point.0, self.strike_point.1, c.size() * 2.0);
                c.set_target(pos.0, pos.1);
                self.control.jump(c);
            }
            _ => {
                TerritoryMission::bite(host, self.creature, &mut self.control);
            }
        }
        let mut pause = if self.role != "guardian" { 1.4 } else { 1.8 };
        if self.enraged(host.creature(self.creature)) {
            pause *= 0.65;
        }
        if self.rallied(m, host) {
            pause *= 0.8;
        }
        let c = host.creature_mut(self.creature);
        c.set_attack_cooldown(c.attack_cooldown().max(pause));
        if self.has("hit_and_run") {
            self.retreat_time = Self::HIT_AND_RUN_SECONDS;
        }
    }
    pub fn spit(&mut self, m: &mut TerritoryMission, host: &mut impl MissionHost, pos: Point) -> bool {
        let c = host.creature_mut(self.creature);
        if c.dead() || c.airborne() || c.webbed_held() || self.control.web_cooldown > 0.0 {
            return false;
        }
        let angle = (pos.1 - c.y()).atan2(pos.0 - c.x());
        let off = (dc_core::math::py_mod(angle - c.heading() + PI, TAU) - PI).abs();
        if self.control.controls.aim_cone < 360 && off > self.control.controls.half_cone() + 0.05 {
            return false;
        }
        if !c.spend_energy(Self::SPIT_ENERGY) {
            return false;
        }
        self.control.web_cooldown = Self::SPIT_COOLDOWN;
        m.spit_acid(host, self.creature, pos);
        true
    }
}
