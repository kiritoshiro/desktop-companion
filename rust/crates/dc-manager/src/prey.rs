//! Port of `manager/prey.py`.
use crate::CreatureManager;
use dc_world::flies::FlyConfigure;
use serde_json::Value;
impl CreatureManager {
    pub fn award_feed_xp(&mut self, id: u64, amount: f64, _source: &str) -> Vec<String> {
        let Some(c) = self.creature_mut(id) else {
            return vec![];
        };
        let events = c.gain_experience(amount);
        self.mark_runtime_state_dirty();
        events
    }
    pub fn apply_fly_settings(&mut self, settings: &Value) {
        let Some(f) = settings.get("flies").filter(|v| v.is_object()) else {
            return;
        };
        if let Some(v) = f.get("enabled") {
            self.flies_enabled = dc_content::util::py_truthy(v);
        }
        for (key, min) in [("min_interval", 0.3), ("max_interval", 0.4)] {
            if let Some(v) = f.get(key) {
                if let Some(n) = dc_content::util::py_float(v) {
                    if key == "min_interval" {
                        self.fly_min_interval = n.max(min);
                    } else {
                        self.fly_max_interval = n.max(min);
                    }
                } else {
                    self.warnings.push("Preset settings.flies interval was invalid; using defaults.".into());
                    break;
                }
            }
        }
        self.fly_max_interval = self.fly_max_interval.max(self.fly_min_interval);
        if let Some(v) = f.get("max_flies") {
            if let Some(n) = dc_content::util::py_int(v) {
                self.fly_max = n.clamp(0, 40);
            } else {
                self.warnings.push("Preset settings.flies max_flies was invalid; using default.".into());
            }
        }
        if let Some(v) = f.get("spawner") {
            self.flies_spawner = dc_content::util::py_truthy(v);
        }
        self.fly_world.configure(FlyConfigure {
            enabled: Some(self.flies_enabled),
            min_interval: Some(self.fly_min_interval),
            max_interval: Some(self.fly_max_interval),
            max_flies: Some(self.fly_max),
            scale: Some(self.size_scale),
            use_spawner: Some(self.flies_spawner),
        });
    }
    pub fn set_flies_enabled(&mut self, enabled: bool) -> String {
        self.flies_enabled = enabled;
        self.fly_world.configure(FlyConfigure { enabled: Some(enabled), ..Default::default() });
        if !enabled {
            self.fly_world.clear();
            for c in &mut self.creatures {
                c.prey = None;
                c.hunting_prey = false;
            }
        }
        format!("Flies turned {}.", if enabled { "on" } else { "off" })
    }
    pub fn set_fly_spawn_interval(&mut self, min: f64, max: f64) -> String {
        self.fly_min_interval = min.max(0.3);
        self.fly_max_interval = max.max(self.fly_min_interval);
        self.fly_world.configure(FlyConfigure {
            min_interval: Some(self.fly_min_interval),
            max_interval: Some(self.fly_max_interval),
            ..Default::default()
        });
        format!("Flies now spawn every {:.0}-{:.0}s.", self.fly_min_interval, self.fly_max_interval)
    }
    pub fn set_fly_max(&mut self, max: i64) -> String {
        self.fly_max = max.clamp(0, 40);
        self.fly_world.configure(FlyConfigure { max_flies: Some(self.fly_max), ..Default::default() });
        format!("Up to {} flies at once.", self.fly_max)
    }
    pub fn set_fly_rate_preset(&mut self, name: &str) -> String {
        let key = name.trim().to_lowercase();
        if key == "off" {
            return self.set_flies_enabled(false);
        }
        let (lo, hi, cap, label) = match key.as_str() {
            "sparse" => (9.0, 18.0, 3, "Sparse"),
            "normal" => (4.0, 9.0, 6, "Normal"),
            "swarm" => (1.0, 3.0, 14, "Swarm"),
            _ => return "Unknown fly setting.".into(),
        };
        self.flies_enabled = true;
        self.fly_min_interval = lo;
        self.fly_max_interval = hi;
        self.fly_max = cap;
        self.fly_world.configure(FlyConfigure {
            enabled: Some(true),
            min_interval: Some(lo),
            max_interval: Some(hi),
            max_flies: Some(cap),
            ..Default::default()
        });
        format!("Flies set to {label}.")
    }
    pub fn release_fly(&mut self) -> String {
        self.fly_world.spawn_now().into()
    }
    pub fn set_fly_spawner_enabled(&mut self, enabled: bool) -> String {
        self.flies_spawner = enabled;
        self.fly_world.configure(FlyConfigure { use_spawner: Some(enabled), ..Default::default() });
        if enabled { "Flies now emerge from a movable nest." } else { "Flies now drift in from the screen edges." }
            .into()
    }
    pub fn add_fly_spawner(&mut self) -> String {
        self.flies_spawner = true;
        self.fly_world.add_spawner(None);
        format!("Added a nest ({} on screen).", self.fly_world.spawners.len())
    }
    pub fn reset_fly_spawner(&mut self) -> String {
        self.flies_spawner = true;
        self.fly_world.reset_spawners();
        "Nest reset to one in its default spot.".into()
    }
}
