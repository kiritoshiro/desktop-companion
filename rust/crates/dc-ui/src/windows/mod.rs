//! One struct per Qt window. Each has `fn ui(&mut self, ctx) -> Vec<UiAction>`
//! and a `show(&mut self, ui)` for drawing inside a parent.

pub mod admin;
pub mod adventure;
pub mod armour;
pub mod character;
pub mod config;
pub mod controls;
pub mod inspector;
pub mod map_editor;
pub mod menu;
pub mod mode_menu;
pub mod prompt;

pub use admin::AdminDialog;
pub use adventure::{AdventureSettingsDialog, PauseDialog};
pub use armour::{InventoryBag, SpiderDoll};
pub use character::{CharacterDialog, CharacterSheet, CharacterWindow, PartyDialog};
pub use config::ConfigWindow;
pub use controls::{ControlsDialog, ControlsEditor};
pub use inspector::CreatureInspector;
pub use map_editor::MapEditor;
pub use menu::{MenuEntry, MenuIcon, MenuOutput, WoodMenu};
pub use mode_menu::ModeShell;
pub use prompt::{PromptAnswer, TextPrompt};
