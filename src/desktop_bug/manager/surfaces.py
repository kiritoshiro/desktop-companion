"""Desktop window and folder awareness: occlusion, hiding, emerging.

Split out of the single ``manager.py`` by DC-43; a pure move.
"""

from __future__ import annotations

import math
from typing import List, Tuple

from ..creature import Creature
from ..world.desktop_environment import DesktopSurface


from .constants import (
    _clamp,
)


class DesktopSurfaceMixin:
    """Desktop window and folder awareness: occlusion, hiding, emerging."""

    # ------------------------------------------------------------------
    # Desktop window / folder awareness
    # ------------------------------------------------------------------
    def _clear_desktop_hide_intent(self, creature: Creature) -> None:
        creature._desktop_hide_surface_key = None
        creature._desktop_hide_kind = None
        creature._desktop_hide_timer = 0.0
        creature._folder_portal_dwell = 0.0

    def _behavior_allows_desktop_hide(self, creature: Creature) -> bool:
        # Hiding should look deliberate. Do not let mouse/social chase, startle,
        # jumping, dragging, inspecting, or catching accidentally send a visible
        # spider under a window. Fully hidden spiders are already behind the
        # surface and are allowed to keep crawling out.
        if creature.dragging or creature.airborne or creature.cage is not None:
            return False
        if creature.state in {
            "Alert", "Approach", "Chase", "Retreat", "Startled",
            "Aim", "Inspect", "Observe", "Jump", "Coil", "Land",
            "Roll", "Catch", "Play", "Cuddle", "Dragged", "Zoom",
        }:
            return False
        if not self._is_fully_hidden(creature):
            try:
                dist_to_mouse = math.hypot(creature.x - self._mouse_x, creature.y - self._mouse_y)
            except Exception:
                dist_to_mouse = 999999.0
            safety = self._personality_float(creature, "desktop_hide_mouse_safety_radius", 260.0)
            if self._mouse_down:
                safety *= 1.7
            if dist_to_mouse < max(80.0, safety):
                return False
        return True

    def _surface_contains(self, surface: DesktopSurface, x: float, y: float, inset: float = 0.0) -> bool:
        return (surface.x + inset <= x <= surface.x + surface.w - inset
                and surface.y + inset <= y <= surface.y + surface.h - inset)

    def _surface_inside_depth(self, surface: DesktopSurface, x: float, y: float) -> float:
        if not self._surface_contains(surface, x, y):
            return 0.0
        return max(0.0, min(x - surface.x, surface.x + surface.w - x, y - surface.y, surface.y + surface.h - y))

    def _bbox_intersects_surface(self, bbox: Tuple[float, float, float, float], surface: DesktopSurface) -> bool:
        x0, y0, x1, y1 = bbox
        return not (x1 <= surface.x or x0 >= surface.x + surface.w or y1 <= surface.y or y0 >= surface.y + surface.h)

    def _bbox_inside_surface(self, bbox: Tuple[float, float, float, float], surface: DesktopSurface) -> bool:
        x0, y0, x1, y1 = bbox
        return (x0 >= surface.x and x1 <= surface.x + surface.w
                and y0 >= surface.y and y1 <= surface.y + surface.h)

    def _bbox_intersection(self, bbox: Tuple[float, float, float, float], surface: DesktopSurface) -> Tuple[float, float, float, float] | None:
        x0, y0, x1, y1 = bbox
        ix0 = max(x0, surface.x)
        iy0 = max(y0, surface.y)
        ix1 = min(x1, surface.x + surface.w)
        iy1 = min(y1, surface.y + surface.h)
        if ix1 <= ix0 or iy1 <= iy0:
            return None
        return ix0, iy0, ix1, iy1

    def _higher_surfaces(self, surface: DesktopSurface) -> List[DesktopSurface]:
        try:
            idx = self.desktop_surfaces.index(surface)
        except ValueError:
            return []
        return [s for s in self.desktop_surfaces[:idx] if self._surface_can_occlude(s)]

    def _point_hidden_by_higher_surface(self, surface: DesktopSurface, x: float, y: float) -> bool:
        return any(self._surface_contains(higher, x, y) for higher in self._higher_surfaces(surface))

    def _surface_visible_at_point(self, surface: DesktopSurface, x: float, y: float) -> bool:
        return self._surface_contains(surface, x, y) and not self._point_hidden_by_higher_surface(surface, x, y)

    def _bbox_has_visible_surface_area(self, surface: DesktopSurface, bbox: Tuple[float, float, float, float]) -> bool:
        overlap = self._bbox_intersection(bbox, surface)
        if overlap is None:
            return False
        x0, y0, x1, y1 = overlap
        samples = [
            ((x0 + x1) * 0.5, (y0 + y1) * 0.5),
            (x0 + 1.0, y0 + 1.0),
            (x1 - 1.0, y0 + 1.0),
            (x0 + 1.0, y1 - 1.0),
            (x1 - 1.0, y1 - 1.0),
        ]
        return any(self._surface_visible_at_point(surface, x, y) for x, y in samples)

    def _surface_visible_qregion(self, surface: DesktopSurface, QRect, QRegion):
        region = QRegion(QRect(
            int(math.floor(surface.x)),
            int(math.floor(surface.y)),
            max(1, int(math.ceil(surface.w))),
            max(1, int(math.ceil(surface.h))),
        ))
        for higher in self._higher_surfaces(surface):
            region -= QRegion(QRect(
                int(math.floor(higher.x)),
                int(math.floor(higher.y)),
                max(1, int(math.ceil(higher.w))),
                max(1, int(math.ceil(higher.h))),
            ))
        return region

    def _is_fully_hidden(self, creature: Creature) -> bool:
        return bool(getattr(creature, "_desktop_fully_hidden", False))

    def _surface_occlusion_is_allowed_for_creature(self, creature: Creature, surface: DesktopSurface) -> bool:
        """Return True only for a deliberate hide/portal target.

        Earlier builds clipped whenever a spider happened to cross any remembered
        window rectangle.  That made mouse chasing and lower covered windows look
        like invisible walls.  Now the manager must first set
        _desktop_hide_surface_key by choosing this exact top-visible surface as a
        hideout.
        """
        key = self._surface_key(surface)
        active_key = getattr(creature, "_desktop_hide_surface_key", None)
        if active_key != key:
            return False
        if not self._behavior_allows_desktop_hide(creature) and not self._is_fully_hidden(creature):
            self._clear_desktop_hide_intent(creature)
            return False
        bbox = creature.bounding_rect(False)
        if not self._bbox_intersects_surface(bbox, surface):
            # The spider deliberately left or missed the window; end this hide
            # attempt rather than leaving an invisible wall behind.
            if not self._surface_contains(surface, creature.x, creature.y):
                self._clear_desktop_hide_intent(creature)
            return False
        return self._bbox_has_visible_surface_area(surface, bbox)

    def _occluding_surfaces_for_creature(self, creature: Creature) -> List[DesktopSurface]:
        if creature.dragging or getattr(creature, "_desktop_spawn_grace", 0.0) > 0.0:
            return []
        active_key = getattr(creature, "_desktop_hide_surface_key", None)
        if active_key is None:
            return []
        surface = self._surface_by_key.get(active_key)
        if surface is None:
            # Window moved/closed or icon vanished; do not leave stale invisible walls.
            self._clear_desktop_hide_intent(creature)
            return []
        if not self._surface_can_occlude(surface):
            self._clear_desktop_hide_intent(creature)
            return []
        if self._surface_occlusion_is_allowed_for_creature(creature, surface):
            return [surface]
        return []

    def _creature_fully_covered_by_any_surface(self, creature: Creature, surfaces: List[DesktopSurface] | None = None) -> bool:
        if creature.dragging:
            return False
        surfaces = surfaces if surfaces is not None else self._occluding_surfaces_for_creature(creature)
        if not surfaces:
            return False
        bbox = creature.bounding_rect(False)
        for surface in surfaces:
            if not self._bbox_inside_surface(bbox, surface):
                continue
            # If a higher window covers part of this surface, this lower surface is
            # not allowed to make the whole spider invisible.
            if any(self._bbox_intersects_surface(bbox, higher) for higher in self._higher_surfaces(surface)):
                continue
            return True
        return False

    def _point_occluded_for_creature(self, creature: Creature, mx: float, my: float) -> bool:
        if creature.dragging:
            return False
        for surface in self._occluding_surfaces_for_creature(creature):
            if self._surface_visible_at_point(surface, mx, my):
                return True
        return False

    def _surface_key(self, surface: DesktopSurface) -> tuple:
        """Stable-enough key used to remember entry/exit across snapshots."""
        return (
            surface.kind,
            surface.class_name,
            int(round(surface.x / 8.0)),
            int(round(surface.y / 8.0)),
            int(round(surface.w / 8.0)),
            int(round(surface.h / 8.0)),
        )

    def _surface_can_occlude(self, surface: DesktopSurface) -> bool:
        """Return whether a window should be allowed to hide spiders.

        Fullscreen or nearly-fullscreen windows make a transparent overlay look
        broken: every spider may spawn "inside" the window rectangle and fade
        to nothing before the user sees it.  Treat those very large windows as
        background instead of hide-behind surfaces.  Smaller app/folder windows
        still behave like walls/cover.
        """
        screen_area = max(1.0, float(self.screen_w * self.screen_h))
        area_ratio = (surface.w * surface.h) / screen_area
        if area_ratio >= 0.86:
            return False
        if surface.w >= self.screen_w * 0.96 and surface.h >= self.screen_h * 0.82:
            return False
        if surface.w >= self.screen_w * 0.82 and surface.h >= self.screen_h * 0.96:
            return False
        return True

    def _distance_to_surface(self, surface: DesktopSurface, x: float, y: float) -> float:
        dx = max(surface.x - x, 0.0, x - (surface.x + surface.w))
        dy = max(surface.y - y, 0.0, y - (surface.y + surface.h))
        return math.hypot(dx, dy)

    def _containing_surface(self, creature: Creature, *, kind: str | None = None, min_depth: float = 0.0) -> DesktopSurface | None:
        best = None
        best_depth = -1.0
        for surface in self.desktop_surfaces:
            if kind is not None and surface.kind != kind:
                continue
            depth = self._surface_inside_depth(surface, creature.x, creature.y)
            if depth >= min_depth and depth > best_depth:
                best = surface
                best_depth = depth
        return best

    def _folder_surfaces(self) -> List[DesktopSurface]:
        return [surface for surface in self.desktop_surfaces
                if surface.kind == "folder"
                and self._surface_can_occlude(surface)
                and self._surface_visible_at_point(surface, surface.x + surface.w * 0.5, surface.y + surface.h * 0.5)]

    def _point_inside_surface(self, surface: DesktopSurface, creature: Creature) -> Tuple[float, float]:
        margin = max(18.0, min(80.0, creature.size * 1.2))
        if surface.w <= margin * 2.0 or surface.h <= margin * 2.0:
            return surface.x + surface.w * 0.5, surface.y + surface.h * 0.5
        return (
            self._rng.uniform(surface.x + margin, surface.x + surface.w - margin),
            self._rng.uniform(surface.y + margin, surface.y + surface.h - margin),
        )

    def _nearest_exit_target(self, creature: Creature, surface: DesktopSurface) -> Tuple[float, float]:
        left = abs(creature.x - surface.x)
        right = abs((surface.x + surface.w) - creature.x)
        top = abs(creature.y - surface.y)
        bottom = abs((surface.y + surface.h) - creature.y)
        side = min(((left, "left"), (right, "right"), (top, "top"), (bottom, "bottom")), key=lambda item: item[0])[1]
        push = max(60.0, creature.size * 2.8)
        if side == "left":
            return _clamp(surface.x - push, creature.margin, self.screen_w - creature.margin), _clamp(creature.y, creature.margin, self.screen_h - creature.margin)
        if side == "right":
            return _clamp(surface.x + surface.w + push, creature.margin, self.screen_w - creature.margin), _clamp(creature.y, creature.margin, self.screen_h - creature.margin)
        if side == "top":
            return _clamp(creature.x, creature.margin, self.screen_w - creature.margin), _clamp(surface.y - push, creature.margin, self.screen_h - creature.margin)
        return _clamp(creature.x, creature.margin, self.screen_w - creature.margin), _clamp(surface.y + surface.h + push, creature.margin, self.screen_h - creature.margin)

    def _maybe_seek_desktop_cover(self, creature: Creature, dt: float) -> None:
        if not self.desktop_surfaces or creature.dragging or creature.airborne or creature.cage is not None:
            self._clear_desktop_hide_intent(creature)
            return
        if self._is_fully_hidden(creature) or self._occluding_surfaces_for_creature(creature):
            return
        if not self._behavior_allows_desktop_hide(creature):
            # A visible spider that starts chasing the cursor or playing should not
            # accidentally disappear under a window it happens to cross.
            self._clear_desktop_hide_intent(creature)
            return

        # Let an active, visible hide attempt time out if the creature did not reach
        # the chosen window.
        if getattr(creature, "_desktop_hide_surface_key", None) is not None:
            creature._desktop_hide_timer = max(0.0, float(getattr(creature, "_desktop_hide_timer", 0.0)) - dt)
            if creature._desktop_hide_timer <= 0.0:
                self._clear_desktop_hide_intent(creature)
            else:
                return

        creature._desktop_seek_timer = max(0.0, getattr(creature, "_desktop_seek_timer", 0.0) - dt)
        if creature._desktop_seek_timer > 0.0:
            return
        creature._desktop_seek_timer = self._personality_range(creature, "desktop_hide_seek_interval", 34.0, 86.0)
        # Hideouts should be a rare, conscious action rather than constant random
        # window clipping.  Explorer-type personalities override this and
        # deliberately look for cover/folder portals much more often.
        if self._rng.random() > _clamp(self._personality_float(creature, "desktop_hide_chance", 0.055), 0.0, 1.0):
            return

        max_area_for_intentional_hide = self.screen_w * self.screen_h * self._personality_float(creature, "desktop_hide_max_area_ratio", 0.56)
        candidates = []
        folder_count = len(self._folder_surfaces())
        for surface in self.desktop_surfaces:
            if not self._surface_can_occlude(surface):
                continue
            if surface.w * surface.h > max_area_for_intentional_hide:
                continue
            if self._surface_contains(surface, creature.x, creature.y):
                continue
            if not self._surface_visible_at_point(surface, surface.x + surface.w * 0.5, surface.y + surface.h * 0.5):
                continue
            dist = self._distance_to_surface(surface, creature.x, creature.y)
            if dist >= self._personality_float(creature, "desktop_hide_max_distance", 420.0):
                continue
            # Folder portals only make sense with at least two Explorer folder
            # windows, but an explorer personality may still hide in a single
            # folder window until another portal exit exists.
            if surface.kind == "folder":
                folder_weight = self._personality_float(creature, "desktop_folder_weight", 0.34)
                if folder_count >= 2 and getattr(creature, "_folder_portal_cooldown", 0.0) <= 0.0:
                    weight = folder_weight
                elif creature.personality.get("folder_hider", False):
                    weight = max(0.18, folder_weight * 0.35)
                else:
                    continue
            elif surface.kind == "window":
                weight = self._personality_float(creature, "desktop_window_weight", 1.0)
            else:
                continue
            candidates.append((surface, max(0.01, weight)))
        if not candidates:
            return
        total = sum(weight for _, weight in candidates)
        pick = self._rng.random() * total
        surface = candidates[-1][0]
        for cand, weight in candidates:
            pick -= weight
            if pick <= 0.0:
                surface = cand
                break
        creature._desktop_hide_surface_key = self._surface_key(surface)
        creature._desktop_hide_kind = surface.kind
        creature._desktop_hide_timer = self._personality_range(creature, "desktop_hide_duration", 7.0, 12.0)
        creature.target_x, creature.target_y = self._point_inside_surface(surface, creature)
        creature.target_heading = math.atan2(creature.target_y - creature.y, creature.target_x - creature.x)
        creature.state = "Wander"
        creature.motion_paused = False
        creature.speed = 38.0 * creature._speed_mult() * self._personality_float(creature, "desktop_hide_speed_multiplier", 1.0)
        creature.state_timer = self._personality_range(creature, "desktop_hide_approach_time", 2.4, 4.8)

    def _desktop_occlusion_alpha(self, creature: Creature) -> float:
        # Whole-spider fading was replaced by strict edge clipping.  This method is
        # left as a backwards-safe hook for older render paths.
        return 1.0

    def _teleport_to_folder_exit(self, creature: Creature, source: DesktopSurface) -> bool:
        folders = [surface for surface in self._folder_surfaces() if surface != source]
        if not folders:
            return False
        dest = self._rng.choice(folders)
        side = self._rng.choice(("left", "right", "top", "bottom"))
        margin = max(24.0, creature.size * 1.1)
        exit_push = max(70.0, creature.size * 3.2)
        if side in ("left", "right"):
            y = self._rng.uniform(dest.y + margin, dest.y + max(margin, dest.h - margin)) if dest.h > margin * 2.0 else dest.y + dest.h * 0.5
            if side == "left":
                inside = (dest.x + margin, y)
                outside = (dest.x - exit_push, y + self._rng.uniform(-35.0, 35.0))
            else:
                inside = (dest.x + dest.w - margin, y)
                outside = (dest.x + dest.w + exit_push, y + self._rng.uniform(-35.0, 35.0))
        else:
            x = self._rng.uniform(dest.x + margin, dest.x + max(margin, dest.w - margin)) if dest.w > margin * 2.0 else dest.x + dest.w * 0.5
            if side == "top":
                inside = (x, dest.y + margin)
                outside = (x + self._rng.uniform(-45.0, 45.0), dest.y - exit_push)
            else:
                inside = (x, dest.y + dest.h - margin)
                outside = (x + self._rng.uniform(-45.0, 45.0), dest.y + dest.h + exit_push)
        inside_x = _clamp(inside[0], creature.margin * 0.4, self.screen_w - creature.margin * 0.4)
        inside_y = _clamp(inside[1], creature.margin * 0.4, self.screen_h - creature.margin * 0.4)
        outside_x = _clamp(outside[0], creature.margin, self.screen_w - creature.margin)
        outside_y = _clamp(outside[1], creature.margin, self.screen_h - creature.margin)

        dx = inside_x - creature.x
        dy = inside_y - creature.y
        creature.x = inside_x
        creature.y = inside_y
        creature.target_x = outside_x
        creature.target_y = outside_y
        creature.target_heading = math.atan2(outside_y - inside_y, outside_x - inside_x)
        creature.heading = creature.target_heading
        creature.state = "Wander"
        creature.motion_paused = False
        creature.speed = 54.0 * creature._speed_mult() * self._personality_float(creature, "desktop_hide_speed_multiplier", 1.0)
        creature.current_speed = min(creature.current_speed, creature.speed)
        creature.state_timer = self._personality_range(creature, "folder_portal_exit_time", 1.5, 2.8)
        creature.inertia_timer = 0.0
        creature.inertia_vx = 0.0
        creature.inertia_vy = 0.0
        creature._translate_leg_world_points(dx, dy, 1.0)
        # Mark the destination folder as the intentional occluder immediately.
        # The spider arrives behind that folder and crawls out through its exact
        # border instead of fading in as a whole.
        creature._desktop_hide_surface_key = self._surface_key(dest)
        creature._desktop_hide_kind = "folder"
        creature._desktop_hide_timer = self._personality_range(creature, "desktop_hide_duration", 6.0, 10.0)
        creature._desktop_visibility_alpha = 1.0
        creature._desktop_fully_hidden = True
        creature._desktop_hidden_timer = 0.0
        creature._folder_portal_dwell = 0.0
        creature._folder_portal_threshold = self._personality_range(creature, "folder_portal_dwell", 0.85, 1.39)
        creature._folder_portal_cooldown = self._personality_range(creature, "folder_portal_exit_cooldown", 38.0, 80.0)
        return True

    def _update_folder_portal(self, creature: Creature, dt: float) -> bool:
        creature._folder_portal_cooldown = max(0.0, getattr(creature, "_folder_portal_cooldown", 0.0) - dt)
        if creature.dragging or creature.airborne or creature.cage is not None:
            creature._folder_portal_dwell = 0.0
            return False
        folders = self._folder_surfaces()
        if len(folders) < 2 or creature._folder_portal_cooldown > 0.0:
            creature._folder_portal_dwell = max(0.0, getattr(creature, "_folder_portal_dwell", 0.0) - dt * 2.0)
            return False
        source = self._containing_surface(creature, kind="folder", min_depth=max(14.0, creature.size * 0.45))
        if source is None or not self._surface_visible_at_point(source, creature.x, creature.y):
            creature._folder_portal_dwell = max(0.0, getattr(creature, "_folder_portal_dwell", 0.0) - dt * 2.0)
            return False
        # Only a folder the spider intentionally chose may become a portal.
        if getattr(creature, "_desktop_hide_surface_key", None) != self._surface_key(source):
            creature._folder_portal_dwell = 0.0
            return False
        if getattr(creature, "_desktop_hide_kind", None) != "folder":
            creature._folder_portal_dwell = 0.0
            return False
        creature._folder_portal_dwell += dt
        threshold = float(getattr(creature, "_folder_portal_threshold", 0.85 + (creature.index % 4) * 0.18))
        if creature._folder_portal_dwell >= threshold:
            return self._teleport_to_folder_exit(creature, source)
        return False

    def _update_desktop_awareness(self, creature: Creature, dt: float) -> None:
        if not self.desktop_surfaces:
            creature._desktop_visibility_alpha = 1.0
            creature._desktop_fully_hidden = False
            creature._desktop_hidden_timer = 0.0
            self._clear_desktop_hide_intent(creature)
            return

        if getattr(creature, "_desktop_spawn_grace", 0.0) > 0.0:
            creature._desktop_spawn_grace = max(0.0, float(getattr(creature, "_desktop_spawn_grace", 0.0)) - dt)
            creature._desktop_visibility_alpha = 1.0
            creature._desktop_fully_hidden = False
            creature._desktop_hidden_timer = 0.0
            creature._folder_portal_dwell = 0.0
            return

        teleported = self._update_folder_portal(creature, dt)
        if teleported:
            return

        occluders = self._occluding_surfaces_for_creature(creature)
        fully_hidden = self._creature_fully_covered_by_any_surface(creature, occluders)
        creature._desktop_visibility_alpha = 1.0
        creature._desktop_fully_hidden = fully_hidden
        creature._desktop_occluding_keys = {self._surface_key(surface) for surface in occluders}

        if fully_hidden and not creature.dragging and creature.cage is None:
            creature.social_target = None
            creature._desktop_hidden_timer = getattr(creature, "_desktop_hidden_timer", 0.0) + dt
            # Do not let a spider stay lost behind a normal window forever; after
            # a brief hide it heads toward the nearest edge and crawls back out.
            if creature._desktop_hidden_timer > self._personality_float(creature, "desktop_hidden_linger", 1.8) and not creature.airborne:
                bbox = creature.bounding_rect(False)
                surface = next((s for s in occluders if self._bbox_inside_surface(bbox, s)), None) or self._containing_surface(creature)
                if surface is not None:
                    creature.target_x, creature.target_y = self._nearest_exit_target(creature, surface)
                    creature.target_heading = math.atan2(creature.target_y - creature.y, creature.target_x - creature.x)
                    creature.state = "Wander"
                    creature.motion_paused = False
                    creature.speed = 48.0 * creature._speed_mult()
                    creature.state_timer = self._rng.uniform(1.0, 2.0)
                    creature._desktop_hidden_timer = 0.0
        else:
            creature._desktop_hidden_timer = 0.0

