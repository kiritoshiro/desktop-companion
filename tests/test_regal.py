"""The regal jumper is selectable, expressive, and yields to gameplay."""
from dataclasses import asdict
import math

import pytest
from PyQt5.QtGui import QImage, QPainter
from PyQt5.QtCore import Qt
from PyQt5.QtWidgets import QComboBox

from desktop_bug.creature import Creature
from desktop_bug.creature.regal import RegalExpression
from desktop_bug.content.discovery import discover_models
from desktop_bug.app.config_ui import ConfigWindow, RANDOM_CATEGORY_ID
from support import ROOT, load_pair


def regal(seed=8):
    c = Creature(*load_pair("regal_fluff", "curious"), 800, 600, seed=seed)
    c.x, c.y, c.heading = 400, 300, -math.pi/2
    c.state, c.current_speed, c.startled_timer = "Idle", 0, 0
    c._initialize_legs()
    return c


def test_selectable_in_jumper_and_all_categories(qapp, caplog):
    # Exercise the actual dropdown population and real preview renderer.
    window = ConfigWindow()
    box = QComboBox()
    try:
        for category in ("jumper", RANDOM_CATEGORY_ID):
            window._fill_skin_box(box, category)
            assert box.findData("regal_fluff") >= 0
        models, warnings = discover_models(ROOT)
        assert not warnings
        assert len(models["regal_fluff"]["legs"]) == 8
        assert not window._model_icon(models["regal_fluff"]).isNull()
        assert "Model preview failed for regal_fluff" not in caplog.text
    finally:
        window.close()


def test_mouse_attention_and_bounded_pose():
    c = regal()
    c.regal = RegalExpression(cooldown=0)
    seen = set()
    for _ in range(900):
        c._update_regal(.04, 460, 200)
        r = c.regal
        seen.add(r.gesture)
        assert 0 <= r.left <= 1 and 0 <= r.right <= 1
        assert abs(r.yaw) <= .65 and abs(r.tilt) <= .2
    assert "rest" in seen and seen.intersection({"question", "peek", "reach"})
    assert c.regal.target == "mouse"
    assert (c.x, c.y) == (400, 300), "expressions must not move navigation contacts"


def test_social_attention_respects_visibility_and_relationship():
    c, other = regal(), regal(9)
    other.x += 70
    c.neighbors = [other]
    c.regal = RegalExpression(cooldown=0)
    c._update_regal(.04, 900, 900)
    assert c.regal.target == "spider"
    assert c.regal.gesture in {"greet", "dance", "question"}
    other._desktop_fully_hidden = True
    c._update_regal(.04, 900, 900)
    assert c.regal.target == "none"
    other._desktop_fully_hidden = False
    c.relation_to = lambda _: "foe"
    c._update_regal(.04, 900, 900)
    assert c.regal.target == "none"
    c.relation_to = lambda _: "friend"
    c.allow_social = False
    c._update_regal(.04, 900, 900)
    assert c.regal.target == "none"


@pytest.mark.parametrize("field,value", [
    ("dragging", True), ("webbed_timer", 3), ("dead", True),
    ("_hunting_prey", True), ("_foe", object()),
    ("player_control", object()), ("_desktop_fully_hidden", True),
    ("state", "Retreat"), ("current_speed", 90), ("startled_timer", 1),
])
def test_priority_states_cancel_displays(field, value):
    c = regal()
    c.regal = RegalExpression(gesture="dance", duration=3.4, left=.8, right=.8)
    setattr(c, field, value)
    c._update_regal(.04, 450, 250)
    assert c.regal.gesture == "rest"
    assert c.regal.left == c.regal.right == 0


def test_render_is_pure_and_poses_are_visibly_different(qapp):
    c = regal()
    c.regal = RegalExpression()

    def draw():
        image = QImage(800, 600, QImage.Format_ARGB32)
        image.fill(Qt.transparent)
        p = QPainter(image)
        p.setRenderHint(QPainter.Antialiasing)
        c.render(p)
        p.end()
        return image

    rng_before = c.rng.getstate()
    before = asdict(c.regal)
    rest = draw()
    assert draw() == rest
    assert c.rng.getstate() == rng_before and asdict(c.regal) == before
    c.regal = RegalExpression(left=.9, right=.8, tilt=.12, palps=.5, pitch=.5)
    assert draw() != rest
    assert not rest.isNull()


def test_other_species_do_not_gain_regal_state():
    c = Creature(*load_pair("velvet_regal_jumper", "curious"), 800, 600, seed=8)
    rng_before = c.rng.getstate()
    c._update_regal(.04, 200, 200)
    assert not hasattr(c, "regal")
    assert c.rng.getstate() == rng_before


def test_normal_update_loop_reaches_mouse_expressions():
    c = regal(17)
    seen = set()
    for _ in range(750):
        c.update(.04, c.x+85, c.y-95, 800, 600)
        seen.add(c.regal.gesture)
        assert all(math.isfinite(v) for leg in c.legs for v in (leg.foot_x, leg.foot_y))
    assert seen.intersection({"question", "peek", "reach"})


def test_jump_interrupts_and_resumes_after_landing():
    c = regal()
    c.regal = RegalExpression(gesture="dance", duration=3.4, left=.8, right=.8)
    c._launch_jump(405, 300, kind="hop", peak=c.size, duration=.5)
    c._update_regal(.04, 450, 250)
    assert c.regal.gesture == "rest"
    for _ in range(25):
        c._update_jump(.04)
    c.state = "Idle"
    c.startled_timer = c.current_speed = 0
    c.regal.cooldown = 0
    c._update_regal(.04, 450, 250)
    assert c.regal.gesture in {"question", "peek", "reach"}


def test_dance_alternates_forelegs_then_settles():
    c = regal()
    c.regal = RegalExpression(gesture="dance", duration=3.4)
    differences = []
    for _ in range(86):
        c._update_regal(.04, 450, 250)
        differences.append(c.regal.left-c.regal.right)
    assert min(differences) < -.1 and max(differences) > .1
    assert c.regal.gesture == "rest"
    assert c.regal.left == c.regal.right == 0
