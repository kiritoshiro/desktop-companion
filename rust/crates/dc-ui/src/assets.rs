//! The PNGs in `assets/ui/` as egui textures (wood_theme's `asset_path`,
//! `texture`, `mode_art` and `menu_icon`).
//!
//! Loading is lazy and cached per [`egui::Context`]. A missing or unreadable
//! file is remembered as missing, and every caller then falls back to plain
//! colours, as the Python did for a stripped build or a moved checkout.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use egui::{ColorImage, Context, Id, TextureHandle, TextureOptions, TextureWrapMode};

/// The 29 carved emblems shipped as `assets/ui/menu_<name>.png`.
pub const MENU_ICONS: &[&str] = &[
    "adventure",
    "appearance",
    "armor",
    "base",
    "behavior",
    "cage",
    "companion",
    "creatures",
    "flies",
    "health",
    "help",
    "inspect",
    "interaction",
    "labels",
    "launch",
    "move",
    "name",
    "performance",
    "pin",
    "preset",
    "quit",
    "randomize",
    "remove",
    "rename",
    "skills",
    "stamina",
    "strategy",
    "teams",
    "xp",
];

/// Every other UI picture.
pub const OTHER_ASSETS: &[&str] = &[
    "app_icon.png",
    "card_frame.png",
    "mode_adventure.png",
    "mode_companion.png",
    "mode_strategy.png",
    "wood_dark_tile.png",
    "wood_tile.png",
];

/// A decoded picture, before it becomes a texture (also used for window icons).
#[derive(Clone)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Decode a PNG file.
pub fn decode_png(path: &Path) -> Option<DecodedImage> {
    let bytes = std::fs::read(path).ok()?;
    decode_png_bytes(&bytes)
}

/// Decode PNG bytes.
pub fn decode_png_bytes(bytes: &[u8]) -> Option<DecodedImage> {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?;
    let rgba = image.to_rgba8();
    Some(DecodedImage { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() })
}

fn has_ui_assets(dir: &Path) -> bool {
    dir.join("wood_tile.png").is_file() || dir.join("menu_preset.png").is_file()
}

/// Where `assets/ui` is: `DC_ASSETS_DIR`, beside the executable or any of its
/// parents, the working directory or its parents, then the source checkout
/// this crate was built from.
pub fn find_assets_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("DC_ASSETS_DIR") {
        let dir = PathBuf::from(dir);
        let ui = if dir.ends_with("ui") { dir.clone() } else { dir.join("ui") };
        if has_ui_assets(&ui) {
            return Some(ui);
        }
        if has_ui_assets(&dir) {
            return Some(dir);
        }
    }
    let mut starts: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        starts.push(parent.to_path_buf());
    }
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    starts.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    for start in starts {
        for dir in start.ancestors() {
            let candidate = dir.join("assets").join("ui");
            if has_ui_assets(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

#[derive(Default)]
struct Store {
    dir: Option<Option<PathBuf>>,
    textures: HashMap<String, Option<TextureHandle>>,
}

/// The per-context texture cache.
#[derive(Clone, Default)]
pub struct UiAssets {
    store: Arc<Mutex<Store>>,
}

impl UiAssets {
    /// The cache of this context, created on first use.
    pub fn of(ctx: &Context) -> UiAssets {
        ctx.data_mut(|d| d.get_temp_mut_or_default::<UiAssets>(Id::unique("dc-ui-assets")).clone())
    }

    /// Point this context at a particular assets folder (tests, packaging).
    pub fn set_dir(ctx: &Context, dir: Option<PathBuf>) {
        let assets = Self::of(ctx);
        let mut store = assets.store.lock().unwrap_or_else(|e| e.into_inner());
        store.dir = Some(dir);
        store.textures.clear();
    }

    /// The assets folder in use, if any.
    pub fn dir(ctx: &Context) -> Option<PathBuf> {
        let assets = Self::of(ctx);
        let mut store = assets.store.lock().unwrap_or_else(|e| e.into_inner());
        store.dir.get_or_insert_with(find_assets_dir).clone()
    }

    /// `asset_path`: the file, if it exists.
    pub fn path(ctx: &Context, name: &str) -> Option<PathBuf> {
        let path = Self::dir(ctx)?.join(name);
        path.is_file().then_some(path)
    }

    /// `texture`: a picture by file name, or None.
    pub fn texture(ctx: &Context, name: &str) -> Option<TextureHandle> {
        let assets = Self::of(ctx);
        if let Some(found) = assets.store.lock().unwrap_or_else(|e| e.into_inner()).textures.get(name) {
            return found.clone();
        }
        let loaded = Self::path(ctx, name).and_then(|path| decode_png(&path)).map(|image| {
            let options = if name.contains("tile") {
                TextureOptions { wrap_mode: TextureWrapMode::Repeat, ..TextureOptions::LINEAR }
            } else {
                TextureOptions::LINEAR
            };
            ctx.load_texture(
                format!("dc-ui/{name}"),
                ColorImage::from_rgba_unmultiplied([image.width as usize, image.height as usize], &image.rgba),
                options,
            )
        });
        if loaded.is_none() {
            log::debug!("UI asset {name} not found; using the plain fallback");
        }
        assets.store.lock().unwrap_or_else(|e| e.into_inner()).textures.insert(name.to_owned(), loaded.clone());
        loaded
    }

    /// `menu_icon`: a carved emblem (`menu_<name>.png`), or None for the
    /// text-only fallback.
    pub fn menu_icon(ctx: &Context, name: &str) -> Option<TextureHandle> {
        Self::texture(ctx, &format!("menu_{name}.png"))
    }

    /// `mode_art`: the picture for a mode card.
    pub fn mode_art(ctx: &Context, kind: &str) -> Option<TextureHandle> {
        crate::theme::mode_art_name(kind).and_then(|name| Self::texture(ctx, name))
    }

    /// The app icon as RGBA, for a window icon.
    pub fn app_icon_rgba() -> Option<DecodedImage> {
        find_assets_dir().and_then(|dir| decode_png(&dir.join("app_icon.png")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_asset_is_found_and_decodes() {
        let dir = find_assets_dir().expect("assets/ui beside the checkout");
        for name in MENU_ICONS {
            let path = dir.join(format!("menu_{name}.png"));
            let image = decode_png(&path).unwrap_or_else(|| panic!("{path:?} decodes"));
            assert!(image.width >= 32 && image.height >= 32, "{name} is readable");
        }
        for name in OTHER_ASSETS {
            assert!(decode_png(&dir.join(name)).is_some(), "{name} decodes");
        }
        assert_eq!(MENU_ICONS.len(), 29);
    }

    #[test]
    fn missing_art_falls_back_to_none() {
        let ctx = Context::default();
        UiAssets::set_dir(&ctx, Some(PathBuf::from("Z:/definitely/not/here")));
        assert!(UiAssets::texture(&ctx, "wood_tile.png").is_none());
        assert!(UiAssets::menu_icon(&ctx, "preset").is_none());
    }

    #[test]
    fn textures_load_into_a_context() {
        let ctx = Context::default();
        let icon = UiAssets::menu_icon(&ctx, "preset").expect("emblem");
        assert!(icon.size()[0] >= 32);
        assert!(UiAssets::mode_art(&ctx, "skirmish").is_some());
        assert!(UiAssets::mode_art(&ctx, "nothing").is_none());
    }
}
