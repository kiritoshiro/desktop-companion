"""Render a reproducible pose sheet and animated preview; no live desktop input."""
import argparse
import json
import math
import os
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="regal-preview-"))

from PyQt5.QtCore import Qt, QRectF  # noqa: E402
from PyQt5.QtGui import QImage, QPainter, QColor, QFont, QFontDatabase  # noqa: E402
from PyQt5.QtWidgets import QApplication  # noqa: E402
from desktop_bug.content.discovery import discover_models  # noqa: E402
from desktop_bug.creature import Creature  # noqa: E402
from desktop_bug.creature.regal import RegalExpression  # noqa: E402


def creature(model_id="regal_fluff"):
    models, _ = discover_models(ROOT)
    personality = json.loads((ROOT / "personalities/curious.json").read_text())
    c = Creature(models[model_id], personality, 1000, 700, seed=17)
    c.heading = math.pi / 2 if c._regal_face_forward() else -math.pi / 2
    c.state = "Idle"
    c.current_speed = c.startled_timer = 0
    return c


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    parser.add_argument("--frames", type=Path, help="Optional PNG animation frames")
    parser.add_argument("--model", choices=("regal_fluff", "regal_scout"), default="regal_fluff")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    app = QApplication.instance() or QApplication([])
    font = Path(os.environ.get("WINDIR", "C:/Windows")) / "Fonts/segoeui.ttf"
    if font.exists():
        QFontDatabase.addApplicationFont(str(font))
    sheet = QImage(1200, 820, QImage.Format_ARGB32)
    sheet.fill(QColor("#eee9df"))
    p = QPainter(sheet)
    p.setRenderHint(QPainter.Antialiasing)
    p.setPen(QColor("#283038"))
    p.setFont(QFont("Segoe UI", 27, QFont.Bold))
    p.drawText(45, 58, args.model.replace("_", " ").upper())
    p.setFont(QFont("Segoe UI", 12))
    p.drawText(46, 87, "Phidippus regius  /  one curious desktop companion")
    poses = [("Watching", "rest", 0), ("A little question", "question", 1.15),
             ("Hello, neighbour", "greet", 1.2), ("Sideways peek", "peek", 1.25),
             ("Tiny dance", "dance", 1.6), ("Cleaning the palps", "groom", 1.1)]
    for i, (label, gesture, age) in enumerate(poses):
        x, y = 200 + i % 3 * 400, 245 + i // 3 * 310
        c = creature(args.model)
        c.size, c.x, c.y = 67, x, y
        c._initialize_legs()
        c.regal = RegalExpression(gesture=gesture, duration=3.4 if gesture == "dance" else 2.5,
                                  age=age, yaw=.35 if i % 2 else -.2, pitch=.4)
        if gesture != "rest":
            c._update_regal(.001, x+65, y+90*math.sin(c.heading))
        c._render_regal(p)
        p.setBrush(Qt.NoBrush)
        p.setPen(QColor("#39434b"))
        p.setFont(QFont("Segoe UI", 12))
        p.drawText(QRectF(x-170, y+130, 340, 30), Qt.AlignCenter, label)
    p.setFont(QFont("Segoe UI", 10))
    p.drawText(45, 790, "Enlarged pose study · procedural rig · six supporting legs during foreleg displays")
    p.end()
    sheet.save(str(args.output / (args.model.replace("_", "-") + "-poses.png")))
    if args.frames is None:
        return
    args.frames.mkdir(parents=True, exist_ok=True)
    c = creature(args.model)
    c.size, c.x, c.y = 55, 230, 185
    c._initialize_legs()
    for frame in range(150):
        t = frame / 25
        c._update_regal(.04, c.x+100*math.sin(t*1.5), c.y+95*math.sin(c.heading))
        img = QImage(460, 360, QImage.Format_RGBA8888)
        img.fill(QColor("#eee9df"))
        painter = QPainter(img)
        painter.setRenderHint(QPainter.Antialiasing)
        c._render_regal(painter)
        painter.setPen(QColor("#39434b"))
        painter.setFont(QFont("Segoe UI", 12))
        painter.drawText(20, 30, args.model.replace("_", " ").title() + " · mouse attention")
        painter.end()
        img.save(str(args.frames / f"{frame:03d}.png"))
    # Keep the QApplication alive through the painter/image teardown.
    del app


if __name__ == "__main__":
    main()
