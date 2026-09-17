import math
import random
from typing import Tuple


def clamp(value: float, low: float, high: float) -> float:
    return max(low, min(high, value))


def lerp(a: float, b: float, t: float) -> float:
    return a + (b - a) * clamp(t, 0.0, 1.0)


def smoothstep(t: float) -> float:
    t = clamp(t, 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def distance(ax: float, ay: float, bx: float, by: float) -> float:
    return math.hypot(bx - ax, by - ay)


def normalize_angle(angle: float) -> float:
    while angle <= -math.pi:
        angle += math.tau
    while angle > math.pi:
        angle -= math.tau
    return angle


def angle_to(ax: float, ay: float, bx: float, by: float) -> float:
    return math.atan2(by - ay, bx - ax)


def angle_lerp(current: float, target: float, max_step: float) -> float:
    delta = normalize_angle(target - current)
    delta = clamp(delta, -abs(max_step), abs(max_step))
    return normalize_angle(current + delta)


def vec_from_angle(angle: float) -> Tuple[float, float]:
    return math.cos(angle), math.sin(angle)


def rand_range(pair, default_low: float, default_high: float, rng=None) -> float:
    source = rng if rng is not None else random
    if isinstance(pair, (list, tuple)) and len(pair) >= 2:
        return source.uniform(float(pair[0]), float(pair[1]))
    return source.uniform(default_low, default_high)


def clamp_point(x: float, y: float, margin: float, width: float, height: float) -> Tuple[float, float]:
    return clamp(x, margin, width - margin), clamp(y, margin, height - margin)


def cursor_is_threatening(
    spider_x: float,
    spider_y: float,
    cursor_x: float,
    cursor_y: float,
    cursor_vx: float,
    cursor_vy: float,
    threat_radius: float,
    speed_threshold: float,
    toward_score_threshold: float = 0.6,
) -> bool:
    dx = spider_x - cursor_x
    dy = spider_y - cursor_y
    dist = max(1.0, math.hypot(dx, dy))
    if dist > threat_radius:
        return False
    cursor_speed = math.hypot(cursor_vx, cursor_vy)
    if cursor_speed < speed_threshold:
        return False
    score = (cursor_vx * dx + cursor_vy * dy) / (max(1.0, cursor_speed) * dist)
    return score > toward_score_threshold
