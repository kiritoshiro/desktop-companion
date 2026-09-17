"""Data-driven jobs for desktop spiders.

Jobs answer *what a spider is doing for the colony*.  They are deliberately
separate from personality (temperament) and abilities (things the spider can
do).  This module contains no Qt or Creature imports so the rules can be
tested headlessly and extended without growing the personality FSM.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass
import math
import random
from typing import Iterable


@dataclass(frozen=True)
class JobDefinition:
    id: str
    display_name: str
    description: str
    ability_ids: tuple[str, ...] = ()
    creates_base: bool = False
    protects_base: bool = False


JOB_DEFINITIONS = (
    JobDefinition("none", "No job", "Acts only from temperament and selected abilities."),
    JobDefinition("hunter", "Hunter", "Tracks nearby prey and threats with focused pursuit."),
    JobDefinition("builder", "Builder", "Establishes and upgrades a shared colony base.", creates_base=True),
    JobDefinition("guard", "Guard", "Patrols a friendly base and responds to declared foes.", protects_base=True),
    JobDefinition("scout", "Scout", "Ranges beyond the base and reports points of interest."),
    JobDefinition("webber", "Web tender", "Maintains the colony's silk structures.", ability_ids=("weave_web",)),
)
JOB_BY_ID = {job.id: job for job in JOB_DEFINITIONS}
JOB_IDS = tuple(job.id for job in JOB_DEFINITIONS)
JOB_OPTIONS = tuple((job.display_name, job.id) for job in JOB_DEFINITIONS)

# A base is finished at this much accumulated build progress: five structure
# levels of 100 each.  Named so the build loop, the completion readout, and the
# "is there work left" check cannot drift apart.
MAX_BUILD_PROGRESS = 500.0

# A job is a shift, not a personality transplant.  Work claims a spider for one
# stretch, then lets go for a shorter one so its temperament -- wandering,
# playing, reacting to the cursor -- still reads on screen.  Without this a
# Builder stood motionless for the roughly 80 seconds a base takes, and a Guard
# orbited its site for the entire session.
BUILD_DUTY_ON = (7.0, 12.0)
BUILD_DUTY_OFF = (4.0, 8.0)
PATROL_DUTY_ON = (9.0, 16.0)
PATROL_DUTY_OFF = (5.0, 9.0)

# Below this fraction of maximum integrity a base is an emergency: its builder
# goes back on duty immediately instead of waiting out an off-duty stretch.
REPAIR_URGENT_INTEGRITY = 0.5


@dataclass
class DutyCycle:
    """Whether one worker is currently on shift, and for how much longer."""

    on_duty: bool = True
    remaining: float = 0.0


def normalize_job_id(value: str | None) -> str:
    job_id = str(value or "none").strip().lower()
    aliases = {"": "none", "default": "none", "weaver": "webber", "web_builder": "builder"}
    job_id = aliases.get(job_id, job_id)
    return job_id if job_id in JOB_BY_ID else "none"


def job_definition(value: str | None) -> JobDefinition:
    return JOB_BY_ID[normalize_job_id(value)]


def job_ability_ids(value: str | None) -> tuple[str, ...]:
    return job_definition(value).ability_ids


@dataclass
class BaseSite:
    """Persistent colony base state; geometry is derived from its level."""

    id: str
    owner_id: str
    team_id: str
    x: float
    y: float
    build_progress: float = 0.0
    level: int = 0
    integrity: float = 100.0
    max_integrity: float = 100.0
    alert: float = 0.0
    last_alert: float = 0.0
    patrol_angle: float = 0.0
    resources: float = 0.0

    @property
    def radius(self) -> float:
        return 30.0 + self.level * 10.0

    @property
    def completion(self) -> float:
        """Fraction of the *whole* base that is built, not of its first level."""
        return max(0.0, min(1.0, self.build_progress / MAX_BUILD_PROGRESS))

    def to_dict(self) -> dict:
        data = asdict(self)
        data["build_progress"] = round(self.build_progress, 3)
        data["integrity"] = round(self.integrity, 3)
        data["alert"] = round(self.alert, 3)
        data["last_alert"] = round(self.last_alert, 3)
        data["patrol_angle"] = round(self.patrol_angle, 5)
        data["resources"] = round(self.resources, 3)
        return data

    @classmethod
    def from_dict(cls, value: dict) -> "BaseSite | None":
        if not isinstance(value, dict):
            return None
        try:
            site = cls(
                id=str(value.get("id", "")),
                owner_id=str(value.get("owner_id", "")),
                team_id=str(value.get("team_id", "neutral"))[:32] or "neutral",
                x=float(value.get("x", 0.0)),
                y=float(value.get("y", 0.0)),
                build_progress=max(0.0, min(MAX_BUILD_PROGRESS, float(value.get("build_progress", 0.0)))),
                level=max(0, min(5, int(value.get("level", 0)))),
                integrity=max(0.0, float(value.get("integrity", 100.0))),
                max_integrity=max(1.0, float(value.get("max_integrity", 100.0))),
                alert=max(0.0, min(1.0, float(value.get("alert", 0.0)))),
                last_alert=max(0.0, float(value.get("last_alert", 0.0))),
                patrol_angle=float(value.get("patrol_angle", 0.0)),
                resources=max(0.0, min(1000.0, float(value.get("resources", 0.0)))),
            )
            site.x = float(site.x)
            site.y = float(site.y)
            return site if site.id and site.owner_id else None
        except (TypeError, ValueError):
            return None


class BaseWorld:
    """Small colony layer used by Builder and Guard jobs.

    A base is a shared team site. Builders add resources and structure levels;
    guards patrol its perimeter and raise an alert when an explicitly hostile
    spider enters the protection radius. It is intentionally not a combat
    system yet: the alert/integrity fields are stable hooks for later damage,
    repairs, doors, and siege behavior.
    """

    def __init__(self, screen_w: int, screen_h: int, saved: Iterable[dict] | None = None,
                 rng: random.Random | None = None):
        self.screen_w = max(200, int(screen_w))
        self.screen_h = max(200, int(screen_h))
        self.bases: dict[str, BaseSite] = {}
        self._clock = 0.0
        # Shift lengths are randomized per worker so a colony does not clock in
        # and out in unison. The module RNG is the default, matching the rest of
        # the behaviour code; tests that need their own stream pass a Random.
        self._rng = rng or random
        self._duty: dict[str, DutyCycle] = {}
        # Filled in by the manager: a base ring is drawn in its team's colour, so
        # two teams on one desktop can be told apart without opening anything.
        self.team_profiles: dict = {}
        for raw in saved or ():
            site = BaseSite.from_dict(raw)
            if site is not None:
                self.bases[site.id] = site

    def set_screen(self, screen_w: int, screen_h: int) -> None:
        self.screen_w = max(200, int(screen_w))
        self.screen_h = max(200, int(screen_h))
        for site in self.bases.values():
            site.x = max(32.0, min(self.screen_w - 32.0, site.x))
            site.y = max(32.0, min(self.screen_h - 32.0, site.y))

    def clear(self) -> None:
        self.bases.clear()
        self._duty.clear()
        self._clock = 0.0

    def _site_key(self, creature) -> str:
        team = str(getattr(creature.progression, "team_id", "neutral") or "neutral")
        if team != "neutral":
            return f"team:{team}"
        return f"owner:{getattr(creature, 'progression_id', creature.index)}"

    def ensure_site(self, creature) -> BaseSite:
        key = self._site_key(creature)
        site = self.bases.get(key)
        if site is None:
            site = BaseSite(
                id=key,
                owner_id=str(getattr(creature, "progression_id", creature.index)),
                team_id=str(getattr(creature.progression, "team_id", "neutral") or "neutral"),
                x=max(42.0, min(self.screen_w - 42.0, float(creature.x))),
                y=max(42.0, min(self.screen_h - 42.0, float(creature.y))),
                patrol_angle=(getattr(creature, "index", 0) * 0.83) % math.tau,
            )
            self.bases[key] = site
        return site

    @staticmethod
    def _needs_work(site: BaseSite) -> bool:
        """Return whether a base still has building or repair work outstanding."""
        return site.build_progress < MAX_BUILD_PROGRESS or site.integrity < site.max_integrity

    def _site_for_guard(self, creature) -> BaseSite | None:
        team = str(getattr(creature.progression, "team_id", "neutral") or "neutral")
        candidates = [site for site in self.bases.values() if site.team_id == team]
        if not candidates:
            return None
        return min(candidates, key=lambda site: math.hypot(site.x - creature.x, site.y - creature.y))

    @staticmethod
    def _creature_key(creature) -> str:
        return str(getattr(creature, "progression_id", getattr(creature, "index", 0)))

    def duty_state(self, creature) -> DutyCycle | None:
        """Return the current shift for one worker, if it has started one."""
        return self._duty.get(self._creature_key(creature))

    def _on_duty(self, creature, dt: float, on_range, off_range,
                 urgent: bool = False, productive: bool = True) -> bool:
        """Advance one worker's shift clock and report whether it is working.

        ``productive`` is False while a worker is on shift but not actually
        getting anything done -- walking back to the site, or busy with
        something its temperament outranked the job with. That time does not
        spend the shift, otherwise a builder burns a whole stretch on the
        commute and arrives just as its break starts.
        """
        key = self._creature_key(creature)
        duty = self._duty.get(key)
        if duty is None:
            # A new worker starts mid-shift rather than at a boundary, so a
            # colony spawned at once does not switch over all at the same frame.
            duty = DutyCycle(True, self._rng.uniform(*on_range))
            self._duty[key] = duty
        if urgent:
            duty.on_duty = True
            duty.remaining = max(duty.remaining, self._rng.uniform(*on_range))
            return True
        if duty.on_duty and not productive:
            return True
        duty.remaining -= max(0.0, float(dt))
        if duty.remaining <= 0.0:
            duty.on_duty = not duty.on_duty
            duty.remaining = self._rng.uniform(*(on_range if duty.on_duty else off_range))
        return duty.on_duty

    def _prune_duty(self, creatures) -> None:
        if len(self._duty) <= len(creatures):
            return
        live = {self._creature_key(creature) for creature in creatures}
        self._duty = {key: duty for key, duty in self._duty.items() if key in live}

    @staticmethod
    def _can_work(creature) -> bool:
        """Return whether a spider is physically and behaviourally free to work.

        ``job_busy`` is set by the creature when a personality state outranks
        its job -- fleeing, feeding, a jump already in the air, silk in
        progress. Reading it here stops a base from gaining progress from a
        worker that is not actually working.
        """
        return not (
            getattr(creature, "dragging", False)
            or getattr(creature, "airborne", False)
            or getattr(creature, "job_busy", False)
        )

    @staticmethod
    def _set_intent(creature, mode: str, target: tuple[float, float] | None = None, alert_target=None, base_id: str | None = None) -> None:
        creature.job_mode = mode
        creature.job_target = target
        creature.job_alert_target = alert_target
        creature.job_base_id = base_id

    def update(self, dt: float, creatures: Iterable) -> None:
        creatures = list(creatures or ())
        self._clock += max(0.0, float(dt))
        self._prune_duty(creatures)
        builders = [c for c in creatures if getattr(c, "job_id", "none") == "builder"]
        for creature in creatures:
            self._set_intent(creature, "idle")

        for builder in builders:
            site = self.ensure_site(builder)
            dist = math.hypot(site.x - builder.x, site.y - builder.y)
            builder.job_base_id = site.id
            # A finished, undamaged base has no work left. Claiming the builder
            # anyway pinned it motionless on top of the site for the rest of the
            # session, because the build intent also pauses its motion.
            if not self._needs_work(site):
                continue
            urgent = site.integrity < site.max_integrity * REPAIR_URGENT_INTEGRITY
            at_site = dist <= max(22.0, site.radius * 0.52)
            can_work = self._can_work(builder)
            if not self._on_duty(
                builder, dt, BUILD_DUTY_ON, BUILD_DUTY_OFF,
                urgent=urgent, productive=at_site and can_work,
            ):
                continue
            if not can_work:
                continue
            if not at_site:
                self._set_intent(builder, "build_travel", (site.x, site.y), base_id=site.id)
            else:
                # Building is intentionally slow and resource-shaped: a base
                # must be revisited over time rather than appearing instantly.
                rate = 6.0 + min(8.0, float(getattr(builder, "level", 1)) * 0.25)
                # Resources are a soft maintenance budget for future crafting;
                # the first version replenishes from the builder's work rather
                # than making a new economy prerequisite block base creation.
                site.resources = min(1000.0, site.resources + dt * 1.8)
                if site.build_progress < MAX_BUILD_PROGRESS:
                    site.build_progress = min(MAX_BUILD_PROGRESS, site.build_progress + dt * rate)
                site.level = min(5, int(site.build_progress // 100.0))
                site.max_integrity = 100.0 + site.level * 35.0
                # Work repairs as well as builds. The floor keeps a newly
                # upgraded base from looking derelict, and the per-second term
                # lets integrity actually reach the maximum -- without it the
                # base stayed permanently just-damaged and never released its
                # builder.
                site.integrity = min(
                    site.max_integrity,
                    max(site.integrity, site.max_integrity * 0.72) + dt * rate,
                )
                self._set_intent(builder, "build", (site.x, site.y), base_id=site.id)

        for guard in (c for c in creatures if getattr(c, "job_id", "none") == "guard"):
            site = self._site_for_guard(guard)
            if site is None:
                continue
            guard.job_base_id = site.id
            hostile = None
            hostile_dist = float("inf")
            for other in creatures:
                if other is guard or getattr(other, "dragging", False):
                    continue
                try:
                    hostile_relation = guard.relation_to(other) == "foe"
                except Exception:
                    hostile_relation = False
                if not hostile_relation:
                    continue
                d = math.hypot(other.x - site.x, other.y - site.y)
                if d <= site.radius + 70.0 and d < hostile_dist:
                    hostile, hostile_dist = other, d
            # An intruder is the one thing that cancels a guard's break.
            on_duty = self._on_duty(
                guard, dt, PATROL_DUTY_ON, PATROL_DUTY_OFF,
                urgent=hostile is not None, productive=self._can_work(guard),
            )
            if hostile is not None:
                site.alert = min(1.0, site.alert + dt * 1.8)
                site.last_alert = self._clock
                if self._can_work(guard):
                    self._set_intent(guard, "guard_alert", (hostile.x, hostile.y), hostile, site.id)
                continue
            # The site's patrol ring keeps turning whether or not this guard is
            # on shift, so a returning guard picks the route up where it is now
            # instead of snapping back to where it left off.
            site.alert = max(0.0, site.alert - dt * 0.22)
            site.patrol_angle = (site.patrol_angle + dt * (0.42 + site.level * 0.03)) % math.tau
            if not on_duty or not self._can_work(guard):
                continue
            patrol_radius = max(18.0, site.radius * 0.78)
            target = (
                site.x + math.cos(site.patrol_angle) * patrol_radius,
                site.y + math.sin(site.patrol_angle) * patrol_radius,
            )
            self._set_intent(guard, "patrol", target, base_id=site.id)

    def to_dict(self) -> list[dict]:
        return [site.to_dict() for site in sorted(self.bases.values(), key=lambda item: item.id)]

    def render(self, painter, clip=None) -> None:
        if not self.bases:
            return
        from PyQt5.QtCore import QLineF, QRectF, Qt
        from PyQt5.QtGui import QColor, QPainter, QPen

        from .teams import team_color

        for site in self.bases.values():
            if clip is not None and (site.x + site.radius < clip[0] or site.x - site.radius > clip[2] or site.y + site.radius < clip[1] or site.y - site.radius > clip[3]):
                continue
            completion = site.completion
            red, green, blue = team_color(site.team_id, self.team_profiles)
            accent = QColor(red, green, blue, 190)
            faint = QColor(accent.red(), accent.green(), accent.blue(), 45)
            painter.save()
            painter.setRenderHint(QPainter.Antialiasing, True)
            painter.setPen(QPen(faint, 1.2, Qt.DashLine))
            painter.setBrush(QColor(25, 30, 38, 30))
            painter.drawEllipse(QRectF(site.x - site.radius, site.y - site.radius, site.radius * 2.0, site.radius * 2.0))
            if completion > 0.01:
                painter.setPen(QPen(accent, 1.6, Qt.SolidLine))
                nodes = max(4, min(10, 4 + site.level))
                points = []
                for idx in range(nodes):
                    angle = site.patrol_angle + idx * math.tau / nodes
                    radius = site.radius * (0.82 + 0.05 * (idx % 2))
                    points.append((site.x + math.cos(angle) * radius, site.y + math.sin(angle) * radius))
                for idx, (px, py) in enumerate(points):
                    qx, qy = points[(idx + 1) % len(points)]
                    if completion >= (idx + 1) / len(points) * 0.92:
                        painter.drawLine(QLineF(px, py, qx, qy))
                    painter.setBrush(accent)
                    painter.drawEllipse(QRectF(px - 2.5, py - 2.5, 5.0, 5.0))
                painter.setBrush(QColor(accent.red(), accent.green(), accent.blue(), 55))
                painter.drawEllipse(QRectF(site.x - 7.0, site.y - 7.0, 14.0, 14.0))
            if site.alert > 0.01:
                painter.setPen(QPen(QColor(245, 92, 72, int(90 + site.alert * 130)), 2.0, Qt.SolidLine))
                painter.setBrush(Qt.NoBrush)
                painter.drawEllipse(QRectF(site.x - site.radius - 5.0, site.y - site.radius - 5.0, (site.radius + 5.0) * 2.0, (site.radius + 5.0) * 2.0))
            painter.restore()
