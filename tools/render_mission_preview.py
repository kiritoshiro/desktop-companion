"""Render the real mission scene without opening or controlling the desktop."""
import os
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace

os.environ.setdefault("QT_QPA_PLATFORM", "windows")
os.environ.setdefault("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="mission-preview-"))
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from PyQt5.QtGui import QColor, QImage, QPainter  # noqa: E402
from PyQt5.QtWidgets import QApplication  # noqa: E402
from desktop_bug.app.adventure_ui import draw_hud  # noqa: E402
from desktop_bug.app.controls import ControlSettings  # noqa: E402
from desktop_bug.app.mission import TerritoryMission  # noqa: E402
from desktop_bug.app.mission_ui import draw_buildings, draw_mission_hud  # noqa: E402
from desktop_bug.manager import CreatureManager  # noqa: E402


def render(width=1600, height=1000):
    app = QApplication.instance() or QApplication([])
    manager = CreatureManager(ROOT / "presets/colony.json", width, height, seed=4)
    mission = TerritoryMission(manager, ControlSettings())
    for _ in range(10):
        mission.update(1/60)
    image = QImage(width, height, QImage.Format_ARGB32_Premultiplied)
    image.fill(QColor("#263735"))
    p = QPainter(image)
    p.setRenderHint(QPainter.Antialiasing)
    window = SimpleNamespace(width=lambda: width, height=lambda: height,
                             player=mission.player, controls=mission.controls,
                             _adventure_hud_position=None, geometry_rect=None)
    draw_buildings(p, mission)
    manager.render(p)
    draw_hud(p, window, mission.player)
    draw_mission_hud(p, window, mission)
    p.end()
    output = ROOT / "reports" / f"mission-preview-{width}.png"
    output.parent.mkdir(exist_ok=True)
    image.save(str(output))
    assert app is not None
    print(output)


if __name__ == "__main__":
    render()
    render(900, 700)
