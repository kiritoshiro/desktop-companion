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

from typing import Any


# Bump when the on-disk shape changes.  Version 1 files are migrated on load.
STATE_SCHEMA_VERSION = 2

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
    for entry in merged.values():
        if not isinstance(entry.get("last_seen"), int):
            # Migrated entries count as seen now, so a schema change never
            # makes something instantly evictable.
            entry["last_seen"] = int(launch)
    return merged


def load_payload(data: Any, launch: int) -> tuple[dict, list]:
    """Return ``(creatures, bases)`` from any supported on-disk payload."""
    if not isinstance(data, dict):
        return {}, []
    bases = data.get("bases")
    if not isinstance(bases, list):
        bases = []
    return migrate_creatures(data.get("creatures"), launch), bases


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


def build_payload(states: dict, bases: list, launch: int) -> dict:
    """Assemble the on-disk payload for a save."""
    return {
        "schema_version": STATE_SCHEMA_VERSION,
        "launch": int(launch),
        "creatures": states,
        "bases": list(bases or []),
    }
