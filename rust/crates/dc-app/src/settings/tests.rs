//! `SettingsHost`, headless: the dc-ui windows are driven through egui
//! `RawInput` (clicks on recorded widgets, typed text, keys), and each test
//! checks what the host then did -- the fake launcher's calls, the files in a
//! throw-away state directory, and the messages a real `OverlayChannelServer`
//! received. Ports the intent of test_settings_table, test_live_channel's
//! settings side, test_reset_progress (the window), test_mission_list_and_close
//! (closing stops the overlay), test_adventure_polish (leaving Adventure) and
//! the mode menu's profile writes.
//!
//! No window opens, the real overlay never starts (`FakeLauncher`), and the
//! pointer is never moved: every pointer event is synthetic egui input.
//!
//! The state directory is dc-content's process-wide override, so every test
//! holds `crate::test_support::state_lock()` (shared with every dc-app test
//! that touches it) for its whole run.

use super::*;

use std::sync::MutexGuard;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use dc_platform::live_channel::{OverlayChannelServer, ServerEvent};
use dc_platform::session_control::{StoppableProcess, request_stop, stop_process, stop_request_path};
use dc_ui::windows::mode_menu::Page;
use egui::accesskit::{Role, Toggled};
use egui::{
    Context, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, ViewportCommand, ViewportId, pos2, vec2,
};
use serde_json::{Map, json};

/// What Python's window wrote (`fixtures/gen_preset_edits.py`).
const PYTHON: &str = include_str!("fixtures/preset_edits.json");

fn python() -> Value {
    serde_json::from_str(PYTHON).expect("fixture")
}

// -- the state directory ------------------------------------------------------

/// A fresh state directory, pinned as dc-content's `state_dir()` until drop.
struct TempState {
    dir: PathBuf,
    _lock: MutexGuard<'static, ()>,
}

impl TempState {
    fn new(tag: &str) -> Self {
        let lock = crate::test_support::state_lock();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("dc-settings-{tag}-{}-{nanos}", std::process::id())).join("state");
        std::fs::create_dir_all(&dir).unwrap();
        dc_content::discovery::set_state_dir_override(Some(dir.clone()));
        Self { dir, _lock: lock }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.path(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    fn json(&self, name: &str) -> Value {
        serde_json::from_str(&self.read(name)).unwrap()
    }

    fn profile(&self) -> Value {
        self.json("adventure-hero.json")
    }

    /// A real overlay's end of the live channel for this state directory.
    fn server(&self) -> OverlayChannelServer {
        let mut server = OverlayChannelServer::new(channel_name(&self.dir));
        assert!(server.listen(), "the test channel could not bind");
        server
    }
}

impl Drop for TempState {
    fn drop(&mut self) {
        dc_content::discovery::set_state_dir_override(None);
        if let Some(parent) = self.dir.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }
}

// -- a fake overlay process ---------------------------------------------------

#[derive(Clone, Debug)]
struct Launch {
    args: Vec<OsString>,
    preset: Value,
    /// A stop request was still on disk as the overlay started.
    stop_request_left: bool,
}

#[derive(Clone, Copy, Debug)]
struct Stop {
    outcome: StopOutcome,
    /// The stop-request file was there while the window waited.
    request_seen: bool,
    terminated: bool,
}

/// The `Popen` that `stop_process` polls.
struct FakeProcess {
    polls: u32,
    alive_for: Option<u32>,
    terminated: bool,
}

impl StoppableProcess for FakeProcess {
    fn is_running(&mut self) -> bool {
        self.polls += 1;
        self.alive_for.is_none_or(|n| self.polls <= n)
    }

    fn terminate(&mut self) {
        self.terminated = true;
    }
}

/// Records every launch and stop; never starts a process.
struct FakeLauncher {
    launches: Vec<Launch>,
    stops: Vec<Stop>,
    running: bool,
    fail_with: Option<String>,
    /// Polls the overlay stays up after a stop request; `None`: wedged.
    answers_stop_after: Option<u32>,
    exit_code: Option<i32>,
}

impl FakeLauncher {
    fn new() -> Self {
        Self {
            launches: Vec::new(),
            stops: Vec::new(),
            running: false,
            fail_with: None,
            answers_stop_after: Some(2),
            exit_code: None,
        }
    }
}

impl Launcher for FakeLauncher {
    fn launch(&mut self, preset: &Path, mode: OverlayMode) -> Result<(), String> {
        let text = std::fs::read_to_string(preset).map_err(|e| e.to_string())?;
        let state_dir = dc_content::discovery::state_dir();
        self.launches.push(Launch {
            args: engine_args(preset, mode),
            preset: serde_json::from_str(&text).map_err(|e| e.to_string())?,
            stop_request_left: stop_request_path(&state_dir).exists(),
        });
        if let Some(e) = &self.fail_with {
            return Err(e.clone());
        }
        self.running = true;
        Ok(())
    }

    fn running(&mut self) -> bool {
        self.running
    }

    /// `ProcessLauncher::stop`'s `stop_process`, on a fake process and
    /// without real sleeps.
    fn stop(&mut self, state_dir: &Path) -> StopOutcome {
        let mut process = FakeProcess { polls: 0, alive_for: self.answers_stop_after, terminated: false };
        let mut request_seen = false;
        let running = self.running;
        let outcome = stop_process(running.then_some(&mut process), state_dir, 0.5, 0.1, |_| {
            request_seen |= stop_request_path(state_dir).is_file();
        });
        self.stops.push(Stop { outcome, request_seen, terminated: process.terminated });
        self.running = false;
        outcome
    }

