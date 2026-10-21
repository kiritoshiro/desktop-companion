//! Port of `manager/persistence.py`. The caller supplies the state directory.
use crate::{CreatureManager, constants::RUNTIME_STATE_FLUSH_SECONDS};
use dc_state::runtime_state as rs;
use serde_json::{Value, json};
impl CreatureManager {
    pub fn mint_progression_id(&mut self) -> String {
        rs::new_creature_id(&self.progression_namespace, Some(&mut self.id_rng))
    }
    pub fn slot_member_ids(&mut self, slot: &str, count: usize) -> Vec<String> {
        let ns = self.progression_namespace.clone();
        let rng = &mut self.id_rng;
        rs::slot_member_ids(&self.progression_states, &ns, slot, count as i64, || rs::new_creature_id(&ns, Some(rng)))
    }
    pub fn load_runtime_state(&mut self) -> rs::Scene {
        let data = std::fs::read(self.state_folder.join("creatures.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
        self.state_launch = rs::next_launch(data.as_ref());
        let (states, bases) = rs::load_payload(data.as_ref(), self.state_launch);
        self.progression_states = states;
        self.base_world = dc_world::BaseWorld::new(self.screen_w, self.screen_h, &bases);
        self.base_world.playfield = Some(self.playfield.clone());
        rs::load_scene(data.as_ref())
    }
    pub fn scene_to_dict(&self) -> rs::Scene {
        rs::Scene {
            cages: self.cages.iter().map(|c| c.to_dict()).collect(),
            webs: self.web_world.to_dict(),
            nests: self.fly_world.spawners_to_dict(),
        }
    }
    pub fn restore_scene(&mut self, scene: &rs::Scene) {
        for e in &scene.cages {
            if let Some(mut c) = dc_world::cage::Cage::from_dict(e) {
                c.clamp_to_screen(self.screen_w, self.screen_h, 8.0);
                self.cages.push(c);
            }
        }
        let ids: Vec<_> = self.creatures.iter().map(|c| c.id).collect();
        for id in ids {
            self.settle_creature_membership(id);
        }
        self.web_world.restore_list(&scene.webs);
        if self.flies_spawner && !scene.nests.is_empty() {
            self.fly_world.restore_spawners(&Value::Array(scene.nests.clone()));
        }
    }
    pub fn runtime_payload(&mut self) -> Value {
        let mut states = self.progression_states.clone();
        let mut ordinal = rs::next_birth_ordinal(&states);
        for c in &self.creatures {
            let born = states
                .get(&c.progression_id)
                .and_then(|e| e.get("born"))
                .and_then(dc_content::util::py_int)
                .unwrap_or_else(|| {
                    let b = ordinal;
                    ordinal += 1;
                    b
                });
            let entry = json!({"name":c.name,"model":c.model.get("id"),"personality":c.personality.get("id"),"progression":c.progression.to_value(),"slot_id":self.slot_ids.get(&c.id).map(String::as_str).unwrap_or(""),"born":born,"last_seen":self.state_launch});
            states.insert(c.progression_id.clone(), entry);
        }
        states = rs::evict(&states, self.state_launch, rs::RETAIN_LAUNCHES);
        rs::build_payload(&states, &self.base_world.to_dict(), self.state_launch, Some(&self.scene_to_dict()))
    }
    pub fn save_runtime_state(&mut self) -> std::io::Result<()> {
        let payload = self.runtime_payload();
        rs::write_payload_atomic(&self.state_folder.join("creatures.json"), &payload)?;
        self.progression_states = payload["creatures"].as_object().cloned().unwrap_or_default();
        self.runtime_state_dirty = false;
        self.runtime_state_flush_accum = 0.0;
        Ok(())
    }
    pub(crate) fn save_or_warn(&mut self) {
        if let Err(e) = self.save_runtime_state() {
            self.warnings.push(format!("Could not write runtime state: {e}"));
        }
    }
    pub fn mark_runtime_state_dirty(&mut self) {
        self.runtime_state_dirty = true;
    }
    pub fn flush_runtime_state(&mut self, dt: f64) {
        if self.runtime_state_dirty {
            self.runtime_state_flush_accum += dt.max(0.0);
            if self.runtime_state_flush_accum >= RUNTIME_STATE_FLUSH_SECONDS {
                self.save_or_warn();
            }
        }
    }
    pub fn reset_saved_progress(&mut self, rng: &mut dc_core::PyRandom) {
        let scene = self.scene_to_dict();
        self.progression_states.clear();
        self.base_world.clear();
        self.cages.clear();
        let data = self.preset_data.clone();
        self.load_preset_data(&data, rng);
        self.restore_scene(&scene);
        self.save_or_warn();
    }
}
