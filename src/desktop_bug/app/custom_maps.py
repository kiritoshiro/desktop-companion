"""Maps the player makes in the map editor.

The owner: *"create editor tool. so i could create my self the map. with
spiders/bosses, buildings and the settings in the buildings and spider
enemies armor and everything else."*

A custom map is one JSON file in ``<state>/maps/``. It says, in the same
terms the built-in maps use (map_layouts.py, campaign.py):

- the map: title, one line, tier, which armour qualities random enemies
  wear, the companion it rewards, the spider cap, seconds between the
  Hatchery's reinforcements;
- every building: kind, name, place (fractions of the screen, or on the
  far screen), who holds it at the start, Hatchery reserves, healing supply,
  and its guards;
- enemies standing on their own, anywhere;
- the boss: name, kind, armour, strength.

A spider (guard, enemy or boss) is a **spec**: its role (how it fights), its
kind ("auto": the map picks), a level above the map's, a health multiplier,
and its armour -- "auto" for the map's rules, or exactly the pieces listed,
at an item level. Pure data, no Qt.
"""
from __future__ import annotations

import copy
import json
import re
from pathlib import Path

from ..content.discovery import state_dir
from ..content.enemy_kinds import ENEMY_KINDS
from ..state.progression import ARMOR_BY_ID, ARMOR_SETS, ARMOR_TIERS, MAX_ITEM_LEVEL
from .campaign import COMPANION_BY_ID, MapInfo
from .map_layouts import LAYOUTS, Placement

PREFIX = "custom-"
ROLES = ("guard", "weaver", "hunter", "spitter")
BUILDING_KINDS = ("home", "food", "silk", "hatchery", "nest", "venom", "lookout", "amber", "nursery")
BUILDING_NAMES = {"home": "Home burrow", "food": "Food cache", "silk": "Silk loom", "hatchery": "Hatchery",
                  "nest": "Enemy nest", "venom": "Venom den", "lookout": "Lookout", "amber": "Amber mine",
                  "nursery": "Nursery"}
# Exactly one of each; a map without them cannot be won or has no start.
REQUIRED = ("home", "hatchery", "nest")


def maps_dir() -> Path:
    return state_dir() / "maps"


def is_custom(map_id) -> bool:
    return isinstance(map_id, str) and map_id.startswith(PREFIX)


# -- specs ---------------------------------------------------------------------

def spider_spec(role="guard", **extra) -> dict:
    spec = {"role": role, "kind": "auto", "level_bonus": 0, "hp": 1.0, "armor": "auto", "item_level": 1}
    spec.update(extra)
    return spec


def clean_spec(raw, boss=False) -> dict:
    raw = raw if isinstance(raw, dict) else {}
    spec = spider_spec()
    role = raw.get("role", "guard")
    spec["role"] = "guardian" if boss else (role if role in ROLES else "guard")
    kind = raw.get("kind", "auto")
    spec["kind"] = kind if kind in ENEMY_KINDS else "auto"
    spec["level_bonus"] = _int(raw.get("level_bonus", 0), -5, 20)
    spec["hp"] = _float(raw.get("hp", 1.0), 0.2, 10.0)
    armor = raw.get("armor", "auto")
    if isinstance(armor, list):
        pieces, slots = [], set()
        for item in armor:
            if item in ARMOR_BY_ID and ARMOR_BY_ID[item].slot not in slots:
                pieces.append(item)
                slots.add(ARMOR_BY_ID[item].slot)
        spec["armor"] = pieces
    else:
        spec["armor"] = "auto"
    spec["item_level"] = _int(raw.get("item_level", 1), 1, MAX_ITEM_LEVEL)
    if boss:
        spec["name"] = str(raw.get("name") or "Guardian")[:40]
    return spec


# -- maps ------------------------------------------------------------------------

def new_map(title="My map") -> dict:
    """A playable starting point: the first map's buildings and guards."""
    buildings = []
    for place in LAYOUTS["territory"]:
        if place.far:
            continue
        buildings.append({"kind": place.kind, "name": place.name, "fx": place.fx, "fy": place.fy,
                          "far": False, "owned": place.owned, "reserves": place.reserves,
                          "supply": 180.0, "guards": [spider_spec(r) for r in place.guards]})
    return clean_map({"id": "", "title": title, "blurb": "A map of your own.", "tier": 1,
                      "enemy_tiers": ["common", "uncommon"], "buildings": buildings, "enemies": [],
                      "boss": spider_spec("guardian", name="Guardian", armor=list(ARMOR_SETS["forager"].pieces),
                                          item_level=2)})


