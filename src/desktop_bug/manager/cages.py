"""Containment cages: adding, dragging, and who is inside one.

Split out of the single ``manager.py`` by DC-43; a pure move.
"""

from __future__ import annotations

import math
from typing import List, Optional, Tuple

from ..creature import Creature
from ..world.cage import Cage




class CageMixin:
    """Containment cages: adding, dragging, and who is inside one."""

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



    # ------------------------------------------------------------------
    # Bases
    #
    # A base is placed by a Builder wherever it happens to settle, and until
    # now nothing could move or clear one -- a colony that had built in an
    # awkward corner of the desktop was stuck with it for the life of the
    # save. Right-clicking one and removing it is the counterpart to "Add a
    # cage here": a direct edit of the scene, not a game action.
    # ------------------------------------------------------------------

    def base_at(self, x: float, y: float):
        """The base site under a point, or None.

        Picks the nearest when rings overlap, so clicking where two teams'
        territory meets removes the one actually under the cursor rather than
        whichever happens to be first in the dictionary.
        """
        base_world = getattr(self, "base_world", None)
        if base_world is None:
            return None
        best = None
        best_distance = 0.0
        for site in base_world.bases.values():
            dx = float(x) - site.x
            dy = float(y) - site.y
            distance = math.hypot(dx, dy)
            # A young base is a small ring but still wants a clickable target.
            reach = max(float(site.radius), 26.0)
            if distance <= reach and (best is None or distance < best_distance):
                best = site
                best_distance = distance
        return best

    def remove_base(self, site) -> str:
        """Delete one base and release everything that was pointing at it."""
        base_world = getattr(self, "base_world", None)
        if base_world is None or site is None:
            return "There is no base there to remove."
        if not base_world.remove_base(site.id):
            return "There is no base there to remove."
        # Nothing else to unpick: every job looks its site up by team each
        # frame, so the workers simply find there is none rather than holding
        # a stale reference. A Builder will found a fresh one in due course.
        team_label = site.team_id or "neutral"
        self.save_runtime_state()
        return f"Removed the {team_label} base."

    def remove_bases(self) -> str:
        base_world = getattr(self, "base_world", None)
        if base_world is None or not base_world.bases:
            return "There are no bases to remove."
        count = len(base_world.bases)
        for site in list(base_world.bases.values()):
            self.remove_base(site)
        return f"Removed {count} base(s)."
