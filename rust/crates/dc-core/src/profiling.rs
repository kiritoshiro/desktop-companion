//! Port of `support/profiling.py`: per-system frame timing, so the overlay's
//! cost is measured rather than guessed.
//!
//! Named sections accumulate per frame, a rolling window keeps the recent
//! history, and the numbers come out as a table or as the lines of the
//! overlay's HUD. It is off unless `DESKTOP_BUG_PROFILE` asks for it.
//!
//! Python kept one module-level profiler (`get_profiler`, `set_profiler`)
//! that was a `NullProfiler` while off. Here the active profiler is a
//! thread-local `Option<FrameProfiler>` (the overlay runs on one thread,
//! and parallel tests stay apart): `None` is the null profiler, so an
//! unprofiled [`section`] is one thread-local read and no clock read.

use crate::math::py_round;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::time::Instant;

pub const PROFILE_ENV: &str = "DESKTOP_BUG_PROFILE";
pub const HUD_ENV: &str = "DESKTOP_BUG_PROFILE_HUD";
/// Two seconds of history at 60 FPS. Long enough that one slow frame does
/// not dominate the mean, short enough that the HUD reacts while you watch.
pub const DEFAULT_WINDOW: usize = 120;
/// The order sections appear in the HUD and the table. A section that is not
/// listed still records; it simply sorts after these.
pub const SECTION_ORDER: [&str; 12] = [
    "frame",
    "behaviour",
    "creatures",
    "render",
    "paint",
    "repaint-region",
    "webs",
    "mouse-webs",
    "flies",
    "jobs",
    "desktop",
    "desktop-probe",
];

fn truthy(value: Option<&str>) -> bool {
    value.is_some_and(|v| !matches!(v.trim().to_lowercase().as_str(), "" | "0" | "false" | "no" | "off"))
}

fn os_env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// `profiling_requested(env)`: true when `env` asks for timing.
pub fn profiling_requested_in(env: impl Fn(&str) -> Option<String>) -> bool {
    truthy(env(PROFILE_ENV).as_deref())
}

/// `profiling_requested()` against the process environment.
pub fn profiling_requested() -> bool {
    profiling_requested_in(os_env)
}

/// `hud_requested(env)`: the on-screen HUD as well as the timing. It follows
/// `DESKTOP_BUG_PROFILE` and is on unless `DESKTOP_BUG_PROFILE_HUD` turns it off.
pub fn hud_requested_in(env: impl Fn(&str) -> Option<String>) -> bool {
    profiling_requested_in(&env) && truthy(Some(env(HUD_ENV).as_deref().unwrap_or("1")))
}

/// `hud_requested()` against the process environment.
pub fn hud_requested() -> bool {
    hud_requested_in(os_env)
}

/// Nearest-rank percentile; 0.0 for no samples. Nearest-rank rather than
/// interpolating, because a frame that really happened is a more honest
/// answer than a number that never did.
pub fn percentile(samples: impl IntoIterator<Item = f64>, fraction: f64) -> f64 {
    let mut ordered: Vec<f64> = samples.into_iter().collect();
    if ordered.is_empty() {
        return 0.0;
    }
    ordered.sort_by(f64::total_cmp);
    let rank = py_round(fraction.clamp(0.0, 1.0) * (ordered.len() - 1) as f64) as usize;
    ordered[rank]
}

/// Accumulates named section times per frame and keeps a rolling window.
pub struct FrameProfiler {
    clock: Box<dyn Fn() -> f64>,
    window: usize,
    history: HashMap<&'static str, VecDeque<f64>>,
    current: HashMap<&'static str, f64>,
    frame_started: f64,
    frame_open: bool,
    pub frames: u64,
}

impl FrameProfiler {
    /// Timed with `Instant` (Python's `time.perf_counter`).
    pub fn new(window: usize) -> Self {
        let origin = Instant::now();
        Self::with_clock(window, move || origin.elapsed().as_secs_f64())
    }

    /// `clock` returns seconds; tests pass a hand-advanced one.
    pub fn with_clock(window: usize, clock: impl Fn() -> f64 + 'static) -> Self {
        FrameProfiler {
            clock: Box::new(clock),
            window: window.max(1),
            history: HashMap::new(),
            current: HashMap::new(),
            frame_started: 0.0,
            frame_open: false,
            frames: 0,
        }
    }

