//! The overlay's end of the live channel to the settings window
//! (`_channel_server`, `_on_channel_message`, `_apply_pushed_preset` and
//! `_broadcast_session_state` in `app/engine.py`).
//!
//! One pipe server per running overlay, named after the state folder
//! (`dc_platform::live_channel`). The preset-file poll and the stop-request
//! file stay in the engine as fallbacks, as in Python: a settings window on
//! another build, or a channel that failed to bind, still works.
//!
//! The engine calls [`LiveChannel::service`] every loop pass with the
//! scene's manager. It applies pushed presets and progress resets there,
//! publishes `session_state` when a settings window connects or a preset
//! lands, and reports a stop request back for the engine's save-and-quit.

use std::path::{Path, PathBuf};

use dc_core::PyRandom;
use dc_manager::CreatureManager;
use dc_platform::live_channel::{
    ChannelMessage, OverlayChannelServer, ServerEvent, channel_name, message, resolve_path_string,
};
use serde_json::{Value, json};

/// What one [`LiveChannel::service`] pass did that the engine must follow up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Serviced {
    /// A `stop_request` arrived: save and quit (`_do_graceful_stop`).
    pub stop: bool,
    /// A pushed preset was applied: the file poll must not apply it again.
    pub preset_applied: bool,
}

pub struct LiveChannel {
    server: OverlayChannelServer,
    /// The launched preset, as `Path.resolve()` gives it.
    preset: String,
    adventure: bool,
}

impl LiveChannel {
    /// Bind the channel for `state_dir`. When it cannot bind (another
    /// overlay holds the name) the channel stays closed and the settings
    /// window falls back to the files.
    pub fn open(state_dir: &Path, preset: &Path, adventure: bool) -> Self {
        let mut server = OverlayChannelServer::new(channel_name(state_dir));
        if !server.listen() {
            log::warn!("Live channel unavailable; settings window will use file polling only");
        }
        Self { server, preset: resolve_path_string(preset), adventure }
    }

    #[cfg(test)]
    pub fn is_listening(&self) -> bool {
        self.server.is_listening()
    }

    pub fn close(&mut self) {
        self.server.close();
    }

    /// Handle every message queued since the last pass.
    pub fn service(&mut self, manager: &mut CreatureManager, rng: &mut PyRandom) -> Serviced {
        let mut done = Serviced::default();
        let mut publish = false;
        for event in self.server.poll() {
            match event {
                // `client_connected.connect(self._broadcast_session_state)`.
                ServerEvent::ClientConnected(_) => publish = true,
                ServerEvent::Message(_, map) => match self.request(&ChannelMessage::from_map(&map)) {
                    Some(Request::Stop) => done.stop = true,
                    Some(Request::ResetProgress) => manager.reset_saved_progress(rng),
                    Some(Request::ApplyPreset(data)) => {
                        let text = manager.reload_from_preset_data(&data, rng);
                        for warning in &manager.warnings {
                            log::warn!("{warning}");
                        }
                        log::info!("{text}");
                        done.preset_applied = true;
                        publish = true;
                    }
                    None => {}
                },
                ServerEvent::ClientDisconnected(_) => {}
            }
        }
        if publish {
            self.broadcast(manager);
        }
        done
    }

    /// `_broadcast_session_state`: the tray-changeable state, to every
    /// connected settings window.
    pub fn broadcast(&self, manager: &CreatureManager) {
        if self.server.client_count() == 0 {
            return;
        }
        self.server.broadcast(&message(json!({"type": "session_state", "state": manager.session_snapshot()})));
    }

