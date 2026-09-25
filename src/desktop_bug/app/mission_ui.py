"""Cached woodland buildings and readable mission overlays."""
from __future__ import annotations


from PyQt5.QtCore import QPointF, QRect, QRectF, Qt
from PyQt5.QtGui import QColor, QFont, QPainterPath, QPen, QRadialGradient

import math

from .adventure_ui import _board, _home_area, hud_rect, short_binding
from .mission_art import building_art
from .stat_text import TIER_COLORS
from ..creature.armour_art import armour_icon
from ..state.progression import ARMOR_BY_ID


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
    draw_loot(painter, mission)


def draw_loot(painter, mission):
    """Dropped armour: its picture, bobbing in a glow of its quality's colour."""
    if not mission.loot:
        return
    painter.save()
    for loot in mission.loot:
        item = ARMOR_BY_ID[loot.item_id]
        colour = QColor(TIER_COLORS.get(item.tier, "#cfc8b8"))
        pulse = 0.5 + 0.5 * math.sin(loot.age * 4.0)
        bob = math.sin(loot.age * 3.0) * 3.0
        glow = QRadialGradient(QPointF(loot.x, loot.y + bob), 30)
        centre = QColor(colour)
        centre.setAlpha(int(120 + 80 * pulse))
        outer = QColor(colour)
        outer.setAlpha(0)
        glow.setColorAt(0.0, centre)
        glow.setColorAt(1.0, outer)
        painter.setPen(Qt.NoPen)
        painter.setBrush(glow)
        painter.drawEllipse(QPointF(loot.x, loot.y + bob), 30, 30)
        painter.drawImage(QRectF(loot.x - 17, loot.y - 17 + bob, 34, 34),
                          armour_icon(item.id, item.slot, 64))
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
        living = [a for a in mission.allies if not a.dead]
        for command, rect in command_rects(window):
            active = command == mission.command and bool(living)
            painter.setPen(QPen(QColor("#8bd9bd" if active else "#95794e"), 1.5))
            painter.setBrush(QColor("#29463d" if active else "#312820"))
            painter.drawRoundedRect(rect, 5, 5)
            painter.setPen(QColor("#f9edce"))
            key = short_binding(window.controls.binding("companion_"+command))
            painter.drawText(rect, Qt.AlignCenter, f"{key}  {command.title()}")
        painter.setPen(QColor("#e7d4ac"))
        party = "  ".join(f"{a.display_name} {a.hp:.0f}/{a.max_hp:.0f}" for a in living)
        painter.drawText(hr.adjusted(14, 186, -14, -3), Qt.AlignLeft | Qt.AlignVCenter,
                         painter.fontMetrics().elidedText(party or "Companions fallen", Qt.ElideRight,
                                                          hr.width() - 28))
    painter.restore()
    if mission.ended:
        draw_end_title(painter, mission)


def end_title_text(mission):
    """(title, subtitle) for the end screen."""
    hero = mission.hero
    extras = []
    found = len(getattr(mission, "found", []))
    if found:
        extras.append(f"found {found} piece{'s' if found != 1 else ''} of armour")
    if getattr(mission, "reward_text", ""):
        extras.append(mission.reward_text)
    tail = "".join(f"  \u00b7  {e}" for e in extras)
    if mission.state == "victory":
        return "VICTORY", f"The desktop is yours  \u00b7  {hero.display_name} reached level {hero.level}{tail}"
    return "DEFEAT", f"{hero.display_name} has fallen  \u00b7  level, XP and loot are kept{tail}"


def draw_end_title(painter, mission):
    """A large VICTORY or DEFEAT across the middle of the arena (the owner)."""
    area = mission.area
    fade = min(1.0, mission.end_clock / 0.4)
    won = mission.state == "victory"
    title, subtitle = end_title_text(mission)
    cx, cy = area.x + area.w / 2.0, area.y + area.h / 2.0
    band = QRectF(area.x, cy - 120, area.w, 240)
    painter.save()
    painter.setClipping(False)
    painter.setPen(Qt.NoPen)
    painter.setBrush(QColor(18, 12, 8, int(200 * fade)))
    painter.drawRect(band)
    edge = QColor("#d6a448" if won else "#b8472f")
    edge.setAlpha(int(255 * fade))
    painter.setPen(QPen(edge, 3))
    painter.drawLine(QPointF(band.left(), band.top()), QPointF(band.right(), band.top()))
    painter.drawLine(QPointF(band.left(), band.bottom()), QPointF(band.right(), band.bottom()))
    font = QFont("Segoe UI", 12, QFont.Black)
    font.setPixelSize(int(min(110, max(56, area.w * 0.075))))
    font.setLetterSpacing(QFont.AbsoluteSpacing, 6)
    path = QPainterPath()
    path.addText(0, 0, font, title)
    box = path.boundingRect()
    path.translate(cx - box.center().x(), cy - 18 - box.center().y())
    painter.setPen(QPen(QColor(10, 6, 4, int(230 * fade)), 6, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
    painter.setBrush(Qt.NoBrush)
    painter.drawPath(path)
    painter.setPen(Qt.NoPen)
    fill = QColor("#f6cf6a" if won else "#e0604a")
    fill.setAlpha(int(255 * fade))
    painter.setBrush(fill)
    painter.drawPath(path)
    small = QFont("Segoe UI", 15, QFont.Bold)
    painter.setFont(small)
    painter.setPen(QColor(246, 226, 184, int(255 * fade)))
    painter.drawText(QRectF(area.x, cy + 38, area.w, 30), Qt.AlignCenter, subtitle)
    left = max(0.0, mission.END_SCREEN_SECONDS - mission.end_clock)
    painter.setFont(QFont("Segoe UI", 10))
    painter.setPen(QColor(220, 195, 154, int(220 * fade)))
    painter.drawText(QRectF(area.x, cy + 72, area.w, 24), Qt.AlignCenter,
                     f"Back to Adventure in {left:.0f}s")
    painter.restore()
