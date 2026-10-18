//! Port of `creature/phase_scheduler.py`: weighted, focus-aware behaviour
//! phase scheduling.
//!
//! The scheduler does not own a generator here. In Python it held either the
//! creature's own seeded stream (seeded creatures) or a private
//! `random.Random()` (unseeded ones); every method that draws takes the
//! stream explicitly, and `Creature` passes the right one.

use crate::state::State;
use dc_content::personality::Personality;
use dc_content::personality_profiles::{
    BEHAVIOUR_PHASE_IDS, PersonalityRef, phase_duration_multiplier_for, phase_scores_for,
};
use dc_core::PyRandom;
use dc_kinematics::pymath::py_sum;
use serde::{Deserialize, Serialize};

/// A behaviour phase id (`BEHAVIOUR_PHASE_IDS`, in catalog order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Phase {
    Wander,
    Jump,
    Roll,
    Zoomies,
    Approach,
    Chase,
    Observe,
    PrepareJumpAttack,
    Inspect,
    Cuddle,
    SocialPlay,
    RunAway,
}

pub const PHASES: [Phase; 12] = [
    Phase::Wander,
    Phase::Jump,
    Phase::Roll,
    Phase::Zoomies,
    Phase::Approach,
    Phase::Chase,
    Phase::Observe,
    Phase::PrepareJumpAttack,
    Phase::Inspect,
    Phase::Cuddle,
    Phase::SocialPlay,
    Phase::RunAway,
];

impl Phase {
    #[inline]
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn as_str(self) -> &'static str {
        BEHAVIOUR_PHASE_IDS[self.index()]
    }

    /// `str(phase_id).strip().lower()` matched against the catalog.
    pub fn parse(id: &str) -> Option<Phase> {
        let id = id.trim().to_lowercase();
        BEHAVIOUR_PHASE_IDS.iter().position(|p| *p == id).map(|i| PHASES[i])
    }

    /// `PHASE_DURATION_RANGES`
    pub fn duration_range(self) -> (f64, f64) {
        match self {
            Phase::Wander => (1.2, 3.4),
            Phase::Jump => (0.10, 0.24),
            Phase::Roll => (0.72, 1.20),
            Phase::Zoomies => (0.40, 0.82),
            Phase::Approach => (0.55, 1.35),
            Phase::Chase => (0.45, 1.25),
            Phase::Observe => (2.4, 5.8),
            Phase::PrepareJumpAttack => (0.55, 1.30),
            Phase::Inspect => (1.8, 4.8),
            Phase::Cuddle => (2.4, 6.2),
            Phase::SocialPlay => (2.1, 5.4),
            Phase::RunAway => (0.55, 1.35),
        }
    }
}

/// `PHASE_STATE_IDS` / `phase_id_for_state`.
pub fn phase_id_for_state(state: State) -> Option<Phase> {
    Some(match state {
        State::Wander => Phase::Wander,
        State::Coil | State::Jump => Phase::Jump,
        State::Roll => Phase::Roll,
        State::Zoom => Phase::Zoomies,
        State::Approach => Phase::Approach,
        State::Chase => Phase::Chase,
        State::Observe => Phase::Observe,
        State::Aim => Phase::PrepareJumpAttack,
        State::Inspect => Phase::Inspect,
        State::Cuddle => Phase::Cuddle,
        State::Play => Phase::SocialPlay,
        State::Retreat => Phase::RunAway,
        _ => return None,
    })
}

/// The object a phase is aimed at (`FOCUS_PHASE_BIAS` keys).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Focus {
    #[default]
    None,
    Cursor,
    CursorWary,
    Prey,
    Creature,
    Friend,
    Foe,
    Web,
}

impl Focus {
    pub fn as_str(self) -> &'static str {
        match self {
            Focus::None => "none",
            Focus::Cursor => "cursor",
            Focus::CursorWary => "cursor_wary",
            Focus::Prey => "prey",
            Focus::Creature => "creature",
            Focus::Friend => "friend",
            Focus::Foe => "foe",
            Focus::Web => "web",
        }
    }

    /// `FOCUS_PHASE_BIAS[focus]`, in `PHASES` order.
    pub fn bias_row(self) -> &'static [f64; 12] {
        &FOCUS_PHASE_BIAS[self as usize]
    }

    #[inline]
    pub fn bias(self, phase: Phase) -> f64 {
        self.bias_row()[phase.index()]
    }
}

