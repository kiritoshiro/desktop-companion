"""Cached woodland buildings and readable mission overlays."""
from __future__ import annotations

from functools import lru_cache
import math
import random

from PyQt5.QtCore import QPointF, QRect, QRectF, Qt
from PyQt5.QtGui import QColor, QFont, QImage, QPainter, QPainterPath, QPen, QRadialGradient

from .adventure_ui import _board, _home_area, hud_rect, short_binding


@lru_cache(maxsize=10)
def building_art(kind, owned):
    """Static detail is painted once per building/ownership pair, not per frame."""
    image = QImage(240, 190, QImage.Format_ARGB32_Premultiplied)
    image.fill(Qt.transparent)
    p = QPainter(image)
    p.setRenderHint(QPainter.Antialiasing)
    rng = random.Random(37)

    def ellipse(x, y, w, h, color, edge=None):
        p.setPen(QPen(QColor(edge), 1.5) if edge else Qt.NoPen)
        p.setBrush(QColor(color))
        p.drawEllipse(QRectF(x, y, w, h))

    ellipse(26, 129, 191, 36, "#211910")
    ellipse(34, 109, 172, 45, "#51422c", "#8c7750")
    for _ in range(65):
        x, y = rng.uniform(38, 199), rng.uniform(117, 148)
        ellipse(x, y, rng.uniform(2, 7), 2, rng.choice(["#78603c", "#95804d", "#373620"]))
    for _ in range(19):
        angle = rng.uniform(0, math.tau)
        x, y = 120+math.cos(angle)*78, 133+math.sin(angle)*16
        ellipse(x, y, 14, 8, rng.choice(["#516442", "#718151", "#3a4b32"]))
    if kind in ("home", "nest"):
        gradient = QRadialGradient(98, 80, 100)
        gradient.setColorAt(0, QColor("#a77b48" if kind == "home" else "#675577"))
        gradient.setColorAt(1, QColor("#3f3027"))
        p.setBrush(gradient)
        p.setPen(QPen(QColor("#bf9965"), 2))
        p.drawEllipse(QRectF(50, 47, 140, 99))
        for angle in range(10, 175, 17):
            x = 120 + math.cos(math.radians(angle))*62
            y = 128 - math.sin(math.radians(angle))*71
            p.setPen(QPen(QColor("#c1a47b"), 5, Qt.SolidLine, Qt.RoundCap))
            p.drawLine(QPointF(x, y), QPointF(x+6, y+13))
        ellipse(87, 87, 68, 60, "#211c1b", "#b48a59")
        ellipse(96, 101, 48, 42, "#100f14")
        for i in range(5):
            p.setPen(QPen(QColor("#e2d3a6"), .8))
            p.drawLine(QPointF(86+i*15, 91), QPointF(105+i*6, 123))
        if kind == "nest":
            for x, y in [(54, 81), (78, 44), (156, 45), (183, 87)]:
                path = QPainterPath(QPointF(x, y+20))
                path.lineTo(x-7, y-14)
                path.lineTo(x+15, y+12)
                p.fillPath(path, QColor("#bd9477"))
        else:
            for x, y in [(66, 111), (166, 117)]:
                p.setPen(QPen(QColor("#c7b99a"), 4))
                p.drawLine(x, y, x, y+22)
                ellipse(x-13, y-6, 27, 14, "#c77d4f", "#efbb74")
                ellipse(x-4, y-3, 4, 3, "#ffe3aa")
    elif kind == "silk":
        for x in (65, 171):
            p.setPen(QPen(QColor("#493329"), 13, Qt.SolidLine, Qt.RoundCap))
            p.drawLine(x, 49, x, 136)
            p.setPen(QPen(QColor("#ad8256"), 3))
            p.drawLine(x-3, 54, x-3, 130)
        p.setPen(QPen(QColor("#9d7852"), 10, Qt.SolidLine, Qt.RoundCap))
        p.drawLine(59, 51, 178, 51)
        for i in range(10):
            p.setPen(QPen(QColor("#dcebe2"), 1.2))
            p.drawLine(QPointF(72+i*10, 55), QPointF(82+i*8, 119))
            path = QPainterPath(QPointF(71, 62+i*5))
            path.quadTo(120, 95+i*4, 168, 62+i*5)
            p.drawPath(path)
        for x in (92, 120, 148):
            ellipse(x-15, 113, 30, 24, "#bedbd5", "#f3f0d4")
            for j in range(4):
                p.setPen(QPen(QColor("#eef7e7"), .9))
                p.drawArc(QRectF(x-12+j, 116, 21, 17), 20*16, 280*16)
    elif kind == "food":
        p.setPen(QPen(QColor("#785a36"), 9))
        p.drawLine(63, 80, 63, 135)
        p.drawLine(177, 80, 177, 135)
        leaf = QPainterPath(QPointF(42, 83))
        leaf.quadTo(88, 22, 126, 42)
        leaf.quadTo(168, 24, 200, 84)
        leaf.quadTo(135, 64, 42, 83)
        p.fillPath(leaf, QColor("#788850"))
        p.setPen(QPen(QColor("#c4c28a"), 1.5))
        p.drawLine(58, 78, 177, 52)
        for x, y in [(89, 110), (116, 120), (146, 111), (163, 126)]:
            ellipse(x-13, y-12, 26, 23, "#b35f47", "#e6a676")
            ellipse(x-7, y-8, 7, 4, "#efc189")
            p.setPen(QPen(QColor("#bccc9b"), 1))
            p.drawLine(x-12, y, x+12, y+3)
        p.setPen(QPen(QColor("#bb9a63"), 5))
        p.drawLine(62, 141, 179, 141)
    else:
        for x, y in [(83, 104), (110, 75), (143, 85), (161, 117), (118, 119)]:
            ellipse(x-19, y-23, 38, 47, "#a3a183", "#dfdac1")
            for j in range(4):
                p.setPen(QPen(QColor("#eee7ce"), 1))
                p.drawArc(QRectF(x-16+j, y-19, 29, 38), 45*16, 230*16)
        p.setPen(QPen(QColor("#bbbcb0"), 1))
        for x in range(50, 195, 12):
            p.drawLine(x, 145, 120, 67)
    # Ownership pennant: a silhouette as well as a distinct colour.
    p.setPen(QPen(QColor("#dbc49a"), 3))
    p.drawLine(197, 91, 197, 143)
    flag = QPainterPath(QPointF(198, 91))
    flag.lineTo(219, 96)
    flag.lineTo(198, 108)
    p.fillPath(flag, QColor("#80ccb1" if owned else "#e09a75"))
    p.end()
    return image


