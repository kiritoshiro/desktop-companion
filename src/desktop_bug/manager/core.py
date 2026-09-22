from __future__ import annotations

import math
import random
import sys
from pathlib import Path
from typing import List, Optional, Tuple

from ..content.personality_profiles import personality_gait_style
from ..creature import Creature, GAIT_LABELS, normalize_gait_style
from ..world.cage import Cage
from ..world.webs import WebWorld
from ..world.mouse_webs import MouseWebWorld
from ..world.flies import FlyWorld
from ..content.discovery import app_root, discover_models, discover_personalities, state_dir
from ..content.preset_io import load_preset
from ..content.skills import (
    DEFAULT_SKILL_IDS,
    normalize_skill_ids,
    unknown_skill_ids,
    SKILL_BY_ID,
    skills_with_default_abilities,
    skills_with_selected_abilities,
)
from ..world.desktop_environment import DesktopSurface
from ..state.progression import normalize_team_stances
from ..state.teams import normalize_teams, teams_payload
from ..world.jobs import BaseWorld, job_ability_ids, normalize_job_id
from ..content.personality_profiles import COMPACT_TEMPERAMENT_IDS
from ..support.profiling import get_profiler
from ..state.runtime_state import (
    normalize_namespace,
)


from .persistence import RuntimeStateMixin
from .cages import CageMixin
from .combat import CombatMixin
from .prey import PreyMixin
from .hunting import HuntingMixin
from .surfaces import DesktopSurfaceMixin
from .naming import NamingMixin
from .constants import (
    RANDOM_MODEL_ID,
    RANDOM_PERSONALITY_ID,
    _clamp,
)


