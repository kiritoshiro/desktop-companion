"""Player-controlled Adventure session rules."""

from __future__ import annotations

import math

from .controls import ControlSettings, clamp_to_cone


class PlayerController:
    WEB_RANGE = 320.0
    WEB_COOLDOWN = 1.2
    WEB_ENERGY = 12.0
    JUMP_ENERGY = 18.0
    JUMP_COOLDOWN = 0.65
    SPRINT_DRAIN = 24.0

    # How far off the aim line a target may be and still be hit, at least.
    AIM_TOLERANCE = 28.0

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
        creature.player_control = self
        creature._prey = None
        creature._hunting_prey = False
        creature._foe = None
        creature.job_mode = "idle"
        creature.enter_idle()

    def release(self):
        self.held.clear()
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
        body = max(8.0, float(getattr(target, "size", 10.0)))
        off_heading = abs((math.atan2(dy, dx) - spider.heading + math.pi) % math.tau - math.pi)
        if off_heading > self.controls.half_cone + math.atan2(body, dist):
            return None
        angle = self.aim_angle()
        along = dx * math.cos(angle) + dy * math.sin(angle)
        if along <= 0.0:
            return None
        across = abs(-dx * math.sin(angle) + dy * math.cos(angle))
        if across > tolerance:
            return None
        return across, dist

    def clear_keys(self):
        self.held.clear()

    def update(self, dt: float):
        spider = self.creature
        self.web_cooldown = max(0.0, self.web_cooldown - dt)
        self.jump_cooldown = max(0.0, self.jump_cooldown - dt)
        if self.paused:
            spider.target_x, spider.target_y = spider.x, spider.y
            spider.speed = 0.0
            spider.motion_paused = True
        else:
            held = self.held
            dx = int("move_right" in held) - int("move_left" in held)
            dy = int("move_down" in held) - int("move_up" in held)
            length = math.hypot(dx, dy)
            moving = length > 0.0
            if self.sprint_exhausted and spider.energy >= spider.max_energy * 0.20:
                self.sprint_exhausted = False
            sprinting = moving and "sprint" in held and not self.sprint_exhausted
            if sprinting:
                spider.energy = max(0.0, spider.energy - self.SPRINT_DRAIN * dt)
                if spider.energy <= 0.0:
                    self.sprint_exhausted = True
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
        if self.paused or spider.dead or spider.airborne or self.jump_cooldown > 0.0 or not spider.has_skill("jump"):
            return False
        if not spider.spend_energy(self.JUMP_ENERGY):
            return False
        dx = spider.target_x - spider.x
        dy = spider.target_y - spider.y
        length = math.hypot(dx, dy)
        if length < 1.0:
            dx, dy = math.cos(spider.heading), math.sin(spider.heading)
            length = 1.0
        spider._launch_jump(spider.x + dx / length * 75.0,
                            spider.y + dy / length * 75.0,
                            kind="pounce", after="idle")
        self.jump_cooldown = self.JUMP_COOLDOWN
        return True

    def bite(self, manager) -> bool:
        spider = self.creature
        if self.paused or spider.dead or spider.airborne or spider.attack_cooldown > 0.0:
            return False
        reach = spider.size * 3.0
        tolerance = max(self.AIM_TOLERANCE, spider.size * 1.5)
        foes = []
        for target in manager.creatures:
            if target is spider or target.dead or spider.relation_to(target) != "foe":
                continue
            hit = self._in_front(target, reach, tolerance)
            if hit is not None:
                foes.append((hit, target))
        if not foes or not manager.conflict_enabled:
            return False
        target = min(foes, key=lambda item: item[0])[1]
        manager._trade_blow(spider, target)
        return True

    def shoot(self, manager) -> bool:
        spider = self.creature
        if self.paused or spider.dead or spider.airborne or self.web_cooldown > 0.0:
            return False
        candidates = []
        for target in manager.creatures:
            if target is spider or target.dead or target.webbed:
                continue
            if spider.relation_to(target) != "foe":
                continue
            candidates.append(target)
        candidates.extend(fly for fly in manager.fly_world.flies
                          if fly.alive and not fly.eaten and not fly.trapped)
        # Anything ahead, inside the cone and near the aim line; the one
        # closest to the line wins, then the nearer of two.
        tolerance = max(self.AIM_TOLERANCE, spider.size * 1.4)
        hits = []
        for target in candidates:
            hit = self._in_front(target, self.WEB_RANGE, tolerance)
            if hit is not None:
                hits.append((hit, target))
        if not hits:
            return False
        target = min(hits, key=lambda item: item[0])[1]
        if spider.energy < self.WEB_ENERGY:
            return False
        if target in manager.creatures:
            launched = manager.fly_world.launch_web_shot_at_creature(spider, target)
        else:
            launched = manager.fly_world.launch_web_shot(spider, target)
        if launched:
            spider.spend_energy(self.WEB_ENERGY)
            self.web_cooldown = self.WEB_COOLDOWN
        return launched
