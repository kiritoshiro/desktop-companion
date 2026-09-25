"""Player-controlled Adventure session rules."""

from __future__ import annotations

import math

from .controls import ControlSettings, clamp_to_cone


class PlayerController:
    """One spider's Adventure rules: aim cone, stamina, silk, cooldowns, reach.

    The player's spider and every mission spider (MissionActor subclasses
    this) act through the same ``shoot``, ``bite`` and ``jump``, so no spider
    can do what the player's cannot -- the owner: "make sure all spiders have
    the same rules ... its just that i can control mine."
    """

    WEB_RANGE = 320.0
    WEB_COOLDOWN = 1.2
    WEB_ENERGY = 12.0
    JUMP_ENERGY = 18.0
    JUMP_COOLDOWN = 0.65
    SPRINT_DRAIN = 24.0
    BITE_REACH = 3.0             # body sizes
    POUNCE_DISTANCE = 75.0       # px a pounce carries
    SILK_CAPACITY = 8
    LOOM_SILK_CAPACITY = 12      # while your side holds the Silk loom

    # How far off the aim line a target may be and still be hit, at least.
    AIM_TOLERANCE = 28.0
    # A bite that finds nothing still has to be recovered from.
    WHIFF_RECOVERY = 0.38
    # Turn-and-walk controls. A tarantula backs up slowly and briefly.
    BACK_UP_SPEED = 0.40         # of the walking speed
    TURN_LEAD = 0.6              # radians asked ahead of the body while turning

    def __init__(self, creature, controls: ControlSettings | None = None):
        self.creature = creature
        self.controls = controls if controls is not None else ControlSettings()
        # Actions held down ("move_up", "sprint", ...), not raw keys, so a
        # rebinding never has to reach in here.
        self.held: set[str] = set()
        self.aim = (creature.x, creature.y)
        self.web_cooldown = 0.0
        self.jump_cooldown = 0.0
        self.paused = False
        self.sprint_exhausted = False
        self.silk_capacity = self.SILK_CAPACITY
        self.silk = float(self.silk_capacity)
        self.feedback = ""
        self.feedback_time = 0.0
        self._was_webbed = False
        creature.player_control = self
        creature._prey = None
        creature._hunting_prey = False
        creature._foe = None
        creature.job_mode = "idle"
        creature.enter_idle()

    def release(self):
        self.held.clear()
        self.creature.reverse_walk = False
        if self.creature.player_control is self:
            self.creature.player_control = None
            self.creature.enter_idle()

    def set_held(self, key, down: bool):
        """Hold or release an action; a Qt key code is looked up in the bindings."""
        action = key if isinstance(key, str) else self.controls.action_for_key(int(key))
        if action is None:
            return
        if down:
            self.held.add(action)
        else:
            self.held.discard(action)

    # -- aiming -----------------------------------------------------------
    def aim_angle(self) -> float:
        """Where a shot goes: towards the pointer, inside the view cone."""
        spider = self.creature
        ax, ay = self.aim
        if math.hypot(ax - spider.x, ay - spider.y) < 1.0:
            return spider.heading
        wanted = math.atan2(ay - spider.y, ax - spider.x)
        return clamp_to_cone(spider.heading, wanted, self.controls.half_cone)

    def in_cone(self, target) -> bool:
        """Is ``target`` inside the view cone (allowing for its own size)?"""
        spider = self.creature
        dx, dy = target.x - spider.x, target.y - spider.y
        dist = math.hypot(dx, dy)
        if dist < 1e-6:
            return True
        body = max(8.0, float(getattr(target, "size", 10.0)))
        off_heading = abs((math.atan2(dy, dx) - spider.heading + math.pi) % math.tau - math.pi)
        return off_heading <= self.controls.half_cone + math.atan2(body, dist)

    def bite_reach(self) -> float:
        return self.creature.size * self.BITE_REACH

    def _in_front(self, target, reach: float, tolerance: float):
        """(offset from the aim line, distance) if ``target`` can be hit, else None.

        It must be within ``reach``, inside the cone (allowing for its own
        size), ahead of the spider, and within ``tolerance`` of the aim line.
        """
        spider = self.creature
        dx, dy = target.x - spider.x, target.y - spider.y
        dist = math.hypot(dx, dy)
        if dist > reach or dist < 1e-6:
            return None
        if not self.in_cone(target):
            return None
        angle = self.aim_angle()
        along = dx * math.cos(angle) + dy * math.sin(angle)
        if along <= 0.0:
            return None
        across = abs(-dx * math.sin(angle) + dy * math.cos(angle))
        if across > tolerance:
            return None
        return across, dist

    @property
    def struggling(self) -> bool:
        """Webbed, the player fights the silk by holding a movement key."""
        return bool(self.held & {"move_up", "move_down", "move_left", "move_right"})

    def _webbed_feedback(self) -> None:
        """Trapped in silk, the spider cannot bite or shoot (the owner)."""
        self.feedback = "Webbed - struggle free before you can fight."
        self.feedback_time = 1.6

    def clear_keys(self):
        self.held.clear()

    def update(self, dt: float):
        spider = self.creature
        self.feedback_time = max(0.0, self.feedback_time - dt)
        if spider.webbed and not self._was_webbed:
            self.feedback = "Webbed! Hold a direction to struggle free."
            self.feedback_time = 2.5
        self._was_webbed = spider.webbed
        self.web_cooldown = max(0.0, self.web_cooldown - dt)
        self.jump_cooldown = max(0.0, self.jump_cooldown - dt)
        if self.paused:
            spider.target_x, spider.target_y = spider.x, spider.y
            spider.speed = 0.0
            spider.motion_paused = True
            spider.reverse_walk = False
        elif self.controls.turn_movement:
            self._update_turn_movement(dt)
        else:
            spider.reverse_walk = False
            held = self.held
            dx = int("move_right" in held) - int("move_left" in held)
            dy = int("move_down" in held) - int("move_up" in held)
            length = math.hypot(dx, dy)
            moving = length > 0.0
            sprinting = self._sprinting(moving, dt)
            if not spider.airborne:
                spider.state = "Player"
            spider.motion_paused = not moving
            spider.speed = (190.0 if sprinting else 115.0) * spider._speed_mult()
            if moving:
                spider.target_x = spider.x + dx / length * 100.0
                spider.target_y = spider.y + dy / length * 100.0
            else:
                spider.target_x, spider.target_y = spider.x, spider.y
                # The pointer only aims (the owner's choice); turning to face
                # it while standing is an option in the controls settings.
                ax, ay = self.aim
                if (self.controls.face_mouse_when_still
                        and math.hypot(ax - spider.x, ay - spider.y) > spider.size):
                    spider.target_heading = math.atan2(ay - spider.y, ax - spider.x)
        if spider.webbed:
            spider.speed *= 0.25
        self.advance_pose(dt)

    def _sprinting(self, moving: bool, dt: float) -> bool:
        spider = self.creature
        if self.sprint_exhausted and spider.energy >= spider.max_energy * 0.20:
            self.sprint_exhausted = False
        sprinting = moving and "sprint" in self.held and not self.sprint_exhausted
        if sprinting:
            spider.energy = max(0.0, spider.energy - self.SPRINT_DRAIN * dt)
            if spider.energy <= 0.0:
                self.sprint_exhausted = True
        return sprinting

    def _update_turn_movement(self, dt: float) -> None:
        """W forward, S back up (still facing forward), A/D turn."""
        spider = self.creature
        held = self.held
        forward = int("move_up" in held) - int("move_down" in held)
        turn = int("move_right" in held) - int("move_left" in held)
        backing = forward < 0
        sprinting = self._sprinting(forward > 0, dt)
        if not spider.airborne:
            spider.state = "Player"
        spider.reverse_walk = backing
        spider.motion_paused = forward == 0
        spider.speed = (190.0 if sprinting else 115.0) * spider._speed_mult()
        if backing:
            spider.speed *= self.BACK_UP_SPEED
        # Where the body should face: ahead of it by a fixed lead while a turn
        # is held, so the gait keeps turning at its own rate.
        facing = spider.heading + turn * self.TURN_LEAD
        if forward > 0:
            spider.target_x = spider.x + math.cos(facing) * 100.0
            spider.target_y = spider.y + math.sin(facing) * 100.0
        elif backing:
            # Walk away from the way it faces, and keep facing it.
            spider.target_heading = facing
            spider.target_x = spider.x - math.cos(spider.heading) * 100.0
            spider.target_y = spider.y - math.sin(spider.heading) * 100.0
        else:
            spider.target_x, spider.target_y = spider.x, spider.y
            if turn:
                spider.target_heading = facing

    def advance_pose(self, dt: float):
        spider = self.creature
        if spider.airborne:
            spider._update_jump(dt)
        else:
            spider._aim_at_a_real_screen()
            if spider._spider_grounded_mode_allowed():
                spider._update_spider_grounded_frame(dt)
            else:
                spider._move_body(dt)
        if spider.cage is not None:
            spider._apply_cage_bounds()
        spider._reconcile_roll()
        spider._update_legs(dt)
        spider._update_mood(dt)
        spider._update_posture(dt)
        spider._update_antennae(dt)

    def jump(self) -> bool:
        spider = self.creature
        if (self.paused or spider.dead or spider.airborne or spider.webbed_held
                or self.jump_cooldown > 0.0 or not spider.has_skill("jump")):
            return False
        if not spider.spend_energy(self.JUMP_ENERGY):
            return False
        dx = spider.target_x - spider.x
        dy = spider.target_y - spider.y
        length = math.hypot(dx, dy)
        if length < 1.0 or spider.reverse_walk:
            # Standing, or backing up: pounce the way the spider faces.
            dx, dy = math.cos(spider.heading), math.sin(spider.heading)
            length = 1.0
        spider._launch_jump(spider.x + dx / length * self.POUNCE_DISTANCE,
                            spider.y + dy / length * self.POUNCE_DISTANCE,
                            kind="pounce", after="idle")
        self.jump_cooldown = self.JUMP_COOLDOWN
        return True

    def bite(self, manager) -> bool:
        spider = self.creature
        if self.paused or spider.dead or spider.airborne or spider.attack_cooldown > 0.0:
            return False
        if spider.webbed_held:
            self._webbed_feedback()
            return False
        reach = self.bite_reach()
        tolerance = max(self.AIM_TOLERANCE, spider.size * 1.5)
        foes = []
        for target in manager.creatures:
            if target is spider or target.dead or spider.relation_to(target) != "foe":
                continue
            hit = self._in_front(target, reach, tolerance)
            if hit is not None:
                foes.append((hit, target))
        if not foes or not manager.conflict_enabled:
            # A miss still bites the air, so every press shows, and costs the
            # same short recovery as the animation.
            angle = self.aim_angle()
            spider.begin_strike(spider.x + math.cos(angle) * 100.0,
                                spider.y + math.sin(angle) * 100.0)
            spider.attack_cooldown = max(spider.attack_cooldown, self.WHIFF_RECOVERY)
            return False
        target = min(foes, key=lambda item: item[0])[1]
        manager._trade_blow(spider, target)
        return True

    def shoot(self, manager) -> bool:
        spider = self.creature
        if self.paused or spider.dead or spider.airborne or self.web_cooldown > 0.0:
            return False
        if spider.webbed_held:
            self._webbed_feedback()
            return False
        if self.silk < 1 or spider.energy < self.WEB_ENERGY:
            self.feedback = ("Silk empty - refill at home or a captured loom"
                             if self.silk < 1 else "Not enough stamina")
            self.feedback_time = 2.5
            return False
        from ..world.aimed_silk import AimedSilk

        manager.fly_world.projectiles.append(AimedSilk(
            spider, self.aim_angle(), self.WEB_RANGE,
            lambda: manager.creatures, lambda: manager.fly_world.flies))
        spider.spend_energy(self.WEB_ENERGY)
        self.silk -= 1
        self.web_cooldown = self.WEB_COOLDOWN
        return True
