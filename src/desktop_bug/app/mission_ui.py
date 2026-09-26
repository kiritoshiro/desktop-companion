"""Cached woodland buildings and readable mission overlays."""
from __future__ import annotations


from PyQt5.QtCore import QPointF, QRect, QRectF, QSizeF, Qt
from PyQt5.QtGui import QColor, QFont, QPainterPath, QPen, QRadialGradient

import math

from .adventure_ui import _board, _home_area, hud_rect, short_binding
from .mission_art import building_art
from .stat_text import TIER_COLORS
from ..creature.armour_art import armour_icon
from ..state.progression import ARMOR_BY_ID


def draw_buildings(painter, mission):
    draw_screen_links(painter, mission)
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
                      "silk": "12 silk capacity + fast refill", "hatchery": "SEALED", "nest": "CLAIMED",
                      "outpost": "TAKEN", "infestation": "DESTROYED"}.get(s.kind, "HELD")
        elif s.kind == "outpost":
            status = f"{s.reserves} raids left / hold to take"
        elif s.kind == "infestation":
            status = "Hold nearby to burn it out"
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
    draw_hazards(painter, mission)


def draw_screen_links(painter, mission):
    """Where the screens join: a chevron at each door, a web-lined hole at
    each tunnel mouth, labelled with the screen it leads to."""
    layout = getattr(mission, "layout", None)
    if layout is None or not layout.multi:
        return
    painter.save()
    painter.setFont(QFont("Segoe UI", 9, QFont.Bold))
    for link in layout.links:
        for here in (link.a, link.b):
            x, y = link.point_on(here)
            there = layout.screens[link.other(here)]
            if link.kind == "tunnel":
                glow = QRadialGradient(QPointF(x, y), 40)
                glow.setColorAt(0.0, QColor(10, 6, 16, 250))
                glow.setColorAt(0.55, QColor(40, 20, 60, 220))
                glow.setColorAt(1.0, QColor(150, 110, 220, 0))
                painter.setPen(Qt.NoPen)
                painter.setBrush(glow)
                painter.drawEllipse(QPointF(x, y), 40, 40)
                painter.setPen(QPen(QColor(230, 220, 255, 150), 1))
                for i in range(8):
                    a = i / 8 * math.tau
                    painter.drawLine(QPointF(x, y), QPointF(x + math.cos(a) * 30, y + math.sin(a) * 30))
                label = f"Tunnel to {there.name}"
            else:
                ox, oy = link.point_on(there.index)
                angle = math.atan2(oy - y, ox - x)
                painter.setPen(QPen(QColor(247, 223, 147, 200), 3, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
                for step in (0, 12):
                    cx, cy = x - math.cos(angle) * (30 - step), y - math.sin(angle) * (30 - step)
                    for side in (2.5, -2.5):
                        painter.drawLine(QPointF(cx + math.cos(angle + side) * 10, cy + math.sin(angle + side) * 10),
                                         QPointF(cx, cy))
                label = f"To {there.name}"
                # The label stands on this side of the seam, clear of the other's.
                x, y = x - math.cos(angle) * 95, y - math.sin(angle) * 95
            box = QRectF(x - 70, y + 30, 140, 20)
            painter.setPen(Qt.NoPen)
            painter.setBrush(QColor(20, 16, 12, 190))
            painter.drawRoundedRect(box, 6, 6)
            painter.setPen(QColor("#f7df93"))
            painter.drawText(box, Qt.AlignCenter, label)
    painter.restore()


def draw_hazards(painter, mission):
    """Acid in flight, acid landing, and words unravelling into silk."""
    painter.save()
    for glob in getattr(mission, "hazards", ()):
        x, y, height = glob.position
        painter.setPen(Qt.NoPen)
        painter.setBrush(QColor(0, 0, 0, 60))
        painter.drawEllipse(QPointF(x, y), 7, 3.5)
        glow = QRadialGradient(QPointF(x, y - height), 13)
        glow.setColorAt(0.0, QColor(235, 255, 150, 255))
        glow.setColorAt(0.45, QColor(140, 230, 40, 230))
        glow.setColorAt(1.0, QColor(90, 180, 20, 0))
        painter.setBrush(glow)
        painter.drawEllipse(QPointF(x, y - height), 13, 13)
    for splash in getattr(mission, "splashes", ()):
        fade = max(0.0, 1.0 - splash.age / 0.9)
        painter.setPen(QPen(QColor(170, 255, 60, int(220 * fade)), 2.5))
        painter.setBrush(QColor(120, 220, 30, int(90 * fade)))
        r = 14 + splash.age * 40
        painter.drawEllipse(QPointF(splash.x, splash.y), r, r * 0.55)
    for thread in getattr(mission, "threads", ()):
        spider = thread.spider
        fade = max(0.0, 1.0 - thread.age / 0.7)
        t = min(1.0, thread.age / 0.45)
        painter.setPen(QPen(QColor(250, 246, 230, int(230 * fade)), 1.4))
        for i in range(3):
            wobble = math.sin(thread.age * 20 + i * 2) * 6
            mx = thread.x0 + (spider.x - thread.x0) * t
            my = thread.y0 + (spider.y - thread.y0) * t + wobble
            painter.drawLine(QPointF(thread.x0 + i * 4, thread.y0), QPointF(mx, my))
    painter.restore()


def minimap_rect(window, mission):
    """Top right of the main screen, sized to the monitors' own proportions."""
    area = _home_area(window)
    layout = mission.layout
    xs = [s.rect.x for s in layout.screens] + [s.rect.right for s in layout.screens]
    ys = [s.rect.y for s in layout.screens] + [s.rect.bottom for s in layout.screens]
    span_w, span_h = max(xs) - min(xs), max(ys) - min(ys)
    scale = min(230.0 / max(1.0, span_w), 120.0 / max(1.0, span_h))
    w, h = span_w * scale, span_h * scale
    return QRectF(area.right() - w - 34, area.top() + 16, w + 20, h + 36), scale, min(xs), min(ys)


def draw_minimap(painter, window, mission):
    """Every screen, where you are, your companions, the enemies and outposts."""
    layout = getattr(mission, "layout", None)
    if layout is None or not layout.multi:
        return
    box, scale, x0, y0 = minimap_rect(window, mission)
    _board(painter, box.toAlignedRect(), 8)

    def at(x, y):
        return QPointF(box.x() + 10 + (x - x0) * scale, box.y() + 26 + (y - y0) * scale)

    painter.setFont(QFont("Segoe UI", 8, QFont.Bold))
    painter.setPen(QColor("#f9df9d"))
    here = layout.screen_at(mission.hero.x, mission.hero.y)
    painter.drawText(QRectF(box.x() + 8, box.y() + 4, box.width() - 16, 18), Qt.AlignLeft | Qt.AlignVCenter,
                     f"Screens  \u00b7  you: {here.name}")
    for screen in layout.screens:
        r = screen.rect
        rect = QRectF(at(r.x, r.y), at(r.right, r.bottom)).adjusted(1, 1, -1, -1)
        painter.setPen(QPen(QColor("#f7df93" if screen.index == here.index else "#9a7951"), 1.4))
        painter.setBrush(QColor(20, 30, 38, 200))
        painter.drawRect(rect)
    painter.setPen(Qt.NoPen)
    for site in mission.sites:
        if site.owned:
            colour = "#8ce0be"
        elif site.kind in ("outpost", "infestation"):
            colour = "#ff9a3c"
        else:
            colour = "#c9a36a"
        painter.setBrush(QColor(colour))
        painter.drawRect(QRectF(at(site.x, site.y) - QPointF(3, 3), QSizeF(6, 6)))
    for link in layout.tunnels:
        painter.setBrush(QColor("#b58cff"))
        for point in (link.a_point, link.b_point):
            painter.drawEllipse(at(*point), 2.5, 2.5)
    for c in mission.manager.creatures:
        if c.dead:
            continue
        if c is mission.hero:
            colour, r = "#ffd76a", 3.5
        elif mission.hero.relation_to(c) == "foe":
            colour, r = "#ff5a4a", 2.2
        else:
            colour, r = "#7fe0a0", 2.5
        painter.setBrush(QColor(colour))
        painter.drawEllipse(at(c.x, c.y), r, r)


def draw_reclaim_extras(painter, window, mission):
    """How much desktop is left, and the opening card that says it is frozen."""
    surface = getattr(mission, "surface", None)
    if surface is None:
        return
    banner = mission_banner_rect(window)
    bar = QRectF(banner.x() + 14, banner.bottom() + 6, banner.width() - 28, 16)
    left = surface.integrity
    lost = getattr(mission, "LOST_BELOW", 0.45)
    painter.setPen(QPen(QColor("#9a7951"), 1))
    painter.setBrush(QColor(24, 18, 14, 225))
    painter.drawRoundedRect(bar, 5, 5)
    if left > lost + 0.2:
        fill = QColor("#7fd06a")
    elif left > lost + 0.08:
        fill = QColor("#e0b24a")
    else:
        fill = QColor("#e0604a")
    painter.setPen(Qt.NoPen)
    painter.setBrush(fill)
    painter.drawRoundedRect(QRectF(bar.x() + 2, bar.y() + 2, (bar.width() - 4) * left, bar.height() - 4), 4, 4)
    painter.setPen(QPen(QColor(255, 255, 255, 170), 1, Qt.DashLine))
    mark = bar.x() + 2 + (bar.width() - 4) * lost
    painter.drawLine(QPointF(mark, bar.top() + 1), QPointF(mark, bar.bottom() - 1))
    painter.setFont(QFont("Segoe UI", 8, QFont.Bold))
    painter.setPen(QColor("#fff0cd"))
    words = getattr(mission, "words_eaten", 0)
    painter.drawText(bar, Qt.AlignCenter,
                     f"Desktop left {left * 100:.0f}%  \u00b7  lost below {lost * 100:.0f}%"
                     f"  \u00b7  words eaten {words}")
    intro = getattr(mission, "intro_time", 0.0)
    if intro <= 0 or mission.ended:
        return
    fade = min(1.0, intro / 0.8)
    area = mission.area
    card = QRectF(area.x + area.w / 2 - 330, area.y + area.h / 2 - 110, 660, 220)
    painter.setPen(QPen(QColor(214, 164, 72, int(255 * fade)), 2))
    painter.setBrush(QColor(16, 10, 8, int(235 * fade)))
    painter.drawRoundedRect(card, 12, 12)
    painter.setFont(QFont("Segoe UI", 24, QFont.Black))
    painter.setPen(QColor(246, 207, 106, int(255 * fade)))
    painter.drawText(card.adjusted(20, 16, -20, -140), Qt.AlignCenter, "Reclaim the desktop")
    painter.setFont(QFont("Segoe UI", 11))
    painter.setPen(QColor(242, 231, 211, int(255 * fade)))
    screens = mission.layout.count
    many = f" - all {screens} of them" if screens > 1 else ""
    text = ("Your desktop has been taken. You have no control of it now: every screen is frozen "
            f"and infested{many}.\n"
            "Destroy the nests before their acid eats the desktop.\n"
            "Esc and Leave gives you the desktop back at once. Nothing real is harmed.")
    painter.drawText(card.adjusted(26, 78, -26, -14), Qt.AlignHCenter | Qt.AlignTop | Qt.TextWordWrap, text)


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
    painter.save()
    painter.setClipping(False)
    draw_minimap(painter, window, mission)
    draw_reclaim_extras(painter, window, mission)
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
    words = getattr(mission, "words_eaten", 0)
    if words:
        plural = "s" if words != 1 else ""
        tail += f"  \u00b7  {words} word{plural} eaten"
    if mission.state == "victory":
        return "VICTORY", f"The desktop is yours  \u00b7  {hero.display_name} reached level {hero.level}{tail}"
    if getattr(mission, "lost_desktop", False):
        return "DEFEAT", f"The acid ate the desktop  \u00b7  level, XP and loot are kept{tail}"
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
