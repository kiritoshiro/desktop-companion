"""Reading and writing the runtime state file: identity, the saved
    scene, and the periodic flush.

Split out of the single ``manager.py`` by DC-43; a pure move.
"""

from __future__ import annotations

import json

from ..world.cage import Cage
from ..state.runtime_state import (
    build_payload,
    evict,
    load_payload,
    load_scene,
    new_creature_id,
    next_birth_ordinal,
    next_launch,
    slot_member_ids,
    stamp_seen,
)


from .constants import (
    RUNTIME_STATE_FLUSH_SECONDS,
    log,
)


class RuntimeStateMixin:
    """Reading and writing the runtime state file: identity, the saved"""

    def _mint_progression_id(self) -> str:
        """Return a fresh preset-scoped id for a spider with no saved profile.

        Identity is generated once and then belongs to that spider. Deriving it
        from a list position meant a spider inherited whatever progression the
        previous occupant of that index had left behind, so raising a slot's
        count or randomizing the colony reshuffled levels and names. Ids come
        from the dedicated stream set up in ``__init__`` so a replayed run
        mints the same ones without disturbing the simulation's own draws.
        """
        return new_creature_id(self._progression_namespace, self._id_rng)

    def _slot_member_ids(self, slot_id: str, count: int) -> list[str]:
        """Return stable ids for one preset slot, reusing saved spiders first."""
        return slot_member_ids(
            self._progression_states,
            self._progression_namespace,
            slot_id,
            count,
            self._mint_progression_id,
        )

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
        self._scene_runtime_state = load_scene(data)
        return states

    def _scene_to_dict(self) -> dict:
        """Serialise the things the player arranged: cages, webs and nests."""
        fly_world = getattr(self, "fly_world", None)
        web_world = getattr(self, "web_world", None)
        return {
            "cages": [cage.to_dict() for cage in self.cages],
            "webs": web_world.to_dict() if web_world is not None else [],
            "nests": fly_world.spawners_to_dict() if fly_world is not None else [],
        }

    def _restore_scene(self) -> None:
        """Put back the cages, webs and nests the last session left behind.

        Called once the preset has built the colony, because loading a preset
        clears both worlds.  A preset describes which spiders exist; the scene
        is what the player then did with the desktop, and only the state file
        knows that.
        """
        scene = getattr(self, "_scene_runtime_state", None)
        if not isinstance(scene, dict):
            return
        for entry in scene.get("cages") or []:
            cage = Cage.from_dict(entry)
            if cage is None:
                continue
            cage.clamp_to_screen(self.screen_w, self.screen_h)
            self.cages.append(cage)
        if self.cages:
            # Membership is geometric, the same rule a reshuffle already uses:
            # a spider standing inside a restored cage is back in that cage.
            for creature in self.creatures:
                self._settle_creature_membership(creature)
        if getattr(self, "web_world", None) is not None:
            self.web_world.restore(scene.get("webs"))
        # Only where the preset actually uses nests: the preset owns that
        # switch, and a saved nest must not turn the mechanic back on.
        nests = scene.get("nests") or []
        if nests and self.flies_spawner and getattr(self, "fly_world", None) is not None:
            self.fly_world.restore_spawners(nests)

    def save_runtime_state(self) -> None:
        """Persist meaningful creature state atomically beside the project/exe."""
        states = dict(self._progression_states)
        launch = int(getattr(self, "_state_launch", 1))
        # Spiders saved for the first time are numbered in the order they were
        # created, which is the order their slot will hand them back in.
        ordinal = next_birth_ordinal(states)
        for creature in self.creatures:
            key = str(getattr(creature, "progression_id", "") or self._mint_progression_id())
            previous = states.get(key)
            born = None
            if isinstance(previous, dict):
                try:
                    born = int(previous["born"])
                except (TypeError, ValueError, KeyError):
                    born = None
            if born is None:
                born = ordinal
                ordinal += 1
            states[key] = stamp_seen({
                "name": creature.name,
                "model": creature.model.get("id"),
                "personality": creature.personality.get("id"),
                "progression": creature.progression.to_dict(),
                "slot_id": str(getattr(creature, "progression_slot_id", "") or ""),
                "born": born,
            }, launch)
        # Retire entries for spiders that have not appeared for a long time, so
        # the file does not grow without bound across presets and slot edits.
        states = evict(states, launch)
        bases = self.base_world.to_dict() if getattr(self, "base_world", None) is not None else []
        payload = build_payload(states, bases, launch, self._scene_to_dict())
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

