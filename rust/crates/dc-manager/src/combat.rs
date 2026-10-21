//! Port of `manager/combat.py`.
use crate::{CreatureManager, constants::*};
use dc_creature::{Creature, constants::*, progression::Relation};
use dc_world::carcass::{CARCASS_FOOD_AMOUNT, CARCASS_LIFETIME, carcass_for, eaters_near};
pub const CONTACT_REACH: f64 = 1.75;
pub const ATTACK_INTERVAL: f64 = 0.85;
pub const ENGAGE_RADIUS_MULT: f64 = 0.62;
pub const DISENGAGE_RADIUS_MULT: f64 = 1.9;
pub const ENGAGEMENT_COMMITMENT: f64 = 2.5;
pub const WEBBED_DAMAGE_BONUS: f64 = 1.6;
fn gap(a: &Creature, b: &Creature) -> f64 {
    (a.kin.x - b.kin.x).hypot(a.kin.y - b.kin.y)
}
impl CreatureManager {
    pub fn clear_foes(&mut self) {
        for c in &mut self.creatures {
            c.foe = None;
        }
    }
    pub fn may_pick_a_fight(c: &Creature) -> bool {
        !c.fleeing()
            && !c.player_control
            && (c.job_id == "guard"
                || c.job_id == "hunter"
                || (c.last_attacker.is_some() && c.hurt_flash > 0.0)
                || c.job_mode == "idle"
                || c.job_mode.is_empty())
    }
    pub fn choose_foes(&mut self) {
        for i in 0..self.creatures.len() {
            let c = &self.creatures[i];
            if c.dead || c.dragging {
                continue;
            }
            if !Self::may_pick_a_fight(c) {
                self.creatures[i].foe = None;
                continue;
            }
            let reaction = crate::surfaces::personality_float(c, "reaction_radius", 360.0);
            let current = c.foe.filter(|id| {
                self.creature(*id)
                    .is_some_and(|f| !f.dead && !f.dragging && gap(c, f) <= reaction * DISENGAGE_RADIUS_MULT)
            });
            if current.is_some() && c.engagement_timer > 0.0 {
                self.creatures[i].foe = current;
                continue;
            }
            let mut best = None;
            let mut best_d = reaction * ENGAGE_RADIUS_MULT;
            for other in &self.creatures {
                if other.id == c.id || other.dead || other.dragging || c.relation_to(&other.view()) != Relation::Foe {
                    continue;
                }
                let d = gap(c, other);
                if d < best_d {
                    best = Some(other.id);
                    best_d = d;
                }
            }
            let best = best.or(current);
            let c = &mut self.creatures[i];
            if best.is_some() && best != current {
                c.engagement_timer = ENGAGEMENT_COMMITMENT;
            }
            c.foe = best;
        }
    }
    pub fn threat_around(&self, c: &Creature) -> (usize, usize, Option<u64>) {
        let (mut threat, mut friends, mut nearest, mut nearest_d) = (0, 0, None, THREAT_SCAN_RADIUS);
        for o in &self.creatures {
            if o.id == c.id || o.dead {
                continue;
            }
            let d = gap(c, o);
            if d > THREAT_SCAN_RADIUS {
                continue;
            }
            match c.relation_to(&o.view()) {
                Relation::Foe => {
                    threat += 1;
                    if d < nearest_d {
                        nearest = Some(o.id);
                        nearest_d = d;
                    }
                }
                Relation::Friend => friends += 1,
                _ => {}
            }
        }
        (threat, friends, nearest)
    }
    pub fn is_clear_of_threats(&self, c: &Creature) -> bool {
        !self.creatures.iter().any(|o| {
            o.id != c.id && !o.dead && c.relation_to(&o.view()) == Relation::Foe && gap(c, o) <= ESCAPED_RADIUS
        })
    }
    pub fn call_off_retreat(c: &mut Creature) {
        c.flee_timer = 0.0;
        c.flee_from = None;
        c.escaped_timer = 0.0;
    }
    pub fn update_nerve(&mut self, dt: f64) {
        for i in 0..self.creatures.len() {
            if self.creatures[i].dead {
                continue;
            }
            self.creatures[i].engagement_timer = (self.creatures[i].engagement_timer - dt).max(0.0);
            let c = &self.creatures[i];
            let hurt = c.health_fraction() < FLEE_HEALTH_FRACTION;
            let recovered = c.health_fraction() >= RALLY_HEALTH_FRACTION;
            let (threat, friends, nearest) = self.threat_around(c);
            let outnumbered = threat > 0 && threat as f64 > (friends + 1) as f64 * OUTNUMBERED_RATIO;
            let clear = self.is_clear_of_threats(c);
            let has_base = self.base_world.base(&format!("team:{}", c.progression.team_id)).is_some();
            let c = &mut self.creatures[i];
            if c.player_control {
                c.flee_timer = 0.0;
                c.flee_from = None;
                c.recovering = false;
                continue;
            }
            if c.dragging {
                c.flee_timer = 0.0;
                continue;
            }
            if c.fleeing() {
                if (recovered || !hurt) && !outnumbered {
                    Self::call_off_retreat(c);
                    continue;
                }
                if clear {
                    c.escaped_timer += dt;
                    if c.escaped_timer >= ESCAPED_SECONDS {
                        Self::call_off_retreat(c);
                        continue;
                    }
                } else {
                    c.escaped_timer = 0.0;
                }
                c.flee_timer = c.flee_timer.max(0.6);
                c.flee_from = nearest.or(c.flee_from);
                continue;
            }
            if (hurt || outnumbered) && nearest.is_some() {
                c.flee_timer = 1.4;
                c.flee_from = nearest;
                c.escaped_timer = 0.0;
                c.recovering = false;
                c.foe = None;
                continue;
            }
            if c.recovering {
                if recovered {
                    c.recovering = false;
                }
            } else if hurt && has_base {
                c.recovering = true;
            }
        }
    }
    pub fn credit_fight_xp(&mut self, id: u64, damage: f64, killed: bool) {
        let amount = damage * DAMAGE_XP_PER_POINT + if killed { KILL_XP_REWARD } else { 0.0 };
        if amount > 0.0 {
            self.award_feed_xp(id, dc_core::math::py_round(amount), if killed { "kill" } else { "fight" });
        }
    }
    pub fn splash(&mut self, attacker: u64, defender: u64, amount: f64) -> f64 {
        if !self.conflict_enabled || self.creature(defender).is_none_or(|c| c.dead) {
            return 0.0;
        }
        let c = self.creature_mut(defender).unwrap();
        let landed = c.take_damage(amount, Some(attacker));
        let killed = c.dead;
        if landed > 0.0 {
            self.credit_fight_xp(attacker, landed, killed);
        }
        landed
    }
    pub fn trade_blow(&mut self, attacker: u64, defender: u64) {
        let Some(a) = self.creature(attacker) else {
            return;
        };
        if a.webbed_held() {
            return;
        }
        let Some(d) = self.creature(defender) else {
            return;
        };
        let (ax, ay, dx, dy) = (a.kin.x, a.kin.y, d.kin.x, d.kin.y);
        let blow = a.damage * if d.webbed() { WEBBED_DAMAGE_BONUS } else { 1.0 };
        let a = self.creature_mut(attacker).unwrap();
        a.attack_cooldown = ATTACK_INTERVAL;
        a.strike_landed(dx, dy);
        let d = self.creature_mut(defender).unwrap();
        d.blow_taken(ax, ay);
        let landed = d.take_damage(blow, Some(attacker));
        let dead = d.dead;
        if landed <= 0.0 {
            return;
        }
        self.credit_fight_xp(attacker, landed, dead);
        if dead {
            return;
        }
        let d = self.creature(defender).unwrap();
        if d.attack_cooldown <= 0.0 && !d.player_control && !d.webbed_held() {
            let damage = d.damage;
            self.creature_mut(defender).unwrap().attack_cooldown = ATTACK_INTERVAL;
            let a = self.creature_mut(attacker).unwrap();
            let struck = a.take_damage(damage, Some(defender));
            let dead = a.dead;
            if struck > 0.0 {
                self.credit_fight_xp(defender, struck, dead);
            }
        }
    }
    pub fn resolve_combat(&mut self, dt: f64) {
        self.update_carcasses(dt);
        self.bury_the_dead();
        self.update_nerve(dt);
        if !self.conflict_enabled {
            self.clear_foes();
            return;
        }
        self.choose_foes();
        let live: Vec<_> = self.creatures.iter().filter(|c| !c.dead && !c.dragging).map(|c| c.id).collect();
        for (i, &aid) in live.iter().enumerate() {
            let a = self.creature(aid).unwrap();
            if !a.player_control && a.attack_cooldown > 0.0 && !a.webbed_held() {
                continue;
            }
            for &did in &live[i + 1..] {
                let a = self.creature(aid).unwrap();
                let d = self.creature(did).unwrap();
                if d.dead
                    || a.relation_to(&d.view()) != Relation::Foe
                    || gap(a, d) > (a.kin.size + d.kin.size) * CONTACT_REACH
                {
                    continue;
                }
                if a.player_control || a.webbed_held() {
                    if !d.player_control && d.attack_cooldown <= 0.0 && !d.webbed_held() {
                        self.trade_blow(did, aid);
                    }
                } else {
                    self.trade_blow(aid, did);
                }
                break;
            }
        }
    }
    pub fn bury_the_dead(&mut self) {
        let fallen: Vec<_> = self.creatures.iter().filter(|c| c.dead).map(|c| c.id).collect();
        if fallen.is_empty() {
            return;
        }
        for &id in &fallen {
            let c = self.creature(id).unwrap();
            let numbers = c
                .damage_numbers
                .iter()
                .map(|n| dc_world::carcass::DamageNumber { amount: n.amount, age: n.age, nudge: n.nudge })
                .collect::<Vec<_>>();
            let carcass = carcass_for(c, c.colors.get("body").map(|b| (b[0], b[1], b[2])), &numbers);
            let key = c.progression_id.clone();
            self.carcasses.push(carcass);
            self.progression_states.remove(&key);
            self.slot_ids.remove(&id);
            if self.dragged_creature == Some(id) {
                self.dragged_creature = None;
            }
            for other in &mut self.creatures {
                if other.job_alert_target == Some(id) {
                    other.job_alert_target = None;
                }
            }
        }
        self.creatures.retain(|c| !c.dead);
        self.refresh_neighbor_links();
        self.refresh_render_order();
        self.mark_runtime_state_dirty();
    }
    pub fn update_carcasses(&mut self, dt: f64) {
        self.carcasses.retain_mut(|carcass| {
            let eaters = eaters_near(carcass, &self.creatures);
            let before = carcass.t;
            let finished = carcass.update(dt, eaters);
            let eaten = (carcass.t - before).max(0.0);
            if eaters > 0 && eaten > 0.0 {
                let share = CARCASS_FOOD_AMOUNT * (eaten / CARCASS_LIFETIME) / eaters as f64;
                for c in &self.creatures {
                    if eaters_near(carcass, std::slice::from_ref(c)) > 0 {
                        self.base_world.credit_team_food(Some(&c.progression.team_id), share);
                    }
                }
            }
            !finished
        });
    }
}
