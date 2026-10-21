//! Port of `creature/arbiter.py`: the single utility arbiter for "what should
//! this spider do next" (DC-18).
//!
//! Python built a list of `Candidate(action_id, score, execute)` closures and
//! ran the best one. Here [`decide`] builds the same candidates, in the same
//! order and with the same random draws (each candidate's noise is drawn
//! while the list is built), and returns the winner as an [`Action`]; the
//! winner's closure body is [`execute`]. Ties go to the earliest candidate,
//! as Python's `max` does.

use crate::creature::{After, Creature};
use crate::perception::Perception;
use crate::phase_scheduler::{Focus, PHASES, Phase};
use crate::pymath::{angle_to, rand_range};
use crate::world::{CreatureId, Env, WebId};
use dc_content::personality_profiles::{PersonalityRef, phase_scores_for};
use std::f64::consts::PI;

/// What the winning candidate does (the body of its Python closure).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Idle,
    /// `engage_cursor`: Alert on a still cursor (hunter), else Approach.
    EngageCursor {
        cursor_still: bool,
    },
    Hop,
    Observe {
        ax: f64,
        ay: f64,
        target: Option<CreatureId>,
    },
    AlertRetreat,
    AlertChase,
    AlertApproach,
    DriftRun,
    RepairWeb(WebId),
    AdoptWeb(WebId),
    WeaveNew,
    WebWalk(WebId),
    ShootWebCursor {
        dist_to_cursor: f64,
    },
    SocialPlay {
        mate: CreatureId,
        mate_x: f64,
        mate_y: f64,
        d: f64,
    },
    SocialInspect {
        mate: CreatureId,
        mate_x: f64,
        mate_y: f64,
    },
    SocialCuddle {
        mate: CreatureId,
        mate_x: f64,
        mate_y: f64,
    },
    CursorPounce {
        abort_chance: f64,
    },
    CursorInspect,
    CursorCuddle,
    CursorPlayfulPounce,
    CursorRoll,
    SelfRoll,
    SelfSpring,
    SelfZoomies,
    ScheduledPhase {
        focus: Focus,
    },
}

/// `Candidate`: one thing a creature could commit to doing next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub action_id: &'static str,
    pub score: f64,
    pub action: Action,
}

impl Candidate {
    fn new(action_id: &'static str, score: f64, action: Action) -> Self {
        Candidate { action_id, score, action }
    }
}

/// `phase_scores_for(personality)` as a `PHASES`-ordered row.
fn phase_weights(c: &Creature) -> [f64; 12] {
    let table = phase_scores_for(PersonalityRef::Def(&c.personality_def));
    let mut out = [0.0; 12];
    for (i, p) in PHASES.iter().enumerate() {
        out[i] = table.get(p.as_str()).copied().unwrap_or(0.0);
    }
    out
}

/// `_noise(creature)`
#[inline]
fn noise(c: &mut Creature) -> f64 {
    c.rng.uniform(0.85, 1.15)
}

/// `decide(creature, perception, mx, my, dist_to_cursor, from_idle,
/// state_timer_expired)`: score every eligible candidate, return the winner.
pub fn decide(
    c: &mut Creature,
    env: &Env<'_>,
    mx: f64,
    my: f64,
    dist_to_cursor: f64,
    from_idle: bool,
    state_timer_expired: bool,
) -> Candidate {
    candidates(c, env, mx, my, dist_to_cursor, from_idle, state_timer_expired)
        .into_iter()
        .fold(None::<Candidate>, |best, cand| match best {
            Some(b) if b.score >= cand.score => Some(b),
            _ => Some(cand),
        })
        .expect("the idle candidate is always present")
}

