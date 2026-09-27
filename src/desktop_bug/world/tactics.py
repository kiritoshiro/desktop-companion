"""Squad tactics: several spiders fighting as one team.

The owner: *"create multispider strategies. so that if there are a few of them
based on the abilities they could coordinate better at defending, attacking.
embed that into enemies for now. but keep it flexible, later on we will use it
in strategy mode when team against a team will be fighting ... based on the
abilities of the spiders and how many of them are, and based on that manage
the levels so that more difficult ones would use these team strategies."*

Nothing here knows about missions, Qt or which side is which. A squad is a
list of :class:`Member` (where each spider is, how hurt, what it is good at);
the foes are a list of :class:`Foe`. :func:`plan` picks a **doctrine** for
that squad -- from what its members can do, how many there are, the foes in
reach, and the squad's skill level -- and gives every member an
:class:`Order`. Whoever drives the spiders (the mission's enemy brains now,
either team in the strategy mode later) follows the orders.

Skill levels (a map's tier decides it in the Adventure):

- 0 -- no teamwork: every spider fights on its own.
- 1 -- focus fire: the squad picks one foe and all go for it; the badly hurt
  fall back to heal.
- 2 -- formations: a tank holds the front while fast ones flank (*hammer and
  anvil*); melee screen the ranged, who shoot from behind them (*screen*);
  an attacking squad gathers before it moves (*regroup*); defenders hold a
  perimeter round their post.
- 3 -- advanced: three or more close-fighters surround a foe from every side
  (*encircle*); defenders send a fast one to lure a foe back into the rest
  lying in wait (*bait and ambush*).
"""
from __future__ import annotations

import math
from dataclasses import dataclass, field

MELEE_ROLES = ("tank", "melee", "fast")
RANGED_ROLES = ("ranged", "support")
# A squad's members stop fighting and go back to heal below this share.
RETREAT_HEALTH = 0.3
# How far a defending squad reaches from its post before it lets a foe be.
DEFEND_RADIUS = 330.0
# Spread beyond which an attacking squad gathers before it goes in.
REGROUP_SPREAD = 260.0


@dataclass
class Member:
    key: object
    x: float
    y: float
    health: float = 1.0            # share of full health, 0..1
    role: str = "melee"            # tank, melee, fast, ranged, support
    reach: float = 34.0            # how close it fights from
    abilities: frozenset = frozenset()
    anchor: tuple[float, float] | None = None   # its post, or its rally point


@dataclass
class Foe:
    key: object
    x: float
    y: float
    health: float = 1.0
    threat: float = 1.0            # how much it matters to stop this one


@dataclass
class Order:
    mode: str                      # engage, flank, screen, kite, encircle, hold, regroup, retreat, bait, ambush
    target: object = None          # a Foe key, or None
    point: tuple[float, float] | None = None   # where to stand; None: close on the target
    note: str = ""


@dataclass
class Plan:
    doctrine: str = "none"
    orders: dict = field(default_factory=dict)
    focus: object = None


def _centroid(points):
    points = list(points)
    if not points:
        return 0.0, 0.0
    return sum(p[0] for p in points) / len(points), sum(p[1] for p in points) / len(points)


def _unit(dx, dy):
    d = math.hypot(dx, dy)
    return (dx / d, dy / d) if d > 1e-6 else (1.0, 0.0)


def choose_focus(members, foes):
    """The foe the squad should all go for: close to it, already hurt, and
    dangerous. A hero counts double through its threat."""
    cx, cy = _centroid((m.x, m.y) for m in members)

    def score(foe):
        distance = math.hypot(foe.x - cx, foe.y - cy)
        return distance * (0.55 + foe.health) / max(0.2, foe.threat)
    return min(foes, key=score) if foes else None