    fn take_exit(&mut self) -> Option<i32> {
        let code = self.exit_code.take()?;
        self.running = false;
        Some(code)
    }
}

type Host = SettingsHost<FakeLauncher>;

fn two_screens() -> Vec<ScreenInfo> {
    vec![
        ScreenInfo {
            name: "\\\\.\\DISPLAY1".into(),
            number: 1,
            width: 1920,
            height: 1080,
            primary: true,
            state: "active".into(),
        },
        ScreenInfo {
            name: "\\\\.\\DISPLAY2".into(),
            number: 2,
            width: 2560,
            height: 1440,
            primary: false,
            state: "active".into(),
        },
    ]
}

fn host(state: &TempState) -> Host {
    let mut host = SettingsHost::new(Some(dc_content::discovery::app_root()), state.dir.clone(), FakeLauncher::new());
    host.set_screens(two_screens);
    host
}

// -- the headless screen ------------------------------------------------------

#[derive(Clone, Debug)]
struct Node {
    role: Role,
    label: String,
    rect: Rect,
    toggled: Option<bool>,
}

/// One egui context fed synthetic input, frame by frame, exactly as dc-ui's
/// runner does: draw, then hand the frame's actions to the host.
struct Screen {
    ctx: Context,
    time: f64,
    pending: Vec<Event>,
    size: egui::Vec2,
    nodes: Vec<Node>,
    /// Viewport commands since the last `take_commands`.
    commands: Vec<ViewportCommand>,
}

impl Screen {
    fn new() -> Self {
        let ctx = Context::default();
        dc_ui::theme::apply_app_theme(&ctx);
        dc_ui::probe::enable(&ctx);
        ctx.enable_accesskit();
        Self {
            ctx,
            time: 0.0,
            pending: Vec::new(),
            size: vec2(1200.0, 2000.0),
            nodes: Vec::new(),
            commands: Vec::new(),
        }
    }

    fn frame(&mut self, host: &mut Host) -> Vec<UiAction> {
        self.time += 1.0 / 30.0;
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            time: Some(self.time),
            events: std::mem::take(&mut self.pending),
            ..Default::default()
        };
        let mut actions = Vec::new();
        let mut output = self.ctx.run_pass(input, |ctx| actions = WindowApp::ui(host, ctx));
        output.textures_delta.clear();
        if let Some(viewport) = output.viewport_output.get(&ViewportId::ROOT) {
            self.commands.extend(viewport.commands.iter().cloned());
        }
        self.nodes.clear();
        if let Some(update) = &output.platform_output.accesskit_update {
            for (_, node) in &update.nodes {
                let Some(b) = node.bounds() else { continue };
                self.nodes.push(Node {
                    role: node.role(),
                    label: node.label().or(node.value()).unwrap_or("").to_owned(),
                    rect: Rect::from_min_max(pos2(b.x0 as f32, b.y0 as f32), pos2(b.x1 as f32, b.y1 as f32)),
                    toggled: node.toggled().map(|t| t == Toggled::True),
                });
            }
        }
        assert!(WindowApp::handle(host, actions.clone()), "the window closed itself");
        actions
    }

    /// Run frames until layout settles (a dialog window grows into its
    /// content over a few frames).
    fn settle(&mut self, host: &mut Host) -> Vec<UiAction> {
        (0..10).flat_map(|_| self.frame(host)).collect()
    }

    fn probe(&self, key: &str) -> Rect {
        dc_ui::probe::rect(&self.ctx, key)
            .unwrap_or_else(|| panic!("no widget recorded as {key:?}; recorded: {:?}", dc_ui::probe::keys(&self.ctx)))
    }

    fn click_at(&mut self, pos: Pos2, host: &mut Host) -> Vec<UiAction> {
        let mut actions = Vec::new();
        let button =
            |pressed| Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        for event in [Event::PointerMoved(pos), button(true), button(false), Event::PointerMoved(pos2(1.0, 1.0))] {
            self.pending.push(event);
            actions.extend(self.frame(host));
        }
        actions
    }

    /// Click the centre of a widget a window recorded (`dc_ui::probe`).
    fn click(&mut self, key: &str, host: &mut Host) -> Vec<UiAction> {
        self.settle(host);
        let rect = self.probe(key);
        self.click_at(rect.center(), host)
    }

    fn node(&self, what: &str, pick: impl Fn(&Node) -> bool) -> Node {
        self.nodes.iter().find(|n| pick(n)).cloned().unwrap_or_else(|| {
            let seen: Vec<_> = self.nodes.iter().map(|n| format!("{:?} {:?}", n.role, n.label)).collect();
            panic!("no node for {what}; nodes: {seen:#?}")
        })
    }

    /// Open the combo box at `combo` and pick the entry labelled `label`
    /// (the entry closest to the box, so a label used elsewhere is skipped).
    fn choose(&mut self, combo: Rect, label: &str, host: &mut Host) {
        self.click_at(combo.center(), host);
        self.settle(host);
        let mut options: Vec<Node> =
            self.nodes.iter().filter(|n| n.label == label && n.role != Role::ComboBox).cloned().collect();
        options.sort_by(|a, b| {
            let d = |n: &Node| (n.rect.center() - combo.center()).length();
            d(a).total_cmp(&d(b))
        });
        let option = options.first().cloned().unwrap_or_else(|| {
            let seen: Vec<_> = self
                .nodes
                .iter()
                .filter(|n| n.role != Role::TextRun && n.role != Role::Label)
                .map(|n| format!("{:?} {:?} {:?}", n.role, n.label, n.rect.center()))
                .collect();
            panic!("{label:?} is not offered at {combo:?}: {seen:#?}")
        });
        self.click_at(option.rect.center(), host);
    }