/// Every candidate `decide` considers, in Python order.
pub fn candidates(
    c: &mut Creature,
    env: &Env<'_>,
    mx: f64,
    my: f64,
    dist_to_cursor: f64,
    from_idle: bool,
    state_timer_expired: bool,
) -> Vec<Candidate> {
    let pop = env.population;
    let focus = c.phase_focus_context(mx, my, pop);
    let reaction = c.pf("reaction_radius", 360.0);
    let weights = phase_weights(c);
    let w = |p: Phase| weights[p.index()];
    let perception = Perception::new(c, env);

    let mut out = vec![Candidate::new("idle", 0.05, Action::Idle)];
    let hunting = c.hunting_prey;

    // _engage_cursor_candidate
    if !hunting && dist_to_cursor < reaction {
        let hunter = c.acts_as_hunter();
        let cursor_still = hunter && c.cursor_is_still_for_observe();
        let closeness = 1.0 - (dist_to_cursor / reaction.max(1.0)).clamp(0.0, 1.0);
        let weight = w(Phase::Approach) + if hunter { 2.5 } else { 0.0 };
        let bias = focus.bias(Phase::Approach);
        let score = weight * bias * (0.4 + closeness) * noise(c);
        if score > 0.0 {
            out.push(Candidate::new("engage_cursor", score, Action::EngageCursor { cursor_still }));
        }
    }
    // _hop_candidate
    if state_timer_expired && c.acts_as_jumper() && c.rng.random() < c.pf("idle_hop_chance", 0.55) {
        let score = (w(Phase::Jump) + 2.0) * noise(c);
        out.push(Candidate::new("hop", score, Action::Hop));
    }
    if state_timer_expired {
        // _observe_candidate
        if let Some((ax, ay, target)) = c.observer_anchor(mx, my, pop) {
            let score = (w(Phase::Observe) + 2.5) * focus.bias(Phase::Observe) * noise(c);
            if score > 0.0 {
                out.push(Candidate::new("observe", score, Action::Observe { ax, ay, target }));
            }
        }
        // _alert_escalation_candidates
        if !hunting && c.state == crate::state::State::Alert {
            let boldness = c.pf("boldness", 0.5).clamp(0.0, 1.0);
            if c.has_skill("run_away") && dist_to_cursor < c.pf("threat_radius", 180.0) * 0.55 {
                let timid = 1.0 - boldness;
                let weight = w(Phase::RunAway) + timid * 3.0;
                let score = weight * focus.bias(Phase::RunAway) * noise(c);
                out.push(Candidate::new("alert_retreat", score, Action::AlertRetreat));
            }
            if c.has_skill("chase") && dist_to_cursor < 95.0 {
                let weight = w(Phase::Chase) + (0.45 + boldness * 0.45) * 4.0;
                let score = weight * focus.bias(Phase::Chase) * noise(c);
                out.push(Candidate::new("alert_chase", score, Action::AlertChase));
            }
            if dist_to_cursor < reaction {
                let weight = w(Phase::Approach) + (0.25 + boldness * 0.65) * 3.0;
                let score = weight * focus.bias(Phase::Approach) * noise(c);
                out.push(Candidate::new("alert_approach", score, Action::AlertApproach));
            }
        }
    }
    // _drift_run_candidate
    if c.should_start_drift_run(from_idle) {
        let score = 6.0 * noise(c);
        out.push(Candidate::new("drift_run", score, Action::DriftRun));
    }
    if state_timer_expired {
        web_care_candidates(c, &perception, reaction, &mut out);
        // _shoot_web_candidate
        if !hunting
            && dist_to_cursor < reaction
            && (c.has_skill("shoot_web") || c.has_skill("wall_web"))
            && c.web_shot_cooldown <= 0.0
        {
            let score = 3.0 * noise(c);
            out.push(Candidate::new("shoot_web_cursor", score, Action::ShootWebCursor { dist_to_cursor }));
        }
        social_candidates(c, env, focus, &weights, &mut out);
        if !hunting {
            cursor_expressive_candidates(c, dist_to_cursor, focus, &weights, &mut out);
        }
        self_flourish_candidates(c, from_idle, state_timer_expired, focus, &weights, &mut out);
        // _scheduled_phase_candidate
        if state_timer_expired && !c.airborne && !c.dragging {
            let preview = c.phase_scheduler.peek_score(focus);
            if preview > 0.0 {
                let score = preview * noise(c);
                out.push(Candidate::new("scheduled_phase", score, Action::ScheduledPhase { focus }));
            }
        }
    }
    out
}

