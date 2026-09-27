"""Local co-op: independent input/progression and a synchronized shared armoury."""
from pathlib import Path
from types import SimpleNamespace

import pytest
from PyQt5.QtCore import QCoreApplication, Qt, QTimer
from PyQt5.QtWidgets import QApplication

from desktop_bug.app.adventure_profile import (
    PLAYER_TWO, fresh_profile, load_profile, save_profile, spider_progression, store_progression,
)
from desktop_bug.app.armoury import wear
from desktop_bug.app.character_ui import CharacterDialog, PartyDialog, show_party_windows
from desktop_bug.app.controls import ControlSettings, DEFAULT_BINDINGS, second_player_controls
from desktop_bug.app.engine import OverlayWindow
from desktop_bug.app.mission import Loot, TerritoryMission
from desktop_bug.manager import CreatureManager
from desktop_bug.state.progression import ABILITY_TREE


@pytest.fixture(autouse=True, scope="module")
def keep_qt_running(qapp):
    # Earlier UI tests may leave a posted last-window-close quit. Nested
    # dialog loops must outlive that unrelated test's windows.
    previous = qapp.quitOnLastWindowClosed()
    qapp.setQuitOnLastWindowClosed(False)
    QCoreApplication.removePostedEvents(qapp, 20)  # QEvent::Quit; PyQt5 does not name it
    yield
    qapp.setQuitOnLastWindowClosed(previous)


@pytest.fixture
def mission(qapp, state_dir):
    profile = fresh_profile()
    profile['two_player'] = True
    save_profile(profile)
    manager = CreatureManager(Path('presets/colony.json'), 1600, 1000, seed=4)
    return TerritoryMission(manager, ControlSettings())


def test_defaults_and_legacy_migration(qapp):
    old = {**DEFAULT_BINDINGS, 'shoot': 'Mouse Left', 'bite': 'Mouse Right'}
    migrated = ControlSettings.from_dict({'bindings': old})
    assert migrated.binding('bite') == 'F'
    assert migrated.binding('shoot') == 'Q'
    explicit = ControlSettings.from_dict({'version': 2, 'bindings': old})
    assert explicit.binding('bite') == 'Mouse Right'
    assert explicit.binding('shoot') == 'Mouse Left'
    p2 = second_player_controls()
    assert p2.action_for_key(Qt.Key_Up) == 'move_up'
    assert p2.action_for_key(Qt.Key_J) == 'bite'
    # Only the screen-wide keys are shared: pause, the map (`) and release (F8).
    assert set(DEFAULT_BINDINGS.values()) & set(p2.bindings.values()) == {'Esc', '`', 'F8'}


def test_players_spawn_independently_and_save(mission):
    m = mission
    assert len(m.players) == 2
    assert m.hero is not m.second_hero
    assert m.second_hero.level == 1
    assert m.hero.relation_to(m.second_hero) != 'foe'
    assert m.second_hero.chooses_own_skills
    assert m.second_hero.progression.equipped == {}
    m.second_hero.gain_experience(2000, 'test')
    assert m.hero.level == 1
    assert m.second_hero.progression.skill_points > 0
    assert m.save_progress()
    restored = m.restarted()
    assert restored.second_hero.level == m.second_hero.level
    assert len(restored.players) == 2


def event(key, repeat=False):
    return SimpleNamespace(key=lambda: key, isAutoRepeat=lambda: repeat, accept=lambda: None)


def test_simultaneous_keyboard_input_and_release(mission):
    m = mission
    fake = SimpleNamespace(mode='adventure', mission=m, player=m.player,
                           controls=ControlSettings(), manager=m.manager, _adventure_paused=False)
    fake._adventure_action = lambda *a, **k: OverlayWindow._adventure_action(fake, *a, **k)
    for key in (Qt.Key_W, Qt.Key_Up, Qt.Key_F, Qt.Key_J):
        OverlayWindow.keyPressEvent(fake, event(key))
    assert m.players[0].held == m.players[1].held == {'move_up', 'bite'}
    OverlayWindow.keyReleaseEvent(fake, event(Qt.Key_W))
    OverlayWindow.keyReleaseEvent(fake, event(Qt.Key_F))
    assert not m.players[0].held
    assert m.players[1].held == {'move_up', 'bite'}
    OverlayWindow.keyReleaseEvent(fake, event(Qt.Key_J, repeat=True))
    assert 'bite' in m.players[1].held
    OverlayWindow.keyReleaseEvent(fake, event(Qt.Key_J))
    assert m.players[1].held == {'move_up'}


