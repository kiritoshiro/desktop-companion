"""Mission behaviour exercised with real creatures, plus live-overlay wiring."""
from pathlib import Path

import pytest

from PyQt5.QtCore import Qt

from desktop_bug.app.controls import ControlSettings
from desktop_bug.app.engine import OverlayWindow
from desktop_bug.app.mission import TerritoryMission
from desktop_bug.manager import CreatureManager


def make_mission():
    manager = CreatureManager(Path("presets/colony.json"), 1600, 1000, seed=4)
    return TerritoryMission(manager, ControlSettings())


def step(mission, seconds):
    for _ in range(round(seconds*60)):
        mission.update(1/60)


def clear_enemies(mission):
    for c in mission.manager.creatures:
        if mission.hero.relation_to(c) == "foe":
            c.take_damage(100000, mission.hero)
    mission.manager._bury_the_dead()
    mission.actors = [a for a in mission.actors if not a.creature.dead]


def stand(mission, site):
    mission.hero.x, mission.hero.y = site.x, site.y
    mission.hero.target_x, mission.hero.target_y = site.x, site.y
    mission.hero._initialize_legs()


def test_mission_starts_with_five_spiders_and_runs(state_dir):
    m = make_mission()
    assert len(m.manager.creatures) == 5
    assert len(m.sites) == 5
    step(m, 3)
    assert m.state == "active"
    assert m.hero.x == pytest.approx(m.sites[0].x)
    assert len(m.manager.creatures) <= 10


def test_silk_fires_without_target_and_hits_enemy(state_dir):
    m = make_mission()
    m.player.aim = (m.hero.x+250, m.hero.y)
    assert m.player.shoot(m.manager)
    assert m.player.silk == 7
    shot = m.manager.fly_world.projectiles[-1]
    shot.update(.05)
    assert shot.travelled > 0 and not shot.hit
    target = next(c for c in m.manager.creatures if m.hero.relation_to(c) == "foe")
    target.x, target.y = m.hero.x+100, m.hero.y
    for _ in range(30):
        shot.update(.02)
    assert target.webbed
    assert shot.hit


def test_empty_silk_and_home_refill(state_dir):
    m = make_mission()
    m.player.silk = 0
    assert not m.player.shoot(m.manager)
    assert "Silk empty" in m.player.feedback
    step(m, 1.2)
    assert m.player.silk >= 1
    assert m.player.shoot(m.manager)


def test_capture_requires_clear_area_and_triggers_counterattack(state_dir):
    m = make_mission()
    site = m.sites[1]
    stand(m, site)
    m._spawn("guard", (site.x, site.y))
    step(m, .25)
    assert site.contested and site.progress == 0
    clear_enemies(m)
    m.hero.hp = m.hero.max_hp
    step(m, 4.1)
    assert site.owned
    assert m.counter_started
    assert len(m.pending) == 2
    assert m.sites[4].warning > 0


def test_silk_capture_and_hatchery_cancel_pending_spawns(state_dir):
    m = make_mission()
    m.capture(m.sites[2])
    assert m.player.silk_capacity == 12 and m.player.silk == 12
    m.pending.append(("hatchery", "hunter", True))
    m.capture(m.sites[3])
    assert not any(e[0] == "hatchery" for e in m.pending)
    assert m.sites[3].reserves == 0


def test_spawn_budget_and_safe_emergence(state_dir):
    m = make_mission()
    for _ in range(20):
        m._spawn("guard", (800, 450))
    assert len(m.manager.creatures) == 10
    m.pending = [("hatchery", "hunter", True)]
    m._spawning(.05)
    assert m.pending
    assert len(m.manager.creatures) == 10
    clear_enemies(m)
    site = m.sites[3]
    stand(m, site)
    m._spawning(.05)
    assert m.pending  # cannot materialize on top of the player


def test_companion_orders_and_target_death(state_dir):
    m = make_mission()
    m.issue("defend", (0, 0))
    point = m.defend_point
    m.hero.x += 300
    step(m, .3)
    assert m.command == "defend" and m.defend_point == point
    target = next(c for c in m.manager.creatures if m.hero.relation_to(c) == "foe")
    m.issue("attack", (target.x, target.y))
    assert m.attack_target is target and m.command == "attack"
    target.take_damage(100000, m.hero)
    step(m, .1)
    assert m.command == "follow"
    m.issue("follow", (0, 0))
    assert m.command == "follow"


