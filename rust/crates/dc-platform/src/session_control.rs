//! Port of app/session_control.py: graceful shutdown between the settings
//! window and the overlay through a small stop-request file, plus the
//! preset/controls mtime polling helpers the engine used beside it
//! (`_check_preset_reload`, `_reload_controls_if_changed`, PRESET_WATCH_MS).
//!
//! The file is the fallback; the live channel ([`crate::live_channel`]) is
//! the fast path. Both stay in place, exactly as in the Python.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const STOP_REQUEST_NAME: &str = "stop-request";
/// How long the settings window waits for the overlay to save and exit.
pub const STOP_GRACE_SECONDS: f64 = 5.0;
pub const STOP_POLL_SECONDS: f64 = 0.1;
/// How often the overlay checks the launched preset file (engine.py).
pub const PRESET_WATCH_MS: u64 = 700;

pub fn stop_request_path(state_dir: impl AsRef<Path>) -> PathBuf {
    state_dir.as_ref().join(STOP_REQUEST_NAME)
}

/// Ask a running overlay to save and quit. Returns whether the request could
/// be left at all.
pub fn request_stop(state_dir: impl AsRef<Path>) -> bool {
    let path = stop_request_path(&state_dir);
    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return false;
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    std::fs::write(&path, now.to_string()).is_ok()
}

/// True exactly once per request, removing it as it is read.
pub fn consume_stop_request(state_dir: impl AsRef<Path>) -> bool {
    let path = stop_request_path(state_dir);
    if !path.is_file() {
        return false;
    }
    std::fs::remove_file(&path).is_ok()
}

/// Drop any pending request (on overlay start and after a stop finishes).
pub fn clear_stop_request(state_dir: impl AsRef<Path>) {
    let _ = std::fs::remove_file(stop_request_path(state_dir));
}

/// What `stop_process` needs from a process handle (`Popen` in Python).
pub trait StoppableProcess {
    /// Whether the process is still running (`poll() is None`).
    fn is_running(&mut self) -> bool;
    /// Kill it outright, and wait until it has gone, so nothing it was
    /// writing (a save of the colony) can land after the caller moves on.
    fn terminate(&mut self);
}

impl StoppableProcess for std::process::Child {
    fn is_running(&mut self) -> bool {
        matches!(self.try_wait(), Ok(None))
    }

    fn terminate(&mut self) {
        if self.kill().is_ok() {
            let _ = self.wait();
        }
    }
}

/// How a stop ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopOutcome {
    NotRunning,
    Graceful,
    Terminated,
}

impl StopOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            StopOutcome::NotRunning => "not-running",
            StopOutcome::Graceful => "graceful",
            StopOutcome::Terminated => "terminated",
        }
    }
}

/// Stop the overlay, preferring a save over a kill. `sleep` is injectable so
/// the settings window can keep repainting while it waits and tests do not
/// spend real time.
pub fn stop_process<P: StoppableProcess>(
    process: Option<&mut P>,
    state_dir: impl AsRef<Path>,
    grace: f64,
    poll: f64,
    mut sleep: impl FnMut(f64),
) -> StopOutcome {
    let state_dir = state_dir.as_ref();
    let Some(process) = process else {
        clear_stop_request(state_dir);
        return StopOutcome::NotRunning;
    };
    if !process.is_running() {
        clear_stop_request(state_dir);
        return StopOutcome::NotRunning;
    }
    if !request_stop(state_dir) {
        process.terminate();
        return StopOutcome::Terminated;
    }
    let mut waited = 0.0;
    let step = poll.max(0.01);
    while waited < grace {
        if !process.is_running() {
            clear_stop_request(state_dir);
            return StopOutcome::Graceful;
        }
        sleep(step);
        waited += step;
    }
    clear_stop_request(state_dir);
    process.terminate();
    StopOutcome::Terminated
}

/// [`stop_process`] with the default grace, poll and a real sleep.
pub fn stop_process_default<P: StoppableProcess>(process: Option<&mut P>, state_dir: impl AsRef<Path>) -> StopOutcome {
    stop_process(process, state_dir, STOP_GRACE_SECONDS, STOP_POLL_SECONDS, |s| {
        std::thread::sleep(Duration::from_secs_f64(s))
    })
}

/// A file's modification time, `None` when it cannot be read.
pub fn file_mtime(path: impl AsRef<Path>) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Watches one file's mtime the way the engine watched the launched preset:
/// [`MtimeWatcher::check`] reports a change without recording it, so a file
/// caught mid-write is retried; the caller records the mtime with
/// [`MtimeWatcher::commit`] once the content loaded.
#[derive(Clone, Debug)]
pub struct MtimeWatcher {
    pub path: PathBuf,
    pub interval_ms: u64,
    last_mtime: Option<SystemTime>,
    last_check_ms: Option<f64>,
}

impl MtimeWatcher {
    /// Starts from the file's current mtime, so an unchanged file never
    /// reports.
    pub fn new(path: impl Into<PathBuf>, interval_ms: u64) -> Self {
        let path = path.into();
        let last_mtime = file_mtime(&path);
        Self { path, interval_ms, last_mtime, last_check_ms: None }
    }

