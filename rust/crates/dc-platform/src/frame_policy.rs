//! Port of support/frame_policy.py: decide what frame rate the overlay
//! deserves right now.
//!
//! A desktop pet that keeps painting at 60 FPS behind a fullscreen game, or
//! on a laptop running off its battery, is being a bad guest. The decision is
//! a pure function of two facts about the machine; the probes that establish
//! those facts are separate, cheap, and answer "no" when they cannot tell.
//!
//! Differs from Python: a window counts as fullscreen only when it covers
//! every monitor. Python checked only the monitor the window is on, so a game
//! on one screen slowed the spiders still visible on the other to 10 FPS.

use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

use crate::monitors::monitors;
use crate::win::{class_name, hwnd_to_raw, rect_from_user32};

/// Nothing the overlay draws can be seen behind a fullscreen window.
pub const FULLSCREEN_FPS: f64 = 10.0;
/// Still watchable on battery, just cheaper.
pub const BATTERY_FPS: f64 = 30.0;
/// Twice a second reacts faster than a person can notice.
pub const POLL_MS: i64 = 500;
/// Maximised windows can sit a pixel or two outside the work area.
pub const FULLSCREEN_SLACK_PX: i32 = 2;

/// Class names that are the desktop itself rather than an application
/// (compared case-folded).
pub const DESKTOP_CLASSES: [&str; 4] = ["progman", "workerw", "shell_traywnd", "windows.ui.core.corewindow"];

/// The frame rate to run at, never above what was asked for. Fullscreen wins
/// over battery because it is the stronger signal.
pub fn decide_fps(target_fps: f64, fullscreen: bool, on_battery: bool) -> f64 {
    let target = target_fps.max(1.0);
    if fullscreen {
        return target.min(FULLSCREEN_FPS);
    }
    if on_battery {
        return target.min(BATTERY_FPS);
    }
    target
}

/// Whether a window rectangle covers a monitor rectangle (both `l, t, r, b`)
/// within [`FULLSCREEN_SLACK_PX`].
pub fn covers_monitor(window: (i32, i32, i32, i32), screen: (i32, i32, i32, i32)) -> bool {
    let slack = FULLSCREEN_SLACK_PX;
    window.0 <= screen.0 + slack
        && window.1 <= screen.1 + slack
        && window.2 >= screen.2 - slack
        && window.3 >= screen.3 - slack
}

/// Whether a window rectangle covers every monitor (all `l, t, r, b`), so
/// no spider is left in view. False for an empty list.
pub fn covers_every_monitor(window: (i32, i32, i32, i32), screens: &[(i32, i32, i32, i32)]) -> bool {
    !screens.is_empty() && screens.iter().all(|&s| covers_monitor(window, s))
}

/// True when the focused window covers every monitor, so a fullscreen game
/// on one screen of two does not count. False wherever it cannot tell.
pub fn foreground_window_is_fullscreen(exclude_hwnd: Option<isize>) -> bool {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() || exclude_hwnd == Some(hwnd_to_raw(hwnd)) {
        return false;
    }
    let cls = class_name(hwnd).trim().to_lowercase();
    if DESKTOP_CLASSES.contains(&cls.as_str()) {
        return false;
    }
    let Some(rect) = rect_from_user32(hwnd) else { return false };
    let screens: Vec<_> =
        monitors().iter().map(|m| (m.rect.x, m.rect.y, m.rect.x + m.rect.w, m.rect.y + m.rect.h)).collect();
    covers_every_monitor((rect.left, rect.top, rect.right, rect.bottom), &screens)
}

/// True when the machine runs from its battery; false when plugged in or
/// unknown, so an unknown answer never throttles anything.
pub fn on_battery() -> bool {
    let mut status = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut status) }.is_err() {
        return false;
    }
    // 0 offline, 1 online, 255 unknown.
    status.ACLineStatus == 0
}

type FullscreenProbe = Box<dyn FnMut(Option<isize>) -> bool>;
type BatteryProbe = Box<dyn FnMut() -> bool>;

/// Tracks the machine state and reports the frame rate that follows from it,
/// re-probing only when `poll_ms` has passed.
pub struct FramePolicy {
    pub target_fps: f64,
    pub poll_ms: i64,
    fullscreen_probe: FullscreenProbe,
    battery_probe: BatteryProbe,
    last_poll_ms: Option<f64>,
    pub fullscreen: bool,
    pub battery: bool,
    pub current_fps: f64,
    pub reason: &'static str,
}

impl FramePolicy {
    /// A policy using the real Win32 probes and [`POLL_MS`].
    pub fn new(target_fps: f64) -> Self {
        Self::with_probes(target_fps, POLL_MS, foreground_window_is_fullscreen, on_battery)
    }

