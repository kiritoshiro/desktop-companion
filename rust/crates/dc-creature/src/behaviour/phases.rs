//! Port of `creature/behaviour/phases.py`: scheduled phases, and the one call
//! into the arbiter.

use crate::creature::{After, Creature};
use crate::phase_scheduler::{Focus, Phase, phase_id_for_state};
use crate::progression::Relation;
use crate::pymath::distance;
use crate::state::State;
use crate::world::{CreatureId, CreatureView, Env, Population};

impl Creature {
    /// `_phase_focus_context(mx, my)`
    pub fn phase_focus_context(&self, mx: f64, my: f64, pop: &Population) -> Focus {
        if self.hunting_prey && self.prey.is_some_and(|p| p.alive) {
            return Focus::Prey;
        }
        if matches!(
            self.state,
            State::WeaveApproach
                | State::Weave
                | State::WebApproach
                | State::WebWalk
                | State::WebAim
                | State::WebShot
                | State::RepairApproach
                | State::Repair
        ) || self.web_target.is_some()
            || self.weaving_web.is_some()
            || self.repairing_web.is_some()
        {
            return Focus::Web;
        }
        if let Some(foe) = self.foe.and_then(|id| pop.get(id))
            && !foe.dead
        {
            return Focus::Foe;
        }
        if let Some(target) = self.social_target.and_then(|id| pop.get(id))
            && !target.dragging
        {
            return self.social_focus_for(target);
        }
        let reaction = self.pf("reaction_radius", 360.0);
        let social_range = (self.kin.size * 12.0).max(reaction * 0.85);
        let mate = if self.allow_social { self.find_social_target(social_range, pop) } else { None };
        let cursor_distance = distance(self.kin.x, self.kin.y, mx, my);
        if let Some(mate) = mate.and_then(|id| pop.get(id))
            && (cursor_distance > reaction * 0.85
                || distance(self.kin.x, self.kin.y, mate.x, mate.y) < cursor_distance * 1.15)
        {
            return self.social_focus_for(mate);
        }
        if cursor_distance < reaction {
            return if self.wary_of_cursor() { Focus::CursorWary } else { Focus::Cursor };
        }
        Focus::None
    }

    /// `_social_focus_for(other)`
    pub fn social_focus_for(&self, other: &CreatureView) -> Focus {
        match self.relation_to(other) {
            Relation::Foe => Focus::Foe,
            Relation::Friend => Focus::Friend,
            Relation::Neutral => Focus::Creature,
        }
    }

    /// `_phase_target(focus, mx, my)`: coordinates and, maybe, a mate.
    pub fn phase_target(&self, focus: Focus, mx: f64, my: f64, pop: &Population) -> (f64, f64, Option<CreatureId>) {
        if focus == Focus::Prey
            && let Some(prey) = self.prey
            && prey.alive
            && !prey.eaten
        {
            return (prey.x, prey.y, None);
        }
        if focus == Focus::Foe
            && let Some(foe) = self.foe.and_then(|id| pop.get(id))
            && !foe.dead
        {
            return (foe.x, foe.y, None);
        }
        if matches!(focus, Focus::Creature | Focus::Friend) {
            let mut mate = self.social_target.filter(|id| pop.get(*id).is_some_and(|m| !m.dragging && !m.airborne));
            if mate.is_none() {
                let range = (self.kin.size * 12.0).max(self.pf("reaction_radius", 360.0));
                mate = self.find_social_target(range, pop);
            }
            if let Some(m) = mate.and_then(|id| pop.get(id)) {
                return (m.x, m.y, Some(m.id));
            }
        }
        (mx, my, None)
    }

    /// `_phase_allowed(phase_id)`
    #[inline]
    pub fn phase_allowed(&self, phase: Phase) -> bool {
        self.phase_scheduler.is_allowed(phase)
    }

    #[inline]
    pub fn phase_allowed_id(&self, id: &str) -> bool {
        self.phase_scheduler.is_allowed_id(id)
    }

    /// `_dispatch_scheduled_phase(phase_id, focus, mx, my)`
    pub fn dispatch_scheduled_phase(&mut self, phase: Phase, focus: Focus, mx: f64, my: f64, pop: &Population) -> bool {
        let target_phase = matches!(
            phase,
            Phase::Approach
                | Phase::Chase
                | Phase::Observe
                | Phase::PrepareJumpAttack
                | Phase::Inspect
                | Phase::Cuddle
                | Phase::SocialPlay
                | Phase::RunAway
        );
        if target_phase && focus == Focus::None {
            return false;
        }
        let (tx, ty, target) = self.phase_target(focus, mx, my, pop);
        match phase {
            Phase::Wander => self.enter_wander(),
            Phase::Jump => {
                let power = self.rng.uniform(0.72, 1.15);
                self.enter_spring(After::Idle, power);
            }
            Phase::Roll => self.enter_roll(None),
            Phase::Zoomies => self.enter_zoomies(),
            Phase::Approach => self.enter_approach(tx, ty),
            Phase::Chase => self.enter_chase(tx, ty),
            Phase::Observe => self.enter_observe(tx, ty, target),
            Phase::PrepareJumpAttack => self.enter_aim(tx, ty, target, After::Outcome, (0.6, 1.25), 0.16, pop),
            Phase::Inspect => self.enter_inspect(tx, ty, target),
            Phase::Cuddle => self.enter_cuddle(tx, ty, target),
            Phase::SocialPlay => {
                let Some(t) = target else { return false };
                self.enter_play(t, None, pop);
            }
            Phase::RunAway => self.enter_retreat(mx, my),
        }
        phase_id_for_state(self.state) == Some(phase)
    }

    /// `_sync_scheduled_phase(focus)`
    pub fn sync_scheduled_phase(&mut self, focus: Focus) {
        let Some(phase) = phase_id_for_state(self.state) else {
            if self.phase_scheduler.current.is_some() {
                self.phase_scheduler.cancel();
            }
            return;
        };
        if self.phase_scheduler.current_phase_id() == Some(phase) {
            return;
        }
        let (sched, rng) = self.scheduler_parts();
        if let Some(duration) = sched.adopt(phase, focus, rng) {
            self.state_timer = duration;
        }
    }

    /// `_finish_expired_scheduled_phase(mx, my)`
    pub fn finish_expired_scheduled_phase(&mut self) -> bool {
        if self.phase_scheduler.current.is_none() || self.phase_scheduler.remaining() > 0.0 {
            return false;
        }
        if matches!(self.state, State::Coil | State::Aim | State::Jump | State::Land | State::Roll) {
            return false;
        }
        self.phase_scheduler.finish();
        if matches!(
            self.state,
            State::Wander
                | State::Approach
                | State::Chase
                | State::Observe
                | State::Inspect
                | State::Cuddle
                | State::Play
                | State::Zoom
                | State::Roll
                | State::Retreat
        ) {
            self.enter_idle();
            return true;
        }
        false
    }

    /// `_run_arbiter(mx, my, dist_to_cursor, from_idle=...)`
    pub fn run_arbiter(&mut self, mx: f64, my: f64, dist_to_cursor: f64, from_idle: bool, env: &mut Env<'_>) {
        self.decision_timer = self.rng.uniform(0.1, 0.2);
        let state_timer_expired = self.state_timer <= 0.0;
        let winner = crate::arbiter::decide(self, env, mx, my, dist_to_cursor, from_idle, state_timer_expired);
        crate::arbiter::execute(self, env, winner.action, mx, my);
    }
}