def test_held_attack_repeats_at_cooldown_and_stops(mission, monkeypatch):
    m = mission
    p = m.players[1]
    strikes = []
    monkeypatch.setattr(p.creature, 'begin_strike', lambda *args: strikes.append(args))
    p.set_held('bite', True)
    for _ in range(20):
        m.update(.05)
    assert 2 <= len(strikes) <= 3
    p.set_held('bite', False)
    count = len(strikes)
    for _ in range(10):
        m.update(.05)
    assert len(strikes) == count
    assert m.hero.attack_cooldown == 0


def test_keyboard_aim_and_web_are_independent(mission):
    m = mission
    first, second = m.players
    first.creature.heading = .3
    second.creature.heading = 2.1
    first.aim = second.aim = (-9000, -9000)
    assert first.aim_angle() == .3
    assert second.aim_angle() == 2.1
    assert second.shoot(m.manager)
    assert second.silk == 7 and first.silk == 8
    assert first.web_cooldown == 0


def test_second_player_can_capture_collect_and_refill(mission):
    m = mission
    for actor in m.actors:
        if actor.role != 'ally':
            actor.creature.dead = True
    site = m.footholds[0]
    m.second_hero.x, m.second_hero.y = site.x, site.y
    site.progress = .999
    m.update(.05)
    assert site.owned
    item = m.profile['armoury']['owned'][0]
    m.loot.append(Loot(item, m.second_hero.x, m.second_hero.y))
    m._collect_loot(.05)
    assert m.profile['armoury']['spares'][item] == 1
    home = m.sites[0]
    m.second_hero.x, m.second_hero.y = home.x, home.y
    m.players[1].silk = 0
    m._refill_silk_at(home, 1)
    assert m.players[1].silk > 0


def test_defeat_waits_for_both_players(mission):
    m = mission
    m.hero.dead = True
    m.update(.01)
    assert m.state == 'active'
    m.second_hero.dead = True
    m.update(.01)
    assert m.state == 'defeat'
    assert all(not p.held for p in m.players)


def test_switching_to_single_player_preserves_second_progress(mission):
    m = mission
    m.second_hero.gain_experience(400, 'test')
    m.save_progress()
    profile = load_profile()
    profile['two_player'] = False
    save_profile(profile)
    solo = m.restarted()
    assert len(solo.players) == 1 and solo.second_hero is None
    solo.save_progress()
    assert load_profile()[PLAYER_TWO]['progression']['total_xp'] >= 400


def test_sheets_share_armour_but_not_skills(qapp, state_dir):
    profile = fresh_profile()
    profile['two_player'] = True
    second = spider_progression(profile, PLAYER_TWO)
    second.skill_points = 2
    second.level = 3
    store_progression(profile, PLAYER_TWO, second)
    one = CharacterDialog(profile=profile, fixed_who=True)
    two = CharacterDialog(profile=profile, who=PLAYER_TWO, fixed_who=True)
    for dialog in (one, two):
        dialog.changed.connect(lambda: (one.refresh(), two.refresh()))
    item = profile['armoury']['owned'][0]
    one._equip(item)
    two._equip(item)
    assert item in two.dress.equipped.values()
    assert item not in one.dress.equipped.values()
    ability = next(a for a in ABILITY_TREE
                   if two.state.can_unlock(a.id))
    two._unlock(ability.id)
    assert ability.id in two.state.unlocked_abilities
    assert ability.id not in one.state.unlocked_abilities
    two.name_edit.setText('Second hero')
    two._rename()
    assert load_profile()[PLAYER_TWO]['name'] == 'Second hero'
    assert profile['name'] != 'Second hero'
    one.close()
    two.close()


def test_both_windows_are_visible_together(qapp, state_dir):
    profile = fresh_profile()
    profile['two_player'] = True
    save_profile(profile)
    observed = []

    def inspect():
        # One window, both players side by side (the owner's layout).
        windows = [w for w in QApplication.topLevelWidgets() if isinstance(w, PartyDialog) and w.isVisible()]
        sheets = [c for w in windows for c in w.findChildren(CharacterDialog) if c.isVisible()]
        observed.append(len(sheets))
        for window in windows:
            window.close()

    QTimer.singleShot(100, inspect)
    show_party_windows()
    assert observed == [2]


def test_live_equipment_refresh_applies_both_players(mission):
    m = mission
    m.save_progress()
    profile = load_profile()
    item = profile['armoury']['owned'][0]
    assert wear(profile, PLAYER_TWO, item)
    save_profile(profile)
    m.refresh_from_profile()
    assert item in m.second_hero.progression.equipped.values()
    assert item not in m.hero.progression.equipped.values()


