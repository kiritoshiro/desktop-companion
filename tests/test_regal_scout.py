"""Inspect actual painted anatomy in world space, including turns and poses."""
import math

import pytest
from PyQt5.QtCore import QPointF, Qt
from PyQt5.QtGui import QImage, QPainter
from PyQt5.QtWidgets import QComboBox

from desktop_bug.app.config_ui import ConfigWindow, RANDOM_CATEGORY_ID
from desktop_bug.creature import Creature
from desktop_bug.creature.regal import RegalExpression
from support import load_pair


class AnatomyPainter:
    def __init__(self, painter):
        self.painter = painter
        self.ellipses = []

    def __getattr__(self, name):
        return getattr(self.painter, name)

    def drawEllipse(self, *args):
        if len(args) == 3 and isinstance(args[0], QPointF):
            self.ellipses.append((self.painter.worldTransform().map(args[0]), args[1], args[2]))
        self.painter.drawEllipse(*args)


@pytest.mark.parametrize("heading", [0, math.pi/2, math.pi, -math.pi/2])
@pytest.mark.parametrize("expressive", [False, True])
def test_painted_eyes_and_palps_face_forward(qapp, heading, expressive):
    c = Creature(*load_pair("regal_scout", "curious"), 800, 600, seed=17)
    c.x, c.y, c.heading = 400, 300, heading
    c.state, c.current_speed, c.startled_timer = "Idle", 0, 0
    c._initialize_legs()
    c.regal = RegalExpression(yaw=.4, pitch=.4, tilt=.12, palps=.5) if expressive else RegalExpression()
    image = QImage(800, 600, QImage.Format_ARGB32)
    image.fill(Qt.transparent)
    p = QPainter(image)
    recorder = AnatomyPainter(p)
    c._render_regal(recorder)
    p.end()

    def forward_points(radius):
        return [(point.x()-c.x)*math.cos(heading)+(point.y()-c.y)*math.sin(heading)
                for point, rx, _ in recorder.ellipses if rx == radius]

    large = forward_points(.182)
    lateral = forward_points(.08)
    dorsal = forward_points(.032) + forward_points(.05)
    palps = forward_points(.145)
    assert len(large) == len(lateral) == len(palps) == 2
    assert len(dorsal) == 4
    assert sum(lateral)/2 < sum(large)/2
    assert max(dorsal) < min(large)
    assert min(palps) > max(large), "palps must lead the face, not point toward the abdomen"


def test_both_models_remain_selectable(qapp, caplog):
    window = ConfigWindow()
    box = QComboBox()
    try:
        for category in ("jumper", RANDOM_CATEGORY_ID):
            window._fill_skin_box(box, category)
            assert box.findData("regal_fluff") >= 0
            assert box.findData("regal_scout") >= 0
        assert "Model preview failed" not in caplog.text
    finally:
        window.close()


def test_original_model_keeps_its_original_face_axis():
    original = Creature(*load_pair("regal_fluff", "curious"), 800, 600, seed=17)
    scout = Creature(*load_pair("regal_scout", "curious"), 800, 600, seed=17)
    assert original._is_regal() and not original._regal_face_forward()
    assert scout._is_regal() and scout._regal_face_forward()
    scout.state, scout.current_speed, scout.startled_timer = "Idle", 0, 0
    scout.regal = RegalExpression(cooldown=0)
    scout._update_regal(.04, scout.x+50, scout.y+50)
    assert scout.regal.gesture in {"question", "peek", "reach"}
