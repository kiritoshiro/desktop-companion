//! Where the key widgets were drawn, for headless tests.
//!
//! Windows call [`record`] for the controls a test needs to press (buttons,
//! switches, slot rows). Recording is off unless a test turns it on with
//! [`enable`], so the windows pay nothing for it in the real app. A test then
//! clicks the centre of a recorded rectangle through ordinary
//! [`egui::RawInput`] events and checks the [`crate::UiAction`]s that come back.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use egui::{Context, Id, Rect};

#[derive(Clone, Default)]
struct Probes {
    enabled: bool,
    rects: Arc<Mutex<HashMap<String, Rect>>>,
}

fn probes(ctx: &Context) -> Probes {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Probes>(Id::unique("dc-ui-probes")).clone())
}

/// Start recording widget positions on this context.
pub fn enable(ctx: &Context) {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Probes>(Id::unique("dc-ui-probes")).enabled = true);
}

/// Remember where `key` was drawn this frame.
pub fn record(ctx: &Context, key: impl Into<String>, rect: Rect) {
    let p = probes(ctx);
    if p.enabled {
        p.rects.lock().unwrap_or_else(|e| e.into_inner()).insert(key.into(), rect);
    }
}

/// Where `key` was last drawn.
pub fn rect(ctx: &Context, key: &str) -> Option<Rect> {
    probes(ctx).rects.lock().unwrap_or_else(|e| e.into_inner()).get(key).copied()
}

/// Every recorded key (sorted), for test failure messages.
pub fn keys(ctx: &Context) -> Vec<String> {
    let mut keys: Vec<String> = probes(ctx).rects.lock().unwrap_or_else(|e| e.into_inner()).keys().cloned().collect();
    keys.sort();
    keys
}

/// Forget everything recorded (before a frame whose layout changed).
pub fn clear(ctx: &Context) {
    probes(ctx).rects.lock().unwrap_or_else(|e| e.into_inner()).clear();
}
