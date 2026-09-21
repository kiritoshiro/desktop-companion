"""Rolling: leaving it cleanly and putting the feet back down.

Split out of the single ``kinematics.py`` by DC-43; a pure move.
"""
from __future__ import annotations

import math

from ...support.math_utils import (
    clamp,
    clamp_point,
)



class RollMixin:
    """Rolling: leaving it cleanly and putting the feet back down."""

    def _finish_roll(self) -> None:
        """Undo everything a tumble was doing: the spin, the tuck, the feet."""
        self._rolling = False
        self.roll_spin = 0.0
        self.roll_tuck = 0.0
        self.roll_progress = 1.0
        self.motion_paused = False
        self.squash = 1.0
        self._reset_roll_contacts()
        self._spider_locomotion_active = False

    def _reconcile_roll(self) -> None:
        """Tidy up after a tumble that something cut short.

        Only the roll running to completion used to clean up after itself, so
        anything that interrupted one -- a startle, a grab, a job, the roll
        skill being switched off -- left the body rotated by whatever the spin
        had reached and the legs still tucked, permanently. Measured across
        seven seeds that was about 410 degrees of leftover rotation, feet past
        their own reach envelope and up to two legs on the wrong side of the
        body.

        This is the same reconciliation the weaving and web-walking states
        already do at the top of every frame, for the same reason: a state can
        be left in more ways than it can be finished.
        """
        if self._rolling and self.state != "Roll":
            self._finish_roll()

    def _reset_roll_contacts(self) -> None:
        """Restore every foot to a safe stance after a visual tumble.

        Roll rotates the rendered creature around its centre, but deliberately
        does not rotate the logical body heading. Leaving the old world-space
        contacts in place therefore makes the body finish in one orientation
        while most feet still belong to the pre-roll orientation. That is the
        source of the one-frame stretched legs and flattened-looking landings
        seen on non-tarantula sprite rigs.

        A roll is an explicit flourish, so a clean contact reset is preferable
        to replaying the normal walking scheduler here. The next grounded frame
        can then begin a new gait from eight bounded, stance contacts.
        """
        for leg in self.legs:
            foot_x, foot_y = self._leg_ideal_foot(leg)
            foot_x, foot_y = self._constrain_leg_point(leg, foot_x, foot_y)
            leg.foot_x, leg.foot_y = foot_x, foot_y
            leg.step_start_x, leg.step_start_y = foot_x, foot_y
            leg.step_target_x, leg.step_target_y = foot_x, foot_y
            leg.step_control_x, leg.step_control_y = foot_x, foot_y
            leg.pending_target_x, leg.pending_target_y = foot_x, foot_y
            leg.pending_delay = 0.0
            leg.step_timer = 0.0
            leg.stepping = False
            leg.pending_step = False
            leg.lift = 0.0
            leg.joint_bends = []
            leg.held_spring_x = 0.0
            leg.held_spring_y = 0.0
            leg.held_spring_vx = 0.0
            leg.held_spring_vy = 0.0
            self._spider_reanchor_stance(leg)
        self._spider_heading_filter = self.heading

    def _update_roll(self, dt: float, mx: float, my: float) -> None:
        # Spin through the planned turns with an ease-out so it whirls fast then
        # settles. The body stays put under the spin; we only nudge the centre
        # along the drift direction and carry the (tucked) feet with it.
        self.motion_paused = True
        self.speed = 0.0
        # The grounded solver runs after the FSM update in the normal frame
        # order. Keep walking momentum out of the roll and its completion frame,
        # otherwise the spider can resume in the pre-roll direction.
        self.current_speed = 0.0
        self.vel_x = 0.0
        self.vel_y = 0.0
        prev_eased = self.roll_eased
        self.roll_progress = min(1.0, self.roll_progress + dt / max(0.05, self.roll_duration))
        self.roll_eased = 1.0 - (1.0 - self.roll_progress) ** 2
        d_eased = self.roll_eased - prev_eased
        self.roll_spin = self.roll_dir * self.roll_total * self.roll_eased
        self.roll_tuck = math.sin(math.pi * clamp(self.roll_progress, 0.0, 1.0)) ** 0.7

        dist = self.roll_distance * d_eased
        ndx = math.cos(self.roll_drift_dir) * dist
        ndy = math.sin(self.roll_drift_dir) * dist
        nx, ny = clamp_point(self.x + ndx, self.y + ndy, self.margin, self.screen_w, self.screen_h)
        adx, ady = nx - self.x, ny - self.y
        self.x, self.y = nx, ny
        self._translate_leg_world_points(adx, ady, 1.0)
        # Freeze the base facing and target so the spin reads cleanly and the body
        # does not also try to turn or travel underneath it.
        self.target_x, self.target_y = self.x, self.y
        self.target_heading = self.heading

        if self.roll_progress >= 1.0:
            self._finish_roll()
            self.enter_idle()

