//! Port of support/logging_setup.py: application logging and the
//! last-resort crash hook, as a [`log`] backend.
//!
//! The packaged build has no console, so everything goes to a rotating file
//! beside the runtime state (`<state>/logs/desktop-bug.log`, 512 KiB, three
//! backups, the same rotation rule as Python's `RotatingFileHandler`) and,
//! when a console exists, to stderr as well.
//!
//! Logger names: Python's loggers were `desktop_bug` and its children
//! (`desktop_bug.engine`). A Rust record's target maps onto that tree:
//! `dc_platform::live_channel` and an explicit `target: "live_channel"` both
//! become `desktop_bug.live_channel`. Records from other crates (wgpu, winit)
//! keep their own module path and are filtered to warnings and above, since
//! the Python logger did not propagate foreign records either.
//!
//! `install_excepthook` becomes [`install_panic_hook`]: a panic is logged
//! (with a backtrace the first time) and repeats are counted. Unlike Python
//! the panicking thread still unwinds; the hook only makes sure it is
//! recorded.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use log::{Level, LevelFilter, Log, Metadata, Record};
use windows::Win32::System::SystemInformation::GetLocalTime;

pub const LOGGER_NAME: &str = "desktop_bug";
pub const LOG_FILE_NAME: &str = "desktop-bug.log";
/// Small enough to attach to a bug report, with a little history behind it.
pub const MAX_BYTES: u64 = 512 * 1024;
pub const BACKUP_COUNT: u32 = 3;

/// The log file path for a state directory.
pub fn log_path(state_dir: impl AsRef<Path>) -> PathBuf {
    state_dir.as_ref().join("logs").join(LOG_FILE_NAME)
}

/// Whether a record target belongs to this application rather than a
/// dependency.
pub fn is_app_target(target: &str) -> bool {
    if target.starts_with("dc_") || target.starts_with(LOGGER_NAME) {
        return true;
    }
    if target.contains("::") {
        return false;
    }
    // A crate-root record from a dependency carries its bare crate name.
    const FOREIGN: [&str; 8] = ["wgpu", "naga", "winit", "tray_icon", "muda", "egui", "calloop", "tiny_skia"];
    !FOREIGN.iter().any(|f| target.starts_with(f))
}

/// The Python logger name a record target stands for.
pub fn logger_name(target: &str) -> String {
    if !is_app_target(target) {
        return target.replace("::", ".");
    }
    if let Some(rest) = target.strip_prefix(LOGGER_NAME) {
        return format!("{LOGGER_NAME}{}", rest.replace("::", "."));
    }
    if target.starts_with("dc_") {
        let mut parts = target.split("::");
        parts.next();
        let rest: Vec<&str> = parts.collect();
        return if rest.is_empty() { LOGGER_NAME.to_string() } else { format!("{LOGGER_NAME}.{}", rest.join(".")) };
    }
    format!("{LOGGER_NAME}.{target}")
}

/// Python's `levelname`.
pub fn level_name(level: Level) -> &'static str {
    match level {
        Level::Error => "ERROR",
        Level::Warn => "WARNING",
        Level::Info => "INFO",
        Level::Debug => "DEBUG",
        Level::Trace => "TRACE",
    }
}