def test_swarm_rewards_and_refills_second_player(mission):
    from desktop_bug.app.swarm import FlySwarmMission
    m = mission
    profile = load_profile()
    profile['selected_map'] = 'swarm'
    profile['admin'] = True
    save_profile(profile)
    swarm = FlySwarmMission(m.manager, ControlSettings())
    second = swarm.players[1]
    second.silk = 0
    fly = swarm.manager.fly_world.flies[0]
    swarm._eat(fly, second.creature)
    assert second.silk > 0 and swarm.caught == 1
    before = second.creature.progression.total_xp
    second.set_held('bite', True)
    swarm._end(True, 'test')
    assert second.creature.progression.total_xp == before + 80
    assert not second.held


def test_paired_windows_disable_parent_and_restore_it(qapp, state_dir):
    from PyQt5.QtWidgets import QWidget
    parent = QWidget()
    parent.show()
    profile = fresh_profile()
    profile['two_player'] = True
    save_profile(profile)
    observed = []

    def inspect():
        windows = [w for w in QApplication.topLevelWidgets() if isinstance(w, PartyDialog) and w.isVisible()]
        sheets = [c for w in windows for c in w.findChildren(CharacterDialog) if c.isVisible()]
        observed.append(not parent.isEnabled() and len(sheets) == 2 and all(s.isEnabled() for s in sheets))
        for window in windows:
            window.close()

    QTimer.singleShot(100, inspect)
    show_party_windows(parent)
    assert observed == [True] and parent.isEnabled()
    parent.close()


def test_real_coop_overlay_frames_and_pause(qapp, state_dir):
    from desktop_bug.app.adventure_ui import hud_rect
    profile = fresh_profile()
    profile['two_player'] = True
    save_profile(profile)
    window = OverlayWindow(Path('presets/colony.json'), mode='adventure')
    try:
        assert len(window.mission.players) == 2
        window.keyPressEvent(event(Qt.Key_J))
        for _ in range(4):
            window.tick()
        assert window.mission.second_hero.attack_cooldown > 0
        region = window._current_paint_region()
        assert region.contains(hud_rect(window, 0).center())
        assert region.contains(hud_rect(window, 1).center())
        assert not hud_rect(window, 0).intersects(hud_rect(window, 1))
        assert not window.grab().toImage().isNull()
        elapsed = window.mission.elapsed
        window._adventure_paused = True
        window.tick()
        assert window.mission.elapsed == elapsed
    finally:
        window.timer.stop()
        window.style_timer.stop()
        window.deleteLater()


def test_mode_checkbox_persists(qapp, state_dir):
    from PyQt5.QtWidgets import QWidget
    from desktop_bug.app.mode_menu import ModeShell
    shell = ModeShell(QWidget(), lambda: None)
    try:
        assert not shell.two_player_check.isChecked()
        shell.two_player_check.setChecked(True)
        assert load_profile()['two_player'] is True
        shell.refresh_adventure()
        assert shell.two_player_check.isChecked()
        shell.two_player_check.setChecked(False)
        assert load_profile()['two_player'] is False
    finally:
        shell.close()


def test_both_players_turn_and_walk_by_default(mission):
    """The owner: "make default controls for both players turn based ... not
    with the arrow direction"."""
    assert all(p.controls.turn_movement for p in mission.players)


def test_the_settings_list_still_scrolls_in_two_player(qapp, state_dir):
    from desktop_bug.app.controls_ui import ControlsEditor

    editor = ControlsEditor()
    editor.lock_bindings(True)
    assert editor.isEnabled() and editor.movement.isEnabled(), "the list and movement stay usable"
    assert not any(b.isEnabled() for b in editor.buttons.values()) and not editor.reset_button.isEnabled()


def test_the_party_window_puts_each_bag_below_its_spider(qapp, state_dir):
    """The owner: "make inventory below the spider anatomy ... one player's
    spider on one side, the other's on the other side with inventory below"."""
    profile = fresh_profile()
    profile['two_player'] = True
    save_profile(profile)
    window = PartyDialog(profile=profile)
    window.show()
    qapp.processEvents()
    left, right = window.sheets
    for sheet in (left, right):
        doll = sheet.doll.mapTo(window, sheet.doll.rect().bottomLeft())
        bag = sheet.bag.mapTo(window, sheet.bag.rect().topLeft())
        assert bag.y() >= doll.y() - 2, "the bag is under the doll"
    assert right.mapTo(window, right.rect().topLeft()).x() > left.mapTo(window, left.rect().topRight()).x() - 2
    window.close()
