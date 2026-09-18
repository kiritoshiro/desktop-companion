from __future__ import annotations

import math
import json
import random
import sys
from pathlib import Path
from typing import List, Optional, Tuple

from .creature import Creature, GAIT_LABELS, normalize_gait_style
from .creature.constants import HUNT_BUSY_STATES
from .cage import Cage
from .webs import WebWorld
from .mouse_webs import MouseWebWorld
from .flies import FlyWorld
from .discovery import app_root, discover_models, discover_personalities, state_dir
from .logging_setup import get_logger
from .preset_io import load_preset
from .skills import (
    DEFAULT_SKILL_IDS,
    normalize_skill_ids,
    unknown_skill_ids,
    SKILL_BY_ID,
    skills_with_default_abilities,
    skills_with_selected_abilities,
)
from .desktop_environment import DesktopSurface
from .math_utils import distance
from .progression import RELATIONS, normalize_team_stances
from .teams import normalize_teams, teams_payload
from .jobs import BaseWorld, FLY_CATCH_RESOURCE_AMOUNT, job_ability_ids, normalize_job_id
from .personality_profiles import COMPACT_TEMPERAMENT_IDS
from .profiling import get_profiler
from .runtime_state import (
    build_payload,
    evict,
    load_payload,
    next_launch,
    normalize_namespace,
    stamp_seen,
)


log = get_logger("manager")

RANDOM_MODEL_ID = "__random_model__"
RANDOM_PERSONALITY_ID = "__random_personality__"
FEED_XP_REWARD = 110
# How long a background progression change may sit unsaved. Short enough that a
# crash loses at most a few seconds of XP, long enough that a hungry colony does
# not rewrite the state file every frame.
RUNTIME_STATE_FLUSH_SECONDS = 5.0


def _clamp(value: float, low: float, high: float) -> float:
    return max(low, min(high, value))


