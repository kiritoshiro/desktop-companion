# Take back the desktop

Implemented on `codex/adventure-territory-mission`, based on freshly fetched
`origin/main` 4208e7c. Launch with the existing Adventure button.

## Play

Capture Food or Silk by clearing the defenders and standing within 92 pixels
of the entrance for four seconds. A foe within 125 pixels contests capture.
Leaving gradually drains incomplete capture progress. The first outpost triggers
a two-spider counterattack from Thorn nest with a four-second warning.

Seal the Hatchery to cancel its remaining queued reinforcements. It otherwise
has six reserves, one hunter every 24 seconds after an initial 28-second delay,
with a three-second emergence warning. Spawns wait for room and never appear
within 150 pixels of the player. Ordinary waves stop at eight living spiders;
the global creation guard caps every spawn at ten including hero and ally.

After one outpost, the Hatchery and the counterattack are cleared, Thorn nest
warns before releasing its guardian. Defeat it, then capture the nest to win.
Player death ends the raid; Scout death does not. Esc restarts or exits.

Home supplies slow healing and one silk charge per second. Food gives faster
healing. Both have a finite healing supply. Silk gives twelve-charge capacity,
an immediate refill and three charges per second while nearby. A shot costs one
charge and twelve stamina, with a 1.2-second cooldown. Misses still spend silk.

WASD move; Shift sprints; Space jumps; left mouse shoots; right mouse bites;
K opens skills. 1 orders Scout to follow, 2 to defend its current position,
3 to attack the pointed enemy. HUD buttons use the last pointed enemy for Attack.
All commands appear in Controls and can be rebound. Normal defenders leave the
home refuge alone; warned reinforcement raiders can pursue you there.

## Architecture and cost

`app/mission.py` owns the session, objective state and bounded spawn queue.
`MissionActor` uses the existing movement/leg code; combat decisions are staggered
at roughly 7 Hz. Guard bites, Weaver silk, Hunter pounces and the stronger guardian
all wind up visibly. Silk interrupts a wind-up; jumps avoid silk and melee hits.
Only two enemy attacks can wind up simultaneously.

The mission bypasses ambient colony jobs, fly spawning and desktop-cover AI.
`app/mission_ui.py` caches at most ten transparent building images (five building
types, two ownership states). Dynamic capture and spawn labels draw separately.
Corpses are capped at four; silk projectiles expire at their range.
No larger spider-count performance claim has been made.

Buildings sit on the primary monitor above the default HUD; smaller screens use
smaller artwork. This is a single-monitor arena, even on a multi-monitor desktop.
No save/resume checkpoint exists inside a raid. Only victory banks hero progression
into a separate `adventure-hero.json`; mission deaths, teams and structures never
write the Companion `creatures.json`. Preset pushes are ignored during a raid.

## Verification

`tests/test_mission.py` exercises real creatures and projectiles: capture contest,
spawn caps and safety, warnings/reserves, commands, projectile misses/hits, pounces,
interrupts, victory/defeat/restart and save isolation. Existing Adventure tests
cover player actions and controls. `tools/render_mission_preview.py` renders actual
buildings, creatures and HUD at 1600x1000 and 900x700 without opening a game window.
On Windows it uses the native Qt font backend for readable preview text.

Manual acceptance: launch Adventure, shoot into empty space and at a foe, exhaust
and refill silk, use all three Scout orders, capture both outposts in either order,
seal the Hatchery, dodge the guardian, win, retry, and verify Companion is unchanged.
Real desktop frame pacing and combat difficulty still need playtesting.

## Files changed for this implementation

- `src/desktop_bug/app/mission.py`: mission state, encounter actors, objectives,
  reinforcements, resource sites, companion orders, victory progression.
- `src/desktop_bug/app/mission_ui.py`: cached building artwork, objective panel,
  ownership/capture/spawn labels, companion buttons.
- `src/desktop_bug/world/aimed_silk.py`: visible aimed projectile and swept collision.
- `src/desktop_bug/app/adventure.py`: silk capacity, shooting feedback and shared movement.
- `src/desktop_bug/app/adventure_ui.py`: silk meter and mission pause choices.
- `src/desktop_bug/app/controls.py`: companion bindings and safe saved-binding migration.
- `src/desktop_bug/app/controls_ui.py`: scrollable binding rows.
- `src/desktop_bug/app/engine.py`: mission lifecycle, rendering and input integration.
- `src/desktop_bug/app/mode_menu.py`: launch-page mission instructions.
- `src/desktop_bug/app/window_placement.py`: detach the app-wide Python event
  filter before native Qt shutdown, avoiding a Windows access violation.
- `src/desktop_bug/manager/persistence.py`: isolate mission writes from Companion saves.
- `tests/test_mission.py`: mission and integration regressions.
- `tests/test_wood_theme.py`: update the HUD fixture for silk fields.
- `tools/render_mission_preview.py`: reproducible native-font visual previews.
- `README.md` and this document: gameplay and development documentation.

Generated evidence lives in `reports/mission-preview-1600.png`,
`reports/mission-preview-900.png`, `reports/mission-full-tests.txt`, and
`reports/baseline-tests.txt`. The pre-existing changes in `presets/default.json`
and untracked `assign.json` were preserved.

## Windows shutdown regression found during verification

The first combined run passed all test assertions but exited with Windows access
violation `0xC0000005` during interpreter teardown. An unchanged archive of latest
main exited cleanly. Grouped UI tests narrowed the interaction to the global
window-centering event filter. Detaching that filter both on `aboutToQuit` and
at Python exit fixed the exact 151-test reproducer (exit 0).
The mission's final full check run is recorded in `reports/mission-final-checks.txt`.

The external free-model advisory review was blocked by automatic approval because
it would export project source. No source was sent; local code review, targeted
regressions, rendering checks and the full suite were used instead.

## Final verification outcome

`tools/run_all_checks.py`: **5/5 passed**, process exit 0, 84 test modules,
49 models and 5 presets. The suite collects 859 tests. The targeted mission
suite passes all 20 tests; the native-shutdown reproduction passes all 151 UI tests
and exits cleanly. Ruff, compilation and `git diff --check` pass.
Use `PYTEST_ADDOPTS=--basetemp=.test-tmp-final-checks` on this machine to avoid the
pre-existing default pytest temp-directory permission error.

Local branch only; no commit, push or merge. Real desktop input, game balance and
frame pacing still need a playthrough. Obsidian hub and work log are updated.
