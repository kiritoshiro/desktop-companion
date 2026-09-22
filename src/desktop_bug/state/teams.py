"""Teams a person can name, colour, and understand.

A team used to be an id out of a fixed list: `pack_a`, `pack_b`, `hunters`,
`rivals`. Those names say nothing about what the group is for, and when this
module was written two of them promised a fight that could not happen, because
there was no combat in the overlay at all. DC-22, DC-45 and DC-47 have since
made that fight real and fatal, so `HOSTILITY_NOTE` below now describes a
consequence rather than warning against expecting one. Worse, nothing on screen
distinguished one team from another, so a preset with two teams looked exactly
like a preset with one.

This module holds what a team *is*: a stable id, a name its owner chose, and a
colour used for its base ring and a small marker on its members. It is
deliberately Qt-free so presets, the settings window, the overlay and the tests
all agree about the same data.

Ids are case-folded, like preset namespaces and saved-spider keys. On Windows
that has already been the cause of four separate bugs, and a team called `Rivals`
in one place and `rivals` in another would be a fifth.
"""

from __future__ import annotations

import colorsys
from dataclasses import dataclass
from typing import Dict, Iterable, Mapping

from .progression import DEFAULT_HOSTILE_TEAMS, RELATIONS, normalize_team_id

NEUTRAL = "neutral"

# What hostility actually does today. The settings window, the inspector and the
# README all say this, and they say it from here so they cannot drift apart or
# quietly start over-promising.
# DC-22, DC-45 and DC-47 made every sentence of the old note false -- it still
# read "There is no combat yet ... nothing takes damage" while a losing spider
# was dying permanently. Kept in one place for the same reason as before.
HOSTILITY_NOTE = (
    "Foes fight. Two hostile spiders that meet will attack, using whatever "
    "skills they have -- silk to pin, a pounce to close -- and a Guard still "
    "raises an alert and intercepts. A beaten spider dies and leaves a carcass "
    "that is eaten away; it does not come back. Turn Conflict off to go back to "
    "alerts without damage."
)

STANCE_LABELS = {
    "friend": "Allies",
    "neutral": "Ignore each other",
    "foe": "Foes",
}

# The teams that shipped before a team could be named. Their labels are kept so
# an existing preset keeps reading the way its author left it, and the colours
# are chosen to be told apart at a glance rather than derived from the id.
BUILT_IN_TEAMS: Dict[str, tuple] = {
    "pack_a": ("Pack A", (79, 163, 209)),
    "pack_b": ("Pack B", (122, 196, 129)),
    "hunters": ("Hunters", (226, 166, 74)),
    "rivals": ("Rivals", (209, 83, 79)),
}


@dataclass(frozen=True)
class TeamProfile:
    """One team: the id everything else refers to, a name, and a colour."""

    id: str
    name: str
    color: tuple

    def hex_color(self) -> str:
        return color_to_hex(self.color)


def default_color(team_id) -> tuple:
    """A stable colour for a team, whether or not anyone chose one.

    Built-in teams get a hand-picked colour. Anything else is derived from the
    id, across the whole hue circle rather than the narrow slice the base ring
    used to use, so two teams a person invents still look different.
    """
    team_id = normalize_team_id(team_id)
    known = BUILT_IN_TEAMS.get(team_id)
    if known is not None:
        return known[1]
    if team_id == NEUTRAL:
        # DC-51: white, at the owner's request -- "white would be neutral".
        # The old grey was hard to tell from a team whose hash happened to
        # land on a desaturated colour, which is the one distinction a
        # colour-only team marker cannot afford to lose.
        return (255, 255, 255)
    return _color_at_hue(_fnv1a(team_id), 0)


# How far apart two team colours have to be, as a plain RGB distance, before
# a person can tell them apart on a busy desktop at spider size.
MIN_COLOR_SEPARATION = 60.0


def _color_at_hue(digest: int, hue_offset: int) -> tuple:
    hue = ((digest % 360) + hue_offset) % 360 / 360.0
    saturation = 0.46 + ((digest >> 9) % 5) * 0.06
    value = 0.78 + ((digest >> 17) % 4) * 0.05
    red, green, blue = colorsys.hsv_to_rgb(hue, saturation, value)
    return (int(round(red * 255)), int(round(green * 255)), int(round(blue * 255)))


def _distance(left, right) -> float:
    return sum((int(a) - int(b)) ** 2 for a, b in zip(left, right)) ** 0.5