/// `%Y-%m-%d %H:%M:%S` in local time.
pub fn local_timestamp() -> String {
    let t = unsafe { GetLocalTime() };
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

/// `"%(asctime)s %(levelname)-7s %(name)s: %(message)s"`.
pub fn format_line(timestamp: &str, level: Level, name: &str, message: &str) -> String {
    format!("{timestamp} {:<7} {name}: {message}", level_name(level))
}

/// A file that rolls over like `logging.handlers.RotatingFileHandler`.
pub struct RotatingFile {
    path: PathBuf,
    max_bytes: u64,
    backup_count: u32,
    file: Option<File>,
    pos: u64,
}

impl RotatingFile {
    pub fn open(path: impl Into<PathBuf>, max_bytes: u64, backup_count: u32) -> std::io::Result<Self> {
        let path = path.into();
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let pos = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self { path, max_bytes, backup_count, file: Some(file), pos })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn backup(&self, i: u32) -> PathBuf {
        let mut s = self.path.clone().into_os_string();
        s.push(format!(".{i}"));
        PathBuf::from(s)
    }

    /// Python 3.13's `shouldRollover`: never roll an empty file, otherwise
    /// roll when the current size plus the new line reaches `max_bytes`.
    fn should_rollover(&self, msg_len: u64) -> bool {
        self.max_bytes > 0 && self.pos != 0 && self.pos + msg_len >= self.max_bytes
    }

    fn do_rollover(&mut self) -> std::io::Result<()> {
        self.file = None; // Windows cannot rename an open file.
        if self.backup_count > 0 {
            for i in (1..self.backup_count).rev() {
                let sfn = self.backup(i);
                let dfn = self.backup(i + 1);
                if sfn.exists() {
                    if dfn.exists() {
                        let _ = std::fs::remove_file(&dfn);
                    }
                    let _ = std::fs::rename(&sfn, &dfn);
                }
            }
            let dfn = self.backup(1);
            if dfn.exists() {
                let _ = std::fs::remove_file(&dfn);
            }
            if self.path.exists() {
                let _ = std::fs::rename(&self.path, &dfn);
            }
        }
        let file = OpenOptions::new().create(true).append(true).open(&self.path)?;
        self.pos = file.metadata().map(|m| m.len()).unwrap_or(0);
        self.file = Some(file);
        Ok(())
    }

    /// Write one line (a newline is appended), rolling over first if needed.
    pub fn write_line(&mut self, line: &str) -> std::io::Result<()> {
        let msg = format!("{line}\n");
        // `len(msg)` in Python counts characters, not bytes.
        if self.should_rollover(msg.chars().count() as u64) {
            self.do_rollover()?;
        }
        if self.file.is_none() {
            self.do_rollover()?;
        }
        let file = self.file.as_mut().expect("log file open");
        file.write_all(msg.as_bytes())?;
        self.pos += msg.len() as u64;
        Ok(())
    }

    pub fn flush(&mut self) {
        if let Some(f) = self.file.as_mut() {
            let _ = f.flush();
        }
    }
}

struct LoggerState {
    configured: bool,
    level: LevelFilter,
    file: Option<RotatingFile>,
    log_path: Option<PathBuf>,
    stderr: bool,
}

struct AppLogger {
    state: Mutex<LoggerState>,
}

impl AppLogger {
    fn effective_level(state: &LoggerState, target: &str) -> LevelFilter {
        if is_app_target(target) { state.level } else { state.level.min(LevelFilter::Warn) }
    }
}

impl Log for AppLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        match self.state.lock() {
            Ok(state) => state.configured && metadata.level() <= Self::effective_level(&state, metadata.target()),
            Err(_) => false,
        }
    }

    fn log(&self, record: &Record) {
        let Ok(mut state) = self.state.lock() else { return };
        if !state.configured || record.level() > Self::effective_level(&state, record.target()) {
            return;
        }
        let line =
            format_line(&local_timestamp(), record.level(), &logger_name(record.target()), &record.args().to_string());
        if let Some(file) = state.file.as_mut() {
            let _ = file.write_line(&line);
            file.flush();
        }
        if state.stderr {
            let _ = writeln!(std::io::stderr(), "{line}");
        }
    }

    fn flush(&self) {
        if let Ok(mut state) = self.state.lock()
            && let Some(f) = state.file.as_mut()
        {
            f.flush();
        }
    }
}

fn logger() -> &'static AppLogger {
    static LOGGER: OnceLock<AppLogger> = OnceLock::new();
    LOGGER.get_or_init(|| AppLogger {
        state: Mutex::new(LoggerState {
            configured: false,
            level: LevelFilter::Info,
            file: None,
            log_path: None,
            stderr: true,
        }),
    })
}

