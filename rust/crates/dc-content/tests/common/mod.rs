//! Shared test helpers: the checkout root, a private state dir, the oracle.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::OnceLock;

use serde_json::Value;

/// The checkout holding `models/`, `personalities/`, `presets/`.
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .map(|p| {
            let s = p.to_string_lossy().into_owned();
            PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s))
        })
        .unwrap()
}

/// Pin the app root and redirect state to a per-process temp dir, so a test
/// run never reads or writes a real player's state (the Python suite does
/// the same with `DESKTOP_BUG_STATE_DIR`).
pub fn isolate() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("dc-content-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dc_content::discovery::set_app_root(Some(root()));
        dc_content::discovery::set_state_dir_override(Some(dir.clone()));
        dir
    })
    .clone()
}

/// Held by tests that write user presets into the shared state dir, or
/// resolve preset paths that such a copy would shadow; tests in one binary
/// run in parallel and the state dir is process-wide.
pub fn preset_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let text = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/content_oracle.json"),
        )
        .unwrap();
        serde_json::from_str(&text).unwrap()
    })
}

/// Assert JSON equality with `1 == 1.0`, reporting the first difference.
pub fn assert_json_eq(label: &str, actual: &Value, expected: &Value) {
    if let Some(diff) = dc_content::util::json_first_difference(actual, expected) {
        panic!("{label}: {diff}");
    }
}
