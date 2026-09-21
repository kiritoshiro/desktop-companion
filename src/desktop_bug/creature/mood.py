"""Emotional state and expressive feeler (antenna) geometry.

This module is intentionally free of any PyQt or Creature dependency so the
emotional model and the antenna curve maths can be reasoned about and tested in
isolation.  The :class:`Creature` owns one :class:`Mood`, relaxes it toward a
personality/mode baseline every frame, and bumps it on events (being grabbed,
finishing a cuddle, spotting a playmate).  Rendering code then reads the mood to
pose the antennae, eyes, blush and body language.

Top-down note: the desktop view looks straight down, so there is no literal
"up".  Emotion is therefore carried by *shape* and *motion* rather than height.
Antennae that curl back into a hook read as happy/alert; antennae that sag and
splay read as sad/sleepy; stiff straight antennae read as focused.  This is the
standard cartoon convention and it survives the missing vertical axis.
"""

from __future__ import annotations

import math
from dataclasses import dataclass


def clamp(value: float, low: float, high: float) -> float:
    return max(low, min(high, value))


def approach(current: float, target: float, dt: float, rate: float) -> float:
    """Frame-rate independent exponential easing of ``current`` toward ``target``."""
    if rate <= 0.0:
        return target
    alpha = 1.0 - math.exp(-rate * dt)
    return current + (target - current) * alpha


# Named baselines.  Each tuple is (valence, arousal, affection, curiosity).
# valence:   -1 gloomy .. +1 delighted
# arousal:    0 sleepy .. 1 wired
# affection:  0 aloof  .. 1 wants cuddles
# curiosity:  0 incurious .. 1 must-investigate
MOOD_BASELINES = {
    "auto": (0.18, 0.40, 0.32, 0.52),
    "playful": (0.62, 0.74, 0.40, 0.66),
    "cuddly": (0.55, 0.34, 0.85, 0.45),
    "curious": (0.28, 0.50, 0.30, 0.92),
    "calm": (0.22, 0.18, 0.34, 0.30),
    "skittish": (-0.08, 0.62, 0.12, 0.55),
    "hunter": (0.30, 0.66, 0.18, 0.60),
    "bold": (0.45, 0.58, 0.30, 0.62),
    "grumpy": (-0.35, 0.40, 0.10, 0.22),
    "zoomy": (0.70, 0.92, 0.42, 0.80),
    "mellow": (0.30, 0.22, 0.40, 0.34),
    "clingy": (0.50, 0.40, 0.95, 0.50),
    "bashful": (-0.05, 0.30, 0.45, 0.40),
    "nope": (-0.22, 0.96, 0.04, 0.22),
    "drifter": (0.36, 0.78, 0.22, 0.70),
}


@dataclass
class Mood:
    valence: float = 0.18
    arousal: float = 0.40
    affection: float = 0.32
    curiosity: float = 0.52

    base_valence: float = 0.18
    base_arousal: float = 0.40
    base_affection: float = 0.32
    base_curiosity: float = 0.52

    def set_baseline(self, valence: float, arousal: float, affection: float, curiosity: float) -> None:
        self.base_valence = clamp(valence, -1.0, 1.0)
        self.base_arousal = clamp(arousal, 0.0, 1.0)
        self.base_affection = clamp(affection, 0.0, 1.0)
        self.base_curiosity = clamp(curiosity, 0.0, 1.0)

    def set_baseline_named(self, name: str) -> None:
        vals = MOOD_BASELINES.get(str(name).lower())
        if vals:
            self.set_baseline(*vals)

    def relax(self, dt: float, rate: float = 0.5) -> None:
        self.valence = approach(self.valence, self.base_valence, dt, rate)
        self.arousal = approach(self.arousal, self.base_arousal, dt, rate * 0.85)
        self.affection = approach(self.affection, self.base_affection, dt, rate * 0.7)
        self.curiosity = approach(self.curiosity, self.base_curiosity, dt, rate * 0.9)

    def bump(self, valence: float = 0.0, arousal: float = 0.0, affection: float = 0.0, curiosity: float = 0.0) -> None:
        self.valence = clamp(self.valence + valence, -1.0, 1.0)
        self.arousal = clamp(self.arousal + arousal, 0.0, 1.0)
        self.affection = clamp(self.affection + affection, 0.0, 1.0)
        self.curiosity = clamp(self.curiosity + curiosity, 0.0, 1.0)

    @property
    def happy(self) -> float:
        return clamp(self.valence, 0.0, 1.0)

    @property
    def sad(self) -> float:
        return clamp(-self.valence, 0.0, 1.0)

    @property
    def sleepy(self) -> float:
        return clamp((0.5 - self.arousal) * 2.0, 0.0, 1.0)

    def label(self) -> str:
        """A coarse descriptor, handy for debugging/telemetry."""
        if self.arousal < 0.22 and self.valence >= -0.1:
            return "sleepy"
        if self.affection > 0.7 and self.valence > 0.2:
            return "affectionate"
        if self.curiosity > 0.7 and self.arousal > 0.4:
            return "curious"
        if self.valence > 0.45 and self.arousal > 0.55:
            return "playful"
        if self.valence < -0.3:
            return "glum"
        if self.arousal > 0.7:
            return "excited"
        return "content"


