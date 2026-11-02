"""Deliberate Rust changes, applied to the Python oracle before it runs.

The fixture generators (rust/crates/*/tools/) drive the unmodified Python app
on `legacy`. Where Rust now behaves differently on purpose, the oracle has to
behave the same way, or the parity tests would only compare against the old
behaviour. Each patch here mirrors one Rust change and names it; the crate's
PORT_STATUS.md explains why. A generator calls `apply_all()` before it builds
anything.
"""
from __future__ import annotations

import inspect
import math
import textwrap


def _replace_method(cls, name: str, *edits: tuple[str, str]) -> None:
    """Recompile `cls.name` with each (old, new) text swapped in its source.
    Edits use the file's own indentation; the source is dedented after."""
    func = getattr(cls, name)
    source = inspect.getsource(func)
    for old, new in edits:
        assert source.count(old) == 1, f"{cls.__name__}.{name}: {old!r}"
        source = source.replace(old, new)
    source = textwrap.dedent(source)
    module = inspect.getmodule(func)
    namespace: dict = {}
    exec(compile(source, module.__file__, "exec"), vars(module), namespace)
    setattr(cls, name, namespace[name])


def tarantula_rig() -> None:
    """dc-content `TARANTULA_REST_REACH`: tarantula-plan legs rest further out
    (I 1.75, II 1.63, III 1.53, IV 1.85 sizes from the root), and dc-kinematics
    `spider_support_pose` gives tarantula-profile legs 0.92 sizes of travel
    past their rest spot so the longer legs can still turn."""
    from desktop_bug.content import body_plans
    from desktop_bug.creature.kinematics import gait

    reach = (1.75, 1.63, 1.53, 1.85)
    for index, leg in enumerate(body_plans._TARANTULA_LEGS):
        af, as_ = leg["attach_forward"], leg["attach_side"]
        rf, rs = leg["rest_forward"], leg["rest_side"]
        # sqrt, not hypot: it rounds the same in Rust.
        scale = reach[index // 2] / math.sqrt((rf - af) * (rf - af) + (rs - as_) * (rs - as_))
        leg["rest_forward"] = af + (rf - af) * scale
        leg["rest_side"] = as_ + (rs - as_) * scale
    _replace_method(gait.GaitConfigMixin, "_spider_support_pose", (
        "if abs(local_f) > self.size * 2.45:",
        "if abs(local_f) > self.size * (max(2.45, abs(float(leg.definition.get(\"rest_forward\", 0.0))) + 0.92)"
        " if config.get(\"profile\") == \"tarantula\" else 2.45):",
    ))


def guard_ring() -> None:
    """dc-world `GUARD_POSTS_BASE` and `update_guard`: a base posts at most
    4 + level guards, and each walks its own stretch of a ring round the base
    (widened to 70 px per guard) instead of a line through the centre."""
    from desktop_bug.world import jobs

    _replace_method(jobs.BaseWorld, "update", (
        "            for slot, guard in enumerate(posted):\n"
        "                self._update_guard(dt, guard, site, slot, len(posted), creatures)\n",
        "            count = min(len(posted), 4 + max(0, int(site.level)))\n"
        "            for slot, guard in enumerate(posted[:count]):\n"
        "                self._update_guard(dt, guard, site, slot, count, creatures)\n",
    ))
    _replace_method(jobs.BaseWorld, "_update_guard", (
        "        axis = site.patrol_angle + (slot / max(1, count)) * math.pi\n"
        "        standoff = site.radius + GUARD_STANDOFF_PAD\n",
        "        sector = math.tau / max(1, count)\n"
        "        half_arc = min(sector * 0.3, 0.6)\n"
        "        standoff = site.radius + GUARD_STANDOFF_PAD\n"
        "        patrol_radius = max(standoff, count * 70.0 / math.tau)\n",
    ), (
        "        end = (site.x + math.cos(axis) * standoff * line[\"end\"],\n"
        "               site.y + math.sin(axis) * standoff * line[\"end\"])\n",
        "        angle = site.patrol_angle + slot * sector + half_arc * line[\"end\"]\n"
        "        end = (site.x + math.cos(angle) * patrol_radius,\n"
        "               site.y + math.sin(angle) * patrol_radius)\n",
    ))


def apply_all() -> None:
    tarantula_rig()
    guard_ring()
