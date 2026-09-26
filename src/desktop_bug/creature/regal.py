"""Regal jumper: bounded attention gestures and a fluffy, elevated face.

Expressions are cosmetic and never take ownership of navigation/combat. Real
salticid displays inspire the poses; friendly greetings are anthropomorphism.
Rendering is pure: repaint frequency cannot advance a pose or consume RNG.
"""
from __future__ import annotations

from dataclasses import dataclass
import math

from PyQt5.QtCore import QLineF, QPointF, Qt
from PyQt5.QtGui import QColor, QPainterPath, QPen, QRadialGradient

from ..support.math_utils import clamp


@dataclass
class RegalExpression:
    clock: float = 0.0
    cooldown: float = 1.2
    age: float = 0.0
    duration: float = 0.0
    gesture: str = "rest"
    target: str = "none"
    yaw: float = 0.0
    pitch: float = 0.0
    tilt: float = 0.0
    left: float = 0.0
    right: float = 0.0
    bounce: float = 0.0
    palps: float = 0.0


class RegalMixin:
    def _is_regal(self):
        return self.model.get("appearance", {}).get("species_profile") == "regal_fluff"

    def _regal_blocked(self):
        return (self.dead or self.dragging or self.webbed or self.airborne
                or self.player_control is not None or self._hunting_prey
                or self._foe is not None or self._desktop_fully_hidden
                or self.state not in ("Idle", "Wander", "Approach", "Play", "Observe")
                or self.current_speed > 45 or self.startled_timer > 0)

    def _update_regal(self, dt, mx, my):
        if not self._is_regal():
            return
        if not hasattr(self, "regal"):
            self.regal = RegalExpression(cooldown=1.2 + self.index * 0.37)
        r = self.regal
        # Safety/skill states always own the body. No greetings while trapped,
        # fighting, carried, airborne, hidden, or controlled by the player.
        blocked = self._regal_blocked()
        r.clock += dt
        r.cooldown = max(0.0, r.cooldown - dt)
        tx, ty = mx, my
        target = "mouse" if math.hypot(mx - self.x, my - self.y) < 230 else "none"
        if self.allow_social:
            candidates = [other for other in self.neighbors
                          if other is not self and not other.dead and not other.dragging
                          and not other._desktop_fully_hidden
                          and self.relation_to(other) != "foe"
                          and math.hypot(other.x - self.x, other.y - self.y) < 180]
            if candidates:
                other = min(candidates, key=lambda o: math.hypot(o.x-self.x, o.y-self.y))
                tx, ty, target = other.x, other.y, "spider"
        if blocked:
            r.gesture, r.target, r.duration, r.age = "rest", "none", 0.0, 0.0
            r.cooldown = max(r.cooldown, 1.0)
            r.left = r.right = r.bounce = r.palps = r.tilt = 0.0
            r.yaw *= math.exp(-dt * 7)
            r.pitch *= math.exp(-dt * 7)
            return
        # Track with the whole cephalothorax; spider eyes do not swivel like
        # human eyeballs. The tiny tilt is a stylised body lean, not a neck.
        dx, dy = tx - self.x, ty - self.y
        side = -math.sin(self.heading) * dx + math.cos(self.heading) * dy
        forward = math.cos(self.heading) * dx + math.sin(self.heading) * dy
        yaw = clamp(side / 160, -0.65, 0.65) if target != "none" else math.sin(r.clock * .55) * .22
        pitch = clamp(forward / 220, -.2, .65) if target != "none" else .12
        blend = 1 - math.exp(-dt * 5)
        r.yaw += (yaw - r.yaw) * blend
        r.pitch += (pitch - r.pitch) * blend
        r.target = target
        if r.duration == 0 and r.cooldown == 0:
            choices = (("greet", "dance", "question") if target == "spider" else
                       ("question", "peek", "reach") if target == "mouse" else
                       ("groom", "peek", "dance"))
            r.gesture = self.rng.choice(choices)
            r.duration = 3.4 if r.gesture == "dance" else 2.5
            r.age = 0.0
        r.left = r.right = r.bounce = r.tilt = r.palps = 0.0
        if r.duration:
            r.age += dt
            u = min(1.0, r.age / r.duration)
            # Ease in, hold, settle. No discontinuity between poses.
            envelope = math.sin(math.pi * u) ** 2
            beat = math.sin(r.age * 8)
            if r.gesture in ("question", "reach"):
                r.left = envelope * (0.85 if r.yaw <= 0 else .1)
                r.right = envelope * (0.85 if r.yaw > 0 else .1)
                r.tilt = envelope * (-.14 if r.yaw <= 0 else .14)
                r.palps = envelope * .25
            elif r.gesture in ("greet", "dance"):
                r.left = envelope * (.76 + .20 * beat)
                r.right = envelope * (.76 - .20 * beat)
                r.tilt = envelope * math.sin(r.age * 4) * .12
                r.bounce = envelope * abs(beat) * .09
                r.palps = envelope * beat * .5
            elif r.gesture == "peek":
                r.tilt = envelope * math.sin(r.age * 2.7) * .19
                r.bounce = envelope * .045
                r.palps = envelope * .22
            else:
                r.palps = envelope * math.sin(r.age * 10)
            if u >= 1:
                r.gesture, r.duration = "rest", 0.0
                r.cooldown = self.rng.uniform(3.5, 7.5)

    def _render_regal(self, painter):
        r = getattr(self, "regal", RegalExpression())
        if self._regal_blocked():
            r = RegalExpression()
        size = self.size
        fur = self._qcolor("highlight")
        body = self._qcolor("body")
        legs = self._qcolor("legs")
        painter.save()
        painter.setPen(Qt.NoPen)
        painter.setBrush(QColor(0, 0, 0, 32))
        shrink = 1 / (1 + self.jump_z / max(size, 1))
        painter.drawEllipse(QPointF(self.x, self.y + size * .25), size * .9 * shrink, size * .5 * shrink)
        # Existing planted-foot solver keeps walking/turning/dragging grounded.
        # Only the first pair is lifted by a greeting; six feet remain available.
        for leg in reversed(self.legs):
            ax, ay, fx, fy = self._leg_draw_points(leg)
            kx, ky = self._solve_knee(ax, ay, fx, fy, leg)
            name = leg.definition.get("name", "")
            raised = (r.left if leg.definition.get("side") == "left" else r.right) if name.startswith("front_") else 0
            if raised:
                side = -1 if leg.definition.get("side") == "left" else 1
                px, py = self._body_local_to_world(size * (1.15 + raised * .3), side * size * .85)
                fx += (px - fx) * raised
                fy += (py - fy) * raised - size * raised * .52
                kx += (ax - kx) * raised * .15
                ky -= size * raised * .35
            lift = self._leg_lift_px(leg)
            points = [(ax, ay - self.jump_z), (kx, ky - self.jump_z - lift * .45),
                      (kx + (fx-kx)*.66, ky + (fy-ky)*.66 - self.jump_z - lift*.8),
                      (fx, fy - self.jump_z - lift)]
            for i, (a, b) in enumerate(zip(points, points[1:])):
                width = size * (.20, .15, .085)[i]
                painter.setPen(QPen(legs, width, Qt.SolidLine, Qt.RoundCap))
                painter.setBrush(Qt.NoBrush)
                curve = QPainterPath(QPointF(*a))
                curve.quadTo(QPointF((a[0]+b[0])*.5, (a[1]+b[1])*.5-width*.22), QPointF(*b))
                painter.drawPath(curve)
                # Ivory cuffs and fine fixed hairs, never random per repaint.
                painter.setPen(QPen(fur.darker(125), width*.94, Qt.SolidLine, Qt.RoundCap))
                painter.drawLine(QPointF(a[0]+(b[0]-a[0])*.28, a[1]+(b[1]-a[1])*.28),
                                 QPointF(a[0]+(b[0]-a[0])*.46, a[1]+(b[1]-a[1])*.46))
                hair_groups = ([], [])
                count = 10 if size < 40 else 18
                for j in range(count):
                    t = .12 + j * .65 / count
                    x, y = a[0] + (b[0]-a[0])*t, a[1] + (b[1]-a[1])*t
                    angle = math.atan2(b[1]-a[1], b[0]-a[0]) + math.pi/2 + .25*math.sin(j*4.7)
                    hair = width * (.58 + .22 * math.sin(j*2.3))
                    for sign in (-1, 1):
                        hair_groups[int(.23 < t < .52)].append(QLineF(
                            x+math.cos(angle)*width*.3*sign, y+math.sin(angle)*width*.3*sign,
                            x+math.cos(angle)*hair*sign, y+math.sin(angle)*hair*sign))
                for color, lines in zip((fur.darker(180), fur), hair_groups):
                    painter.setPen(QPen(color, max(.35, size*.009), Qt.SolidLine, Qt.RoundCap))
                    painter.drawLines(lines)
        lunge_x, lunge_y = self.combat_body_offset()
        painter.translate(self.x + lunge_x, self.y - self.jump_z + self.body_bob + lunge_y - size*r.bounce)
        painter.rotate(math.degrees(self.heading) + 90)
        painter.scale(size, size)
        painter.rotate(math.degrees(r.tilt))

        def plush(cx, cy, rx, ry, color, hairs=48):
            hairs = hairs // 2 if size < 40 else hairs
            gradient = QRadialGradient(QPointF(cx-rx*.3, cy-ry*.4), max(rx, ry)*1.6)
            gradient.setColorAt(0, color.lighter(155))
            gradient.setColorAt(.6, color)
            gradient.setColorAt(1, color.darker(170))
            painter.setPen(Qt.NoPen)
            painter.setBrush(gradient)
            painter.drawEllipse(QPointF(cx, cy), rx, ry)
            painter.setPen(QPen(fur, .009, Qt.SolidLine, Qt.RoundCap))
            lines = []
            for i in range(hairs):
                a = i * math.tau / hairs + math.sin(i*4.3)*.014
                length = .035 + .07 * (.5+.5*math.sin(i*7.3))
                x, y = cx+math.cos(a)*rx*.94, cy+math.sin(a)*ry*.94
                lines.append(QLineF(x, y, x+math.cos(a+.3)*length, y+math.sin(a+.3)*length))
            painter.drawLines(lines)
            # Short surface setae break up the solid body, with deterministic
            # spacing so fur neither flickers nor allocates random particles.
            if hairs:
                painter.setPen(QPen(fur.darker(140), .008, Qt.SolidLine, Qt.RoundCap))
                lines = []
                for i in range(hairs):
                    a = i * 2.39996
                    radius = math.sqrt((i+.5)/hairs)*.9
                    x, y = cx+math.cos(a)*rx*radius, cy+math.sin(a)*ry*radius
                    lines.append(QLineF(x, y, x+math.cos(a)*.028, y+math.sin(a)*.035))
                painter.drawLines(lines)

        plush(0, .32, .48, .65, body, 72)
        painter.setPen(Qt.NoPen)
        painter.setBrush(fur)
        for x, y, w, h in ((0, .14, .17, .12), (-.21, .55, .09, .13), (.21, .55, .09, .13)):
            painter.drawEllipse(QPointF(x, y), w, h)
        # Broad raised cephalothorax, attached directly to the abdomen.
        painter.save()
        painter.translate(r.yaw*.12, -.32-r.pitch*.09)
        painter.rotate(math.degrees(r.yaw)*.23)
        plush(0, 0, .59, .48, body, 84)
        # White brow forms a soft crown without adding a cartoon mouth.
        painter.setPen(QPen(fur, .065, Qt.SolidLine, Qt.RoundCap))
        brow = QPainterPath(QPointF(-.46, -.19))
        brow.quadTo(QPointF(0, -.45), QPointF(.46, -.19))
        painter.drawPath(brow)
        # Eight eyes, including small dorsal eyes, two large forward lenses.
        for x, y, radius in ((-.49,-.16,.08), (.49,-.16,.08),
                             (-.39,-.33,.032), (.39,-.33,.032),
                             (-.29,-.39,.05), (.29,-.39,.05),
                             (-.205,-.075,.182), (.205,-.075,.182)):
            painter.setPen(QPen(QColor(113, 128, 151), .016))
            g = QRadialGradient(QPointF(x-radius*.28, y-radius*.3), radius*1.5)
            g.setColorAt(0, QColor(43, 59, 86))
            g.setColorAt(.55, self._qcolor("eyes"))
            g.setColorAt(1, QColor(3, 6, 12))
            painter.setBrush(g)
            painter.drawEllipse(QPointF(x, y), radius, radius)
            painter.setPen(Qt.NoPen)
            painter.setBrush(QColor(249, 252, 255))
            painter.drawEllipse(QPointF(x-radius*.32, y-radius*.35), radius*.23, radius*.18)
            if radius > .1:
                painter.setBrush(QColor(103, 145, 211, 190))
                painter.drawEllipse(QPointF(x+radius*.28, y+radius*.42), radius*.21, radius*.09)
        for side in (-1, 1):
            # Iridescent chelicerae, tucked behind two independent woolly palps.
            c = self._qcolor("accent")
            plush(side*.13, .26, .115, .19, c, 0)
            painter.save()
            painter.translate(side*(.29 + abs(r.palps)*.045), .27 + side*r.palps*.065)
            painter.rotate(side * (14 + r.palps*22))
            painter.setPen(QPen(legs, .11, Qt.SolidLine, Qt.RoundCap))
            painter.drawLine(QPointF(0, -.13), QPointF(0, .10))
            plush(0, .065, .145, .205, fur.darker(110), 36)
            painter.restore()
        painter.restore()
        painter.restore()
        self._draw_equipment(painter)