    pub fn now(&self) -> f64 {
        (self.clock)()
    }

    /// Record time against a section. Inside a frame, sections accumulate
    /// rather than overwrite, so one entered once per creature reports what
    /// that system cost across the whole frame. Outside a frame it records a
    /// sample of its own: painting happens after the tick has returned, so it
    /// is not inside any frame and would otherwise be silently dropped.
    pub fn add(&mut self, name: &'static str, seconds: f64) {
        if self.frame_open {
            *self.current.entry(name).or_insert(0.0) += seconds;
        } else {
            self.record(name, seconds * 1000.0);
        }
    }

    fn record(&mut self, name: &'static str, milliseconds: f64) {
        let history = self.history.entry(name).or_insert_with(|| VecDeque::with_capacity(self.window));
        if history.len() == self.window {
            history.pop_front();
        }
        history.push_back(milliseconds);
    }

    pub fn begin_frame(&mut self) {
        self.current.clear();
        self.frame_started = self.now();
        self.frame_open = true;
    }

    pub fn end_frame(&mut self) {
        let frame = self.now() - self.frame_started;
        self.current.insert("frame", frame);
        self.frame_open = false;
        let current = std::mem::take(&mut self.current);
        for (&name, &seconds) in &current {
            self.record(name, seconds * 1000.0);
        }
        // Keep the map's capacity for the next frame.
        self.current = current;
        self.current.clear();
        self.frames += 1;
    }

    pub fn reset(&mut self) {
        self.history.clear();
        self.current.clear();
        self.frame_open = false;
        self.frames = 0;
    }

    /// Section names, the well-known ones first and the rest alphabetical.
    pub fn names(&self) -> Vec<&'static str> {
        let mut names: Vec<&'static str> =
            SECTION_ORDER.iter().copied().filter(|n| self.history.contains_key(n)).collect();
        let mut rest: Vec<&'static str> = self.history.keys().copied().filter(|n| !SECTION_ORDER.contains(n)).collect();
        rest.sort_unstable();
        names.extend(rest);
        names
    }

    /// `(mean_ms, p95_ms, max_ms)` over the rolling window.
    pub fn stats(&self, name: &str) -> (f64, f64, f64) {
        match self.history.get(name) {
            Some(h) if !h.is_empty() => {
                let mean = h.iter().sum::<f64>() / h.len() as f64;
                let worst = h.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                (mean, percentile(h.iter().copied(), 0.95), worst)
            }
            _ => (0.0, 0.0, 0.0),
        }
    }

    pub fn table(&self) -> Vec<(&'static str, f64, f64, f64)> {
        self.names()
            .into_iter()
            .map(|n| {
                let (mean, p95, worst) = self.stats(n);
                (n, mean, p95, worst)
            })
            .collect()
    }

    /// Short lines for the on-screen HUD: the frame, then the six costliest
    /// systems, then the frame rate the mean frame allows.
    pub fn hud_lines(&self) -> Vec<String> {
        let rows = self.table();
        if rows.is_empty() {
            return Vec::new();
        }
        let frame: Vec<_> = rows.iter().filter(|r| r.0 == "frame").copied().collect();
        let mut others: Vec<_> = rows.iter().filter(|r| r.0 != "frame").copied().collect();
        // Stable, so equal means keep the table's order (as Python's `sorted`).
        others.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut lines: Vec<String> = frame
            .iter()
            .chain(others.iter().take(6))
            .map(|(name, mean, p95, _)| format!("{name:<14} {mean:5.2} ms  p95 {p95:5.2}"))
            .collect();
        if let Some(&(_, mean, _, _)) = frame.first()
            && mean > 0.0
        {
            lines.push(format!("{:<14} {:5.1}", "~fps", 1000.0 / mean));
        }
        lines
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<FrameProfiler>> = const { RefCell::new(None) };
}

/// Run `f` on the active profiler (`get_profiler()`); `None` while off, or
/// when called from inside another `with_profiler`.
pub fn with_profiler<R>(f: impl FnOnce(&mut FrameProfiler) -> R) -> Option<R> {
    ACTIVE.with(|active| {
        let mut active = active.try_borrow_mut().ok()?;
        active.as_mut().map(f)
    })
}