/// Attach the rotating file once and return the path it writes to.
///
/// `None` when the directory cannot be written (a read-only install): losing
/// the log is not a reason to stop the overlay, so the failure is reported
/// by the return value only. A second call returns the first call's answer.
pub fn configure_logging(state_dir: impl AsRef<Path>, level: LevelFilter) -> Option<PathBuf> {
    let app = logger();
    {
        let state = app.state.lock().ok()?;
        if state.configured {
            return state.log_path.clone();
        }
    }
    // `set_logger` succeeds once per process; a later failure means another
    // backend (env_logger in a test, say) already owns the facade.
    let _ = log::set_logger(app);
    log::set_max_level(level);

    let path = log_path(&state_dir);
    let file = path
        .parent()
        .and_then(|dir| std::fs::create_dir_all(dir).ok())
        .and_then(|_| RotatingFile::open(&path, MAX_BYTES, BACKUP_COUNT).ok());
    let mut state = app.state.lock().ok()?;
    state.log_path = file.as_ref().map(|f| f.path().to_path_buf());
    state.file = file;
    state.level = level;
    state.stderr = has_console();
    state.configured = true;
    state.log_path.clone()
}

fn has_console() -> bool {
    use windows::Win32::System::Console::GetConsoleWindow;
    !unsafe { GetConsoleWindow() }.0.is_null()
}

/// Whether [`configure_logging`] has run (and not been reset).
pub fn is_configured() -> bool {
    logger().state.lock().map(|s| s.configured).unwrap_or(false)
}

/// Drop the configured file so a test can configure a fresh directory.
pub fn reset_for_tests() {
    if let Ok(mut state) = logger().state.lock() {
        state.file = None;
        state.log_path = None;
        state.configured = false;
    }
}

/// Flush the log file.
pub fn flush() {
    logger().flush();
}

type Notifier = Arc<dyn Fn(&str) + Send + Sync>;

/// The crash notifier [`install_panic_hook`] takes.
pub type PanicNotifier = Box<dyn Fn(&str) + Send + Sync>;

/// Counts of each distinct panic seen by the hook, shared for inspection.
pub type PanicCounts = Arc<Mutex<HashMap<String, u64>>>;

/// Whether the `count`-th repeat of a panic gets its own log line.
pub fn should_log_repeat(count: u64) -> bool {
    count == 1 || matches!(count, 2 | 10 | 100) || count.is_multiple_of(1000)
}

/// Log panics instead of letting them disappear (`install_excepthook`).
///
/// Repeats are counted rather than written out again: a panic in the frame
/// loop would otherwise fill the log 60 times a second. `notify` is called
/// once per distinct panic (the tray balloon in the engine).
pub fn install_panic_hook(notify: Option<PanicNotifier>) -> PanicCounts {
    let seen: PanicCounts = Arc::new(Mutex::new(HashMap::new()));
    let counts = seen.clone();
    let notify: Option<Notifier> = notify.map(Arc::from);
    std::panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".to_string());
        let location = info.location().map(|l| format!(" at {}:{}", l.file(), l.line())).unwrap_or_default();
        let signature = format!("{payload}{location}");
        let count = {
            let mut map = counts.lock().unwrap_or_else(|e| e.into_inner());
            let c = map.entry(signature.clone()).or_insert(0);
            *c += 1;
            *c
        };
        if count == 1 {
            let bt = std::backtrace::Backtrace::force_capture();
            log::error!(target: "crash", "Unhandled panic: {signature}\n{bt}");
        } else if should_log_repeat(count) {
            log::error!(target: "crash", "Unhandled panic: {signature} (seen {count} times)");
        }
        if count == 1
            && let Some(n) = notify.as_ref()
        {
            // A failing notifier must not mask the crash.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| n(&format!("panic: {payload}"))));
        }
    }));
    seen
}

