"""Mode selection around the existing Companion settings page."""

from __future__ import annotations

from PyQt5.QtCore import Qt
from PyQt5.QtGui import QColor, QIcon, QPainter, QPen, QPixmap
from PyQt5.QtWidgets import (QFrame, QHBoxLayout, QLabel, QPushButton,
                             QStackedWidget, QVBoxLayout, QWidget)


def mode_icon(kind: str) -> QIcon:
    """Draw compact vector icons that stay crisp without bundled assets."""
    pix = QPixmap(72, 72)
    pix.fill(Qt.transparent)
    p = QPainter(pix)
    p.setRenderHint(QPainter.Antialiasing)
    p.setPen(QPen(QColor("#e9e4ff"), 3, Qt.SolidLine, Qt.RoundCap))
    if kind == "companion":
        p.setBrush(QColor("#8059c8"))
        p.drawEllipse(22, 17, 29, 35)
        for side in (-1, 1):
            for i in range(3):
                y = 26 + i * 10
                p.drawLine(36 + side * 11, y, 36 + side * 27, y + (i - 1) * 8)
        p.drawEllipse(44, 26, 3, 3)
    elif kind == "skirmish":
        p.setBrush(QColor("#b84876"))
        p.drawEllipse(18, 18, 36, 36)
        p.drawLine(36, 7, 36, 19)
        p.drawLine(36, 53, 36, 65)
        p.drawLine(7, 36, 19, 36)
        p.drawLine(53, 36, 65, 36)
        p.setBrush(QColor("#f8dfb4"))
        p.drawEllipse(30, 30, 12, 12)
    else:
        p.setBrush(QColor("#318c86"))
        p.drawEllipse(26, 5, 20, 20)
        p.drawEllipse(8, 45, 20, 20)
        p.drawEllipse(44, 45, 20, 20)
        p.drawLine(36, 25, 18, 45)
        p.drawLine(36, 25, 54, 45)
        p.drawLine(28, 55, 44, 55)
    p.end()
    return QIcon(pix)


class ModeShell(QWidget):
    def __init__(self, companion: QWidget, start_adventure, parent=None):
        super().__init__(parent)
        self.setObjectName("modeShell")
        self.setStyleSheet("""
            QWidget#modeShell { background: #171a2b; color: #f4f1ff; }
            QFrame#modeCard { background: #272b45; border: 1px solid #48516e;
                              border-radius: 16px; }
            QLabel#modeTitle { color: #ffffff; font-size: 17pt; font-weight: 700; }
            QLabel#modeText { color: #cbd2e6; font-size: 10pt; }
            QPushButton#modeChoice { background: #7560d6; color: #ffffff;
                                     border: 0; border-radius: 8px;
                                     padding: 10px 18px; font-weight: 700; }
            QPushButton#modeChoice:hover { background: #927ff0; }
            QPushButton#modeChoice:focus { border: 2px solid #f8dc8c; }
            QPushButton#modeBack { color: #e9e4ff; background: #303651;
                                   border: 1px solid #606b8d; border-radius: 8px; }
        """)
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        self.stack = QStackedWidget()
        outer.addWidget(self.stack)
        self.home = self._home()
        self.stack.addWidget(self.home)
        self.companion = self._page("Companion", "Your living desktop colony", companion)
        self.skirmish = self._adventure_page(start_adventure)
        self.strategy = self._page("Strategy", "Colony command is being developed.",
                                   QLabel("Build a base, direct squads and conquer rival territory. Coming later."))
        for page in (self.companion, self.skirmish, self.strategy):
            self.stack.addWidget(page)
        self.stack.setCurrentWidget(self.home)

    def show_mode(self, mode: str):
        self.stack.setCurrentWidget(getattr(self, mode))

    def _home(self):
        page = QWidget()
        layout = QVBoxLayout(page)
        layout.setContentsMargins(36, 38, 36, 38)
        layout.setSpacing(20)
        title = QLabel("Desktop Companion")
        title.setObjectName("modeTitle")
        title.setStyleSheet("font-size: 28pt;")
        layout.addWidget(title)
        sub = QLabel("Choose how you want to spend time with your spiders.")
        sub.setObjectName("modeText")
        layout.addWidget(sub)
        cards = QHBoxLayout()
        cards.setSpacing(18)
        for kind, title, description in (
            ("companion", "Companion", "Watch your spiders live on the desktop and manage their colony."),
            ("skirmish", "Skirmish", "Mission maps are coming. Try direct spider control now."),
            ("strategy", "Strategy", "Command a colony against rivals. Preview only."),
        ):
            card = QFrame()
            card.setObjectName("modeCard")
            card.setMaximumWidth(260)
            cell = QVBoxLayout(card)
            cell.setContentsMargins(20, 22, 20, 22)
            icon = QLabel()
            icon.setPixmap(mode_icon(kind).pixmap(72, 72))
            cell.addWidget(icon, alignment=Qt.AlignLeft)
            heading = QLabel(title)
            heading.setObjectName("modeTitle")
            cell.addWidget(heading)
            desc = QLabel(description)
            desc.setWordWrap(True)
            desc.setMaximumWidth(210)
            desc.setObjectName("modeText")
            cell.addWidget(desc)
            cell.addStretch()
            button = QPushButton("Open " + title)
            button.setObjectName("modeChoice")
            button.clicked.connect(lambda _=False, value=kind: self.show_mode(value))
            cell.addWidget(button)
            cards.addWidget(card, 1)
        layout.addLayout(cards, 1)
        return page

    def _page(self, title, subtitle, body):
        page = QWidget()
        layout = QVBoxLayout(page)
        layout.setContentsMargins(16, 12, 16, 12)
        header = QHBoxLayout()
        back = QPushButton("← Modes")
        back.setObjectName("modeBack")
        back.clicked.connect(lambda: self.stack.setCurrentWidget(self.home))
        header.addWidget(back)
        label = QLabel(title + "  ·  " + subtitle)
        label.setObjectName("modeTitle")
        label.setWordWrap(True)
        label.setMaximumWidth(720)
        header.addWidget(label, 1)
        layout.addLayout(header)
        if isinstance(body, QLabel):
            body.setObjectName("modeText")
            body.setWordWrap(True)
            body.setMaximumWidth(640)
        layout.addWidget(body, 1)
        return page

    def _adventure_page(self, start_adventure):
        panel = QWidget()
        layout = QVBoxLayout(panel)
        layout.setContentsMargins(34, 28, 34, 28)
        layout.setSpacing(20)
        title = QLabel("Adventure control prototype")
        title.setObjectName("modeTitle")
        layout.addWidget(title)
        for text in (
            "Control one spider from your current preset using its saved level, health and energy.",
            "WASD move   ·   Shift sprint   ·   Space jump   ·   Mouse aim   ·   Click shoot silk",
            "K opens the existing skill tree. Esc pauses, saves or releases control.",
            "Skirmish maps, objectives and conquest are planned for this view.",
        ):
            label = QLabel(text)
            label.setObjectName("modeText")
            label.setWordWrap(True)
            layout.addWidget(label)
        launch = QPushButton("Start Adventure")
        launch.setObjectName("modeChoice")
        launch.setMinimumHeight(48)
        launch.clicked.connect(start_adventure)
        layout.addWidget(launch)
        layout.addStretch()
        self.adventure_launch = launch
        return self._page("Skirmish", "Adventure control preview", panel)
