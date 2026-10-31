# Desktop Bug Companion

A Windows desktop overlay that puts small procedural spiders on top of your real
desktop. They walk with planted-foot IK, react to the cursor, hunt flies, spin
webs, form teams, build bases and fight rivals. Empty overlay space stays
click-through; spider pixels can be grabbed and thrown.

The app is written in Rust (`rust/`). The original Python/PyQt5 app is kept on
the [`legacy`](https://github.com/kiritoshiro/desktop-companion/tree/legacy)
branch.

## Requirements

- Windows 10/11 (click-through and cursor behaviour are Windows-only).
- To build: a Rust toolchain from [rustup](https://rustup.rs/);
  `rust/rust-toolchain.toml` picks the channel.

## Setup and run

Download `DesktopBugCompanion-<version>-windows-x64.zip` from the
[Releases](https://github.com/kiritoshiro/desktop-companion/releases), unzip it
and run `DesktopBugCompanion.exe`. Keep the `models`, `personalities`, `presets`
and `assets` folders beside the exe; the app loads them from there.

From a checkout, in `rust/`:

```powershell
cargo run --release -p dc-app                                             # settings window
cargo run --release -p dc-app -- --engine --preset presets\default.json  # overlay only
```

The overlay also takes `--mode adventure`, `--seed N`, `--verbose`,
`--software`, `--seconds S` (quit after that long) and `--snapshot out.png`
(save a screenshot of the primary monitor 3 s in).

## Using it

**Settings window.** Add slots and pick, per slot: category (Bug, Segmented,
Jumper, Tarantula), skin and colours, temperament, job (Hunter, Builder, Guard,
Scout, Web tender), abilities, team, count and size. **Save and launch overlay**
starts it; saving while it runs applies changes live. **Stop overlay** asks it
to save and quit. **Reset saved progress...** forgets every spider's level, XP
and name and removes all bases (presets, settings, cages and webs are kept).

**On the desktop.**
- Left-drag a spider to pick it up; release to throw it.
- Right-click a spider to name it, toggle its abilities, or inspect its level,
  talents, armour, team and relations. Right-click a base to remove it.
- Tray menu: randomize, size, mood, social play, flies, cages, the
  "always show" switches (names, levels, health and stamina are on by default;
  XP is a thin line under the level, on request), and **Let spiders web-trap the
  mouse** (the master switch for anything that moves your pointer).
- Wiggle the mouse to break free of a silk trap. Swipe across a web to tear it.

**Teams and bases.** Spiders on one team are friends; **Rivals** is hostile to
every other team, and the Teams panel sets any pair to friend, ignore or foe.
Foes fight, and a beaten spider dies. Builders raise a team base of dirt
mounds; Guards walk a line through it and intercept foes; Scouts, Web tenders
and Hunters work around it. A spider with no team looks after its own base.

Teams are named and coloured in a preset, and pairs set in `team_relations`:

```json
"teams": { "pack_a": { "name": "Home colony", "color": "#4fa3d1" } },
"settings": { "team_relations": { "pack_a": { "rivals": "foe" } } }
```

## Modes and Adventure control

The settings window opens on a three-mode start screen:

- **Companion** contains the existing preset editor and desktop overlay controls.
- **Adventure / Skirmish** launches **Take back the desktop**, a territory raid
  with one controlled spider, a Scout companion and a maximum of ten spiders.
- **Strategy** previews the planned colony command mode.

Adventure uses the first living spider's appearance and initial progression;
subsequent victories use the separately saved Adventure hero. Each raid starts
with full health, stamina and eight silk charges. By default WASD
moves, Shift sprints (draining energy), Space jumps, Q fires a web and
holding F repeatedly bites, K opens the skill tree and Esc pauses. The mouse only aims:
within a cone in front of the spider (90 degrees by default), and the spider
turns by walking. **Controls...** on the Adventure page, or Settings in the
pause menu, lists what every button does, changes the aim cone, and rebinds any
action to another key or mouse button (saved in `state\controls.json`; Esc
always pauses). Check **Two players · one keyboard** to add a second saved hero:
Arrows move, J attacks (hold), U webs, I jumps, O sprints, P opens gear. Both
players aim forward, share one bag, and have separate armour/skill windows
opened together by K or P. Close both to resume. See [local co-op details](docs/adventure-mission.md#local-two-player-mode).
The status panel shows health,
stamina, silk ammunition and action cooldowns; drag its header to move it.
Silk fires visibly even on a miss: from a key, straight where your spider
faces; from a mouse button, exactly at the pointer. Stand at home to refill;
capturing the Silk loom raises capacity to twelve and refills faster.
Companions walk at your pace and run, on their own stamina, to catch up or
when you run. Companion commands are **1 Follow**, **2 Defend here**, **3 Attack target**
(point at an enemy first), also available as HUD buttons and rebindable controls.
Adventure captures mouse input while active; **Esc** pauses, opens settings,
restarts or exits the raid. Spiders at the same level use the same baseline size
and name style, growing slightly with each level.

Capture Food or Silk, seal the Hatchery, survive the counterattack, defeat the
Thorn guardian and hold the final nest. Clear defenders and stand by an entrance
for four seconds to capture it. Orange attack rings warn of incoming strikes.
Victory banks XP and equipment in `adventure-hero.json`; death ends the raid.
A restarted or abandoned raid loses unbanked rewards. Companion saves remain
separate. The arena uses the primary monitor. Strategy orders are future work.
See [mission rules and verification](docs/adventure-mission.md).

## Saved state

Progress is saved to `%LOCALAPPDATA%\DesktopBugCompanion\state\creatures.json`
(levels, XP, talents, equipment, names, teams, relations, bases, and the
cages, webs and nests you placed). Your own presets go to `state\presets\` and
take precedence over the shipped ones. Logs are in
`state\logs\desktop-bug.log` (run with `--verbose` for more detail). The Python
app on `legacy` uses the same folder and format.

- `portable.txt` beside the project or `.exe` keeps state there instead.
- `DESKTOP_BUG_STATE_DIR` overrides the location entirely (the tests use it).

## Troubleshooting

- **Overlay blank, black, or swallowing clicks:** the overlay draws on the GPU
  (DirectX 12 through DirectComposition) and falls back to a CPU path
  (`UpdateLayeredWindow`) when that fails. `--software` forces the CPU path.
- **Black rectangle over the desktop:** some screen recorders, remote desktops
  and GPU overlay tools break per-pixel transparency; try a normal local
  session.
- **Frame rate:** 60 FPS, or lower with `DESKTOP_BUG_FPS` (10–60). It drops to
  30 on battery, and to 10 while a fullscreen window covers every monitor.
- To quit: **Stop overlay** in the settings window, or **Quit overlay** in the
  tray menu.

## Content

Models (`models/<id>/model.json`), personalities (`personalities/*.json`) and
presets (`presets/*.json`) are auto-discovered; the app needs no code changes.
A model needs `id`, `display_name`, `base_size`, `default_personality`,
`colors` and `legs` (or a `body_plan`); its look is tuned in its `appearance`
block. A personality needs `id`, `display_name`, `speed_multiplier`,
`reaction_radius`, `boldness`, `wander_frequency`, and a `temperament` of six
0-10 traits. The tests load every shipped model, personality and preset, so
`cargo test -p dc-content` (in `rust/`) checks a new one. They also count the
shipped files: update the counts in `rust/crates/dc-content/tests/ported.rs`
when you add or remove one.

## Development

In `rust/`:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked   # what CI runs
cargo test -p dc-kinematics       # one crate
```

Tests are headless and write only to temporary folders, so a run never touches
your saved spiders. `rust/PORTING.md` lists the crates and what each one
covers; each crate's `PORT_STATUS.md` notes where it differs from the Python
app.

The carved-wood interface art in `assets/ui/` is generated:
`cargo run --release -p dc-art --example generate_ui_art ../assets/ui` redraws
the textures, card frame and menu emblems. The mode pictures and app icon are
still drawn by `tools/generate_ui_art.py` on `legacy`.

## Build and release

`pwsh rust\package.ps1` builds a release and packs
`rust\dist\DesktopBugCompanion\` (the exe beside `models`, `personalities`,
`presets` and `assets`) and `rust\dist\DesktopBugCompanion-<version>-windows-x64.zip`.
Release builds have no console window.

CI (`.github/workflows/ci.yml`) runs formatting, clippy and the tests on pushes
to `main` and on every pull request. Pushing a tag such as `v1.0.0` runs
`release-windows.yml`, which refuses a tag that disagrees with `version` in
`rust/Cargo.toml`, runs the tests and publishes the zip to a GitHub Release.