    /// `_on_channel_message` and `_apply_pushed_preset`'s checks: what a
    /// message asks the overlay to do, if anything.
    fn request(&self, msg: &ChannelMessage) -> Option<Request> {
        match msg {
            ChannelMessage::StopRequest => Some(Request::Stop),
            // Adventure plays the raid's own rules and progress.
            ChannelMessage::PresetUpdate { .. } | ChannelMessage::ResetProgress if self.adventure => None,
            ChannelMessage::ResetProgress => Some(Request::ResetProgress),
            ChannelMessage::PresetUpdate { preset_path, data } => {
                if !data.is_object() {
                    return None;
                }
                // A preset other than the one this overlay runs.
                if let Some(path) = preset_path.as_deref().filter(|p| !p.is_empty())
                    && resolve_path_string(&PathBuf::from(path)) != self.preset
                {
                    return None;
                }
                Some(Request::ApplyPreset(data.clone()))
            }
            // `client_connected` already queued a fresh broadcast.
            ChannelMessage::Hello => None,
            ChannelMessage::SessionState { .. } | ChannelMessage::Unknown(_) => {
                log::debug!("Ignoring live-channel message {msg:?}");
                None
            }
        }
    }
}

#[derive(Debug, PartialEq)]
enum Request {
    Stop,
    ResetProgress,
    ApplyPreset(Value),
}