def plan(members, foes, level: int, stance: str = "defend", anchor=None, notice: float = 360.0) -> Plan:
    """Orders for one squad. ``stance``: "defend" a post at ``anchor``, or
    "attack". ``notice``: how far a defending squad watches beyond its post
    (for the bait at level 3). Members given no order fight on their own."""
    members = [m for m in members if m is not None]
    result = Plan()
    if level <= 0 or len(members) < 2:
        return result
    anchor = anchor or _centroid((m.x, m.y) for m in members)
    orders = result.orders

    # Level 1 and up: the badly hurt fall back to heal, unless they are the
    # squad's wall.
    fighting = []
    for m in members:
        if m.health < RETREAT_HEALTH and m.role != "tank" and m.anchor is not None:
            orders[m.key] = Order("retreat", point=m.anchor, note="falls back to heal")
        else:
            fighting.append(m)
    if len(fighting) < 2:
        result.doctrine = "fall back" if orders else "none"
        return result

    if stance == "defend":
        engaged = [f for f in foes if math.hypot(f.x - anchor[0], f.y - anchor[1]) < DEFEND_RADIUS]
    else:
        engaged = list(foes)

    if not engaged:
        if stance == "defend" and level >= 3:
            lure = [f for f in foes if math.hypot(f.x - anchor[0], f.y - anchor[1]) < DEFEND_RADIUS + notice]
            fast = [m for m in fighting if m.role == "fast"]
            if lure and fast and len(fighting) >= 3:
                return _bait_and_ambush(result, fighting, lure, fast[0], anchor)
        if stance == "defend" and level >= 2:
            return _perimeter(result, fighting, anchor, foes)
        if stance == "attack" and level >= 2:
            spread = max(math.hypot(m.x - n.x, m.y - n.y) for m in fighting for n in fighting)
            if spread > REGROUP_SPREAD:
                rally = _centroid((m.x, m.y) for m in fighting)
                for m in fighting:
                    orders[m.key] = Order("regroup", point=rally, note="gathers before the attack")
                result.doctrine = "regroup"
        return result

    focus = choose_focus(fighting, engaged)
    result.focus = focus.key
    melee = [m for m in fighting if m.role in MELEE_ROLES]
    ranged = [m for m in fighting if m.role in RANGED_ROLES]
    tanks = [m for m in melee if m.role == "tank"]
    fast = [m for m in melee if m.role == "fast"]

    if level >= 3 and len(melee) >= 3:
        _encircle(orders, melee, focus)
        _kite(orders, ranged, focus, melee)
        result.doctrine = "encircle"
    elif level >= 2 and tanks and fast:
        _hammer_and_anvil(orders, tanks, [m for m in melee if m not in tanks], focus)
        _kite(orders, ranged, focus, melee)
        result.doctrine = "hammer and anvil"
    elif level >= 2 and ranged and melee:
        _screen(orders, melee, ranged, focus)
        result.doctrine = "screen"
    else:
        for m in fighting:
            orders[m.key] = Order("engage", target=focus.key, note="focus fire")
        result.doctrine = "focus fire"
    return result


# -- the doctrines ------------------------------------------------------------

def _encircle(orders, melee, focus):
    """Close-fighters take evenly spaced places round the foe, starting from
    the side they come from, so it cannot face them all."""
    cx, cy = _centroid((m.x, m.y) for m in melee)
    start = math.atan2(cy - focus.y, cx - focus.x)
    count = len(melee)
    slots = [start + i * math.tau / count for i in range(count)]
    free = list(melee)
    for angle in slots:
        # The nearest free member takes each place.
        px, py = focus.x + math.cos(angle) * 60.0, focus.y + math.sin(angle) * 60.0
        m = min(free, key=lambda m: math.hypot(m.x - px, m.y - py))
        free.remove(m)
        orders[m.key] = Order("encircle", target=focus.key, point=(px, py), note="surrounds")


def _hammer_and_anvil(orders, tanks, others, focus):
    """The tank meets the foe head on; the rest swing round to its sides and
    back while it is busy."""
    for t in tanks:
        orders[t.key] = Order("engage", target=focus.key, note="holds the front")
    tx, ty = _centroid((t.x, t.y) for t in tanks)
    fx, fy = _unit(focus.x - tx, focus.y - ty)     # from the tank through the foe
    for index, m in enumerate(others):
        side = 1 if index % 2 == 0 else -1
        # Beside and behind the foe, as seen from the tank.
        px = focus.x + fx * 38 + (-fy) * side * 58
        py = focus.y + fy * 38 + fx * side * 58
        orders[m.key] = Order("flank", target=focus.key, point=(px, py), note="flanks")


