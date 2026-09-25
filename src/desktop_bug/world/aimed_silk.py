"""Visible straight silk, with swept collision so fast shots cannot tunnel."""
from __future__ import annotations

import math
from types import SimpleNamespace

from .flies import WebShotProjectile, WEB_SHOT_SPEED, _pin_fly_with_shot


class AimedSilk(WebShotProjectile):
    def __init__(self, shooter, angle, reach, creatures, flies):
        endpoint = SimpleNamespace(x=shooter.x + math.cos(angle) * reach,
                                   y=shooter.y + math.sin(angle) * reach)
        super().__init__(shooter, endpoint, is_gone=lambda _: False)
        self.pos = (shooter.x + math.cos(angle) * shooter.size * 0.7,
                    shooter.y + math.sin(angle) * shooter.size * 0.7)
        self.origin = self.pos
        self.trail = [self.pos]
        self.vel = (math.cos(angle) * WEB_SHOT_SPEED, math.sin(angle) * WEB_SHOT_SPEED)
        self.max_travel = reach
        self.creatures = creatures
        self.flies = flies

    def update(self, dt):
        if self.done:
            return
        ox, oy = self.pos
        step = min(WEB_SHOT_SPEED * dt, self.max_travel - self.travelled)
        dx = self.vel[0] / WEB_SHOT_SPEED * step
        dy = self.vel[1] / WEB_SHOT_SPEED * step
        self.pos = (ox + dx, oy + dy)
        self.travelled += step
        self.trail = (self.trail + [self.pos])[-9:]
        hits = []
        for target in list(self.creatures()) + list(self.flies()):
            is_spider = hasattr(target, "web_pinned")
            if is_spider:
                if (target is self.shooter or target.dead or target.webbed
                        or self.shooter.relation_to(target) != "foe"
                        or getattr(target, "airborne", False)):
                    continue
            elif not target.alive or target.eaten or target.trapped:
                continue
            t = max(0.0, min(1.0, ((target.x - ox) * dx + (target.y - oy) * dy)
                             / max(0.001, dx * dx + dy * dy)))
            radius = max(13.0, float(getattr(target, "size", 0)) * 0.7)
            if math.hypot(target.x - ox - dx * t, target.y - oy - dy * t) <= radius:
                hits.append((t, target, is_spider))
        if hits:
            _, target, is_spider = min(hits, key=lambda item: item[0])
            if is_spider:
                target.web_pinned("trap", self.shooter)
            else:
                _pin_fly_with_shot(target, self.origin, "trap", self.shooter)
            self.done = self.hit = True
        elif self.travelled >= self.max_travel:
            self.done = True