def draw_buildings(painter, mission):
    painter.save()
    painter.setFont(QFont("Segoe UI", 9))
    scale = .72 if mission.area.w < 1100 or mission.area.h < 800 else 1.0
    for s in mission.sites:
        painter.drawImage(QRectF(s.x-120*scale, s.y-139*scale, 240*scale, 190*scale),
                          building_art(s.kind, s.owned))
        color = QColor("#8ce0be" if s.owned else "#efb28c")
        painter.setPen(QPen(color, 1.5, Qt.DashLine))
        painter.setBrush(Qt.NoBrush)
        painter.drawEllipse(QPointF(s.x, s.y), 87*scale, 39*scale)
        label = QRectF(s.x-95, s.y+29, 190, 44)
        painter.setPen(QPen(QColor("#9a7951"), 1))
        painter.setBrush(QColor(30, 24, 20, 235))
        painter.drawRoundedRect(label, 7, 7)
        painter.setPen(QColor("#fff0cd"))
        painter.drawText(label.adjusted(4, 1, -4, -21), Qt.AlignCenter, s.name)
        if s.warning > 0:
            status = f"EMERGING IN {s.warning:.1f}s"
        elif s.contested and not s.owned:
            status = "Clear the defenders"
        elif s.owned:
            status = {"home": "Heal + refill silk", "food": f"Healing supply {s.supply:.0f}",
                      "silk": "12 silk capacity + fast refill", "hatchery": "SEALED", "nest": "CLAIMED"}[s.kind]
        elif s.kind == "hatchery":
            status = f"{s.reserves} reserves / hold to seal"
        elif s.kind == "nest":
            status = "Seal hatchery + defeat guardian"
        else:
            status = "Hold nearby to capture"
        painter.setPen(color)
        painter.drawText(label.adjusted(4, 21, -4, -1), Qt.AlignCenter, status)
        if 0 < s.progress < 1:
            painter.fillRect(QRectF(s.x-83, s.y+77, 166, 5), QColor("#31291f"))
            painter.fillRect(QRectF(s.x-83, s.y+77, 166*s.progress, 5), QColor("#eacb7f"))
    for actor in mission.actors:
        if actor.windup > 0:
            c = actor.creature
            painter.setBrush(Qt.NoBrush)
            painter.setPen(QPen(QColor("#ffb77a"), 2.5, Qt.DashLine))
            painter.drawEllipse(QPointF(c.x, c.y), c.size*2.8, c.size*2.8)
    painter.restore()


