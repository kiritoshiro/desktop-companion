//! What the dc-app tests share across modules.
//!
//! dc-content's state-dir override (`set_state_dir_override`) is one value
//! for the whole test process. Every test that sets it, or that runs code
//! reading `dc_content::discovery::state_dir()` while another might set it,
//! holds [`state_lock`] for its whole run. Tests that pass their state
//! folder explicitly (the Adventure and overlay UI tests) need no lock.

use std::sync::{Mutex, MutexGuard};

static STATE_LOCK: Mutex<()> = Mutex::new(());

/// The one lock round the process-wide state directory. A test that
/// panicked while holding it does not poison the others.
pub fn state_lock() -> MutexGuard<'static, ()> {
    STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}