/// `set_profiler`: install `profiler` (`None` switches measurement off) and
/// return the one it replaces.
pub fn set_profiler(profiler: Option<FrameProfiler>) -> Option<FrameProfiler> {
    ACTIVE.with(|active| active.replace(profiler))
}

/// `start_profiling`: switch measurement on with a fresh profiler.
pub fn start_profiling(window: usize) {
    set_profiler(Some(FrameProfiler::new(window)));
}

/// `stop_profiling`, handing back the profiler so its numbers can be read.
pub fn stop_profiling() -> Option<FrameProfiler> {
    set_profiler(None)
}

/// `profiler_from_env`: start measuring when the environment asks for it.
/// True when a profiler is active afterwards.
pub fn profiler_from_env() -> bool {
    if profiling_requested() {
        start_profiling(DEFAULT_WINDOW);
    }
    enabled()
}

/// The profiler's `enabled`.
pub fn enabled() -> bool {
    ACTIVE.with(|active| active.try_borrow().map(|a| a.is_some()).unwrap_or(true))
}

pub fn begin_frame() {
    with_profiler(FrameProfiler::begin_frame);
}

pub fn end_frame() {
    with_profiler(FrameProfiler::end_frame);
}

/// The active profiler's HUD lines; none while off.
pub fn hud_lines() -> Vec<String> {
    with_profiler(|p| p.hud_lines()).unwrap_or_default()
}

/// A named span (Python's `with profiler.section(name):`), recorded when it
/// is dropped. While off it holds nothing and records nothing.
#[must_use = "a section times until it is dropped"]
pub struct Section {
    name: &'static str,
    started: Option<f64>,
}

pub fn section(name: &'static str) -> Section {
    Section { name, started: with_profiler(|p| p.now()) }
}

impl Drop for Section {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            with_profiler(|p| {
                let elapsed = p.now() - started;
                p.add(self.name, elapsed);
            });
        }
    }
}

/// `f` timed as section `name`.
pub fn timed<R>(name: &'static str, f: impl FnOnce() -> R) -> R {
    let _span = section(name);
    f()
}

/// `begin_frame` now and `end_frame` when dropped, so every way out of a
/// tick closes the frame.
#[must_use = "a frame times until it is dropped"]
pub struct Frame(());

pub fn frame() -> Frame {
    begin_frame();
    Frame(())
}

