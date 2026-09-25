"""A bounded, isolated territory raid. No Companion state is written here."""
from __future__ import annotations

from dataclasses import dataclass
import copy
import math

from .adventure import PlayerController
from .adventure_profile import (hero_progression, load_profile, profile_path, record_result,
                                save_profile)
from ..content.palettes import enemy_palette
from ..world.playfield import Playfield, ScreenRect


@dataclass
class MissionSite:
    kind: str
    name: str
    x: float
    y: float
    owned: bool = False
    progress: float = 0.0
    contested: bool = False
    reserves: int = 0
    warning: float = 0.0
    supply: float = 180.0


class MissionActor(PlayerController):
    """A mission spider's brain, acting through the player's own rules.

    It decides what to do -- whom to chase, when to strike -- but every strike
    goes through PlayerController's ``shoot``, ``bite`` and ``jump``: the same
    view cone, stamina, silk, cooldowns and reach as the player's spider (the
    owner: "enemy can shoot webs from any angle he is facing not just from
    the front 90 degrees. make sure all spiders have the same rules").

    What it keeps that the player has not: a short wind-up before each strike,
    so the player can see it coming, and a pause after it. Both only slow it.
    """

    # Webbed, a mission spider always fights the silk, as a player holding a
    # movement key does; it used to sit out the whole net.
    struggling = True

    def __init__(self, creature, mission, role, home, raider=False):
        super().__init__(creature, mission.controls)
        self.mission = mission
        self.role = role
        self.home = home
        self.raider = raider
        self.windup = 0.0
        self.strike_target = None
        self.strike_point = home
        self.strike_kind = None
        self.target = None
        self.think = creature.index * 0.013
        self.silk_capacity = mission.silk_capacity_for(creature)
        self.silk = float(self.silk_capacity)

    def _choose_strike(self, target, gap):
        """The strike the rules allow right now, or None."""
        c = self.creature
        if not self.in_cone(target):
            return None
        if self.role == "weaver":
            if (gap < self.WEB_RANGE and self.web_cooldown <= 0 and self.silk >= 1
                    and c.energy >= self.WEB_ENERGY):
                return "shoot"
            return "bite" if gap < self.bite_reach() else None
        if gap < self.bite_reach():
            return "bite"
        if (self.role == "hunter" and gap < self.bite_reach() + self.POUNCE_DISTANCE
                and self.jump_cooldown <= 0 and c.energy >= self.JUMP_ENERGY
                and c.has_skill("jump")):
            return "pounce"
        return None

    def update(self, dt):
        c, m = self.creature, self.mission
        self.think -= dt
        self.web_cooldown = max(0.0, self.web_cooldown - dt)
        self.jump_cooldown = max(0.0, self.jump_cooldown - dt)
        if self.think <= 0:
            self.think = 0.15
            targets = [s for s in m.manager.creatures if not s.dead and c.relation_to(s) == "foe"]
            if self.role == "ally":
                anchor = m.defend_point if m.command == "defend" else (m.hero.x, m.hero.y)
                self.home = anchor if m.command == "defend" else m.area.clamp(
                    anchor[0] - math.cos(m.hero.heading)*85,
                    anchor[1] - math.sin(m.hero.heading)*85, c.size*2)
                targets = [s for s in targets if math.hypot(s.x-anchor[0], s.y-anchor[1]) < 220]
                if m.command == "attack" and m.attack_target is not None and not m.attack_target.dead:
                    targets = [m.attack_target]
            elif not self.raider:
                refuge = m.sites[0]
                targets = [s for s in targets if math.hypot(s.x-self.home[0], s.y-self.home[1]) < 260
                           and math.hypot(s.x-refuge.x, s.y-refuge.y) > 130]
            self.target = min(targets, key=lambda s: math.hypot(s.x-c.x, s.y-c.y), default=None)
        target = self.target
        if target is not None and target.dead:
            target = self.target = None
        tx, ty = (target.x, target.y) if target is not None else self.home
        gap = math.hypot(tx-c.x, ty-c.y)
        reach = self.bite_reach()
        c.motion_paused = gap < (reach * 0.72 if target else 22)
        # Its own pace, never faster than the player's walk, and scaled by
        # level and armour exactly as the player's is.
        c.speed = (95.0 if self.role == "hunter" else 68.0) * c._speed_mult()
        if target is not None and self.role == "weaver":
            c.motion_paused = 135 < gap < 230
            if gap < 135:
                tx, ty = c.x + (c.x-tx), c.y + (c.y-ty)
        if self.windup > 0:
            c.motion_paused = True
            self.windup -= dt
            if c.webbed:
                self.windup = 0
                c.attack_cooldown = 0.9
            elif self.windup <= 0:
                self._release_strike()
        elif (target is not None and not c.webbed and not c.airborne
              and c.attack_cooldown <= 0):
            kind = self._choose_strike(target, gap)
            # At most two foes commit to attacks at once; warnings remain readable.
            busy = sum(a.windup > 0 for a in m.actors if a.role != "ally")
            if kind is not None and (self.role == "ally" or busy < 2):
                self.windup = 0.7 if self.role == "guardian" else 0.55
                self.strike_kind = kind
                self.strike_target = target
                self.strike_point = (target.x, target.y)
                c.begin_strike(target.x, target.y)
        c.target_x, c.target_y = m.manager.playfield.clamp(tx, ty, c.size)
        if target is not None:
            c.target_heading = math.atan2(target.y-c.y, target.x-c.x)
        if c.webbed:
            c.speed *= 0.25
        if not c.airborne:
            c.state = "Player"
        self.advance_pose(dt)

    def _release_strike(self):
        """Strike where it wound up to: a target that moved away is missed."""
        c, m = self.creature, self.mission
        self.aim = self.strike_point
        if self.strike_kind == "shoot":
            self.shoot(m.manager)
        elif self.strike_kind == "pounce":
            c.target_x, c.target_y = m.area.clamp(*self.strike_point, c.size*2)
            self.jump()
        else:
            self.bite(m.manager)
        # The pause after a strike: this brain's pacing, not a rule.
        c.attack_cooldown = max(c.attack_cooldown, 1.4 if self.role != "guardian" else 1.8)