class CreatureManager:
    def __init__(self, preset_path: Path, screen_w: int, screen_h: int, seed: int | None = None):
        self.root = app_root()
        # A caller that wants a replayable run passes a seed, used both for
        # this manager's own random choices (spawn positions, random-model
        # slots) and handed to each spawned Creature so its stream is tied to
        # the same run. No seed keeps `random` itself, so anything that seeds
        # the module-level generator directly (existing tests, mainly) is
        # unaffected.
        self.seed = seed
        self._rng = random if seed is None else random.Random(seed)
        self.screen_w = screen_w
        self.screen_h = screen_h
        self.creatures: List[Creature] = []
        self.dragged_creature: Optional[Creature] = None
        self.models = {}
        self.personalities = {}
        self.warnings = []
        self.size_scale = 1.0
        self.interferable = True
        self.mood_mode = "auto"
        self.social_play = False
        self.gait_style = "classic"
        # Declared stances between teams, shared by every spider in the scene.
        self.team_stances: dict = {}
        # Who each team is: the name its owner chose and its colour. Keyed
        # by the same case-folded id the stances and the slots use.
        self.team_profiles: dict = {}
        # Right-click naming and the hover/always-on name label.
        self.naming_enabled = True
        self.always_show_names = False
        # Containment cages and the in-progress direct-manipulation of one.
        self.cages: List[Cage] = []
        self._cage_drag = None  # dict: {cage, mode, corner, off_x, off_y}
        # Shared web world: all spiders read and write the same set of webs, so
        # one can build a web and another can walk it or finish it when left.
        self.web_world = WebWorld(screen_w, screen_h)
        # Colony structures are shared by team jobs and persisted separately
        # from the launch preset.  Presets choose jobs; runtime state remembers
        # the progress of the bases they built.
        self._base_runtime_state = []
        self.base_world = BaseWorld(screen_w, screen_h)
        # Shared cursor-silk world: holds the single active web glob / trap that
        # a spider shoots at the real pointer. Only the engine can actually move
        # the OS pointer, so this just computes the desired position each frame.
        self.allow_mouse_capture = True
        self.mouse_web_world = MouseWebWorld(
            screen_w, screen_h, can_control=sys.platform.startswith("win")
        )
        self._desired_cursor: Optional[Tuple[float, float]] = None
        # Flies are autonomous prey: they buzz around, flee spiders, and stick to
        # webs.  The spiders hunt and devour them.  The fly world owns the flock
        # and its spawn timer; this manager owns the predator side of it.
        self.flies_enabled = True
        self.fly_min_interval = 4.0
        self.fly_max_interval = 9.0
        self.fly_max = 6
        self.flies_spawner = True
        self.fly_world = FlyWorld(
            screen_w, screen_h,
            enabled=self.flies_enabled,
            min_interval=self.fly_min_interval,
            max_interval=self.fly_max_interval,
            max_flies=self.fly_max,
            scale=self.size_scale,
        )
        self._dragged_fly = None
        self._dragged_spawner = None
        self._render_order: List[Creature] = []
        self._render_sort_accum = 0.0
        self._neighbor_refresh_accum = 0.0
        # Snapshot of visible real desktop windows, refreshed by the overlay.
        # The list is kept in Win32 Z order (front to back). Spiders only hide
        # behind a specific top-visible window/folder they intentionally chose,
        # never behind lower windows or random incidental crossings.
        self.desktop_surfaces: List[DesktopSurface] = []
        self._surface_by_key = {}
        # Off by default (D1, DC-13): reading desktop icon positions means
        # OpenProcess/ReadProcessMemory against Explorer, the specific pattern
        # antivirus heuristics flag. Window occlusion needs none of that and is
        # controlled separately, never gated by this.
        self.desktop_icons_enabled = False
        self._mouse_x = -100000.0
        self._mouse_y = -100000.0
        self._mouse_down = False
        self._progression_namespace = "default"
        # Resolved in one place so the overlay and the settings window cannot
        # disagree about where runtime state and session control files live.
        self._progression_state_path = state_dir() / "creatures.json"
        # Feeding happens inside the frame loop, so persisting there would put a
        # full JSON rewrite on the render thread every time a fly is eaten.
        # Frequent changes mark the state dirty and a debounced flush in
        # ``update`` writes it; explicit user actions still save immediately.
        self._runtime_state_dirty = False
        self._runtime_state_flush_accum = 0.0
        # Counts launches so an entry for a spider that no longer exists can be
        # retired eventually. Set by the load below.
        self._state_launch = 1
        self._progression_states = self._load_progression_states()
        self.base_world = BaseWorld(screen_w, screen_h, self._base_runtime_state, rng=self._rng)
        self.load_preset(preset_path)

    def _runtime_progression_id(self, index: int) -> str:
        """Return a preset-scoped id for a spider that has no saved slot.

        Bare ``runtime:<index>`` keys are shared by every preset, so a level 12
        hunter in one preset would hand its progression to whatever spider
        happened to land on the same index in another. Scoping the key to the
        loaded preset keeps saved profiles separate.
        """
        return f"{self._progression_namespace}|runtime:{int(index)}"

    def _load_progression_states(self) -> dict:
        """Load per-creature runtime state, migrating old files and bad ones.

        Migration folds keys that differ only by the case of their preset
        namespace, drops keys from the scheme that predated preset scoping, and
        advances the launch counter that drives eviction.
        """
        data = None
        try:
            with self._progression_state_path.open("r", encoding="utf-8") as handle:
                data = json.load(handle)
        except (OSError, ValueError, TypeError):
            data = None
        self._state_launch = next_launch(data)
        states, bases = load_payload(data, self._state_launch)
        self._base_runtime_state = bases
        return states

    def save_runtime_state(self) -> None:
        """Persist meaningful creature state atomically beside the project/exe."""
        states = dict(self._progression_states)
        launch = int(getattr(self, "_state_launch", 1))
        for creature in self.creatures:
            key = str(getattr(creature, "progression_id", self._runtime_progression_id(creature.index)))
            states[key] = stamp_seen({
                "name": creature.name,
                "model": creature.model.get("id"),
                "personality": creature.personality.get("id"),
                "progression": creature.progression.to_dict(),
            }, launch)
        # Retire entries for spiders that have not appeared for a long time, so
        # the file does not grow without bound across presets and slot edits.
        states = evict(states, launch)
        bases = self.base_world.to_dict() if getattr(self, "base_world", None) is not None else []
        payload = build_payload(states, bases, launch)
        try:
            self._progression_state_path.parent.mkdir(parents=True, exist_ok=True)
            temp_path = self._progression_state_path.with_suffix(".tmp")
            with temp_path.open("w", encoding="utf-8") as handle:
                json.dump(payload, handle, indent=2)
                handle.write("\n")
            temp_path.replace(self._progression_state_path)
            self._progression_states = states
            self._runtime_state_dirty = False
            self._runtime_state_flush_accum = 0.0
        except OSError:
            # A read-only portable folder should not prevent the overlay from
            # running; progression still remains live for the current session.
            log.warning(
                "Could not write runtime state to %s; progress will not survive this session",
                self._progression_state_path,
                exc_info=True,
            )
            return

    def mark_runtime_state_dirty(self) -> None:
        """Request a save without writing to disk inside the frame loop."""
        self._runtime_state_dirty = True

    def _flush_runtime_state(self, dt: float) -> None:
        if not self._runtime_state_dirty:
            return
        self._runtime_state_flush_accum += max(0.0, float(dt))
        if self._runtime_state_flush_accum >= RUNTIME_STATE_FLUSH_SECONDS:
            self.save_runtime_state()

    def _create_creature(
        self,
        model: dict,
        personality: dict,
        index: int,
        pos: Tuple[float, float] | None = None,
        skills: list[str] | None = None,
        color_overrides: dict | None = None,
        progression_state: dict | None = None,
        progression_id: str | None = None,
        team_id: str | None = None,
        job_id: str | None = None,
    ) -> Creature:
        state_key = str(progression_id or self._runtime_progression_id(index))
        stored = self._progression_states.get(state_key, {})
        if progression_state is None and isinstance(stored, dict):
            progression_state = stored.get("progression", stored)
        creature = Creature(
            model,
            personality,
            self.screen_w,
            self.screen_h,
            index=index,
            size_scale=self.size_scale,
            skills=skills,
            gait_style=self.gait_style,
            color_overrides=color_overrides,
            progression_state=progression_state,
            progression_id=state_key,
            job_id=normalize_job_id(job_id),
            seed=self.seed,
        )
        if isinstance(stored, dict) and isinstance(stored.get("name"), str):
            creature.set_name(stored["name"])
        saved_progression = stored.get("progression") if isinstance(stored, dict) else None
        # A launch preset supplies the initial team, while an inspector-chosen
        # team in the runtime sidecar remains authoritative on later launches.
        if team_id is not None and not (
            isinstance(saved_progression, dict) and "team_id" in saved_progression
        ):
            creature.set_team(team_id)
        creature.team_stances = self.team_stances
        creature.team_profiles = self.team_profiles
        creature.web_world = self.web_world
        creature.mouse_web_world = self.mouse_web_world
        creature.fly_world = self.fly_world
        if pos is not None:
            creature.x, creature.y = pos
            creature.target_x, creature.target_y = pos
            creature.resize_screen(self.screen_w, self.screen_h)
            creature._initialize_legs()
        # New spiders inherit the manager's current mood/social settings.
        if self.mood_mode and self.mood_mode != "auto":
            creature.set_mood_mode(self.mood_mode)
        creature.set_allow_social(self.social_play)
        return creature

    def _refresh_neighbor_links(self) -> None:
        """Refresh social-neighbor references for spiders in the same visual layer.

        A spider that has crawled fully behind a real desktop window is no longer
        visually on top of the desktop overlay.  While it is hidden, other spiders
        should not chase, cuddle, tag, or watch it; it also should not target them.
        """
        if not self.social_play or len(self.creatures) <= 1:
            for creature in self.creatures:
                creature.set_neighbors([])
            return

        visible_neighbors = [c for c in self.creatures if not self._is_fully_hidden(c)]
        for creature in self.creatures:
            if self._is_fully_hidden(creature):
                creature.social_target = None
                creature.set_neighbors([])
            else:
                creature.set_neighbors(visible_neighbors)

    def _refresh_render_order(self) -> None:
        self._render_order = sorted(self.creatures, key=lambda c: c.y)

    def _update_camouflage_touch_state(self, mx: float, my: float) -> None:
        """Keep Camouflage spiders visible while something touches them.

        The opacity animation itself lives in the overlay timer, but contact is
        easiest to detect here because the manager already knows click-through,
        window occlusion, the dragged creature, and which spiders are visible.
        """
        for creature in self.creatures:
            try:
                max_strength = float(creature.personality.get("camouflage_strength", 0.0) or 0.0)
            except Exception:
                max_strength = 0.0
            if max_strength <= 0.0:
                continue
            touched = creature.dragging or creature is self.dragged_creature
            # A fully hidden window/portal spider is not in the overlay layer, so
            # the cursor and other spiders should not reveal it through the window.
            if self._is_fully_hidden(creature) and not touched:
                continue
            if not touched:
                try:
                    cursor_padding = max(4.0, creature.size * self._personality_float(creature, "camouflage_cursor_touch_padding", 0.10))
                    if math.hypot(creature.x - mx, creature.y - my) <= creature.size * 2.25 + cursor_padding:
                        touched = creature.hit_test(mx, my)
                except Exception:
                    touched = False
            if not touched:
                try:
                    radius_mult = self._personality_float(creature, "camouflage_social_touch_radius_mult", 1.45)
                    for other in self.creatures:
                        if other is creature or self._is_fully_hidden(other):
                            continue
                        contact_radius = max(8.0, (creature.size + other.size) * 0.5 * radius_mult)
                        if math.hypot(creature.x - other.x, creature.y - other.y) <= contact_radius:
                            touched = True
                            break
                except Exception:
                    touched = False
            if touched:
                try:
                    creature.register_camouflage_touch()
                except Exception:
                    creature._camouflage_strength = 0.0
                    creature._camouflage_idle_timer = 0.0
                    creature._camouflage_visible_timer = 5.0

    def load_preset(self, preset_path: Path) -> None:
        # Case-folded: Windows paths are case-insensitive, so Default.json and
        # default.json are one preset and must share one saved profile.
        self._progression_namespace = normalize_namespace(Path(preset_path).stem)
        self.creatures.clear()
        if getattr(self, "web_world", None) is not None:
            self.web_world.clear()
        if getattr(self, "mouse_web_world", None) is not None:
            self.mouse_web_world.clear()
        if getattr(self, "fly_world", None) is not None:
            self.fly_world.clear()
        self.dragged_creature = None
        self.models, model_warnings = discover_models(self.root)
        self.personalities, personality_warnings = discover_personalities(self.root)
        self.warnings = model_warnings + personality_warnings
        preset = load_preset(Path(preset_path))

        settings = preset.get("settings", {})
        if isinstance(settings, dict):
            try:
                self.size_scale = _clamp(float(settings.get("size_scale", self.size_scale)), 0.45, 2.25)
            except Exception:
                self.warnings.append("Preset settings.size_scale was invalid; using normal size.")
                self.size_scale = 1.0
            self.interferable = bool(settings.get("interferable", self.interferable))
            self.mood_mode = str(settings.get("mood_mode", self.mood_mode) or "auto").lower()
            self.social_play = bool(settings.get("social_play", self.social_play))
            self.gait_style = normalize_gait_style(settings.get("gait_style", self.gait_style))
            self.team_stances = normalize_team_stances(settings.get("team_relations"))
            # Every team a slot refers to gets an identity, even in an older
            # preset that has no `teams` block at all.
            self.set_team_profiles(
                settings.get("teams"),
                [slot.get("team_id", slot.get("team")) for slot in preset.get("slots", [])
                 if isinstance(slot, dict)],
            )
            self.apply_fly_settings(settings)

        index = 0
        for slot_index, slot in enumerate(preset.get("slots", [])):
            slot_id = str(slot.get("slot_id") or f"slot-{slot_index}").strip() or f"slot-{slot_index}"
            model_id = slot.get("model")
            if model_id == RANDOM_MODEL_ID:
                model_id = self._random_model_id()
            model = self.models.get(model_id)
            if not model:
                self.warnings.append(f"Preset slot references missing model: {model_id}")
                continue

            personality_id = slot.get("personality") or model.get("default_personality")
            if personality_id == RANDOM_PERSONALITY_ID:
                personality_id = self._random_personality_id()
            personality = self.personalities.get(personality_id) or self._valid_personality_for_model(model)
            if not personality:
                self.warnings.append(f"Preset slot references missing personality: {personality_id}")
                continue

            job_id = normalize_job_id(slot.get("job", "none"))
            raw_abilities = slot.get("abilities")
            job_abilities = job_ability_ids(job_id)
            raw_skills = slot.get("skills")
            if raw_abilities is not None:
                # New presets customize only true capabilities. The
                # personality still owns all behaviour phases.
                skills = skills_with_selected_abilities(personality, list(raw_abilities) + list(job_abilities))
            elif raw_skills is None:
                # No explicit skills on the slot: fall back to the personality's
                # own default abilities (common set plus its specialty) rather
                # than handing every spider every skill.
                skills = skills_with_default_abilities(personality, job_abilities)
            else:
                bad_skills = unknown_skill_ids(raw_skills)
                if bad_skills:
                    self.warnings.append(f"Preset slot for {model_id} ignored unknown skill(s): {', '.join(bad_skills)}")
                skills = normalize_skill_ids(list(raw_skills) + list(job_abilities))

            if bool(slot.get("count_random", False)):
                count = self._rng.randint(1, 10)
            else:
                count = max(1, min(50, int(slot.get("count", 1))))
            for member_index in range(count):
                creature = self._create_creature(
                    model,
                    personality,
                    index,
                    skills=skills,
                    color_overrides=slot.get("colors"),
                    progression_id=f"{self._progression_namespace}|{slot_id}:{member_index}",
                    team_id=slot.get("team_id", slot.get("team", "neutral")),
                    job_id=job_id,
                )
                # Avoid all creatures spawning directly on top of each other.
                creature.x += self._rng.uniform(-80.0, 80.0)
                creature.y += self._rng.uniform(-80.0, 80.0)
                creature.resize_screen(self.screen_w, self.screen_h)
                self.creatures.append(creature)
                index += 1
        if not self.creatures and self.models and self.personalities:
            # Fallback for a bad/empty preset: spawn one default spider so the user sees something.
            model = next(iter(self.models.values()))
            personality = self.personalities.get(model.get("default_personality")) or next(iter(self.personalities.values()))
            self.creatures.append(self._create_creature(
                model, personality, 0, skills=list(DEFAULT_SKILL_IDS),
                progression_id=f"{self._progression_namespace}|fallback:0",
            ))
        self._refresh_neighbor_links()
        self._refresh_render_order()

    def resize(self, screen_w: int, screen_h: int) -> None:
        self.screen_w = screen_w
        self.screen_h = screen_h
        for creature in self.creatures:
            creature.resize_screen(screen_w, screen_h)
        for cage in self.cages:
            cage.clamp_to_screen(self.screen_w, self.screen_h)
        if getattr(self, "web_world", None) is not None:
            self.web_world.set_screen(screen_w, screen_h)
        if getattr(self, "mouse_web_world", None) is not None:
            self.mouse_web_world.set_screen(screen_w, screen_h)
        if getattr(self, "fly_world", None) is not None:
            self.fly_world.set_screen(screen_w, screen_h)
        if getattr(self, "base_world", None) is not None:
            self.base_world.set_screen(screen_w, screen_h)

    def set_desktop_surfaces(self, surfaces: List[DesktopSurface]) -> None:
        """Replace the live snapshot of real desktop windows/folders/icons."""
        self.desktop_surfaces = list(surfaces or [])
        self._surface_by_key = {}
        for surface in self.desktop_surfaces:
            # Keep the first/frontmost surface when rounded keys collide.
            self._surface_by_key.setdefault(self._surface_key(surface), surface)

    def _personality_float(self, creature: Creature, key: str, default: float) -> float:
        try:
            return float(creature.personality.get(key, default))
        except Exception:
            return float(default)

    def _personality_range(self, creature: Creature, key: str, low: float, high: float) -> float:
        raw = creature.personality.get(key)
        try:
            if isinstance(raw, (list, tuple)) and len(raw) >= 2:
                a = float(raw[0])
                b = float(raw[1])
                if b < a:
                    a, b = b, a
                return self._rng.uniform(a, b)
            if raw is not None:
                return float(raw)
        except Exception:
            pass
        return self._rng.uniform(float(low), float(high))

    def wants_mouse(self, mx: float, my: float) -> bool:
        """True when the overlay should capture the mouse at this point.

        Spider pixels are claimed when dragging or naming is enabled; cage frames
        and grips are claimed whenever cages exist. Everywhere else stays
        click-through so the real desktop keeps working.
        """
        if self.cages and self.cage_handle_at(mx, my) is not None:
            return True
        if self.dragged_creature is not None or self._cage_drag is not None:
            return True
        if self._dragged_fly is not None or self._dragged_spawner is not None:
            return True
        if (self.interferable or self.naming_enabled) and self.creature_at(mx, my) is not None:
            return True
        # Flies and the movable nest are grabbable when interaction is enabled.
        if self.interferable and (self.fly_world.hit_fly_at(mx, my) is not None
                                  or self.fly_world.hit_spawner_at(mx, my) is not None):
            return True
        return False

    def creature_at(self, mx: float, my: float) -> Optional[Creature]:
        """Topmost spider under the point, ignoring the interaction toggle.

        Used for hover labels and right-click naming, which should work even when
        dragging is turned off.
        """
        order = self._render_order if self._render_order else self.creatures
        for creature in reversed(order):
            # A spider that is visually behind a real window should also stop
            # stealing clicks from that window.  Keep dragged spiders interactive.
            if not creature.dragging:
                if self._is_fully_hidden(creature):
                    continue
                if self._point_occluded_for_creature(creature, mx, my):
                    continue
            if creature.hit_test(mx, my):
                return creature
        return None

    def _pick_creature(self, mx: float, my: float) -> Optional[Creature]:
        if not self.interferable:
            return None
        # Match render order: higher-y spiders are visually on top, so check them first.
        return self.creature_at(mx, my)

    # ------------------------------------------------------------------
    # Cages
    # ------------------------------------------------------------------
    def add_cage(self, cx: float | None = None, cy: float | None = None,
                 w: float = 360.0, h: float = 260.0) -> str:
        """Create a cage centred at (cx, cy), defaulting to screen centre."""
        if cx is None:
            cx = self.screen_w * 0.5
        if cy is None:
            cy = self.screen_h * 0.5
        cage = Cage(cx - w * 0.5, cy - h * 0.5, w, h)
        cage.clamp_to_screen(self.screen_w, self.screen_h)
        self.cages.append(cage)
        captured = self._capture_inside(cage)
        if captured:
            return f"Cage added; {captured} spider(s) now enclosed."
        return "Cage added. Drag a spider inside to keep it there."

    def remove_cages(self) -> str:
        if not self.cages:
            return "There are no cages to remove."
        for creature in self.creatures:
            creature.cage = None
        count = len(self.cages)
        self.cages.clear()
        self._cage_drag = None
        return f"Removed {count} cage(s); all spiders roam freely again."

    def _capture_inside(self, cage: Cage) -> int:
        """Assign every currently-enclosed free spider to this cage."""
        count = 0
        for creature in self.creatures:
            if creature.cage is None and cage.contains_center(creature.x, creature.y):
                creature.cage = cage
                count += 1
        return count

    def cage_handle_at(self, mx: float, my: float):
        """Return (cage, corner|None) if the point grabs a cage frame/grip."""
        for cage in reversed(self.cages):
            corner = cage.corner_at(mx, my)
            if corner:
                return cage, corner
            if cage.on_border(mx, my):
                return cage, None
        return None

    def cage_hover_kind(self, mx: float, my: float) -> Optional[str]:
        """'resize-nw'/.. , 'move', or None for cursor feedback."""
        hit = self.cage_handle_at(mx, my)
        if hit is None:
            return None
        cage, corner = hit
        return f"resize-{corner}" if corner else "move"

    def _start_cage_drag(self, mx: float, my: float) -> bool:
        hit = self.cage_handle_at(mx, my)
        if hit is None:
            return False
        cage, corner = hit
        if corner:
            self._cage_drag = {"cage": cage, "mode": "resize", "corner": corner,
                               "off_x": 0.0, "off_y": 0.0}
        else:
            self._cage_drag = {"cage": cage, "mode": "move", "corner": None,
                               "off_x": cage.x - mx, "off_y": cage.y - my}
        return True

    def _drag_cage(self, mx: float, my: float) -> None:
        drag = self._cage_drag
        if not drag:
            return
        cage = drag["cage"]
        if drag["mode"] == "resize":
            cage.resize_corner(drag["corner"], mx, my)
            cage.clamp_to_screen(self.screen_w, self.screen_h)
            # Keep enclosed spiders inside the new (possibly smaller) bounds.
            for creature in self.creatures:
                if creature.cage is cage and not creature.dragging:
                    nx, ny = cage.clamp_center(creature.x, creature.y, creature.size * 0.85)
                    creature._translate_leg_world_points(nx - creature.x, ny - creature.y, 0.6)
                    creature.x, creature.y = nx, ny
                    creature.target_x, creature.target_y = nx, ny
        else:
            old_x, old_y = cage.x, cage.y
            cage.move_to(mx + drag["off_x"], my + drag["off_y"])
            cage.clamp_to_screen(self.screen_w, self.screen_h)
            dx = cage.x - old_x
            dy = cage.y - old_y
            if dx or dy:
                # Carry enclosed spiders so they travel with their cage.
                for creature in self.creatures:
                    if creature.cage is cage and not creature.dragging:
                        creature.x += dx
                        creature.y += dy
                        creature.target_x += dx
                        creature.target_y += dy
                        creature._translate_leg_world_points(dx, dy, 1.0)

    def _settle_creature_membership(self, creature: Creature) -> None:
        """Decide a dropped spider's cage from where it was released."""
        for cage in reversed(self.cages):
            if cage.contains_center(creature.x, creature.y):
                creature.cage = cage
                return
        creature.cage = None

    def cage_dirty_rects(self) -> List[Tuple[float, float, float, float]]:
        """Thin border bands for every cage so they stay painted under partial
        repaints without redrawing their open interiors."""
        rects: List[Tuple[float, float, float, float]] = []
        for cage in self.cages:
            rects.extend(cage.border_rects())
        return rects


    def set_interferable(self, enabled: bool) -> None:
        self.interferable = bool(enabled)
        if not self.interferable and self.dragged_creature is not None:
            # Let go immediately if the user disables interaction mid-drag.
            self.dragged_creature.dragging = False
            self.dragged_creature.motion_paused = False
            self.dragged_creature.inertia_vx = 0.0
            self.dragged_creature.inertia_vy = 0.0
            self.dragged_creature.inertia_timer = 0.0
            self.dragged_creature.enter_idle()
            self.dragged_creature = None

    def set_size_scale(self, scale: float) -> None:
        self.size_scale = max(0.45, min(2.25, float(scale)))
        for creature in self.creatures:
            creature.set_size_scale(self.size_scale)
        if getattr(self, "fly_world", None) is not None:
            self.fly_world.configure(scale=self.size_scale)

    def set_mood_mode(self, mode: str) -> str:
        """Set the emotional baseline for every spider at runtime."""
        self.mood_mode = str(mode or "auto").lower()
        for creature in self.creatures:
            creature.set_mood_mode(self.mood_mode)
        label = "Auto (per personality)" if self.mood_mode == "auto" else self.mood_mode.capitalize()
        return f"Mood set to {label} for {len(self.creatures)} spider(s)."

    def set_gait_style(self, style: str) -> str:
        """Set the movement/leg-animation style for every spider at runtime."""
        self.gait_style = normalize_gait_style(style)
        for creature in self.creatures:
            creature.set_gait_style(self.gait_style)
        label = GAIT_LABELS.get(self.gait_style, "Classic")
        return f"Movement set to {label} for {len(self.creatures)} spider(s)."

    def set_social_play(self, enabled: bool) -> str:
        """Enable or disable spiders seeking each other out to play."""
        self.social_play = bool(enabled)
        for creature in self.creatures:
            creature.set_allow_social(self.social_play)
        self._refresh_neighbor_links()
        state = "on" if self.social_play else "off"
        return f"Social play turned {state}."

    def set_team_stances(self, raw) -> str:
        """Replace the declared stances between teams and share them live."""
        self.team_stances = normalize_team_stances(raw)
        for creature in self.creatures:
            creature.team_stances = self.team_stances
        declared = sum(len(row) for row in self.team_stances.values()) // 2
        return f"Team relations updated ({declared} declared)."

    def set_team_profiles(self, raw, used_ids=()) -> str:
        """Replace the team names and colours, and share them live.

        Shared by reference, the way the stances are, so a team renamed while
        the overlay is running is renamed everywhere at once.
        """
        self.team_profiles = normalize_teams(raw, used_ids)
        for creature in self.creatures:
            creature.team_profiles = self.team_profiles
        base_world = getattr(self, "base_world", None)
        if base_world is not None:
            base_world.team_profiles = self.team_profiles
        return f"Teams updated ({len(self.team_profiles)} named)."

    def team_payload(self) -> dict:
        """The `teams` block for saving, so a preset round-trips its names."""
        return teams_payload(self.team_profiles)

    def set_allow_mouse_capture(self, enabled: bool) -> str:
        """Allow or forbid spiders shooting silk that traps/shoves the pointer."""
        self.allow_mouse_capture = bool(enabled)
        self.mouse_web_world.enabled = self.allow_mouse_capture
        if not self.allow_mouse_capture:
            # Free the pointer immediately and cancel any glob in flight.
            self.mouse_web_world.clear()
        state = "on" if self.allow_mouse_capture else "off"
        return f"Mouse web-trapping turned {state}."

    def set_desktop_icons_enabled(self, enabled: bool) -> str:
        """Allow or forbid probing real desktop icon positions (D1, DC-13).

        Off by default: reading them means OpenProcess/ReadProcessMemory
        against Explorer, which is the specific pattern antivirus heuristics
        flag, on top of being the more expensive half of desktop probing.
        Window occlusion (EnumWindows, no process memory) is unaffected and
        stays on regardless of this setting.
        """
        self.desktop_icons_enabled = bool(enabled)
        state = "on" if self.desktop_icons_enabled else "off"
        return f"Desktop icon awareness turned {state}."

    # ------------------------------------------------------------------
    # Flies
    # ------------------------------------------------------------------
    def award_feed_xp(self, creature: Creature, amount: int = FEED_XP_REWARD,
                      source: str = "feed") -> list[str]:
        """Shared hook for future food sources (flies, treats, web catches)."""
        if creature is None or creature not in self.creatures:
            return []
        events = creature.gain_experience(amount, reason=source)
        self.mark_runtime_state_dirty()
        return events

    def apply_fly_settings(self, settings: dict | None) -> None:
        """Read the optional ``flies`` block of a preset's settings."""
        if not isinstance(settings, dict):
            return
        flies = settings.get("flies")
        if not isinstance(flies, dict):
            return
        if "enabled" in flies:
            self.flies_enabled = bool(flies.get("enabled"))
        try:
            if "min_interval" in flies:
                self.fly_min_interval = max(0.3, float(flies.get("min_interval")))
            if "max_interval" in flies:
                self.fly_max_interval = max(0.4, float(flies.get("max_interval")))
        except Exception:
            self.warnings.append("Preset settings.flies interval was invalid; using defaults.")
        if self.fly_max_interval < self.fly_min_interval:
            self.fly_max_interval = self.fly_min_interval
        try:
            if "max_flies" in flies:
                self.fly_max = max(0, min(40, int(flies.get("max_flies"))))
        except Exception:
            self.warnings.append("Preset settings.flies max_flies was invalid; using default.")
        if "spawner" in flies:
            self.flies_spawner = bool(flies.get("spawner"))
        self.fly_world.configure(
            enabled=self.flies_enabled,
            min_interval=self.fly_min_interval,
            max_interval=self.fly_max_interval,
            max_flies=self.fly_max,
            scale=self.size_scale,
            use_spawner=self.flies_spawner,
        )

    def set_flies_enabled(self, enabled: bool) -> str:
        self.flies_enabled = bool(enabled)
        self.fly_world.configure(enabled=self.flies_enabled)
        if not self.flies_enabled:
            self.fly_world.clear()
            for creature in self.creatures:
                creature._prey = None
                creature._hunting_prey = False
        return f"Flies turned {'on' if self.flies_enabled else 'off'}."

    def set_fly_spawn_interval(self, min_interval: float, max_interval: float) -> str:
        self.fly_min_interval = max(0.3, float(min_interval))
        self.fly_max_interval = max(self.fly_min_interval, float(max_interval))
        self.fly_world.configure(min_interval=self.fly_min_interval,
                                 max_interval=self.fly_max_interval)
        return f"Flies now spawn every {self.fly_min_interval:.0f}-{self.fly_max_interval:.0f}s."

    def set_fly_max(self, max_flies: int) -> str:
        self.fly_max = max(0, min(40, int(max_flies)))
        self.fly_world.configure(max_flies=self.fly_max)
        return f"Up to {self.fly_max} flies at once."

    def set_fly_rate_preset(self, name: str) -> str:
        """Quick spawn-rate presets used by the tray menu."""
        presets = {
            "off": (False, self.fly_min_interval, self.fly_max_interval, self.fly_max),
            "sparse": (True, 9.0, 18.0, 3),
            "normal": (True, 4.0, 9.0, 6),
            "swarm": (True, 1.0, 3.0, 14),
        }
        key = str(name).strip().lower()
        if key not in presets:
            return "Unknown fly setting."
        enabled, lo, hi, cap = presets[key]
        self.flies_enabled = bool(enabled)
        if key != "off":
            self.fly_min_interval = float(lo)
            self.fly_max_interval = float(hi)
            self.fly_max = int(cap)
        self.fly_world.configure(
            enabled=self.flies_enabled,
            min_interval=self.fly_min_interval,
            max_interval=self.fly_max_interval,
            max_flies=self.fly_max,
        )
        if not self.flies_enabled:
            self.fly_world.clear()
            for creature in self.creatures:
                creature._prey = None
                creature._hunting_prey = False
            return "Flies turned off."
        labels = {"sparse": "Sparse", "normal": "Normal", "swarm": "Swarm"}
        return f"Flies set to {labels.get(key, key)}."

    def release_fly(self) -> str:
        """Spawn one fly immediately, regardless of the spawn timer."""
        return self.fly_world.spawn_now()

    def set_fly_spawner_enabled(self, enabled: bool) -> str:
        self.flies_spawner = bool(enabled)
        self.fly_world.configure(use_spawner=self.flies_spawner)
        if self.flies_spawner:
            return "Flies now emerge from a movable nest."
        return "Flies now drift in from the screen edges."

    def add_fly_spawner(self) -> str:
        self.flies_spawner = True
        self.fly_world.add_spawner()
        return f"Added a nest ({len(self.fly_world.spawners)} on screen)."

    def reset_fly_spawner(self) -> str:
        self.flies_spawner = True
        self.fly_world.reset_spawners()
        return "Nest reset to one in its default spot."

    def _random_model_id(self) -> str | None:
        return self._rng.choice(list(self.models.keys())) if self.models else None

    def _random_personality_id(self) -> str | None:
        pool = [pid for pid in COMPACT_TEMPERAMENT_IDS if pid in self.personalities]
        return self._rng.choice(pool) if pool else (self._rng.choice(list(self.personalities.keys())) if self.personalities else None)

    def _valid_personality_for_model(self, model: dict, preferred_id: str | None = None) -> dict | None:
        if preferred_id and preferred_id in self.personalities:
            return self.personalities[preferred_id]
        default_id = model.get("default_personality")
        return self.personalities.get(default_id) or (self._rng.choice(list(self.personalities.values())) if self.personalities else None)

    def _replace_with_traits(self, traits: List[tuple], keep_positions: bool = True) -> None:
        old_positions = [(c.x, c.y) for c in self.creatures]
        old_names = [c.name for c in self.creatures]
        old_progression = [c.progression.to_dict() for c in self.creatures]
        old_progression_ids = [
            getattr(c, "progression_id", self._runtime_progression_id(i))
            for i, c in enumerate(self.creatures)
        ]
        self.creatures.clear()
        if getattr(self, "web_world", None) is not None:
            self.web_world.clear()
        if getattr(self, "mouse_web_world", None) is not None:
            self.mouse_web_world.clear()
        if getattr(self, "fly_world", None) is not None:
            # The old spiders are gone; drop any claims/tethers that referenced them.
            self.fly_world.clear_hunters()
        self.dragged_creature = None
        for index, trait in enumerate(traits):
            model_id, personality_id = trait[0], trait[1]
            skills = normalize_skill_ids(trait[2]) if len(trait) > 2 else list(DEFAULT_SKILL_IDS)
            model = self.models.get(model_id)
            if not model:
                continue
            personality = self.personalities.get(personality_id) or self._valid_personality_for_model(model)
            if not personality:
                continue
            pos = old_positions[index] if keep_positions and index < len(old_positions) else None
            color_overrides = trait[3] if len(trait) > 3 else None
            progression_id = (
                old_progression_ids[index]
                if keep_positions and index < len(old_progression_ids)
                else self._runtime_progression_id(index)
            )
            progression_state = old_progression[index] if keep_positions and index < len(old_progression) else None
            team_id = trait[4] if len(trait) > 4 else None
            job_id = normalize_job_id(trait[5] if len(trait) > 5 else "none")
            creature = self._create_creature(
                model,
                personality,
                index,
                pos=pos,
                skills=skills,
                color_overrides=color_overrides,
                progression_state=progression_state,
                progression_id=progression_id,
                team_id=team_id,
                job_id=job_id,
            )
            # Names follow the slot index when positions are preserved so a
            # casual "randomize models" does not silently wipe pet names.
            if keep_positions and index < len(old_names):
                creature.set_name(old_names[index])
            self.creatures.append(creature)
        if not self.creatures and self.models and self.personalities:
            model_id = self._random_model_id()
            model = self.models.get(model_id) if model_id else None
            personality = self._valid_personality_for_model(model) if model else None
            if model and personality:
                self.creatures.append(self._create_creature(
                    model, personality, 0, skills=list(DEFAULT_SKILL_IDS),
                    progression_id=f"{self._progression_namespace}|fallback:0",
                ))
        # Re-home cage membership from geometry. Enclosed spiders stay enclosed.
        if keep_positions and self.cages:
            for creature in self.creatures:
                self._settle_creature_membership(creature)
        elif not keep_positions:
            for creature in self.creatures:
                creature.cage = None
        self._refresh_neighbor_links()
        self._refresh_render_order()

    def reload_from_preset_data(self, data: dict) -> str:
        """Apply a preset's settings and roster to the running overlay in place.

        Used for live updates: the settings window rewrites the launched preset
        file, and the overlay reloads it without restarting the process.  The
        spider roster is rebuilt fresh (existing webs and pointer traps are
        cleared), so changing models, personalities, counts, or skills takes
        effect immediately.
        """
        if not isinstance(data, dict):
            return "Ignored malformed preset data."
        settings = data.get("settings", {})
        if isinstance(settings, dict):
            if "size_scale" in settings:
                try:
                    self.set_size_scale(float(settings.get("size_scale", self.size_scale)))
                except Exception:
                    pass
            if "interferable" in settings:
                self.set_interferable(bool(settings.get("interferable")))
            if "mood_mode" in settings:
                self.set_mood_mode(str(settings.get("mood_mode") or "auto"))
            if "social_play" in settings:
                self.set_social_play(bool(settings.get("social_play")))
            if "gait_style" in settings:
                self.set_gait_style(str(settings.get("gait_style") or "classic"))
            if "allow_mouse_capture" in settings:
                self.set_allow_mouse_capture(bool(settings.get("allow_mouse_capture")))
            if "desktop_icons_enabled" in settings:
                self.set_desktop_icons_enabled(bool(settings.get("desktop_icons_enabled")))
            if "team_relations" in settings:
                self.set_team_stances(settings.get("team_relations"))
            # A live edit can rename a team or recolour it, and it can also add
            # a slot on a team the block has never mentioned, so the ids in use
            # are passed in every time rather than only at launch.
            self.set_team_profiles(
                settings.get("teams", self.team_payload()),
                [slot.get("team_id", slot.get("team")) for slot in data.get("slots", [])
                 if isinstance(slot, dict)],
            )
            self.apply_fly_settings(settings)

        traits: list[tuple] = []
        for slot in data.get("slots", []):
            model_id = slot.get("model")
            if model_id == RANDOM_MODEL_ID:
                model_id = self._random_model_id()
            model = self.models.get(model_id)
            if not model:
                continue
            personality_id = slot.get("personality") or model.get("default_personality")
            if personality_id == RANDOM_PERSONALITY_ID:
                personality_id = self._random_personality_id()
            personality = self.personalities.get(personality_id) or self._valid_personality_for_model(model)
            if not personality:
                continue
            job_id = normalize_job_id(slot.get("job", "none"))
            job_abilities = job_ability_ids(job_id)
            raw_abilities = slot.get("abilities")
            raw_skills = slot.get("skills")
            if raw_abilities is not None:
                skills = skills_with_selected_abilities(personality, list(raw_abilities) + list(job_abilities))
            else:
                skills = skills_with_default_abilities(personality, job_abilities) if raw_skills is None else normalize_skill_ids(list(raw_skills) + list(job_abilities))
            if bool(slot.get("count_random", False)):
                count = self._rng.randint(1, 10)
            else:
                count = max(1, min(50, int(slot.get("count", 1))))
            for _ in range(count):
                traits.append((model["id"], personality["id"], skills, slot.get("colors"), slot.get("team_id", slot.get("team", "neutral")), job_id))

        if traits:
            self._replace_with_traits(traits, keep_positions=False)
        return f"Applied live: {len(self.creatures)} spider(s)."

    def session_snapshot(self) -> dict:
        """A JSON-safe snapshot of the state a tray action can change live.

        Sent to any connected settings window over the live channel (DC-16)
        so its model reflects mood, size, flies and social-play toggles, and
        per-spider names and teams, instead of only learning about them the
        next time it happens to reload the preset file (C7).
        """
        return {
            "mood_mode": self.mood_mode,
            "size_scale": self.size_scale,
            "social_play": self.social_play,
            "flies_enabled": self.flies_enabled,
            "interferable": self.interferable,
            "creatures": [
                {
                    "id": str(getattr(creature, "progression_id", creature.index)),
                    "name": creature.name,
                    "team_id": creature.progression.team_id,
                }
                for creature in self.creatures
            ],
        }

    def randomize_models(self) -> str:
        if not self.creatures or not self.models:
            return "No models available."
        traits: list[tuple] = []
        for creature in self.creatures:
            model_id = self._random_model_id()
            model = self.models.get(model_id) if model_id else None
            if not model:
                continue
            personality = self._valid_personality_for_model(model, creature.personality.get("id"))
            if personality:
                traits.append((model["id"], personality["id"], creature.skill_ids(), dict(getattr(creature, "color_overrides", {})), creature.progression.team_id, getattr(creature, "job_id", "none")))
        self._replace_with_traits(traits, keep_positions=True)
        return f"Randomized model for {len(self.creatures)} spider(s)."

    def randomize_personalities(self) -> str:
        if not self.creatures or not self.personalities:
            return "No personalities available."
        traits: list[tuple] = []
        for creature in self.creatures:
            personality_id = self._random_personality_id()
            if personality_id:
                traits.append((creature.model["id"], personality_id, creature.skill_ids(), dict(getattr(creature, "color_overrides", {})), creature.progression.team_id, getattr(creature, "job_id", "none")))
        self._replace_with_traits(traits, keep_positions=True)
        return f"Randomized personality for {len(self.creatures)} spider(s)."

    def randomize_count(self, low: int = 1, high: int = 10) -> str:
        if not self.models or not self.personalities:
            return "No models/personalities available."
        count = self._rng.randint(int(low), int(high))
        existing_traits = [
            (c.model["id"], c.personality["id"], c.skill_ids(), dict(getattr(c, "color_overrides", {})), c.progression.team_id, getattr(c, "job_id", "none"))
            for c in self.creatures
        ]
        if not existing_traits:
            model_id = self._random_model_id()
            model = self.models.get(model_id) if model_id else None
            personality = self._valid_personality_for_model(model) if model else None
            existing_traits = [(model["id"], personality["id"], list(DEFAULT_SKILL_IDS), None, "neutral", "none")] if model and personality else []
        traits = [existing_traits[i % len(existing_traits)] for i in range(count)] if existing_traits else []
        self._replace_with_traits(traits, keep_positions=True)
        return f"Randomized count: {len(self.creatures)} spider(s)."

    def randomize_everything(self) -> str:
        if not self.models or not self.personalities:
            return "No models/personalities available."
        count = self._rng.randint(1, 10)
        traits: list[tuple] = []
        for _ in range(count):
            model_id = self._random_model_id()
            personality_id = self._random_personality_id()
            if model_id and personality_id:
                traits.append((model_id, personality_id, list(DEFAULT_SKILL_IDS), None, "neutral", "none"))
        self._replace_with_traits(traits, keep_positions=False)
        return f"Randomized everything: {len(self.creatures)} spider(s)."

    def set_creature_skill(self, creature: Creature, skill_id: str, enabled: bool) -> str:
        if creature is None:
            return "No spider selected."
        if skill_id not in SKILL_BY_ID:
            return f"Unknown skill: {skill_id}."
        creature.set_skill_enabled(skill_id, enabled)
        label = SKILL_BY_ID[skill_id].display_name
        name = f"“{creature.name}”" if creature.name else "this spider"
        return f"{label} {'enabled' if enabled else 'disabled'} for {name}."

    # ------------------------------------------------------------------
    # Fly hunting: which spider chases which fly, and the killing bite.
    #
    # DC-18 (C1): the manager still owns *target selection* below -- which
    # fly (if any) a spider locks onto -- the same manager-injected state
    # DC-17's docstring already treats as acceptable. Acting on that lock
    # used to be CreatureManager._drive_hunt, which called
    # enter_approach/enter_chase and wrote target_x/speed/motion_paused on
    # the creature directly, every frame, from outside -- the outside-in
    # overwrite the plan calls out. That reaction moved onto the creature
    # itself as BehaviourMixin._pursue_prey, reading the candidate through
    # Perception.prey() and using the creature's own seeded rng.
    # ------------------------------------------------------------------

    def _creature_can_hunt(self, creature: Creature) -> bool:
        if creature.airborne:
            return False
        if self._is_fully_hidden(creature):
            return False
        if getattr(creature, "_feed_cooldown", 0.0) > 0.0:
            return False
        # A spider being carried can still lock onto a fly it spots and web it,
        # it just cannot run it down; so dragging does not block acquisition.
        if creature.dragging or creature is self.dragged_creature:
            return True
        if creature.state in HUNT_BUSY_STATES:
            return False
        # Needs some way to actually close on prey.
        return (creature.has_skill("chase") or creature.has_skill("approach")
                or creature.has_skill("jump"))

    def _hunt_sense_radius(self, creature: Creature, trapped: bool) -> float:
        reaction = float(creature.personality.get("reaction_radius", 360))
        sense = _clamp(reaction * 0.9, 240.0, 900.0)
        if trapped:
            # A fly thrashing on a web telegraphs itself: the wider radius is the
            # "the web is moving, come and get it" signal to nearby spiders.
            sense = sense * 1.5 + 160.0
        return sense

    def _update_prey_targets(self, dt: float) -> None:
        # Tick down the per-spider hunt cooldowns.
        for creature in self.creatures:
            creature._feed_cooldown = max(0.0, getattr(creature, "_feed_cooldown", 0.0) - dt)
            creature._pounce_cooldown = max(0.0, getattr(creature, "_pounce_cooldown", 0.0) - dt)
            creature._trap_shot_cooldown = max(0.0, getattr(creature, "_trap_shot_cooldown", 0.0) - dt)
            creature._prey_recheck = max(0.0, getattr(creature, "_prey_recheck", 0.0) - dt)

        flies = [f for f in self.fly_world.flies if f.alive and not f.eaten and not f.dragging]
        if not self.flies_enabled or not flies:
            for creature in self.creatures:
                prey = getattr(creature, "_prey", None)
                if prey is not None:
                    prey.hunters.discard(creature)
                    creature._prey = None
            return

        for creature in self.creatures:
            prey = getattr(creature, "_prey", None)

            # Drop a target that died, was eaten, or that we can no longer hunt.
            if prey is not None and (prey not in flies or not self._creature_can_hunt(creature)):
                prey.hunters.discard(creature)
                creature._prey = None
                prey = None

            # Keep the current target if it is still reasonably close (hysteresis),
            # so spiders do not jitter between flies every frame.
            if prey is not None:
                keep = self._hunt_sense_radius(creature, prey.trapped) * 1.6
                if distance(creature.x, creature.y, prey.x, prey.y) > keep:
                    prey.hunters.discard(creature)
                    creature._prey = None
                    prey = None

            if not self._creature_can_hunt(creature):
                continue

            # Re-pick periodically (or immediately when we have no target).
            if prey is not None and creature._prey_recheck > 0.0:
                continue
            creature._prey_recheck = self._rng.uniform(0.2, 0.4)

            best = None
            best_score = -1e18
            for fly in flies:
                sense = self._hunt_sense_radius(creature, fly.trapped)
                d = distance(creature.x, creature.y, fly.x, fly.y)
                if d > sense and fly is not prey:
                    continue
                # Higher score = better target.  Prefer trapped flies (easy and
                # signalled), closer flies, and flies fewer spiders already chase.
                score = (sense - d)
                if fly.trapped:
                    score += 500.0
                claimers = len(fly.hunters - {creature})
                score -= claimers * 140.0
                if fly is prey:
                    score += 90.0  # mild stickiness toward the current target
                if score > best_score:
                    best_score = score
                    best = fly

            if best is not prey:
                if prey is not None:
                    prey.hunters.discard(creature)
                if best is not None:
                    best.hunters.add(creature)
                creature._prey = best

    def _creature_focus(self, creature: Creature, mx: float, my: float):
        """Return (focus_x, focus_y, hunting) for this spider's update call."""
        prey = getattr(creature, "_prey", None)
        if prey is not None and prey.alive and not prey.eaten:
            return prey.x, prey.y, True
        return mx, my, False

    def _resolve_fly_catches(self) -> None:
        for fly in self.fly_world.flies:
            if not fly.alive or fly.eaten or fly.dragging:
                continue
            # Keep a tether visually attached to a living trapper that is still
            # closing in, so the silk does not dangle from empty space.
            if fly.trapped and fly.tether_from is not None:
                trapper = getattr(fly, "_trapper", None)
                if trapper is not None and trapper in self.creatures and not trapper.dragging:
                    fly.tether_from = (
                        trapper.x + math.cos(trapper.heading) * trapper.size * 0.6,
                        trapper.y + math.sin(trapper.heading) * trapper.size * 0.6,
                    )

            best = None
            best_d = 1e18
            for creature in self.creatures:
                if creature.dragging or creature.state == "Feed":
                    continue
                if self._is_fully_hidden(creature):
                    continue
                catch = creature.size * 1.5 + fly.size + 4.0
                d = distance(creature.x, creature.y, fly.x, fly.y)
                if d <= catch and d < best_d:
                    best_d = d
                    best = creature
            if best is not None:
                # Leave a little pile of fading remains where the fly was eaten.
                self.fly_world.add_remains((fly.x, fly.y), scale=fly.size / 9.0)
                fly.begin_eaten()
                # This is the single authoritative feeding hook.  It runs only
                # after the fly transitions to ``eaten`` so repeated collision
                # checks cannot award duplicate XP.
                self.award_feed_xp(best, FEED_XP_REWARD, "fly")
                # DC-21: every eaten fly tops up its eater's team food a
                # little, regardless of job -- the Hunter's own carry-home
                # trip (``HUNTER_CARRY_FOOD_AMOUNT`` in jobs.py) stays the
                # larger, deliberate source; this is the broader "any catch
                # counts" top-up the plan's "eaten flies credit team food"
                # asks for.
                base_world = getattr(self, "base_world", None)
                if base_world is not None:
                    base_world.credit_team_food(
                        getattr(best.progression, "team_id", None), FLY_CATCH_RESOURCE_AMOUNT,
                    )
                # Free every hunter that was locked onto this fly.
                for hunter in list(fly.hunters):
                    hunter._prey = None
                    hunter._hunting_prey = False
                fly.hunters.clear()
                best.enter_feed(fly.x, fly.y)
                best._feed_cooldown = self._rng.uniform(0.6, 1.2)

    def update(
        self,
        dt: float,
        mx: float,
        my: float,
        mouse_down: bool = False,
        mouse_pressed: bool = False,
        mouse_released: bool = False,
    ) -> Optional[Tuple[float, float]]:
        self._mouse_x = mx
        self._mouse_y = my
        self._mouse_down = bool(mouse_down)
        if mouse_pressed:
            handled = False
            # 1) A cage corner grip always wins so the cage stays resizable even
            #    when a spider sits nearby.
            hit = self.cage_handle_at(mx, my)
            if hit is not None and hit[1] is not None:
                handled = self._start_cage_drag(mx, my)
            # 2) Grab a spider (only when dragging is enabled).
            if not handled and self.interferable:
                self.dragged_creature = self._pick_creature(mx, my)
                if self.dragged_creature is not None:
                    self.dragged_creature.start_drag(mx, my)
                    handled = True
            # 2b) Grab a fly, or the nest object, so they can be moved around.
            if not handled and self.interferable:
                fly = self.fly_world.hit_fly_at(mx, my)
                if fly is not None:
                    fly.start_drag(mx, my)
                    fly._prey = None  # drop any hunters' lock when picked up
                    for hunter in list(fly.hunters):
                        hunter._prey = None
                        hunter._hunting_prey = False
                    fly.hunters.clear()
                    self._dragged_fly = fly
                    handled = True
            if not handled and self.interferable:
                spawner = self.fly_world.hit_spawner_at(mx, my)
                if spawner is not None:
                    spawner.start_drag(mx, my)
                    self._dragged_spawner = spawner
                    handled = True
            # 3) Otherwise grab the cage frame to move the whole enclosure.
            if not handled and hit is not None and hit[1] is None:
                self._start_cage_drag(mx, my)

        if mouse_down and self._cage_drag is not None:
            self._drag_cage(mx, my)
        elif self.interferable and mouse_down and self.dragged_creature is not None:
            self.dragged_creature.drag_to(dt, mx, my)
        elif mouse_down and self._dragged_fly is not None:
            self._dragged_fly.drag_to(mx, my)
        elif mouse_down and self._dragged_spawner is not None:
            self._dragged_spawner.drag_to(mx, my, self.screen_w, self.screen_h)

        if mouse_released and self._cage_drag is not None:
            self._cage_drag = None

        if mouse_released and self._dragged_fly is not None:
            self._dragged_fly.release_drag(mx, my)
            self._dragged_fly = None
        if mouse_released and self._dragged_spawner is not None:
            self._dragged_spawner.release_drag()
            self._dragged_spawner = None

        if (mouse_released or not self.interferable) and self.dragged_creature is not None:
            self.dragged_creature.release_drag(mx, my)
            # Where it was dropped decides which cage (if any) now holds it.
            self._settle_creature_membership(self.dragged_creature)
            self.dragged_creature = None

        self._update_camouflage_touch_state(mx, my)

        # At high FPS the spider animation should stay smooth, but social-link
        # refreshes and z-order sorting do not need to run 60 times per second.
        self._neighbor_refresh_accum += dt
        if self._neighbor_refresh_accum >= 0.15:
            self._neighbor_refresh_accum = 0.0
            self._refresh_neighbor_links()

        # Every system below is timed so the cost of a frame can be attributed
        # instead of guessed at. `get_profiler()` returns a do-nothing object
        # unless DESKTOP_BUG_PROFILE asked for measurement.
        profiler = get_profiler()

        # Decide which fly (if any) each spider is hunting this frame.
        with profiler.section("behaviour"):
            self._update_prey_targets(dt)
        # Jobs publish their per-frame work intent before the Creature FSM runs:
        # builders travel/build, guards patrol/raise alerts, and personality
        # remains free to describe *how* that work looks.
        with profiler.section("jobs"):
            self.base_world.update(dt, self.creatures, self.web_world)
        # Base construction advances continuously, so it uses the same debounced
        # save as feeding instead of only being persisted on quit.
        if any(getattr(creature, "job_mode", "idle") == "build" for creature in self.creatures):
            self.mark_runtime_state_dirty()
        self._flush_runtime_state(dt)

        # Hoisted out of the loop: the span objects are reused, so timing ten
        # spiders costs ten clock reads per system rather than ten lookups too.
        desktop_span = profiler.section("desktop")
        creatures_span = profiler.section("creatures")
        for creature in self.creatures:
            has_prey = getattr(creature, "_prey", None) is not None
            with desktop_span:
                # A spider locked onto a fly should not wander off behind a window.
                if not has_prey:
                    self._maybe_seek_desktop_cover(creature, dt)
                hidden = self._is_fully_hidden(creature)
            if hidden and not creature.dragging:
                # Once a spider is fully behind a window, it is no longer in the
                # overlay layer: it should keep crawling, but it should not react
                # to the real mouse cursor or visible spiders until it emerges.
                creature.social_target = None
                creature._hunting_prey = False
                with creatures_span:
                    creature.update(dt, -100000.0, -100000.0, self.screen_w, self.screen_h)
            else:
                fx, fy, hunting = self._creature_focus(creature, mx, my)
                # Publish the lock; the spider's own update() reacts to it via
                # Perception.prey()/_pursue_prey (DC-18, C1) with the fly fed
                # in as its focus, instead of the manager driving it directly.
                creature._hunting_prey = hunting
                with creatures_span:
                    creature.update(dt, fx, fy, self.screen_w, self.screen_h)
            with desktop_span:
                self._update_desktop_awareness(creature, dt)

        # Advance web build progress, bounce decay, tearing from the moving
        # cursor, and dirty-rect bookkeeping once after all spiders have moved.
        with profiler.section("webs"):
            self.web_world.update(dt, (mx, my))

        # Advance the cursor-silk world. While a spider has the pointer trapped
        # or is shoving it to a wall, this returns the overlay-local position the
        # engine should force the OS pointer to this frame; otherwise None.
        self.mouse_web_world.enabled = self.allow_mouse_capture
        with profiler.section("mouse-webs"):
            self._desired_cursor = self.mouse_web_world.update(dt, mx, my)

        # Move the flies (they flee the spiders that just moved and may stick to
        # webs), then let any spider that reached a fly devour it.
        with profiler.section("flies"):
            self.fly_world.update(dt, self.creatures, self.web_world)
            self._resolve_fly_catches()

        # Resort enough to look correct when spiders cross, without paying the
        # sort/allocation cost every single frame. Dragging still updates quickly.
        self._render_sort_accum += dt
        if self.dragged_creature is not None or self._render_sort_accum >= 0.10:
            self._render_sort_accum = 0.0
            self._refresh_render_order()

        return self._desired_cursor

    def render(self, painter) -> None:
        profiler = get_profiler()
        # Cages draw first so spiders appear inside them.
        active_cage = self._cage_drag["cage"] if self._cage_drag else None
        with profiler.section("render-props"):
            if self.cages:
                counts = {id(c): 0 for c in self.cages}
                for creature in self.creatures:
                    if creature.cage is not None and id(creature.cage) in counts:
                        counts[id(creature.cage)] += 1
                for cage in self.cages:
                    cage.draw(painter, member_count=counts.get(id(cage), 0), active=cage is active_cage)
            # During partial repaints only the exposed region is redrawn; skip
            # spiders that fall entirely outside it so we do not build leg paths for
            # creatures that would be clipped away anyway.
            clip = None
            try:
                cr = painter.clipBoundingRect()
                if not cr.isNull():
                    clip = (cr.left() - 2.0, cr.top() - 2.0, cr.right() + 2.0, cr.bottom() + 2.0)
            except Exception:
                clip = None
            # Webs sit above the cages but beneath the spiders, so a spider always
            # appears to stand on top of the silk it is weaving or walking.
            self.base_world.render(painter, clip)
            self.web_world.render(painter, clip)
            # Cursor-silk (the flying glob, the trap splat, the wall shove) draws in
            # the same layer, beneath the spiders.
            self.mouse_web_world.render(painter, clip)
            # Flies buzz above the silk but beneath the spiders, so a spider visibly
            # covers a fly as it lands on it to feed.
            self.fly_world.render(painter, clip)
        order = self._render_order if self._render_order else self.creatures
        render_span = profiler.section("render")
        for creature in order:
            with render_span:
                if clip is not None:
                    x0, y0, x1, y1 = creature.bounding_rect(self.always_show_names)
                    if x1 < clip[0] or x0 > clip[2] or y1 < clip[1] or y0 > clip[3]:
                        continue

                occluders = self._occluding_surfaces_for_creature(creature)
                if occluders and self._creature_fully_covered_by_any_surface(creature, occluders):
                    continue

                # Do not fade the entire spider.  Clip drawing against the *visible*
                # part of the chosen top window, so the border cuts the spider cleanly
                # and lower covered windows do not create invisible walls.
                if occluders:
                    try:
                        from PyQt5.QtCore import QRect, Qt
                        from PyQt5.QtGui import QRegion
                        visible_region = QRegion(QRect(0, 0, int(self.screen_w), int(self.screen_h)))
                        for surface in occluders:
                            hidden_region = self._surface_visible_qregion(surface, QRect, QRegion)
                            if not hidden_region.isEmpty():
                                visible_region -= hidden_region
                        painter.save()
                        painter.setClipRegion(visible_region, Qt.IntersectClip)
                        creature.render(painter, always_show_names=self.always_show_names)
                        painter.restore()
                    except Exception:
                        # If a Qt clipping call fails for any reason, keep the spider
                        # visible rather than making it vanish suddenly.
                        creature.render(painter, always_show_names=self.always_show_names)
                else:
                    creature.render(painter, always_show_names=self.always_show_names)

    # ------------------------------------------------------------------
    # Desktop window / folder awareness
    # ------------------------------------------------------------------
    def _clear_desktop_hide_intent(self, creature: Creature) -> None:
        creature._desktop_hide_surface_key = None
        creature._desktop_hide_kind = None
        creature._desktop_hide_timer = 0.0
        creature._folder_portal_dwell = 0.0

    def _behavior_allows_desktop_hide(self, creature: Creature) -> bool:
        # Hiding should look deliberate. Do not let mouse/social chase, startle,
        # jumping, dragging, inspecting, or catching accidentally send a visible
        # spider under a window. Fully hidden spiders are already behind the
        # surface and are allowed to keep crawling out.
        if creature.dragging or creature.airborne or creature.cage is not None:
            return False
        if creature.state in {
            "Alert", "Approach", "Chase", "Retreat", "Startled",
            "Aim", "Inspect", "Observe", "Jump", "Coil", "Land",
            "Roll", "Catch", "Play", "Cuddle", "Dragged", "Zoom",
        }:
            return False
        if not self._is_fully_hidden(creature):
            try:
                dist_to_mouse = math.hypot(creature.x - self._mouse_x, creature.y - self._mouse_y)
            except Exception:
                dist_to_mouse = 999999.0
            safety = self._personality_float(creature, "desktop_hide_mouse_safety_radius", 260.0)
            if self._mouse_down:
                safety *= 1.7
            if dist_to_mouse < max(80.0, safety):
                return False
        return True

    def _surface_contains(self, surface: DesktopSurface, x: float, y: float, inset: float = 0.0) -> bool:
        return (surface.x + inset <= x <= surface.x + surface.w - inset
                and surface.y + inset <= y <= surface.y + surface.h - inset)

    def _surface_inside_depth(self, surface: DesktopSurface, x: float, y: float) -> float:
        if not self._surface_contains(surface, x, y):
            return 0.0
        return max(0.0, min(x - surface.x, surface.x + surface.w - x, y - surface.y, surface.y + surface.h - y))

    def _bbox_intersects_surface(self, bbox: Tuple[float, float, float, float], surface: DesktopSurface) -> bool:
        x0, y0, x1, y1 = bbox
        return not (x1 <= surface.x or x0 >= surface.x + surface.w or y1 <= surface.y or y0 >= surface.y + surface.h)

    def _bbox_inside_surface(self, bbox: Tuple[float, float, float, float], surface: DesktopSurface) -> bool:
        x0, y0, x1, y1 = bbox
        return (x0 >= surface.x and x1 <= surface.x + surface.w
                and y0 >= surface.y and y1 <= surface.y + surface.h)

    def _bbox_intersection(self, bbox: Tuple[float, float, float, float], surface: DesktopSurface) -> Tuple[float, float, float, float] | None:
        x0, y0, x1, y1 = bbox
        ix0 = max(x0, surface.x)
        iy0 = max(y0, surface.y)
        ix1 = min(x1, surface.x + surface.w)
        iy1 = min(y1, surface.y + surface.h)
        if ix1 <= ix0 or iy1 <= iy0:
            return None
        return ix0, iy0, ix1, iy1

    def _higher_surfaces(self, surface: DesktopSurface) -> List[DesktopSurface]:
        try:
            idx = self.desktop_surfaces.index(surface)
        except ValueError:
            return []
        return [s for s in self.desktop_surfaces[:idx] if self._surface_can_occlude(s)]

    def _point_hidden_by_higher_surface(self, surface: DesktopSurface, x: float, y: float) -> bool:
        return any(self._surface_contains(higher, x, y) for higher in self._higher_surfaces(surface))

    def _surface_visible_at_point(self, surface: DesktopSurface, x: float, y: float) -> bool:
        return self._surface_contains(surface, x, y) and not self._point_hidden_by_higher_surface(surface, x, y)

    def _bbox_has_visible_surface_area(self, surface: DesktopSurface, bbox: Tuple[float, float, float, float]) -> bool:
        overlap = self._bbox_intersection(bbox, surface)
        if overlap is None:
            return False
        x0, y0, x1, y1 = overlap
        samples = [
            ((x0 + x1) * 0.5, (y0 + y1) * 0.5),
            (x0 + 1.0, y0 + 1.0),
            (x1 - 1.0, y0 + 1.0),
            (x0 + 1.0, y1 - 1.0),
            (x1 - 1.0, y1 - 1.0),
        ]
        return any(self._surface_visible_at_point(surface, x, y) for x, y in samples)

    def _surface_visible_qregion(self, surface: DesktopSurface, QRect, QRegion):
        region = QRegion(QRect(
            int(math.floor(surface.x)),
            int(math.floor(surface.y)),
            max(1, int(math.ceil(surface.w))),
            max(1, int(math.ceil(surface.h))),
        ))
        for higher in self._higher_surfaces(surface):
            region -= QRegion(QRect(
                int(math.floor(higher.x)),
                int(math.floor(higher.y)),
                max(1, int(math.ceil(higher.w))),
                max(1, int(math.ceil(higher.h))),
            ))
        return region

    def _is_fully_hidden(self, creature: Creature) -> bool:
        return bool(getattr(creature, "_desktop_fully_hidden", False))

    def _surface_occlusion_is_allowed_for_creature(self, creature: Creature, surface: DesktopSurface) -> bool:
        """Return True only for a deliberate hide/portal target.

        Earlier builds clipped whenever a spider happened to cross any remembered
        window rectangle.  That made mouse chasing and lower covered windows look
        like invisible walls.  Now the manager must first set
        _desktop_hide_surface_key by choosing this exact top-visible surface as a
        hideout.
        """
        key = self._surface_key(surface)
        active_key = getattr(creature, "_desktop_hide_surface_key", None)
        if active_key != key:
            return False
        if not self._behavior_allows_desktop_hide(creature) and not self._is_fully_hidden(creature):
            self._clear_desktop_hide_intent(creature)
            return False
        bbox = creature.bounding_rect(False)
        if not self._bbox_intersects_surface(bbox, surface):
            # The spider deliberately left or missed the window; end this hide
            # attempt rather than leaving an invisible wall behind.
            if not self._surface_contains(surface, creature.x, creature.y):
                self._clear_desktop_hide_intent(creature)
            return False
        return self._bbox_has_visible_surface_area(surface, bbox)

    def _occluding_surfaces_for_creature(self, creature: Creature) -> List[DesktopSurface]:
        if creature.dragging or getattr(creature, "_desktop_spawn_grace", 0.0) > 0.0:
            return []
        active_key = getattr(creature, "_desktop_hide_surface_key", None)
        if active_key is None:
            return []
        surface = self._surface_by_key.get(active_key)
        if surface is None:
            # Window moved/closed or icon vanished; do not leave stale invisible walls.
            self._clear_desktop_hide_intent(creature)
            return []
        if not self._surface_can_occlude(surface):
            self._clear_desktop_hide_intent(creature)
            return []
        if self._surface_occlusion_is_allowed_for_creature(creature, surface):
            return [surface]
        return []

    def _creature_fully_covered_by_any_surface(self, creature: Creature, surfaces: List[DesktopSurface] | None = None) -> bool:
        if creature.dragging:
            return False
        surfaces = surfaces if surfaces is not None else self._occluding_surfaces_for_creature(creature)
        if not surfaces:
            return False
        bbox = creature.bounding_rect(False)
        for surface in surfaces:
            if not self._bbox_inside_surface(bbox, surface):
                continue
            # If a higher window covers part of this surface, this lower surface is
            # not allowed to make the whole spider invisible.
            if any(self._bbox_intersects_surface(bbox, higher) for higher in self._higher_surfaces(surface)):
                continue
            return True
        return False

    def _point_occluded_for_creature(self, creature: Creature, mx: float, my: float) -> bool:
        if creature.dragging:
            return False
        for surface in self._occluding_surfaces_for_creature(creature):
            if self._surface_visible_at_point(surface, mx, my):
                return True
        return False

    def _surface_key(self, surface: DesktopSurface) -> tuple:
        """Stable-enough key used to remember entry/exit across snapshots."""
        return (
            surface.kind,
            surface.class_name,
            int(round(surface.x / 8.0)),
            int(round(surface.y / 8.0)),
            int(round(surface.w / 8.0)),
            int(round(surface.h / 8.0)),
        )

    def _surface_can_occlude(self, surface: DesktopSurface) -> bool:
        """Return whether a window should be allowed to hide spiders.

        Fullscreen or nearly-fullscreen windows make a transparent overlay look
        broken: every spider may spawn "inside" the window rectangle and fade
        to nothing before the user sees it.  Treat those very large windows as
        background instead of hide-behind surfaces.  Smaller app/folder windows
        still behave like walls/cover.
        """
        screen_area = max(1.0, float(self.screen_w * self.screen_h))
        area_ratio = (surface.w * surface.h) / screen_area
        if area_ratio >= 0.86:
            return False
        if surface.w >= self.screen_w * 0.96 and surface.h >= self.screen_h * 0.82:
            return False
        if surface.w >= self.screen_w * 0.82 and surface.h >= self.screen_h * 0.96:
            return False
        return True

    def _distance_to_surface(self, surface: DesktopSurface, x: float, y: float) -> float:
        dx = max(surface.x - x, 0.0, x - (surface.x + surface.w))
        dy = max(surface.y - y, 0.0, y - (surface.y + surface.h))
        return math.hypot(dx, dy)

    def _containing_surface(self, creature: Creature, *, kind: str | None = None, min_depth: float = 0.0) -> DesktopSurface | None:
        best = None
        best_depth = -1.0
        for surface in self.desktop_surfaces:
            if kind is not None and surface.kind != kind:
                continue
            depth = self._surface_inside_depth(surface, creature.x, creature.y)
            if depth >= min_depth and depth > best_depth:
                best = surface
                best_depth = depth
        return best

    def _folder_surfaces(self) -> List[DesktopSurface]:
        return [surface for surface in self.desktop_surfaces
                if surface.kind == "folder"
                and self._surface_can_occlude(surface)
                and self._surface_visible_at_point(surface, surface.x + surface.w * 0.5, surface.y + surface.h * 0.5)]

    def _point_inside_surface(self, surface: DesktopSurface, creature: Creature) -> Tuple[float, float]:
        margin = max(18.0, min(80.0, creature.size * 1.2))
        if surface.w <= margin * 2.0 or surface.h <= margin * 2.0:
            return surface.x + surface.w * 0.5, surface.y + surface.h * 0.5
        return (
            self._rng.uniform(surface.x + margin, surface.x + surface.w - margin),
            self._rng.uniform(surface.y + margin, surface.y + surface.h - margin),
        )

    def _nearest_exit_target(self, creature: Creature, surface: DesktopSurface) -> Tuple[float, float]:
        left = abs(creature.x - surface.x)
        right = abs((surface.x + surface.w) - creature.x)
        top = abs(creature.y - surface.y)
        bottom = abs((surface.y + surface.h) - creature.y)
        side = min(((left, "left"), (right, "right"), (top, "top"), (bottom, "bottom")), key=lambda item: item[0])[1]
        push = max(60.0, creature.size * 2.8)
        if side == "left":
            return _clamp(surface.x - push, creature.margin, self.screen_w - creature.margin), _clamp(creature.y, creature.margin, self.screen_h - creature.margin)
        if side == "right":
            return _clamp(surface.x + surface.w + push, creature.margin, self.screen_w - creature.margin), _clamp(creature.y, creature.margin, self.screen_h - creature.margin)
        if side == "top":
            return _clamp(creature.x, creature.margin, self.screen_w - creature.margin), _clamp(surface.y - push, creature.margin, self.screen_h - creature.margin)
        return _clamp(creature.x, creature.margin, self.screen_w - creature.margin), _clamp(surface.y + surface.h + push, creature.margin, self.screen_h - creature.margin)

    def _maybe_seek_desktop_cover(self, creature: Creature, dt: float) -> None:
        if not self.desktop_surfaces or creature.dragging or creature.airborne or creature.cage is not None:
            self._clear_desktop_hide_intent(creature)
            return
        if self._is_fully_hidden(creature) or self._occluding_surfaces_for_creature(creature):
            return
        if not self._behavior_allows_desktop_hide(creature):
            # A visible spider that starts chasing the cursor or playing should not
            # accidentally disappear under a window it happens to cross.
            self._clear_desktop_hide_intent(creature)
            return

        # Let an active, visible hide attempt time out if the creature did not reach
        # the chosen window.
        if getattr(creature, "_desktop_hide_surface_key", None) is not None:
            creature._desktop_hide_timer = max(0.0, float(getattr(creature, "_desktop_hide_timer", 0.0)) - dt)
            if creature._desktop_hide_timer <= 0.0:
                self._clear_desktop_hide_intent(creature)
            else:
                return

        creature._desktop_seek_timer = max(0.0, getattr(creature, "_desktop_seek_timer", 0.0) - dt)
        if creature._desktop_seek_timer > 0.0:
            return
        creature._desktop_seek_timer = self._personality_range(creature, "desktop_hide_seek_interval", 34.0, 86.0)
        # Hideouts should be a rare, conscious action rather than constant random
        # window clipping.  Explorer-type personalities override this and
        # deliberately look for cover/folder portals much more often.
        if self._rng.random() > _clamp(self._personality_float(creature, "desktop_hide_chance", 0.055), 0.0, 1.0):
            return

        max_area_for_intentional_hide = self.screen_w * self.screen_h * self._personality_float(creature, "desktop_hide_max_area_ratio", 0.56)
        candidates = []
        folder_count = len(self._folder_surfaces())
        for surface in self.desktop_surfaces:
            if not self._surface_can_occlude(surface):
                continue
            if surface.w * surface.h > max_area_for_intentional_hide:
                continue
            if self._surface_contains(surface, creature.x, creature.y):
                continue
            if not self._surface_visible_at_point(surface, surface.x + surface.w * 0.5, surface.y + surface.h * 0.5):
                continue
            dist = self._distance_to_surface(surface, creature.x, creature.y)
            if dist >= self._personality_float(creature, "desktop_hide_max_distance", 420.0):
                continue
            # Folder portals only make sense with at least two Explorer folder
            # windows, but an explorer personality may still hide in a single
            # folder window until another portal exit exists.
            if surface.kind == "folder":
                folder_weight = self._personality_float(creature, "desktop_folder_weight", 0.34)
                if folder_count >= 2 and getattr(creature, "_folder_portal_cooldown", 0.0) <= 0.0:
                    weight = folder_weight
                elif creature.personality.get("folder_hider", False):
                    weight = max(0.18, folder_weight * 0.35)
                else:
                    continue
            elif surface.kind == "window":
                weight = self._personality_float(creature, "desktop_window_weight", 1.0)
            else:
                continue
            candidates.append((surface, max(0.01, weight)))
        if not candidates:
            return
        total = sum(weight for _, weight in candidates)
        pick = self._rng.random() * total
        surface = candidates[-1][0]
        for cand, weight in candidates:
            pick -= weight
            if pick <= 0.0:
                surface = cand
                break
        creature._desktop_hide_surface_key = self._surface_key(surface)
        creature._desktop_hide_kind = surface.kind
        creature._desktop_hide_timer = self._personality_range(creature, "desktop_hide_duration", 7.0, 12.0)
        creature.target_x, creature.target_y = self._point_inside_surface(surface, creature)
        creature.target_heading = math.atan2(creature.target_y - creature.y, creature.target_x - creature.x)
        creature.state = "Wander"
        creature.motion_paused = False
        creature.speed = 38.0 * creature._speed_mult() * self._personality_float(creature, "desktop_hide_speed_multiplier", 1.0)
        creature.state_timer = self._personality_range(creature, "desktop_hide_approach_time", 2.4, 4.8)

    def _desktop_occlusion_alpha(self, creature: Creature) -> float:
        # Whole-spider fading was replaced by strict edge clipping.  This method is
        # left as a backwards-safe hook for older render paths.
        return 1.0

    def _teleport_to_folder_exit(self, creature: Creature, source: DesktopSurface) -> bool:
        folders = [surface for surface in self._folder_surfaces() if surface != source]
        if not folders:
            return False
        dest = self._rng.choice(folders)
        side = self._rng.choice(("left", "right", "top", "bottom"))
        margin = max(24.0, creature.size * 1.1)
        exit_push = max(70.0, creature.size * 3.2)
        if side in ("left", "right"):
            y = self._rng.uniform(dest.y + margin, dest.y + max(margin, dest.h - margin)) if dest.h > margin * 2.0 else dest.y + dest.h * 0.5
            if side == "left":
                inside = (dest.x + margin, y)
                outside = (dest.x - exit_push, y + self._rng.uniform(-35.0, 35.0))
            else:
                inside = (dest.x + dest.w - margin, y)
                outside = (dest.x + dest.w + exit_push, y + self._rng.uniform(-35.0, 35.0))
        else:
            x = self._rng.uniform(dest.x + margin, dest.x + max(margin, dest.w - margin)) if dest.w > margin * 2.0 else dest.x + dest.w * 0.5
            if side == "top":
                inside = (x, dest.y + margin)
                outside = (x + self._rng.uniform(-45.0, 45.0), dest.y - exit_push)
            else:
                inside = (x, dest.y + dest.h - margin)
                outside = (x + self._rng.uniform(-45.0, 45.0), dest.y + dest.h + exit_push)
        inside_x = _clamp(inside[0], creature.margin * 0.4, self.screen_w - creature.margin * 0.4)
        inside_y = _clamp(inside[1], creature.margin * 0.4, self.screen_h - creature.margin * 0.4)
        outside_x = _clamp(outside[0], creature.margin, self.screen_w - creature.margin)
        outside_y = _clamp(outside[1], creature.margin, self.screen_h - creature.margin)

        dx = inside_x - creature.x
        dy = inside_y - creature.y
        creature.x = inside_x
        creature.y = inside_y
        creature.target_x = outside_x
        creature.target_y = outside_y
        creature.target_heading = math.atan2(outside_y - inside_y, outside_x - inside_x)
        creature.heading = creature.target_heading
        creature.state = "Wander"
        creature.motion_paused = False
        creature.speed = 54.0 * creature._speed_mult() * self._personality_float(creature, "desktop_hide_speed_multiplier", 1.0)
        creature.current_speed = min(creature.current_speed, creature.speed)
        creature.state_timer = self._personality_range(creature, "folder_portal_exit_time", 1.5, 2.8)
        creature.inertia_timer = 0.0
        creature.inertia_vx = 0.0
        creature.inertia_vy = 0.0
        creature._translate_leg_world_points(dx, dy, 1.0)
        # Mark the destination folder as the intentional occluder immediately.
        # The spider arrives behind that folder and crawls out through its exact
        # border instead of fading in as a whole.
        creature._desktop_hide_surface_key = self._surface_key(dest)
        creature._desktop_hide_kind = "folder"
        creature._desktop_hide_timer = self._personality_range(creature, "desktop_hide_duration", 6.0, 10.0)
        creature._desktop_visibility_alpha = 1.0
        creature._desktop_fully_hidden = True
        creature._desktop_hidden_timer = 0.0
        creature._folder_portal_dwell = 0.0
        creature._folder_portal_threshold = self._personality_range(creature, "folder_portal_dwell", 0.85, 1.39)
        creature._folder_portal_cooldown = self._personality_range(creature, "folder_portal_exit_cooldown", 38.0, 80.0)
        return True

    def _update_folder_portal(self, creature: Creature, dt: float) -> bool:
        creature._folder_portal_cooldown = max(0.0, getattr(creature, "_folder_portal_cooldown", 0.0) - dt)
        if creature.dragging or creature.airborne or creature.cage is not None:
            creature._folder_portal_dwell = 0.0
            return False
        folders = self._folder_surfaces()
        if len(folders) < 2 or creature._folder_portal_cooldown > 0.0:
            creature._folder_portal_dwell = max(0.0, getattr(creature, "_folder_portal_dwell", 0.0) - dt * 2.0)
            return False
        source = self._containing_surface(creature, kind="folder", min_depth=max(14.0, creature.size * 0.45))
        if source is None or not self._surface_visible_at_point(source, creature.x, creature.y):
            creature._folder_portal_dwell = max(0.0, getattr(creature, "_folder_portal_dwell", 0.0) - dt * 2.0)
            return False
        # Only a folder the spider intentionally chose may become a portal.
        if getattr(creature, "_desktop_hide_surface_key", None) != self._surface_key(source):
            creature._folder_portal_dwell = 0.0
            return False
        if getattr(creature, "_desktop_hide_kind", None) != "folder":
            creature._folder_portal_dwell = 0.0
            return False
        creature._folder_portal_dwell += dt
        threshold = float(getattr(creature, "_folder_portal_threshold", 0.85 + (creature.index % 4) * 0.18))
        if creature._folder_portal_dwell >= threshold:
            return self._teleport_to_folder_exit(creature, source)
        return False

    def _update_desktop_awareness(self, creature: Creature, dt: float) -> None:
        if not self.desktop_surfaces:
            creature._desktop_visibility_alpha = 1.0
            creature._desktop_fully_hidden = False
            creature._desktop_hidden_timer = 0.0
            self._clear_desktop_hide_intent(creature)
            return

        if getattr(creature, "_desktop_spawn_grace", 0.0) > 0.0:
            creature._desktop_spawn_grace = max(0.0, float(getattr(creature, "_desktop_spawn_grace", 0.0)) - dt)
            creature._desktop_visibility_alpha = 1.0
            creature._desktop_fully_hidden = False
            creature._desktop_hidden_timer = 0.0
            creature._folder_portal_dwell = 0.0
            return

        teleported = self._update_folder_portal(creature, dt)
        if teleported:
            return

        occluders = self._occluding_surfaces_for_creature(creature)
        fully_hidden = self._creature_fully_covered_by_any_surface(creature, occluders)
        creature._desktop_visibility_alpha = 1.0
        creature._desktop_fully_hidden = fully_hidden
        creature._desktop_occluding_keys = {self._surface_key(surface) for surface in occluders}

        if fully_hidden and not creature.dragging and creature.cage is None:
            creature.social_target = None
            creature._desktop_hidden_timer = getattr(creature, "_desktop_hidden_timer", 0.0) + dt
            # Do not let a spider stay lost behind a normal window forever; after
            # a brief hide it heads toward the nearest edge and crawls back out.
            if creature._desktop_hidden_timer > self._personality_float(creature, "desktop_hidden_linger", 1.8) and not creature.airborne:
                bbox = creature.bounding_rect(False)
                surface = next((s for s in occluders if self._bbox_inside_surface(bbox, s)), None) or self._containing_surface(creature)
                if surface is not None:
                    creature.target_x, creature.target_y = self._nearest_exit_target(creature, surface)
                    creature.target_heading = math.atan2(creature.target_y - creature.y, creature.target_x - creature.x)
                    creature.state = "Wander"
                    creature.motion_paused = False
                    creature.speed = 48.0 * creature._speed_mult()
                    creature.state_timer = self._rng.uniform(1.0, 2.0)
                    creature._desktop_hidden_timer = 0.0
        else:
            creature._desktop_hidden_timer = 0.0

    # ------------------------------------------------------------------
    # Naming / hover
    # ------------------------------------------------------------------
    def update_hover(self, mx: float, my: float) -> Optional[Creature]:
        """Mark the spider under the cursor as hovered (drives the name label)."""
        hovered = self.creature_at(mx, my)
        for creature in self.creatures:
            creature._hovered = creature is hovered
        return hovered

    def clear_hover(self) -> None:
        for creature in self.creatures:
            creature._hovered = False

    def set_naming_enabled(self, enabled: bool) -> str:
        self.naming_enabled = bool(enabled)
        if not self.naming_enabled:
            self.clear_hover()
        return f"Right-click naming turned {'on' if self.naming_enabled else 'off'}."

    def set_always_show_names(self, enabled: bool) -> str:
        self.always_show_names = bool(enabled)
        return f"Spider names {'always shown' if self.always_show_names else 'shown on hover only'}."

    def name_creature(self, creature: Creature, name: str) -> str:
        if creature is None:
            return "No spider there to name."
        creature.set_name(name)
        self.save_runtime_state()
        if creature.name:
            return f"Named this spider \u201c{creature.name}\u201d."
        return "Cleared this spider's name."

    def set_creature_level_pin(self, creature: Creature, enabled: bool) -> str:
        if creature is None:
            return "No spider there to update."
        creature.set_level_label_pinned(enabled)
        self.save_runtime_state()
        return f"Level display {'pinned above' if enabled else 'removed from'} this spider's name."

    def set_creature_health_pin(self, creature: Creature, enabled: bool) -> str:
        if creature is None:
            return "No spider there to update."
        creature.set_health_label_pinned(enabled)
        self.save_runtime_state()
        return f"Health bar {'pinned above' if enabled else 'removed from'} this spider."

    def set_creature_team(self, creature: Creature, team_id: str) -> str:
        if creature is None:
            return "No spider there to update."
        creature.set_team(team_id)
        self.save_runtime_state()
        return f"Assigned this spider to team {creature.progression.team_id!r}."

    def set_creature_relation(self, creature: Creature, other: Creature, relation: str) -> str:
        if creature is None or other is None or creature is other:
            return "No pair of spiders selected."
        relation = str(relation).strip().lower()
        if relation not in RELATIONS:
            return "Relation must be friend, neutral, or foe."
        left_id = str(getattr(other, "progression_id", other.index))
        right_id = str(getattr(creature, "progression_id", creature.index))
        # Store pair choices in both directions so UI and future combat checks
        # cannot disagree about who is a friend or foe.
        creature.progression.relation_overrides[left_id] = relation
        other.progression.relation_overrides[right_id] = relation
        self.save_runtime_state()
        return f"Set {creature.display_name} and {other.display_name} to {relation}."

    def equip_creature_item(self, creature: Creature, item_id: str) -> str:
        if creature is None:
            return "No spider there to equip."
        ok, message = creature.equip_item(item_id)
        if ok:
            self.save_runtime_state()
        return message

    def unequip_creature_item(self, creature: Creature, slot: str) -> str:
        if creature is None:
            return "No spider there to unequip."
        ok, message = creature.unequip_item(slot)
        if ok:
            self.save_runtime_state()
        return message

    def unlock_creature_ability(self, creature: Creature, ability_id: str) -> str:
        if creature is None:
            return "No spider there to unlock."
        ok, message = creature.unlock_progression_ability(ability_id)
        if ok:
            self.save_runtime_state()
        return message
