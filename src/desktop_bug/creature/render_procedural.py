"""The procedural body/leg/antenna renderer, plus the colour and sprite-leg
geometry helpers it shares with the sprite-rig renderer.

Split out of the original monolithic creature.py (DC-11): a pure move, the
methods below are unchanged, only relocated and regrouped by concern.
"""

from __future__ import annotations

from PyQt5.QtCore import QPointF, QRectF, Qt
from PyQt5.QtGui import QBrush, QColor, QPainterPath, QPen, QPixmap, QPolygonF

import math
from pathlib import Path
from typing import Optional, Tuple

from ..support.math_utils import (
    clamp,
)
from .mood import antenna_drive_from_mood, build_antenna_points
from . import render_batch
from .render_batch import (
    LAYER_CORE,
    LAYER_FOOT,
    LAYER_FUZZ,
    LAYER_HAIR,
    LAYER_JOINT,
    LAYER_SEGMENT,
    LegBatch,
)
from .render_detail import (
    MERGED_HAIR_TINT,
    hair_detail,
    hair_pen_width,
    joint_node_shows,
)
from .sprite_tint import palette_signature, tint_assets
from ..state.progression import (
    equipped_items,
)
from .kinematics import LegState

def _mix(base, other, amount: float):
    """Warm one colour toward another, keeping the first one's alpha.

    Used when a hair over-stroke is merged into the segment it sat under: the
    stroke takes the hair's width so the leg keeps its weight, and a little of
    its colour so it keeps its tone.
    """
    if other is None or amount <= 0.0:
        return base
    inv = 1.0 - amount
    return QColor(
        int(base.red() * inv + other.red() * amount),
        int(base.green() * inv + other.green() * amount),
        int(base.blue() * inv + other.blue() * amount),
        base.alpha(),
    )


