"""Data-driven jobs for desktop spiders.

Jobs answer *what a spider is doing for the colony*.  They are deliberately
separate from personality (temperament) and abilities (things the spider can
do).  This module contains no Qt or Creature imports so the rules can be
tested headlessly and extended without growing the personality FSM.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass, field
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

# DC-20: Scout, Webber and Hunter get their own duty-cycle constants rather
# than reusing Builder's/Guard's -- each shift feels distinct and none of the
# three should silently drift if a future tuning pass changes BUILD_*/PATROL_*.
# The on:off ratio for Scout/Webber runs noticeably longer-on than Builder's
# or Guard's: `presets/colony.json` also runs a fly spawner, and any spider
# with `chase`/`approach` skills gets pulled out of its job by the
# pre-existing, disclosed hunt-vs-job priority (`_job_outranked_by_personality`
# outranks every job mode but `guard_alert` whenever `_hunting_prey` is set --
# see this package's PR notes and DC-18's work-log entry). A nominal on:off
# ratio around 60/40, as Builder/Guard use, is not enough headroom to clear
# DC-20's "no job idle more than 60% of frames" bar once that preemption
# takes its share; this package was told not to retune the preemption itself,
# so the duty cycle -- which the plan explicitly says is fair game to tune
# per job -- carries the compensation instead.
SCOUT_DUTY_ON = (9.0, 14.0)
SCOUT_DUTY_OFF = (3.0, 5.0)
WEBBER_DUTY_ON = (10.0, 15.0)
WEBBER_DUTY_OFF = (3.0, 5.0)
HUNTER_DUTY_ON = (8.0, 14.0)
HUNTER_DUTY_OFF = (4.0, 7.0)

# Scout: the screen is divided into a coarse grid of sectors it takes turns
# visiting, oldest-covered first, so it eventually covers the whole desktop
# rather than orbiting one corner.
SCOUT_SECTOR_COLS = 4
SCOUT_SECTOR_ROWS = 3
SCOUT_ARRIVE_DIST = 42.0
SCOUT_REPORT_DWELL = (1.0, 2.0)

# Webber: how close a web must land to a team's base to count as "near it"
# for repair/maintenance purposes, and how many intact webs within that ring
# is "enough" before a webber goes looking for fresh silk to lay instead.
WEBBER_ARRIVE_DIST = 26.0
WEBBER_NEAR_BASE_PAD = 260.0
WEBBER_TARGET_WEB_COUNT = 2
WEBBER_WEAVE_SPEED = 42.0

# Hunter: how far past the base radius its home patrol ring sits, and how
# much a base's soft resource budget grows per delivered catch.
HUNTER_PATROL_RADIUS_PAD = 70.0
HUNTER_CARRY_FOOD_AMOUNT = 12.0

# DC-21: any team member eating a fly tops the team's food up a little too --
# not just a dedicated Hunter's deliberate carry-home trip, which stays the
# larger, more reliable source above. See ``BaseWorld.credit_team_food``,
# called from ``CreatureManager._resolve_fly_catches`` for *every* catch
# regardless of the eater's job.
FLY_CATCH_RESOURCE_AMOUNT = 4.0

# Below this fraction of maximum integrity a base is an emergency: its builder
# goes back on duty immediately instead of waiting out an off-duty stretch.
REPAIR_URGENT_INTEGRITY = 0.5

# DC-21: closing the resource loop. Advancing ``build_progress`` now spends
# ``site.resources`` (the team's food, banked by Hunters and incidental
# catches) rather than the base being self-sufficient purely from builder
# time. One point of progress costs this many banked resources; a base with
# an empty larder still gets visited and held at its current progress, but
# does not advance until food arrives.
BUILD_RESOURCE_COST_PER_PROGRESS = 1.0

# DC-21: a visible, on-screen benefit for a leveled-up base -- friendly
# creatures within this much past the base's ring slowly regenerate hp, and
# (below) a guard's threat-response ring grows with the base's level.
BASE_REGEN_RADIUS_PAD = 90.0
BASE_REGEN_HP_PER_LEVEL = 1.4
GUARD_ALERT_RADIUS_PAD = 70.0
GUARD_ALERT_RADIUS_PER_LEVEL = 15.0


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
    # DC-20: a small rolling log of what a Scout has reported nearby -- the
    # team's shared "blackboard". Each entry is a plain dict so it round-trips
    # through JSON without a schema bump; oldest entries drop once it grows
    # past a handful.
    points_of_interest: list = field(default_factory=list)

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
        data["points_of_interest"] = list(self.points_of_interest)
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
                points_of_interest=[
                    dict(poi) for poi in (value.get("points_of_interest") or ())
                    if isinstance(poi, dict)
                ][-8:],
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
        # DC-20 per-worker job state, keyed the same way as ``_duty``.
        self._scout_targets: dict[str, int] = {}
        self._scout_report_timers: dict[str, float] = {}
        self._scout_coverage: dict[str, dict[int, float]] = {}
        self._webber_claims: dict[str, dict] = {}
        self._hunt_carry: dict[str, bool] = {}
        self._hunt_fed_seen: dict[str, bool] = {}
        self._hunt_patrol_angle: dict[str, float] = {}
        self._hunt_home: dict[str, tuple[float, float]] = {}
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
        self._scout_targets.clear()
        self._scout_report_timers.clear()
        self._scout_coverage.clear()
        self._webber_claims.clear()
        self._hunt_carry.clear()
        self._hunt_fed_seen.clear()
        self._hunt_patrol_angle.clear()
        self._hunt_home.clear()
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

    def _site_for_team(self, creature) -> BaseSite | None:
        """Nearest base belonging to ``creature``'s own team, if any exists yet.

        Used by every job that works *around* a base without founding one
        itself (Guard, and DC-20's Scout/Webber/Hunter): a team with no
        builder, or whose builder has not founded a site yet, simply has
        nothing for these jobs to do until one exists.
        """
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
        self._scout_targets = {k: v for k, v in self._scout_targets.items() if k in live}
        self._scout_report_timers = {k: v for k, v in self._scout_report_timers.items() if k in live}
        self._webber_claims = {k: v for k, v in self._webber_claims.items() if k in live}
        self._hunt_carry = {k: v for k, v in self._hunt_carry.items() if k in live}
        self._hunt_fed_seen = {k: v for k, v in self._hunt_fed_seen.items() if k in live}
        self._hunt_patrol_angle = {k: v for k, v in self._hunt_patrol_angle.items() if k in live}
        self._hunt_home = {k: v for k, v in self._hunt_home.items() if k in live}

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

    def update(self, dt: float, creatures: Iterable, web_world=None) -> None:
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
                # DC-21: building now spends the team's banked food instead of
                # replenishing itself from builder time -- a builder can be at
                # the site, on duty and able to work, and still make no
                # progress if ``site.resources`` is empty. The cap at
                # ``resources / cost`` is what actually enforces that; a
                # builder with unlimited time cannot outrun an empty larder.
                if site.build_progress < MAX_BUILD_PROGRESS and site.resources > 0.0:
                    progress_step = min(
                        dt * rate, site.resources / BUILD_RESOURCE_COST_PER_PROGRESS,
                    )
                    if progress_step > 0.0:
                        site.build_progress = min(MAX_BUILD_PROGRESS, site.build_progress + progress_step)
                        site.resources = max(0.0, site.resources - progress_step * BUILD_RESOURCE_COST_PER_PROGRESS)
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
            site = self._site_for_team(guard)
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
                # DC-21: a higher-level base gives its guard a wider threat
                # response ring -- the visible payoff for the base economy
                # actually advancing, on top of the passive alert-ring redraw
                # that already scales with level in ``render``.
                alert_pad = GUARD_ALERT_RADIUS_PAD + site.level * GUARD_ALERT_RADIUS_PER_LEVEL
                if d <= site.radius + alert_pad and d < hostile_dist:
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

        for scout in (c for c in creatures if getattr(c, "job_id", "none") == "scout"):
            self._update_scout(dt, scout, creatures)

        for webber in (c for c in creatures if getattr(c, "job_id", "none") == "webber"):
            self._update_webber(dt, webber, web_world)

        for hunter in (c for c in creatures if getattr(c, "job_id", "none") == "hunter"):
            self._update_hunter(dt, hunter, creatures)

        self._apply_base_regen(dt, creatures)

    def _apply_base_regen(self, dt: float, creatures) -> None:
        """DC-21: heal creatures resting near their own team's leveled base.

        Only a base at level 1+ (i.e. one the economy has actually funded
        past its first structure tier) grants this -- a freshly founded,
        unbuilt site gives no benefit yet, so the payoff reads as earned.
        Every job (not just Guard/Builder) benefits, since this represents
        the base itself, not any one worker's task.
        """
        for creature in creatures:
            if getattr(creature, "dragging", False):
                continue
            heal = getattr(creature, "heal", None)
            if not callable(heal):
                continue
            team = str(getattr(getattr(creature, "progression", None), "team_id", "neutral") or "neutral")
            if team == "neutral":
                continue
            site = self.bases.get(f"team:{team}")
            if site is None or site.level <= 0:
                continue
            if math.hypot(creature.x - site.x, creature.y - site.y) <= site.radius + BASE_REGEN_RADIUS_PAD:
                heal(dt * BASE_REGEN_HP_PER_LEVEL * site.level)

    def credit_team_food(self, team_id: str, amount: float) -> None:
        """Credit a team's base with incidentally-gathered food.

        Called for *every* fly caught by a team member, regardless of job --
        see ``FLY_CATCH_RESOURCE_AMOUNT``'s comment for why this is a smaller
        top-up alongside the Hunter's larger, deliberate carry-home amount.
        A team with no base yet (or a solitary, base-less creature) simply
        has nowhere to bank it.
        """
        team = str(team_id or "neutral")
        if team == "neutral":
            return
        site = self.bases.get(f"team:{team}")
        if site is not None:
            site.resources = min(1000.0, site.resources + max(0.0, float(amount)))

    # -- Scout: sector coverage + a team blackboard ---------------------

    def _sector_center(self, sector: int) -> tuple[float, float]:
        col = sector % SCOUT_SECTOR_COLS
        row = (sector // SCOUT_SECTOR_COLS) % SCOUT_SECTOR_ROWS
        cell_w = self.screen_w / SCOUT_SECTOR_COLS
        cell_h = self.screen_h / SCOUT_SECTOR_ROWS
        return (cell_w * (col + 0.5), cell_h * (row + 0.5))

    def _pick_scout_sector(self, team_id: str, exclude_key: str) -> int:
        """Least-recently-covered sector for this team, with a little spread.

        Ties among the stalest few sectors are broken randomly so two scouts
        on the same team do not both beeline for the identical sector the
        instant it goes stale.
        """
        total = SCOUT_SECTOR_COLS * SCOUT_SECTOR_ROWS
        coverage = self._scout_coverage.setdefault(team_id, {})
        taken = {v for k, v in self._scout_targets.items() if k != exclude_key}
        candidates = [s for s in range(total) if s not in taken] or list(range(total))
        candidates.sort(key=lambda s: coverage.get(s, -1.0))
        pool = candidates[: max(1, len(candidates) // 3)]
        return self._rng.choice(pool)

    def _report_sector(self, site: BaseSite, scout, sector: int, creatures) -> None:
        """Publish what a scout finds at a sector to its team's blackboard."""
        cx, cy = self._sector_center(sector)
        kind = "clear"
        poi_x, poi_y = cx, cy
        best_d = 150.0
        for other in creatures:
            if other is scout or getattr(other, "dragging", False):
                continue
            try:
                hostile = scout.relation_to(other) == "foe"
            except Exception:
                hostile = False
            if not hostile:
                continue
            d = math.hypot(other.x - cx, other.y - cy)
            if d < best_d:
                best_d = d
                kind = "foe"
                poi_x, poi_y = other.x, other.y
        site.points_of_interest.append({
            "sector": sector,
            "x": round(poi_x, 1),
            "y": round(poi_y, 1),
            "kind": kind,
            "reported_at": round(self._clock, 2),
        })
        # A rolling window: the blackboard is "what's out there lately", not
        # an ever-growing log.
        if len(site.points_of_interest) > 8:
            del site.points_of_interest[: len(site.points_of_interest) - 8]

    def _update_scout(self, dt: float, scout, creatures) -> None:
        site = self._site_for_team(scout)
        if site is None:
            # Nothing to report to yet -- a scout with no team base just
            # keeps wandering on temperament until one exists.
            return
        scout.job_base_id = site.id
        can_work = self._can_work(scout)
        key = self._creature_key(scout)
        report_timer = self._scout_report_timers.get(key, 0.0)
        reporting = report_timer > 0.0
        on_duty = self._on_duty(
            scout, dt, SCOUT_DUTY_ON, SCOUT_DUTY_OFF,
            urgent=False, productive=can_work,
        )
        if not on_duty or not can_work:
            return
        if reporting:
            report_timer = max(0.0, report_timer - dt)
            self._scout_report_timers[key] = report_timer
            target = self._sector_center(self._scout_targets.get(key, 0))
            self._set_intent(scout, "scout_report", target, base_id=site.id)
            if report_timer <= 0.0:
                self._scout_targets.pop(key, None)
            return
        sector = self._scout_targets.get(key)
        if sector is None:
            sector = self._pick_scout_sector(site.team_id, key)
            self._scout_targets[key] = sector
        target = self._sector_center(sector)
        if math.hypot(target[0] - scout.x, target[1] - scout.y) > SCOUT_ARRIVE_DIST:
            self._set_intent(scout, "scout_travel", target, base_id=site.id)
        else:
            # Arrived: log coverage and whatever this sector currently holds
            # to the team's shared blackboard, then hold for a beat so the
            # report reads as a pause rather than a flicker between legs.
            self._scout_coverage.setdefault(site.team_id, {})[sector] = self._clock
            self._report_sector(site, scout, sector, creatures)
            self._scout_report_timers[key] = self._rng.uniform(*SCOUT_REPORT_DWELL)
            self._set_intent(scout, "scout_report", target, base_id=site.id)

    # -- Webber: repair-and-maintain the team's silk near its base ------

    def _find_webber_work(self, webber, site: BaseSite, web_world) -> dict | None:
        """Find this webber a duty-cycled maintenance task, nearest first.

        Mending a torn web always wins. Failing that, keep the team's web
        count topped up so a colony whose silk never gets cut still has a
        webber doing something -- this is the "maintain" half of the goal,
        distinct from a personality-driven weaver's own dice roll.
        ``WebWorld.claim_site`` has no notion of "near a point", only fixed
        corner/edge/open-space specs, so "near the base" is honoured for
        repair and adoption (the nearest torn/unfinished web to the site) but
        not for brand-new silk -- a real placement bias would need a change
        to ``webs.py`` this package does not make.
        """
        near = WEBBER_NEAR_BASE_PAD + site.radius
        repairable = web_world.find_repairable_web(webber, max_dist=1e9)
        if repairable is not None:
            dist = math.hypot(repairable.hub[0] - site.x, repairable.hub[1] - site.y)
            if dist <= near and web_world.claim_repair(repairable, webber):
                return {"web": repairable, "mode": "repair"}
        adoptable = web_world.find_adoptable_web(webber, max_dist=1e9)
        if adoptable is not None:
            dist = math.hypot(adoptable.hub[0] - site.x, adoptable.hub[1] - site.y)
            if dist <= near and web_world.adopt(adoptable, webber):
                return {"web": adoptable, "mode": "weave"}
        team_webs = sum(
            1 for w in web_world.webs
            if w.is_complete() and not w.is_damaged()
            and math.hypot(w.hub[0] - site.x, w.hub[1] - site.y) <= near
        )
        if team_webs < WEBBER_TARGET_WEB_COUNT:
            # Pass the creature's own seeded stream, not this module's ``self._rng``:
            # ``claim_site`` defaults to the unseeded module-level ``random`` when
            # given none, which would make a seeded colony run pick a different new
            # web site every replay -- exactly the DC-09/DC-40 seeding contract this
            # package must not quietly break.
            new_web = web_world.claim_site(
                webber, prefer_corner=False, rng=getattr(webber, "rng", None),
            )
            if new_web is not None:
                return {"web": new_web, "mode": "weave"}
        return None

    def _update_webber(self, dt: float, webber, web_world) -> None:
        site = self._site_for_team(webber)
        if site is None:
            return
        webber.job_base_id = site.id
        can_work = self._can_work(webber)
        key = self._creature_key(webber)
        claim = self._webber_claims.get(key)
        if claim is not None and (web_world is None or claim["web"] not in web_world.webs):
            claim = None
            self._webber_claims.pop(key, None)
        if claim is None and web_world is not None:
            claim = self._find_webber_work(webber, site, web_world)
            if claim is not None:
                self._webber_claims[key] = claim
        productive = can_work and claim is not None
        urgent = claim is not None and claim["mode"] == "repair"
        on_duty = self._on_duty(
            webber, dt, WEBBER_DUTY_ON, WEBBER_DUTY_OFF,
            urgent=urgent, productive=productive,
        )
        if not on_duty or not can_work or claim is None:
            return
        web = claim["web"]
        at_web = math.hypot(web.hub[0] - webber.x, web.hub[1] - webber.y) <= WEBBER_ARRIVE_DIST
        if not at_web:
            self._set_intent(webber, "web_travel", web.hub, base_id=site.id)
            return
        if claim["mode"] == "repair":
            web.repair_near(webber.x, webber.y, dt)
            self._set_intent(webber, "web_repair", web.hub, base_id=site.id)
            if not web.is_damaged():
                web_world.release_repair(web)
                self._webber_claims.pop(key, None)
        else:
            tip, _drawing = web.working_point()
            web.advance(dt, WEBBER_WEAVE_SPEED, tip)
            self._set_intent(webber, "web_weave", web.hub, base_id=site.id)
            if web.is_complete():
                self._webber_claims.pop(key, None)

    # -- Hunter: prey and intruders close to home, food carried back ----

    def _update_hunter(self, dt: float, hunter, creatures) -> None:
        key = self._creature_key(hunter)
        site = self._site_for_team(hunter)
        if site is not None:
            home_x, home_y, home_radius, base_id = site.x, site.y, site.radius, site.id
        else:
            # This hunter's own team has founded no base -- either it has no
            # builder yet, or (colony.json's actual arrangement) the hunter is
            # deliberately on a base-less rival team, an intruder rather than
            # a colonist. Either way "close to home" still needs a home: the
            # spot this hunter was first seen at stands in for one, so the
            # job produces its own states without a real ``BaseSite`` to
            # patrol around or bank food in.
            home_x, home_y = self._hunt_home.setdefault(key, (hunter.x, hunter.y))
            home_radius, base_id = 60.0, None
        hunter.job_base_id = base_id
        can_work = self._can_work(hunter)
        hunting_now = bool(getattr(hunter, "_hunting_prey", False))
        fed_now = str(getattr(hunter, "state", "")) == "Feed"
        if fed_now and not self._hunt_fed_seen.get(key, False):
            self._hunt_carry[key] = True
        self._hunt_fed_seen[key] = fed_now
        carrying = self._hunt_carry.get(key, False)

        if hunting_now and not carrying:
            # A live hunt (personality-driven ``_pursue_prey``) already
            # outranks job duty by design -- see
            # ``BehaviourMixin._job_outranked_by_personality`` and this
            # package's disclosed note on the hunt-vs-job priority dynamic,
            # which is intentionally left untouched here. The job still
            # marks itself on duty for the frame; it is just not the one
            # steering.
            self._set_intent(hunter, "hunting", base_id=base_id)
            return

        on_duty = self._on_duty(
            hunter, dt, HUNTER_DUTY_ON, HUNTER_DUTY_OFF,
            urgent=carrying, productive=can_work,
        )
        if not on_duty or not can_work:
            return

        if carrying:
            dist = math.hypot(home_x - hunter.x, home_y - hunter.y)
            if dist > 30.0:
                self._set_intent(hunter, "hunt_return", (home_x, home_y), base_id=base_id)
            else:
                if site is not None:
                    site.resources = min(1000.0, site.resources + HUNTER_CARRY_FOOD_AMOUNT)
                self._hunt_carry[key] = False
            return

        # Nothing to deliver and nothing to chase: patrol a ring close to
        # home so the next fly or foe this hunter meets is one near its base,
        # per the plan's "close to home" preference. Target *selection*
        # among flies stays entirely in ``CreatureManager._update_prey_targets``
        # -- the same nearest-with-hysteresis scoring every hunter uses --
        # since biasing it would touch the disclosed hunt-vs-job priority
        # dynamic this package was told not to retune.
        angle = self._hunt_patrol_angle.get(key)
        if angle is None:
            angle = self._rng.uniform(0.0, math.tau)
        angle = (angle + dt * 0.6) % math.tau
        self._hunt_patrol_angle[key] = angle
        radius = max(50.0, home_radius + HUNTER_PATROL_RADIUS_PAD)
        target = (home_x + math.cos(angle) * radius, home_y + math.sin(angle) * radius)
        alert_target = None
        best_d = radius + 40.0
        for other in creatures:
            if other is hunter or getattr(other, "dragging", False):
                continue
            try:
                hostile = hunter.relation_to(other) == "foe"
            except Exception:
                hostile = False
            if not hostile:
                continue
            d = math.hypot(other.x - home_x, other.y - home_y)
            if d <= best_d:
                best_d = d
                alert_target = other
                target = (other.x, other.y)
        self._set_intent(hunter, "hunt_patrol", target, alert_target=alert_target, base_id=base_id)

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
