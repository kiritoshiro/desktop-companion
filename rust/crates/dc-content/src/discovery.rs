//! Port of `content/discovery.py`: where data lives, and finding it.
//!
//! - Data roots: [`app_root`], [`candidate_roots`], [`data_dirs`],
//!   [`find_data_file`], [`resolve_preset_path`].
//! - State location: [`state_dir`] (`DESKTOP_BUG_STATE_DIR`, then a
//!   `portable.txt` marker, then `%LOCALAPPDATA%\DesktopBugCompanion\state`),
//!   [`migrate_legacy_state_dir`], [`state_dir_is_writable`]. The same rules
//!   are available on an explicit [`StateLocation`] for tests.
//! - Discovery and validation of models, personalities and presets.
//!
//! **Forced change:** Python's `app_root()` is the project folder (counted
//! from the module file) or, frozen, the executable's folder. A Rust binary
//! has no source file at run time, so [`app_root`] is: an explicit
//! [`set_app_root`] override, else `DESKTOP_BUG_APP_ROOT`, else the nearest
//! folder at or above the executable that holds `models/` and
//! `personalities/` (the executable's own folder in a release, the checkout
//! in a `cargo` build), else this crate's checkout, else the executable's
//! folder. There is no PyInstaller `_MEIPASS`; the `_internal` fallback stays.

use std::path::{Component, Path, PathBuf};
use std::sync::RwLock;

use serde_json::Value;

use crate::body_plans::{BODY_PLAN_IDS, is_body_plan, resolve_body_plan, resolve_model_body_plan};
use crate::model::ModelDef;
use crate::personality::Personality;
use crate::personality_profiles::{
    BEHAVIOUR_MODULE_IDS, TEMPERAMENT_TRAIT_IDS, ability_bundle, annotate_personality,
    canonical_personality_definitions, is_behaviour_phase, is_movement_profile,
};
use crate::skills::{is_ability_skill, is_skill};
use crate::util::{OrderedMap, casefold, is_number, py_float, py_int, py_key, py_repr, py_truthy};

// ---------------------------------------------------------------------------
// Roots
// ---------------------------------------------------------------------------

static APP_ROOT_OVERRIDE: RwLock<Option<PathBuf>> = RwLock::new(None);
static STATE_DIR_OVERRIDE: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Pin [`app_root`] (the embedding app, or a test). `None` restores discovery.
pub fn set_app_root(path: Option<PathBuf>) {
    *APP_ROOT_OVERRIDE.write().unwrap() = path;
}

/// Pin [`state_dir`] for this process, ahead of `DESKTOP_BUG_STATE_DIR`
/// (a test harness cannot safely mutate the environment of a threaded process).
pub fn set_state_dir_override(path: Option<PathBuf>) {
    *STATE_DIR_OVERRIDE.write().unwrap() = path;
}

fn looks_like_data_root(dir: &Path) -> bool {
    dir.join("models").is_dir() && dir.join("personalities").is_dir()
}

/// The writable/editable data root (see the module notes).
pub fn app_root() -> PathBuf {
    if let Some(p) = APP_ROOT_OVERRIDE.read().unwrap().clone() {
        return p;
    }
    if let Ok(p) = std::env::var("DESKTOP_BUG_APP_ROOT")
        && !p.trim().is_empty()
    {
        return PathBuf::from(p.trim());
    }
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    if let Some(dir) = &exe_dir {
        let dir = resolve_path(dir);
        for ancestor in dir.ancestors() {
            if looks_like_data_root(ancestor) {
                return ancestor.to_path_buf();
            }
        }
    }
    // crates/dc-content -> crates -> rust -> the checkout.
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("..");
    if looks_like_data_root(&checkout) {
        return resolve_path(&checkout);
    }
    exe_dir.unwrap_or_else(|| PathBuf::from("."))
}

/// Strip Windows' `\\?\` verbatim prefix that `canonicalize` adds.
fn simplify(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    path
}

/// `Path.resolve()` (non-strict): canonical when it exists, absolute otherwise.
pub fn resolve_path(path: &Path) -> PathBuf {
    match std::fs::canonicalize(path) {
        Ok(p) => simplify(p),
        Err(_) => std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()),
    }
}