/// `_web_care_candidates`: repair, adopt, weave-new and web-walk.
fn web_care_candidates(c: &mut Creature, perception: &Perception, reaction: f64, out: &mut Vec<Candidate>) {
    if c.has_skill("weave_web") && c.cage.is_none() && perception.has_web_world && c.weave_cooldown <= 0.0 {
        let webber = c.acts_as_webber();
        if let Some(web) = perception.repairable_web(if webber { 1e9 } else { reaction * 1.5 }) {
            let score = (if webber { 9.0 } else { 2.5 }) * noise(c);
            out.push(Candidate::new("repair_web", score, Action::RepairWeb(web)));
        }
        if let Some(web) = perception.adoptable_web(if webber { 1e9 } else { reaction * 1.6 }) {
            let score = (if webber { 8.5 } else { 2.2 }) * noise(c);
            out.push(Candidate::new("adopt_web", score, Action::AdoptWeb(web)));
        }
        let intact = perception.intact_web_count() as f64;
        let satiation = (1.0 - intact * c.pf("web_satiation_per_web", 0.22)).clamp(0.12, 1.0);
        let base = if webber { 6.2 + c.mood.curiosity * 2.8 } else { 0.3 };
        let score = base * satiation * noise(c);
        out.push(Candidate::new("weave_new", score, Action::WeaveNew));
    }
    if c.has_skill("web_walk")
        && c.cage.is_none()
        && perception.has_web_world
        && c.web_walk_cooldown <= 0.0
        && let Some(web) = perception.walkable_web(reaction * 1.8)
    {
        let webber = c.acts_as_webber();
        let base = if webber { 4.5 } else { 1.6 + c.mood.curiosity * 2.0 };
        let score = base * noise(c);
        out.push(Candidate::new("web_walk", score, Action::WebWalk(web)));
    }
}

/// `_social_candidates`
fn social_candidates(c: &mut Creature, env: &Env<'_>, focus: Focus, weights: &[f64; 12], out: &mut Vec<Candidate>) {
    let hunter = c.acts_as_hunter();
    if !c.allow_social || hunter || c.social_cooldown > 0.0 {
        return;
    }
    let reaction = c.pf("reaction_radius", 360.0);
    let social_range = (reaction * 0.85).max(c.kin.size * 12.0);
    let Some(mate) = c.find_social_target(social_range, env.population).and_then(|id| env.population.get(id)) else {
        return;
    };
    let (mate_id, mate_x, mate_y) = (mate.id, mate.x, mate.y);
    let d = ((c.kin.x - mate_x).powi(2) + (c.kin.y - mate_y).powi(2)).sqrt();
    let m = c.mood;
    let play_w = 0.25 + m.arousal * 0.6 + m.valence.max(0.0) * 0.4;
    let inspect_w = 0.25 + m.curiosity * 0.8;
    let cuddle_w = 0.1 + m.affection * 0.8;
    let play_weight = weights[Phase::SocialPlay.index()] + play_w * 2.0;
    let inspect_weight = weights[Phase::Inspect.index()] + inspect_w * 2.0;
    let cuddle_weight = weights[Phase::Cuddle.index()] + cuddle_w * 2.0;
    let s = play_weight * focus.bias(Phase::SocialPlay) * noise(c);
    out.push(Candidate::new("social_play", s, Action::SocialPlay { mate: mate_id, mate_x, mate_y, d }));
    let s = inspect_weight * focus.bias(Phase::Inspect) * noise(c);
    out.push(Candidate::new("social_inspect", s, Action::SocialInspect { mate: mate_id, mate_x, mate_y }));
    let s = cuddle_weight * focus.bias(Phase::Cuddle) * noise(c);
    out.push(Candidate::new("social_cuddle", s, Action::SocialCuddle { mate: mate_id, mate_x, mate_y }));
}