    /// The node of `role` on the same row as `anchor`.
    fn in_row(&self, anchor: Rect, role: Role) -> Node {
        self.node(&format!("{role:?} beside {anchor:?}"), |n| {
            n.role == role && (n.rect.center().y - anchor.center().y).abs() < 14.0
        })
    }

    /// Type into the drag value or text field at `rect`, then Enter.
    fn type_into(&mut self, rect: Rect, text: &str, host: &mut Host) -> Vec<UiAction> {
        let mut actions = self.click_at(rect.center(), host);
        self.pending.push(Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        });
        self.pending.push(Event::Text(text.into()));
        actions.extend(self.frame(host));
        actions.extend(self.key(Key::Enter, host));
        actions.extend(self.settle(host));
        actions
    }

    fn key(&mut self, key: Key, host: &mut Host) -> Vec<UiAction> {
        let event =
            |pressed| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE };
        self.pending.push(event(true));
        let mut actions = self.frame(host);
        self.pending.push(event(false));
        actions.extend(self.frame(host));
        actions
    }

    fn take_commands(&mut self) -> Vec<ViewportCommand> {
        std::mem::take(&mut self.commands)
    }

    fn toggled(&self, label: &str) -> Option<bool> {
        self.nodes.iter().find(|n| n.label == label).and_then(|n| n.toggled)
    }
}

fn open_page(screen: &mut Screen, host: &mut Host, page: Page) {
    if host.window.shell.page != Page::Home {
        host.window.shell.page = Page::Home;
    }
    let key = match page {
        Page::Companion => "mode:companion",
        Page::Skirmish => "mode:skirmish",
        other => panic!("{other:?}"),
    };
    screen.click(key, host);
    assert_eq!(host.window.shell.page, page);
    screen.settle(host);
}

/// The next message of `kind` the overlay received.
fn received(server: &OverlayChannelServer, kind: &str) -> Map<String, Value> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(ServerEvent::Message(_, m)) = server.recv_timeout(Duration::from_millis(50))
            && m.get("type").and_then(Value::as_str) == Some(kind)
        {
            return m;
        }
    }
    panic!("the overlay never received {kind}");
}

/// Everything the overlay received so far (types only).
fn received_types(server: &OverlayChannelServer) -> Vec<String> {
    std::thread::sleep(Duration::from_millis(100));
    server
        .poll()
        .into_iter()
        .filter_map(|e| match e {
            ServerEvent::Message(_, m) => m.get("type").and_then(Value::as_str).map(str::to_owned),
            _ => None,
        })
        .collect()
}

fn user_preset(state: &TempState, file: &str) -> PathBuf {
    state.dir.join("presets").join(file)
}

const LAUNCHED: &str = "Overlay launched. Edit and press Save to apply changes live, or Stop overlay to close it.";

/// Launch a Companion overlay from the Companion page.
fn launch_companion(screen: &mut Screen, host: &mut Host) {
    open_page(screen, host, Page::Companion);
    screen.click("button:Save and launch overlay", host);
    assert_eq!(host.window.status, LAUNCHED);
}

/// The window gives the overlay's channel only 100 ms before it falls back
/// to the files the overlay polls, and a busy CI runner can miss that. Tests
/// that check a channel message connect first.
fn connect_channel(host: &mut Host) {
    assert!(host.channel_connected(5000), "the settings window never reached the overlay's channel");
}

// -- launch -------------------------------------------------------------------

#[test]
fn save_and_launch_writes_the_preset_then_starts_the_engine() {
    let state = TempState::new("launch");
    // A request left behind by an earlier session would stop the new overlay.
    assert!(request_stop(&state.dir));
    let mut host = host(&state);
    let mut screen = Screen::new();
    launch_companion(&mut screen, &mut host);

    let preset = user_preset(&state, "Default.json");
    let launches = &host.launcher.launches;
    assert_eq!(launches.len(), 1);
    let args: Vec<String> = launches[0].args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
    assert_eq!(args, ["--engine", "--preset", &preset.to_string_lossy(), "--mode", "companion"]);
    // Saved to the user's own folder before the overlay was started, and it
    // is exactly what the window holds.
    assert_eq!(launches[0].preset, host.window.current_preset_data().unwrap());
    assert_eq!(launches[0].preset["name"], json!("Default"));
    assert!(!launches[0].stop_request_left, "the stale stop request was not cleared first");
    assert_eq!(host.launched_preset.as_deref(), Some(preset.as_path()));
    assert_eq!(host.overlay_mode, Some(OverlayMode::Companion));
    assert!(host.window.overlay_running);
    // The shipped default.json was not written over.
    let shipped = dc_content::discovery::app_root().join("presets").join("default.json");
    assert!(std::fs::read_to_string(shipped).unwrap().contains("\"mood_mode\""));
}

#[test]
fn an_overlay_that_dies_at_once_is_reported() {
    let state = TempState::new("launch-fail");
    let mut host = host(&state);
    host.launcher.fail_with = Some("The overlay process exited with code 3 right after starting.".into());
    let mut screen = Screen::new();
    open_page(&mut screen, &mut host, Page::Companion);
    screen.click("button:Save and launch overlay", &mut host);
    assert_eq!(host.launcher.launches.len(), 1);
    assert_eq!(
        host.window.status,
        "Overlay failed to start. The overlay process exited with code 3 right after starting."
    );
    assert!(host.launched_preset.is_none());
    assert!(!host.window.overlay_running);
}

// -- live update ----------------------------------------------------------------