/// Undo [`install_panic_hook`] (back to the default hook).
pub fn restore_panic_hook() {
    let _ = std::panic::take_hook();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "dc-platform-{tag}-{}-{}",
            std::process::id(),
            crate::win::tick_ms() as u64
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn names_map_onto_the_python_tree() {
        assert_eq!(logger_name("dc_platform::live_channel"), "desktop_bug.live_channel");
        assert_eq!(logger_name("engine"), "desktop_bug.engine");
        assert_eq!(logger_name("dc_app"), "desktop_bug");
        assert_eq!(logger_name("desktop_bug::crash"), "desktop_bug.crash");
        assert_eq!(logger_name("wgpu_core::device"), "wgpu_core.device");
        assert!(!is_app_target("wgpu_hal"));
        assert!(is_app_target("overlay"));
    }

    #[test]
    fn line_format_matches_python() {
        let line = format_line("2026-10-18 12:00:00", Level::Info, "desktop_bug.test", "hello 0.1.0");
        assert_eq!(line, "2026-10-18 12:00:00 INFO    desktop_bug.test: hello 0.1.0");
        let line = format_line("2026-10-18 12:00:00", Level::Warn, "desktop_bug", "x");
        assert_eq!(line, "2026-10-18 12:00:00 WARNING desktop_bug: x");
        assert_eq!(local_timestamp().len(), 19);
    }

    #[test]
    fn rotation_keeps_three_backups() {
        let dir = temp_dir("rotate");
        let path = dir.join("t.log");
        let mut f = RotatingFile::open(&path, 100, BACKUP_COUNT).unwrap();
        for i in 0..40 {
            f.write_line(&format!("line {i:03} ............................")).unwrap();
        }
        drop(f);
        assert!(path.exists());
        for i in 1..=3 {
            assert!(dir.join(format!("t.log.{i}")).exists(), "backup {i} missing");
        }
        assert!(!dir.join("t.log.4").exists(), "more backups than BACKUP_COUNT");
        assert!(std::fs::metadata(&path).unwrap().len() < 100);
        let last = std::fs::read_to_string(&path).unwrap();
        assert!(last.contains("line 039"), "{last}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_file_never_rolls_over() {
        let dir = temp_dir("empty");
        let path = dir.join("t.log");
        let mut f = RotatingFile::open(&path, 10, 3).unwrap();
        f.write_line("a line far longer than ten bytes").unwrap();
        assert!(!dir.join("t.log.1").exists());
        f.write_line("second").unwrap();
        assert!(dir.join("t.log.1").exists());
        drop(f);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn repeat_schedule() {
        let logged: Vec<u64> = (1..=3000).filter(|c| should_log_repeat(*c)).collect();
        assert_eq!(logged, vec![1, 2, 10, 100, 1000, 2000, 3000]);
    }

    // The global logger is process-wide, so configuration, the
    // unwritable-directory case and the panic hook share one test.
    #[test]
    fn configures_once_and_writes_python_lines() {
        reset_for_tests();
        let dir = temp_dir("configure");
        let path = configure_logging(&dir, LevelFilter::Debug).expect("log file");
        assert_eq!(path, log_path(&dir));
        assert!(path.parent().unwrap().is_dir());
        assert_eq!(configure_logging(&dir, LevelFilter::Debug), Some(path.clone()), "configuring twice");

        log::info!(target: "test", "hello {}", dc_core::VERSION);
        log::debug!(target: "wgpu_core::device", "foreign debug noise");
        flush();
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains(&format!("hello {}", dc_core::VERSION)), "{written}");
        assert!(written.contains("desktop_bug.test"), "{written}");
        assert!(!written.contains("foreign debug noise"), "{written}");

        // A panic is logged once with its message and counted after that.
        let counts = install_panic_hook(None);
        for _ in 0..2 {
            let _ = std::panic::catch_unwind(|| panic!("test boom"));
        }
        restore_panic_hook();
        flush();
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("Unhandled panic: test boom"), "{written}");
        assert!(written.contains("(seen 2 times)"), "{written}");
        assert_eq!(counts.lock().unwrap().values().copied().max(), Some(2));

        // An unwritable directory reports None rather than failing.
        reset_for_tests();
        let blocker = dir.join("not-a-dir");
        std::fs::write(&blocker, b"x").unwrap();
        assert_eq!(configure_logging(&blocker, LevelFilter::Info), None);
        reset_for_tests();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