    pub fn last_mtime(&self) -> Option<SystemTime> {
        self.last_mtime
    }

    /// Is a check due at `now_ms`? Marks it done when it is.
    pub fn due(&mut self, now_ms: f64) -> bool {
        if let Some(last) = self.last_check_ms
            && now_ms - last < self.interval_ms as f64
        {
            return false;
        }
        self.last_check_ms = Some(now_ms);
        true
    }

    /// The new mtime when the file exists and changed since the last commit
    /// (preset semantics: a missing file is "no change").
    pub fn check(&self) -> Option<SystemTime> {
        let mtime = file_mtime(&self.path)?;
        if Some(mtime) == self.last_mtime { None } else { Some(mtime) }
    }

    /// Controls semantics: a file that appears or disappears also counts.
    /// Returns `Some(new_state)` on change.
    pub fn check_presence(&self) -> Option<Option<SystemTime>> {
        let mtime = file_mtime(&self.path);
        if mtime == self.last_mtime { None } else { Some(mtime) }
    }

    pub fn commit(&mut self, mtime: Option<SystemTime>) {
        self.last_mtime = mtime;
    }

    /// Record whatever is on disk now (after a pushed preset was applied, so
    /// the file poll does not reload the same content again).
    pub fn refresh(&mut self) {
        if let Some(m) = file_mtime(&self.path) {
            self.last_mtime = Some(m);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "dc-session-{tag}-{}-{}",
            std::process::id(),
            crate::win::tick_ms() as u64
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    struct FakeProcess {
        polls: u32,
        exits_after: Option<u32>,
        terminated: bool,
    }

    impl FakeProcess {
        fn new(exits_after: Option<u32>) -> Self {
            Self { polls: 0, exits_after, terminated: false }
        }
    }

    impl StoppableProcess for FakeProcess {
        fn is_running(&mut self) -> bool {
            self.polls += 1;
            match self.exits_after {
                None => true,
                Some(n) => self.polls <= n,
            }
        }
        fn terminate(&mut self) {
            self.terminated = true;
        }
    }

    #[test]
    fn request_file() {
        let tmp = temp_dir("request");
        assert!(!consume_stop_request(&tmp), "no request should read as none");
        assert!(request_stop(&tmp));
        assert!(stop_request_path(&tmp).is_file());
        assert!(consume_stop_request(&tmp));
        assert!(!consume_stop_request(&tmp));
        assert!(!stop_request_path(&tmp).exists());

        request_stop(&tmp);
        clear_stop_request(&tmp);
        assert!(!consume_stop_request(&tmp), "clear left a stale request");
        clear_stop_request(&tmp);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn stop_process_paths() {
        let tmp = temp_dir("stop");
        let mut slept = Vec::new();

        let mut proc = FakeProcess::new(Some(2));
        assert_eq!(
            stop_process(Some(&mut proc), &tmp, STOP_GRACE_SECONDS, STOP_POLL_SECONDS, |s| slept.push(s)),
            StopOutcome::Graceful
        );
        assert!(!proc.terminated, "a responsive overlay was killed anyway");
        assert!(!stop_request_path(&tmp).exists(), "request outlived the stop");
        assert!(slept.len() <= 2, "{slept:?}");

        slept.clear();
        let mut proc = FakeProcess::new(None);
        assert_eq!(stop_process(Some(&mut proc), &tmp, 0.5, 0.1, |s| slept.push(s)), StopOutcome::Terminated);
        assert!(proc.terminated);
        assert!(!stop_request_path(&tmp).exists());
        assert_eq!(slept.len(), 5, "{slept:?}");
        assert!(slept.iter().all(|s| *s == 0.1));

        assert_eq!(stop_process::<FakeProcess>(None, &tmp, 5.0, 0.1, |_| {}), StopOutcome::NotRunning);
        let mut already = FakeProcess::new(Some(0));
        assert_eq!(stop_process(Some(&mut already), &tmp, 5.0, 0.1, |_| {}), StopOutcome::NotRunning);
        assert!(!already.terminated);
        assert_eq!(StopOutcome::Graceful.as_str(), "graceful");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn mtime_watcher_reports_changes_until_committed() {
        let tmp = temp_dir("mtime");
        let path = tmp.join("preset.json");
        std::fs::write(&path, b"{}").unwrap();
        let mut w = MtimeWatcher::new(&path, PRESET_WATCH_MS);
        assert!(w.due(0.0));
        assert!(!w.due(100.0));
        assert!(w.due(700.0));
        assert_eq!(w.check(), None, "an unchanged file reported a change");

        let later = SystemTime::now() + Duration::from_secs(5);
        let f = std::fs::File::options().write(true).open(&path).unwrap();
        f.set_modified(later).unwrap();
        drop(f);
        let changed = w.check().expect("change not seen");
        assert_eq!(w.check(), Some(changed), "check must not record the mtime");
        w.commit(Some(changed));
        assert_eq!(w.check(), None);

        std::fs::remove_file(&path).unwrap();
        assert_eq!(w.check(), None, "a missing preset is not a change");
        assert_eq!(w.check_presence(), Some(None), "a missing controls file is a change");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