def test_victory_requires_guardian_and_banks_separate_progress(state_dir):
    m = make_mission()
    clear_enemies(m)
    stand(m, m.sites[4])
    step(m, 4.1)
    assert not m.sites[4].owned
    m.capture(m.sites[1])
    m.capture(m.sites[3])
    m.pending.clear()
    stand(m, m.sites[0])
    step(m, 4.2)
    assert m.guardian is not None
    m.guardian.take_damage(100000, m.hero)
    stand(m, m.sites[4])
    step(m, 4.2)
    assert m.state == "victory"
    assert m.progress_path.exists()
    assert m.saved
    elapsed = m.elapsed
    step(m, 2)
    assert m.elapsed == elapsed


def test_defeat_keeps_the_heros_progress_and_restart_starts_clean(state_dir):
    """2026-09-25, the owner: "as progress keep it level xp and skills chosen
    saved." A defeat used to throw away what the raid earned; it now keeps
    it, as a win does. The enemies and the raid itself still start fresh."""
    m = make_mission()
    m.hero.gain_experience(30, "test")
    earned = m.hero.progression.total_xp
    m.hero.take_damage(100000)
    step(m, .1)
    assert m.state == "defeat"
    fresh = TerritoryMission(m.manager, m.controls, m.area)
    assert fresh.hero.progression.total_xp == earned
    assert fresh.hero.progression.team_id == "adventurers"
    assert fresh.state == "active"
    assert all(not c.dead for c in fresh.manager.creatures)


def test_mission_save_leaves_companion_file_untouched(state_dir):
    manager = CreatureManager(Path("presets/colony.json"), 1600, 1000, seed=4)
    manager.save_runtime_state()
    path = manager._progression_state_path
    before = path.read_bytes()
    m = TerritoryMission(manager, ControlSettings())
    m.hero.gain_experience(30, "test")
    manager.save_runtime_state()
    assert path.read_bytes() == before
    m.state = "victory"
    manager.save_runtime_state()
    assert path.read_bytes() == before
    assert m.progress_path.exists()


def test_overlay_launch_paint_input_and_reload_isolation(qapp, state_dir):
    window = OverlayWindow(Path("presets/colony.json"), seed=4, mode="adventure")
    try:
        window.timer.stop()
        assert window.mission is not None
        assert window.player is window.mission.player
        window._adventure_action("companion_defend", True)
        assert window.mission.command == "defend"
        hero = window.mission.hero
        window._on_channel_message({"type": "preset_update", "data": {}})
        assert window.mission.hero is hero
        window._adventure_action("move_right", True)
        window.mission.update(.05)
        window._adventure_action("move_right", False)
        assert window.controls.action_for_key(Qt.Key_1) == "companion_follow"
        window.grab()  # executes complete HUD/building render path
    finally:
        window.timer.stop()
        window.style_timer.stop()
        window.input_timer.stop()
        window.close()
        window.deleteLater()


def test_enemy_windup_can_be_interrupted_by_silk(state_dir):
    m = make_mission()
    guard = next(a for a in m.actors if a.role == "guard")
    stand(m, m.sites[1])
    guard.creature.x, guard.creature.y = m.hero.x+65, m.hero.y
    guard.target = m.hero
    guard.think = 1
    before = m.hero.hp
    guard.update(.05)
    assert guard.windup > 0
    assert m.hero.hp == before
    guard.creature.web_pinned("trap", m.hero)
    guard.update(.05)
    assert guard.windup == 0
    assert m.hero.hp == before


def test_hunter_pounce_is_warned_then_airborne(state_dir):
    m = make_mission()
    hunter = next(a for a in m.actors if a.role == "hunter")
    hunter.creature.x, hunter.creature.y = m.hero.x+150, m.hero.y
    hunter.target = m.hero
    hunter.think = 2
    hunter.update(.05)
    assert hunter.windup > 0
    assert not hunter.creature.airborne
    for _ in range(12):
        hunter.update(.05)
    assert hunter.creature.airborne


def test_attack_button_reuses_pointed_enemy(state_dir):
    m = make_mission()
    target = next(c for c in m.manager.creatures if m.hero.relation_to(c) == "foe")
    m.player.aim = target.x, target.y
    m.update(.01)
    m.issue("attack", (800, 980))
    assert m.attack_target is target


