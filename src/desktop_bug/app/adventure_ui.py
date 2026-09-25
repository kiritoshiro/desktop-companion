"""Adventure HUD and pause controls for the transparent game window.

Carved-wood look (``wood_theme``): a walnut board with a raised rim, brass
lettering, bars cut into the wood and ability slots as recessed wells.
"""

from __future__ import annotations

from PyQt5.QtCore import QPointF, QRect, QRectF, Qt
from PyQt5.QtGui import QBrush, QColor, QLinearGradient, QPainterPath, QPen
from PyQt5.QtWidgets import QComboBox, QDialog, QHBoxLayout, QLabel, QPushButton, QVBoxLayout

from . import wood_theme


HUD_WIDTH = 430
HUD_HEIGHT = 210

_BRASS = QColor(wood_theme.BRASS)
_CREAM = QColor(wood_theme.CREAM)
_DIM = QColor(150, 124, 92)


def _home_area(window) -> QRect:
    """Where the HUD sits by default: the main monitor, in overlay pixels.

    The overlay spans every monitor, so "the middle of the window" is the
    seam between two screens. Falls back to the whole window when the main
    monitor is unknown or not inside it (and in tests that fake a window).
    """
    whole = QRect(0, 0, window.width(), window.height())
    origin = getattr(window, "geometry_rect", None)
    if origin is None:
        return whole
    from .window_placement import primary_rect_local

    area = primary_rect_local(origin.topLeft()).intersected(whole)
    return area if area.width() >= 200 and area.height() >= 200 else whole


