"""Weaving, repairing, walking and shooting silk.

Split out of the single ``behaviour.py`` by DC-43; a pure move.
"""
from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    pass

from ...support.math_utils import (
    angle_to,
    clamp,
    distance,
    rand_range,
)


class WebBehaviourMixin:
    """Weaving, repairing, walking and shooting silk."""

    def _begin_weave(self) -> bool:
        """Claim a fresh site and start walking to it to build.

        Corners are still favoured, but a webber will often pick an open spot out
        in the room so its webs end up scattered around rather than only hugging
        the screen edges.
        """
        if self.web_world is None or self.cage is not None:
            return False
        prefer_corner = self.rng.random() < float(self.personality.get("web_corner_bias", 0.55))
        web = self.web_world.claim_site(self, prefer_corner=prefer_corner)
        if web is None:
            # Every site is taken; back off briefly before trying again.
            self.weave_cooldown = rand_range(self.personality.get("weave_retry_cooldown"), 5.0, 10.0, rng=self.rng)
            return False
        self.weaving_web = web
        self.enter_weave_approach()
        return True

    def _begin_adopt(self, web) -> bool:
        """Take over an abandoned, unfinished web to finish it off."""
        if self.web_world is None or web is None:
            return False
        if not self.web_world.adopt(web, self):
            return False
        self.weaving_web = web
        self.enter_weave_approach()
        return True

    def _begin_web_walk(self, web) -> bool:
        """Walk onto a finished web to bounce-test it."""
        if self.web_world is None or web is None or not web.is_complete():
            return False
        self.web_target = web
        self.enter_web_approach()
        return True

    def _abandon_weaving(self) -> None:
        """Release the in-progress web so any spider can finish it later."""
        if self.weaving_web is not None and self.web_world is not None:
            self.web_world.abandon(self.weaving_web)
        self.weaving_web = None
        self._weave_drawing = False
        # Short cooldown so an interrupted spider does not instantly re-weave.
        self.weave_cooldown = max(self.weave_cooldown,
                                  rand_range(self.personality.get("weave_retry_cooldown"), 3.5, 7.5, rng=self.rng))

    def _finish_weave(self) -> None:
        self.weaving_web = None
        self._weave_drawing = False
        base = rand_range(self.personality.get("weave_cooldown"), 8.0, 16.0, rng=self.rng)
        self.weave_cooldown = base / (4.0 if self._acts_as_webber() else 1.0)
        self.mood.bump(arousal=-0.05, valence=0.18, affection=0.04)
        self.enter_idle()

    def _begin_repair(self, web) -> bool:
        """Claim a torn finished web and walk over to re-knit it."""
        if self.web_world is None or web is None:
            return False
        if not self.web_world.claim_repair(web, self):
            return False
        self.repairing_web = web
        self.enter_repair_approach()
        return True

    def _abandon_repair(self) -> None:
        if self.repairing_web is not None and self.web_world is not None:
            self.web_world.release_repair(self.repairing_web)
        self.repairing_web = None
        self.weave_cooldown = max(self.weave_cooldown,
                                  rand_range(self.personality.get("weave_retry_cooldown"), 2.5, 5.0, rng=self.rng))

    def _finish_repair(self) -> None:
        if self.repairing_web is not None and self.web_world is not None:
            self.web_world.release_repair(self.repairing_web)
        self.repairing_web = None
        # Mending is satisfying but quick to come off cooldown for a webber, so it
        # stays attentive to further damage.
        base = rand_range(self.personality.get("weave_cooldown"), 8.0, 16.0, rng=self.rng)
        self.weave_cooldown = base / (6.0 if self._acts_as_webber() else 1.5)
        self.mood.bump(valence=0.14, affection=0.05, arousal=-0.04)
        self.enter_idle()

    def _repair_point(self):
        web = self.repairing_web
        if web is None:
            return None
        cen = web.damaged_centroid()
        if cen is None:
            return None
        # Stand a little inset so the spider can actually reach the torn area.
        ins = web.reach_inset
        return (clamp(cen[0], ins, self.screen_w - ins),
                clamp(cen[1], ins, self.screen_h - ins))

    def enter_repair_approach(self) -> None:
        if not self.has_skill("weave_web") or self.repairing_web is None:
            self.enter_idle()
            return
        self.state = "RepairApproach"
        self.motion_paused = False
        point = self._repair_point()
        if point is None:
            self._finish_repair()
            return
        self.target_x, self.target_y = point
        self.speed = max(48.0, self._weave_speed * 0.95) * self._speed_mult()
        self.state_timer = 7.0  # safety: do not approach forever
        self.mood.bump(arousal=0.06, valence=0.03)

    def _update_repair_approach(self, dt: float, mx: float, my: float) -> None:
        web = self.repairing_web
        if web is None or self.web_world is None or web not in self.web_world.webs:
            self._abandon_repair()
            self.enter_idle()
            return
        if not web.is_damaged():
            # Someone else mended it, or it got removed/rebuilt meanwhile.
            self._finish_repair()
            return
        point = self._repair_point()
        if point is None:
            self._finish_repair()
            return
        self.target_x, self.target_y = point
        self._set_focus(point[0], point[1], 0.6)
        if distance(self.x, self.y, point[0], point[1]) < 18.0 or self.state_timer <= 0.0:
            self.enter_repair()

    def enter_repair(self) -> None:
        if self.repairing_web is None:
            self.enter_idle()
            return
        self.state = "Repair"
        self.motion_paused = False
        self.speed = self._weave_speed * 0.8 * self._speed_mult()
        self.state_timer = 20.0  # safety cap on a single mend
        self.mood.bump(arousal=0.03, valence=0.05)

    def _update_repair(self, dt: float, mx: float, my: float) -> None:
        web = self.repairing_web
        if web is None or self.web_world is None or web not in self.web_world.webs:
            self._abandon_repair()
            self.enter_idle()
            return
        if not web.is_damaged():
            self._finish_repair()
            return
        # Walk to the torn area and re-knit the broken segments nearest the
        # spider first, so the mend reads as the spider working across the damage.
        point = self._repair_point()
        if point is not None:
            self.target_x, self.target_y = point
            self._set_focus(point[0], point[1], 0.5)
            close = distance(self.x, self.y, point[0], point[1]) < self.size * 2.2
        else:
            close = True
        self._weave_drawing = True
        if close:
            self.speed = 0.0
        # Mend regardless of exact proximity once committed, mending nearest to
        # the spider first; the approach above is visual.
        web.repair_near(self.x, self.y, dt)
        if not web.is_damaged():
            self._finish_repair()
            return
        if self.state_timer <= 0.0:
            self._abandon_repair()
            self.enter_idle()

    def enter_weave_approach(self) -> None:
        if not self.has_skill("weave_web") or self.weaving_web is None:
            self.enter_idle()
            return
        self.state = "WeaveApproach"
        self.motion_paused = False
        point, _drawing = self.weaving_web.working_point(
            screen_w=self.screen_w, screen_h=self.screen_h)
        self.target_x, self.target_y = point
        self.speed = max(46.0, self._weave_speed * 0.95) * self._speed_mult()
        self.state_timer = 7.0  # safety: do not approach forever
        self.mood.bump(arousal=0.05, valence=0.05)

    def _update_weave_approach(self, dt: float, mx: float, my: float) -> None:
        web = self.weaving_web
        if web is None or self.web_world is None or web not in self.web_world.webs:
            self.weaving_web = None
            self.enter_idle()
            return
        if web.is_complete():
            # Someone finished it while this spider was on its way over.
            self.weaving_web = None
            self.weave_cooldown = rand_range(self.personality.get("weave_cooldown"), 6.0, 14.0, rng=self.rng)
            self.enter_idle()
            return
        point, _drawing = web.working_point(screen_w=self.screen_w, screen_h=self.screen_h)
        self.target_x, self.target_y = point
        self._set_focus(point[0], point[1], 0.6)
        if distance(self.x, self.y, point[0], point[1]) < 16.0 or self.state_timer <= 0.0:
            self.enter_weave()

    def enter_weave(self) -> None:
        if not self.has_skill("weave_web") or self.weaving_web is None:
            self.enter_idle()
            return
        self.state = "Weave"
        self.motion_paused = False
        self.speed = self._weave_speed * self._speed_mult()
        self.state_timer = 34.0  # generous safety cap for a whole web
        self.mood.bump(arousal=0.04, valence=0.06)

    def _update_weave(self, dt: float, mx: float, my: float) -> None:
        web = self.weaving_web
        if web is None or self.web_world is None or web not in self.web_world.webs:
            self.weaving_web = None
            self.enter_idle()
            return
        if web.is_complete():
            self._finish_weave()
            return
        # Trace the silk: aim at the current working point and lay thread while
        # the body keeps up with it.  ``advance`` only extends silk while the
        # spider trails the reachable tip, so the strand stays pinned to it.
        point, drawing = web.working_point(screen_w=self.screen_w, screen_h=self.screen_h)
        self._weave_drawing = drawing
        self.target_x, self.target_y = point
        # Reposition runs (lifting silk to a new anchor) travel a lot quicker
        # so only the actual silk-laying reads as a careful, deliberate pace.
        self.speed = self._weave_speed * (1.0 if drawing else 2.4) * self._speed_mult()
        self._set_focus(point[0], point[1], 0.5)
        # Loosen the lead while weaving: the silk tip may run a little ahead of
        # the body (the spinnerets lead the body centre), so the build is not
        # throttled to a crawl every time the body has to turn between radii.
        web.advance(dt, self._weave_speed, (self.x, self.y), lead_max=58.0)
        if web.is_complete():
            self._finish_weave()
            return
        if self.state_timer <= 0.0:
            # Took too long (blocked path); leave it adoptable and move on.
            self._abandon_weaving()
            self.enter_idle()

    def enter_web_approach(self) -> None:
        if not self.has_skill("web_walk") or self.web_target is None:
            self.enter_idle()
            return
        self.state = "WebApproach"
        self.motion_paused = False
        px, py = self.web_target.walkable_point(self.screen_w, self.screen_h)
        self._web_walk_point = (px, py)
        self.target_x, self.target_y = px, py
        self.speed = max(48.0, self._weave_speed * 0.85) * self._speed_mult()
        self.state_timer = 7.0

    def _update_web_approach(self, dt: float, mx: float, my: float) -> None:
        web = self.web_target
        if (web is None or self.web_world is None or web not in self.web_world.webs
                or not web.is_complete()):
            self.web_target = None
            self.enter_idle()
            return
        px, py = self._web_walk_point if self._web_walk_point is not None else (web.hub[0], web.hub[1])
        self.target_x, self.target_y = px, py
        self._set_focus(px, py, 0.5)
        if distance(self.x, self.y, px, py) < 16.0 or self.state_timer <= 0.0:
            self.enter_web_walk()

    def enter_web_walk(self) -> None:
        if self.web_target is None:
            self.enter_idle()
            return
        self.state = "WebWalk"
        self.motion_paused = True
        self.speed = 0.0
        self._web_pluck_count = self.rng.randint(2, 4)
        self._web_pluck_timer = rand_range(None, 0.25, 0.5, rng=self.rng)
        self.state_timer = 6.0
        self.mood.bump(arousal=0.06, valence=0.05, curiosity=0.05)

    def _update_web_walk(self, dt: float, mx: float, my: float) -> None:
        web = self.web_target
        if web is None or self.web_world is None or web not in self.web_world.webs:
            self.web_target = None
            self.motion_paused = False
            self.enter_idle()
            return
        # Stand on the web and pluck it; the web's own wobble is the bounce test.
        self.motion_paused = True
        self.speed = 0.0
        self.target_x, self.target_y = self.x, self.y
        self._set_focus(web.hub[0], web.hub[1], 0.4)
        self._web_pluck_timer -= dt
        if self._web_pluck_timer <= 0.0:
            if self._web_pluck_count > 0:
                web.pluck((self.x, self.y), strength=self.rng.uniform(0.6, 1.3))
                self._web_pluck_count -= 1
                self._web_pluck_timer = rand_range(None, 0.5, 0.95, rng=self.rng)
                self.mood.bump(arousal=0.03, valence=0.04, curiosity=0.03)
            else:
                self._leave_web_walk()
                return
        if self.state_timer <= 0.0:
            self._leave_web_walk()

    def _leave_web_walk(self) -> None:
        self.web_target = None
        self.web_walk_cooldown = rand_range(self.personality.get("web_walk_cooldown"), 8.0, 20.0, rng=self.rng)
        self.motion_paused = False
        self.enter_idle()

    def _can_shoot_web(self, kind: str, prey=None) -> bool:
        skill = "wall_web" if kind == "wall" else "shoot_web"
        if not self.has_skill(skill) or self.airborne:
            return False
        if prey is not None:
            # Firing at a fly goes through the fly world and never touches the
            # real pointer, so neither the cursor-capture toggle nor being held
            # by the mouse gates it: a spider you are carrying can still web a fly.
            return self.perception.can_shoot_prey_web()
        if self.dragging:
            return False
        world = self.mouse_web_world
        return (
            world is not None
            and getattr(world, "enabled", False)
            and not world.busy()
        )

    def _maybe_shoot_web_at_cursor(self, dist_to_cursor: float, mx: float, my: float) -> bool:
        """Roll to fire sticky silk at the pointer. Returns True if it committed.

        Shared by the hunting states and the generic idle decision so any spider
        with the skill can use it, while a web-shooter does it eagerly.
        """
        # When hunting a fly the "cursor" coordinates are really the prey, so the
        # pointer-capture silk must never fire here (it would grab the real
        # mouse). The manager traps flies through the fly world instead.
        if self._hunting_prey:
            return False
        if self.web_shot_cooldown > 0.0:
            return False
        can_trap = self._can_shoot_web("trap")
        can_wall = self._can_shoot_web("wall")
        if not (can_trap or can_wall):
            return False
        web_shooter = self._acts_as_web_shooter()
        reaction = float(self.personality.get("reaction_radius", 360))
        range_mult = float(self.personality.get("web_shot_range_mult",
                                                0.85 if web_shooter else 0.5))
        shot_range = max(self.size * 4.0, reaction * range_mult)
        if dist_to_cursor > shot_range:
            return False
        m = self.mood
        boldness = clamp(float(self.personality.get("boldness", 0.5)), 0.0, 1.0)
        chance = float(self.personality.get("web_shot_chance", 0.6 if web_shooter else 0.05))
        # A still pointer is an easy mark; excitement and boldness help.
        if self._cursor_is_still_for_observe():
            chance *= 1.55
        chance = clamp(chance + m.arousal * 0.15 + boldness * 0.1, 0.0, 0.97)
        if self.rng.random() >= chance:
            return False
        # Pick the shot. The wall shove is a flashier finisher used a little less
        # often when the spider can also trap in place.
        kind = "trap"
        if can_wall and (not can_trap or
                         self.rng.random() < float(self.personality.get("wall_web_bias", 0.3))):
            kind = "wall"
        self.enter_web_aim(mx, my, kind)
        return True

    def _maybe_shoot_web_at_prey(self, prey) -> bool:
        """Roll to fling trapping silk at a fly. Returns True if it committed.

        The same shot the spider uses on the cursor, aimed at prey instead. A
        web-shooter does it eagerly; any spider with the skill does it sometimes.
        """
        if prey is None or self.web_shot_cooldown > 0.0:
            return False
        if prey.trapped or prey.dragging or not prey.alive:
            return False
        can_trap = self._can_shoot_web("trap", prey=prey)
        can_wall = self._can_shoot_web("wall", prey=prey)
        if not (can_trap or can_wall):
            return False
        web_shooter = self._acts_as_web_shooter()
        reaction = float(self.personality.get("reaction_radius", 360))
        range_mult = float(self.personality.get("web_shot_range_mult",
                                                0.85 if web_shooter else 0.5))
        shot_range = max(self.size * 4.0, reaction * range_mult)
        d = distance(self.x, self.y, prey.x, prey.y)
        if d > shot_range or d < self.size * 1.4:
            return False
        boldness = clamp(float(self.personality.get("boldness", 0.5)), 0.0, 1.0)
        chance = float(self.personality.get("web_shot_chance", 0.6 if web_shooter else 0.12))
        chance = clamp(chance + self.mood.arousal * 0.15 + boldness * 0.1, 0.0, 0.97)
        if self.rng.random() >= chance:
            return False
        kind = "trap"
        if can_wall and (not can_trap or
                         self.rng.random() < float(self.personality.get("wall_web_bias", 0.3))):
            kind = "wall"
        self.enter_web_aim(prey.x, prey.y, kind, prey=prey)
        return True

    def enter_web_aim(self, mx: float, my: float, kind: str = "trap", prey=None) -> None:
        """Crouch and range the target, then fire a glob of sticky silk."""
        if not self._can_shoot_web(kind, prey=prey):
            self._end_web_state()
            return
        self._web_shot_prey = prey
        self.state = "WebAim"
        self.motion_paused = True
        self.speed = 0.0
        self.social_target = None
        self._web_shot_kind = "wall" if kind == "wall" else "trap"
        self._set_focus(mx, my, 1.0)
        self.target_heading = angle_to(self.x, self.y, mx, my)
        self.state_timer = rand_range(self.personality.get("web_aim_time"), 0.3, 0.58, rng=self.rng)
        self.range_clock = 0.0
        self.range_mode = "waggle"
        self.range_switch = self.rng.uniform(0.14, 0.28)
        self.mood.bump(arousal=0.2, curiosity=0.05)

    def _update_web_aim(self, dt: float, mx: float, my: float) -> None:
        prey = self._web_shot_prey
        if prey is not None:
            # Aiming at a fly: bail if it got caught/eaten/grabbed first, and
            # keep tracking its live position as it tries to flee the aim.
            if not prey.alive or prey.eaten or prey.dragging or prey.trapped:
                self._finish_web_shot(fired=False)
                return
            mx, my = prey.x, prey.y
        else:
            # Bail out if pointer-silk got turned off or something else grabbed
            # the pointer while we were ranging.
            world = self.mouse_web_world
            if world is None or not getattr(world, "enabled", False) or world.busy():
                self._finish_web_shot(fired=False)
                return
        self.motion_paused = True
        self.speed = 0.0
        self.target_x, self.target_y = self.x, self.y
        self._set_focus(mx, my, 1.0)
        self.target_heading = angle_to(self.x, self.y, mx, my)
        # Converge the feelers into a rangefinder and coil low, like the pounce aim.
        self.aim_intent = min(1.0, self.aim_intent + dt * 3.2)
        self.crouch = min(1.0, self.crouch + dt * 4.0)
        self.range_clock += dt
        if self.range_clock >= self.range_switch:
            self.range_clock = 0.0
            self.range_switch = self.rng.uniform(0.14, 0.28)
            self.range_mode = "nod" if self.range_mode == "waggle" else "waggle"
            if self.range_mode == "waggle":
                self.wiggle_burst = max(self.wiggle_burst, 0.6)
        if self.state_timer <= 0.0:
            self._fire_web_shot(mx, my)

    def _fire_web_shot(self, mx: float, my: float) -> None:
        kind = self._web_shot_kind
        prey = self._web_shot_prey
        # Launch from a little ahead of the body, where the spinnerets/front are.
        fx, fy, _rx, _ry = self._basis()
        origin = (self.x + fx * self.size * 0.6, self.y + fy * self.size * 0.6)
        launched = False
        if prey is not None:
            if self.fly_world is not None and self._can_shoot_web(kind, prey=prey):
                launched = self.fly_world.launch_web_shot(self, prey, kind=kind)
        else:
            world = self.mouse_web_world
            if world is not None and self._can_shoot_web(kind):
                # Silk is thrown, not guided: aim once, leading the pointer by
                # its current velocity. In-flight correction arrives with the
                # Silk tracking ability rather than being free from level one.
                launched = world.shoot(
                    origin,
                    (mx, my),
                    kind=kind,
                    homing=self._progression_effect("web_homing"),
                    lead=(self.prev_cursor_vx, self.prev_cursor_vy),
                )
        if launched:
            self.mood.bump(arousal=0.12, valence=0.12, curiosity=0.05)
            self.enter_web_shot()
        else:
            self._finish_web_shot(fired=False)

    def enter_web_shot(self) -> None:
        """Brief recoil hold right after letting the silk fly."""
        self.state = "WebShot"
        self.motion_paused = True
        self.speed = 0.0
        self.state_timer = rand_range(self.personality.get("web_recoil_time"), 0.18, 0.3, rng=self.rng)
        self.rear = min(1.0, self.rear + 0.4)
        self.wiggle_burst = max(self.wiggle_burst, 0.5)

    def _update_web_shot(self, dt: float, mx: float, my: float) -> None:
        self.motion_paused = True
        self.speed = 0.0
        self.target_x, self.target_y = self.x, self.y
        self._set_focus(mx, my, 0.7)
        # Recover from the crouch as the body uncoils out of the shot.
        self.crouch = max(0.0, self.crouch - dt * 3.5)
        if self.state_timer <= 0.0:
            self._finish_web_shot(fired=True)

    def _finish_web_shot(self, fired: bool) -> None:
        web_shooter = self._acts_as_web_shooter()
        prey_shot = self._web_shot_prey is not None
        self._web_shot_prey = None
        if fired:
            base = rand_range(self.personality.get("web_shot_cooldown"), 8.0, 18.0, rng=self.rng)
        else:
            # Did not actually fire (blocked/cancelled): retry sooner.
            base = rand_range(self.personality.get("web_shot_retry_cooldown"), 2.0, 4.5, rng=self.rng)
        self.web_shot_cooldown = base / (3.5 if web_shooter else 1.0)
        if prey_shot:
            # After webbing a fly the spider should close in promptly to eat it,
            # so it is not blocked from firing again by a long cursor cooldown.
            self.web_shot_cooldown = min(self.web_shot_cooldown, 1.2)
        self._end_web_state()

    def _end_web_state(self) -> None:
        """Leave a web aim/shot cleanly: back to being held if still dragged."""
        self.motion_paused = False
        if self.dragging:
            self.state = "Dragged"
            self.motion_paused = True
            self.speed = 0.0
        else:
            self.enter_idle()

