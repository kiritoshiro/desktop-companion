"""Which mission the Adventure overlay plays, on which screens.

The map chosen on the Adventure page decides the kind: a raid on the
woodland buildings, or Reclaim the desktop on a frozen picture of the
desktop. The monitors decide the arena: every screen when the profile's
``all_screens`` is on and there is more than one, else the main screen.
Kept apart from the overlay window so it can be tested without one.
"""
from __future__ import annotations

from .adventure_profile import load_profile
from .campaign import chosen_map
from .desktop_capture import synthetic_snapshot
from .desktop_surface import DesktopSurface
from .mission import TerritoryMission
from .reclaim import ReclaimMission
from .swarm import FlySwarmMission
from ..world.screen_layout import ScreenLayout


def build_layout(rects, primary: int, all_screens: bool = True) -> ScreenLayout:
    rects = list(rects)
    primary = primary if 0 <= primary < len(rects) else 0
    if all_screens and len(rects) > 1:
        return ScreenLayout(rects, primary)
    return ScreenLayout.single(rects[primary])


def create_mission(manager, controls, rects, primary: int = 0, capture=None):
    """The mission for the chosen map. ``capture`` is called for Reclaim the
    desktop and returns a DesktopSnapshot (None when the screen cannot be
    read, which leaves a plain made-up desktop to fight on)."""
    profile = load_profile()
    info = chosen_map(profile)
    if info.kind == "reclaim":
        snapshot = capture() if capture is not None else None
        if snapshot is None or not snapshot.screens:
            snapshot = synthetic_snapshot(list(rects), primary=primary)
        layout = ScreenLayout([shot.rect for shot in snapshot.screens], snapshot.primary)
        return ReclaimMission(manager, controls, layout, DesktopSurface(snapshot))
    layout = build_layout(rects, primary, bool(profile.get("all_screens", True)))
    if info.kind == "swarm":
        return FlySwarmMission(manager, controls, layout=layout)
    return TerritoryMission(manager, controls, layout=layout)