def distinct_color(team_id, taken: Iterable[tuple] = ()) -> tuple:
    """The team's own colour, moved aside if something else already looks like it.

    A hash over the id gives a stable colour, but a hash cannot promise that two
    particular ids differ: `porch_guard` and `shed_crew` landed on exactly the
    same colour, which is the one failure this is here to prevent. So the hue
    rotates until the colour is clear of the ones already in use. A team keeps
    its natural colour whenever nothing conflicts, and the result depends only
    on the set of teams, so it is the same on every launch.
    """
    team_id = normalize_team_id(team_id)
    known = BUILT_IN_TEAMS.get(team_id)
    if known is not None:
        return known[1]
    if team_id == NEUTRAL:
        # DC-51: white, at the owner's request -- "white would be neutral".
        # The old grey was hard to tell from a team whose hash happened to
        # land on a desaturated colour, which is the one distinction a
        # colour-only team marker cannot afford to lose.
        return (255, 255, 255)
    digest = _fnv1a(team_id)
    others = list(taken)
    for step in range(8):
        candidate = _color_at_hue(digest, step * 47)
        if all(_distance(candidate, other) >= MIN_COLOR_SEPARATION for other in others):
            return candidate
    return _color_at_hue(digest, 0)


def _fnv1a(text: str) -> int:
    """A stable 32-bit hash. Python's own is randomised per process, and a
    weighted character sum collided between two ordinary-looking team names."""
    digest = 2166136261
    for char in text.encode("utf-8"):
        digest = ((digest ^ char) * 16777619) & 0xFFFFFFFF
    return digest


def default_name(team_id) -> str:
    """The name a team has before anybody renames it."""
    team_id = normalize_team_id(team_id)
    known = BUILT_IN_TEAMS.get(team_id)
    if known is not None:
        return known[0]
    if team_id == NEUTRAL:
        return "Neutral / solo"
    return team_id.replace("_", " ").replace("-", " ").strip().title() or "Team"


def parse_color(value) -> tuple | None:
    """Accept `#rrggbb`, `rrggbb`, or a three-number sequence. None if unusable."""
    if value is None:
        return None
    if isinstance(value, (list, tuple)) and len(value) >= 3:
        try:
            return tuple(max(0, min(255, int(round(float(part))))) for part in value[:3])
        except (TypeError, ValueError):
            return None
    text = str(value).strip().lstrip("#")
    if len(text) == 3:
        text = "".join(char * 2 for char in text)
    if len(text) != 6:
        return None
    try:
        return (int(text[0:2], 16), int(text[2:4], 16), int(text[4:6], 16))
    except ValueError:
        return None


def color_to_hex(color) -> str:
    red, green, blue = (max(0, min(255, int(part))) for part in tuple(color)[:3])
    return f"#{red:02x}{green:02x}{blue:02x}"


def normalize_team_name(value, team_id: str) -> str:
    """A team's display name, never empty and never long enough to break a row."""
    text = str(value or "").strip()
    if not text:
        return default_name(team_id)
    return text[:32]


def normalize_teams(raw, used_ids: Iterable[str] = ()) -> Dict[str, TeamProfile]:
    """Build the team table from a preset block, plus any id a slot refers to.

    A slot can name a team the `teams` block never mentions -- an older preset
    has no block at all -- so every id in use gets an entry, with its default
    name and colour, rather than silently having no identity.
    """
    profiles: Dict[str, TeamProfile] = {}
    # A chosen colour is never moved; only the ones this code invents are kept
    # clear of each other, so a person's own choice always wins.
    taken: list = []
    pending: list = []

    if isinstance(raw, Mapping):
        for key, value in raw.items():
            team_id = normalize_team_id(key)
            if team_id == NEUTRAL:
                # "No team" is not a team; it cannot be renamed or coloured.
                continue
            if isinstance(value, Mapping):
                name = normalize_team_name(value.get("name"), team_id)
                chosen = parse_color(value.get("color"))
            else:
                # A bare string is read as the name, which is the shape a person
                # writing one of these by hand reaches for first.
                name = normalize_team_name(value, team_id)
                chosen = None
            if chosen is None:
                pending.append((team_id, name))
                profiles[team_id] = TeamProfile(team_id, name, (0, 0, 0))
            else:
                profiles[team_id] = TeamProfile(team_id, name, chosen)
                taken.append(chosen)

    for value in used_ids:
        team_id = normalize_team_id(value)
        if team_id == NEUTRAL or team_id in profiles:
            continue
        name = default_name(team_id)
        pending.append((team_id, name))
        profiles[team_id] = TeamProfile(team_id, name, (0, 0, 0))

    # Sorted, so the colours a scene ends up with depend on which teams it has
    # and not on the order they happened to be written in.
    for team_id, name in sorted(pending):
        color = distinct_color(team_id, taken)
        taken.append(color)
        profiles[team_id] = TeamProfile(team_id, name, color)
    return profiles