#[test]
fn a_second_launch_applies_live_instead_of_relaunching() {
    let state = TempState::new("live");
    let server = state.server();
    let mut host = host(&state);
    let mut screen = Screen::new();
    launch_companion(&mut screen, &mut host);
    let preset = user_preset(&state, "Default.json");
    connect_channel(&mut host);

    // An edit, then Save and launch again while it runs.
    screen.click("check:XP", &mut host);
    assert!(host.window.label_switches["always_show_xp"]);
    screen.click("button:Save and launch overlay", &mut host);
    assert_eq!(host.launcher.launches.len(), 1, "a running overlay was launched again");
    assert!(host.launcher.stops.is_empty());
    assert_eq!(host.window.status, "Applied changes to the running overlay. No restart needed.");
    assert_eq!(received(&server, "hello")["v"], json!(1));
    let update = received(&server, "preset_update");
    assert_eq!(update["preset_path"], json!(preset.to_string_lossy()));
    assert_eq!(update["data"]["settings"]["always_show_xp"], json!(true));
    assert_eq!(update["data"], host.window.current_preset_data().unwrap());
    // The file the overlay polls carries the same preset (the fallback).
    let on_disk: Value = serde_json::from_str(&std::fs::read_to_string(&preset).unwrap()).unwrap();
    assert_eq!(on_disk, update["data"]);

    // Save also applies live.
    screen.click("check:Health", &mut host);
    screen.click("button:Save", &mut host);
    assert!(host.window.status.ends_with("\nApplied changes to the running overlay."), "{}", host.window.status);
    let update = received(&server, "preset_update");
    assert_eq!(update["data"]["settings"]["always_show_health"], json!(false));

    // A tray change comes back over the channel.
    server.broadcast(&dc_platform::live_channel::message(
        json!({"type": "session_state", "state": {"always_show_names": false, "size_scale": 1.25}}),
    ));
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut repaint = false;
    while host.window.status != "Live update received from the running overlay." && Instant::now() < deadline {
        repaint |= host.watch().repaint;
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(repaint, "no live update arrived");
    assert!(!host.window.label_switches["always_show_names"]);
    assert_eq!(host.window.size_scale(), 1.25);
}

/// Both ends in Rust: the settings window's live apply reaches the overlay's
/// `LiveChannel` and its manager, and the overlay's state comes back.
#[test]
fn the_overlay_side_applies_what_the_window_pushes() {
    let state = TempState::new("live-overlay");
    let mut host = host(&state);
    let mut screen = Screen::new();
    launch_companion(&mut screen, &mut host);
    let preset = user_preset(&state, "Default.json");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut rng = dc_core::PyRandom::from_int(7);
    let mut manager =
        dc_manager::CreatureManager::new(&root, &preset, (1920.0, 1080.0), Some(7), &state.dir, false, &mut rng)
            .expect("the launched preset loads");
    let mut channel = crate::live::LiveChannel::open(&state.dir, &preset, false);
    assert!(channel.is_listening());
    let before = manager.always_show_xp;

    screen.click("check:XP", &mut host);
    screen.click("button:Save and launch overlay", &mut host);
    assert_eq!(host.launcher.launches.len(), 1, "a running overlay was launched again");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut applied = false;
    while !applied && Instant::now() < deadline {
        applied = channel.service(&mut manager, &mut rng).preset_applied;
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(applied, "the pushed preset never reached the overlay");
    assert_eq!(manager.always_show_xp, !before);
    assert_eq!(manager.always_show_xp, host.window.label_switches["always_show_xp"]);

    // The overlay's answer: what a tray toggle would publish. Earlier
    // broadcasts (on connect, after the preset) may still be queued.
    let names = !host.window.label_switches["always_show_names"];
    manager.always_show_names = names;
    channel.broadcast(&manager);
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.window.label_switches["always_show_names"] != names && Instant::now() < deadline {
        host.watch();
        channel.service(&mut manager, &mut rng);
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(host.window.label_switches["always_show_names"], names, "the overlay's state never came back");
    assert_eq!(host.window.status, "Live update received from the running overlay.");
    channel.close();
}

#[test]
fn with_no_overlay_running_save_only_saves() {
    let state = TempState::new("save");
    let server = state.server();
    let mut host = host(&state);
    let mut screen = Screen::new();
    open_page(&mut screen, &mut host, Page::Companion);
    screen.click("button:Save", &mut host);
    let preset = user_preset(&state, "Default.json");
    assert_eq!(host.window.status, format!("Saved your preset to {}", preset.display()));
    assert!(preset.is_file());
    assert!(host.launcher.launches.is_empty());
    assert!(!received_types(&server).contains(&"preset_update".to_owned()));
}

// -- stop -------------------------------------------------------------------------

#[test]
fn stop_asks_over_the_channel_and_leaves_the_stop_request_file() {
    let state = TempState::new("stop");
    let server = state.server();
    let mut host = host(&state);
    let mut screen = Screen::new();
    launch_companion(&mut screen, &mut host);
    connect_channel(&mut host);
    screen.click("button:Stop overlay", &mut host);
    received(&server, "stop_request");
    let stops = &host.launcher.stops;
    assert_eq!(stops.len(), 1);
    assert_eq!(stops[0].outcome, StopOutcome::Graceful);
    assert!(stops[0].request_seen, "no stop-request file while the window waited");
    assert!(!stops[0].terminated);
    assert!(!stop_request_path(&state.dir).exists(), "the request outlived the stop");
    assert_eq!(host.window.status, "Overlay stopped and saved its spiders.");
    assert!(host.launched_preset.is_none());
    assert!(!host.window.overlay_running);
}

#[test]
fn a_wedged_overlay_is_killed_after_the_grace_period() {
    let state = TempState::new("stop-wedged");
    let mut host = host(&state);
    host.launcher.answers_stop_after = None;
    let mut screen = Screen::new();
    launch_companion(&mut screen, &mut host);
    screen.click("button:Stop overlay", &mut host);
    let stop = host.launcher.stops[0];
    assert_eq!(stop.outcome, StopOutcome::Terminated);
    assert!(stop.request_seen && stop.terminated);
    assert_eq!(host.window.status, "Overlay did not respond and was closed; recent progress may not have been saved.");
}

#[test]
fn stop_with_nothing_running_clears_a_stale_request() {
    let state = TempState::new("stop-idle");
    assert!(request_stop(&state.dir));
    let mut host = host(&state);
    let mut screen = Screen::new();
    open_page(&mut screen, &mut host, Page::Companion);
    let actions = screen.click("button:Stop overlay", &mut host);
    assert!(actions.contains(&UiAction::StopOverlay));
    assert!(host.launcher.stops.is_empty());
    assert!(!stop_request_path(&state.dir).exists());
    assert_eq!(host.window.status, "No overlay process is running from this window.");
}

// -- reset ------------------------------------------------------------------------

fn confirm_reset(screen: &mut Screen, host: &mut Host) -> Vec<UiAction> {
    let asked = screen.click("button:Reset saved progress...", host);
    assert!(!asked.contains(&UiAction::ResetProgress), "reset without asking");
    screen.click("dialog:Yes", host)
}

#[test]
fn reset_clears_the_creatures_and_keeps_the_rest_like_python() {
    let state = TempState::new("reset");
    let oracle = python();
    let input = serde_json::to_string_pretty(&oracle["reset_input"]).unwrap() + "\n";
    std::fs::write(state.path("creatures.json"), &input).unwrap();
    let mut host = host(&state);
    let mut screen = Screen::new();
    open_page(&mut screen, &mut host, Page::Companion);
    screen.click("button:Save", &mut host);
    let preset_before = std::fs::read_to_string(user_preset(&state, "Default.json")).unwrap();
    let actions = confirm_reset(&mut screen, &mut host);
    assert!(actions.contains(&UiAction::ResetProgress));
    assert_eq!(state.read("creatures.json"), oracle["reset"].as_str().unwrap(), "differs from Python's reset");
    assert_eq!(host.window.status, "Saved progress reset.");
    // Presets and the window's settings are kept.
    assert_eq!(std::fs::read_to_string(user_preset(&state, "Default.json")).unwrap(), preset_before);
    assert_eq!(host.window.slots.len(), 5);
}

#[test]
fn reset_asks_a_listening_overlay_and_leaves_the_file_to_it() {
    let state = TempState::new("reset-live");
    let server = state.server();
    std::fs::write(state.path("creatures.json"), python()["reset_input"].to_string()).unwrap();
    let before = state.read("creatures.json");
    let mut host = host(&state);
    let mut screen = Screen::new();
    open_page(&mut screen, &mut host, Page::Companion);
    connect_channel(&mut host);
    confirm_reset(&mut screen, &mut host);
    received(&server, "reset_progress");
    assert_eq!(host.window.status, "Saved progress reset in the running overlay.");
    assert_eq!(state.read("creatures.json"), before);
}

// -- presets --------------------------------------------------------------------

#[test]
fn preset_edits_save_exactly_what_python_saves() {
    let state = TempState::new("preset-edits");
    let oracle = python();
    let mut host = host(&state);
    let mut screen = Screen::new();
    open_page(&mut screen, &mut host, Page::Companion);
    let saved = user_preset(&state, "Default.json");

    screen.click("button:Save", &mut host);
    assert_eq!(std::fs::read_to_string(&saved).unwrap(), oracle["unchanged"].as_str().unwrap());

    let name = |host: &Host, id: &str| host.window.catalog.models[id].display_name.clone();
    // Labelled as the window lists them (`Legacy: ` for a non-canonical one).
    let temperament = |host: &Host, id: &str| {
        let p = &host.window.catalog.personalities[id];
        if p.canonical { p.display_name.clone() } else { format!("Legacy: {}", p.display_name) }
    };
    // Slot 0: skin, temperament and count.
    let row0 = screen.probe("slot0:category");
    let skin = screen.node("slot 0 skin", |n| {
        n.role == Role::ComboBox
            && n.label.starts_with("Which artwork this slot uses.")
            && (n.rect.center().y - row0.center().y).abs() < 14.0
    });
    let wanted = name(&host, "plush_snow_hybrid_2");
    screen.choose(skin.rect, &wanted, &mut host);
    assert_eq!(host.window.slots[0].model, "plush_snow_hybrid_2");
    let label = temperament(&host, "curious");
    screen.choose(screen.probe("slot0:temperament"), &label, &mut host);
    assert_eq!(host.window.slots[0].personality, "curious");
    screen.settle(&mut host);
    let count = screen.in_row(screen.probe("slot0:skills"), Role::SpinButton);
    screen.type_into(count.rect, "4", &mut host);
    assert_eq!(host.window.slots[0].count, 4);
    // Slot 2: temperament. Slot 3: category, its skin following.
    let label = temperament(&host, "cautious");
    screen.choose(screen.probe("slot2:temperament"), &label, &mut host);
    screen.choose(screen.probe("slot3:category"), "Jumper", &mut host);
    // The skin that follows a category is the newest model file, so Python's
    // pick depends on when the files were checked out: compare with the
    // newest skin here, and substitute it into Python's save.
    let category = host.window.slots[3].category.clone();
    let newest = host.window.catalog.skins_for(Some(&category))[0].id.clone();
    assert_eq!(host.window.slots[3].model, newest);
    let python_pick = oracle["jumper_skin"].as_str().unwrap();
    let expected = oracle["edited"].as_str().unwrap().replace(&format!("\"{python_pick}\""), &format!("\"{newest}\""));

    screen.click("button:Save", &mut host);
    assert_eq!(std::fs::read_to_string(&saved).unwrap(), expected);
}

#[test]
fn a_preset_loads_and_saves_back_unchanged() {
    let state = TempState::new("preset-load");
    let oracle = python();
    let mut host = host(&state);
    let mut screen = Screen::new();
    open_page(&mut screen, &mut host, Page::Companion);
    // The "Saved preset" box is the one left of Load.
    screen.settle(&mut host);
    let load = screen.probe("button:Load");
    let combo = screen
        .nodes
        .iter()
        .filter(|n| {
            n.role == Role::ComboBox
                && (n.rect.center().y - load.center().y).abs() < 14.0
                && n.rect.right() <= load.left()
        })
        .max_by(|a, b| a.rect.right().total_cmp(&b.rect.right()))
        .cloned()
        .expect("the preset box");
    screen.choose(combo.rect, "Colony", &mut host);
    let colony = dc_content::discovery::app_root().join("presets").join("colony.json");
    assert_eq!(host.window.preset_choice.as_deref().map(Path::new), Some(colony.as_path()));
    let actions = screen.click("button:Load", &mut host);
    assert!(actions.iter().any(|a| matches!(a, UiAction::LoadPreset { .. })));
    assert_eq!(host.window.preset_name, "Colony");
    assert_eq!(host.window.status, format!("Loaded preset: {}", colony.display()));
    assert_eq!(host.window.slots.len(), 5);

    screen.click("button:Save", &mut host);
    let saved = user_preset(&state, "Colony.json");
    let text = std::fs::read_to_string(&saved).unwrap();
    // The same keys and values as Python's save (the key order of
    // `settings` differs: see `colony_saves_in_pythons_key_order`).
    let python_colony: Value = serde_json::from_str(oracle["colony"].as_str().unwrap()).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), python_colony, "differs from Python's save");

    // Loading what was saved and saving it again changes nothing.
    host.act(UiAction::LoadPreset { path: saved.to_string_lossy().into_owned() });
    screen.click("button:Save", &mut host);
    assert_eq!(std::fs::read_to_string(&saved).unwrap(), text);
}

/// colony.json carries the retired `mood_mode`, `social_play` and
/// `gait_style` between `interferable` and `flies`. Python's `dict.pop` keeps
/// the order of the rest, and so does dc-ui's `ConfigWindow::
/// current_settings_data` (`shift_remove`: a plain `remove` under
/// `preserve_order` would move `team_relations`/`teams` ahead of `flies`).
#[test]
fn colony_saves_in_pythons_key_order() {
    let state = TempState::new("preset-order");
    let mut host = host(&state);
    let colony = dc_content::discovery::app_root().join("presets").join("colony.json");
    host.act(UiAction::LoadPreset { path: colony.to_string_lossy().into_owned() });
    let preset = host.window.current_preset_data().unwrap();
    host.act(UiAction::SavePreset { preset });
    let text = std::fs::read_to_string(user_preset(&state, "Colony.json")).unwrap();
    assert_eq!(text, python()["colony"].as_str().unwrap());
}

// -- the Adventure profile --------------------------------------------------------

#[test]
fn the_raid_setup_persists_to_the_adventure_profile() {
    let state = TempState::new("profile");
    let mut host = host(&state);
    let mut screen = Screen::new();
    open_page(&mut screen, &mut host, Page::Skirmish);

    screen.click("segment:Two players \u{b7} one keyboard", &mut host);
    assert_eq!(state.profile()["two_player"], json!(true));
    screen.click("pill:all-screens", &mut host);
    assert_eq!(state.profile()["all_screens"], json!(false));
    screen.click("pill:screen2", &mut host);
    assert_eq!(state.profile()["disabled_screens"], json!(["\\\\.\\DISPLAY2"]));
    screen.settle(&mut host);
    assert_eq!(screen.toggled("Use screen 2 (2560\u{d7}1440) in missions"), Some(false));
    screen.click("pill:screen2", &mut host);
    assert_eq!(state.profile()["disabled_screens"], json!([]));
    screen.click("segment:One player", &mut host);
    assert_eq!(state.profile()["two_player"], json!(false));

    // A map: remembered for the overlay, then Adventure starts -- once.
    screen.click("map:territory:play", &mut host);
    let profile = state.profile();
    assert_eq!(profile["selected_map"], json!("territory"));
    assert_eq!(profile["all_screens"], json!(false), "the other switches kept");
    let launches = &host.launcher.launches;
    assert_eq!(launches.len(), 1);
    assert_eq!(launches[0].args.last().map(|a| a.to_string_lossy().into_owned()).as_deref(), Some("adventure"));
    assert_eq!(host.window.status, LAUNCHED, "the launch was followed by a second, live one");
}

#[test]
fn a_locked_map_does_not_launch() {
    let state = TempState::new("locked");
    let mut host = host(&state);
    let actions = vec![
        UiAction::PlayMap { map_id: "obsidian".into() },
        UiAction::LaunchOverlay { mode: OverlayMode::Adventure, preset: host.window.current_preset_data().unwrap() },
    ];
    assert!(WindowApp::handle(&mut host, actions));
    assert!(host.launcher.launches.is_empty());
    assert!(!state.path("adventure-hero.json").exists() || state.profile()["selected_map"] != json!("obsidian"));
}

#[test]
fn a_companion_overlay_is_replaced_by_adventure() {
    let state = TempState::new("switch");
    let mut host = host(&state);
    let mut screen = Screen::new();
    launch_companion(&mut screen, &mut host);
    open_page(&mut screen, &mut host, Page::Skirmish);
    screen.click("map:territory:play", &mut host);
    assert_eq!(host.launcher.stops.len(), 1);
    assert_eq!(host.launcher.launches.len(), 2);
    assert_eq!(host.overlay_mode, Some(OverlayMode::Adventure));
}

// -- hiding during Adventure --------------------------------------------------------

#[test]
fn the_window_hides_while_adventure_runs_and_returns_when_it_exits() {
    let state = TempState::new("hide");
    let mut host = host(&state);
    let mut screen = Screen::new();
    open_page(&mut screen, &mut host, Page::Skirmish);
    screen.take_commands();
    screen.click("map:territory:play", &mut host);
    // Hidden in the frame that asked, which the runner applies before it
    // hands the actions over: before the overlay starts.
    assert!(screen.take_commands().contains(&ViewportCommand::Visible(false)));
    assert_eq!(host.launcher.launches.len(), 1);
    assert!(host.hidden_for_adventure);
    assert_ne!(host.watch().visible, Some(true));
    assert_eq!(host.watch().visible, None, "stays hidden while it runs");

    // The mission ends: the window comes back with the new profile.
    host.launcher.exit_code = Some(0);
    let outcome = host.watch();
    assert_eq!(outcome.visible, Some(true));
    assert!(outcome.repaint);
    assert!(!host.hidden_for_adventure);
    assert_eq!(host.window.status, "Overlay exited with code 0.");
    assert!(!host.window.overlay_running);
    assert_eq!(host.watch().visible, None);
}

#[test]
fn a_failed_adventure_launch_shows_the_window_again() {
    let state = TempState::new("hide-fail");
    let mut host = host(&state);
    host.launcher.fail_with = Some("no GPU".into());
    let mut screen = Screen::new();
    open_page(&mut screen, &mut host, Page::Skirmish);
    screen.take_commands();
    screen.click("map:territory:play", &mut host);
    assert!(screen.take_commands().contains(&ViewportCommand::Visible(false)));
    assert!(!host.hidden_for_adventure);
    assert_eq!(host.watch().visible, Some(true));
    assert_eq!(host.window.status, "Overlay failed to start. no GPU");
}

#[test]
fn leaving_the_adventure_page_stops_only_an_adventure_overlay() {
    let state = TempState::new("leave");
    let mut host = host(&state);
    let mut screen = Screen::new();
    open_page(&mut screen, &mut host, Page::Skirmish);
    screen.click("map:territory:play", &mut host);
    screen.click("button:Modes", &mut host);
    assert_eq!(host.launcher.stops.len(), 1);
    assert_eq!(host.overlay_mode, None);

    // A Companion overlay is meant to keep living.
    launch_companion(&mut screen, &mut host);
    open_page(&mut screen, &mut host, Page::Skirmish);
    screen.click("button:Modes", &mut host);
    assert_eq!(host.launcher.stops.len(), 1);
    assert!(host.launcher.running);
}

// -- dialogs --------------------------------------------------------------------------

fn dialog_name(host: &Host) -> Option<&'static str> {
    host.dialog.as_ref().map(|d| match d {
        Dialog::Character(c) => match c.as_ref() {
            CharacterWindow::Single(_) => "character",
            CharacterWindow::Party(_) => "party",
        },
        Dialog::Controls(_) => "controls",
        Dialog::MapEditor(_) => "map editor",
        Dialog::Admin(_) => "admin",
    })
}

