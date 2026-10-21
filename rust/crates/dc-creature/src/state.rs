//! The creature FSM state names (`self.state` strings in the Python).
//!
//! Python keeps the state as a free string; the port uses a closed enum with
//! the exact names (`as_str`/`parse`). Every name the Python code ever
//! assigns is listed; [`State::Unknown`] stands for any other string a
//! caller or test might set.

use dc_kinematics::MotionState;
use serde::{Deserialize, Serialize};

macro_rules! states {
    ($($name:ident),* $(,)?) => {
        /// A creature's FSM state.
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum State {
            #[default]
            Idle,
            $($name,)*
            /// Any string the Python code never assigns.
            Unknown,
        }

        impl State {
            pub fn as_str(self) -> &'static str {
                match self {
                    State::Idle => "Idle",
                    $(State::$name => stringify!($name),)*
                    State::Unknown => "Unknown",
                }
            }

            pub fn parse(name: &str) -> State {
                match name {
                    "Idle" => State::Idle,
                    $(stringify!($name) => State::$name,)*
                    _ => State::Unknown,
                }
            }
        }
    };
}

states!(
    Alert,
    Approach,
    Chase,
    Retreat,
    Startled,
    Wander,
    DriftRun,
    Observe,
    Inspect,
    Cuddle,
    Aim,
    Coil,
    Jump,
    Land,
    Catch,
    Feed,
    Play,
    Zoom,
    Roll,
    WeaveApproach,
    Weave,
    WebApproach,
    WebWalk,
    WebAim,
    WebShot,
    RepairApproach,
    Repair,
    Dragged,
    Downed,
    Player,
    JobTravel,
    JobBuild,
    JobPatrol,
    JobGuardAlert,
    JobScoutTravel,
    JobScoutReport,
    JobWebTravel,
    JobWebRepair,
    JobWebWeave,
    JobHuntPatrol,
    JobHuntReturn,
);

impl State {
    /// The kinematics' view of this state.
    pub fn motion(self) -> MotionState {
        MotionState::from_name(self.as_str())
    }

    /// `state in (...)`
    #[inline]
    pub fn is_in(self, set: &[State]) -> bool {
        set.contains(&self)
    }
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
