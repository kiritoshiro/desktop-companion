"""Mode selection around the existing Companion settings page.

Three modes: Companion (the living desktop colony), Adventure (take control
of one spider; skirmish missions grow here) and Strategy (colony command,
later). Dressed in the carved-wood theme from ``wood_theme``.
"""

from __future__ import annotations

from PyQt5.QtCore import Qt
from PyQt5.QtGui import QColor
from PyQt5.QtWidgets import (QFrame, QGraphicsDropShadowEffect, QHBoxLayout, QLabel,
                             QPushButton, QScrollArea, QStackedWidget, QVBoxLayout, QWidget)

from . import wood_theme

MODES = (
    ("companion", "Companion",
     "Your spiders live on the desktop: they hunt flies, spin webs, build bases "
     "and grow. Manage the colony."),
    ("skirmish", "Adventure",
     "Take control of one spider. Hunt, fight and level up with every skill it "
     "has. Skirmish missions grow here."),
    ("strategy", "Strategy",
     "Command a colony against rival teams: build bases, direct squads, take "
     "territory. Coming later."),
)


def _engrave(label: QLabel, light: bool = True) -> QLabel:
    """A soft lip under the letters, as if cut into the board."""
    effect = QGraphicsDropShadowEffect(label)
    effect.setOffset(0, 1.5)
    effect.setBlurRadius(2)
    effect.setColor(QColor(255, 226, 170, 150) if light else QColor(20, 9, 2, 200))
    label.setGraphicsEffect(effect)
    return label


def _art(kind: str, width: int) -> QLabel:
    label = QLabel()
    label.setObjectName("modeArt")
    label.setAlignment(Qt.AlignCenter)
    pixmap = wood_theme.mode_art(kind)
    if pixmap is not None:
        label.setPixmap(pixmap.scaledToWidth(width, Qt.SmoothTransformation))
    return label