class RenderProceduralMixin:
    """Procedural drawing and the shared colour/leg-geometry helpers."""

    def _qcolor(self, key: str, alpha: int = 255):
        """A painting colour for one palette slot, blended toward camouflage.

        Asked about 100 times per spider per frame. The blended integer triple
        is cached per palette key and thrown away whenever the camouflage state
        changes, so a camouflaging spider still recomputes every frame and a
        spider that is simply walking about does not. A fresh QColor is still
        built each call rather than a shared one being handed out, because a
        caller that mutated it would corrupt every later frame.
        """

        signature = (self._camouflage_color, self._camouflage_strength)
        cache = self._qcolor_cache
        if cache is None or cache[0] != signature:
            cache = (signature, {})
            self._qcolor_cache = cache
        triple = cache[1].get(key)
        if triple is None:
            triple = self._blend_palette_color(key)
            cache[1][key] = triple
        return QColor(triple[0], triple[1], triple[2], alpha)

    def _blend_palette_color(self, key: str) -> tuple:
        raw = self.colors.get(key, [35, 30, 25])
        rgb = [float(raw[0]), float(raw[1]), float(raw[2])]
        camo = self._camouflage_color
        strength = clamp(float(self._camouflage_strength), 0.0, 1.0)
        color_blend = clamp(float(self.personality.get("camouflage_color_blend", 0.0) or 0.0), 0.0, 1.0)
        if camo is not None and strength > 0.001 and color_blend > 0.001:
            blend = strength * color_blend * (0.72 if key == "eyes" else 0.96 if key == "highlight" else 1.0)
            rgb = [rgb[i] * (1.0 - blend) + float(camo[i]) * blend for i in range(3)]
        return (int(clamp(rgb[0], 0, 255)), int(clamp(rgb[1], 0, 255)), int(clamp(rgb[2], 0, 255)))

    def _appearance(self, key: str, default):
        return self.model.get("appearance", {}).get(key, default)

    def _appearance_color(self, key: str, fallback_key: str = "highlight"):
        """Resolve an appearance accent through an explicit slot palette first."""
        if key in getattr(self, "color_overrides", {}):
            return self.colors.get(key)
        if fallback_key in getattr(self, "color_overrides", {}):
            return self.colors.get(fallback_key)
        return self._appearance(key, self.colors.get(fallback_key, [120, 80, 60]))

    def _qcolor_triplet(self, rgb, alpha: int = 255):
        """Same as `_qcolor`, for a colour that is not in the palette.

        Cached the same way and for the same reason: about fifty calls per
        spider per frame, nearly all of them repeats of a handful of accents.
        """

        key = (float(rgb[0]), float(rgb[1]), float(rgb[2]))
        signature = (self._camouflage_color, self._camouflage_strength)
        cache = self._triplet_cache
        if cache is None or cache[0] != signature:
            cache = (signature, {})
            self._triplet_cache = cache
        blended = cache[1].get(key)
        if blended is None:
            blended = self._blend_triplet(key)
            if len(cache[1]) > 128:
                cache[1].clear()
            cache[1][key] = blended
        return QColor(blended[0], blended[1], blended[2], alpha)

    def _blend_triplet(self, rgb) -> tuple:
        out = [float(rgb[0]), float(rgb[1]), float(rgb[2])]
        camo = self._camouflage_color
        strength = clamp(float(self._camouflage_strength), 0.0, 1.0)
        color_blend = clamp(float(self.personality.get("camouflage_color_blend", 0.0) or 0.0), 0.0, 1.0)
        if camo is not None and strength > 0.001 and color_blend > 0.001:
            blend = strength * color_blend * 0.92
            out = [out[i] * (1.0 - blend) + float(camo[i]) * blend for i in range(3)]
        return (int(clamp(out[0], 0, 255)), int(clamp(out[1], 0, 255)), int(clamp(out[2], 0, 255)))

    def _load_sprite_assets(self):

        folder = self.model.get("_folder")
        if not folder:
            return {}
        assets_dir = Path(folder) / "assets"
        # DC-48: the cache is keyed on the palette as well as the folder, so
        # two spiders from one model in different colours each get their own
        # tinted set instead of the first one winning. An untinted spider has
        # an empty signature and shares the pixmaps straight off disk.
        signature = palette_signature(getattr(self, "color_overrides", None))
        cache_key = (str(assets_dir.resolve()), signature)
        if cache_key in self.SPRITE_CACHE:
            return self.SPRITE_CACHE[cache_key]

        names = ["abdomen", "cephalothorax", "leg_upper", "leg_lower", "leg_tip", "leg_knuckle", "shadow"]
        loaded = {}
        for name in names:
            candidate = assets_dir / f"{name}.png"
            if candidate.exists():
                pixmap = QPixmap(str(candidate))
                if not pixmap.isNull():
                    loaded[name] = pixmap
        # Tinting is the expensive part, so it happens here -- once per model
        # per palette, on first render -- and never per frame.
        loaded = tint_assets(loaded, getattr(self, "color_overrides", None))
        self.SPRITE_CACHE[cache_key] = loaded
        return loaded

    def _sprite_leg_chain_config(self):
        """Return a sanitized optional multi-knuckle leg-chain configuration.

        The legacy three-piece sprite rig remains the default.  A model opts in
        with ``appearance.leg_chain`` so existing creatures keep their exact
        renderer path and asset requirements.
        """
        raw = self._appearance("leg_chain", None)
        if not isinstance(raw, dict) or not bool(raw.get("enabled", False)):
            return None
        try:
            segment_count = int(raw.get("segment_count", 4))
            if segment_count not in (4, 5):
                return None
            asset_name = str(raw.get("extra_segment_asset", "leg_knuckle")).strip()
            if not asset_name:
                return None
            split = clamp(float(raw.get("knuckle_split", 0.46)), 0.28, 0.66)
            bend = clamp(float(raw.get("knuckle_bend", 0.12)), 0.0, 0.24)
            max_stretch = clamp(float(raw.get("max_stretch", 1.05)), 1.0, 1.12)
            elevated_arc = clamp(float(raw.get("elevated_arc", 0.0)), 0.0, 0.45)
            proximal_lift = clamp(float(raw.get("proximal_lift", 1.0)), 1.0, 2.20)
            hairy = bool(raw.get("hairy", False))
            hair_scale = clamp(float(raw.get("hair_scale", 0.18)), 0.0, 0.50)
            default_scales = [1.0, 0.84, 0.64, 0.52, 0.42][:segment_count]
            # Width scales are deliberately separate from anatomical lengths.
            # Older configs only had segment_scales, which made the renderer
            # reuse presentation values as geometry and allowed a proximal
            # segment to become much longer than its knuckle could control.
            default_lengths = [1.0, 1.05, 0.78, 0.56, 0.44][:segment_count]
            default_joints = [0.95, 0.72, 0.60, 0.48][:segment_count - 1]
            default_profile = [1.0, 0.64, 0.28, 0.18][:segment_count - 1]
            default_phase_offsets = [0.0, 0.32, -0.22, 0.44][:segment_count - 1]
            default_bend_directions = [1.0, 1.0, 0.72, 0.42][:segment_count - 1]
            segment_scales = [float(value) for value in raw.get("segment_scales", default_scales)]
            width_scales = [float(value) for value in raw.get("width_scales", segment_scales)]
            raw_color_keys = raw.get("segment_color_keys", ["legs"] * segment_count)
            if not isinstance(raw_color_keys, list):
                raw_color_keys = ["legs"] * segment_count
            segment_color_keys = [str(value).strip() or "legs" for value in raw_color_keys]
            segment_lengths = [float(value) for value in raw.get("segment_lengths", default_lengths)]
            joint_scales = [float(value) for value in raw.get("joint_scales", default_joints)]
            bend_profile = [float(value) for value in raw.get("bend_profile", default_profile)]
            joint_phase_offsets = [clamp(float(value), -math.tau, math.tau) for value in raw.get("joint_phase_offsets", default_phase_offsets)]
            bend_directions = [clamp(float(value), -1.4, 1.4) for value in raw.get("bend_directions", default_bend_directions)]
            joint_count = segment_count - 1
            if (len(segment_scales) != segment_count or len(width_scales) != segment_count
                    or len(segment_color_keys) != segment_count
                    or len(segment_lengths) != segment_count
                    or len(joint_scales) != joint_count
                    or len(bend_profile) != joint_count or len(joint_phase_offsets) != joint_count
                    or len(bend_directions) != joint_count):
                return None
            if any(value <= 0.0 or not math.isfinite(value)
                   for value in segment_scales + width_scales + segment_lengths + joint_scales + bend_profile):
                return None
            if any(not math.isfinite(value) for value in joint_phase_offsets + bend_directions):
                return None
            return {
                "segment_count": segment_count,
                "asset_name": asset_name,
                "split": split,
                "bend": bend,
                "max_stretch": max_stretch,
                "elevated_arc": elevated_arc,
                "proximal_lift": proximal_lift,
                "hairy": hairy,
                "hair_scale": hair_scale,
                "segment_scales": segment_scales,
                "width_scales": width_scales,
                "segment_color_keys": segment_color_keys,
                "joint_color_key": str(raw.get("joint_color_key", "legs")).strip() or "legs",
                "tip_color_key": str(raw.get("tip_color_key", "legs")).strip() or "legs",
                "segment_lengths": segment_lengths,
                "joint_scales": joint_scales,
                "bend_profile": bend_profile,
                "joint_phase_offsets": joint_phase_offsets,
                "bend_directions": bend_directions,
            }
        except (TypeError, ValueError):
            return None

    def _update_spider_joint_articulation(self, dt: float, config: dict) -> None:
        """Give each interior joint its own smooth flex rhythm.

        Foot contacts and body pose are mechanical state. Joint bend is a
        separate degree of freedom: it follows the leg's phase and position in
        the chain, then folds more strongly during swing. This prevents an
        added knuckle from merely duplicating one elbow angle everywhere.
        """
        chain = self._sprite_leg_chain_config()
        if chain is None:
            return
        joint_count = chain["segment_count"] - 1
        profile = chain["bend_profile"]
        phase_offsets = chain["joint_phase_offsets"]
        for leg in self.legs:
            if len(leg.joint_bends) != joint_count:
                leg.joint_bends = [0.88 + 0.08 * index for index in range(joint_count)]
            if leg.stepping:
                step_t = clamp(leg.step_timer / max(0.001, leg.step_duration), 0.0, 1.0)
                swing = math.sin(math.pi * step_t)
            else:
                swing = 0.0
            gait_angle = self._lively_gait_phase * math.tau + leg.joint_phase
            for index in range(joint_count):
                # Joints lead/lag within one leg. The middle joint folds most
                # during a lifted swing; distal joints remain restrained.
                joint_angle = gait_angle + phase_offsets[index]
                joint_wave = (
                    math.sin(joint_angle * (1.0 + index * 0.13)) * 0.72
                    + math.sin(joint_angle * 1.37 + index * 0.81) * 0.28
                )
                target = 0.86 + 0.11 * joint_wave + 0.08 * profile[index]
                if leg.stepping:
                    target += swing * (0.16 + 0.07 * (index == joint_count // 2))
                target = clamp(target, 0.55, 1.28)
                leg.joint_bends[index] += (target - leg.joint_bends[index]) * (1.0 - math.exp(-dt * 12.0))

    def _safe_sprite_leg_foot(self, leg: LegState, x: float, y: float, chain_config=None) -> Tuple[float, float]:
        """Keep rendered sprite feet inside the same reach envelope as the IK knee."""
        spider_gait = self._spider_gait_config()
        if (
            spider_gait is not None
            and not leg.stepping
            and not leg.pending_step
            and self.held_release_timer <= 0.0
        ):
            # Do not move a planted contact point just because the attachment
            # moved with the body.  Corrective steps are scheduled by the gait
            # controller before the foot becomes impossible.
            return x, y
        safe_x, safe_y = self._limit_world_point_to_leg_reach(leg, x, y, visual=True)
        if chain_config is None:
            return safe_x, safe_y
        d = leg.definition
        segment_reach = (float(d.get("upper_len", 0.85)) + float(d.get("lower_len", 1.05))) * self.size
        max_reach = min(self._leg_max_reach(leg, visual=True), segment_reach * chain_config["max_stretch"])
        ax, ay = self._leg_attach(leg)
        dx, dy = safe_x - ax, safe_y - ay
        distance = math.hypot(dx, dy)
        if distance > max_reach and distance > 1e-5:
            scale = max_reach / distance
            safe_x, safe_y = ax + dx * scale, ay + dy * scale
        return safe_x, safe_y

    def _sprite_leg_chain_points(self, leg: LegState, ax: float, ay: float,
                                 fx: float, fy: float, chain_config: dict):
        """Solve a bounded four- or five-segment leg as one coordinated chain.

        Memoised for the frame being drawn. A procedural spider solves every leg
        twice per frame -- once for the leg itself and once for the sockets and
        knuckles drawn over it -- from identical inputs. The key is those exact
        inputs, so a cached answer is the answer the solve would have produced,
        and a copy is handed out so a caller cannot corrupt the second use.

        The old renderer solved one large outward knee and then subdivided the
        remaining line.  That made the extra knuckle decorative: all joints
        inherited the same forced bow, and a long foot catch could stretch the
        first segment far past its own joint. These points now start from the
        independently phased joint pose, then pass through a 2D maximum-length
        chain solve. The root and foot stay fixed while every knuckle receives
        its own anatomical length limit.
        """
        cache_key = (id(leg), ax, ay, fx, fy)
        cached = self._chain_points_cache.get(cache_key)
        if cached is not None:
            return list(cached)

        a_f, a_s = self._world_to_body_local(ax, ay)
        f_f, f_s = self._world_to_body_local(fx, fy)
        direct = max(1e-4, math.hypot(f_f - a_f, f_s - a_s))
        seg_scales = chain_config["segment_scales"]
        total = max(1e-4, sum(seg_scales))
        fractions = []
        running = 0.0
        for scale in seg_scales[:-1]:
            running += scale / total
            fractions.append(running)

        _, _, rx, ry = self._basis()
        side = self._side_sign(leg.definition.get("side", "right"))
        front = self._leg_front_factor(leg)
        # The foot has already been reach-limited on the ground plane.  Keep
        # the joint fold modest at rest and let a lifted leg tuck a little more.
        bend_base = self.size * chain_config["bend"] * (0.78 + 0.34 * leg.lift)
        forward_bias = self.size * 0.075 * front * (0.72 + 0.28 * leg.lift)
        profiles = chain_config["bend_profile"]
        directions = chain_config["bend_directions"]
        catch_strength = 0.0
        if not self.dragging and self.catch_blend > 0.001 and front > 0.15:
            catch_strength = clamp(
                self.catch_blend * (front - 0.15) / 0.85,
                0.0, 1.0,
            )
        # Catching must flex through the individual knuckles. This modest arc
        # is intentionally strongest at the first two interior joints, then
        # tapers toward the tarsus so a cursor target cannot make a straight,
        # rubbery front limb.
        catch_bend_profile = [0.55, 1.00, 0.72, 0.34]
        # Parsed and kept for the models that declare them, but since DC-69
        # nothing displaces the seed pose by them: they only ever fed a
        # screen-space offset, and a screen-space offset is what made a leg's
        # pose depend on which way its owner happened to be facing. Left in
        # the config rather than stripped from eight model files, and read
        # here so the unused-name check stays honest about it.
        _elevated_arc_unused = chain_config["elevated_arc"]
        _proximal_lift_unused = chain_config["proximal_lift"]
        if self.dragging:
            # _leg_draw_points has already tucked and lowered the endpoint. Do
            # not add the walking rig's high proximal arc on top of that pose.
            # Keep a small gravity sag and let faster hand motion increase the
            # relaxed bend instead of making every leg point at one center.
            held_speed01 = clamp(self.current_speed / 260.0, 0.0, 1.0)
            held_response = clamp(float(getattr(self, "held_drag_response", 0.0)), 0.0, 1.0)
            # Fold the suspended chain through its knuckles. The walking pose
            # is deliberately restrained, but a carried spider needs a visible
            # soft knee instead of a straight radial spoke. The speed term is
            # the one the comment above has always promised and the code never
            # applied: it was computed, left unused, and reported by ruff as
            # B6. It carries about half the weight of the drag response, which
            # is the ratio the suspended-knee helper below already uses.
            bend_base *= 3.40 + held_response * 0.90 + held_speed01 * 0.45
            forward_bias *= 0.35
            carry_wave_amount = clamp(
                float(chain_config.get("carry_joint_wave", 0.42)), 0.0, 0.65
            )
            carry_wave_phase = float(getattr(self, "held_pose_clock", 0.0)) * 3.0
            carry_wave_phase += float(getattr(leg, "phase_seed", 0.0)) * 0.38
        else:
            held_response = 0.0
            carry_wave_amount = 0.0
            carry_wave_phase = 0.0
        joint_bends = leg.joint_bends
        if len(joint_bends) != len(profiles):
            joint_bends = [0.90 + 0.06 * index for index in range(len(profiles))]

        upper = max(self.size * 0.22, float(leg.definition.get("upper_len", 0.85)) * self.size)
        lower = max(self.size * 0.22, float(leg.definition.get("lower_len", 1.05)) * self.size)
        # Width scales are deliberately separate from anatomical lengths.
        # Older configs only had segment_scales, which made presentation values
        # double as geometry and allowed a proximal segment to grow too long.
        length_weights = chain_config["segment_lengths"]
        length_total = max(1e-4, sum(length_weights))
        segment_limits = [
            (upper + lower) * chain_config["max_stretch"] * weight / length_total
            for weight in length_weights
        ]

        def constrain_to_segment_limits(seed_points):
            """Project only overlong links while preserving both contacts."""
            points = list(seed_points)
            target = (fx, fy)

            def project(point, other, limit):
                dx = point[0] - other[0]
                dy = point[1] - other[1]
                distance = math.hypot(dx, dy)
                if distance <= limit or distance < 1e-5:
                    return point
                return (other[0] + dx * limit / distance,
                        other[1] + dy * limit / distance)

            # This is a max-length chain, so short links retain the curved
            # seed pose instead of being forced into a straight IK line.
            # Alternating projections keep the root and foot contacts stable.
            for _ in range(10):
                points[0] = (ax, ay)
                for index, limit in enumerate(segment_limits):
                    points[index + 1] = project(points[index + 1], points[index], limit)
                points[-1] = target
                for index in range(len(segment_limits) - 1, -1, -1):
                    points[index] = project(points[index], points[index + 1], segment_limits[index])
            points[0] = (ax, ay)
            points[-1] = target
            return points

        def build(bend_scale: float):
            seed_points = [(ax, ay)]
            for index, fraction in enumerate(fractions):
                bend = bend_base * profiles[index] * directions[index] * joint_bends[index] * bend_scale
                if self.dragging:
                    # A carried leg is not frozen at one folded silhouette. Let
                    # each knuckle alternately loosen and tighten in response
                    # to the hand's movement, with the middle joints doing most
                    # of the visible work.
                    joint_wave = math.sin(
                        carry_wave_phase + index * 0.74
                    )
                    joint_gain = 0.55 + 0.14 * min(index, 2)
                    bend *= 1.0 + carry_wave_amount * joint_gain * (
                        0.24 + 0.76 * held_response
                    ) * joint_wave
                if catch_strength > 0.0:
                    bend += (
                        self.size * 0.075 * catch_strength
                        * catch_bend_profile[index]
                    )
                local_f = a_f + (f_f - a_f) * fraction + forward_bias * profiles[index]
                local_s = a_s + (f_s - a_s) * fraction + side * bend
                point_x, point_y = self._body_local_to_world(local_f, local_s)
                if self.dragging:
                    # Screen-down gravity acts on every suspended joint.  The
                    # sag follows each leg's own lane; it is not a pull toward
                    # a central point below the body.
                    point_y += self.size * (0.105 + held_response * 0.075) * math.sin(math.pi * fraction)
                    point_y += self.size * carry_wave_amount * (0.012 + held_response * 0.028) * math.sin(
                        carry_wave_phase + index * 0.86
                    )
                    # A relaxed suspended leg folds slightly back toward its
                    # own body lane before the distal links fall away again.
                    local_s -= side * self.size * (0.055 + held_response * 0.025) * math.sin(math.pi * fraction)
                # DC-67: the knee arc bows *outward from the body*, not up
                # the screen.
                #
                # `rise` used to be subtracted from point_y directly, which is
                # a lift in screen space. Screen-up is not away from the body:
                # for a spider walking horizontally it is outward for the four
                # legs on one side and straight across the shell for the four
                # on the other. Measured on a neutral stance with the heading
                # set by hand, so there is no randomness in it at all:
                #
                #   heading   0 deg : left legs +0.43..+0.46 outboard,
                #                     right legs -0.09..-0.12 -- i.e. inboard,
                #                     the first joint tucked under the shell
                #   heading  90 deg : both sides +0.14..+0.19, symmetric
                #   heading 180 deg : the same fault, mirrored onto the left
                #
                # Legs are drawn beneath the body, so a tucked proximal joint
                # is simply not visible: the spider looked like it had legs
                # coming out from under itself, which is what the owner saw.
                # Only a spider walking straight up or down the screen was
                # ever drawn correctly.
                #
                # Bowing along the body's own outward axis gives every leg the
                # same arc at every heading, and keeps the knee-high tarantula
                # silhouette the arc was added for.
                # DC-69: the seed joint carries no screen-space offset.
                #
                # There used to be a `rise` here, subtracted from point_y so a
                # joint read as carried high off the floor. Up the screen is
                # not a direction the body knows about, so what that actually
                # did depended on which way the spider was walking: outward
                # for one flank and straight across the shell for the other,
                # which is how four of the eight legs came to be drawn
                # underneath the body (DC-67).
                #
                # DC-67 replaced it with a bow along the body's own outward
                # axis. That fixed the symmetry and then kept going: the first
                # joint bowed 0.45 body-widths at *every* heading, where the
                # original managed 0.15 at the vertical ones. Three times
                # wider, and the owner's verdict on seeing it walk was "thats
                # not how they supposed to lok like".
                #
                # Measured at heading 0 / 90, first-joint bow in body-widths:
                #
                #   original   0.43 one flank, -0.12 the other (tucked) / 0.15
                #   DC-67      0.43..0.46 / 0.43..0.46
                #   DC-69      0.15..0.19 / 0.15..0.19
                #
                # The bend below is body-relative and already gives the knee
                # its fold. Dropping the screen-space term is what finally
                # makes the pose the same whatever the compass says, which is
                # the property that was wrong underneath both bugs.
                seed_points.append((point_x, point_y))
            seed_points.append((fx, fy))
            points = constrain_to_segment_limits(seed_points)
            path_len = sum(math.hypot(points[i + 1][0] - points[i][0],
                                      points[i + 1][1] - points[i][1])
                           for i in range(len(points) - 1))
            return points, path_len

        points, path_len = build(1.0)
        # A bent chain may be a little longer than the direct anchor-to-foot
        # line, but never enough to look like a stretched rubber limb.
        path_ratio = 1.56 if self.dragging else 1.28
        path_budget = min((upper + lower) * chain_config["max_stretch"], direct * path_ratio)
        if path_len > path_budget:
            extra = max(1e-4, path_len - direct)
            bend_scale = clamp((path_budget - direct) / extra, 0.0, 1.0)
            points, _ = build(bend_scale)
        self._chain_points_cache[cache_key] = points
        return list(points)

    def _draw_sprite_segment(self, painter, pixmap, x1: float, y1: float, x2: float, y2: float, thickness: float, opacity: float = 1.0):

        if pixmap is None or pixmap.isNull():
            return
        dx = x2 - x1
        dy = y2 - y1
        length = math.hypot(dx, dy)
        if length < 0.5:
            return
        angle = math.degrees(math.atan2(dy, dx))
        painter.save()
        # Preserve any parent opacity, including Camouflage's whole-spider fade.
        # The previous code set segment opacity absolutely, so sprite-rig legs
        # stayed visible while the body disappeared.
        try:
            parent_opacity = float(painter.opacity())
        except Exception:
            parent_opacity = 1.0
        painter.setOpacity(parent_opacity * max(0.0, min(1.0, opacity)))
        painter.translate((x1 + x2) * 0.5, (y1 + y2) * 0.5)
        painter.rotate(angle)
        rect = QRectF(-length * 0.5, -thickness * 0.5, length, thickness)
        painter.drawPixmap(rect, pixmap, QRectF(pixmap.rect()))
        painter.restore()

    def _solve_knee(self, ax: float, ay: float, fx: float, fy: float, leg: LegState) -> Tuple[float, float]:
        """Stable anatomical knee solver.

        This solves in spider body-local space so the knee always bows outward
        from the body.  It is intentionally not a strict two-bone IK clamp: the
        sprite/procedural rig is stylised, and a strict IK branch was causing rear
        legs to lock and some legs to fold to the wrong side during turns.
        """
        d = leg.definition
        sign = self._side_sign(d.get("side", "right"))
        spider_gait = self._spider_gait_config()
        if spider_gait is None or leg.stepping or leg.pending_step:
            fx, fy = self._limit_world_point_to_leg_reach(leg, fx, fy, visual=True)
        a_f, a_s = self._world_to_body_local(ax, ay)
        f_f, f_s = self._world_to_body_local(fx, fy)

        if self.dragging:
            # A held spider has no stance to brace against. Use a small,
            # phase-varied bend around the midpoint instead of the strong
            # outward walking-knee rule, so every leg hangs loosely in the air.
            phase = float(getattr(leg, "phase_seed", 0.0))
            held_speed01 = clamp(self.current_speed / 260.0, 0.0, 1.0)
            held_response = clamp(float(getattr(self, "held_drag_response", 0.0)), 0.0, 1.0)
            relax = 0.105 + held_response * 0.035 + 0.018 * (0.5 + 0.5 * math.sin(phase)) + held_speed01 * 0.020
            knee_f = a_f + (f_f - a_f) * (0.44 - held_speed01 * 0.035)
            knee_s = a_s + (f_s - a_s) * 0.48 + sign * self.size * relax
            knee_x, knee_y = self._body_local_to_world(knee_f, knee_s)
            return knee_x, knee_y + self.size * (0.040 + held_speed01 * 0.045)

        rest_f = float(d.get("rest_forward", 0.0)) * self.size
        rest_s = abs(float(d.get("rest_side", 1.0)) * self.size)
        attach_s = abs(float(d.get("attach_side", 0.30)) * self.size)
        l1 = float(d.get("upper_len", 0.8)) * self.size

        # Strong outward bow. Never let the knee fold across the body axis.
        outward_side = max(abs(a_s), abs(f_s), attach_s + self.size * 0.12, rest_s * 0.60)
        outward_side += l1 * (0.10 + 0.08 * leg.lift)
        knee_s = sign * outward_side

        # Front legs bend forward, rear legs bend backward, middle legs stay near their lane.
        lane_pull = clamp(rest_f - a_f, -self.size * 0.92, self.size * 0.92)
        knee_f = (a_f + f_f) * 0.50 + lane_pull * 0.38
        if rest_f > self.size * 0.45:
            knee_f = max(knee_f, a_f + self.size * 0.20)
        elif rest_f < -self.size * 0.45:
            knee_f = min(knee_f, a_f - self.size * 0.20)

        # Keep knee and foot in the same broad anatomical lane.
        forward_min = min(a_f, rest_f, f_f) - self.size * 0.56
        forward_max = max(a_f, rest_f, f_f) + self.size * 0.56
        knee_f = clamp(knee_f, forward_min, forward_max)
        return self._body_local_to_world(knee_f, knee_s)

    def _render_procedural(self, painter) -> None:

        startle = self._startle_amount()
        startle_highlight = self._startle_highlight_active(startle)
        motion_startle = 0.0 if self.dragging else startle
        tremble_x = math.sin(self.breath_phase * 17.0 + self.startle_phase) * self.size * 0.018 * motion_startle
        tremble_y = math.cos(self.breath_phase * 19.0 + self.startle_phase * 0.7) * self.size * 0.018 * motion_startle
        leg_y_off = -self.jump_z
        jz_shadow = clamp(self.jump_z / max(1.0, self.size), 0.0, 3.0)
        shadow_shrink = 1.0 / (1.0 + jz_shadow * 0.55)

        painter.setPen(Qt.NoPen)
        painter.setBrush(QBrush(self._qcolor("legs", max(10, int(30 * shadow_shrink)))))
        painter.drawEllipse(QPointF(self.x, self.y + self.size * 0.14), self.size * 0.78 * shadow_shrink, self.size * 0.42 * shadow_shrink)

        leg_width_scale = float(self._appearance("leg_thickness", 1.0))
        fluffiness = clamp(float(self._appearance("fluffiness", 0.0)), 0.0, 1.0)
        segmented_legs = bool(self._appearance("segmented_legs", False))
        show_joint_nodes = bool(self._appearance("show_joint_nodes", False))
        joint_node_scale = float(self._appearance("joint_node_scale", 1.0))
        coxa_width_scale = float(self._appearance("coxa_thickness_scale", 1.0))
        chain_config = self._sprite_leg_chain_config()
        # DC-71 measured about 60% of render as Qt turning each stroked path
        # into an outline polygon, a cost that is per primitive and takes no
        # account of size. So detail that cannot be seen is not merely wasted,
        # it is the most expensive kind of waste. Decided once per spider from
        # its resting leg width, not per frame from the live one, so the level
        # cannot flicker while it walks.
        resting_leg_width = max(1.4, self.size * 0.066 * leg_width_scale)
        detail = hair_detail(resting_leg_width, chain_config)
        # Hoisted out of the loop as well as gated: the over-stroke colour does
        # not vary between segments or between legs, but the shipped code
        # rebuilt it for every one of the forty segments a tarantula has.
        hair_color = None
        if detail.separate_hair or detail.merge_hair:
            hair_color = self._qcolor_triplet(
                self._appearance_color("fluff_color", "highlight"),
                int(72 + chain_config["hair_scale"] * 90),
            )

        # Every leg stroke and joint node goes into one batch and is issued as
        # a handful of paths once the loop is done. Profiling put drawEllipse
        # at 10% of render and drawLine at 9%, with setPen almost free, so the
        # win is in making fewer draws -- not fewer pen changes.
        batch = LegBatch(
            width_step=render_batch.LEG_WIDTH_STEP,
            direct=None if render_batch.BATCH_LEGS else painter,
        )
        # Legs first, underneath body. Segment thickness tapers from coxa to tarsus.
        for leg in self.legs:
            ax, ay, foot_x, foot_y = self._leg_draw_points(leg)
            if chain_config and segmented_legs:
                # Procedural tarantula legs need the same post-drag endpoint
                # guard as sprite-rig legs. Without this, a released carried
                # contact could flash as a long raw leg for one paint cycle.
                foot_x, foot_y = self._safe_sprite_leg_foot(
                    leg, foot_x, foot_y, chain_config
                )
            kx, ky = self._solve_knee(ax, ay, foot_x, foot_y, leg)
            # Lift the swinging / reaching leg up off the ground (lively gait).
            # Pure draw-time screen offset applied after the knee solve so the
            # ground-plane IK stays correct; the foot also tucks slightly toward
            # the body as it rises, like a real leg flexing through its arc.
            lift_px = self._leg_lift_px(leg)
            if lift_px > 0.0:
                flex = leg.lift * 0.08
                foot_x += (ax - foot_x) * flex
                foot_y += (ay - foot_y) * flex
                foot_y -= lift_px
                ky -= lift_px * 0.45
            _, _, rx, ry = self._basis()
            side = self._side_sign(leg.definition.get("side", "right"))
            coxa_len = float(leg.definition.get("coxa_len", 0.20)) * self.size
            coxa_x = ax + rx * side * coxa_len + (kx - ax) * 0.08
            coxa_y = ay + ry * side * coxa_len + (ky - ay) * 0.08
            tarsus_x = kx + (foot_x - kx) * 0.72
            tarsus_y = ky + (foot_y - ky) * 0.72
            if leg_y_off:
                ay += leg_y_off
                ky += leg_y_off
                coxa_y += leg_y_off
                tarsus_y += leg_y_off
                foot_y += leg_y_off

            chain_points = None
            if chain_config and segmented_legs:
                # Procedural models use the same multi-joint solver as sprite
                # models, but draw the segments as tapered lines. This keeps
                # the model's joint definition and its visible articulation
                # identical without requiring copied bitmap assets.
                chain_points = self._sprite_leg_chain_points(leg, ax, ay, foot_x, foot_y, chain_config)
                coxa_x, coxa_y = chain_points[1]
                kx, ky = chain_points[2]
                tarsus_x, tarsus_y = chain_points[3]

            base_width = max(1.4, self.size * (0.066 + leg.lift * 0.016) * leg_width_scale) * (1.0 + startle * 0.10)
            coxa_width = max(1.2, base_width * 1.08 * coxa_width_scale)
            femur_width = max(1.0, base_width * 0.98)
            tibia_width = max(1.0, base_width * 0.78)
            tarsus_width = max(1.0, base_width * 0.42)
            # A segmented spider must remain a set of separated rods. The old
            # fuzzy spline filled the gaps between joints and made long legs
            # look like bat wings, so reserve it for legacy unsegmented legs.
            if fluffiness > 0.0 and chain_points is None:
                # The chain_points branch that used to sit here was dead: the
                # whole block only runs when chain_points is None.
                fuzzy_path = QPainterPath(QPointF(ax, ay))
                fuzzy_path.cubicTo(QPointF(coxa_x, coxa_y), QPointF(coxa_x, coxa_y), QPointF(kx, ky))
                fuzzy_path.lineTo(QPointF(tarsus_x, tarsus_y))
                fuzzy_path.lineTo(QPointF(foot_x, foot_y))
                batch.add_path(LAYER_FUZZ,
                               self._qcolor("highlight", int(40 + fluffiness * 50)),
                               base_width * (1.4 + fluffiness * 0.6),
                               fuzzy_path)

            # Leg motion is communicated by the articulated pose. Do not make
            # the swinging leg glow; highlight remains reserved for a genuine
            # startled state shared by the whole creature.
            color = self._qcolor("highlight" if startle_highlight else "legs", 230 if startle_highlight else 245)
            if chain_points is not None:
                # Keep a distinct narrow metatarsus between the tibia and toe.
                # The previous four-entry list silently dropped the fifth
                # point when Chosen One switched to a five-segment chain.
                width_bases = [
                    coxa_width,
                    femur_width,
                    tibia_width,
                    max(tarsus_width, tibia_width * 0.72),
                    tarsus_width,
                ]
                scales = chain_config["width_scales"]
                segment_color_keys = chain_config.get(
                    "segment_color_keys", ["legs"] * (len(chain_points) - 1)
                )
                chain_widths = [width_bases[index] * scales[index] for index in range(len(chain_points) - 1)]
                for index, width in enumerate(chain_widths):
                    start_x, start_y = chain_points[index]
                    end_x, end_y = chain_points[index + 1]
                    if detail.separate_hair:
                        batch.line(LAYER_HAIR, hair_color,
                                   hair_pen_width(width, chain_config["hair_scale"]),
                                   start_x, start_y, end_x, end_y)
                    if startle_highlight:
                        segment_color = self._qcolor("highlight", 230)
                    else:
                        segment_color = self._qcolor(
                            segment_color_keys[index] if index < len(segment_color_keys) else "legs",
                            245 if index < len(chain_widths) - 1 else 220,
                        )
                    stroke_width = width
                    if detail.merge_hair:
                        # One stroke doing the work of two: the hair's width,
                        # so the leg keeps its weight, warmed toward the hair's
                        # colour so it keeps its tone.
                        stroke_width = hair_pen_width(width, chain_config["hair_scale"])
                        segment_color = _mix(segment_color, hair_color, MERGED_HAIR_TINT)
                    batch.line(LAYER_SEGMENT, segment_color, stroke_width,
                               start_x, start_y, end_x, end_y)
            elif segmented_legs:
                batch.line(LAYER_SEGMENT, color, coxa_width, ax, ay, coxa_x, coxa_y)
                batch.line(LAYER_SEGMENT, color, femur_width, coxa_x, coxa_y, kx, ky)
                batch.line(LAYER_SEGMENT, color, tibia_width, kx, ky, tarsus_x, tarsus_y)
                batch.line(LAYER_SEGMENT, self._qcolor("legs", 210), tarsus_width,
                           tarsus_x, tarsus_y, foot_x, foot_y)
            else:
                path = QPainterPath(QPointF(ax, ay))
                path.cubicTo(QPointF(coxa_x, coxa_y), QPointF(coxa_x, coxa_y), QPointF(kx, ky))
                path.lineTo(QPointF(tarsus_x, tarsus_y))
                path.lineTo(QPointF(foot_x, foot_y))
                batch.add_path(LAYER_SEGMENT, color, base_width, path)
                batch.line(LAYER_SEGMENT, self._qcolor("legs", 210),
                           max(1.0, base_width * 0.48),
                           tarsus_x, tarsus_y, foot_x, foot_y)
            if show_joint_nodes:
                # These are the draws the profile singled out. An ellipse's
                # radius lives inside the path, so every node sharing a colour
                # batches into one fill whatever size each one is.
                joint_color = self._qcolor(
                    chain_config.get("joint_color_key", "legs") if chain_config else "legs",
                    225,
                )
                node_r = max(1.0, base_width * 0.24 * joint_node_scale)
                if chain_points is not None:
                    joint_scales = chain_config["joint_scales"]
                    joints = chain_points[1:-1]
                    # A node whose radius is smaller than the half-width of
                    # the stroke it sits on is not a joint, it is a stain
                    # inside the leg. On a tarantula that is all four of them
                    # at every size the model reaches.
                    shown = []
                    for joint_index, (joint_x, joint_y) in enumerate(joints):
                        scale = joint_scales[joint_index]
                        radius = node_r * scale * (1.12 if joint_index == 1 else 0.96)
                        if not joint_node_shows(radius, *chain_widths[joint_index:joint_index + 2]):
                            continue
                        shown.append((joint_index, joint_x, joint_y))
                        batch.dot(LAYER_JOINT, joint_color, joint_x, joint_y, radius)
                    # A small core on every joint makes all four independently
                    # animated pivots readable at desktop scale. The patella
                    # and distal hinge receive a slightly stronger core.
                    if shown:
                        core_color = self._qcolor(chain_config.get("joint_color_key", "legs"), 230)
                        for joint_index, joint_x, joint_y in shown:
                            core_scale = 0.48 if joint_index in (1, len(joints) - 1) else 0.34
                            batch.dot(LAYER_CORE, core_color, joint_x, joint_y,
                                      node_r * core_scale)
                else:
                    batch.dot(LAYER_JOINT, joint_color, coxa_x, coxa_y, node_r * 1.05)
                    batch.dot(LAYER_JOINT, joint_color, kx, ky, node_r * 1.30)
                    batch.dot(LAYER_JOINT, joint_color, tarsus_x, tarsus_y, node_r * 0.98)
                    batch.dot(LAYER_CORE, self._qcolor("highlight", 210),
                              kx, ky, node_r * 0.60)
            foot_key = "highlight" if startle_highlight else (
                chain_config.get("tip_color_key", "legs") if chain_config else "legs"
            )
            batch.point(LAYER_FOOT, self._qcolor(foot_key, 220),
                        max(1.0, tarsus_width * 0.8), foot_x, foot_y)

        batch.flush(painter)

        crouch_drop = self.crouch * self.size * 0.06
        painter.save()
        # DC-59: a landed blow throws the body over its planted feet, so the
        # legs stretch behind it. The feet are solved in world space and are
        # not moved, which is the whole effect.
        lunge_x, lunge_y = self.combat_body_offset()
        painter.translate(self.x + tremble_x + lunge_x,
                          self.y + self.body_bob + tremble_y - self.jump_z + crouch_drop + lunge_y)
        painter.rotate(math.degrees(self.heading))
        # Snowpuff-2's stance solver owns the body pose; a second cosmetic
        # rotation here would rotate the shell away from the leg roots.
        if self._spider_gait_config() is None:
            painter.rotate(math.degrees(self.body_wiggle))
        jz_body = clamp(self.jump_z / max(1.0, self.size), 0.0, 3.0)
        jump_scale = 1.0 + jz_body * 0.12
        visual_squash = 1.0 if self.state == "Roll" or abs(self.roll_spin) > 1e-4 else self.squash
        vfac = clamp(visual_squash * (1.0 - self.crouch * 0.14), 0.55, 1.2)
        vroot = math.sqrt(vfac)
        painter.scale(jump_scale / vroot, jump_scale * vroot)
        body = self._qcolor("body", 255)
        highlight = self._qcolor("highlight", int(self._appearance("highlight_alpha", 145)))
        leg_color = self._qcolor("legs", 255)
        fluff_color = self._qcolor_triplet(self._appearance_color("fluff_color", "highlight"), int(38 + fluffiness * 70))

        abdomen_scale = self._appearance("abdomen_scale", [0.92, 0.84])
        ceph_scale = self._appearance("cephalothorax_scale", [0.70, 0.66])
        abdomen_offset_x = float(self._appearance("abdomen_offset_x", -0.18)) * self.size
        ceph_offset_x = float(self._appearance("cephalothorax_offset_x", 0.40)) * self.size
        pedicel_cfg = self._appearance("pedicel", {})
        head_cfg = self._appearance("head", {})
        pedipalp_scale = float(self._appearance("pedipalp_scale", 1.0))
        eye_scale = float(self._appearance("eye_scale", 1.0))
        eye_count = max(2, int(self._appearance("eye_count", 2)))

        abdomen_w = self.size * (float(abdomen_scale[0]) * (1.0 - self.abdomen_pulse * 0.35)) * (1.0 + startle * 0.055)
        abdomen_h = self.size * (float(abdomen_scale[1]) * (1.0 + self.abdomen_pulse)) * (1.0 - startle * 0.060)
        ceph_w = self.size * (float(ceph_scale[0]) * (1.0 + self.ceph_pulse * 0.25)) * (1.0 + startle * 0.075)
        ceph_h = self.size * (float(ceph_scale[1]) * (1.0 + self.ceph_pulse)) * (1.0 + startle * 0.030)
        ceph_w *= (1.0 + self.rear * 0.12)
        ceph_h *= (1.0 + self.rear * 0.12)
        ceph_offset_x += self.rear * self.size * 0.05
        # Squared up, the front half rises and the abdomen drops behind it.
        # `rear` on its own only scaled the cephalothorax by 12%, which is
        # invisible at spider size -- rendered and looked at.
        stance = getattr(self, "combat_stance", 0.0)
        if stance > 0.01:
            ceph_offset_x += stance * self.size * 0.16
            ceph_w *= (1.0 + stance * 0.10)
            ceph_h *= (1.0 + stance * 0.10)
            abdomen_offset_x -= stance * self.size * 0.10
        abdo_wag = self.abdomen_wag * self.size * 0.45

        pedicel_enabled = isinstance(pedicel_cfg, dict) and bool(pedicel_cfg.get("enabled", True))
        pedicel_offset_x = float(pedicel_cfg.get("offset_x", -0.04)) * self.size if pedicel_enabled else 0.0
        pedicel_scale = pedicel_cfg.get("scale", [0.20, 0.12]) if pedicel_enabled else [0.0, 0.0]
        pedicel_w = self.size * float(pedicel_scale[0]) if len(pedicel_scale) >= 2 else 0.0
        pedicel_h = self.size * float(pedicel_scale[1]) if len(pedicel_scale) >= 2 else 0.0
        head_enabled = isinstance(head_cfg, dict) and bool(head_cfg.get("enabled", True))
        head_offset_x = float(head_cfg.get("offset_x", 0.49)) * self.size if head_enabled else ceph_offset_x
        head_scale = head_cfg.get("scale", [0.22, 0.20]) if head_enabled else [0.0, 0.0]
        head_w = self.size * float(head_scale[0]) if len(head_scale) >= 2 else 0.0
        head_h = self.size * float(head_scale[1]) if len(head_scale) >= 2 else 0.0

        if fluffiness > 0.0:
            painter.setPen(Qt.NoPen)
            painter.setBrush(QBrush(fluff_color))
            for ox, oy, sx, sy in [
                (abdomen_offset_x - self.size * 0.03, -self.size * 0.03, abdomen_w * 0.60, abdomen_h * 0.54),
                (abdomen_offset_x - self.size * 0.13, self.size * 0.12, abdomen_w * 0.58, abdomen_h * 0.46),
                (ceph_offset_x + self.size * 0.01, -self.size * 0.06, ceph_w * 0.62, ceph_h * 0.52),
                (ceph_offset_x + self.size * 0.02, self.size * 0.08, ceph_w * 0.54, ceph_h * 0.42),
                (head_offset_x, -self.size * 0.02, head_w * 0.66, head_h * 0.56),
            ]:
                painter.drawEllipse(QRectF(ox - sx * 0.5, oy - sy * 0.5, sx, sy))

        painter.setPen(QPen(leg_color, max(1.0, self.size * 0.035), Qt.SolidLine, Qt.RoundCap))
        painter.setBrush(QBrush(body))
        painter.drawEllipse(QRectF(abdomen_offset_x - abdomen_w * 0.5, -abdomen_h * 0.5 + abdo_wag, abdomen_w, abdomen_h))
        if pedicel_enabled and pedicel_w > 0.0 and pedicel_h > 0.0:
            pedicel_color = self._qcolor(str(pedicel_cfg.get("color_key", "body")), 255)
            pedicel_outline = self._qcolor(str(pedicel_cfg.get("outline_key", "legs")), 210)
            pedicel_outline_width = max(1.0, self.size * clamp(float(pedicel_cfg.get("outline_width", 0.025)), 0.01, 0.07))
            painter.setPen(QPen(pedicel_outline, pedicel_outline_width, Qt.SolidLine, Qt.RoundCap))
            painter.setBrush(QBrush(pedicel_color))
            painter.drawEllipse(QRectF(pedicel_offset_x - pedicel_w * 0.5, -pedicel_h * 0.5, pedicel_w, pedicel_h))
        painter.drawEllipse(QRectF(ceph_offset_x - ceph_w * 0.5, -ceph_h * 0.5, ceph_w, ceph_h))
        if head_enabled and head_w > 0.0 and head_h > 0.0:
            head_color = self._qcolor(str(head_cfg.get("color_key", "body")), 255)
            head_outline = self._qcolor(str(head_cfg.get("outline_key", "legs")), 225)
            head_outline_width = max(1.0, self.size * clamp(float(head_cfg.get("outline_width", 0.028)), 0.01, 0.07))
            painter.setPen(QPen(head_outline, head_outline_width, Qt.SolidLine, Qt.RoundCap))
            painter.setBrush(QBrush(head_color))
            painter.drawEllipse(QRectF(head_offset_x - head_w * 0.5, -head_h * 0.5, head_w, head_h))
        painter.setPen(Qt.NoPen)
        painter.setBrush(QBrush(highlight))
        painter.drawEllipse(QRectF(abdomen_offset_x - abdomen_w * 0.18, -abdomen_h * 0.28 + abdo_wag, abdomen_w * 0.28, abdomen_h * 0.18))
        painter.drawEllipse(QRectF(ceph_offset_x - ceph_w * 0.10, -ceph_h * 0.22, ceph_w * 0.20, ceph_h * 0.13))
        if head_enabled and head_w > 0.0 and head_h > 0.0:
            head_highlight = self._qcolor(
                str(head_cfg.get("highlight_key", "highlight")),
                int(head_cfg.get("highlight_alpha", 115)),
            )
            painter.setBrush(QBrush(head_highlight))
            painter.drawEllipse(QRectF(head_offset_x - head_w * 0.18, -head_h * 0.26, head_w * 0.28, head_h * 0.16))

        stripe_color = self._appearance_color("stripe_color", "leg_band") if self._appearance("stripe_color", None) is not None else None
        if stripe_color:
            painter.setBrush(QBrush(self._qcolor_triplet(stripe_color, int(self._appearance("stripe_alpha", 90)))))
            stripe_count = max(1, int(self._appearance("stripe_count", 2)))
            for i in range(stripe_count):
                t = i / max(1, stripe_count - 1)
                cx = abdomen_offset_x - abdomen_w * (0.12 + t * 0.18)
                painter.drawEllipse(QRectF(cx - abdomen_w * 0.08, -abdomen_h * 0.28 + t * abdomen_h * 0.16, abdomen_w * 0.16, abdomen_h * 0.10))

        painter.setPen(QPen(leg_color, max(1.2, self.size * 0.045), Qt.SolidLine, Qt.RoundCap))
        antenna_cfg = self._appearance("antennae", {})
        custom_hand_palps = (
            isinstance(antenna_cfg, dict)
            and str(antenna_cfg.get("style", "")).strip().lower()
            in ("front_leg", "tarantula_front_legs", "tarantula_hand_palps")
        )
        if not custom_hand_palps:
            palps_y = self.size * 0.12 * pedipalp_scale * (1.0 + startle * 0.55)
            painter.drawLine(QPointF(ceph_offset_x + ceph_w * 0.25, -palps_y), QPointF(ceph_offset_x + ceph_w * 0.55, -palps_y * 1.85))
            painter.drawLine(QPointF(ceph_offset_x + ceph_w * 0.25, palps_y), QPointF(ceph_offset_x + ceph_w * 0.55, palps_y * 1.85))

        # The head is a small front lobe on the flat carapace. Keep the eyes
        # and mouthparts there instead of letting them drift over the abdomen.
        if head_enabled and head_w > 0.0 and head_h > 0.0:
            face_forward = float(head_cfg.get("face_forward", 0.10))
            face_side = float(head_cfg.get("face_side", 0.0))
            head_face_x = head_offset_x + head_w * face_forward
            head_face_y = head_h * face_side
        else:
            head_face_x = ceph_offset_x
            head_face_y = 0.0
        aiming = clamp(self.aim_intent, 0.0, 1.0)
        eye_startle = self._eye_startle_amount(startle)
        eye_r = max(1.0, self.size * 0.035 * eye_scale) * (1.0 + eye_startle * 0.72)
        eyes = []
        if eye_count <= 2:
            eyes.append((head_face_x, head_face_y - head_h * 0.16, eye_r))
            eyes.append((head_face_x, head_face_y + head_h * 0.16, eye_r))
        else:
            rows = 2
            cols = max(2, eye_count // 2)
            for r in range(rows):
                for c in range(cols):
                    if r * cols + c >= eye_count:
                        break
                    eyex = head_face_x - head_w * 0.20 + head_w * (0.08 + c * 0.13)
                    eyey = head_face_y + (-0.18 + r * 0.18 + (c % 2) * 0.02) * head_h
                    eyes.append((eyex, eyey, eye_r * (0.85 if c % 2 else 1.0)))
        if head_enabled and head_w > 0.0 and head_h > 0.0:
            fang_color = self._qcolor("legs", 235)
            painter.setPen(QPen(fang_color, max(1.0, self.size * 0.018), Qt.SolidLine, Qt.RoundCap))
            fang_x = head_offset_x + head_w * 0.33
            for side_sign in (-1.0, 1.0):
                painter.drawLine(
                    QPointF(fang_x, side_sign * head_h * 0.13),
                    QPointF(fang_x + head_w * 0.04, side_sign * head_h * 0.42),
                )
        self._draw_eyes(painter, eyes, ceph_offset_x, ceph_w, ceph_h, startle, aiming)
        self._draw_antennae(painter, ceph_offset_x, ceph_w, ceph_h, startle)
        painter.restore()
        # The body above is drawn in a body-local QPainter transform.  Leg
        # connections are computed in world coordinates, so paint them only
        # after leaving that transform.  Drawing them inside it double-applied
        # heading/scale on rotated spiders and made the proximal legs collapse
        # or detach from the carapace.
        self._draw_leg_connections(painter, chain_config)
        self._draw_equipment(painter)

    def _draw_equipment(self, painter) -> None:
        """Draw restrained anatomy-aware armor overlays for equipped items.

        The catalog is useful even before every model has custom armor art. These
        vector accents deliberately follow the spider's own body/leg geometry and
        keep the equipment readable at desktop scale without replacing model art.
        """

        equipped = {item.slot: item for item in equipped_items(self.progression)}
        if not equipped:
            return
        armor_color = self._qcolor("highlight", 175)
        dark_color = self._qcolor("legs", 175)
        painter.save()
        painter.setPen(QPen(armor_color, max(1.0, self.size * 0.035), Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
        painter.setBrush(Qt.NoBrush)
        if "abdomen" in equipped:
            painter.drawEllipse(QRectF(self.x - self.size * 0.52, self.y - self.size * 0.38,
                                       self.size * 0.78, self.size * 0.70))
        if "carapace" in equipped:
            cx, cy = self._body_local_to_world(self.size * 0.34, 0.0)
            painter.drawEllipse(QRectF(cx - self.size * 0.34, cy - self.size * 0.27,
                                       self.size * 0.68, self.size * 0.54))
        if "head" in equipped:
            hx, hy = self._body_local_to_world(self.size * 0.55, 0.0)
            painter.drawEllipse(QRectF(hx - self.size * 0.16, hy - self.size * 0.14,
                                       self.size * 0.32, self.size * 0.28))
        if "legs" in equipped:
            painter.setPen(QPen(armor_color, max(1.0, self.size * 0.055), Qt.SolidLine, Qt.RoundCap))
            for leg in self.legs:
                ax, ay, fx, fy = self._leg_draw_points(leg)
                mx = ax + (fx - ax) * 0.34
                my = ay + (fy - ay) * 0.34
                painter.drawPoint(QPointF(mx, my))
        if "pedipalps" in equipped:
            painter.setPen(QPen(dark_color, max(1.0, self.size * 0.060), Qt.SolidLine, Qt.RoundCap))
            for target in self.antenna_hand_targets:
                if target != (0.0, 0.0):
                    painter.drawPoint(QPointF(*target))
        painter.restore()

    def _leg_front_factor(self, leg: LegState) -> float:
        """How forward-facing a leg is: ~1 front pair, ~0 mid, ~-1 rear."""
        d = leg.definition
        if "rest_forward" in d:
            return clamp(float(d.get("rest_forward", 0.0)), -1.0, 1.0)
        return math.cos(math.radians(float(d.get("attach_angle", 0.0))))

    def _leg_lift_px(self, leg: LegState) -> float:
        """Screen-space height a swinging (or reaching) leg is raised, for the
        lively gait.  Returns 0 for the classic gait so its look is unchanged.

        The swing lift peels a stepping foot off the ground in a visible arc;
        front legs that are reaching out to feel an object rise a little extra.
        """
        if not self._uses_lively_gait():
            return 0.0
        spider_gait = self._spider_gait_config()
        lift_scale = spider_gait["swing_height"] if spider_gait is not None else (0.42 if self._uses_skitter_gait() else 0.55)
        px = leg.lift * self.size * lift_scale
        front = self._leg_front_factor(leg)
        if self.catch_blend > 0.01 and front > 0.15:
            # A catch is an articulated reach, not a straight upward yank of
            # all four front feet. Keep only a small clearance so the chain's
            # knuckles, rather than this endpoint offset, form the bend.
            px += self.catch_blend * clamp((front - 0.15) / 0.85, 0.0, 1.0) * self.size * 0.12
        return px

    def _front_leg_feeler_pose(self, leg: LegState, ax: float, ay: float, foot_x: float, foot_y: float, front: float) -> Tuple[float, float]:
        """Optional model-specific pose bias that lets the front legs replace the
        cartoon antennae.  Only the foremost legs are affected, and only for
        models that opt into it through appearance settings.
        """
        if not bool(self._appearance("front_leg_feeler_mode", False)) or front <= 0.18:
            return foot_x, foot_y

        strength = clamp((front - 0.18) / 0.82, 0.0, 1.0)
        idle_lift = float(self._appearance("front_leg_idle_lift", 0.0)) * self.size * strength
        idle_forward = float(self._appearance("front_leg_idle_forward", 0.0)) * self.size * strength
        sway = float(self._appearance("front_leg_feeler_sway", 0.0)) * self.size * strength

        forward_dx = math.cos(self.heading) * idle_forward
        forward_dy = math.sin(self.heading) * idle_forward
        side_sign = self._side_sign(leg.definition.get("side", "right"))
        side_dx = -math.sin(self.heading) * sway * side_sign
        side_dy = math.cos(self.heading) * sway * side_sign

        # Small organic probing motion: quick little feeler sweeps with pauses.
        # The lively/skitter gait already handles the planted stepping; this is
        # only a presentational bias layered on top.
        feel_clock = self.breath_phase * (2.9 if self._uses_skitter_gait() else 2.1) + leg.phase_seed * 0.9
        pulse = 0.5 + 0.5 * math.sin(feel_clock)
        foot_x += forward_dx * (0.60 + pulse * 0.40) + side_dx * math.sin(feel_clock * 1.8)
        foot_y += forward_dy * (0.60 + pulse * 0.40) + side_dy * math.sin(feel_clock * 1.8)
        foot_y -= idle_lift * (0.65 + pulse * 0.35)

        # Keep the reach believable.
        max_span = self.size * (float(leg.definition.get("reach", 1.8)) + 0.45)
        ddx, ddy = foot_x - ax, foot_y - ay
        dlen = math.hypot(ddx, ddy)
        if dlen > max_span and dlen > 1e-4:
            scale = max_span / dlen
            foot_x = ax + ddx * scale
            foot_y = ay + ddy * scale
        return foot_x, foot_y

    def _picked_up_leg_pose(self, leg: LegState, ax: float, ay: float, foot_x: float, foot_y: float) -> Tuple[float, float]:
        """Let each carried leg sag in its own lane under screen-down gravity."""
        # A carried spider has no useful ground contact, but its legs should
        # remain visible around the body. Gravity is a downward field across
        # the whole lower side of the body, not an attractor at one point. Do
        # not use breath_phase here: the held pose must not tremble.
        phase = float(getattr(leg, "phase_seed", 0.0))
        held_speed01 = clamp(self.current_speed / 260.0, 0.0, 1.0)
        held_response = clamp(float(getattr(self, "held_drag_response", 0.0)), 0.0, 1.0)
        held_clock = float(getattr(self, "held_pose_clock", 0.0))
        settle = clamp(float(getattr(self, "held_leg_relax", 1.0)), 0.0, 1.0)
        d = leg.definition
        try:
            sign = self._side_sign(d.get("side", "right"))
            lane_f = float(d.get("rest_forward", 0.0)) * self.size
            lane_s = float(d.get("rest_side", 1.0)) * self.size * sign
            lane_x, lane_y = self._body_local_to_world(lane_f, lane_s)
        except (TypeError, ValueError):
            lane_x, lane_y = self._leg_ideal_foot(leg)
        # The current foot may still be behind the body after a drag. Gradually
        # replace that history with the leg's own body-relative lane, so a stop
        # lets every leg relax southward instead of staying glued to the travel
        # direction.
        # Keep only a small amount of the old world-space foot history. A fast
        # drag can move the body many pixels in one frame; using that history
        # directly would leave the endpoint behind the body and make the legs
        # disappear under it. The visible response comes from the damped lag
        # below, while the pose itself stays in its own body-relative lane.
        history_blend = (1.0 - settle) * 0.22
        lane_source_x = lane_x + (foot_x - lane_x) * history_blend
        lane_source_y = lane_y + (foot_y - lane_y) * history_blend
        # Pull the endpoints into a relaxed, compact crouch. Preserve each
        # leg's own lane so the feet remain visible, but do not leave the
        # picked-up spider in its fully spread walking star.
        spread = 0.68 - held_response * 0.04 + 0.025 * math.sin(phase * 0.73 + held_clock * 0.34)
        foot_x = ax + (lane_source_x - ax) * spread

        # The whole leg set follows the hand with a small damped lag. The
        # phase offset keeps the legs from moving as one rigid fan.
        leg_phase = held_clock + phase * 0.24
        foot_x += float(getattr(self, "held_drag_sway_x", 0.0)) * (0.72 + 0.10 * math.sin(leg_phase))
        foot_y += float(getattr(self, "held_drag_sway_y", 0.0)) * (0.72 + 0.10 * math.cos(leg_phase))

        # A moving hand gives the feet a small, smooth counter-lag. It is
        # deliberately bounded and horizontal-first so gravity still wins.
        drag_speed = math.hypot(self.drag_vel_x, self.drag_vel_y)
        if drag_speed > 1.0:
            lag = self.size * (0.025 + held_speed01 * 0.16)
            foot_x -= (self.drag_vel_x / drag_speed) * lag
            foot_y -= (self.drag_vel_y / drag_speed) * lag * 0.30

        # Gravity is screen-down, not body-relative. Put the feet clearly south
        # of the abdomen. Each leg receives a slightly different drop along
        # the same lower gravity band, rather than converging on a small sun.
        bottom_band = self.y + self.size * (0.52 + 0.022 * math.sin(phase * 1.13 + held_clock * 0.24))
        natural_drop = clamp(lane_source_y - ay, 0.0, self.size * 0.45)
        gravity_floor = bottom_band + self.size * (0.10 + held_speed01 * 0.045) + natural_drop * 0.10
        bounce_amp = self.size * (0.012 + held_response * 0.052)
        bounce = bounce_amp * math.sin(held_clock * 1.10 + phase * 0.31)
        foot_y = max(foot_y, gravity_floor + bounce)
        # Unlike the old single floor-relative wobble, these offsets are driven
        # by the hand's acceleration and stored per leg. They lag, overshoot,
        # and settle independently, so a fast drag produces a soft dangling
        # bounce instead of a static dead fold or one synchronized fan.
        foot_x += float(getattr(leg, "held_spring_x", 0.0))
        foot_y += float(getattr(leg, "held_spring_y", 0.0))
        return foot_x, foot_y

    def _leg_draw_points(self, leg: LegState) -> Tuple[float, float, float, float]:
        """World-space (attach, foot) for rendering with catch-reach + airborne tuck.

        The tuned gait keeps ``leg.foot_x/foot_y`` planted; this only adjusts the
        *rendered* foot so the front legs can reach out during a catch, legs tuck
        during a jump, and a held spider can hang its feet below the body. Vertical
        jump lift is applied later as a pure draw-time screen offset so the solved
        knee geometry stays correct.
        """
        ax, ay = self._leg_attach(leg)
        foot_x, foot_y = self._visual_foot_for_render(leg)
        front = self._leg_front_factor(leg)
        if self.dragging:
            picked_x, picked_y = self._picked_up_leg_pose(leg, ax, ay, foot_x, foot_y)
            return ax, ay, picked_x, picked_y
        if self.catch_blend > 0.001 and front > 0.15:
            cx, cy = self.catch_point
            reach = self.catch_blend * clamp((front - 0.15) / 0.85, 0.0, 1.0)
            # Constrain the requested hand/catch point to this leg's own
            # forward/lateral lane before moving the endpoint. Without this,
            # all four front legs can chase one mouse point across their lanes,
            # after which the chain renderer is forced into a straight bend.
            cx, cy = self._constrain_leg_point(leg, cx, cy)
            rx = foot_x + (cx - foot_x) * 0.40 * reach
            ry = foot_y + (cy - foot_y) * 0.40 * reach
            # Never let a distant target stretch the leg past a believable span.
            max_span = self.size * (float(leg.definition.get("reach", 1.8)) + 0.4)
            ddx, ddy = rx - ax, ry - ay
            dlen = math.hypot(ddx, ddy)
            if dlen > max_span and dlen > 1e-4:
                scale = max_span / dlen
                rx = ax + ddx * scale
                ry = ay + ddy * scale
            foot_x, foot_y = rx, ry
        if self.state == "Feed" and front > 0.2:
            # Work the prey held at the front: rapid in/out tugging along the
            # line to the catch point, plus a small sideways knead, each front
            # leg slightly out of phase so they look like busy little hands.
            paw = self._feed_paw
            cx, cy = self.catch_point
            dx, dy = cx - foot_x, cy - foot_y
            dl = math.hypot(dx, dy) or 1.0
            ux, uy = dx / dl, dy / dl
            amp = self.size * 0.11 * clamp((front - 0.2) / 0.8, 0.0, 1.0)
            osc = math.sin(paw * 22.0 + leg.phase_seed * 2.0)
            foot_x += ux * amp * osc
            foot_y += uy * amp * osc
            knead = math.sin(paw * 17.0 + leg.phase_seed)
            foot_x += -uy * amp * 0.5 * knead
            foot_y += ux * amp * 0.5 * knead
        foot_x, foot_y = self._front_leg_feeler_pose(leg, ax, ay, foot_x, foot_y, front)
        foot_x, foot_y = self._combat_leg_pose(leg, ax, ay, foot_x, foot_y, front)
        if self.airborne and self.jump_peak > 1e-3:
            tuck = clamp(self.jump_z / self.jump_peak, 0.0, 1.0) * 0.8
            foot_x += (ax - foot_x) * tuck
            foot_y += (ay - foot_y) * tuck
        if self.roll_tuck > 1e-3:
            # Curl the legs in toward the body so the spinning spider reads as a
            # tucked ball rather than a splayed star. Keep a visible gap between
            # the body and the feet: collapsing every chain into one point makes
            # the smaller sprite-rig spiders look flattened during the tumble.
            tuck_strength = clamp(float(self._appearance("roll_tuck_strength", 0.46)), 0.25, 0.62)
            t = clamp(self.roll_tuck, 0.0, 1.0) * tuck_strength
            foot_x += (ax - foot_x) * t
            foot_y += (ay - foot_y) * t
        return ax, ay, foot_x, foot_y

    def _combat_leg_pose(self, leg: LegState, ax: float, ay: float,
                         foot_x: float, foot_y: float, front: float) -> Tuple[float, float]:
        """Raise and spread the front legs of a spider squared up at a foe.

        The threat posture of a real tarantula, and the reason this touches
        the *rendered* foot rather than the planted one: the back legs stay
        exactly where they are, so the spider holds its ground while its front
        half rises. Moving the planted foot would make it walk backwards.

        Only the front pairs, weighted by `_leg_front_factor`, so a spider
        does not levitate.
        """
        stance = getattr(self, "combat_stance", 0.0)
        if stance <= 0.01 or front <= 0.15:
            return foot_x, foot_y
        weight = stance * clamp((front - 0.15) / 0.85, 0.0, 1.0)
        face_x = getattr(self, "combat_face_x", 0.0)
        face_y = getattr(self, "combat_face_y", 0.0)
        # Up, out to the side, and a little towards the foe: raised and
        # spread rather than raised and pressed together.
        lateral_x, lateral_y = -face_y, face_x
        side = self._side_sign(leg.definition.get("side", "right"))
        foot_x += face_x * self.size * 0.30 * weight
        foot_y += face_y * self.size * 0.30 * weight
        foot_x += lateral_x * side * self.size * 0.34 * weight
        foot_y += lateral_y * side * self.size * 0.34 * weight
        foot_y -= self.size * 0.52 * weight
        return foot_x, foot_y

    def _draw_leg_connections(self, painter, chain_config: Optional[dict]) -> None:
        """Paint the short coxa/trochanter bridges that seat legs in the body.

        The articulated leg is intentionally rendered underneath the shell.  A
        tarantula still needs a visible proximal connection, though: without a
        painted socket and a short outward coxa, the lateral roots look
        detached or disappear under the cephalothorax.  This overlay redraws
        only that first socket-to-coxa portion; the remaining leg chain keeps
        its normal depth ordering.
        """

        cfg = self._appearance("leg_connections", {})
        if not isinstance(cfg, dict) or cfg.get("enabled", False) is not True or not chain_config:
            return

        try:
            socket_radius = clamp(float(cfg.get("socket_radius", 0.09)), 0.04, 0.18) * self.size
            outline_width = clamp(float(cfg.get("socket_outline_width", 0.035)), 0.01, 0.08) * self.size
            socket_core_scale = clamp(float(cfg.get("socket_core_scale", 0.66)), 0.35, 0.90)
            coxa_length = clamp(float(cfg.get("coxa_length", 0.18)), 0.06, 0.32) * self.size
            trochanter_length = clamp(float(cfg.get("trochanter_length", 0.12)), 0.04, 0.24) * self.size
            coxa_width = clamp(float(cfg.get("coxa_width", 0.10)), 0.04, 0.18) * self.size
            trochanter_width = clamp(float(cfg.get("trochanter_width", 0.075)), 0.03, 0.14) * self.size
            joint_radius = clamp(float(cfg.get("joint_radius", 0.06)), 0.03, 0.12) * self.size
        except (TypeError, ValueError):
            return

        socket_color = self._qcolor(str(cfg.get("socket_color_key", "body")), 245)
        core_color = self._qcolor(str(cfg.get("core_color_key", "legs")), 250)
        accent_color = self._qcolor(str(cfg.get("accent_color_key", "highlight")), 150)
        segment_color_keys = chain_config.get("segment_color_keys", ["legs"] * chain_config["segment_count"])
        proximal_color = self._qcolor(
            segment_color_keys[0] if segment_color_keys else str(cfg.get("core_color_key", "legs")),
            245,
        )

        for leg in self.legs:
            ax, ay = self._leg_attach(leg)
            foot_x, foot_y = self._leg_draw_points(leg)[2:]
            foot_x, foot_y = self._safe_sprite_leg_foot(leg, foot_x, foot_y, chain_config)
            chain_points = self._sprite_leg_chain_points(leg, ax, ay, foot_x, foot_y, chain_config)
            root_x, root_y = chain_points[0]
            first_x, first_y = chain_points[1]
            direction_x = first_x - root_x
            direction_y = first_y - root_y
            direction_len = math.hypot(direction_x, direction_y)
            if direction_len < 1e-4 and len(chain_points) > 2:
                second_x, second_y = chain_points[2]
                direction_x = second_x - root_x
                direction_y = second_y - root_y
                direction_len = math.hypot(direction_x, direction_y)
            if direction_len < 1e-4:
                continue

            # The root overlay follows the first articulated link outward from
            # the socket.  The old bridge pointed inward toward the body and
            # read as a tube disappearing underneath the shell; this makes the
            # coxa visibly emerge from a lateral carapace socket instead.
            direction_x /= direction_len
            direction_y /= direction_len
            exposed_length = min(
                coxa_length + trochanter_length,
                max(self.size * 0.12, direction_len),
            )
            end_x = root_x + direction_x * exposed_length
            end_y = root_y + direction_y * exposed_length
            split = coxa_length / max(1e-4, coxa_length + trochanter_length)
            joint_x = root_x + (end_x - root_x) * split
            joint_y = root_y + (end_y - root_y) * split

            painter.setPen(QPen(socket_color, max(1.0, outline_width), Qt.SolidLine, Qt.RoundCap))
            painter.drawLine(QPointF(root_x, root_y), QPointF(end_x, end_y))
            painter.setPen(QPen(proximal_color, max(1.2, coxa_width), Qt.SolidLine, Qt.RoundCap))
            painter.drawLine(QPointF(root_x, root_y), QPointF(joint_x, joint_y))
            painter.setPen(QPen(core_color, max(1.0, trochanter_width), Qt.SolidLine, Qt.RoundCap))
            painter.drawLine(QPointF(joint_x, joint_y), QPointF(end_x, end_y))

            painter.setPen(Qt.NoPen)
            painter.setBrush(QBrush(socket_color))
            painter.drawEllipse(QPointF(root_x, root_y), socket_radius, socket_radius * 0.76)
            painter.setBrush(QBrush(proximal_color))
            painter.drawEllipse(
                QPointF(root_x, root_y),
                socket_radius * socket_core_scale,
                socket_radius * socket_core_scale * 0.74,
            )
            painter.setBrush(QBrush(accent_color))
            painter.drawEllipse(QPointF(joint_x, joint_y), joint_radius, joint_radius * 0.72)

    def _draw_antennae(self, painter, ceph_offset_x: float, ceph_w: float, ceph_h: float, startle: float) -> None:
        """Two expressive feelers on the head front; shape carries the emotion."""

        cfg = self._appearance("antennae", {})
        if not isinstance(cfg, dict):
            cfg = {}
        if cfg.get("enabled", True) is False:
            return
        style = str(cfg.get("style", "")).strip().lower()
        hand_palp_style = style == "tarantula_hand_palps"

        segments = max(3, int(cfg.get("segments", 5)))
        length_units = float(cfg.get("length", 1.15))
        base_forward = float(cfg.get("base_forward", 0.34))
        base_side = float(cfg.get("base_side", 0.26))
        thickness = float(cfg.get("thickness", 0.06))
        color_key = str(cfg.get("color_key", "legs"))
        tip_color_key = str(cfg.get("tip_color_key", "highlight"))

        aiming = 0.0 if self.dragging else clamp(self.aim_intent, 0.0, 1.0)
        inspecting = 0.0 if self.dragging else clamp(self.inspect_intent, 0.0, 1.0)
        cuddling = 0.0 if self.dragging else clamp(max(self.cuddle_intent, self.catch_blend), 0.0, 1.0)
        aim_angle = self._antenna_aim_angle()
        drive = antenna_drive_from_mood(
            self.mood,
            aiming=aiming,
            inspecting=inspecting,
            cuddling=cuddling,
            aim_angle=aim_angle,
        )

        base_x = ceph_offset_x + ceph_w * base_forward
        span = length_units * self.size * (1.0 + (0.0 if self.dragging else startle) * 0.08)
        line_col = self._qcolor(color_key, 240)
        tip_col = self._qcolor(tip_color_key, 240)

        # Tarantulas do not have insect-like antennae.  Their front appendages
        # are broad, jointed, and rise out of the cephalothorax before the
        # remaining links descend toward the ground.  Keep this as an explicit
        # model style so the expressive feeler rig remains available to the
        # other creatures.
        if style in (
            "front_leg", "tarantula_front_legs", "tarantula_hand_palps"
        ):
            raw_lengths = cfg.get("segment_lengths", [0.24, 0.28, 0.22, 0.15, 0.11])
            if not isinstance(raw_lengths, list):
                raw_lengths = [0.24, 0.28, 0.22, 0.15, 0.11]
            try:
                lengths = [float(value) for value in raw_lengths[:segments]]
            except (TypeError, ValueError):
                lengths = []
            if len(lengths) != segments or any(value <= 0.0 or not math.isfinite(value) for value in lengths):
                lengths = [1.0 for _ in range(segments)]
            length_total = max(1e-4, sum(lengths))
            # Leg-like palp silhouette: the proximal link rises, then the
            # remaining links reverse through the knee and descend to the tip.
            default_angle_profile = [0.90, 0.56, 0.22, -0.13, -0.46]
            raw_angle_profile = cfg.get("rest_angles", default_angle_profile)
            if not isinstance(raw_angle_profile, list):
                raw_angle_profile = default_angle_profile
            try:
                angle_profile = [float(value) for value in raw_angle_profile[:segments]]
            except (TypeError, ValueError):
                angle_profile = []
            if len(angle_profile) != segments or any(
                not math.isfinite(value) for value in angle_profile
            ):
                angle_profile = list(default_angle_profile[:segments])
            while len(angle_profile) < segments:
                angle_profile.append(angle_profile[-1] * 0.72)
            joint_scale = clamp(float(cfg.get("joint_scale", 0.95)), 0.45, 1.60)
            proximal_rise = clamp(float(cfg.get("proximal_rise", 0.12)), 0.0, 0.28) * self.size
            screen_lift_scale = clamp(float(cfg.get("screen_lift_scale", 0.22)), 0.0, 0.45)
            leg_thickness = clamp(float(cfg.get("leg_thickness", 1.0)), 0.65, 1.60)
            hairy = bool(cfg.get("hairy", False))
            hair_scale = clamp(float(cfg.get("hair_scale", 0.16)), 0.0, 0.50)
            raw_width_profile = cfg.get(
                "segment_widths", [1.20, 1.08, 0.92, 0.72, 0.48]
            )
            if not isinstance(raw_width_profile, list):
                raw_width_profile = [1.20, 1.08, 0.92, 0.72, 0.48]
            try:
                width_profile = [float(value) for value in raw_width_profile[:segments]]
            except (TypeError, ValueError):
                width_profile = []
            if len(width_profile) != segments or any(
                value <= 0.0 or not math.isfinite(value) for value in width_profile
            ):
                width_profile = [1.20, 1.08, 0.92, 0.72, 0.48][:segments]
            while len(width_profile) < segments:
                width_profile.append(max(0.30, width_profile[-1] * 0.72))
            raw_segment_colors = cfg.get("segment_color_keys", [color_key] * segments)
            if not isinstance(raw_segment_colors, list):
                raw_segment_colors = [color_key] * segments
            segment_color_keys = [str(value).strip() or color_key for value in raw_segment_colors]
            if len(segment_color_keys) != segments:
                segment_color_keys = [color_key] * segments
            joint_color_key = str(cfg.get("joint_color_key", segment_color_keys[1] if segments > 1 else color_key)).strip() or color_key
            claw_color_key = str(cfg.get("claw_color_key", cfg.get("tip_color_key", "highlight"))).strip() or "highlight"
            if hand_palp_style:
                # Real tarantula pedipalps are compact appendages tucked beside
                # the chelicerae. Their attention drive may curl the joints, but
                # it must not turn the whole palp into a long reaching limb.
                min_drive_length = clamp(float(cfg.get("min_drive_length", 0.78)), 0.55, 1.0)
                max_drive_length = clamp(float(cfg.get("max_drive_length", 1.08)), min_drive_length, 1.20)
                max_extension = clamp(float(cfg.get("max_extension", 0.06)), 0.0, 0.12)
                span *= clamp(drive.length, min_drive_length, max_drive_length)
            else:
                max_extension = 0.24
                span *= clamp(drive.length, 0.65, 1.50)

            for idx, side_sign in enumerate((-1.0, 1.0)):
                root_y = side_sign * ceph_h * base_side
                stored_angles = getattr(self, "antenna_segment_angles", [[], []])
                stored_lifts = getattr(self, "antenna_joint_lifts", [[], []])
                side_angles = stored_angles[idx] if idx < len(stored_angles) else []
                side_lifts = stored_lifts[idx] if idx < len(stored_lifts) else []
                extension = getattr(self, "antenna_extension", [0.0, 0.0])
                extension = extension[idx] if idx < len(extension) else 0.0
                fwd = 0.0
                lateral = 0.0
                screen = [(base_x, root_y)]
                for segment_index in range(segments):
                    if len(side_angles) == segments:
                        angle = side_angles[segment_index]
                    else:
                        angle = side_sign * angle_profile[segment_index]
                    # Ordinary feelers may use a shared aim overlay.  Tarantula
                    # pedipalps must not: the controller above has already
                    # applied a bounded command to each knuckle, and replacing
                    # those angles here would turn the articulated hand into a
                    # single straight mouse pointer.
                    if drive.aim_blend > 0.01 and not hand_palp_style:
                        angle = angle * (1.0 - drive.aim_blend) + drive.aim_angle * drive.aim_blend
                    segment = span * (1.0 + clamp(float(extension), 0.0, max_extension)) * lengths[segment_index] / length_total
                    fwd += math.cos(angle) * segment
                    lateral += math.sin(angle) * segment
                    progress = (segment_index + 1) / max(1, segments)
                    if len(side_lifts) == segments:
                        rise = self.size * clamp(float(side_lifts[segment_index]), 0.0, 0.34)
                    else:
                        rise = proximal_rise * max(0.0, 1.0 - progress)
                    # Lift is only a small mirrored screen projection. The old
                    # unsigned subtraction moved both hands toward the same
                    # screen side, while projecting the full lift collapsed
                    # both proximal links into the center. Either mistake made
                    # a symmetric pair look crossed/broken.
                    screen.append(
                        (base_x + fwd, root_y + lateral - side_sign * rise * screen_lift_scale)
                    )

                for segment_index in range(segments):
                    width = max(
                        0.85,
                        self.size * thickness * drive.thickness * leg_thickness
                        * width_profile[segment_index],
                    )
                    x1, y1 = screen[segment_index]
                    x2, y2 = screen[segment_index + 1]
                    if hairy and hair_scale > 0.0:
                        hair_color = self._qcolor_triplet(
                            self._appearance_color("fluff_color", "highlight"),
                            int(70 + hair_scale * 95),
                        )
                        painter.setPen(QPen(hair_color, width * (1.16 + hair_scale * 0.60), Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
                        painter.drawLine(QPointF(x1, y1), QPointF(x2, y2))
                    segment_color = self._qcolor(
                        "highlight" if startle and not self.dragging else segment_color_keys[segment_index],
                        240,
                    )
                    cap = Qt.FlatCap if segment_index == segments - 1 else Qt.RoundCap
                    painter.setPen(QPen(segment_color, width, Qt.SolidLine, cap, Qt.RoundJoin))
                    painter.drawLine(QPointF(x1, y1), QPointF(x2, y2))

                painter.setPen(Qt.NoPen)
                painter.setBrush(QBrush(self._qcolor(joint_color_key, 235)))
                for joint_index, (joint_x, joint_y) in enumerate(screen[1:-1]):
                    adjacent_width = width_profile[min(joint_index, segments - 1)]
                    joint_radius = max(
                        0.85,
                        self.size * thickness * drive.thickness * leg_thickness
                        * adjacent_width * 0.20 * joint_scale,
                    )
                    joint_radius *= 1.08 if joint_index == 0 else (0.92 - min(0.25, joint_index * 0.05))
                    painter.drawEllipse(QPointF(joint_x, joint_y), joint_radius, joint_radius * 0.82)
                tx, ty = screen[-1]
                # The terminal tarsus is tapered and ends in one small pointed
                # claw. A round bulb here was the main visual cue that made the
                # appendage read like a broken antenna instead of a short leg.
                if segments >= 2:
                    nail_length = clamp(float(cfg.get("tip_nail_length", 0.065)), 0.025, 0.12) * span
                    nail_curl = clamp(float(cfg.get("tip_curl", 0.72)), 0.30, 1.10)
                    last_dx = screen[-1][0] - screen[-2][0]
                    last_dy = screen[-1][1] - screen[-2][1]
                    last_norm = max(1e-4, math.hypot(last_dx, last_dy))
                    last_angle = math.atan2(last_dy, last_dx)
                    first_nail = (
                        tx + last_dx / last_norm * nail_length * 0.42,
                        ty + last_dy / last_norm * nail_length * 0.42,
                    )
                    curl_angle = last_angle - side_sign * nail_curl
                    nail_end = (
                        first_nail[0] + math.cos(curl_angle) * nail_length * 0.58,
                        first_nail[1] + math.sin(curl_angle) * nail_length * 0.58,
                    )
                    claw_color = self._qcolor(
                        "highlight" if startle and not self.dragging else claw_color_key,
                        245,
                    )
                    tip_radius = max(
                        0.75,
                        self.size * thickness * drive.thickness * leg_thickness
                        * width_profile[-1] * 0.24,
                    )
                    last_unit_x = last_dx / last_norm
                    last_unit_y = last_dy / last_norm
                    claw_base = (
                        tx + last_unit_x * nail_length * 0.12,
                        ty + last_unit_y * nail_length * 0.12,
                    )
                    perp_x, perp_y = -last_unit_y, last_unit_x
                    painter.setPen(Qt.NoPen)
                    painter.setBrush(QBrush(claw_color))
                    painter.drawPolygon(QPolygonF([
                        QPointF(
                            claw_base[0] + perp_x * tip_radius,
                            claw_base[1] + perp_y * tip_radius,
                        ),
                        QPointF(
                            claw_base[0] - perp_x * tip_radius,
                            claw_base[1] - perp_y * tip_radius,
                        ),
                        QPointF(*nail_end),
                    ]))
            return

        for idx, side_sign in enumerate((-1.0, 1.0)):
            pts = build_antenna_points(side_sign, segments, drive, self.antenna_phase[idx])
            root_y = side_sign * ceph_h * base_side
            screen = [(base_x + fwd * span, root_y + sd * span) for (fwd, sd) in pts]
            n = len(screen)
            for j in range(n - 1):
                t = j / max(1, n - 2)
                w = max(1.0, self.size * thickness * drive.thickness * (1.0 - 0.55 * t))
                painter.setPen(QPen(line_col, w, Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
                x1, y1 = screen[j]
                x2, y2 = screen[j + 1]
                painter.drawLine(QPointF(x1, y1), QPointF(x2, y2))
            tx, ty = screen[-1]
            r = max(1.2, self.size * thickness * drive.tip_bulb * 0.9)
            painter.setPen(Qt.NoPen)
            painter.setBrush(QBrush(tip_col))
            painter.drawEllipse(QPointF(tx, ty), r, r)

    def _draw_eyes(self, painter, eyes, ceph_offset_x: float, ceph_w: float, ceph_h: float, startle: float, aiming: float) -> None:
        """Draw a set of eyes whose openness, gaze and shape follow the mood."""

        m = self.mood
        # Affectionate / cute blush sits under the eyes.
        blush_amt = clamp(m.affection * 0.75 + float(self._appearance("cute_blush", 0.0)), 0.0, 1.0)
        if blush_amt > 0.05:
            col = self._qcolor_triplet([255, 168, 176], int(30 + blush_amt * 80))
            painter.setPen(Qt.NoPen)
            painter.setBrush(QBrush(col))
            painter.drawEllipse(QRectF(ceph_offset_x - ceph_w * 0.05, -ceph_h * 0.32, ceph_w * 0.17, ceph_h * 0.12))
            painter.drawEllipse(QRectF(ceph_offset_x - ceph_w * 0.05, ceph_h * 0.20, ceph_w * 0.17, ceph_h * 0.12))

        eye_startle = self._eye_startle_amount(startle)
        openness = clamp(
            0.62 + m.arousal * 0.5 + aiming * 0.32 - m.sleepy * 0.55 - max(0.0, m.valence) * 0.16,
            0.12,
            1.3,
        ) * (1.0 + eye_startle * 0.4)
        eff_open = clamp(openness * (1.0 - self.expression_blink * 0.92), 0.05, 1.4)
        squint_happy = (m.valence > 0.45 and m.arousal < 0.72 and aiming < 0.2 and self.expression_blink < 0.4)

        base_col = self._qcolor("eyes", 240)
        pupil_col = self._qcolor_triplet([20, 18, 26], 255)
        shine_col = self._qcolor_triplet([255, 255, 255], 235)

        for (ex, ey, r) in eyes:
            if squint_happy:
                painter.setPen(QPen(pupil_col, max(1.2, r * 0.5), Qt.SolidLine, Qt.RoundCap, Qt.RoundJoin))
                painter.setBrush(Qt.NoBrush)
                arc = QPainterPath(QPointF(ex - r, ey + r * 0.28))
                arc.quadTo(QPointF(ex, ey - r * 0.82), QPointF(ex + r, ey + r * 0.28))
                painter.drawPath(arc)
                continue
            painter.setPen(Qt.NoPen)
            painter.setBrush(QBrush(base_col))
            painter.drawEllipse(QRectF(ex - r, ey - r * eff_open, r * 2.0, r * 2.0 * eff_open))
            if eff_open > 0.22:
                gx = ex + self.look_fwd * r * 0.40
                gy = ey + self.look_side * r * 0.40 * eff_open
                pr = r * 0.52
                painter.setBrush(QBrush(pupil_col))
                painter.drawEllipse(QPointF(gx, gy), pr, pr * eff_open)
                painter.setBrush(QBrush(shine_col))
                painter.drawEllipse(QPointF(gx - pr * 0.32, gy - pr * 0.38 * eff_open), pr * 0.34, pr * 0.34 * eff_open)

