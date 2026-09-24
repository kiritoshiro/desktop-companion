"""Rules for the runtime state file: identity, migration and eviction.

The state file holds what should outlive one session -- names, levels,
inventory, teams, relations and colony bases -- keyed by a preset-scoped
creature id.  This module owns the rules about those keys and is deliberately
free of Qt, ``Creature`` and filesystem access, so they can be exercised
headlessly and reasoned about on their own.

Two defects motivated it.  The namespace came from the preset filename stem
without folding case, and Windows paths are case-insensitive, so one spider
could accumulate two separate saved profiles.  Nothing ever removed an entry,
so every spider that had ever existed under any preset was rewritten on every
save, forever.
"""

from __future__ import annotations

import json
import uuid
from pathlib import Path
from typing import Any, Callable


# Bump when the on-disk shape changes.  Versions 1 and 2 are migrated on load.
STATE_SCHEMA_VERSION = 3

# An entry not seen for this many launches is dropped on the next save.  It is
# deliberately generous: carrying a few stale entries costs a little disk,
# while dropping one too early silently deletes a spider's level and name.
RETAIN_LAUNCHES = 50

DEFAULT_NAMESPACE = "default"


def normalize_namespace(value: Any) -> str:
    """Return the case-insensitive namespace for a preset.

    The namespace is the preset's filename stem.  Windows treats
    ``Default.json`` and ``default.json`` as the same file, so they must map to
    the same saved profile; before this was folded they did not.
    """
    text = str(value or "").strip().casefold()
    return text or DEFAULT_NAMESPACE


def normalize_state_key(key: Any) -> str | None:
    """Return the canonical key for one saved creature, or ``None`` to drop it.

    A key is ``<namespace>|<slot or runtime id>``.  Keys carrying no namespace
    come from a scheme that predated preset scoping.  They cannot be attributed
    to any preset, so they are dropped rather than handed to whichever spider
    happens to land on the same index.
    """
    text = str(key or "")
    if "|" not in text:
        return None
    namespace, rest = text.split("|", 1)
    if not rest:
        return None
    return f"{normalize_namespace(namespace)}|{rest}"


def new_creature_id(namespace: Any, rng: Any = None) -> str:
    """Mint an id for a spider that has no saved profile yet.

    Identity is generated once and then belongs to the spider.  Deriving it
    from a slot's member index instead meant that changing a slot's ``count``
    handed one spider's level and name to whichever spider next landed on that
    index.  ``rng`` lets a seeded run mint reproducible ids.
    """
    token = uuid.uuid4().hex[:12] if rng is None else f"{rng.getrandbits(48):012x}"
    return f"{normalize_namespace(namespace)}|{token}"


def slot_id_from_key(key: Any) -> str:
    """Return the slot a version 2 key belonged to.

    Version 2 keys were ``<namespace>|<slot id>:<member index>``.  The slot is
    everything before the final colon; a key without one names its own slot.
    """
    text = str(key or "")
    rest = text.split("|", 1)[1] if "|" in text else text
    head, sep, tail = rest.rpartition(":")
    if sep and head and tail.isdigit():
        return head
    return rest


def member_index_from_key(key: Any) -> int | None:
    """Return the member index a version 2 key carried, if it had one.

    Migration uses it to keep a slot's existing spiders in the order they were
    already in, rather than reordering a colony the first time it is loaded
    under the generated-id scheme.
    """
    text = str(key or "")
    rest = text.split("|", 1)[1] if "|" in text else text
    head, sep, tail = rest.rpartition(":")
    if sep and head and tail.isdigit():
        return int(tail)
    return None


def next_birth_ordinal(states: Any) -> int:
    """Return the next unused birth ordinal across every saved entry."""
    highest = -1
    if isinstance(states, dict):
        for entry in states.values():
            if not isinstance(entry, dict):
                continue
            try:
                highest = max(highest, int(entry.get("born", -1)))
            except (TypeError, ValueError):
                continue
    return highest + 1


def entry_rank(entry: Any) -> tuple[int, int]:
    """Rank a saved entry so a key collision keeps the more advanced spider.

    Folding case can map two stored entries onto one key.  Rather than letting
    dictionary order decide, keep the higher level and then the higher lifetime
    XP, which is the copy a player would be sorry to lose.
    """
    if not isinstance(entry, dict):
        return (0, 0)
    progression = entry.get("progression")
    if not isinstance(progression, dict):
        # Older entries stored the progression fields at the top level.
        progression = entry
    try:
        level = int(progression.get("level", 1))
    except (TypeError, ValueError):
        level = 1
    try:
        total_xp = int(progression.get("total_xp", 0))
    except (TypeError, ValueError):
        total_xp = 0
    return (level, total_xp)


def next_launch(data: Any) -> int:
    """Return the launch counter for this session, one past the stored one."""
    if not isinstance(data, dict):
        return 1
    try:
        return max(0, int(data.get("launch", 0))) + 1
    except (TypeError, ValueError):
        return 1


