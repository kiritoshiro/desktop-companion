"""The hybrid sprite-rig renderer: skinned leg segments over a drawn body.

Split out of the original monolithic creature.py (DC-11): a pure move, the
methods below are unchanged, only relocated and regrouped by concern.
"""

from __future__ import annotations

from PyQt5.QtCore import QPointF, QRectF, Qt
from PyQt5.QtGui import QBrush, QPainter, QPen

import math
import os

from ..math_utils import (
    clamp,
)

class RenderSpriteMixin:
    """Sprite-rig drawing, layered over the shared leg/colour helpers."""

    def _render_sprite_rig(self, painter) -> None:

        startle = self._startle_amount()
        startle_highlight = self._startle_highlight_active(startle)
        motion_startle = 0.0 if self.dragging else startle
        tremble_x = math.sin(self.breath_phase * 17.0 + self.startle_phase) * self.size * 0.018 * motion_startle
        tremble_y = math.cos(self.breath_phase * 19.0 + self.startle_phase * 0.7) * self.size * 0.018 * motion_startle
        leg_y_off = -self.jump_z
        jz_shadow = clamp(self.jump_z / max(1.0, self.size), 0.0, 3.0)
        shadow_shrink = 1.0 / (1.0 + jz_shadow * 0.55)

        assets = self._load_sprite_assets()
        if not assets:
            self._render_procedural(painter)
            return

        painter.save()
        # Keep smooth pixmap transforms enabled by default. Disabling this caused
        # some PNG sprite-rig parts to render clipped/partly invisible on Windows
        # transparent overlays. Set DESKTOP_BUG_FAST_PIXMAPS=1 only if you prefer
        # speed over visual correctness.
        if os.environ.get("DESKTOP_BUG_FAST_PIXMAPS", "0").strip().lower() not in {"1", "true", "yes", "on"}:
            painter.setRenderHint(QPainter.SmoothPixmapTransform, True)

        shadow = assets.get("shadow")
        shadow_scale = float(self._appearance("shadow_scale", 1.6))
        if shadow is not None:
            painter.save()
            # Keep shadow tied to parent opacity too, so Camouflage fades as one
            # complete creature instead of leaving solid parts behind.
            try:
                parent_opacity = float(painter.opacity())
            except Exception:
                parent_opacity = 1.0
            painter.setOpacity(parent_opacity * 0.34 * shadow_shrink)
            shadow_w = self.size * shadow_scale * (1.0 + clamp(self.current_speed / 180.0, 0.0, 1.0) * 0.10) * shadow_shrink
            shadow_h = shadow_w * 0.52
            painter.drawPixmap(QRectF(self.x - shadow_w * 0.5, self.y + self.size * 0.05, shadow_w, shadow_h), shadow, QRectF(shadow.rect()))
            painter.restore()
        else:
            painter.setPen(Qt.NoPen)
            painter.setBrush(QBrush(self._qcolor("legs", max(8, int(28 * shadow_shrink)))))
            painter.drawEllipse(QPointF(self.x, self.y + self.size * 0.14), self.size * 0.88 * shadow_shrink, self.size * 0.46 * shadow_shrink)

        leg_upper = assets.get("leg_upper")
        leg_lower = assets.get("leg_lower", leg_upper)
        leg_tip = assets.get("leg_tip", leg_lower)
        leg_thick = float(self._appearance("leg_segment_thickness", self.size * 0.34)) * (1.0 + startle * 0.08)
        leg_tip_thick = float(self._appearance("leg_tip_thickness", self.size * 0.18)) * (1.0 + startle * 0.08)
        foot_bulb = float(self._appearance("foot_bulb", self.size * 0.055)) * (1.0 + startle * 0.20)
        chain_config = self._sprite_leg_chain_config()
        leg_knuckle = assets.get(chain_config["asset_name"], leg_lower) if chain_config else None

        for leg in self.legs:
            ax, ay, foot_x, foot_y = self._leg_draw_points(leg)
            # Use the same safe endpoint for the knee solve and the final draw.
            # Previously _solve_knee clamped only its private copy of the foot,
            # while the sprite segments still drew to the unclamped endpoint.
            foot_x, foot_y = self._safe_sprite_leg_foot(leg, foot_x, foot_y, chain_config)
            kx, ky = self._solve_knee(ax, ay, foot_x, foot_y, leg)
            # Lift the swinging / reaching leg off the ground (lively gait).
            lift_px = self._leg_lift_px(leg)
            if lift_px > 0.0:
                flex = leg.lift * 0.08
                foot_x += (ax - foot_x) * flex
                foot_y += (ay - foot_y) * flex
                foot_y -= lift_px
                ky -= lift_px * 0.45
            if leg_y_off:
                ay += leg_y_off
                ky += leg_y_off
                foot_y += leg_y_off
            step_gain = leg.lift * 0.12
            seg_scale = 1.0 + step_gain
            if chain_config:
                points = self._sprite_leg_chain_points(leg, ax, ay, foot_x, foot_y, chain_config)
                scales = chain_config["segment_scales"]
                width_bases = [
                    leg_thick,
                    leg_thick,
                    leg_thick * 0.84,
                    leg_thick * 0.68,
                    leg_tip_thick,
                ]
                widths = [width_bases[index] * scales[index] * seg_scale for index in range(len(points) - 1)]
                sprites = [leg_upper, leg_lower, leg_knuckle, leg_knuckle, leg_tip][:len(points) - 1]
                for index, sprite in enumerate(sprites):
                    start_x, start_y = points[index]
                    end_x, end_y = points[index + 1]
                    self._draw_sprite_segment(painter, sprite, start_x, start_y, end_x, end_y, widths[index], opacity=1.0)
            else:
                tarsus_x = kx + (foot_x - kx) * 0.72
                tarsus_y = ky + (foot_y - ky) * 0.72
                self._draw_sprite_segment(painter, leg_upper, ax, ay, kx, ky, leg_thick * seg_scale, opacity=1.0)
                self._draw_sprite_segment(painter, leg_lower, kx, ky, tarsus_x, tarsus_y, leg_thick * 0.84 * seg_scale, opacity=1.0)
                self._draw_sprite_segment(painter, leg_tip, tarsus_x, tarsus_y, foot_x, foot_y, leg_tip_thick * seg_scale, opacity=1.0)
            painter.setPen(Qt.NoPen)
            painter.setBrush(QBrush(self._qcolor("legs", 215)))
            if chain_config:
                joint_scales = chain_config["joint_scales"]
                for joint_index, (joint_x, joint_y) in enumerate(points[1:-1]):
                    radius = leg_thick * 0.14 * joint_scales[joint_index]
                    painter.drawEllipse(QPointF(joint_x, joint_y), radius, radius)
                painter.setBrush(QBrush(self._qcolor("highlight", 210)))
                joints = points[1:-1]
                for joint_index, (joint_x, joint_y) in enumerate(joints):
                    core_scale = 0.075 if joint_index in (1, len(joints) - 1) else 0.052
                    radius = leg_thick * core_scale * joint_scales[joint_index]
                    painter.drawEllipse(QPointF(joint_x, joint_y), radius, radius)
            else:
                knee_scale = 1.0
                painter.drawEllipse(QPointF(kx, ky), leg_thick * 0.14 * knee_scale, leg_thick * 0.14 * knee_scale)
                painter.drawEllipse(QPointF(tarsus_x, tarsus_y), leg_tip_thick * 0.18, leg_tip_thick * 0.18)
            painter.setBrush(QBrush(self._qcolor("highlight" if startle_highlight else "legs", 215 if startle_highlight else 180)))
            painter.drawEllipse(QPointF(foot_x, foot_y), foot_bulb * (1.0 + step_gain), foot_bulb * 0.70 * (1.0 + step_gain))

        crouch_drop = self.crouch * self.size * 0.06
        painter.save()
        painter.translate(self.x + tremble_x, self.y + self.body_bob + tremble_y - self.jump_z + crouch_drop)
        painter.rotate(math.degrees(self.heading))
        if self._spider_gait_config() is None:
            painter.rotate(math.degrees(self.body_wiggle))
        jz_body = clamp(self.jump_z / max(1.0, self.size), 0.0, 3.0)
        jump_scale = 1.0 + jz_body * 0.12
        visual_squash = 1.0 if self.state == "Roll" or abs(self.roll_spin) > 1e-4 else self.squash
        vfac = clamp(visual_squash * (1.0 - self.crouch * 0.14), 0.55, 1.2)
        vroot = math.sqrt(vfac)
        painter.scale(jump_scale / vroot, jump_scale * vroot)

        abdomen = assets.get("abdomen")
        cephalothorax = assets.get("cephalothorax")
        abdomen_scale = self._appearance("abdomen_scale", [1.08, 0.92])
        ceph_scale = self._appearance("cephalothorax_scale", [0.84, 0.78])
        abdomen_offset_x = float(self._appearance("abdomen_offset_x", -0.18)) * self.size
        ceph_offset_x = float(self._appearance("cephalothorax_offset_x", 0.36)) * self.size

        abdomen_w = self.size * float(abdomen_scale[0]) * (1.0 - self.abdomen_pulse * 0.18) * (1.0 + startle * 0.050)
        abdomen_h = self.size * float(abdomen_scale[1]) * (1.0 + self.abdomen_pulse * 0.55) * (1.0 - startle * 0.055)
        ceph_w = self.size * float(ceph_scale[0]) * (1.0 + self.ceph_pulse * 0.08) * (1.0 + startle * 0.080)
        ceph_h = self.size * float(ceph_scale[1]) * (1.0 + self.ceph_pulse * 0.18) * (1.0 + startle * 0.030)
        ceph_w *= (1.0 + self.rear * 0.12)
        ceph_h *= (1.0 + self.rear * 0.12)
        ceph_offset_x += self.rear * self.size * 0.05

        if abdomen is not None:
            painter.save()
            painter.rotate(math.degrees(self.abdomen_wag))
            painter.drawPixmap(QRectF(abdomen_offset_x - abdomen_w * 0.5, -abdomen_h * 0.5, abdomen_w, abdomen_h), abdomen, QRectF(abdomen.rect()))
            painter.restore()
        if cephalothorax is not None:
            painter.drawPixmap(QRectF(ceph_offset_x - ceph_w * 0.5, -ceph_h * 0.5, ceph_w, ceph_h), cephalothorax, QRectF(cephalothorax.rect()))

        painter.setPen(QPen(self._qcolor("legs", 215), max(1.0, self.size * 0.036), Qt.SolidLine, Qt.RoundCap))
        palp_span = self.size * float(self._appearance("pedipalp_scale", 1.0))
        painter.drawLine(QPointF(ceph_offset_x + ceph_w * 0.24, -self.size * 0.10), QPointF(ceph_offset_x + ceph_w * 0.54, -self.size * 0.21 * palp_span / self.size))
        painter.drawLine(QPointF(ceph_offset_x + ceph_w * 0.24, self.size * 0.10), QPointF(ceph_offset_x + ceph_w * 0.54, self.size * 0.21 * palp_span / self.size))

        eye_scale = float(self._appearance("eye_scale", 0.95))
        eye_count = max(2, int(self._appearance("eye_count", 4)))
        eye_layout = str(self._appearance("eye_layout", "grid")).lower()
        aiming = clamp(self.aim_intent, 0.0, 1.0)
        eye_startle = self._eye_startle_amount(startle)
        eye_r = max(1.0, self.size * 0.030 * eye_scale) * (1.0 + eye_startle * 0.78)
        eyes = []
        if eye_layout == "jumping_spider":
            big_r = eye_r * 1.9
            small_r = eye_r * 0.75
            eyes = [
                (ceph_offset_x + ceph_w * 0.05, -ceph_h * 0.03, big_r),
                (ceph_offset_x + ceph_w * 0.28, -ceph_h * 0.03, big_r),
                (ceph_offset_x - ceph_w * 0.10, -ceph_h * 0.14, small_r),
                (ceph_offset_x + ceph_w * 0.44, -ceph_h * 0.14, small_r),
                (ceph_offset_x - ceph_w * 0.04, ceph_h * 0.16, small_r * 0.9),
                (ceph_offset_x + ceph_w * 0.36, ceph_h * 0.16, small_r * 0.9),
            ]
        else:
            rows = 2
            cols = max(2, eye_count // 2)
            for r in range(rows):
                for c in range(cols):
                    if r * cols + c >= eye_count:
                        break
                    eyex = ceph_offset_x + ceph_w * (0.02 + c * 0.11)
                    eyey = (-0.17 + r * 0.17 + (c % 2) * 0.015) * ceph_h
                    eyes.append((eyex, eyey, eye_r * (0.92 if c % 2 else 1.0)))
        self._draw_eyes(painter, eyes, ceph_offset_x, ceph_w, ceph_h, startle, aiming)
        self._draw_antennae(painter, ceph_offset_x, ceph_w, ceph_h, startle)
        painter.restore()
        painter.restore()
        self._draw_equipment(painter)

