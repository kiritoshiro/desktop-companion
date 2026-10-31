//! Desktop Bug Companion. Without `--engine` it opens the settings window
//! (`app/config_ui.py` `main`); with it, it is the overlay process
//! (`app/engine.py` `main`), which the settings window starts.
//!
//! ```text
//! dc-app [--seconds S]
//! dc-app --engine [--preset presets/default.json] [--mode companion|adventure]
//!        [--seed N] [--verbose] [--software] [--seconds S] [--snapshot out.png]
//! ```
//!
//! `--software` paints through `UpdateLayeredWindow` instead of the GPU;
//! `--seconds` quits after that long (development runs), and an overlay run
//! that way never moves the real mouse pointer; `--snapshot` saves a screen
//! capture of the primary monitor 3 s in (with the overlay on it).

// No console window beside the app in a release build, as the PyInstaller
// build's `console=False`. Logs still go to the state folder, and to a
// redirected stderr.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adventure;
mod camouflage;
mod dpi;
mod engine;
mod live;
mod metrics;
mod overlay_ui;
mod profile_hud;
mod repaint;
mod settings;
#[cfg(test)]
mod test_support;

use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Companion,
    Adventure,
}

pub struct Args {
    pub engine: bool,
    pub preset: PathBuf,
    pub mode: Mode,
    pub seed: Option<i64>,
    pub verbose: bool,
    pub software: bool,
    pub seconds: Option<f64>,
    pub snapshot: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        engine: false,
        preset: PathBuf::from("presets/default.json"),
        mode: Mode::Companion,
        seed: None,
        verbose: false,
        software: false,
        seconds: None,
        snapshot: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        match a.as_str() {
            "--engine" => args.engine = true,
            "--preset" => args.preset = PathBuf::from(value("--preset")?),
            "--seed" => args.seed = Some(value("--seed")?.parse().map_err(|e| format!("--seed: {e}"))?),
            "--seconds" => args.seconds = Some(value("--seconds")?.parse().map_err(|e| format!("--seconds: {e}"))?),
            "--snapshot" => args.snapshot = Some(PathBuf::from(value("--snapshot")?)),
            "--verbose" => args.verbose = true,
            "--software" => args.software = true,
            "--mode" => {
                args.mode = match value("--mode")?.as_str() {
                    "companion" => Mode::Companion,
                    "adventure" => Mode::Adventure,
                    other => return Err(format!("--mode {other}: companion or adventure")),
                }
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(args)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("dc-app: {e}");
            return ExitCode::from(2);
        }
    };
    // Before logging, so a first run logs to the new place (DC-15).
    dc_content::discovery::migrate_legacy_state_dir();
    let state_dir = dc_content::discovery::state_dir();
    let level = if args.verbose { log::LevelFilter::Debug } else { log::LevelFilter::Info };
    let written_to = dc_platform::logging_setup::configure_logging(&state_dir, level);
    let what = if args.engine { "overlay" } else { "settings window" };
    log::info!("Desktop Bug Companion (Rust) {what} starting");
    if written_to.is_none() {
        log::warn!("No log file could be opened under {}", state_dir.display());
    }
    let result = if args.engine {
        let preset = dc_content::discovery::resolve_preset_path(&args.preset);
        engine::run(args, preset, state_dir)
    } else {
        settings::run(state_dir, args.seconds)
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log::error!("{e}");
            eprintln!("dc-app: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    /// `build.rs` links `app_icon.ico` into every binary of this crate, this
    /// test harness included, as icon group 1.
    #[test]
    fn exe_carries_the_app_icon() {
        use winit::platform::windows::IconExtWindows;
        assert!(winit::window::Icon::from_resource(1, None).is_ok());
    }
}