    /// A policy with injected probes (tests, or a probe running elsewhere).
    pub fn with_probes(
        target_fps: f64,
        poll_ms: i64,
        fullscreen_probe: impl FnMut(Option<isize>) -> bool + 'static,
        battery_probe: impl FnMut() -> bool + 'static,
    ) -> Self {
        let target = target_fps.max(1.0);
        Self {
            target_fps: target,
            poll_ms: poll_ms.max(0),
            fullscreen_probe: Box::new(fullscreen_probe),
            battery_probe: Box::new(battery_probe),
            last_poll_ms: None,
            fullscreen: false,
            battery: false,
            current_fps: target,
            reason: "target",
        }
    }

    /// The user chose a rate; re-derive from it immediately.
    pub fn set_target_fps(&mut self, fps: f64) -> f64 {
        self.target_fps = fps.max(1.0);
        self.decide()
    }

    /// Re-probe if it is time to, returning a new FPS only when it changed.
    pub fn poll(&mut self, now_ms: f64, exclude_hwnd: Option<isize>) -> Option<f64> {
        if let Some(last) = self.last_poll_ms
            && now_ms - last < self.poll_ms as f64
        {
            return None;
        }
        self.last_poll_ms = Some(now_ms);
        self.fullscreen = (self.fullscreen_probe)(exclude_hwnd);
        self.battery = (self.battery_probe)();
        let previous = self.current_fps;
        let decided = self.decide();
        if (decided - previous).abs() > 0.01 { Some(decided) } else { None }
    }

    fn decide(&mut self) -> f64 {
        self.current_fps = decide_fps(self.target_fps, self.fullscreen, self.battery);
        self.reason = if self.fullscreen {
            "fullscreen window in front"
        } else if self.battery {
            "on battery"
        } else {
            "target"
        };
        self.current_fps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn frame_rate_policy() {
        assert_eq!(decide_fps(60.0, false, false), 60.0);
        assert_eq!(decide_fps(60.0, false, true), BATTERY_FPS);
        assert_eq!(decide_fps(60.0, true, false), FULLSCREEN_FPS);
        assert_eq!(decide_fps(60.0, true, true), FULLSCREEN_FPS);
        assert_eq!(decide_fps(20.0, false, true), 20.0);
        assert_eq!(decide_fps(20.0, false, false), 20.0);
        assert_eq!(decide_fps(0.0, false, false), 1.0);
    }

    #[test]
    fn policy_restores_the_rate() {
        let fullscreen = Rc::new(Cell::new(false));
        let battery = Rc::new(Cell::new(false));
        let (f, b) = (fullscreen.clone(), battery.clone());
        let mut policy = FramePolicy::with_probes(60.0, 100, move |_| f.get(), move || b.get());
        assert_eq!(policy.poll(0.0, None), None, "an unchanged machine should not restart the timer");
        assert_eq!(policy.current_fps, 60.0);

        fullscreen.set(true);
        assert_eq!(policy.poll(50.0, None), None, "the policy polled before its interval elapsed");
        assert_eq!(policy.poll(200.0, None), Some(FULLSCREEN_FPS));
        assert!(policy.reason.contains("fullscreen"));
        assert_eq!(policy.poll(400.0, None), None);

        fullscreen.set(false);
        assert_eq!(policy.poll(600.0, None), Some(60.0), "the frame rate was not restored");
        assert_eq!(policy.reason, "target");

        battery.set(true);
        assert_eq!(policy.poll(800.0, None), Some(BATTERY_FPS));
        assert_eq!(policy.set_target_fps(20.0), 20.0);
    }

    #[test]
    fn covers_monitor_allows_slack() {
        let screen = (0, 0, 1920, 1080);
        assert!(covers_monitor((0, 0, 1920, 1080), screen));
        assert!(covers_monitor((2, 2, 1918, 1078), screen));
        assert!(!covers_monitor((3, 0, 1920, 1080), screen));
        assert!(covers_monitor((-8, -8, 1928, 1088), screen));
        assert!(!covers_monitor((0, 0, 1920, 1040), screen));
    }

    /// The owner's layout: a game fullscreen on the 2560x1440 primary leaves
    /// the 4K screen beside it in view, so the spiders keep their rate.
    #[test]
    fn a_game_on_one_of_two_monitors_is_not_fullscreen() {
        let primary = (0, 0, 2560, 1440);
        let second = (2560, -718, 6400, 1442);
        assert!(covers_every_monitor(primary, &[primary]));
        assert!(!covers_every_monitor(primary, &[primary, second]));
        assert!(!covers_every_monitor(second, &[primary, second]));
        assert!(covers_every_monitor((0, -718, 6400, 1442), &[primary, second]));
        assert!(!covers_every_monitor(primary, &[]));
    }

    #[test]
    fn probes_are_safe_when_they_cannot_tell() {
        let _ = foreground_window_is_fullscreen(None);
        let _ = on_battery();
        assert!(DESKTOP_CLASSES.contains(&"progman"));
        assert!(DESKTOP_CLASSES.contains(&"workerw"));
    }
}