@dataclass
class AntennaDrive:
    """Resolved per-frame antenna animation parameters (units of body ``size``)."""

    length: float = 1.0
    base_angle: float = 0.62      # radians from the forward axis, before side sign
    curl: float = 0.4             # +curl hooks back (happy/alert), -curl sags (sad)
    curve_gain: float = 0.55      # radians of bend accumulated per segment at curl=1
    wave_amp: float = 0.25        # feeler sway amplitude
    wave_freq: float = 2.4        # feeler sway speed
    droopiness: float = 0.0       # extra outward sag for low valence / sleepiness
    aim_angle: float = 0.0        # shared body-local angle to a focus target
    aim_blend: float = 0.0        # 0 free .. 1 fully pointing at the target
    thickness: float = 1.0
    tip_bulb: float = 1.0


def antenna_drive_from_mood(
    mood: Mood,
    *,
    aiming: float = 0.0,
    inspecting: float = 0.0,
    cuddling: float = 0.0,
    aim_angle: float = 0.0,
) -> AntennaDrive:
    """Translate an emotional state plus situational blends into antenna motion.

    ``aiming``/``inspecting``/``cuddling`` are 0..1 situational weights set by the
    behaviour state machine.  ``aim_angle`` is the body-local bearing to whatever
    the creature is focused on (cursor, prey, playmate).
    """
    happy = mood.happy
    sad = mood.sad
    sleepy = mood.sleepy

    curl = clamp(0.18 + mood.valence * 0.85 + mood.affection * 0.35 - sleepy * 0.30, -1.0, 1.0)
    length = clamp(0.86 + mood.curiosity * 0.26 + mood.arousal * 0.12 - sleepy * 0.20, 0.6, 1.5)
    wave_amp = clamp(0.12 + mood.curiosity * 0.45 + mood.arousal * 0.28 - sleepy * 0.10, 0.0, 1.1)
    wave_freq = clamp(1.6 + mood.arousal * 3.4 + mood.curiosity * 1.2, 0.8, 7.0)
    droop = clamp(sad * 0.8 + sleepy * 0.7, 0.0, 1.4)
    base_angle = clamp(0.62 - mood.arousal * 0.10 + droop * 0.18, 0.30, 0.95)

    # Inspecting: extend, probe faster, lean toward target.
    length += inspecting * 0.28
    wave_amp += inspecting * 0.35
    wave_freq += inspecting * 1.4
    aim_blend = clamp(inspecting * 0.62, 0.0, 0.85)

    # Aiming/hunting: stiffen, straighten, converge on the target.
    stiffen = clamp(aiming, 0.0, 1.0)
    curl = curl * (1.0 - stiffen * 0.75) + 0.10 * stiffen
    wave_amp *= (1.0 - stiffen * 0.65)
    length += aiming * 0.18
    aim_blend = max(aim_blend, clamp(aiming * 0.92, 0.0, 0.95))

    # Cuddling: high happy curl that wraps inward, slow gentle sway.
    curl = curl * (1.0 - cuddling * 0.5) + 0.9 * cuddling
    wave_amp = wave_amp * (1.0 - cuddling * 0.4) + 0.18 * cuddling
    wave_freq = wave_freq * (1.0 - cuddling * 0.4) + 1.1 * cuddling
    aim_blend = max(aim_blend, clamp(cuddling * 0.55, 0.0, 0.7))

    thickness = clamp(0.92 + mood.arousal * 0.16 + cuddling * 0.12, 0.7, 1.4)
    tip_bulb = clamp(0.85 + happy * 0.5 + cuddling * 0.5 + mood.curiosity * 0.2, 0.6, 1.9)

    return AntennaDrive(
        length=length,
        base_angle=base_angle,
        curl=curl,
        curve_gain=0.55,
        wave_amp=wave_amp,
        wave_freq=wave_freq,
        droopiness=droop,
        aim_angle=aim_angle,
        aim_blend=aim_blend,
        thickness=thickness,
        tip_bulb=tip_bulb,
    )


def build_antenna_points(
    side_sign: float,
    n_seg: int,
    drive: AntennaDrive,
    wave_phase: float,
):
    """Return body-local ``(forward, side)`` points from base to tip, in ``size`` units.

    forward is +x (heading), side is +y (the creature's right).  ``side_sign`` is
    -1 for the left feeler and +1 for the right.  The integration starts at a
    splayed forward bearing and bends by ``curl`` (a recurving hook when positive)
    plus a travelling sine wave (the live "feeler" motion).  When ``aim_blend`` is
    high the start bearing rotates toward ``aim_angle`` and the curl/wave fade, so
    both feelers converge on a shared target instead of mirroring.
    """
    n_seg = max(2, int(n_seg))
    base_angle = side_sign * drive.base_angle
    start_angle = base_angle * (1.0 - drive.aim_blend) + drive.aim_angle * drive.aim_blend
    eff_curve = drive.curve_gain * (1.0 - 0.8 * drive.aim_blend) * side_sign
    seg = drive.length / n_seg

    pts = [(0.0, 0.0)]
    fwd = 0.0
    side = 0.0
    ang = start_angle
    for i in range(n_seg):
        t = i / (n_seg - 1)
        wave = (
            math.sin(wave_phase - i * drive.wave_freq * 0.55)
            * drive.wave_amp
            * (0.30 + t)
            * (1.0 - 0.85 * drive.aim_blend)
            * side_sign
        )
        ang += eff_curve * drive.curl + wave * 0.5 + side_sign * drive.droopiness * 0.10 * t
        fwd += math.cos(ang) * seg
        side += math.sin(ang) * seg
        pts.append((fwd, side))
    return pts