def test_reinforcement_warning_and_finite_reserves(state_dir):
    m = make_mission()
    m.wave_clock = 0
    before = len(m.manager.creatures)
    m._spawning(.05)
    assert len(m.manager.creatures) == before
    assert m.sites[3].warning > 0
    for _ in range(65):
        m._spawning(.05)
    assert len(m.manager.creatures) == before+1
    assert m.sites[3].reserves == 5


def test_projectile_miss_expires_and_cannot_hit_friend(state_dir):
    m = make_mission()
    m.hero.heading = 0
    m.player.aim = m.hero.x+300, m.hero.y
    m.ally.x, m.ally.y = m.hero.x+80, m.hero.y
    assert m.player.shoot(m.manager)
    shot = m.manager.fly_world.projectiles[-1]
    for _ in range(80):
        shot.update(.02)
    assert shot.done and not shot.hit
    assert not m.ally.webbed


def test_base_art_cache_has_bounded_variants(qapp):
    from desktop_bug.app.mission_ui import building_art
    a = building_art("silk", True)
    assert building_art("silk", True) is a
    assert a.pixelColor(0, 0).alpha() == 0
    assert not a.isNull()


def test_new_companion_bindings_preserve_old_custom_keys():
    settings = ControlSettings.from_dict({"bindings": {"shoot": "1", "bite": "2", "jump": "3"}})
    assert settings.binding("shoot") == "1"
    assert settings.binding("bite") == "2"
    assert settings.binding("jump") == "3"
    assert len(set(settings.bindings.values())) == len(settings.bindings)
    assert settings.binding("companion_follow") != "1"


def test_malformed_saved_level_and_companion_diplomacy_cannot_break_mission(state_dir):
    import json
    (state_dir / "adventure-hero.json").write_text(json.dumps({"progression": {
        "level": "broken", "relation_overrides": {"rivals": "friend"}}}), encoding="utf-8")
    m = make_mission()
    assert m.hero.level == 1
    assert all(m.hero.relation_to(a.creature) == "foe" for a in m.actors if a.role != "ally")


def test_primary_monitor_arena_clamps_destination_before_movement(state_dir):
    from desktop_bug.world.playfield import ScreenRect
    manager = CreatureManager(Path("presets/colony.json"), 2500, 1000, seed=4)
    area = ScreenRect(900, 0, 1600, 1000)
    m = TerritoryMission(manager, ControlSettings(), area)
    m.hero.x = area.x + m.hero.margin + 1
    m.hero.y = 550
    m.hero._initialize_legs()
    m.player.set_held("move_left", True)
    step(m, 1)
    assert m.hero.x >= area.x + m.hero.margin
    assert m.hero.target_x >= area.x + m.hero.margin
    assert m.hero.playfield.rects == (area,)


def test_enemies_are_black_and_red_and_the_heroes_are_not(state_dir):
    """The owner: "enemies should be of different color than my spider.
    make them more black-red pattern." """
    m = make_mission()
    friends = [c for c in m.manager.creatures if m.hero.relation_to(c) != "foe"]
    foes = [c for c in m.manager.creatures if m.hero.relation_to(c) == "foe"]
    assert m.hero in friends and m.ally in friends and foes
    for c in friends:
        assert "marking" not in c.colors, c.display_name
    for c in foes:
        assert "marking" in c.colors, c.display_name
        assert max(c.colors["body"]) < 40, c.display_name


def test_the_scout_heals_at_an_owned_base_too(state_dir):
    """The owner: "companion spider should also be able to heal in the bases
    if nearby." The hero always could; the Scout never did."""
    m = make_mission()
    home = m.sites[0]
    clear_enemies(m)
    m.hero.x, m.hero.y = home.x + 400, home.y
    m.ally.hp = m.ally.max_hp * 0.4
    m.ally.x, m.ally.y = home.x + 60, home.y + 40
    before, supply = m.ally.hp, home.supply
    m._heal_at(home, 1.0)
    assert m.ally.hp > before, "a hurt Scout by the burrow should heal"
    assert home.supply < supply, "from the same supply as the hero"
    far = m.ally.hp
    m.ally.x, m.ally.y = home.x + 300, home.y
    m._heal_at(home, 1.0)
    assert m.ally.hp == far, "not from across the arena"