impl Drop for Frame {
    fn drop(&mut self) {
        end_frame();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    /// A profiler on a hand-advanced clock, installed for this test's thread.
    fn install(window: usize) -> Rc<Cell<f64>> {
        let clock = Rc::new(Cell::new(0.0));
        let read = clock.clone();
        set_profiler(Some(FrameProfiler::with_clock(window, move || read.get())));
        clock
    }

    fn spend(clock: &Cell<f64>, name: &'static str, ms: f64) {
        let _s = section(name);
        clock.set(clock.get() + ms / 1000.0);
    }

    fn stats(name: &str) -> (f64, f64, f64) {
        with_profiler(|p| p.stats(name)).unwrap()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn profiler_accumulates_and_windows() {
        let clock = install(3);
        begin_frame();
        // Entered twice in one frame, as a per-creature section is: the
        // frame's cost for that system is the sum, not the last one.
        spend(&clock, "creatures", 2.0);
        spend(&clock, "creatures", 3.0);
        end_frame();
        let (mean, _, worst) = stats("creatures");
        assert!(close(mean, 5.0) && close(worst, 5.0), "{mean} {worst}");
        assert!(close(stats("frame").0, 5.0));

        // Four frames into a window of three leaves the first behind.
        for ms in [10.0, 20.0, 30.0] {
            let _f = frame();
            spend(&clock, "creatures", ms);
        }
        let (mean, _, worst) = stats("creatures");
        assert!(close(mean, 20.0), "the rolling window kept an expired frame: {mean}");
        assert!(close(worst, 30.0));
        assert_eq!(with_profiler(|p| p.frames), Some(4));
        stop_profiling();
    }

    #[test]
    fn percentile_is_nearest_rank() {
        assert!(close(percentile([1.0, 2.0, 3.0, 4.0], 0.95), 4.0));
        assert_eq!(percentile([], 0.95), 0.0);
        // round(0.5 * 3) == 2 (half to even), not 1.5's interpolation.
        assert_eq!(percentile([4.0, 1.0, 3.0, 2.0], 0.5), 3.0);
        assert_eq!(percentile([1.0, 2.0, 3.0], 0.0), 1.0);
        assert_eq!(percentile([1.0, 2.0, 3.0], 7.0), 3.0);
    }

    #[test]
    fn painting_is_recorded_after_the_frame() {
        let clock = install(8);
        for _ in 0..3 {
            {
                let _f = frame();
                spend(&clock, "creatures", 1.0);
            }
            // After the frame, the way the paint really arrives.
            spend(&clock, "paint", 4.0);
        }
        assert!(close(stats("paint").0, 4.0), "painting was not recorded: {:?}", stats("paint"));
        // And it did not contaminate the frame total, which never held it.
        assert!(close(stats("frame").0, 1.0), "{:?}", stats("frame"));
        stop_profiling();
    }

    #[test]
    fn off_is_inert() {
        stop_profiling();
        assert!(!enabled());
        {
            let _f = frame();
            let s = section("anything");
            assert!(s.started.is_none(), "an unprofiled section read the clock");
            assert_eq!(timed("x", || 7), 7);
        }
        assert!(hud_lines().is_empty());
        assert!(with_profiler(|p| p.frames).is_none());
    }

    #[test]
    fn a_section_outlives_stop() {
        let clock = install(4);
        let s = section("webs");
        clock.set(0.002);
        let stopped = stop_profiling().unwrap();
        drop(s);
        assert!(stopped.names().is_empty());
    }

    #[test]
    fn env_switches() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string())
        };
        assert!(!profiling_requested_in(env(&[])));
        assert!(!profiling_requested_in(env(&[(PROFILE_ENV, "0")])));
        assert!(!profiling_requested_in(env(&[(PROFILE_ENV, " OFF ")])));
        assert!(!profiling_requested_in(env(&[(PROFILE_ENV, "")])));
        assert!(!profiling_requested_in(env(&[(PROFILE_ENV, "no")])));
        assert!(!profiling_requested_in(env(&[(PROFILE_ENV, "False")])));
        assert!(profiling_requested_in(env(&[(PROFILE_ENV, "1")])));
        assert!(profiling_requested_in(env(&[(PROFILE_ENV, "yes")])));
        // The HUD follows profiling and can be turned off on its own.
        assert!(hud_requested_in(env(&[(PROFILE_ENV, "1")])));
        assert!(!hud_requested_in(env(&[(PROFILE_ENV, "1"), (HUD_ENV, "0")])));
        assert!(!hud_requested_in(env(&[(HUD_ENV, "1")])));
    }

    #[test]
    fn names_put_the_known_sections_first() {
        let mut p = FrameProfiler::with_clock(4, || 0.0);
        for name in ["zeta", "paint", "combat", "frame", "behaviour"] {
            p.add(name, 0.001);
        }
        assert_eq!(p.names(), ["frame", "behaviour", "paint", "combat", "zeta"]);
        p.reset();
        assert!(p.names().is_empty() && p.table().is_empty());
    }

    #[test]
    fn hud_lines_lead_with_the_worst() {
        let clock = install(4);
        {
            let _f = frame();
            for (name, ms) in [("webs", 0.5), ("render", 9.0), ("creatures", 3.0)] {
                spend(&clock, name, ms);
            }
        }
        let lines = hud_lines();
        assert_eq!(
            lines,
            [
                "frame          12.50 ms  p95 12.50",
                "render          9.00 ms  p95  9.00",
                "creatures       3.00 ms  p95  3.00",
                "webs            0.50 ms  p95  0.50",
                "~fps            80.0",
            ]
        );
        stop_profiling();
    }

    #[test]
    fn hud_lines_keep_six_systems() {
        let mut p = FrameProfiler::with_clock(4, || 0.0);
        p.begin_frame();
        for (i, name) in ["a", "b", "c", "d", "e", "f", "g", "h"].into_iter().enumerate() {
            p.add(name, (i + 1) as f64 / 1000.0);
        }
        p.end_frame();
        let lines = p.hud_lines();
        // The frame, six systems; no fps line, the fake clock never moved.
        assert_eq!(lines.len(), 7, "{lines:?}");
        assert!(lines[0].starts_with("frame "));
        assert!(lines[1].starts_with("h ") && lines[6].starts_with("c "), "{lines:?}");
    }
}