/// `_cursor_expressive_candidates`
fn cursor_expressive_candidates(
    c: &mut Creature,
    dist_to_cursor: f64,
    focus: Focus,
    weights: &[f64; 12],
    out: &mut Vec<Candidate>,
) {
    let reaction = c.pf("reaction_radius", 360.0);
    if dist_to_cursor >= reaction {
        return;
    }
    let boldness = c.pf("boldness", 0.5).clamp(0.0, 1.0);
    let m = c.mood;
    let near = dist_to_cursor < c.kin.size * 6.5;
    let mid = dist_to_cursor < reaction * 0.8;
    let w = |p: Phase| weights[p.index()];
    if mid && (boldness > 0.6 || m.arousal > 0.6) {
        let weight = w(Phase::PrepareJumpAttack) + (boldness + m.arousal) * 2.0;
        let s = weight * focus.bias(Phase::PrepareJumpAttack) * noise(c);
        out.push(Candidate::new("cursor_pounce", s, Action::CursorPounce { abort_chance: 0.22 - boldness * 0.15 }));
    }
    if mid && m.curiosity > 0.55 {
        let weight = w(Phase::Inspect) + m.curiosity * 2.0;
        let s = weight * focus.bias(Phase::Inspect) * noise(c);
        out.push(Candidate::new("cursor_inspect", s, Action::CursorInspect));
    }
    if near && m.affection > 0.55 {
        let weight = w(Phase::Cuddle) + m.affection * 2.0;
        let s = weight * focus.bias(Phase::Cuddle) * noise(c);
        out.push(Candidate::new("cursor_cuddle", s, Action::CursorCuddle));
    }
    if near && m.valence > 0.4 && m.arousal > 0.5 {
        let weight = w(Phase::PrepareJumpAttack) + (m.valence + m.arousal);
        let s = weight * focus.bias(Phase::PrepareJumpAttack) * noise(c) * 0.6;
        out.push(Candidate::new("cursor_playful_pounce", s, Action::CursorPlayfulPounce));
    }
    if near && m.valence > 0.5 && m.arousal > 0.55 {
        let weight = w(Phase::Roll) + (m.valence + m.arousal) * 0.5;
        let s = weight * focus.bias(Phase::Roll) * noise(c) * 0.5;
        out.push(Candidate::new("cursor_roll", s, Action::CursorRoll));
    }
}

/// `_self_flourish_candidates`
fn self_flourish_candidates(
    c: &mut Creature,
    from_idle: bool,
    state_timer_expired: bool,
    focus: Focus,
    weights: &[f64; 12],
    out: &mut Vec<Candidate>,
) {
    if !(from_idle && state_timer_expired) || c.acts_as_hunter() {
        return;
    }
    let m = c.mood;
    if !(m.valence > 0.4 && m.arousal > 0.55) {
        return;
    }
    let s = weights[Phase::Roll.index()] * focus.bias(Phase::Roll) * noise(c) * 0.6;
    out.push(Candidate::new("self_roll", s, Action::SelfRoll));
    let s = weights[Phase::Jump.index()] * focus.bias(Phase::Jump) * noise(c) * 0.6;
    out.push(Candidate::new("self_spring", s, Action::SelfSpring));
    let s = weights[Phase::Zoomies.index()] * focus.bias(Phase::Zoomies) * noise(c) * 0.6;
    out.push(Candidate::new("self_zoomies", s, Action::SelfZoomies));
}

