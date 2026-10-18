//! Opens the settings window on its own, with the real models,
//! temperaments and presets dc-content discovers (or the sample catalogue
//! when none are found).
//!
//! ```text
//! cargo run -p dc-ui --example settings_window -- [--seconds N] [--page companion|skirmish|strategy] [--root DIR]
//! ```
//!
//! Actions are logged, and Save / Load / Refresh are carried out against the
//! preset files so the window can be tried end to end; launching an overlay
//! is only logged.

use std::path::PathBuf;
use std::time::Duration;

use dc_ui::windows::config::ConfigWindow;
use dc_ui::windows::mode_menu::Page;
use dc_ui::{RunOptions, UiAction, WindowApp, bridge, sample};

struct SettingsApp {
    window: ConfigWindow,
    root: Option<PathBuf>,
}

impl WindowApp for SettingsApp {
    fn ui(&mut self, ctx: &egui::Context) -> Vec<UiAction> {
        self.window.ui(ctx)
    }

    fn handle(&mut self, actions: Vec<UiAction>) -> bool {
        for action in actions {
            match action {
                UiAction::CloseWindow => return false,
                UiAction::LoadPreset { path } => {
                    match dc_content::preset_io::load_preset_value(std::path::Path::new(&path)) {
                        Ok(data) => {
                            self.window.load_preset(&data);
                            self.window.set_status(format!("Loaded preset: {path}"));
                        }
                        Err(e) => self.window.set_status(format!("Could not load preset: {e}")),
                    }
                }
                UiAction::RefreshLibrary => {
                    let catalog = bridge::settings_catalog(self.root.as_deref());
                    self.window.set_catalog(catalog);
                    self.window.set_presets(bridge::presets(self.root.as_deref()));
                }
                UiAction::LaunchOverlay { mode, preset } => {
                    log::info!(
                        "launch {} overlay with {} slot(s)",
                        mode.as_str(),
                        preset["slots"].as_array().map_or(0, Vec::len)
                    );
                    self.window.set_status("Overlay launch requested (the example does not start one).");
                }
                other => log::info!("UI action: {other:?}"),
            }
        }
        true
    }
}

fn main() {
    let mut seconds = None;
    let mut page = None;
    let mut root: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seconds" => seconds = args.next().and_then(|v| v.parse::<f64>().ok()),
            "--page" => page = args.next(),
            "--root" => root = args.next().map(PathBuf::from),
            _ => eprintln!("unknown argument {arg}"),
        }
    }
    if root.is_none() {
        // The checkout this example was built from holds models/, presets/ ...
        let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
        if checkout.join("models").is_dir() {
            root = Some(checkout);
        }
    }

    let mut catalog = bridge::settings_catalog(root.as_deref());
    if catalog.models.is_empty() || catalog.personalities.is_empty() {
        eprintln!("no content found; using the sample catalogue");
        catalog = sample::settings_catalog();
    }
    let presets = bridge::presets(root.as_deref());
    let mut window = ConfigWindow::new(catalog);
    let default = presets.iter().find(|p| p.label == "Default").cloned();
    window.set_presets(presets);
    match default.and_then(|p| dc_content::preset_io::load_preset_value(std::path::Path::new(&p.path)).ok()) {
        Some(data) => window.load_preset(&data),
        None => window.load_preset(&sample::default_preset()),
    }
    window.shell.refresh_adventure(sample::adventure_view());
    if let Some(page) = page {
        window.shell.page = Page::from_kind(&page);
    }

    let mut options = RunOptions::new(ConfigWindow::title(), [960.0, 860.0]);
    options.min_size = Some([840.0, 560.0]);
    options.exit_after = seconds.map(Duration::from_secs_f64);
    let started = std::time::Instant::now();
    match dc_ui::run_window_with(options, SettingsApp { window, root }) {
        Ok(()) => println!("settings window closed after {:.1}s", started.elapsed().as_secs_f64()),
        Err(e) => {
            eprintln!("could not open the settings window: {e}");
            std::process::exit(1);
        }
    }
}