def migrate_creatures(raw: Any, launch: int) -> dict:
    """Canonicalise saved creature entries and stamp any that lack a sighting.

    Entries are copied rather than mutated in place, so a caller can migrate a
    payload it does not own.  ``last_seen`` is only defaulted, never advanced:
    advancing it is a save-time decision, made in :func:`stamp_seen`.
    """
    if not isinstance(raw, dict):
        return {}
    merged: dict = {}
    for key, entry in raw.items():
        canonical = normalize_state_key(key)
        if canonical is None or not isinstance(entry, dict):
            continue
        existing = merged.get(canonical)
        if existing is not None and entry_rank(existing) >= entry_rank(entry):
            continue
        merged[canonical] = dict(entry)
    for key, entry in merged.items():
        if not isinstance(entry.get("last_seen"), int):
            # Migrated entries count as seen now, so a schema change never
            # makes something instantly evictable.
            entry["last_seen"] = int(launch)
        if not str(entry.get("slot_id", "")).strip():
            # Version 2 carried the slot inside the key. Recovering it here is
            # what lets an existing saved spider keep its level and name when
            # identity stops being derived from a slot member index.
            entry["slot_id"] = slot_id_from_key(key)
        if not isinstance(entry.get("born"), int):
            # ``born`` orders a slot's members, so it has to be the order they
            # were created in and not the launch they appeared in: spiders
            # born in the same launch would otherwise tie and be re-sorted by
            # the random text of their generated ids on every reload. Version
            # 2 keys carry that order already, in the member index.
            index = member_index_from_key(key)
            entry["born"] = int(index) if index is not None else 0
    return merged


def saved_slot_members(states: Any, namespace: Any, slot_id: Any) -> list[str]:
    """Return the saved ids for one preset slot, oldest spider first.

    Birth order is what makes reuse stable: raising a slot's ``count`` appends
    new spiders and lowering it drops the youngest, while everyone already
    saved keeps the profile they had.
    """
    if not isinstance(states, dict):
        return []
    prefix = f"{normalize_namespace(namespace)}|"
    wanted = str(slot_id)
    found: list[tuple[int, str]] = []
    for key, entry in states.items():
        if not isinstance(entry, dict) or not str(key).startswith(prefix):
            continue
        if str(entry.get("slot_id", "")) != wanted:
            continue
        try:
            born = int(entry.get("born", 0))
        except (TypeError, ValueError):
            born = 0
        found.append((born, str(key)))
    found.sort()
    return [key for _born, key in found]


def slot_member_ids(states: Any, namespace: Any, slot_id: Any, count: int,
                    mint: Callable[[], str]) -> list[str]:
    """Return ``count`` stable ids for one slot, reusing saved spiders first."""
    wanted = max(0, int(count))
    ids = saved_slot_members(states, namespace, slot_id)[:wanted]
    taken = set(ids)
    while len(ids) < wanted:
        candidate = str(mint())
        if candidate in taken:
            continue
        taken.add(candidate)
        ids.append(candidate)
    return ids


def load_payload(data: Any, launch: int) -> tuple[dict, list]:
    """Return ``(creatures, bases)`` from any supported on-disk payload."""
    if not isinstance(data, dict):
        return {}, []
    bases = data.get("bases")
    if not isinstance(bases, list):
        bases = []
    return migrate_creatures(data.get("creatures"), launch), bases


def load_scene(data: Any) -> dict:
    """Return the saved scene -- cages, webs and fly nests -- from a payload.

    Always returns the three lists, so a caller never has to distinguish "no
    scene saved" (a file written before version 3) from "an empty desktop".
    Each list is handed back untouched for the world that owns that object to
    validate, since only it knows what a usable entry looks like.
    """
    scene = data.get("scene") if isinstance(data, dict) else None
    if not isinstance(scene, dict):
        scene = {}
    return {
        key: scene[key] if isinstance(scene.get(key), list) else []
        for key in ("cages", "webs", "nests")
    }


def stamp_seen(entry: dict, launch: int) -> dict:
    """Mark one entry as present in this launch."""
    entry = dict(entry)
    entry["last_seen"] = int(launch)
    return entry


def evict(states: Any, launch: int, retain: int = RETAIN_LAUNCHES) -> dict:
    """Drop entries not seen for more than ``retain`` launches."""
    if not isinstance(states, dict):
        return {}
    horizon = max(0, int(retain))
    kept: dict = {}
    for key, entry in states.items():
        if not isinstance(entry, dict):
            continue
        try:
            last_seen = int(entry.get("last_seen", launch))
        except (TypeError, ValueError):
            last_seen = int(launch)
        if int(launch) - last_seen <= horizon:
            kept[key] = entry
    return kept


def reset_saved_progress(path) -> bool:
    """Forget every spider's saved stats and every base in a state file (DC-85).

    For when no overlay is running to do it itself. The scene -- cages, webs
    and nests the player placed -- and the launch counter are kept. Written
    atomically, like a save. True if the file was written.
    """
    path = Path(path)
    try:
        with path.open("r", encoding="utf-8") as handle:
            data = json.load(handle)
    except FileNotFoundError:
        return True
    except (OSError, ValueError, TypeError):
        data = {}
    if not isinstance(data, dict):
        data = {}
    launch = int(data.get("launch", 1) or 1)
    payload = build_payload({}, [], launch, data.get("scene"))
    temp = path.with_suffix(".tmp")
    try:
        with temp.open("w", encoding="utf-8") as handle:
            json.dump(payload, handle, indent=2)
            handle.write("\n")
        temp.replace(path)
        return True
    except OSError:
        return False


def build_payload(states: dict, bases: list, launch: int,
                  scene: dict | None = None) -> dict:
    """Assemble the on-disk payload for a save."""
    scene = scene if isinstance(scene, dict) else {}
    return {
        "schema_version": STATE_SCHEMA_VERSION,
        "launch": int(launch),
        "creatures": states,
        "bases": list(bases or []),
        "scene": {
            key: list(scene.get(key) or [])
            for key in ("cages", "webs", "nests")
        },
    }