def hud_rect(window):
    width = min(HUD_WIDTH, max(1, window.width() - 16))
    height = min(HUD_HEIGHT, max(1, window.height() - 16))
    position = getattr(window, "_adventure_hud_position", None)
    if position is None:
        area = _home_area(window)
        return QRect(max(8, area.left() + (area.width() - width) // 2),
                     max(8, area.bottom() - height - 14), width, height)
    return QRect(max(8, min(position.x(), window.width() - width - 8)),
                 max(8, min(position.y(), window.height() - height - 8)),
                 width, height)


def _board(painter, rect: QRect, radius: float) -> None:
    """Walnut with a raised, lit rim and a shadow under it."""
    outer = QRectF(rect)
    path = QPainterPath()
    path.addRoundedRect(outer, radius, radius)
    painter.setPen(Qt.NoPen)
    for i in range(4, 0, -1):
        painter.setBrush(QColor(10, 4, 0, 30))
        painter.drawPath(path.translated(i * 0.6, i * 1.0))
    texture = wood_theme.texture("wood_dark_tile.png")
    painter.setBrush(QBrush(texture) if texture is not None else QColor(wood_theme.WALNUT))
    painter.drawPath(path)
    painter.setBrush(QColor(20, 10, 3, 40))
    painter.drawPath(path)
    painter.setBrush(Qt.NoBrush)
    painter.setPen(QPen(QColor(210, 160, 100, 120), 1.4))
    painter.drawRoundedRect(outer.adjusted(1.5, 1.5, -1.5, -1.5), radius - 1, radius - 1)
    painter.setPen(QPen(QColor(18, 8, 2, 230), 2.0))
    painter.drawRoundedRect(outer, radius, radius)
    painter.setPen(QPen(QColor(18, 8, 2, 150), 1.2))
    painter.drawRoundedRect(outer.adjusted(5, 5, -5, -5), radius - 4, radius - 4)


def _well(painter, rect: QRect, radius: float, lit: bool = True) -> None:
    """A recess cut into the board: dark floor, shadowed top, lit bottom lip."""
    r = QRectF(rect)
    painter.setPen(Qt.NoPen)
    painter.setBrush(QColor(14, 6, 1, 150 if lit else 185))
    painter.drawRoundedRect(r, radius, radius)
    painter.setPen(QPen(QColor(8, 3, 0, 200), 1.6))
    painter.drawLine(r.topLeft() + QPointF(radius, 0.8), r.topRight() + QPointF(-radius, 0.8))
    painter.setPen(QPen(QColor(220, 170, 110, 90), 1.2))
    painter.drawLine(r.bottomLeft() + QPointF(radius, 0.6), r.bottomRight() + QPointF(-radius, 0.6))


def short_binding(name: str) -> str:
    """A binding as it fits on an ability slot: "Mouse Left" -> "L CLICK"."""
    mouse = {"Mouse Left": "L CLICK", "Mouse Right": "R CLICK", "Mouse Middle": "M CLICK",
             "Mouse Back": "M4", "Mouse Forward": "M5"}
    return mouse.get(name, name.upper())


def draw_hud(painter, window, controller):
    spider = controller.creature
    rect = hud_rect(window)
    painter.save()
    painter.setClipping(False)
    painter.setRenderHint(painter.Antialiasing, True)
    _board(painter, rect, 14)

    painter.setPen(_BRASS)
    body_font = painter.font()
    title_font = spider._label_font()
    painter.setFont(title_font)
    title = f"{spider.display_name}  ·  Level {spider.level}"
    title = painter.fontMetrics().elidedText(title, Qt.ElideRight, max(1, rect.width() - 64))
    title_rect = QRect(rect.left() + 16, rect.top() + 5, rect.width() - 64, 26)
    painter.setPen(QColor(10, 4, 0, 200))
    painter.drawText(title_rect.translated(0, 1), Qt.AlignLeft | Qt.AlignVCenter, title)
    painter.setPen(_BRASS)
    painter.drawText(title_rect, Qt.AlignLeft | Qt.AlignVCenter, title)
    painter.setFont(body_font)
    # The drag handle: three grooves.
    for y in (13, 18, 23):
        painter.setPen(QPen(QColor(8, 3, 0, 220), 2, Qt.SolidLine, Qt.RoundCap))
        painter.drawLine(rect.right() - 31, rect.top() + y, rect.right() - 16, rect.top() + y)
        painter.setPen(QPen(QColor(210, 160, 100, 110), 1, Qt.SolidLine, Qt.RoundCap))
        painter.drawLine(QPointF(rect.right() - 31, rect.top() + y + 1.5),
                         QPointF(rect.right() - 16, rect.top() + y + 1.5))

    def bar(y, label, value, maximum, color):
        x = rect.left() + 16
        width = rect.width() - 32
        painter.setPen(_CREAM)
        painter.drawText(x, y, f"{label}  {value:.0f}/{maximum:.0f}")
        track = QRect(x, y + 4, width, 9)
        _well(painter, track, 4)
        fraction = max(0.0, min(1.0, value / max(1, maximum)))
        if fraction > 0.0:
            fill = QRectF(x + 1.5, y + 5.5, max(2.0, (width - 3) * fraction), 6)
            grad = QLinearGradient(fill.topLeft(), fill.bottomLeft())
            base = QColor(color)
            grad.setColorAt(0.0, base.lighter(135))
            grad.setColorAt(1.0, base.darker(125))
            painter.setPen(Qt.NoPen)
            painter.setBrush(QBrush(grad))
            painter.drawRoundedRect(fill, 3, 3)

    bar(rect.top() + 44, "HEALTH", spider.hp, spider.max_hp, wood_theme.HEALTH)
    bar(rect.top() + 68, "STAMINA", spider.energy, spider.max_energy, wood_theme.STAMINA)
    bar(rect.top() + 92, "SILK", controller.silk, controller.silk_capacity, "#c6dfd6")
    controls = getattr(controller, "controls", None)

    def key(action, default):
        return short_binding(controls.binding(action)) if controls is not None else default

    slots = (
        ("jump", key("jump", "SPACE"), "Jump", controller.jump_cooldown,
         spider.energy >= controller.JUMP_ENERGY),
        ("web", key("shoot", "L CLICK"), "Web", controller.web_cooldown,
         spider.energy >= controller.WEB_ENERGY and controller.silk >= 1),
        ("bite", key("bite", "R CLICK"), "Bite", spider.attack_cooldown, True),
        ("skills", key("skills", "K"), "Skills", 0.0, True),
    )
    card_width = (rect.width() - 38) // 4
    for index, (kind, key, label, cooldown, enough_energy) in enumerate(slots):
        card = QRect(rect.left() + 12 + index * (card_width + 5),
                     rect.top() + 108, card_width, 40)
        ready = cooldown <= 0.0 and enough_energy
        _well(painter, card, 7, lit=ready)
        if ready:
            painter.setPen(QPen(QColor(214, 164, 72, 150), 1.2))
            painter.setBrush(Qt.NoBrush)
            painter.drawRoundedRect(QRectF(card).adjusted(0.5, 0.5, -0.5, -0.5), 7, 7)
        color = _BRASS if ready else _DIM
        painter.setPen(QPen(color, 2))
        painter.setBrush(Qt.NoBrush)
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
        painter.setPen(_CREAM if ready else _DIM)
        painter.drawText(card.left() + 33, card.top() + 16, key)
        painter.setPen(color)
        painter.drawText(card.left() + 33, card.top() + 31,
                         f"{cooldown:.1f}s" if cooldown > 0.0 else label)
    if controller.feedback_time > 0:
        painter.setPen(QColor("#ffe2a5"))
        painter.drawText(rect.adjusted(12, -25, -12, -rect.height()), Qt.AlignCenter, controller.feedback)
    painter.restore()


class PauseDialog(QDialog):
    def __init__(self, parent):
        super().__init__(parent)
        self.setWindowTitle("Adventure paused")
        self.setMinimumWidth(300)
        self.choice = "resume"
        self.setStyleSheet(wood_theme.dialog_qss())
        layout = QVBoxLayout(self)
        layout.setContentsMargins(22, 18, 22, 20)
        layout.setSpacing(8)
        title = QLabel("Adventure paused")
        title.setObjectName("dialogTitle")
        title.setAlignment(Qt.AlignCenter)
        layout.addWidget(title)
        layout.addSpacing(4)
        mission = getattr(parent, "mission", None)
        choices = (
            ("Resume", "resume"),
            ("Skill tree", "skills"),
            ("Save", "save"),
            ("Settings", "settings"),
            ("Release spider", "release"),
            ("Save and exit", "exit"),
        )
        if mission is not None:
            choices = (("Resume", "resume"), ("Skill tree", "skills"),
                       ("Settings", "settings"), ("Restart raid", "restart"),
                       ("Retry saving victory", "save"), ("Exit raid", "exit"))
        for label, choice in choices:
            button = QPushButton(label)
            button.setCursor(Qt.PointingHandCursor)
            button.clicked.connect(lambda _=False, value=choice: self._choose(value))
            layout.addWidget(button)

    def _choose(self, value):
        self.choice = value
        self.accept()


class AdventureSettingsDialog(QDialog):
    def __init__(self, window):
        super().__init__(window)
        from .controls_ui import ControlsEditor, controls_qss

        self.setWindowTitle("Adventure settings")
        self.setMinimumWidth(640)
        self.setStyleSheet(wood_theme.dialog_qss() + controls_qss())
        layout = QVBoxLayout(self)
        layout.setContentsMargins(22, 18, 22, 20)
        title = QLabel("Adventure settings")
        title.setObjectName("dialogTitle")
        layout.addWidget(title)
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
        heading = QLabel("Controls")
        heading.setObjectName("dialogTitle")
        layout.addWidget(heading)
        self.controls = ControlsEditor()
        layout.addWidget(self.controls)
        note = QLabel("Drag the status panel to move it.")
        note.setObjectName("controlsHint")
        layout.addWidget(note)
        done = QPushButton("Apply and return")
        done.clicked.connect(self.accept)
        layout.addWidget(done)
