//! Desktop Bug Companion: the overlay process (`app/engine.py` `main`).
//!
//! ```text
//! dc-app [--preset presets/default.json] [--seed N] [--verbose]
//!        [--software] [--seconds S] [--snapshot out.png]
//! ```
//!
//! `--software` paints through `UpdateLayeredWindow` instead of the GPU;
//! `--seconds` quits after that long (development runs) and then never moves
//! the real mouse pointer; `--snapshot` saves a screen capture of the primary
//! monitor 3 s in (with the overlay on it).

mod engine;
mod metrics;

use std::path::PathBuf;
use std::process::ExitCode;

pub struct Args {
    pub preset: PathBuf,
    pub seed: Option<i64>,
    pub verbose: bool,
    pub software: bool,
    pub seconds: Option<f64>,
    pub snapshot: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        preset: PathBuf::from("presets/default.json"),
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
            "--preset" => args.preset = PathBuf::from(value("--preset")?),
            "--seed" => args.seed = Some(value("--seed")?.parse().map_err(|e| format!("--seed: {e}"))?),
            "--seconds" => args.seconds = Some(value("--seconds")?.parse().map_err(|e| format!("--seconds: {e}"))?),
            "--snapshot" => args.snapshot = Some(PathBuf::from(value("--snapshot")?)),
            "--verbose" => args.verbose = true,
            "--software" => args.software = true,
            "--mode" => {
                let mode = value("--mode")?;
                if mode != "companion" {
                    return Err(format!("--mode {mode} is not ported yet; only companion runs"));
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
    log::info!("Desktop Bug Companion (Rust) overlay starting");
    if written_to.is_none() {
        log::warn!("No log file could be opened under {}", state_dir.display());
    }
    let preset = dc_content::discovery::resolve_preset_path(&args.preset);
    match engine::run(args, preset, state_dir) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log::error!("{e}");
            eprintln!("dc-app: {e}");
            ExitCode::FAILURE
        }
    }
}
