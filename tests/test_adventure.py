"""Adventure ownership, stamina, and the new start navigation."""
from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

from PyQt5.QtCore import QPoint, QRect, Qt

from desktop_bug.app.adventure import PlayerController
from desktop_bug.app.adventure_ui import hud_rect
from desktop_bug.app.engine import OverlayWindow
from desktop_bug.app.config_ui import ConfigWindow
from desktop_bug.creature import Creature
from desktop_bug.manager import CreatureManager
from desktop_bug.state.progression import xp_to_next_level
from support import load_pair


def test_player_moves_and_spends_stamina_without_ai_taking_over():
    model, personality = load_pair()
    spider = Creature(model, personality, 900, 700, index=0, seed=7)
    player = PlayerController(spider)
    start_x = spider.x
    player.set_held(Qt.Key_D, True)
    player.set_held(Qt.Key_Shift, True)
    for _ in range(120):
        spider.update(1 / 60, 100, 100, 900, 700)
    assert spider.x > start_x + 5
    assert spider.energy < spider.max_energy
    assert spider.state == "Player"
    assert spider.player_control is player
    player.clear_keys()
    player.release()
    assert spider.player_control is None


def test_player_earns_points_to_choose_in_the_existing_tree():
    model, personality = load_pair()
    spider = Creature(model, personality, 900, 700, index=0, seed=7)
    player = PlayerController(spider)
    spider.gain_experience(xp_to_next_level(1), "test")
    assert spider.level == 2
    assert spider.progression.skill_points == 1
    assert not spider.progression.unlocked_abilities
    assert spider.unlock_progression_ability("vitality")[0]
    player.release()


def test_adventure_web_shot_uses_real_projectile_and_cooldown(state_dir):
    manager = CreatureManager(Path("presets/colony.json"), 900, 700, seed=4)
    shooter, target = manager.creatures[:2]
    shooter.x = target.x - 90
    shooter.y = target.y
    shooter.progression.team_id = "pack_a"
    target.progression.team_id = "rivals"
    player = PlayerController(shooter)
    player.aim = (target.x, target.y)
    before = shooter.energy
    assert player.shoot(manager)
    assert shooter.energy == before - player.WEB_ENERGY
    assert player.web_cooldown > 0
    assert manager.fly_world.projectiles
    assert not player.shoot(manager)


def test_mode_menu_opens_each_view_and_keeps_companion_settings(qapp, state_dir):
    window = ConfigWindow()
    try:
        shell = window.mode_shell
        assert shell.stack.currentWidget() is shell.home
        shell.show_mode("companion")
        assert shell.stack.currentWidget() is shell.companion
        assert window.launch_btn.text() == "Save and launch overlay"
        shell.show_mode("skirmish")
        assert shell.stack.currentWidget() is shell.skirmish
        assert shell.adventure_launch.isEnabled()
        shell.show_mode("strategy")
        assert shell.stack.currentWidget() is shell.strategy
    finally:
        window.close()




def test_foe_can_hit_controlled_spider_without_automatic_counterattack(state_dir):
    manager = CreatureManager(Path("presets/colony.json"), 900, 700, seed=4)
    spider, foe = manager.creatures[:2]
    spider.progression.team_id = "pack_a"
    foe.progression.team_id = "rivals"
    foe.x, foe.y = spider.x + spider.size, spider.y
    player = PlayerController(spider)
    spider.attack_cooldown = foe.attack_cooldown = 0.0
    spider_hp, foe_hp = spider.hp, foe.hp
    manager._resolve_combat(1 / 60)
    assert spider.hp < spider_hp
    assert foe.hp == foe_hp
    assert spider.player_control is player


def test_status_panel_can_be_dragged_without_shooting(qapp):
    repaints = []
    overlay = SimpleNamespace(
        mode="adventure", player=object(), _adventure_paused=False,
        geometry_rect=QRect(0, 0, 800, 600),
        _adventure_hud_position=None, _adventure_hud_drag_offset=None,
        width=lambda: 800, height=lambda: 600,
        _request_full_repaint=lambda: repaints.append(True),
    )
    original = hud_rect(overlay)
    pressed = original.center()
    press = SimpleNamespace(globalPos=lambda: pressed, button=lambda: Qt.LeftButton,
                            accept=lambda: None)
    OverlayWindow.mousePressEvent(overlay, press)
    assert overlay._adventure_hud_drag_offset is not None

    moved = QPoint(250, 220)
    move = SimpleNamespace(globalPos=lambda: moved, buttons=lambda: Qt.LeftButton,
                           accept=lambda: None)
    OverlayWindow.mouseMoveEvent(overlay, move)
    assert hud_rect(overlay).topLeft() == moved - (pressed - original.topLeft())
    assert repaints

    release = SimpleNamespace(button=lambda: Qt.LeftButton, accept=lambda: None)
    OverlayWindow.mouseReleaseEvent(overlay, release)
    assert overlay._adventure_hud_drag_offset is None


def test_spider_size_and_name_style_follow_level_not_model(qapp):
    model, personality = load_pair()
    level_one = Creature(model, personality, 900, 700, index=0, seed=7)
    other_model = dict(model, base_size=100)
    another = Creature(other_model, personality, 900, 700, index=1, seed=8)
    higher = Creature(model, personality, 900, 700, index=2, seed=9,
                      progression_state={"level": 8})
    assert level_one.size == another.size == Creature.BASE_SIZE
    assert level_one._label_font().pointSizeF() == another._label_font().pointSizeF()
    assert level_one._label_font().weight() == another._label_font().weight()
    assert level_one.size < higher.size < level_one.size * 1.1
    assert level_one._label_font().pointSizeF() < higher._label_font().pointSizeF()
    assert level_one._label_font().weight() == higher._label_font().weight()
