//! What the windows ask the app layer to do. dc-ui never spawns processes or
//! writes files; every button that did so in the Qt windows yields one of
//! these instead, and the app wires it to dc-app / dc-adventure / dc-state.

use serde_json::Value;

use crate::model::ControlSettings;

/// Which overlay to launch (`launch_engine(mode=...)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayMode {
    Companion,
    Adventure,
}

impl OverlayMode {
    pub fn as_str(self) -> &'static str {
        match self {
            OverlayMode::Companion => "companion",
            OverlayMode::Adventure => "adventure",
        }
    }
}

/// One request from a window.
#[derive(Clone, Debug, PartialEq)]
pub enum UiAction {
    // -- settings window (config_ui) ------------------------------------------
    /// "Save": write this preset (`save_preset`), and apply it live if the
    /// overlay runs from it.
    SavePreset {
        preset: Value,
    },
    /// "Load": read the preset at `path` and hand it back with
    /// `ConfigWindow::load_preset`.
    LoadPreset {
        path: String,
    },
    /// "Refresh library": rediscover models, personalities and presets.
    RefreshLibrary,
    /// "Save and launch overlay": save to the user's presets folder and start
    /// the overlay -- or, when one is running, apply the preset live.
    LaunchOverlay {
        mode: OverlayMode,
        preset: Value,
    },
    /// "Stop overlay": ask it to save and quit.
    StopOverlay,
    /// "Reset saved progress...", confirmed.
    ResetProgress,
    /// "Open project folder".
    OpenProjectFolder,

    // -- mode menu ----------------------------------------------------------
    /// The page shown changed (companion / skirmish / strategy / home).
    ShowMode(String),
    /// Left the Adventure page: close an Adventure overlay (`leave_adventure`).
    LeaveAdventure,
    /// Play a map: remember it as `selected_map`, then start Adventure.
    PlayMap {
        map_id: String,
    },
    SetTwoPlayer(bool),
    SetAllScreens(bool),
    SetScreen {
        name: String,
        on: bool,
    },
    OpenCharacter,
    OpenControls,
    OpenMapEditor {
        map_id: Option<String>,
    },
    OpenAdmin,

    // -- controls editor ------------------------------------------------------
    /// Every change is saved at once (`save_controls`).
    SaveControls(ControlSettings),

    // -- character, armour and inspector ------------------------------------------
    RenameSpider {
        who: String,
        name: String,
    },
    UnlockSkill {
        who: String,
        ability_id: String,
    },
    Equip {
        who: String,
        item_id: String,
    },
    Unequip {
        who: String,
        slot: String,
    },
    UpgradeItem {
        item_id: String,
    },
    SellSpares {
        item_id: String,
        count: u32,
    },
    /// Inspector: "Level and XP beside its name".
    SetLevelPin(bool),
    /// Inspector: "Health bar (without the name)".
    SetHealthPin(bool),
    /// Inspector: move the spider to a team (an id).
    SetTeam(String),
    /// Inspector: how it feels about another spider.
    SetRelation {
        other: String,
        relation: String,
    },
    /// Inspector: "Add to bag".
    AddInventoryItem(String),

    // -- adventure dialogs ------------------------------------------------
    /// The pause menu's choice (`resume`, `skills`, `save`, `settings`,
    /// `release`, `restart`, `exit`).
    PauseChoice(String),
    SetFrameRate(u32),

    // -- admin --------------------------------------------------------------
    SetAdmin(bool),
    ResetAdventureProgress,
    GiveAmber(i64),
    GiveAllArmour,
    UnlockAllCompanions,
    SetHeroLevel(u32),

    // -- map editor ------------------------------------------------------------
    /// Save the map (`custom_maps.save_map`); answer with `MapEditor::saved`.
    SaveMap(Value),
    DeleteMap(String),
    /// Open another saved map in the editor; answer with `MapEditor::load`.
    OpenMap(String),
    /// "Save and play".
    SaveAndPlayMap(Value),

    /// The window's Close / Done.
    CloseWindow,
}
