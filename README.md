# Desktop Bug Companion

A Windows desktop overlay that puts small procedural spiders on top of your real
desktop. They walk with planted-foot IK, react to the cursor, hunt flies, spin
webs, form teams, build bases and fight rivals. Empty overlay space stays
click-through; spider pixels can be grabbed and thrown.

## Requirements

- Windows 10/11 (click-through and cursor behaviour are Windows-only).
- Python 3.10+ and `requirements.txt` (PyQt5, PyInstaller) for development.

## Setup and run

Run everything from the repository root; the app loads `models/`,
`personalities/` and `presets/` relative to it.

```powershell
python -m venv .venv
.\.venv\Scripts\Activate.ps1
python -m pip install -r requirements.txt
.\run_dev.bat
```

`run_dev.bat` sets `PYTHONPATH` and prefers `.venv`. To start the settings
window or the overlay yourself:

```powershell
$env:PYTHONPATH = "$PWD\src"
.\.venv\Scripts\python.exe -m desktop_bug.app.config_ui
.\.venv\Scripts\python.exe -m desktop_bug.app.engine --preset presets\default.json
```

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
moves, Shift sprints (draining energy), Space jumps, left-click fires a web and
right-click bites, K opens the skill tree and Esc pauses. The mouse only aims:
within a cone in front of the spider (90 degrees by default), and the spider
turns by walking. **Controls...** on the Adventure page, or Settings in the
pause menu, lists what every button does, changes the aim cone, and rebinds any
action to another key or mouse button (saved in `state\controls.json`; Esc
always pauses). The status panel shows health,
stamina, silk ammunition and action cooldowns; drag its header to move it.
Silk fires visibly along your aim even on a miss. Stand at home to refill;
capturing the Silk loom raises capacity to twelve and refills faster.
Companion commands are **1 Follow**, **2 Defend here**, **3 Attack target**
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
`state\logs\desktop-bug.log` (run with `--verbose` for more detail).

- `portable.txt` beside the project or `.exe` keeps state there instead.
- `DESKTOP_BUG_STATE_DIR` overrides the location entirely (the tests use it).

## Troubleshooting

- **Overlay blank, black, or swallowing clicks:** the overlay draws with OpenGL
  by default. Set `DESKTOP_BUG_GL=0` to use the CPU path.
- **Black rectangle over the desktop:** some screen recorders, remote desktops
  and GPU overlay tools break per-pixel transparency; try a normal local
  session.
- To quit: **Stop overlay** in the settings window, or **Quit overlay** in the
  tray menu.

## Content

Models (`models/<id>/model.json`), personalities (`personalities/*.json`) and
presets (`presets/*.json`) are auto-discovered; no code changes are needed.
A model needs `id`, `display_name`, `base_size`, `default_personality`,
`colors` and `legs` (or a `body_plan`); its look is tuned in its `appearance`
block. A personality needs `id`, `display_name`, `speed_multiplier`,
`reaction_radius`, `boldness`, `wander_frequency`, and a `temperament` of six
0-10 traits. Validate with:

```powershell
python tools\validate_model.py models\spider\model.json
python tools\validate_preset.py presets\default.json
```

## Development

```powershell
python tools\run_all_checks.py      # compile, validate content, pytest, ruff -- what CI runs
python -m pytest tests\test_tarantula.py -v
```

The carved-wood interface art in `assets/ui/` (mode pictures, wood textures,
card frame) is generated: `python tools\generate_ui_art.py` redraws it,
burning the current tarantula model into the pictures.

Tests are headless: `tests/conftest.py` forces Qt's offscreen platform and a
private state folder, so a run never touches your saved spiders.

Code lives in `src/desktop_bug/`: `app/` (settings window, overlay engine,
Win32 glue), `creature/` (behaviour, kinematics, rendering), `manager/`
(the colony, input, persistence), `world/` (flies, webs, cages, bases and
jobs), `content/` (discovery, body plans, skills), `state/` (progression and
the saved-state format).

## Build and release

`build_exe.bat` builds `dist\DesktopBugCompanion.exe` from
`DesktopBugCompanion.spec`, whose entry point is `launcher.py`. It is one
file with the content bundled; a `models`, `personalities` or `presets` folder
beside the `.exe` overrides it. The
packaged exe accepts `--engine --preset <file>` as well.

CI (`.github/workflows/ci.yml`) runs `tools/run_all_checks.py` and a build
smoke test on pushes and pull requests to `main`. Pushing a tag such as
`v1.0.0` runs `release-windows.yml`, which refuses a tag that disagrees with
`__version__` in `src/desktop_bug/__init__.py` and publishes the `.exe` to a
GitHub Release.