class ModeShell(QWidget):
    def __init__(self, companion: QWidget, start_adventure, parent=None):
        super().__init__(parent)
        self.setObjectName("modeShell")
        # A QWidget subclass only paints a style-sheet background when asked.
        self.setAttribute(Qt.WA_StyledBackground, True)
        self.setStyleSheet(wood_theme.shell_qss())
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        self.stack = QStackedWidget()
        outer.addWidget(self.stack)
        self.home = self._home()
        self.stack.addWidget(self.home)
        self.companion = self._page("Companion", "Your living desktop colony", companion)
        self.skirmish = self._adventure_page(start_adventure)
        self.strategy = self._strategy_page()
        for page in (self.companion, self.skirmish, self.strategy):
            self.stack.addWidget(page)
        self.stack.setCurrentWidget(self.home)

    def show_mode(self, mode: str):
        self.stack.setCurrentWidget(getattr(self, mode))

    def _home(self):
        page = QWidget()
        layout = QVBoxLayout(page)
        layout.setContentsMargins(36, 30, 36, 34)
        layout.setSpacing(8)
        layout.addStretch(1)
        title = QLabel("Desktop Companion")
        title.setObjectName("modeHeading")
        title.setAlignment(Qt.AlignCenter)
        layout.addWidget(_engrave(title))
        sub = QLabel("Choose how to play")
        sub.setObjectName("modeSubheading")
        sub.setAlignment(Qt.AlignCenter)
        layout.addWidget(_engrave(sub))
        layout.addSpacing(14)
        cards = QHBoxLayout()
        cards.setSpacing(18)
        self.mode_buttons = {}
        for kind, title_text, description in MODES:
            card = QFrame()
            card.setObjectName("modeCard")
            card.setMinimumWidth(250)
            card.setMaximumWidth(340)
            cell = QVBoxLayout(card)
            cell.setContentsMargins(0, 0, 0, 0)
            cell.setSpacing(10)
            cell.addWidget(_art(kind, 250))
            heading = QLabel(title_text)
            heading.setObjectName("modeTitle")
            cell.addWidget(_engrave(heading, light=False))
            desc = QLabel(description)
            desc.setWordWrap(True)
            desc.setObjectName("modeText")
            cell.addWidget(desc)
            cell.addStretch()
            button = QPushButton("Preview" if kind == "strategy" else "Play " + title_text)
            button.setObjectName("modeChoice")
            button.setCursor(Qt.PointingHandCursor)
            button.clicked.connect(lambda _=False, value=kind: self.show_mode(value))
            cell.addWidget(button)
            self.mode_buttons[kind] = button
            cards.addWidget(card, 1)
        layout.addLayout(cards)
        layout.addStretch(1)
        return page

    def _page(self, title, subtitle, body):
        page = QWidget()
        layout = QVBoxLayout(page)
        layout.setContentsMargins(16, 12, 16, 12)
        bar = QFrame()
        bar.setObjectName("pageHeader")
        header = QHBoxLayout(bar)
        header.setContentsMargins(10, 6, 14, 6)
        back = QPushButton("← Modes")
        back.setObjectName("modeBack")
        back.setCursor(Qt.PointingHandCursor)
        back.clicked.connect(lambda: self.stack.setCurrentWidget(self.home))
        header.addWidget(back)
        label = QLabel(title + "  ·  " + subtitle)
        label.setObjectName("modeTitle")
        label.setWordWrap(True)
        header.addWidget(_engrave(label, light=False), 1)
        layout.addWidget(bar)
        if isinstance(body, QLabel):
            body.setObjectName("modeText")
            body.setWordWrap(True)
        layout.addWidget(body, 1)
        return page

    def _plaque(self, kind: str, lines, button: QPushButton | None = None) -> QWidget:
        """A picture, and a carved plaque of text below it."""
        panel = QWidget()
        layout = QVBoxLayout(panel)
        layout.setContentsMargins(24, 16, 24, 16)
        layout.setSpacing(14)
        layout.addWidget(_art(kind, 520), alignment=Qt.AlignHCenter)
        plaque = QFrame()
        plaque.setObjectName("modePlaque")
        plaque.setFixedWidth(620)
        text = QVBoxLayout(plaque)
        text.setContentsMargins(0, 0, 0, 0)
        text.setSpacing(8)
        for line in lines:
            label = QLabel(line)
            label.setObjectName("modeText")
            label.setWordWrap(True)
            text.addWidget(label)
        if button is not None:
            text.addSpacing(6)
            text.addWidget(button)
        layout.addWidget(plaque, alignment=Qt.AlignHCenter)
        layout.addStretch()
        # Scrolls rather than squeezes when the window is short.
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.NoFrame)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        scroll.setStyleSheet("QScrollArea, QScrollArea > QWidget > QWidget { background: transparent; }")
        scroll.setWidget(panel)
        return scroll

    def _adventure_page(self, start_adventure):
        launch = QPushButton("Start Adventure")
        launch.setObjectName("modeChoice")
        launch.setMinimumHeight(46)
        launch.setCursor(Qt.PointingHandCursor)
        launch.clicked.connect(start_adventure)
        self.adventure_launch = launch
        panel = self._plaque("skirmish", (
            "Take control of one spider from your current preset, with its saved "
            "level, health and energy. Everything it earns stays with it.",
            "<b>WASD</b> move  ·  <b>Shift</b> sprint  ·  <b>Space</b> jump  ·  "
            "<b>Mouse</b> aim  ·  <b>Left click</b> shoot silk  ·  "
            "<b>Right click</b> bite",
            "<b>K</b> skill tree  ·  <b>Esc</b> pause, save or release the spider",
            "Skirmish maps, objectives and conquest are on the way.",
        ), launch)
        return self._page("Adventure", "Play as your spider", panel)

    def _strategy_page(self):
        panel = self._plaque("strategy", (
            "Build a base, direct squads and take rival territory.",
            "Colony command is being developed. Coming later.",
        ))
        return self._page("Strategy", "Colony command", panel)
