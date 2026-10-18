//! Port of the logic of `creature/web_net.py`: laying out the net over a
//! webbed spider and the struggle against it. Drawing the net is
//! `dc-paint`'s: it reads [`Creature::web_net`], [`Creature::net_points`],
//! [`Creature::web_strength`], `struggle` and `struggle_clock`.

use crate::constants::*;
use crate::creature::Creature;
use crate::pymath::{clamp, py_mod};
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

pub const NET_SPOKES: usize = 9;
pub const NET_RINGS: [f64; 4] = [0.28, 0.52, 0.76, 1.0];
pub const NET_OUTER: f64 = 1.35;
pub const NET_SAG: f64 = 0.16;

/// One spoke of the net (the Python dict).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct NetSpoke {
    pub angle: f64,
    pub rings: [f64; 4],
    /// World-space guy-line end.
    pub anchor: (f64, f64),
    pub tear: f64,
    pub ring_tears: [f64; 4],
}

impl Creature {
    /// `_pin_net()`: spokes, rings, guy lines and where it tears.
    pub fn pin_net(&mut self) {
        let rng = &mut self.rng;
        let start = rng.uniform(0.0, TAU);
        let (x, y, size) = (self.kin.x, self.kin.y, self.kin.size);
        let mut spokes = Vec::with_capacity(NET_SPOKES);
        for i in 0..NET_SPOKES {
            let angle = start + i as f64 * TAU / NET_SPOKES as f64 + rng.uniform(-0.16, 0.16);
            let mut rings = [0.0; 4];
            for (r, f) in rings.iter_mut().zip(NET_RINGS) {
                *r = f * rng.uniform(0.90, 1.08);
            }
            let anchor = size * rng.uniform(1.75, 2.15);
            let tear = rng.uniform(0.0, 0.55);
            let mut ring_tears = [0.0; 4];
            for t in ring_tears.iter_mut() {
                *t = rng.uniform(0.0, 1.0);
            }
            spokes.push(NetSpoke { angle, rings, anchor: (x + angle.cos() * anchor, y + angle.sin() * anchor), tear, ring_tears });
        }
        self.web_anchors = spokes.iter().map(|s| s.anchor).collect();
        self.web_net = spokes;
        self.struggle_clock = 0.0;
    }

    /// `_wants_to_struggle()`
    pub fn wants_to_struggle(&self) -> bool {
        if self.player_control {
            return self.player_struggling;
        }
        true
    }

    /// `_update_web_struggle(dt)`
    pub fn update_web_struggle(&mut self, dt: f64) {
        if !self.webbed() || self.dead {
            self.struggle = (self.struggle - dt * 4.0).max(0.0);
            if !self.webbed() {
                self.web_anchors.clear();
                self.web_net.clear();
            }
            return;
        }
        self.struggle_clock += dt;
        let bursting = py_mod(self.struggle_clock, STRUGGLE_PERIOD) < STRUGGLE_PERIOD * STRUGGLE_DUTY;
        let active = bursting && self.wants_to_struggle();
        let target = if active { 1.0 } else { 0.0 };
        self.struggle += (target - self.struggle) * (1.0 - (-dt * 14.0).exp());
        if active {
            self.webbed_timer = (self.webbed_timer - dt * STRUGGLE_DRAIN).max(0.0);
        }
    }

    /// `web_strength()`: 1 for a fresh net, 0 as the spider gets loose.
    pub fn web_strength(&self) -> f64 {
        clamp(self.webbed_timer / WEBBED_SECONDS, 0.0, 1.0)
    }

    /// `_net_points()`: per spoke, its ring points in world space with the
    /// strain applied.
    pub fn net_points(&self) -> Vec<(NetSpoke, [(f64, f64); 4])> {
        let strain = 1.0 + 0.035 * self.struggle * (self.struggle_clock * 27.0).sin();
        let outer = self.kin.size * NET_OUTER * strain;
        let (cx, cy) = (self.kin.x, self.kin.y - self.jump_z);
        self.web_net
            .iter()
            .map(|spoke| {
                let (c, s) = (spoke.angle.cos(), spoke.angle.sin());
                let mut pts = [(0.0, 0.0); 4];
                for (p, r) in pts.iter_mut().zip(spoke.rings) {
                    *p = (cx + c * outer * r, cy + s * outer * r);
                }
                (*spoke, pts)
            })
            .collect()
    }
}