/// Run the winning candidate's `execute`.
pub fn execute(c: &mut Creature, env: &mut Env<'_>, action: Action, mx: f64, my: f64) {
    match action {
        Action::Idle => {
            if c.state == crate::state::State::Idle && c.state_timer > 0.0 {
                return;
            }
            c.enter_idle();
        }
        Action::EngageCursor { cursor_still } => {
            if cursor_still {
                c.enter_alert(mx, my);
            } else {
                c.enter_approach(mx, my);
            }
        }
        Action::Hop => {
            let (x, y, heading, size) = (c.kin.x, c.kin.y, c.kin.heading, c.kin.size);
            let a = heading + c.rng.uniform(-0.65, 0.65);
            let tx = x + a.cos() * size * c.rng.uniform(1.0, 2.0);
            let a = heading + c.rng.uniform(-0.65, 0.65);
            let ty = y + a.sin() * size * c.rng.uniform(1.0, 2.0);
            let power = rand_range(c.personality.get("hop_power"), 0.34, 0.58, &mut c.rng);
            c.enter_coil(After::Wander, power, Some((tx, ty)));
        }
        Action::Observe { ax, ay, target } => c.enter_observe(ax, ay, target),
        Action::AlertRetreat => c.enter_retreat(mx, my),
        Action::AlertChase => c.enter_chase(mx, my),
        Action::AlertApproach => c.enter_approach(mx, my),
        Action::DriftRun => c.enter_drift_run(None),
        Action::RepairWeb(web) => {
            c.begin_repair(web, env);
        }
        Action::AdoptWeb(web) => {
            c.begin_adopt(web, env);
        }
        Action::WeaveNew => {
            c.begin_weave(env);
        }
        Action::WebWalk(web) => {
            c.begin_web_walk(web, env);
        }
        Action::ShootWebCursor { dist_to_cursor } => {
            c.maybe_shoot_web_at_cursor(dist_to_cursor, mx, my, env);
        }
        Action::SocialPlay { mate, mate_x, mate_y, d } => {
            if d > c.kin.size * 3.0 && c.mood.arousal > 0.45 && c.rng.random() < 0.5 {
                c.enter_aim(mate_x, mate_y, Some(mate), After::Play, (0.4, 0.8), 0.1, env.population);
            } else {
                c.enter_play(mate, None, env.population);
            }
        }
        Action::SocialInspect { mate, mate_x, mate_y } => c.enter_inspect(mate_x, mate_y, Some(mate)),
        Action::SocialCuddle { mate, mate_x, mate_y } => c.enter_cuddle(mate_x, mate_y, Some(mate)),
        Action::CursorPounce { abort_chance } => {
            c.enter_aim(mx, my, None, After::Outcome, (0.55, 1.2), abort_chance, env.population)
        }
        Action::CursorInspect => c.enter_inspect(mx, my, None),
        Action::CursorCuddle => c.enter_cuddle(mx, my, None),
        Action::CursorPlayfulPounce => c.enter_aim(mx, my, None, After::Outcome, (0.35, 0.7), 0.1, env.population),
        Action::CursorRoll => {
            let direction = angle_to(c.kin.x, c.kin.y, mx, my) + PI;
            c.enter_roll(Some(direction));
        }
        Action::SelfRoll => c.enter_roll(None),
        Action::SelfSpring => {
            let power = c.rng.uniform(0.7, 1.2);
            c.enter_spring(After::Idle, power);
        }
        Action::SelfZoomies => c.enter_zoomies(),
        Action::ScheduledPhase { focus } => {
            for _ in 0..PHASES.len() + 1 {
                let plan = {
                    let (sched, rng) = c.scheduler_parts();
                    sched.choose(focus, rng)
                };
                let Some(plan) = plan else {
                    c.enter_idle();
                    return;
                };
                if c.dispatch_scheduled_phase(plan.phase_id, focus, mx, my, env.population) {
                    c.state_timer = plan.duration;
                    return;
                }
                c.phase_scheduler.finish();
            }
            c.enter_idle();
        }
    }
}