fn unique_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut out = Vec::new();
    for path in paths {
        let key = resolve_path(&path);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(path);
    }
    out
}

fn added_time(path: &Path) -> f64 {
    std::fs::metadata(path)
        .and_then(|m| m.created().or_else(|_| m.modified()))
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// `_newest_first`: newest file first, then parent name, then name (case-folded).
fn sort_newest_first(paths: &mut [PathBuf]) {
    let key = |p: &PathBuf| {
        let parent = p.parent().and_then(Path::file_name).map(|s| casefold(&s.to_string_lossy())).unwrap_or_default();
        let name = p.file_name().map(|s| casefold(&s.to_string_lossy())).unwrap_or_default();
        (-added_time(p), parent, name)
    };
    paths.sort_by(|a, b| {
        let (ka, kb) = (key(a), key(b));
        ka.0.partial_cmp(&kb.0).unwrap_or(std::cmp::Ordering::Equal).then(ka.1.cmp(&kb.1)).then(ka.2.cmp(&kb.2))
    });
}

/// `candidate_roots`: where data folders may live, user data first.
pub fn candidate_roots(root: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(r) = root {
        roots.push(r.to_path_buf());
    }
    roots.push(user_data_root());
    let writable = app_root();
    roots.push(writable.clone());
    roots.push(writable.join("_internal"));
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    unique_paths(roots)
}

/// `data_path`: a writable path beside the project/executable.
pub fn data_path(parts: &[&str]) -> PathBuf {
    parts.iter().fold(app_root(), |p, part| p.join(part))
}

/// `user_data_root`: where things the user creates are written (= `state_dir()`).
pub fn user_data_root() -> PathBuf {
    state_dir()
}

/// `user_presets_dir`.
pub fn user_presets_dir() -> PathBuf {
    user_data_root().join("presets")
}

/// `shipped_presets_dirs`: preset folders that came with the build (read-only).
pub fn shipped_presets_dirs() -> Vec<PathBuf> {
    let root = app_root();
    unique_paths(vec![root.join("presets"), root.join("_internal").join("presets")])
}

/// `is_shipped_preset`: case-insensitive, because `Default.json` and
/// `default.json` are one file on Windows.
pub fn is_shipped_preset(path: &Path) -> bool {
    let candidate = resolve_path(path);
    let parent = candidate.parent().map(|p| casefold(&p.to_string_lossy())).unwrap_or_default();
    shipped_presets_dirs().iter().any(|dir| casefold(&resolve_path(dir).to_string_lossy()) == parent)
}

fn path_parts(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            Component::ParentDir => Some("..".into()),
            _ => None,
        })
        .collect()
}

/// `resolve_preset_path`: resolve a preset argument for *reading*. A path
/// that cannot be found comes back unchanged so the caller reports it.
pub fn resolve_preset_path(value: &Path) -> PathBuf {
    if value.is_absolute() {
        return value.to_path_buf();
    }
    let parts = path_parts(value);
    if parts.is_empty() {
        return find_data_file(&["presets", "default.json"], None);
    }
    let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
    let found = find_data_file(&refs, None);
    if found.exists() {
        return found;
    }
    if parts.len() == 1 {
        let bundled = find_data_file(&["presets", &parts[0]], None);
        if bundled.exists() {
            return bundled;
        }
    }
    found
}

/// `data_dirs`: existing data directories named `folder_name`.
pub fn data_dirs(folder_name: &str, root: Option<&Path>) -> Vec<PathBuf> {
    let dirs = candidate_roots(root).into_iter().map(|b| b.join(folder_name)).filter(|p| p.is_dir()).collect();
    unique_paths(dirs)
}

/// `find_data_file`: an existing bundled/editable file, or the writable fallback.
pub fn find_data_file(parts: &[&str], root: Option<&Path>) -> PathBuf {
    for base in candidate_roots(root) {
        let path = parts.iter().fold(base, |p, part| p.join(part));
        if path.exists() {
            return path;
        }
    }
    data_path(parts)
}

// ---------------------------------------------------------------------------
// State location
// ---------------------------------------------------------------------------