class CreatureManager(
    RuntimeStateMixin,
    CageMixin,
    CombatMixin,
    PreyMixin,
    HuntingMixin,
    DesktopSurfaceMixin,
    NamingMixin,
):
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
        # Minting creature ids draws from its own stream, not the simulation's.
        # Sharing one stream made a seeded run's scene depend on how many ids
        # the save file left to mint, so the same seed laid spiders out
        # differently on a second launch -- exactly what seeding exists to
        # prevent. A string seed derives reproducibly across processes.
        self._id_rng = None if seed is None else random.Random(f"{seed}:creature-ids")
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
        # DC-52: on by default and no longer shown anywhere. This was a
        # master switch over a `social_play` skill that every temperament
        # already carries, weighted by its sociability -- 0 for a hunter, 10
        # for a cuddly one -- so all it could do was make a sociable spider
        # antisocial. The temperament decides; an old preset that set it to
        # false is still honoured.
        self.social_play = True
        # DC-22: conflict is on by default. The audience for it is the user
        # who watches these spiders fight for their lands, so shipping it off
        # would hide the thing it exists for -- see the decision record. The
        # switch stays because turning it off must make damage impossible,
        # not merely unlikely.
        self.conflict_enabled = True
        # DC-47: remains of spiders that lost, being eaten and on their way out.
        self.carcasses: List = []
        # None means "ask each temperament", which is what a preset saved by
        # the settings window now does. A preset that names a gait still
        # overrides every spider, so an old one keeps behaving as it did.
        self.gait_style = None
        # Declared stances between teams, shared by every spider in the scene.
        self.team_stances: dict = {}
        # Who each team is: the name its owner chose and its colour. Keyed
        # by the same case-folded id the stances and the slots use.
        self.team_profiles: dict = {}
        # Right-click naming and the hover/always-on name label.
        self.naming_enabled = True
        self.always_show_names = False
        # Scene-wide "show everyone's level / health" switches, pushed onto
        # each creature by _apply_label_overrides. Not saved per spider, so a
        # deliberate per-spider pin survives these being switched off.
        self.always_show_levels = False
        self.always_show_health = False
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
        # The base being carried by the pointer, and where on it the pointer
        # took hold. DC-53: a base can be dragged as well as moved through
        # the right-click menu.
        self._dragged_base = None
        self._base_drag_offset = (0.0, 0.0)
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
        # After the preset, never before: loading one clears the web world and
        # the cage list that the saved scene is about to refill.
        self._restore_scene()

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
        slot_id: str | None = None,
    ) -> Creature:
        state_key = str(progression_id or self._mint_progression_id())
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
            gait_style=self.gait_style or personality_gait_style(personality),
            color_overrides=color_overrides,
            progression_state=progression_state,
            progression_id=state_key,
            job_id=normalize_job_id(job_id),
            seed=self.seed,
        )
        # Remembered so a save can record which slot this spider belongs to,
        # which is what lets the next launch hand it back its own profile.
        creature.progression_slot_id = str(
            slot_id if slot_id is not None else (stored.get("slot_id", "") if isinstance(stored, dict) else "")
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
        # A spider born into a scene that is already showing every level or
        # health bar has to join it, rather than being the one that is missing.
        creature.force_show_level = getattr(self, "always_show_levels", False)
        creature.force_show_health = getattr(self, "always_show_health", False)
        creature.web_world = self.web_world
        creature.mouse_web_world = self.mouse_web_world
        creature.fly_world = self.fly_world
        creature.base_world = getattr(self, "base_world", None)
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
        self._dragged_base = None
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
            self.conflict_enabled = bool(settings.get("conflict", self.conflict_enabled))
            if settings.get("gait_style") is not None:
                self.gait_style = normalize_gait_style(settings["gait_style"])
            for key in ("always_show_names", "always_show_levels", "always_show_health"):
                if key in settings:
                    setattr(self, key, bool(settings[key]))
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
            # Saved spiders for this slot come back in the order they were
            # born, so changing a slot's count appends or drops at the tail
            # instead of reshuffling everyone's level and name.
            member_ids = self._slot_member_ids(slot_id, count)
            for member_index in range(count):
                creature = self._create_creature(
                    model,
                    personality,
                    index,
                    skills=skills,
                    color_overrides=slot.get("colors"),
                    progression_id=member_ids[member_index],
                    team_id=slot.get("team_id", slot.get("team", "neutral")),
                    job_id=job_id,
                    slot_id=slot_id,
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
                progression_id=self._slot_member_ids("fallback", 1)[0],
                slot_id="fallback",
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
        if self._dragged_base is not None:
            return True
        if (self.interferable or self.naming_enabled) and self.creature_at(mx, my) is not None:
            return True
        # Flies and the movable nest are grabbable when interaction is enabled.
        if self.interferable and (self.fly_world.hit_fly_at(mx, my) is not None
                                  or self.fly_world.hit_spawner_at(mx, my) is not None):
            return True
        # The dug earth of a base, so it can be picked up and carried. Only
        # the earth: see `base_grab_at` for why this is not the site radius.
        if self.base_grab_at(mx, my) is not None:
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

    def set_conflict_enabled(self, enabled: bool) -> str:
        """Turn conflict on or off (DC-22).

        Turning it off stops new fights immediately, but leaves anyone
        currently knocked out to recover normally rather than stranding them.
        """
        self.conflict_enabled = bool(enabled)
        state = "on" if self.conflict_enabled else "off"
        return f"Conflict turned {state}."

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
        old_progression_ids = [getattr(c, "progression_id", "") for c in self.creatures]
        old_slot_ids = [str(getattr(c, "progression_slot_id", "") or "") for c in self.creatures]
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
            # A spider kept in place keeps its own identity; anything beyond
            # the previous colony is genuinely new and is minted one.
            reuse = keep_positions and index < len(old_progression_ids) and old_progression_ids[index]
            progression_id = old_progression_ids[index] if reuse else self._mint_progression_id()
            slot_id = old_slot_ids[index] if reuse else ""
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
                slot_id=slot_id,
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
                    progression_id=self._slot_member_ids("fallback", 1)[0],
                    slot_id="fallback",
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
            if "conflict" in settings:
                self.set_conflict_enabled(bool(settings.get("conflict")))
            if "gait_style" in settings:
                self.set_gait_style(str(settings.get("gait_style") or "classic"))
            # DC-52: the settings window's three "Always show" switches, so
            # ticking one reaches a running overlay the same way the size
            # slider does instead of waiting for a relaunch.
            for key, setter in (("always_show_names", self.set_always_show_names),
                                ("always_show_levels", self.set_always_show_levels),
                                ("always_show_health", self.set_always_show_health)):
                if key in settings:
                    setter(bool(settings[key]))
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
            "always_show_names": self.always_show_names,
            "always_show_levels": self.always_show_levels,
            "always_show_health": self.always_show_health,
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
                handled = self._start_cage_drag(mx, my)
            # 4) A base is last, because it is the largest thing on screen and
            #    spiders stand on top of their own: grabbing the earth must
            #    never take priority over grabbing the spider standing on it.
            if not handled and self.interferable:
                handled = self.start_base_drag(mx, my)

        if mouse_down and self._cage_drag is not None:
            self._drag_cage(mx, my)
        elif self.interferable and mouse_down and self.dragged_creature is not None:
            self.dragged_creature.drag_to(dt, mx, my)
        elif mouse_down and self._dragged_fly is not None:
            self._dragged_fly.drag_to(mx, my)
        elif mouse_down and self._dragged_spawner is not None:
            self._dragged_spawner.drag_to(mx, my, self.screen_w, self.screen_h)
        elif mouse_down and self._dragged_base is not None:
            self.drag_base_to(mx, my)

        if mouse_released and self._cage_drag is not None:
            self._cage_drag = None

        if mouse_released and self._dragged_fly is not None:
            self._dragged_fly.release_drag(mx, my)
            self._dragged_fly = None
        if mouse_released and self._dragged_spawner is not None:
            self._dragged_spawner.release_drag()
            self._dragged_spawner = None
        if (mouse_released or not self.interferable) and self._dragged_base is not None:
            self.release_base_drag()

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

        # DC-22: foes that are touching trade blows, and anyone whose
        # knock-out has run out gets back up. After movement, so contact is
        # judged on where the spiders actually ended this frame.
        with profiler.section("combat"):
            self._resolve_combat(dt)

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
            # DC-47: remains lie on the ground, under everything living.
            self.render_carcasses(painter)
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