#[cfg(test)]
mod tests {
    use super::*;
    use dc_platform::live_channel::{ClientEvent, SettingsChannelClient};
    use std::time::{Duration, Instant};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let dir = std::env::temp_dir().join(format!("dc-live-{name}-{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    fn preset() -> PathBuf {
        root().join("presets/default.json")
    }

    fn manager(state: &Path) -> (CreatureManager, PyRandom) {
        let mut rng = PyRandom::from_int(7);
        let m = CreatureManager::new(&root(), &preset(), (1920.0, 1080.0), Some(7), state, false, &mut rng)
            .expect("the default preset loads");
        (m, rng)
    }

    fn default_preset() -> Value {
        serde_json::from_str(&std::fs::read_to_string(preset()).unwrap()).unwrap()
    }

    /// Service the channel until `done` sees what it waits for (2 s at most).
    fn until<T>(mut step: impl FnMut() -> Option<T>) -> Option<T> {
        let end = Instant::now() + Duration::from_secs(2);
        while Instant::now() < end {
            if let Some(v) = step() {
                return Some(v);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }

    fn session_state(client: &SettingsChannelClient) -> Option<Value> {
        client.poll().into_iter().find_map(|e| match e {
            ClientEvent::Message(m) => match ChannelMessage::from_map(&m) {
                ChannelMessage::SessionState { state } => Some(state),
                _ => None,
            },
            _ => None,
        })
    }

    fn preset_update(path: Option<&Path>, data: Value) -> serde_json::Map<String, Value> {
        ChannelMessage::PresetUpdate { preset_path: path.map(|p| p.to_string_lossy().into_owned()), data }.to_map()
    }

    #[test]
    fn requests_follow_on_channel_message() {
        let state = TempDir::new("requests");
        let companion = LiveChannel {
            server: OverlayChannelServer::new("unused"),
            preset: resolve_path_string(&preset()),
            adventure: false,
        };
        let data = default_preset();
        let ours = preset_update(Some(&preset()), data.clone());
        assert_eq!(companion.request(&ChannelMessage::from_map(&ours)), Some(Request::ApplyPreset(data.clone())));
        // No path, or an empty one, means the running preset.
        let unnamed = preset_update(None, data.clone());
        assert_eq!(companion.request(&ChannelMessage::from_map(&unnamed)), Some(Request::ApplyPreset(data.clone())));
        let other = preset_update(Some(&state.0.join("other.json")), data.clone());
        assert_eq!(companion.request(&ChannelMessage::from_map(&other)), None);
        let malformed = preset_update(Some(&preset()), json!([1, 2]));
        assert_eq!(companion.request(&ChannelMessage::from_map(&malformed)), None);
        assert_eq!(companion.request(&ChannelMessage::ResetProgress), Some(Request::ResetProgress));
        assert_eq!(companion.request(&ChannelMessage::StopRequest), Some(Request::Stop));
        assert_eq!(companion.request(&ChannelMessage::Hello), None);
        assert_eq!(companion.request(&ChannelMessage::Unknown("zzz".into())), None);

        // Adventure only listens for stop.
        let adventure = LiveChannel { adventure: true, ..companion };
        assert_eq!(adventure.request(&ChannelMessage::from_map(&ours)), None);
        assert_eq!(adventure.request(&ChannelMessage::ResetProgress), None);
        assert_eq!(adventure.request(&ChannelMessage::StopRequest), Some(Request::Stop));
    }

    #[test]
    fn a_settings_window_gets_the_state_pushes_a_preset_and_stops_the_overlay() {
        let state = TempDir::new("pipe");
        let (mut m, mut rng) = manager(&state.0);
        let mut channel = LiveChannel::open(&state.0, &preset(), false);
        assert!(channel.is_listening(), "the overlay could not bind its channel");
        // A second overlay on the same state folder falls back to files.
        let rival = LiveChannel::open(&state.0, &preset(), false);
        assert!(!rival.is_listening());

        let mut client = SettingsChannelClient::new(channel_name(&state.0));
        assert!(client.try_connect(500), "the settings window could not connect");
        assert!(client.send(&ChannelMessage::Hello.to_map()));

        // Connecting publishes the session state.
        let first = until(|| {
            channel.service(&mut m, &mut rng);
            session_state(&client)
        })
        .expect("no session_state after connecting");
        assert_eq!(first["creatures"].as_array().unwrap().len(), m.creatures.len());
        assert!(m.creatures.len() >= 2);

        // A pushed preset is applied at once and published again.
        let mut data = default_preset();
        data["slots"] = json!([data["slots"][0].clone()]);
        data["settings"]["size_scale"] = json!(1.5);
        assert!(client.send(&preset_update(Some(&preset()), data)));
        let mut serviced = Serviced::default();
        let second = until(|| {
            let s = channel.service(&mut m, &mut rng);
            serviced.preset_applied |= s.preset_applied;
            session_state(&client)
        })
        .expect("no session_state after the preset");
        assert!(serviced.preset_applied);
        assert_eq!(m.creatures.len(), 1);
        assert_eq!(second["creatures"].as_array().unwrap().len(), 1);
        assert_eq!(second["size_scale"], json!(1.5));

        // A preset for another file changes nothing.
        let mut other = default_preset();
        other["slots"] = json!([]);
        assert!(client.send(&preset_update(Some(&state.0.join("other.json")), other)));
        // Then stop: handled in the order sent, after the ignored preset.
        assert!(client.send(&ChannelMessage::StopRequest.to_map()));
        let stop = until(|| channel.service(&mut m, &mut rng).stop.then_some(())).is_some();
        assert!(stop, "the stop request never arrived");
        assert_eq!(m.creatures.len(), 1);

        client.close();
        channel.close();
        assert!(!channel.is_listening());
    }

    #[test]
    fn reset_progress_over_the_channel_forgets_saved_stats() {
        let state = TempDir::new("reset");
        let (mut m, mut rng) = manager(&state.0);
        // As tests/test_reset_progress.py grows the colony.
        let count = m.creatures.len();
        for c in &mut m.creatures {
            c.progression.level = 7;
            c.progression.xp = 900;
        }
        m.save_runtime_state().unwrap();
        let mut channel = LiveChannel::open(&state.0, &preset(), false);
        let mut client = SettingsChannelClient::new(channel_name(&state.0));
        assert!(client.try_connect(500));
        assert!(client.send(&ChannelMessage::ResetProgress.to_map()));
        let reset = until(|| {
            channel.service(&mut m, &mut rng);
            m.creatures.iter().all(|c| c.progression.level < 7).then_some(())
        });
        assert!(reset.is_some(), "the reset never reached the manager");
        assert_eq!(m.creatures.len(), count, "the same colony comes back");
        let saved: Value =
            serde_json::from_str(&std::fs::read_to_string(state.0.join("creatures.json")).unwrap()).unwrap();
        let entries = saved["creatures"].as_object().unwrap();
        assert!(entries.values().all(|e| e["progression"]["level"].as_i64().unwrap() < 7), "{saved}");
        client.close();
        channel.close();
    }
}