#[test]
fn the_admin_window_writes_the_profile() {
    let state = TempState::new("admin");
    let mut host = host(&state);
    let mut screen = Screen::new();
    open_page(&mut screen, &mut host, Page::Skirmish);
    screen.click("button:Admin", &mut host);
    assert_eq!(dialog_name(&host), Some("admin"));
    screen.click("check:admin", &mut host);
    assert_eq!(state.profile()["admin"], json!(true));
    screen.click("admin:amber", &mut host);
    assert_eq!(profile_mod::load_profile(&state.path("adventure-hero.json")).armoury.amber, 1000);
    screen.settle(&mut host);
    let level = screen.in_row(screen.probe("admin:level"), Role::SpinButton);
    screen.type_into(level.rect, "6", &mut host);
    screen.click("admin:level", &mut host);
    let profile = profile_mod::load_profile(&state.path("adventure-hero.json"));
    assert_eq!(profile_mod::hero_progression(&profile).level, 6);
    screen.click("button:Close", &mut host);
    assert_eq!(dialog_name(&host), None);
    // The page shows the new profile.
    assert_eq!(host.window.shell.adventure.level, 6);
    assert!(host.window.shell.adventure.admin);
}

#[test]
fn the_character_window_renames_and_learns() {
    let state = TempState::new("character");
    // Level 3: a skill point to spend on Vitality.
    admin::set_hero_level(&state.path("adventure-hero.json"), 3);
    let mut host = host(&state);
    let mut screen = Screen::new();
    open_page(&mut screen, &mut host, Page::Skirmish);
    screen.click("button:Character", &mut host);
    assert_eq!(dialog_name(&host), Some("character"));
    screen.settle(&mut host);
    let name = screen.probe("name:hero");
    let actions = screen.type_into(name, "Shelob", &mut host);
    assert!(actions.contains(&UiAction::RenameSpider { who: HERO.into(), name: "Shelob".into() }), "{actions:?}");
    assert_eq!(state.profile()["name"], json!("Shelob"));
    screen.click("tab:Skills", &mut host);
    screen.click("skill:hero:vitality", &mut host);
    let profile = profile_mod::load_profile(&state.path("adventure-hero.json"));
    assert!(profile_mod::hero_progression(&profile).unlocked_abilities.contains(&"vitality".to_owned()));
    screen.click("button:Done", &mut host);
    assert_eq!(dialog_name(&host), None);
    assert_eq!(host.window.shell.adventure.hero_name, "Shelob");
}