def clean_map(raw) -> dict:
    raw = raw if isinstance(raw, dict) else {}
    data = {
        "id": str(raw.get("id") or ""),
        "title": (str(raw.get("title") or "My map").strip() or "My map")[:60],
        "blurb": str(raw.get("blurb") or "")[:200],
        "tier": _int(raw.get("tier", 1), 1, 6),
        "enemy_tiers": [t for t in (raw.get("enemy_tiers") or []) if t in ARMOR_TIERS] or ["common"],
        "reward_companion": raw.get("reward_companion") if raw.get("reward_companion") in COMPANION_BY_ID else None,
        "cap": _int(raw.get("cap", 12), 4, 30),
        "wave_every": _float(raw.get("wave_every", 24.0), 5.0, 300.0),
        "buildings": [],
        "enemies": [],
        "boss": clean_spec(raw.get("boss"), boss=True),
    }
    for b in raw.get("buildings") or []:
        if not isinstance(b, dict) or b.get("kind") not in BUILDING_KINDS:
            continue
        data["buildings"].append({
            "kind": b["kind"],
            "name": (str(b.get("name") or BUILDING_NAMES[b["kind"]]).strip() or BUILDING_NAMES[b["kind"]])[:40],
            "fx": _float(b.get("fx", 0.5), 0.0, 1.0),
            "fy": _float(b.get("fy", 0.5), 0.0, 1.0),
            "far": bool(b.get("far", False)),
            "owned": bool(b.get("owned", b["kind"] == "home")),
            "reserves": _int(b.get("reserves", 6 if b["kind"] == "hatchery" else 0), 0, 50),
            "supply": _float(b.get("supply", 180.0), 0.0, 5000.0),
            "guards": [clean_spec(g) for g in (b.get("guards") or [])][:8],
        })
    for e in raw.get("enemies") or []:
        if isinstance(e, dict):
            spec = clean_spec(e)
            spec.update({"fx": _float(e.get("fx", 0.5), 0.0, 1.0), "fy": _float(e.get("fy", 0.5), 0.0, 1.0),
                         "far": bool(e.get("far", False))})
            data["enemies"].append(spec)
    data["enemies"] = data["enemies"][:20]
    return data


def problems(data) -> list[str]:
    """What stops a map being played, in words; empty when it is fine."""
    found = []
    kinds = [b["kind"] for b in data.get("buildings", [])]
    for kind in REQUIRED:
        count = kinds.count(kind)
        if count == 0:
            found.append(f"Needs a {BUILDING_NAMES[kind]}.")
        elif count > 1:
            found.append(f"Only one {BUILDING_NAMES[kind]} is allowed.")
    homes = [b for b in data.get("buildings", []) if b["kind"] == "home"]
    if homes and (homes[0]["far"] or not homes[0]["owned"]):
        found.append("The Home burrow must be yours and on the main screen.")
    if not any(b["kind"] not in REQUIRED for b in data.get("buildings", [])):
        found.append("Needs at least one other building to capture first.")
    return found


def map_info(data) -> MapInfo:
    boss = data["boss"]
    armour = boss.get("armor") if isinstance(boss.get("armor"), list) else []
    # The set named on the map card: the one the boss wears most of.
    counts = {sid: sum(i in s.pieces for i in armour) for sid, s in ARMOR_SETS.items()}
    guardian_set = max(counts, key=counts.get) if armour and max(counts.values()) else "forager"
    return MapInfo(data["id"], data["title"], data["blurb"] or "A map of your own.", data["tier"], None,
                   tuple(data["enemy_tiers"]), boss.get("name") or "Guardian", guardian_set,
                   data.get("reward_companion"), kind="raid")


def placements(data) -> tuple[Placement, ...]:
    """The buildings as the mission reads them; a building placed on the far
    screen falls back to the same spot on the main one."""
    out = []
    for b in data["buildings"]:
        out.append(Placement(b["kind"], b["name"], b["fx"], b["fy"], guards=tuple(b["guards"]),
                             reserves=b["reserves"], owned=b["owned"], far=b["far"],
                             alt=(b["fx"], b["fy"]), supply=b["supply"]))
    return tuple(out)


# -- files -----------------------------------------------------------------------

def _path(map_id) -> Path:
    return maps_dir() / f"{map_id}.json"


def list_maps() -> list[dict]:
    folder = maps_dir()
    if not folder.is_dir():
        return []
    maps = []
    for path in sorted(folder.glob(f"{PREFIX}*.json")):
        data = load_map(path.stem)
        if data is not None:
            maps.append(data)
    return sorted(maps, key=lambda d: d["title"].casefold())


def load_map(map_id) -> dict | None:
    if not is_custom(map_id):
        return None
    try:
        raw = json.loads(_path(map_id).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    data = clean_map(raw)
    data["id"] = map_id
    return data


def save_map(data) -> str:
    """Write a map; a new one gets an id from its title. Returns the id."""
    data = clean_map(copy.deepcopy(data))
    if not is_custom(data["id"]):
        stem = re.sub(r"[^a-z0-9]+", "-", data["title"].casefold()).strip("-") or "map"
        map_id, n = f"{PREFIX}{stem}", 2
        while _path(map_id).exists():
            map_id, n = f"{PREFIX}{stem}-{n}", n + 1
        data["id"] = map_id
    folder = maps_dir()
    folder.mkdir(parents=True, exist_ok=True)
    temp = _path(data["id"]).with_suffix(".tmp")
    temp.write_text(json.dumps(data, indent=2), encoding="utf-8")
    temp.replace(_path(data["id"]))
    return data["id"]


def delete_map(map_id) -> bool:
    if not is_custom(map_id):
        return False
    try:
        _path(map_id).unlink()
        return True
    except OSError:
        return False


def _int(value, low, high) -> int:
    try:
        return max(low, min(high, int(value)))
    except (TypeError, ValueError):
        return low


def _float(value, low, high) -> float:
    try:
        return max(low, min(high, float(value)))
    except (TypeError, ValueError):
        return low
