"""Cached woodland buildings and readable mission overlays."""
from __future__ import annotations


from PyQt5.QtCore import QPointF, QRect, QRectF, Qt
from PyQt5.QtGui import QColor, QFont, QPen, QRadialGradient

from .adventure_ui import _board, _home_area, hud_rect, short_binding
from .mission_art import building_art


def draw_buildings(painter, mission):
    painter.save()
    painter.setFont(QFont("Segoe UI", 9))
    scale = .72 if mission.area.w < 1100 or mission.area.h < 800 else 1.0
    for s in mission.sites:
        painter.drawImage(QRectF(s.x-120*scale, s.y-139*scale, 240*scale, 190*scale),
                          building_art(s.kind, s.owned))
        color = QColor("#8ce0be" if s.owned else "#efb28c")
        # The territory: a soft glow in the owner's colour, not a dashed ring.
        glow = QRadialGradient(QPointF(s.x, s.y), 92*scale)
        inner = QColor(color)
        inner.setAlpha(0)
        edge = QColor(color)
        edge.setAlpha(70)
        glow.setColorAt(0.0, inner)
        glow.setColorAt(0.78, inner)
        glow.setColorAt(0.95, edge)
        glow.setColorAt(1.0, inner)
        painter.save()
        painter.translate(s.x, s.y)
        painter.scale(1.0, 39.0 / 87.0)
        painter.translate(-s.x, -s.y)
        painter.setPen(Qt.NoPen)
        painter.setBrush(glow)
        painter.drawEllipse(QPointF(s.x, s.y), 92*scale, 92*scale)
        painter.restore()
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