/// `normalize_focus(value)`
pub fn normalize_focus(value: Option<&str>) -> Focus {
    let v = value.unwrap_or("none").trim().to_lowercase();
    let v = match v.as_str() {
        "mouse" | "pointer" | "target" => "cursor",
        "fly" => "prey",
        "spider" | "mate" => "creature",
        "ally" | "teammate" => "friend",
        "enemy" => "foe",
        "silk" => "web",
        other => other,
    };
    match v {
        "cursor" => Focus::Cursor,
        "cursor_wary" => Focus::CursorWary,
        "prey" => Focus::Prey,
        "creature" => Focus::Creature,
        "friend" => Focus::Friend,
        "foe" => Focus::Foe,
        "web" => Focus::Web,
        _ => Focus::None,
    }
}

/// `FOCUS_PHASE_BIAS`, rows in `Focus` order, columns in `PHASES` order:
/// wander, jump, roll, zoomies, approach, chase, observe,
/// prepare_jump_attack, inspect, cuddle, social_play, run_away.
pub const FOCUS_PHASE_BIAS: [[f64; 12]; 8] = [
    [1.8, 1.5, 1.3, 1.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [0.45, 0.65, 0.45, 0.55, 1.7, 2.2, 1.5, 1.9, 1.25, 0.75, 0.20, 0.65],
    [0.90, 0.55, 0.40, 0.55, 0.20, 0.15, 1.9, 0.15, 0.35, 0.10, 0.30, 2.6],
    [0.20, 1.0, 0.10, 0.65, 1.8, 2.8, 1.45, 2.5, 0.70, 0.05, 0.05, 0.10],
    [0.25, 1.0, 0.75, 0.75, 1.4, 0.65, 1.6, 1.0, 1.9, 2.3, 2.8, 0.20],
    [0.30, 1.0, 0.85, 0.85, 1.6, 0.70, 1.3, 0.55, 1.5, 2.6, 3.0, 0.10],
    [0.10, 0.85, 0.20, 0.35, 1.5, 2.6, 1.2, 2.4, 0.25, 0.0, 0.0, 2.0],
    [0.75, 0.15, 0.05, 0.10, 0.75, 0.10, 1.7, 0.10, 1.8, 0.05, 0.05, 0.25],
];

/// `PhasePlan`
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhasePlan {
    pub phase_id: Phase,
    pub focus: Focus,
    pub score: f64,
    pub duration: f64,
    pub remaining: f64,
}

/// `BehaviourPhaseScheduler`
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BehaviourPhaseScheduler {
    /// `scores`, in `PHASES` order (every phase is always present).
    pub scores: [f64; 12],
    pub duration_multiplier: f64,
    /// `enabled`: which phases the creature's skills allow.
    pub enabled: [bool; 12],
    pub deck: Vec<Phase>,
    pub current: Option<PhasePlan>,
    pub last_phase_id: Option<Phase>,
    pub cycles_completed: u64,
}

impl BehaviourPhaseScheduler {
    /// `BehaviourPhaseScheduler(personality, enabled_behaviours, rng)`:
    /// builds the first deck (one `shuffle`).
    pub fn new<S: AsRef<str>>(personality: &Personality, enabled_behaviours: &[S], rng: &mut PyRandom) -> Self {
        Self::from_ref(PersonalityRef::Def(personality), enabled_behaviours, rng)
    }

    pub fn from_ref<S: AsRef<str>>(personality: PersonalityRef<'_>, enabled_behaviours: &[S], rng: &mut PyRandom) -> Self {
        let table = phase_scores_for(personality);
        let mut scores = [0.0; 12];
        for (i, p) in PHASES.iter().enumerate() {
            scores[i] = table.get(p.as_str()).copied().unwrap_or(0.0);
        }
        let mut enabled = [false; 12];
        for id in enabled_behaviours {
            let id = id.as_ref().trim().to_lowercase();
            if let Some(p) = Phase::parse(&id) {
                enabled[p.index()] = true;
            }
        }
        let mut s = BehaviourPhaseScheduler {
            scores,
            duration_multiplier: phase_duration_multiplier_for(personality),
            enabled,
            deck: Vec::new(),
            current: None,
            last_phase_id: None,
            cycles_completed: 0,
        };
        s.refill_deck(rng);
        s
    }

    pub fn current_phase_id(&self) -> Option<Phase> {
        self.current.map(|c| c.phase_id)
    }

    pub fn remaining(&self) -> f64 {
        self.current.map(|c| c.remaining).unwrap_or(0.0)
    }

    /// `is_allowed(phase_id)`
    #[inline]
    pub fn is_allowed(&self, phase: Phase) -> bool {
        self.scores[phase.index()] > 0.0 && self.enabled[phase.index()]
    }

    /// `is_allowed` on a raw id (`False` for an unknown one).
    pub fn is_allowed_id(&self, phase_id: &str) -> bool {
        Phase::parse(phase_id).is_some_and(|p| self.is_allowed(p))
    }

    pub fn tick(&mut self, dt: f64) {
        if let Some(c) = self.current.as_mut() {
            c.remaining = (c.remaining - dt.max(0.0)).max(0.0);
        }
    }

    pub fn cancel(&mut self) {
        if let Some(c) = self.current {
            self.last_phase_id = Some(c.phase_id);
        }
        self.current = None;
    }

    pub fn finish(&mut self) {
        self.cancel();
    }

    fn refill_deck(&mut self, rng: &mut PyRandom) {
        let mut slots = Vec::new();
        for p in PHASES {
            if !self.is_allowed(p) {
                continue;
            }
            let n = (dc_core::math::py_round(self.scores[p.index()]) as i64).max(1);
            for _ in 0..n {
                slots.push(p);
            }
        }
        rng.shuffle(&mut slots);
        self.deck = slots;
        self.cycles_completed += 1;
    }

    fn duration_for(&self, phase: Phase, rng: &mut PyRandom) -> f64 {
        let (low, high) = phase.duration_range();
        let score = self.scores[phase.index()].clamp(0.0, 10.0);
        let score_scale = 0.72 + score / 10.0 * 0.56;
        let low = low * self.duration_multiplier * score_scale;
        let high = high * self.duration_multiplier * score_scale;
        rng.uniform(low, low.max(high))
    }

    /// `choose(focus)`: draw the next phase.
    pub fn choose(&mut self, focus: Focus, rng: &mut PyRandom) -> Option<PhasePlan> {
        for _ in 0..2 {
            let bias = focus.bias_row();
            let mut distinct = [false; 12];
            let mut distinct_count = 0;
            for p in &self.deck {
                if self.is_allowed(*p) && bias[p.index()] > 0.0 && !distinct[p.index()] {
                    distinct[p.index()] = true;
                    distinct_count += 1;
                }
            }
            let mut candidates: Vec<(Phase, usize, f64)> = Vec::new();
            for (index, p) in self.deck.iter().enumerate() {
                if !self.is_allowed(*p) {
                    continue;
                }
                let b = bias[p.index()].max(0.0);
                if b <= 0.0 {
                    continue;
                }
                if Some(*p) == self.last_phase_id && distinct_count > 1 {
                    continue;
                }
                candidates.push((*p, index, b));
            }
            if !candidates.is_empty() {
                let total = py_sum(candidates.iter().map(|c| c.2));
                let mut pick = rng.uniform(0.0, total);
                let mut selected = *candidates.last().unwrap();
                for c in &candidates {
                    pick -= c.2;
                    if pick <= 0.0 {
                        selected = *c;
                        break;
                    }
                }
                let (phase, deck_index, _) = selected;
                self.deck.remove(deck_index);
                let score = self.scores[phase.index()];
                let duration = self.duration_for(phase, rng);
                let plan = PhasePlan { phase_id: phase, focus, score, duration, remaining: duration };
                self.current = Some(plan);
                return Some(plan);
            }
            self.refill_deck(rng);
        }
        None
    }

    /// `peek_score(focus)`: non-mutating preview of `choose`.
    pub fn peek_score(&self, focus: Focus) -> f64 {
        let bias = focus.bias_row();
        let mut best = 0.0f64;
        let mut seen = [false; 12];
        for p in &self.deck {
            if seen[p.index()] || !self.is_allowed(*p) {
                continue;
            }
            seen[p.index()] = true;
            let b = bias[p.index()].max(0.0);
            if b <= 0.0 {
                continue;
            }
            best = best.max(self.scores[p.index()] * b);
        }
        best
    }

    /// `adopt(phase_id, focus)`: adopt a phase entered by legacy FSM code.
    pub fn adopt(&mut self, phase: Phase, focus: Focus, rng: &mut PyRandom) -> Option<f64> {
        if !self.is_allowed(phase) {
            self.cancel();
            return None;
        }
        let duration = self.duration_for(phase, rng);
        self.current =
            Some(PhasePlan { phase_id: phase, focus, score: self.scores[phase.index()], duration, remaining: duration });
        Some(duration)
    }
}