def command_rects(window):
    r = hud_rect(window)
    width = (r.width()-32)//3
    return [(command, QRect(r.x()+12+i*(width+4), r.y()+152, width, 30))
            for i, command in enumerate(("follow", "defend", "attack"))]


def mission_banner_rect(window):
    area = _home_area(window)
    width = min(640, area.width()-24)
    return QRect(area.center().x()-width//2, area.top()+16, width, 91)


def draw_mission_hud(painter, window, mission):
    painter.save()
    painter.setClipping(False)
    r = mission_banner_rect(window)
    _board(painter, r, 10)
    painter.setFont(QFont("Segoe UI", 12, QFont.Bold))
    painter.setPen(QColor("#f9df9d"))
    text = painter.fontMetrics().elidedText(mission.objective, Qt.ElideRight, r.width()-28)
    painter.drawText(r.adjusted(14, 7, -14, -57), Qt.AlignLeft | Qt.AlignVCenter, text)
    painter.setFont(QFont("Segoe UI", 9))
    painter.setPen(QColor("#f2e7d3"))
    detail = mission.notice if mission.notice_time > 0 else "Clear defenders, then stand by the entrance. Retreat home to recover."
    if mission.state != "active":
        detail = mission.save_error or ("Victory progress saved. Esc: replay or exit." if mission.state == "victory"
                                        else "Esc: retry or exit. Your Companion colony is unchanged.")
    painter.drawText(r.adjusted(14, 33, -14, -9), Qt.TextWordWrap | Qt.AlignLeft, detail)
    if window.player is not None:
        hr = hud_rect(window)
        for command, rect in command_rects(window):
            active = command == mission.command and not mission.ally.dead
            painter.setPen(QPen(QColor("#8bd9bd" if active else "#95794e"), 1.5))
            painter.setBrush(QColor("#29463d" if active else "#312820"))
            painter.drawRoundedRect(rect, 5, 5)
            painter.setPen(QColor("#f9edce"))
            key = short_binding(window.controls.binding("companion_"+command))
            painter.drawText(rect, Qt.AlignCenter, f"{key}  {command.title()}")
        painter.setPen(QColor("#e7d4ac"))
        painter.drawText(hr.adjusted(14, 186, -14, -3), Qt.AlignLeft | Qt.AlignVCenter,
                         "Scout fallen" if mission.ally.dead else f"Scout {mission.ally.hp:.0f}/{mission.ally.max_hp:.0f} HP | Defend holds here; Attack aims at foe")
    painter.restore()
