"""The owner's round of Adventure polish.

- *"when attacking it should show attacking movement, maybe the bite"*: a
  bite winds up, snaps forward with the front legs thrown at the target,
  and settles -- and a bite at nothing still shows.
- *"create those woody textures for other windows, like skills, right
  click, inspection, inventory"*: one application-wide sheet for dialogs
  and menus.
- *"from the menu remove that row of randomize"*.
- *"when leaving the adventure mode ... the spiders are left on screen"*:
  leaving the Adventure page closes the Adventure overlay.
"""

from __future__ import annotations

import pytest
from PyQt5.QtWidgets import QLabel

from desktop_bug.app import wood_theme
from desktop_bug.app.adventure import PlayerController
from desktop_bug.app.mode_menu import ModeShell
from desktop_bug.creature import Creature
from desktop_bug.creature.constants import LUNGE_REACH, STRIKE_DURATION
from support import ROOT, load_pair

DT = 1.0 / 60.0


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Widgets and style sheets need the one Qt application object."""


def _spider():
    model, personality = load_pair()
    spider = Creature(model, personality, 900, 700, index=0, seed=7)
    spider.x, spider.y = 450.0, 350.0
    spider.heading = spider.target_heading = 0.0
    return spider


# ------------------------------------------------------------------ the bite

def test_a_bite_at_nothing_still_shows():
    spider = _spider()
    player = PlayerController(spider)
    player.aim = (spider.x + 200.0, spider.y)
    manager = type("M", (), {"creatures": [spider], "conflict_enabled": True})()
    assert player.bite(manager) is False, "there was nothing to hit"
    assert spider.strike_clock is not None, "the miss did not animate"
    assert spider.attack_cooldown > 0.0, "a whiff should cost a short recovery"
    furthest = 0.0
    for _ in range(int(STRIKE_DURATION / DT) + 2):
        spider._update_combat_pose(DT)
        furthest = max(furthest, spider.combat_body_offset()[0])
    assert furthest >= LUNGE_REACH * spider.size, "the body did not snap forward"
    assert spider.strike_clock is None and spider.combat_body_offset() == (0.0, 0.0)


def test_the_front_legs_are_thrown_at_the_target():
    spider = _spider()
    front = max(spider.legs, key=spider._leg_front_factor)
    ax, ay = spider._leg_attach(front)
    fx, fy = front.foot_x, front.foot_y
    factor = spider._leg_front_factor(front)
    rest = spider._combat_leg_pose(front, ax, ay, fx, fy, factor)
    spider.begin_strike(spider.x + 300.0, spider.y)
    peak = rest[0]
    for _ in range(int(STRIKE_DURATION / DT)):
        spider._update_combat_pose(DT)
        peak = max(peak, spider._combat_leg_pose(front, ax, ay, fx, fy, factor)[0])
    assert peak - rest[0] > spider.size * 0.4, (rest, peak)


def test_the_bite_changes_nothing_that_decides_a_fight():
    spider = _spider()
    before = (spider.hp, spider.damage, spider.x, spider.y, spider.speed)
    spider.begin_strike(spider.x + 100.0, spider.y)
    for _ in range(10):
        spider._update_combat_pose(DT)
    assert (spider.hp, spider.damage, spider.x, spider.y, spider.speed) == before


# ------------------------------------------------------------- the windows

def test_dialogs_and_menus_are_wood_and_the_overlay_is_not_touched():
    sheet = wood_theme.app_qss()
    assert sheet.count("{") == sheet.count("}")
    for selector in ("QDialog QTabBar::tab", "QDialog QProgressBar", "QMenu", "QToolTip"):
        assert selector in sheet, selector
    # A bare QWidget rule would paint over the overlay, which covers every monitor.
    assert "\n        QWidget {" not in sheet and not sheet.lstrip().startswith("QWidget")
    for module in ("engine.py", "config_ui.py"):
        source = (ROOT / "src" / "desktop_bug" / "app" / module).read_text(encoding="utf-8")
        assert "wood_theme.apply_app_theme(app)" in source, module


def test_the_randomize_row_is_gone(qapp):
    from desktop_bug.app.config_ui import ConfigWindow

    window = ConfigWindow()
    try:
        texts = [label.text() for label in window.findChildren(QLabel)]
        assert not any(text.startswith("Randomize") for text in texts)
        for name in ("random_model_btn", "random_personality_btn",
                     "random_count_btn", "random_all_btn"):
            assert not hasattr(window, name), name
    finally:
        window.close()


# ------------------------------------------------------ leaving Adventure

def test_leaving_the_adventure_page_closes_adventure():
    left = []
    shell = ModeShell(QLabel("editor"), lambda: None, leave_adventure=lambda: left.append(1))
    shell.show_mode("companion")
    shell.stack.setCurrentWidget(shell.home)
    assert not left, "only leaving Adventure should close it"
    shell.show_mode("skirmish")
    shell.stack.setCurrentWidget(shell.home)            # "<- Modes"
    assert left == [1]
    shell.show_mode("skirmish")
    shell.show_mode("companion")
    assert left == [1, 1]


def test_only_an_adventure_overlay_is_stopped(monkeypatch):
    from desktop_bug.app.config_ui import ConfigWindow

    window = ConfigWindow()
    stopped = []
    monkeypatch.setattr(window, "stop_overlay", lambda: stopped.append(window._overlay_mode))
    monkeypatch.setattr(window, "_overlay_running", lambda: True)
    try:
        window._overlay_mode = "companion"
        assert window.leave_adventure() is False
        window._overlay_mode = "adventure"
        assert window.leave_adventure() is True
        assert stopped == ["adventure"]
        assert window._overlay_mode is None
    finally:
        window.close()


def test_the_inventory_tab_keeps_its_ampersand():
    source = (ROOT / "src" / "desktop_bug" / "app" / "engine.py").read_text(encoding="utf-8")
    assert '"Inventory && armor"' in source, "a single & is a shortcut marker and vanishes"