class TerritoryMission:
    MISSION_ID = "territory"
    CAP = 10
    CAPTURE_SECONDS = 4.0
    # How long the VICTORY / DEFEAT title stays up before the overlay closes
    # and the Adventure page comes back (the owner's request).
    END_SCREEN_SECONDS = 4.5

    def __init__(self, manager, controls, area=None):
        self.manager = manager
        self.controls = controls
        self.elapsed = 0.0
        self.state = "active"
        self.command = "follow"
        self.attack_target = None
        self.hover_target = None
        self.notice = "Capture Food or Silk, then seal the Hatchery. Hold clear sites for 4 seconds."
        self.notice_time = 9.0
        self.wave_clock = 28.0
        self.pending = []
        self.counter_started = False
        self.guardian = None
        self.guardian_warning = None
        self.actors = []
        self.saved = False
        self.save_error = ""
        self.end_clock = 0.0
        self._serial = 0
        # The hero is the player's own Adventure spider: its name and
        # progression come from adventure-hero.json, and a hero who has not
        # played starts at level 1 (the owner) -- not as a copy of whichever
        # Companion spider happens to be first in the preset.
        self.progress_path = profile_path()
        self.profile = load_profile(self.progress_path)
        self.hero_name = self.profile["name"]
        previous = getattr(manager, "mission", None)
        source = previous.hero if previous else next((c for c in manager.creatures if not c.dead), None)
        if source is None:
            raise ValueError("Adventure needs at least one spider in the preset")
        self.model = source.model
        self.personality = source.personality
        self.start_progress = hero_progression(self.profile).to_dict()
        # Mission factions are fixed, regardless of Companion diplomacy.
        self.start_progress["relation_overrides"] = {}
        manager.mission = self
        manager.conflict_enabled = True
        manager.interferable = manager.naming_enabled = manager.allow_mouse_capture = False
        manager.cages.clear()
        manager.web_world.clear()
        manager.fly_world.clear()
        manager.fly_world.clear_spawners()
        manager.fly_world.remains.clear()
        manager.base_world.clear()
        manager.carcasses.clear()
        manager.creatures = []
        manager._render_order = []
        manager.team_stances = {"adventurers": {"rivals": "foe"}}
        if area is None:
            area = max(manager.playfield.rects, key=lambda r: r.w*r.h,
                       default=ScreenRect(0, 0, manager.screen_w, manager.screen_h))
        # One real monitor is a complete arena, avoiding disconnected screen gaps.
        self.area = area
        self.playfield = Playfield(manager.screen_w, manager.screen_h, [area])
        def point(fx, fy):
            usable_height = max(160, area.h - 360)
            return area.clamp(area.x + area.w*fx, area.y + 115 + usable_height*fy, 85)
        self.sites = [
            MissionSite("home", "Home burrow", *point(.16, .57), owned=True),
            MissionSite("food", "Food cache", *point(.39, .29)),
            MissionSite("silk", "Silk loom", *point(.39, .70)),
            MissionSite("hatchery", "Hatchery", *point(.64, .47), reserves=6),
            MissionSite("nest", "Thorn nest", *point(.83, .30)),
        ]
        self.hero = self._spawn("hero", (self.sites[0].x, self.sites[0].y))
        self.player = PlayerController(self.hero, controls)
        self.ally = self._spawn("ally", (self.hero.x+45, self.hero.y+65))
        self.defend_point = (self.ally.x, self.ally.y)
        for site, role in zip(self.sites[1:4], ("guard", "weaver", "hunter")):
            self._spawn(role, (site.x+35, site.y+40))
        manager._refresh_neighbor_links()
        manager._refresh_render_order()

    def _spawn(self, role, pos, raider=False):
        if sum(not c.dead for c in self.manager.creatures) >= self.CAP:
            return None
        self._serial += 1
        progress = copy.deepcopy(self.start_progress) if role == "hero" else {"level": max(1, int(self.start_progress.get("level", 1)))}
        c = self.manager._create_creature(
            self.model, self.personality, self._serial, pos=pos,
            progression_state=progress, progression_id=f"mission-{self._serial}",
            team_id="adventurers" if role in ("hero", "ally") else "rivals",
            # Enemies are black and crimson with a redback marking, so they
            # never look like the player's own spider.
            color_overrides=None if role in ("hero", "ally") else enemy_palette(),
            skills=["jump", "shoot_web", "chase", "approach"])
        c.set_name({"hero": self.hero_name, "ally": "Scout", "guardian": "Thorn guardian"}.get(role, role.title()))
        if role == "hero":
            # The player picks the hero's skills in the character window;
            # points are banked rather than spent automatically (DC-57).
            c.chooses_own_skills = True
        c.playfield = self.playfield
        c.hp = c.max_hp
        c.energy = c.max_energy
        c.heading = c.target_heading = 0 if role in ("hero", "ally") else math.pi
        if role == "guardian":
            c.max_hp *= 2.5
            c.hp = c.max_hp
            c.damage *= 1.25
        elif role != "hero":
            c.max_hp *= .75 if role == "weaver" else .9
            c.hp = c.max_hp
            c.damage *= .65
        self.manager.creatures.append(c)
        if role != "hero":
            self.actors.append(MissionActor(c, self, role, pos, raider))
        self.manager._refresh_render_order()
        return c

    def issue(self, command, aim):
        if self.state != "active":
            return
        if self.ally.dead:
            self.announce("Scout has fallen. You can still finish the raid.")
            return
        if command == "attack":
            targets = [c for c in self.manager.creatures if not c.dead and self.hero.relation_to(c) == "foe"]
            target = min(targets, key=lambda c: math.hypot(c.x-aim[0], c.y-aim[1]), default=None)
            if target is None or math.hypot(target.x-aim[0], target.y-aim[1]) > 100:
                target = self.hover_target
            if target is None or target.dead:
                self.announce("Point at an enemy, then press Attack target.")
                return
            self.attack_target = target
        self.command = command
        if command == "defend":
            self.defend_point = (self.ally.x, self.ally.y)
        self.announce({"follow": "Scout: following you", "defend": "Scout: defending this position",
                       "attack": "Scout: attacking your target"}[command])
        for actor in self.actors:
            if actor.role == "ally":
                actor.think = 0

    def announce(self, text):
        self.notice, self.notice_time = text, 5.0

    @property
    def objective(self):
        if self.state == "victory":
            return "VICTORY - the desktop is yours"
        if self.state == "defeat":
            return "RAID ENDED - your spider has fallen"
        if not any(s.owned for s in self.sites[1:3]):
            return "01 / Capture the Food cache or Silk loom"
        if not self.sites[3].owned:
            return "02 / Seal the Hatchery to stop reinforcements"
        if any(raider for _, _, raider in self.pending) or any(a.raider and not a.creature.dead for a in self.actors):
            return "03 / Defeat the counterattack"
        return "04 / Defeat the guardian and claim Thorn nest"

    def update(self, dt):
        if self.state != "active":
            self.end_clock += max(0.0, dt)
            return
        dt = min(.05, max(0, dt))
        self.elapsed += dt
        for enemy in self.manager.creatures:
            if (not enemy.dead and self.hero.relation_to(enemy) == "foe"
                    and math.hypot(enemy.x-self.player.aim[0], enemy.y-self.player.aim[1]) < 75):
                self.hover_target = enemy
        self.notice_time = max(0, self.notice_time-dt)
        for c in list(self.manager.creatures):
            c.update(dt, -10000, -10000, self.manager.screen_w, self.manager.screen_h)
            c.x, c.y = self.area.clamp(c.x, c.y, c.margin)
        for projectile in self.manager.fly_world.projectiles:
            projectile.update(dt)
        self.manager.fly_world.projectiles = [p for p in self.manager.fly_world.projectiles if not p.done]
        self.manager._bury_the_dead()
        self.actors = [a for a in self.actors if not a.creature.dead]
        self.manager.carcasses = [c for c in self.manager.carcasses[-4:] if not c.update(dt)]
        if self.hero.dead:
            self.state = "defeat"
            self.player.clear_keys()
            self.finish(won=False)
            return
        if self.command == "attack" and (self.attack_target is None or self.attack_target.dead):
            self.command = "follow"
            self.attack_target = None
        for site in self.sites:
            site.contested = any(not c.dead and self.hero.relation_to(c) == "foe"
                                 and math.hypot(c.x-site.x, c.y-site.y) < 125
                                 for c in self.manager.creatures)
            near = math.hypot(self.hero.x-site.x, self.hero.y-site.y) < 92
            # A base heals and refills whichever side holds it, while the
            # other side keeps away: the player's bases for the player and
            # the Scout, the rest for the enemy.
            held_safe = (not site.contested) if site.owned else not self._adventurers_near(site)
            if held_safe:
                self._heal_at(site, dt)
                self._refill_silk_at(site, dt)
            if site.owned:
                continue
            unlocked = site.kind != "nest" or (self.sites[3].owned and any(s.owned for s in self.sites[1:3])
                        and self.guardian is not None and self.guardian.dead
                        and not self.pending and not any(a.raider for a in self.actors))
            if near and not site.contested and unlocked:
                site.progress = min(1, site.progress + dt/self.CAPTURE_SECONDS)
                if site.progress >= 1:
                    self.capture(site)
            elif not near:
                site.progress = max(0, site.progress - dt*.12)
        self._spawning(dt)

    # How close a spider must be to use a base. The Scout gets a little more,
    # because a following Scout trails about 85 px behind the hero.
    BASE_REACH = 92.0
    SCOUT_HEAL_REACH = 110.0
    # What each base gives its holder: health and silk per second. The Thorn
    # nest is the enemy's home burrow and serves them as the burrow serves you.
    HEAL_RATES = {"food": 8.0, "home": 3.0, "nest": 3.0}
    SILK_RATES = {"silk": 3.0, "home": 1.0, "nest": 1.0}

    def _adventurers_near(self, site) -> bool:
        return any(not c.dead and math.hypot(c.x-site.x, c.y-site.y) < 125
                   for c in (self.hero, self.ally) if c is not None)

    def _holders(self, site):
        """(spider, controller, reach) for every spider of the side holding ``site``."""
        if site.owned:
            yield self.hero, self.player, self.BASE_REACH
            if self.ally is not None and not self.ally.dead:
                ally = next((a for a in self.actors if a.creature is self.ally), None)
                yield self.ally, ally, self.SCOUT_HEAL_REACH
            return
        for actor in self.actors:
            if actor.role != "ally" and not actor.creature.dead:
                yield actor.creature, actor, self.BASE_REACH

    def silk_capacity_for(self, spider) -> int:
        """Holding the Silk loom raises the silk a side can carry, for either side."""
        loom = next((s for s in self.sites if s.kind == "silk"), None)
        adventurer = spider.progression.team_id == "adventurers"
        holds = loom is not None and loom.owned == adventurer
        return PlayerController.LOOM_SILK_CAPACITY if holds else PlayerController.SILK_CAPACITY

    def _refill_silk_at(self, site, dt):
        rate = self.SILK_RATES.get(site.kind)
        if rate is None or (site.kind == "home" and not site.owned):
            return
        for spider, control, reach in self._holders(site):
            if control is None or math.hypot(spider.x-site.x, spider.y-site.y) >= reach:
                continue
            control.silk = min(control.silk_capacity, control.silk + dt*rate)

    def _heal_at(self, site, dt):
        """A base heals the side that holds it, from its supply.

        The hero's Home and Food heal the hero and the Scout (the owner:
        "companion spider should also be able to heal in the bases if
        nearby"); the enemy's Food cache and Thorn nest heal the enemy, by the
        same rates (the owner: "make sure all spiders have the same rules").
        """
        rate = self.HEAL_RATES.get(site.kind)
        if rate is None or (site.kind == "home" and not site.owned):
            return
        for spider, _control, reach in self._holders(site):
            if site.supply <= 0:
                return
            if math.hypot(spider.x-site.x, spider.y-site.y) >= reach:
                continue
            amount = min(dt*rate, site.supply, spider.max_hp-spider.hp)
            if amount > 0:
                spider.heal(amount)
                site.supply -= amount

    def capture(self, site):
        if site.owned:
            return
        site.owned = True
        self.hero.gain_experience(35, "territory captured")
        self.announce(f"{site.name} secured")
        if site.kind == "silk":
            self.player.silk_capacity = PlayerController.LOOM_SILK_CAPACITY
            self.player.silk = float(self.player.silk_capacity)
            # The enemy lost the loom: back to what they can carry without it.
            for actor in self.actors:
                if actor.role != "ally":
                    actor.silk_capacity = PlayerController.SILK_CAPACITY
                    actor.silk = min(actor.silk, actor.silk_capacity)
        if site.kind == "hatchery":
            site.reserves = 0
            site.warning = 0
            self.pending = [entry for entry in self.pending if entry[0] != "hatchery"]
        if site.kind in ("food", "silk") and not self.counter_started:
            self.counter_started = True
            self.pending.extend([("nest", "hunter", True), ("nest", "guard", True)])
            self.sites[4].warning = 4.0
            self.announce("Outpost secured! Counterattack from Thorn nest in 4 seconds.")
        if site.kind == "nest":
            self.state = "victory"
            self.hero.gain_experience(100, "raid complete")
            self.player.clear_keys()
            self.finish(won=True)

    def _spawning(self, dt):
        hatch = self.sites[3]
        self.wave_clock -= dt
        if not hatch.owned and hatch.reserves > 0 and self.wave_clock <= 0:
            if not any(e[0] == "hatchery" for e in self.pending):
                self.pending.append(("hatchery", "hunter", True))
                hatch.warning = 3.0
                self.announce("The Hatchery is stirring - a hunter is emerging")
            self.wave_clock = 24.0
        for site in self.sites:
            site.warning = max(0, site.warning-dt)
        for entry in list(self.pending):
            source, role, raider = entry
            site = next(s for s in self.sites if s.kind == source)
            if site.owned or (source == "hatchery" and site.reserves <= 0):
                self.pending.remove(entry)
                continue
            if site.warning > 0 or len(self.manager.creatures) >= 8:
                continue
            pos = self.area.clamp(site.x+50, site.y+50, 65)
            if math.hypot(self.hero.x-pos[0], self.hero.y-pos[1]) < 150:
                continue
            if self._spawn(role, pos, raider) is not None:
                self.pending.remove(entry)
                if source == "hatchery":
                    site.reserves -= 1
                site.warning = 1.5 if any(e[0] == source for e in self.pending) else 0
        if (self.guardian is None and hatch.owned and any(s.owned for s in self.sites[1:3])
                and not self.pending and not any(a.raider for a in self.actors)):
            nest = self.sites[4]
            if self.guardian_warning is None:
                self.guardian_warning = 4.0
                self.announce("Thorn nest is stirring. The guardian will emerge in 4 seconds.")
            self.guardian_warning = max(0.0, self.guardian_warning-dt)
            nest.warning = self.guardian_warning
            if self.guardian_warning <= 0 and math.hypot(self.hero.x-nest.x, self.hero.y-nest.y) >= 150:
                self.guardian = self._spawn("guardian", (nest.x, nest.y))
                if self.guardian is not None:
                    self.announce("Thorn guardian awakened. Bait its strike, then counterattack.")

    @property
    def ended(self) -> bool:
        return self.state != "active"

    @property
    def end_screen_done(self) -> bool:
        return self.ended and self.end_clock >= self.END_SCREEN_SECONDS

    def finish(self, won: bool) -> None:
        """Bank the hero and record the result, once, however the raid ended.

        The owner: "as progress keep it level xp and skills chosen saved."
        A defeat keeps what the hero earned, as a win does.
        """
        if self.saved:
            return
        record_result(self.profile, self.MISSION_ID, won, self.elapsed)
        self.save_progress()

    def save_progress(self) -> bool:
        """Write the hero's progression to adventure-hero.json.

        Also called when the player leaves mid-raid. Mission entities never
        enter creatures.json.
        """
        self.profile["progression"] = self.hero.progression.to_dict()
        if save_profile(self.profile, self.progress_path):
            self.saved = self.ended
            self.save_error = ""
            return True
        self.save_error = "Progress could not be saved. Retry from the pause menu."
        return False