def teams_payload(profiles: Mapping[str, TeamProfile]) -> dict:
    """The `teams` block to save, in the shape `normalize_teams` reads back."""
    return {
        team_id: {"name": profile.name, "color": profile.hex_color()}
        for team_id, profile in sorted(profiles.items())
    }


def team_profile(team_id, profiles: Mapping[str, TeamProfile] | None = None) -> TeamProfile:
    """The profile for a team, invented from its id if nobody defined one."""
    team_id = normalize_team_id(team_id)
    if profiles:
        found = profiles.get(team_id)
        if found is not None:
            return found
    return TeamProfile(team_id, default_name(team_id), default_color(team_id))


def team_label(team_id, profiles: Mapping[str, TeamProfile] | None = None) -> str:
    return team_profile(team_id, profiles).name


def team_color(team_id, profiles: Mapping[str, TeamProfile] | None = None) -> tuple:
    return team_profile(team_id, profiles).color


def stance_label(relation) -> str:
    return STANCE_LABELS.get(str(relation or "neutral"), "Ignore each other")


def describe_stance(left_id, right_id, relation,
                    profiles: Mapping[str, TeamProfile] | None = None) -> str:
    """One sentence a person can read, naming both teams and what follows.

    Says what hostility does. It used to end "Nothing takes damage yet",
    which stopped being true at DC-22 and became badly misleading at DC-47,
    when losing a fight started killing the spider for good.
    """
    left = team_label(left_id, profiles)
    right = team_label(right_id, profiles)
    relation = str(relation or "neutral")
    if relation == "friend":
        return f"{left} and {right} are allies and keep each other company."
    if relation == "foe":
        return (f"{left} and {right} are foes: they fight on sight and a Guard of "
                f"either team intercepts the other near its base. Damage is real "
                f"and the loser dies.")
    return f"{left} and {right} ignore each other."


def stance_pairs(profiles: Mapping[str, TeamProfile], stances: Mapping | None = None):
    """Every unordered pair of teams with the relation currently between them.

    Ordered by id so the settings window lists them the same way every time.
    """
    from .progression import team_stance

    ids = sorted(profiles)
    pairs = []
    for index, left in enumerate(ids):
        for right in ids[index + 1:]:
            relation = team_stance(left, right, dict(stances or {})) or "neutral"
            pairs.append((left, right, relation))
    return pairs


def minimal_stances(stances: Mapping | None) -> dict:
    """The `team_relations` block to save: each pair once, in a stable order.

    The runtime keeps a stance in both directions, because a half-declared one
    would let a Guard and its intruder disagree about whether anything hostile
    is happening. Writing both into the file would be noise, and would rewrite
    a hand-edited preset the first time it was saved, so only one direction is
    stored and the loader mirrors it back.

    An explicit `neutral` is kept rather than dropped: for a team that is
    hostile by default, "these two ignore each other" is a real decision and
    losing it would silently restore the hostility.
    """
    payload: dict = {}
    seen = set()
    for left, row in sorted((stances or {}).items()):
        left_id = normalize_team_id(left)
        if not isinstance(row, Mapping) or left_id == NEUTRAL:
            continue
        for right, relation in sorted(row.items()):
            right_id = normalize_team_id(right)
            relation = str(relation or "").strip().lower()
            if relation not in RELATIONS or right_id == NEUTRAL or right_id == left_id:
                continue
            pair = tuple(sorted((left_id, right_id)))
            if pair in seen:
                continue
            seen.add(pair)
            payload.setdefault(pair[0], {})[pair[1]] = relation
    return payload


def is_default_hostile(team_id) -> bool:
    """True for a team that is hostile to everyone unless told otherwise."""
    return normalize_team_id(team_id) in DEFAULT_HOSTILE_TEAMS