def _screen(orders, melee, ranged, focus):
    """Close-fighters stand between the foe and the shooters, who shoot from
    behind them."""
    rx, ry = _centroid((m.x, m.y) for m in ranged)
    ux, uy = _unit(rx - focus.x, ry - focus.y)      # from the foe towards the shooters
    for index, m in enumerate(melee):
        offset = (index - (len(melee) - 1) / 2) * 34
        px = focus.x + ux * 46 + (-uy) * offset
        py = focus.y + uy * 46 + ux * offset
        orders[m.key] = Order("screen", target=focus.key, point=(px, py), note="screens the shooters")
    _kite(orders, ranged, focus, melee)


def _kite(orders, ranged, focus, melee):
    """Shooters keep their range from the foe, on the far side of the
    close-fighters."""
    if not ranged:
        return
    if melee:
        mx, my = _centroid((m.x, m.y) for m in melee)
        ux, uy = _unit(mx - focus.x, my - focus.y)
    else:
        rx, ry = _centroid((m.x, m.y) for m in ranged)
        ux, uy = _unit(rx - focus.x, ry - focus.y)
    for index, m in enumerate(ranged):
        spread = (index - (len(ranged) - 1) / 2) * 0.5
        c, s = math.cos(spread), math.sin(spread)
        dx, dy = ux * c - uy * s, ux * s + uy * c
        distance = max(110.0, m.reach * 0.8)
        orders[m.key] = Order("kite", target=focus.key,
                              point=(focus.x + dx * distance, focus.y + dy * distance), note="shoots from range")


def _perimeter(result, members, anchor, foes):
    """Nobody close: stand round the post, the wall on the side the foes are."""
    ax, ay = anchor
    if foes:
        nearest = min(foes, key=lambda f: math.hypot(f.x - ax, f.y - ay))
        facing = math.atan2(nearest.y - ay, nearest.x - ax)
    else:
        facing = math.pi
    ordered = sorted(members, key=lambda m: (m.role != "tank", m.role in RANGED_ROLES))
    for index, m in enumerate(ordered):
        if m.role in RANGED_ROLES:
            angle, radius = facing + math.pi + (index - len(ordered) / 2) * 0.5, 55.0
        else:
            angle, radius = facing + (index - (len(ordered) - 1) / 2) * 0.7, 95.0
        result.orders[m.key] = Order("hold", point=(ax + math.cos(angle) * radius, ay + math.sin(angle) * radius),
                                     note="holds the perimeter")
    result.doctrine = "perimeter"
    return result


def _bait_and_ambush(result, members, lure, bait, anchor):
    """A fast one goes out to draw the foe in; the rest wait round the post."""
    ax, ay = anchor
    foe = min(lure, key=lambda f: math.hypot(f.x - ax, f.y - ay))
    result.focus = foe.key
    ux, uy = _unit(foe.x - ax, foe.y - ay)
    # Close enough to be seen and chased, not so close it is caught.
    result.orders[bait.key] = Order("bait", target=foe.key,
                                    point=(foe.x - ux * 120, foe.y - uy * 120), note="lures a foe in")
    rest = [m for m in members if m is not bait]
    for index, m in enumerate(rest):
        side = 1 if index % 2 == 0 else -1
        angle = math.atan2(uy, ux) + side * (0.9 + 0.3 * (index // 2))
        result.orders[m.key] = Order("ambush", target=foe.key,
                                     point=(ax + math.cos(angle) * 80, ay + math.sin(angle) * 80),
                                     note="lies in wait")
    result.doctrine = "bait and ambush"
    return result


def tactics_level(tier: int) -> int:
    """A map's squad skill from its tier: none on the first maps."""
    return max(0, min(3, int(tier) - 1))