#[test]
fn two_players_open_the_party_window() {
    let state = TempState::new("party");
    let mut host = host(&state);
    let mut screen = Screen::new();
    open_page(&mut screen, &mut host, Page::Skirmish);
    screen.click("segment:Two players \u{b7} one keyboard", &mut host);
    screen.click("button:Character", &mut host);
    assert_eq!(dialog_name(&host), Some("party"));
    screen.click("button:Done", &mut host);
    assert_eq!(dialog_name(&host), None);
}

#[test]
fn the_controls_editor_saves_every_change() {
    let state = TempState::new("controls");
    let mut host = host(&state);
    let mut screen = Screen::new();
    open_page(&mut screen, &mut host, Page::Skirmish);
    screen.click("button:Controls", &mut host);
    assert_eq!(dialog_name(&host), Some("controls"));
    screen.click("binding:jump", &mut host);
    screen.key(Key::W, &mut host);
    let saved = state.json("controls.json");
    assert_eq!(saved["bindings"]["jump"], json!("W"));
    assert_eq!(saved["bindings"]["move_up"], json!("Space"), "swapped, never a gap");
    screen.click("button:Done", &mut host);
    assert_eq!(dialog_name(&host), None);
    assert_eq!(host.window.shell.adventure.controls.binding("jump"), "W");
}

