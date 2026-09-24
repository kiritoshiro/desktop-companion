"""Adventure HUD and pause controls for the transparent game window."""

from __future__ import annotations

from PyQt5.QtCore import QRect
from PyQt5.QtGui import QColor, QPen
from PyQt5.QtWidgets import QComboBox, QDialog, QHBoxLayout, QLabel, QPushButton, QVBoxLayout


HUD_WIDTH = 430
HUD_HEIGHT = 134


def hud_rect(window):
    return QRect(max(8, (window.width() - HUD_WIDTH) // 2),
                 max(8, window.height() - HUD_HEIGHT - 14),
                 min(HUD_WIDTH, window.width() - 16), HUD_HEIGHT)


def draw_hud(painter, window, controller):
    spider = controller.creature
    rect = hud_rect(window)
    painter.save()
    painter.setClipping(False)
    painter.setPen(QPen(QColor("#8994b7"), 1))
    painter.setBrush(QColor(19, 23, 38, 230))
    painter.drawRoundedRect(rect, 14, 14)
    painter.setPen(QColor("#f7f5ff"))
    painter.drawText(rect.left() + 16, rect.top() + 22,
                     f"{spider.display_name}  ·  Level {spider.level}")

    def bar(y, label, value, maximum, color):
        x = rect.left() + 16
        width = rect.width() - 32
        painter.setPen(QColor("#dbe0ef"))
        painter.drawText(x, y, f"{label}  {value:.0f}/{maximum:.0f}")
        fill = QRect(x, y + 5, width, 7)
        painter.fillRect(fill, QColor("#414961"))
        painter.fillRect(QRect(x, y + 5, int(width * max(0, min(1, value / max(1, maximum)))), 7), QColor(color))

    bar(rect.top() + 42, "HEALTH", spider.hp, spider.max_hp, "#e56e83")
    bar(rect.top() + 66, "STAMINA", spider.energy, spider.max_energy, "#5ad1bd")
    slots = (
        ("jump", "SPACE", "Jump", controller.jump_cooldown, spider.energy >= controller.JUMP_ENERGY),
        ("web", "L CLICK", "Web", controller.web_cooldown, spider.energy >= controller.WEB_ENERGY),
        ("bite", "R CLICK", "Bite", spider.attack_cooldown, True),
        ("skills", "K", "Skills", 0.0, True),
    )
    card_width = (rect.width() - 38) // 4
    for index, (kind, key, label, cooldown, enough_energy) in enumerate(slots):
        card = QRect(rect.left() + 12 + index * (card_width + 5),
                     rect.top() + 84, card_width, 40)
        ready = cooldown <= 0.0 and enough_energy
        painter.setPen(QPen(QColor("#8e9bbd" if ready else "#566077"), 1))
        painter.setBrush(QColor("#343d60" if ready else "#293147"))
        painter.drawRoundedRect(card, 7, 7)
        color = QColor("#f4dda0" if ready else "#8f9aaa")
        painter.setPen(QPen(color, 2))
        cx, cy = card.left() + 17, card.top() + 19
        if kind == "jump":
            painter.drawLine(cx - 8, cy + 7, cx, cy - 6)
            painter.drawLine(cx, cy - 6, cx + 8, cy + 7)
            painter.drawLine(cx - 6, cy + 7, cx + 6, cy + 7)
        elif kind == "web":
            painter.drawEllipse(cx - 8, cy - 8, 16, 16)
            for x1, y1, x2, y2 in ((-10, 0, 10, 0), (0, -10, 0, 10),
                                   (-7, -7, 7, 7), (-7, 7, 7, -7)):
                painter.drawLine(cx + x1, cy + y1, cx + x2, cy + y2)
        elif kind == "bite":
            painter.drawLine(cx - 9, cy - 7, cx - 4, cy + 7)
            painter.drawLine(cx - 4, cy + 7, cx, cy - 3)
            painter.drawLine(cx + 9, cy - 7, cx + 4, cy + 7)
            painter.drawLine(cx + 4, cy + 7, cx, cy - 3)
        else:
            painter.drawLine(cx, cy - 7, cx - 7, cy + 6)
            painter.drawLine(cx, cy - 7, cx + 7, cy + 6)
            for px, py in ((cx, cy - 7), (cx - 7, cy + 6), (cx + 7, cy + 6)):
                painter.drawEllipse(px - 2, py - 2, 4, 4)
        painter.setPen(color)
        painter.drawText(card.left() + 33, card.top() + 16, key)
        painter.drawText(card.left() + 33, card.top() + 31,
                         f"{cooldown:.1f}s" if cooldown > 0.0 else label)
    painter.restore()


class PauseDialog(QDialog):
    def __init__(self, parent):
        super().__init__(parent)
        self.setWindowTitle("Adventure paused")
        self.setMinimumWidth(300)
        self.choice = "resume"
        self.setStyleSheet("""
            QDialog { background: #20253c; color: #f6f3ff; }
            QLabel { color: #f6f3ff; font-size: 15pt; font-weight: bold; }
            QPushButton { min-height: 36px; border-radius: 7px;
                          background: #353e62; color: white; }
            QPushButton:hover { background: #5968a3; }
        """)
        layout = QVBoxLayout(self)
        layout.addWidget(QLabel("Adventure paused"))
        for label, choice in (
            ("Resume", "resume"),
            ("Skill tree", "skills"),
            ("Save", "save"),
            ("Settings", "settings"),
            ("Release spider", "release"),
            ("Save and exit", "exit"),
        ):
            button = QPushButton(label)
            button.clicked.connect(lambda _=False, value=choice: self._choose(value))
            layout.addWidget(button)

    def _choose(self, value):
        self.choice = value
        self.accept()


class AdventureSettingsDialog(QDialog):
    def __init__(self, window):
        super().__init__(window)
        self.setWindowTitle("Adventure settings")
        layout = QVBoxLayout(self)
        layout.addWidget(QLabel("Frame rate"))
        row = QHBoxLayout()
        self.fps = QComboBox()
        for value in (30, 45, 60):
            self.fps.addItem(f"{value} FPS", value)
        current = min(range(self.fps.count()),
                      key=lambda index: abs(self.fps.itemData(index) - window.frame_policy.target_fps))
        self.fps.setCurrentIndex(current)
        row.addWidget(self.fps)
        layout.addLayout(row)
        controls = QLabel("WASD move · Shift sprint · Space jump · Click shoot · K skills · Esc pause")
        controls.setWordWrap(True)
        layout.addWidget(controls)
        done = QPushButton("Apply and return")
        done.clicked.connect(self.accept)
        layout.addWidget(done)
