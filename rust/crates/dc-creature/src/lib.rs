//! `dc-creature`: the simulation half of `src/desktop_bug/creature/` — the
//! `Creature` state and construction, the per-frame `update`, the mind
//! (mood, perception, phase scheduler, arbiter, expression) and the
//! personality state machine (`behaviour/*`), plus the logic of the damage
//! numbers, the web net and the regal jumper's gestures.
//!
//! Entry point: [`Creature::new`] then [`Creature::update`] each frame with an
//! [`world::Env`]; painters read [`Creature::render_snapshot`] and
//! `creature.kin`. See `PORT_STATUS.md` for the full manager contract.

pub mod arbiter;
pub mod behaviour;
pub mod constants;
pub mod creature;
pub mod damage_numbers;
pub mod expression;
pub mod labels;
pub mod manipulation;
pub mod mood;
pub mod perception;
pub mod phase_scheduler;
pub mod progression;
pub mod pymath;
pub mod regal;
pub mod snapshot;
pub mod state;
pub mod update;
pub mod web_net;
pub mod world;
pub mod world_impls;

pub use creature::{Creature, CreatureConfig};
pub use labels::{ApproxLabelMetrics, LabelMetrics, LabelSpec};
pub use snapshot::RenderSnapshot;
pub use state::State;
pub use update::UpdateOutcome;
pub use world::{Command, CreatureId, CreatureView, Env, Population};