#[test]
fn the_map_editor_saves_deletes_and_plays() {
    let state = TempState::new("editor");
    let mut host = host(&state);
    let mut screen = Screen::new();
    open_page(&mut screen, &mut host, Page::Skirmish);
    screen.click("button:Map editor", &mut host);
    assert_eq!(dialog_name(&host), Some("map editor"));
    screen.click("editor:save", &mut host);
    let maps = custom_maps::list_maps(&state.dir);
    assert_eq!(maps.len(), 1);
    let id = maps[0].id.clone();
    let Some(Dialog::MapEditor(editor)) = host.dialog.as_ref() else { panic!("closed") };
    assert_eq!(editor.library, vec![(id.clone(), maps[0].title.clone())]);
    assert_eq!(host.window.shell.adventure.custom_maps.len(), 1);

    // Delete keeps the editor open on a new map (`MapEditor.delete`).
    screen.click("editor:delete", &mut host);
    screen.click("dialog:Yes", &mut host);
    assert!(custom_maps::list_maps(&state.dir).is_empty());
    assert_eq!(dialog_name(&host), Some("map editor"));
    let Some(Dialog::MapEditor(editor)) = host.dialog.as_ref() else { panic!("closed") };
    assert!(editor.library.is_empty());

    // Save and play: saved, the editor closes, Adventure starts on that map
    // -- and the settings window stays.
    screen.take_commands();
    screen.click("editor:play", &mut host);
    let maps = custom_maps::list_maps(&state.dir);
    assert_eq!(maps.len(), 1);
    assert_eq!(dialog_name(&host), None);
    assert_eq!(state.profile()["selected_map"], json!(maps[0].id));
    assert_eq!(host.launcher.launches.len(), 1);
    assert!(host.launcher.stops.is_empty(), "the new overlay was stopped again");
    assert!(screen.take_commands().contains(&ViewportCommand::Visible(false)));
    assert!(host.hidden_for_adventure);
}

