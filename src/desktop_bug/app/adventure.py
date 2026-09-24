"""Player-controlled Adventure session rules."""

from __future__ import annotations

import math

class PlayerController:
    WEB_RANGE = 320.0
    WEB_COOLDOWN = 1.2
    WEB_ENERGY = 12.0
    JUMP_ENERGY = 18.0
    JUMP_COOLDOWN = 0.65
    SPRINT_DRAIN = 24.0

    def __init__(self, creature):
        self.creature = creature
        self.held: set[int] = set()
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

    def set_held(self, key: int, down: bool):
        if down:
            self.held.add(key)
        else:
            self.held.discard(key)

    def clear_keys(self):
        self.held.clear()

    def update(self, dt: float):
        from PyQt5.QtCore import Qt

        spider = self.creature
        self.web_cooldown = max(0.0, self.web_cooldown - dt)
        self.jump_cooldown = max(0.0, self.jump_cooldown - dt)
        if self.paused:
            spider.target_x, spider.target_y = spider.x, spider.y
            spider.speed = 0.0
            spider.motion_paused = True
        else:
            dx = int(Qt.Key_D in self.held) - int(Qt.Key_A in self.held)
            dy = int(Qt.Key_S in self.held) - int(Qt.Key_W in self.held)
            length = math.hypot(dx, dy)
            moving = length > 0.0
            if self.sprint_exhausted and spider.energy >= spider.max_energy * 0.20:
                self.sprint_exhausted = False
            sprinting = moving and Qt.Key_Shift in self.held and not self.sprint_exhausted
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
                ax, ay = self.aim
                if math.hypot(ax - spider.x, ay - spider.y) > spider.size:
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
        ax, ay = self.aim
        reach = spider.size * 3.0
        foes = [target for target in manager.creatures
                if target is not spider and not target.dead
                and spider.relation_to(target) == "foe"
                and math.hypot(target.x - spider.x, target.y - spider.y) <= reach
                and math.hypot(target.x - ax, target.y - ay) <= max(24.0, spider.size * 1.5)]
        if not foes or not manager.conflict_enabled:
            return False
        target = min(foes, key=lambda obj: math.hypot(obj.x - ax, obj.y - ay))
        manager._trade_blow(spider, target)
        return True

    def shoot(self, manager) -> bool:
        spider = self.creature
        if self.paused or spider.dead or spider.airborne or self.web_cooldown > 0.0:
            return False
        ax, ay = self.aim
        if math.hypot(ax - spider.x, ay - spider.y) > self.WEB_RANGE:
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
        candidates = [target for target in candidates
                      if math.hypot(target.x - spider.x, target.y - spider.y) <= self.WEB_RANGE]
        if not candidates:
            return False
        target = min(candidates, key=lambda obj: math.hypot(obj.x - ax, obj.y - ay))
        if math.hypot(target.x - ax, target.y - ay) > max(20.0, spider.size * 1.4):
            return False
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