/// The inputs `state_dir()` depends on, so the rules can be exercised without
/// touching the process environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateLocation {
    /// `DESKTOP_BUG_STATE_DIR` (or the process override).
    pub override_dir: Option<PathBuf>,
    pub app_root: PathBuf,
    /// `LOCALAPPDATA`.
    pub local_app_data: Option<String>,
    /// `Path.home()`.
    pub home: PathBuf,
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

impl StateLocation {
    /// The live process's inputs.
    pub fn current() -> Self {
        let override_dir = STATE_DIR_OVERRIDE
            .read()
            .unwrap()
            .clone()
            .or_else(|| env_nonempty("DESKTOP_BUG_STATE_DIR").map(PathBuf::from));
        let home = env_nonempty("USERPROFILE")
            .or_else(|| env_nonempty("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        Self { override_dir, app_root: app_root(), local_app_data: std::env::var("LOCALAPPDATA").ok(), home }
    }

    /// `is_portable_install`: a `portable.txt` marker beside the install.
    pub fn is_portable_install(&self) -> bool {
        self.app_root.join("portable.txt").exists()
    }

    /// `default_state_root`: `%LOCALAPPDATA%\DesktopBugCompanion`.
    pub fn default_state_root(&self) -> PathBuf {
        let local = self.local_app_data.as_deref().map(str::trim).unwrap_or("");
        let base = if local.is_empty() { self.home.join("AppData").join("Local") } else { PathBuf::from(local) };
        base.join("DesktopBugCompanion")
    }

    /// `state_dir`.
    pub fn state_dir(&self) -> PathBuf {
        if let Some(p) = &self.override_dir {
            return p.clone();
        }
        if self.is_portable_install() {
            return self.app_root.join("state");
        }
        self.default_state_root().join("state")
    }

    /// `migrate_legacy_state_dir`: copy (never move) an old beside-the-exe
    /// state folder to its new home, once.
    pub fn migrate_legacy_state_dir(&self) {
        if self.override_dir.is_some() || self.is_portable_install() {
            return;
        }
        let new_dir = self.state_dir();
        let legacy = self.app_root.join("state");
        if legacy == new_dir || !legacy.is_dir() || new_dir.join("creatures.json").exists() {
            return;
        }
        let result = (|| -> std::io::Result<()> {
            std::fs::create_dir_all(&new_dir)?;
            for item in std::fs::read_dir(&legacy)? {
                let item = item?;
                if item.file_type()?.is_file() {
                    std::fs::copy(item.path(), new_dir.join(item.file_name()))?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => log::info!("Migrated legacy state from {} to {}", legacy.display(), new_dir.display()),
            Err(e) => {
                log::warn!("Could not migrate legacy state from {} to {}: {e}", legacy.display(), new_dir.display())
            }
        }
    }

    /// `state_dir_is_writable`: create the folder and write a probe file.
    pub fn state_dir_is_writable(&self) -> bool {
        let dir = self.state_dir();
        let probe = dir.join(".write_test");
        std::fs::create_dir_all(&dir).is_ok()
            && std::fs::write(&probe, b"").is_ok()
            && std::fs::remove_file(&probe).is_ok()
    }
}

pub fn is_portable_install() -> bool {
    StateLocation::current().is_portable_install()
}
pub fn default_state_root() -> PathBuf {
    StateLocation::current().default_state_root()
}
/// The directory holding runtime state and session control files.
pub fn state_dir() -> PathBuf {
    StateLocation::current().state_dir()
}
pub fn migrate_legacy_state_dir() {
    StateLocation::current().migrate_legacy_state_dir()
}
pub fn state_dir_is_writable() -> bool {
    StateLocation::current().state_dir_is_writable()
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn get<'a>(data: &'a Value, key: &str) -> Option<&'a Value> {
    data.as_object().and_then(|o| o.get(key))
}

/// `validate_model(data, path)`: `Err(message)` exactly as Python words it.
pub fn validate_model(data: &Value, path: &Path) -> Result<(), String> {
    let path = path.display();
    let mut data = data.clone();
    if let Some(plan) = get(&data, "body_plan") {
        let ok = plan.as_str().is_some_and(is_body_plan);
        if !ok {
            return Err(format!(
                "{path}: body_plan must be one of {} (got {})",
                BODY_PLAN_IDS.join(", "),
                py_repr(plan)
            ));
        }
        resolve_body_plan(&mut data);
    }
    let Some(obj) = data.as_object() else {
        return Err(format!(
            "{path}: missing required model field(s): id, display_name, base_size, default_personality, colors, legs"
        ));
    };
    let required = ["id", "display_name", "base_size", "default_personality", "colors", "legs"];
    let missing: Vec<&str> = required.iter().copied().filter(|k| !obj.contains_key(*k)).collect();
    if !missing.is_empty() {
        return Err(format!("{path}: missing required model field(s): {}", missing.join(", ")));
    }
    let legs = match obj.get("legs") {
        Some(Value::Array(legs)) if !legs.is_empty() => legs,
        _ => return Err(format!("{path}: legs must be a non-empty list")),
    };
    for (index, leg) in legs.iter().enumerate() {
        for key in ["name", "side", "gait_group", "attach_angle", "rest_angle", "reach", "upper_len", "lower_len"] {
            if !leg.as_object().is_some_and(|l| l.contains_key(key)) {
                return Err(format!("{path}: leg {index} missing {key}"));
            }
        }
    }
    let appearance = obj.get("appearance");
    if let Some(a) = appearance
        && !a.is_null()
        && !a.is_object()
    {
        return Err(format!("{path}: appearance must be an object"));
    }
    let chain = appearance.and_then(Value::as_object).and_then(|a| a.get("leg_chain"));
    if let Some(chain) = chain {
        if chain.is_null() {
            return Ok(());
        }
        let Some(chain) = chain.as_object() else {
            return Err(format!("{path}: appearance.leg_chain must be an object"));
        };
        if chain.get("enabled").is_some_and(py_truthy) {
            let count_value = chain.get("segment_count").cloned().unwrap_or(Value::from(4));
            let count_ok = is_number(&count_value) && count_value.as_f64().is_some_and(|c| c == 4.0 || c == 5.0);
            if !count_ok {
                return Err(format!("{path}: appearance.leg_chain.segment_count must be 4 or 5"));
            }
            let asset_ok = match chain.get("extra_segment_asset") {
                None => true,
                Some(Value::String(s)) => !s.trim().is_empty(),
                Some(_) => false,
            };
            if !asset_ok {
                return Err(format!("{path}: appearance.leg_chain.extra_segment_asset must be a non-empty string"));
            }
            let segment_count = py_int(&count_value).unwrap_or(4) as usize;
            for (key, expected) in [
                ("segment_scales", segment_count),
                ("segment_lengths", segment_count),
                ("joint_scales", segment_count - 1),
                ("bend_profile", segment_count - 1),
                ("joint_phase_offsets", segment_count - 1),
                ("bend_directions", segment_count - 1),
            ] {
                match chain.get(key) {
                    None | Some(Value::Null) => {}
                    Some(Value::Array(v)) if v.len() == expected => {}
                    Some(_) => {
                        return Err(format!("{path}: appearance.leg_chain.{key} must contain {expected} values"));
                    }
                }
            }
            if let Some(curve) = chain.get("leg_curve")
                && !is_number(curve)
            {
                return Err(format!("{path}: appearance.leg_chain.leg_curve must be a number of degrees"));
            }
        }
    }
    Ok(())
}

fn number_in(value: &Value, low: f64, high: f64) -> Option<bool> {
    if value.is_boolean() {
        return None;
    }
    py_float(value).map(|f| (low..=high).contains(&f))
}

/// `validate_personality(data, path)`.
pub fn validate_personality(data: &Value, path: &Path) -> Result<(), String> {
    let path = path.display();
    let empty = serde_json::Map::new();
    let obj = data.as_object().unwrap_or(&empty);
    let required = ["id", "display_name", "speed_multiplier", "reaction_radius", "boldness", "wander_frequency"];
    let missing: Vec<&str> = required.iter().copied().filter(|k| !obj.contains_key(*k)).collect();
    if !missing.is_empty() {
        return Err(format!("{path}: missing required personality field(s): {}", missing.join(", ")));
    }
    for key in ["skills", "behaviours", "abilities", "ability_bundles"] {
        let Some(values) = obj.get(key) else { continue };
        if key == "skills" && values.is_null() {
            continue;
        }
        let strings: Option<Vec<&str>> = values.as_array().and_then(|a| a.iter().map(Value::as_str).collect());
        let Some(strings) = strings else {
            return Err(format!("{path}: {key} must be a list of strings"));
        };
        let unknown: Vec<&str> = strings
            .iter()
            .copied()
            .filter(|v| {
                let id = v.trim().to_lowercase();
                match key {
                    "skills" => !is_skill(&id),
                    "behaviours" => !(is_skill(&id) && !is_ability_skill(&id)),
                    "abilities" => !is_ability_skill(&id),
                    _ => ability_bundle(&id).is_none(),
                }
            })
            .collect();
        if !unknown.is_empty() {
            return Err(if key == "ability_bundles" {
                format!("{path}: unknown ability bundle(s): {}", unknown.join(", "))
            } else {
                format!("{path}: unknown {key} id(s): {}", unknown.join(", "))
            });
        }
    }
    if let Some(v) = obj.get("include_common_abilities")
        && !v.is_boolean()
    {
        return Err(format!("{path}: include_common_abilities must be true or false"));
    }
    if let Some(modules) = obj.get("behaviour_modules") {
        let strings: Option<Vec<&str>> = modules.as_array().and_then(|a| a.iter().map(Value::as_str).collect());
        let Some(strings) = strings else {
            return Err(format!("{path}: behaviour_modules must be a list of strings"));
        };
        let unknown: Vec<&str> = strings
            .iter()
            .copied()
            .filter(|v| !BEHAVIOUR_MODULE_IDS.contains(&v.trim().to_lowercase().as_str()))
            .collect();
        if !unknown.is_empty() {
            return Err(format!("{path}: unknown behaviour_modules id(s): {}", unknown.join(", ")));
        }
    }
    if let Some(mp) = obj.get("movement_profile")
        && !is_movement_profile(&py_key(mp))
    {
        return Err(format!("{path}: unknown movement_profile: {}", crate::util::py_str(mp)));
    }
    if let Some(traits) = obj.get("temperament") {
        let Some(traits) = traits.as_object() else {
            return Err(format!("{path}: temperament must be an object of 0..10 values"));
        };
        for trait_id in TEMPERAMENT_TRAIT_IDS {
            let Some(value) = traits.get(trait_id) else {
                return Err(format!("{path}: temperament is missing {trait_id}"));
            };
            match number_in(value, 0.0, 10.0) {
                None => return Err(format!("{path}: temperament values must be numbers from 0 to 10")),
                Some(false) => {
                    return Err(format!("{path}: temperament value for {trait_id} must be between 0 and 10"));
                }
                Some(true) => {}
            }
        }
    }
    if let Some(scores) = obj.get("phase_scores") {
        let Some(scores) = scores.as_object() else {
            return Err(format!("{path}: phase_scores must be an object of phase ids to 0..10 values"));
        };
        for (phase_id, value) in scores {
            if !is_behaviour_phase(&phase_id.trim().to_lowercase()) {
                return Err(format!("{path}: unknown behaviour phase: {phase_id}"));
            }
            match number_in(value, 0.0, 10.0) {
                None => return Err(format!("{path}: phase_scores values must be numbers from 0 to 10")),
                Some(false) => return Err(format!("{path}: phase score for {phase_id} must be between 0 and 10")),
                Some(true) => {}
            }
        }
    }
    if let Some(value) = obj.get("phase_duration_multiplier")
        && number_in(value, 0.25, 2.5) != Some(true)
    {
        return Err(format!("{path}: phase_duration_multiplier must be between 0.25 and 2.5"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

fn read_json(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| format!("{}: {e}", path.display()))
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")))
        .collect()
}

/// Load, resolve, validate and type one model file (the body of the
/// `discover_models` loop).
pub fn load_model_file(path: &Path) -> Result<ModelDef, String> {
    let mut data = read_json(path)?;
    resolve_body_plan(&mut data);
    validate_model(&data, path)?;
    let mut model = ModelDef::from_value(data).map_err(|e| format!("{}: {e}", path.display()))?;
    resolve_model_body_plan(&mut model);
    model.path = Some(path.display().to_string());
    model.folder = path.parent().map(|p| p.display().to_string());
    Ok(model)
}

/// `discover_models(root)`: every valid model by id (first copy found wins,
/// editable folders before bundled ones) and the warnings for the rest.
pub fn discover_models(root: Option<&Path>) -> (OrderedMap<ModelDef>, Vec<String>) {
    let mut models = OrderedMap::new();
    let mut warnings = Vec::new();
    for models_dir in data_dirs("models", root) {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&models_dir)
            .map(|it| it.filter_map(Result::ok).map(|e| e.path().join("model.json")).filter(|p| p.is_file()).collect())
            .unwrap_or_default();
        sort_newest_first(&mut paths);
        for path in paths {
            match load_model_file(&path) {
                Ok(model) => {
                    if !models.contains_key(&model.id) {
                        models.insert(model.id.clone(), model);
                    }
                }
                Err(message) => warnings.push(message),
            }
        }
    }
    (models, warnings)
}

/// Load, validate, annotate and type one personality file.
pub fn load_personality_file(path: &Path) -> Result<Personality, String> {
    let data = read_json(path)?;
    validate_personality(&data, path)?;
    let personality = Personality::from_value(data).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut personality = annotate_personality(&personality);
    personality.path = Some(path.display().to_string());
    Ok(personality)
}

/// `discover_personalities(root)`: files first (sorted by name), then the
/// built-in temperaments that no file shadows.
pub fn discover_personalities(root: Option<&Path>) -> (OrderedMap<Personality>, Vec<String>) {
    let mut personalities = OrderedMap::new();
    let mut warnings = Vec::new();
    for dir in data_dirs("personalities", root) {
        let mut paths = json_files(&dir);
        // WindowsPath ordering is case-insensitive.
        paths.sort_by_key(|p| casefold(&p.to_string_lossy()));
        for path in paths {
            match load_personality_file(&path) {
                Ok(p) => {
                    if !personalities.contains_key(&p.id) {
                        personalities.insert(p.id.clone(), p);
                    }
                }
                Err(message) => warnings.push(message),
            }
        }
    }
    for (id, personality) in canonical_personality_definitions().into_entries() {
        if personalities.contains_key(&id) {
            continue;
        }
        let mut data = annotate_personality(&personality);
        data.path = Some("<built-in temperament catalog>".into());
        personalities.insert(id, data);
    }
    (personalities, warnings)
}

/// `discover_presets(root)`: every preset file, newest first, one per
/// case-folded stem (a user's copy shadows the shipped one).
pub fn discover_presets(root: Option<&Path>) -> Vec<PathBuf> {
    let writable = root.map(Path::to_path_buf).unwrap_or_else(app_root).join("presets");
    let _ = std::fs::create_dir_all(&writable);
    let mut presets = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in data_dirs("presets", root) {
        let mut paths = json_files(&dir);
        sort_newest_first(&mut paths);
        for path in paths {
            let stem = path.file_stem().map(|s| casefold(&s.to_string_lossy())).unwrap_or_default();
            if seen.insert(stem) {
                presets.push(path);
            }
        }
    }
    presets
}

/// The checks `tools/run_all_checks.py` runs over the shipped data
/// (`validate_model.py` on every `models/*/model.json`,
/// `validate_preset.py` on every `presets/*.json`). Returns one message per
/// invalid file; empty means everything passed.
pub fn run_data_checks(root: &Path) -> Vec<String> {
    let mut failures = Vec::new();
    let mut models: Vec<PathBuf> = std::fs::read_dir(root.join("models"))
        .map(|it| it.filter_map(Result::ok).map(|e| e.path().join("model.json")).filter(|p| p.is_file()).collect())
        .unwrap_or_default();
    models.sort();
    let mut presets = json_files(&root.join("presets"));
    presets.sort();
    if models.is_empty() || presets.is_empty() {
        failures.push("data discovery: no models or no presets found".to_string());
        return failures;
    }
    for path in models {
        match read_json(&path).and_then(|data| validate_model(&data, &path)) {
            Ok(()) => {}
            Err(message) => failures.push(format!("invalid model: {message}")),
        }
    }
    for path in presets {
        match read_json(&path).and_then(|data| crate::preset_io::validate_preset(&data)) {
            Ok(()) => {}
            Err(message) => failures.push(format!("invalid preset: {}: {message}", path.display())),
        }
    }
    failures
}