#[test]
fn closing_a_dialog_with_its_title_bar_close_box() {
    let state = TempState::new("dialog-x");
    let mut host = host(&state);
    let mut screen = Screen::new();
    open_page(&mut screen, &mut host, Page::Skirmish);
    screen.click("button:Admin", &mut host);
    screen.settle(&mut host);
    let close = screen.node("the dialog's close box", |n| n.role == Role::Button && n.label == "Close window");
    screen.click_at(close.rect.center(), &mut host);
    assert_eq!(dialog_name(&host), None);
}

// -- shutdown --------------------------------------------------------------------------

#[test]
fn closing_the_window_saves_and_stops_the_overlay_without_asking() {
    let state = TempState::new("close");
    let server = state.server();
    let mut host = host(&state);
    let mut screen = Screen::new();
    launch_companion(&mut screen, &mut host);
    connect_channel(&mut host);
    WindowApp::on_close(&mut host);
    received(&server, "stop_request");
    assert_eq!(host.launcher.stops.len(), 1);
    assert_eq!(host.launcher.stops[0].outcome, StopOutcome::Graceful);
    // The channel is closed with the window.
    let deadline = Instant::now() + Duration::from_secs(5);
    while server.client_count() > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(server.client_count(), 0, "the settings window's channel stayed open");
}

#[test]
fn closing_with_nothing_running_stops_nothing_and_a_stray_close_is_ignored() {
    let state = TempState::new("close-idle");
    let mut host = host(&state);
    assert!(WindowApp::handle(&mut host, vec![UiAction::CloseWindow]), "CloseWindow with no dialog closed the app");
    WindowApp::on_close(&mut host);
    assert!(host.launcher.stops.is_empty());
}
