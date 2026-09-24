from __future__ import annotations

import argparse
import logging
import math
import os
import signal
import sys
from dataclasses import replace
from pathlib import Path

from PyQt5.QtCore import QElapsedTimer, QPoint, QRect, QTimer, Qt
from PyQt5.QtGui import (QColor, QCursor, QGuiApplication, QIcon, QPainter, QPixmap,
                         QRegion, QSurfaceFormat)
from PyQt5.QtWidgets import (
    QActionGroup,
    QApplication,
    QCheckBox,
    QComboBox,
    QDialog,
    QFormLayout,
    QHBoxLayout,
    QInputDialog,
    QLabel,
    QLineEdit,
    QMenu,
    QOpenGLWidget,
    QPushButton,
    QProgressBar,
    QTabWidget,
    QVBoxLayout,
    QSystemTrayIcon,
    QWidget,
)

from .. import __version__
from ..content.discovery import migrate_legacy_state_dir, resolve_preset_path, state_dir
from ..support.dpi import enable_high_dpi_scaling, logical_to_physical, screen_device_pixel_ratio_at
from ..support.logging_setup import configure_logging, get_logger, install_excepthook, log_path
from .session_control import clear_stop_request, consume_stop_request
from .live_channel import OverlayChannelServer, channel_name
from ..manager import CreatureManager
from ..content.preset_io import load_preset
from .overlay_win32 import apply_click_through, set_cursor_pos, set_input_transparent
from ..world.desktop_environment import snapshot_desktop_surfaces
from ..world.playfield import ScreenRect
from ..support.frame_policy import FramePolicy
from ..support.profiling import hud_requested, profiler_from_env
from ..content.skills import SKILLS
from ..state.progression import ABILITY_TREE, ARMOR_CATALOG, normalize_team_id, xp_to_next_level
from ..state.teams import HOSTILITY_NOTE, team_label
from ..world.jobs import job_definition


log = get_logger("engine")


def _target_fps() -> float:
    """Return requested overlay FPS.

    Default is smooth 60 FPS again.  The efficiency fixes now target the
    expensive background work instead of lowering the visual frame rate.
    Set DESKTOP_BUG_FPS=45/30/20 if you want to trade smoothness for lower CPU.
    """
    try:
        fps = float(os.environ.get("DESKTOP_BUG_FPS", "60"))
    except Exception:
        fps = 60.0
    return max(10.0, min(60.0, fps))


# DC-79: how often the GL overlay re-decides whether to let the mouse through.
INPUT_CHECK_MS = 50


def _frame_interval_ms_for_fps(fps: float) -> int:
    return max(16, int(round(1000.0 / max(1.0, fps))))


def gl_overlay_enabled() -> bool:
    """Whether the overlay paints onto an OpenGL surface. On by default since
    DC-78, at the owner's request; ``DESKTOP_BUG_GL=0`` turns it off.

    Measured on twenty tarantulas at 1600x1000, offscreen, interleaved in one
    process: a GPU framebuffer runs the same QPainter calls about **9%**
    faster than a software QImage (8.7 / 8.6 / 10.1 over three runs), and 8.7%
    with both sides clipped to the dirty region the overlay actually repaints.

    Nine percent, not the transformation it sounds like, because **Qt's
    OpenGL paint engine still turns every stroked path into triangles on the
    CPU**. The GPU takes the fill and the antialiasing -- which DC-71 measured
    at roughly 27% and 8% of render -- and leaves the per-primitive geometry
    alone. Before DC-73 the same comparison gave 20-24%; DC-73 removed the
    fill-heavy primitives, so most of that win had already been taken by
    cheaper means.

    DC-74 shipped it off, because 9% did not seem to justify making every
    launch depend on a working GL driver. The owner asked for it on (DC-78).
    What that still leaves unproven is the whole window on other machines,
    other drivers and a packaged build -- if the overlay ever comes up blank
    or opaque, ``DESKTOP_BUG_GL=0`` is the first thing to try.

    Only an explicit "no" turns it off. Anything else, a typo included, keeps
    the default rather than silently changing the surface.
    """
    return os.environ.get("DESKTOP_BUG_GL", "").strip().lower() not in {"0", "false", "no", "off"}


def configure_gl_surface() -> bool:
    """Ask for an alpha channel and multisampling, before any QApplication.

    A QOpenGLWidget cannot be translucent without an alpha buffer in the
    default surface format, and the format has to be set before the
    application object exists or it will not apply.
    """
    if not gl_overlay_enabled():
        return False
    fmt = QSurfaceFormat()
    fmt.setAlphaBufferSize(8)
    fmt.setSamples(4)
    QSurfaceFormat.setDefaultFormat(fmt)
    return True


GL_OVERLAY = gl_overlay_enabled()
# QOpenGLWidget drives painting through paintGL, QWidget through paintEvent,
# so the base class decides which of the two the class below defines. Read
# once at import: an overlay that changed surface type mid-run would have to
# rebuild its window.
_OverlayBase = QOpenGLWidget if GL_OVERLAY else QWidget


TARGET_FPS = _target_fps()
FRAME_INTERVAL_MS = _frame_interval_ms_for_fps(TARGET_FPS)
SCREEN_GEOMETRY_CHECK_MS = 1500
# Window/icon enumeration is useful but can become expensive if it runs too
# often. Refresh a little over once per second; the hide behaviour is deliberate
# and does not need frame-perfect window snapshots.
DESKTOP_SURFACE_CHECK_MS = 1250
# Camouflage now only animates opacity, so this is cheap and does not grab
# desktop pixels anymore.
CAMOUFLAGE_SAMPLE_MS = 120

# How often to check whether the launched preset file changed on disk.  The
# settings window rewrites it to push live edits, and the overlay reloads it
# without restarting.
PRESET_WATCH_MS = 700
# Extra repaint slack around each creature.  Antennae and tiny round endpoints can
# swing farther than the body/leg footprint during fast animation, especially on
# transparent top-level windows where missed pixels remain visible until a full
# refresh.
CREATURE_REPAINT_EXTRA_PAD_PX = 28
CREATURE_REPAINT_EXTRA_PAD_SIZE_MULT = 1.9
# Periodic whole-window clear is a safety net for compositor/driver edge cases.
# Keep it infrequent; repainting a full transparent desktop overlay too often is
# one of the easiest ways to cause lag on Windows.
FULL_REPAINT_SAFETY_FRAMES = 150


def left_mouse_button_down() -> bool:
    """Return the global left-button state even while the overlay is click-through."""
    if sys.platform.startswith("win"):
        try:
            import ctypes

            VK_LBUTTON = 0x01
            return bool(ctypes.windll.user32.GetAsyncKeyState(VK_LBUTTON) & 0x8000)
        except Exception:
            return False
    try:
        return bool(QApplication.mouseButtons() & Qt.LeftButton)
    except Exception:
        return False

def virtual_screen_geometry() -> QRect:
    """Union of every screen's geometry, in Qt logical (device-independent) pixels.

    ``QScreen.geometry()`` is documented to report device-independent pixels
    once ``Qt.AA_EnableHighDpiScaling`` is set (see dpi.py), and Qt lays out
    each screen's logical origin so adjacent monitors tile without gaps or
    overlaps even when they run at different scale factors. That means this
    function, `QCursor.pos()` and every widget-local coordinate in this file
    already share one consistent logical space and need no DPI conversion
    among themselves -- only the boundary into raw Win32 calls does (see
    `_refresh_desktop_surfaces` and the cursor-trap conversion in `tick`).
    """
    screens = QGuiApplication.screens()
    if not screens:
        return QRect(0, 0, 1280, 720)
    rect = screens[0].geometry()
    for screen in screens[1:]:
        rect = rect.united(screen.geometry())
    return rect


def screen_rects_local(origin: QPoint) -> list:
    """Every monitor as an overlay-local rectangle (DC-65).

    The overlay spans the *bounding box* of all monitors, and on a mixed
    layout that box contains regions no screen shows -- a 2560x1440 beside a
    1920x1080 leaves 1920x360 of nothing in the corner. A spider clamped only
    to the window can sit in there: updated, painted, and invisible.

    Same logical-pixel space as `virtual_screen_geometry`, so no DPI
    conversion is involved (see that function's note).
    """
    rects = []
    for screen in QGuiApplication.screens():
        g = screen.geometry()
        rects.append(ScreenRect(float(g.x() - origin.x()), float(g.y() - origin.y()),
                                float(g.width()), float(g.height())))
    return rects


class CreatureInspectorDialog(QDialog):
    """Compact runtime inspector for one live spider."""

    def __init__(self, window, creature):
        super().__init__(window)
        self.window = window
        self.creature = creature
        self.setWindowTitle(f"Inspect {creature.display_name}")
        self.setMinimumWidth(440)
        self.tabs = QTabWidget(self)
        root = QVBoxLayout(self)
        root.addWidget(self.tabs)

        self.status_tab = QWidget()
        status_layout = QVBoxLayout(self.status_tab)
        self.status_labels = {}
        form = QFormLayout()
        for key, title in (("level", "Level"), ("job", "Job"), ("hp", "HP"), ("energy", "Energy"),
                           ("armor", "Armor"), ("damage", "Damage"), ("team", "Team"),
                           ("relations", "Relations")):
            label = QLabel()
            label.setWordWrap(True)
            self.status_labels[key] = label
            form.addRow(f"{title}:", label)
        status_layout.addLayout(form)
        self.xp_bar = QProgressBar()
        self.xp_bar.setTextVisible(True)
        status_layout.addWidget(QLabel("Experience"))
        status_layout.addWidget(self.xp_bar)
        self.pin_check = QCheckBox("Pin level and XP above the spider's name")
        self.pin_check.toggled.connect(self._set_pin)
        status_layout.addWidget(self.pin_check)
        self.health_pin_check = QCheckBox("Pin the health bar above the spider")
        self.health_pin_check.setToolTip(
            "Keeps a small health bar on screen for this spider instead of only "
            "showing it here. Nothing can damage a spider yet, so it stays full."
        )
        self.health_pin_check.toggled.connect(self._set_health_pin)
        status_layout.addWidget(self.health_pin_check)
        self.team_combo = QComboBox()
        self.team_combo.setEditable(True)
        # The teams this scene actually has, under the names their owner gave
        # them, so a team picked before launch and a team picked here are
        # recognisably the same group rather than two similar-looking ids.
        self.team_combo.setToolTip(HOSTILITY_NOTE)
        self._team_signature = None
        self._refresh_team_choices()
        # Commit on a chosen entry or a finished edit, never on every keystroke:
        # ``currentTextChanged`` would assign (and persist) "h", "hu", "hun"…
        # while the user is still typing "hunters".
        self.team_combo.activated.connect(self._commit_team)
        self.team_combo.lineEdit().editingFinished.connect(self._commit_team)
        status_layout.addWidget(QLabel("Team assignment"))
        status_layout.addWidget(self.team_combo)
        team_note = QLabel(HOSTILITY_NOTE)
        team_note.setWordWrap(True)
        team_note.setStyleSheet("color: #6a7180;")
        status_layout.addWidget(team_note)
        status_layout.addWidget(QLabel("Relationship with other spiders"))
        self.relations_layout = QVBoxLayout()
        status_layout.addLayout(self.relations_layout)
        # Keep interactive controls stable between live-stat refreshes. Reusing
        # the combo boxes is important: deleting a combo while its popup is open
        # makes the menu disappear on the next 400 ms refresh tick.
        self._relation_signature = None
        self._relation_controls = {}
        self._abilities_signature = None
        self._inventory_signature = None
        self.tabs.addTab(self.status_tab, "Status")

        self.abilities_tab = QWidget()
        self.abilities_layout = QVBoxLayout(self.abilities_tab)
        self.tabs.addTab(self.abilities_tab, "Skill tree")

        self.inventory_tab = QWidget()
        self.inventory_layout = QVBoxLayout(self.inventory_tab)
        self.tabs.addTab(self.inventory_tab, "Inventory & armor")

        self.refresh_timer = QTimer(self)
        self.refresh_timer.timeout.connect(self.refresh)
        self.refresh_timer.start(400)
        self.refresh()

    @staticmethod
    def _clear_layout(layout) -> None:
        while layout.count():
            item = layout.takeAt(0)
            widget = item.widget()
            if widget is not None:
                widget.deleteLater()
            elif item.layout() is not None:
                child = item.layout()
                CreatureInspectorDialog._clear_layout(child)
                child.deleteLater()

    def _set_pin(self, enabled: bool) -> None:
        self.window._announce(self.window.manager.set_creature_level_pin(self.creature, enabled))

    def _set_health_pin(self, enabled: bool) -> None:
        self.window._announce(
            self.window.manager.set_creature_health_pin(self.creature, enabled))

    def _team_profiles(self) -> dict:
        return getattr(self.window.manager, "team_profiles", {}) or {}

    def _refresh_team_choices(self) -> None:
        """Offer every named team, plus whatever this spider is already on.

        Rebuilt only when the set of teams changes, because replacing the items
        of a combo while its popup is open closes the popup, and this dialog
        refreshes itself every 400 ms.
        """
        profiles = self._team_profiles()
        current = normalize_team_id(getattr(self.creature.progression, "team_id", "neutral"))
        signature = (tuple(sorted((tid, p.name) for tid, p in profiles.items())), current)
        if signature == getattr(self, "_team_signature", None):
            return
        self._team_signature = signature
        self.team_combo.blockSignals(True)
        self.team_combo.clear()
        self.team_combo.addItem("Neutral / solo", "neutral")
        for team_id in sorted(profiles):
            self.team_combo.addItem(profiles[team_id].name, team_id)
        if current != "neutral" and self.team_combo.findData(current) < 0:
            self.team_combo.addItem(team_label(current, profiles), current)
        index = self.team_combo.findData(current)
        self.team_combo.setCurrentIndex(index if index >= 0 else 0)
        self.team_combo.blockSignals(False)

    def _commit_team(self, *_args) -> None:
        text = str(self.team_combo.currentText()).strip()
        if not self.isVisible() or not text:
            return
        # A chosen entry carries its id. Typed text is a name, and a name the
        # scene already uses means that team rather than a new one with the same
        # label; anything else becomes a new team id derived from what was typed.
        index = self.team_combo.findText(text)
        if index >= 0 and self.team_combo.itemData(index) is not None:
            team = normalize_team_id(self.team_combo.itemData(index))
        else:
            team = normalize_team_id(text.replace(" ", "_"))
        if team == normalize_team_id(self.creature.progression.team_id):
            return
        self.window._announce(self.window.manager.set_creature_team(self.creature, team))
        self._team_signature = None
        self._refresh_team_choices()

    def _set_relation(self, other, relation: str) -> None:
        self.window._announce(self.window.manager.set_creature_relation(self.creature, other, relation))
        self.refresh()

    def _refresh_relations(self) -> None:
        others = [
            other for other in self.window.manager.creatures
            if other is not self.creature
        ]
        signature = tuple(id(other) for other in others)
        if signature != self._relation_signature:
            self._clear_layout(self.relations_layout)
            self._relation_controls = {}
            for other in others:
                row = QHBoxLayout()
                label = QLabel(other.display_name)
                row.addWidget(label, 1)
                combo = QComboBox()
                combo.addItems(["friend", "neutral", "foe"])
                combo.currentTextChanged.connect(
                    lambda relation, target=other: self._set_relation(target, relation)
                )
                row.addWidget(combo)
                self.relations_layout.addLayout(row)
                self._relation_controls[id(other)] = (other, label, combo)
            self._relation_signature = signature

        for other in others:
            entry = self._relation_controls.get(id(other))
            if entry is None:
                continue
            _stored_other, label, combo = entry
            label.setText(other.display_name)
            relation = self.creature.relation_to(other)
            # Do not disturb an actively opened popup. The selected value will
            # be synchronized on the next tick after the user closes it.
            if combo.currentText() != relation and not combo.view().isVisible():
                combo.blockSignals(True)
                combo.setCurrentText(relation)
                combo.blockSignals(False)

    def _unlock(self, ability_id: str) -> None:
        self.window._announce(self.window.manager.unlock_creature_ability(self.creature, ability_id))
        self.refresh()

    def _equip(self, item_id: str) -> None:
        self.window._announce(self.window.manager.equip_creature_item(self.creature, item_id))
        self.refresh()

    def _add_item(self, item_id: str) -> None:
        if self.creature.add_inventory_item(item_id):
            self.window.manager.save_runtime_state()
            self.window._announce("Added armor to this spider's inventory.")
        self.refresh()

    def _unequip(self, slot: str) -> None:
        self.window._announce(self.window.manager.unequip_creature_item(self.creature, slot))
        self.refresh()

    def refresh(self) -> None:
        if not self.creature or self.creature not in self.window.manager.creatures:
            self.close()
            return
        snapshot = self.creature.progression_snapshot()
        self.status_labels["level"].setText(f"{snapshot['level']} / 30  ·  {snapshot['skill_points']} point(s) available")
        self.status_labels["job"].setText(job_definition(self.creature.job_id).display_name)
        self.status_labels["hp"].setText(f"{snapshot['hp']:.0f} / {snapshot['max_hp']:.0f}")
        self.status_labels["energy"].setText(f"{snapshot['energy']:.0f} / {snapshot['max_energy']:.0f}")
        self.status_labels["armor"].setText(f"{snapshot['armor']:.1f}")
        self.status_labels["damage"].setText(f"{snapshot['damage']:.1f}")
        self.status_labels["team"].setText(
            team_label(self.creature.progression.team_id, self._team_profiles()))
        relations = []
        for other in self.window.manager.creatures:
            if other is self.creature:
                continue
            relations.append(f"{other.display_name}: {self.creature.relation_to(other)}")
        self.status_labels["relations"].setText(", ".join(relations) if relations else "No other spiders")
        self._refresh_relations()
        xp_max = xp_to_next_level(self.creature.level)
        self.xp_bar.setMaximum(max(1, xp_max))
        self.xp_bar.setValue(min(xp_max, snapshot["xp"] if self.creature.level < 30 else xp_max))
        self.xp_bar.setFormat("Level cap reached" if self.creature.level >= 30 else f"{snapshot['xp']} / {xp_max} XP")
        self.pin_check.blockSignals(True)
        self.pin_check.setChecked(self.creature.level_label_pinned)
        self.pin_check.blockSignals(False)
        self.health_pin_check.blockSignals(True)
        self.health_pin_check.setChecked(self.creature.health_label_pinned)
        self.health_pin_check.blockSignals(False)
        self._refresh_team_choices()
        self._refresh_abilities()
        self._refresh_inventory()

    def _refresh_abilities(self) -> None:
        signature = (
            self.creature.level,
            self.creature.progression.skill_points,
            tuple(self.creature.progression.unlocked_abilities),
        )
        if signature == self._abilities_signature:
            return
        self._clear_layout(self.abilities_layout)
        for node in ABILITY_TREE:
            row = QHBoxLayout()
            label = QLabel(f"{node.name} — {node.description}")
            label.setWordWrap(True)
            label.setToolTip(node.description)
            unlocked = node.id in self.creature.progression.unlocked_abilities
            button = QPushButton("Unlocked" if unlocked else f"Unlock ({node.cost})")
            button.setEnabled(not unlocked and self.creature.progression.can_unlock(node.id))
            button.clicked.connect(lambda checked=False, aid=node.id: self._unlock(aid))
            row.addWidget(label, 1)
            row.addWidget(button)
            self.abilities_layout.addLayout(row)
        self.abilities_layout.addStretch(1)
        self._abilities_signature = signature

    def _refresh_inventory(self) -> None:
        signature = (
            tuple(self.creature.progression.inventory),
            tuple(sorted(self.creature.progression.equipped.items())),
        )
        if signature == self._inventory_signature:
            return
        self._clear_layout(self.inventory_layout)
        equipped = self.creature.progression.equipped
        for item in ARMOR_CATALOG:
            owned = item.id in self.creature.progression.inventory
            equipped_here = equipped.get(item.slot) == item.id
            row = QHBoxLayout()
            label = QLabel(f"{item.name} [{item.slot}] — {item.description}")
            label.setWordWrap(True)
            action = QPushButton("Unequip" if equipped_here else ("Equip" if owned else "Add"))
            if equipped_here:
                action.clicked.connect(lambda checked=False, slot=item.slot: self._unequip(slot))
            elif owned:
                action.clicked.connect(lambda checked=False, iid=item.id: self._equip(iid))
            else:
                action.clicked.connect(lambda checked=False, iid=item.id: self._add_item(iid))
            row.addWidget(label, 1)
            row.addWidget(action)
            self.inventory_layout.addLayout(row)
        self.inventory_layout.addWidget(QLabel("Equipped: " + (", ".join(f"{slot}={item}" for slot, item in equipped.items()) or "nothing")))
        self.inventory_layout.addStretch(1)
        self._inventory_signature = signature

    def closeEvent(self, event):  # noqa: N802 - Qt API name
        self.refresh_timer.stop()
        super().closeEvent(event)


class OverlayWindow(_OverlayBase):
    def __init__(self, preset_path: Path, seed: int | None = None):
        super().__init__(None)
        self.setWindowTitle("Desktop Bug Companion Overlay")
        self.setWindowFlags(
            Qt.FramelessWindowHint
            | Qt.WindowStaysOnTopHint
            | Qt.Tool
            | Qt.BypassWindowManagerHint
        )
        self.setAttribute(Qt.WA_TranslucentBackground, True)
        self.setAttribute(Qt.WA_NoSystemBackground, True)
        # On Windows, nativeEvent returns HTTRANSPARENT away from spider pixels.
        # On other platforms we keep the previous fully click-through behavior.
        self.setAttribute(Qt.WA_TransparentForMouseEvents, not sys.platform.startswith("win"))
        self.setAttribute(Qt.WA_ShowWithoutActivating, True)
        if GL_OVERLAY:
            # Qt's documented requirement for a translucent QOpenGLWidget:
            # without it the GL surface is composited under the window's own
            # background rather than through it.
            self.setAttribute(Qt.WA_AlwaysStackOnTop, True)

        self.geometry_rect = virtual_screen_geometry()
        self._last_screen_check_ms = 0
        self._last_desktop_surface_check_ms = 0
        self._last_camouflage_sample_ms = 0
        self.setGeometry(self.geometry_rect)
        self.manager = CreatureManager(preset_path, self.width(), self.height(), seed=seed)
        self.manager.set_screen_rects(screen_rects_local(self.geometry_rect.topLeft()))
        for warning in self.manager.warnings:
            log.warning("%s", warning)

        # Live-reload bookkeeping: remember the preset this overlay was launched
        # from and its last-seen modification time, so edits saved by the
        # settings window can be applied without stopping the overlay.
        self._preset_path = Path(preset_path)
        self._last_preset_check_ms = 0
        try:
            self._last_preset_mtime = self._preset_path.stat().st_mtime
        except OSError:
            self._last_preset_mtime = 0.0

        # Session control: a stop request left by a crashed session would make
        # this overlay quit as soon as it finished loading, so clear it first.
        self._state_dir = state_dir()
        self._log_path = log_path(self._state_dir)
        self._stop_requested = False
        # The base picked up by "Move this base", waiting for a second
        # right-click to say where it goes. Held by id rather than by object
        # so a base deleted in between simply cancels the move.
        self._moving_base_id = None
        clear_stop_request(self._state_dir)

        # Live two-way channel to any settings window (DC-16). One local-socket
        # server per running overlay; the preset-file poll above and the
        # stop-request file both stay in place as fallbacks for a settings
        # window on an incompatible build, or for whenever the channel simply
        # fails to bind (another process already holds the name, no local
        # socket support in this environment, etc).
        self._channel_server = OverlayChannelServer(channel_name(self._state_dir), self)
        self._channel_server.message_received.connect(self._on_channel_message)
        self._channel_server.client_connected.connect(self._broadcast_session_state)
        if not self._channel_server.listen():
            log.warning("Live channel unavailable; settings window will use file polling only")

        self.elapsed = QElapsedTimer()
        self.elapsed.start()
        self.last_ms = self.elapsed.elapsed()
        self.last_left_mouse_down = False

        # Partial-repaint bookkeeping: only the area around the spiders (and any
        # cages) is repainted each frame instead of the whole transparent
        # desktop-sized surface, which is what keeps a full-screen overlay cheap.
        self._prev_region = QRegion()
        self._full_repaint_pending = True
        self._frames_since_full_repaint = 0
        self._last_hover_kind = None
        # Previous full footprint of each cage (keyed by id) so a moved or resized
        # cage repaints its whole old area and leaves no translucent ghost behind.
        self._cage_fp_prev = {}

        # Measurement, off unless DESKTOP_BUG_PROFILE asks for it. Held on the
        # window so the tray menu and the HUD can read the same numbers the
        # manager is recording into.
        self.profiler = profiler_from_env()
        self.show_profile_hud = hud_requested()

        self.current_fps = TARGET_FPS
        # A fullscreen window in front means nothing the overlay draws can be
        # seen, and a laptop on battery should not be paying for a pet at 60 Hz.
        # The policy owns the ceiling; the tray menu sets the target it works from.
        self.frame_policy = FramePolicy(TARGET_FPS)
        self.timer = QTimer(self)
        self.timer.setTimerType(Qt.PreciseTimer)
        self.timer.timeout.connect(self.tick)
        self.timer.start(FRAME_INTERVAL_MS)

        # DC-79: on the GL overlay, whether the window lets the mouse through
        # is decided on its own timer, not in `tick`. The frame rate can drop
        # to 1 FPS (FramePolicy, e.g. under a fullscreen app); tied to the
        # frame, a cursor last seen over a spider would leave the whole screen
        # captured for up to a second at a time.
        self.input_timer = QTimer(self)
        self.input_timer.timeout.connect(self._update_input_transparency)
        if GL_OVERLAY:
            self.input_timer.start(INPUT_CHECK_MS)

        # Qt/Windows can rewrite extended styles after show/repaint. Reapply periodically.
        self.style_timer = QTimer(self)
        self.style_timer.timeout.connect(lambda: apply_click_through(self))
        self.style_timer.start(1000)

    def showEvent(self, event):  # noqa: N802 - Qt API name
        super().showEvent(event)
        if GL_OVERLAY:
            # Start out letting the desktop keep its mouse; the input timer
            # takes it back only when the cursor is over something to grab.
            set_input_transparent(self, True)
        QTimer.singleShot(0, lambda: apply_click_through(self))
        QTimer.singleShot(0, self._refresh_desktop_surfaces)
        QTimer.singleShot(250, lambda: apply_click_through(self))

    def set_target_fps(self, fps: float) -> None:
        """Change animation FPS at runtime from the tray menu.

        This sets the ceiling rather than the rate: if a fullscreen window is in
        front or the machine is on battery, the policy still runs slower than
        what was asked for, and restores this rate when that stops being true.
        """
        target = max(10.0, min(60.0, float(fps)))
        self._apply_fps(self.frame_policy.set_target_fps(target))

    def _apply_fps(self, fps: float) -> None:
        self.current_fps = max(1.0, float(fps))
        self.timer.setInterval(_frame_interval_ms_for_fps(self.current_fps))
        # Reset timing so switching FPS does not produce one large simulation step.
        self.last_ms = self.elapsed.elapsed()

    def _refresh_desktop_surfaces(self) -> None:
        """Refresh visible real windows/folders for hide-behind behaviour."""
        try:
            hwnd = int(self.winId())
        except Exception:
            hwnd = None
        top_left = self.geometry_rect.topLeft()
        # snapshot_desktop_surfaces reads raw Win32 window/icon rectangles,
        # which are always native pixels. geometry_rect/width()/height() are
        # Qt logical pixels once high-DPI scaling is on, so convert the
        # origin/size into native pixels before the call, then scale the
        # native-local rectangles it returns back down into the logical
        # pixels the manager and every creature position live in (DC-14).
        # This uses one ratio for the whole snapshot (the screen under the
        # overlay's own origin), which is exact for a single monitor or a
        # multi-monitor desktop at one uniform scale; a real mixed-DPI rig
        # still needs the manual check this package's plan entry defers to a
        # person, since a window on a differently-scaled monitor than the
        # origin's would need its own ratio.
        dpr = screen_device_pixel_ratio_at(top_left.x(), top_left.y())
        physical_origin_x, physical_origin_y = logical_to_physical(top_left.x(), top_left.y(), dpr)
        physical_w, physical_h = logical_to_physical(self.width(), self.height(), dpr)
        surfaces = snapshot_desktop_surfaces(
            exclude_hwnd=hwnd,
            origin_x=int(round(physical_origin_x)),
            origin_y=int(round(physical_origin_y)),
            screen_w=int(round(physical_w)),
            screen_h=int(round(physical_h)),
            include_desktop_icons=self.manager.desktop_icons_enabled,
        )
        if dpr != 1.0:
            surfaces = [
                replace(surface, x=surface.x / dpr, y=surface.y / dpr, w=surface.w / dpr, h=surface.h / dpr)
                for surface in surfaces
            ]
        self.manager.set_desktop_surfaces(surfaces)

    def _refresh_camouflage_samples(self) -> None:
        """Animate visibility-only camouflage strength.

        Camouflage now behaves like a shy hide response: after spawn or touch it
        is fully visible, and it only fades down after being left alone for a
        personality-defined delay.  This still avoids all live screen capture, so
        the effect is cheap and stable over time.
        """
        sample_dt = CAMOUFLAGE_SAMPLE_MS / 1000.0
        for creature in self.manager.creatures:
            try:
                max_strength = max(0.0, min(1.0, float(creature.personality.get("camouflage_strength", 0.0) or 0.0)))
            except Exception:
                max_strength = 0.0
            if max_strength <= 0.0 and getattr(creature, "_camouflage_strength", 0.0) <= 0.001:
                creature._camouflage_strength = 0.0
                creature._camouflage_color = None
                continue

            # Being dragged, recently touched, or emerging from a window should
            # keep the spider fully visible.  The manager refreshes this timer
            # whenever the cursor or another visible spider touches it.
            if creature.dragging:
                try:
                    creature.register_camouflage_touch(visible_time=4.5)
                except Exception:
                    creature._camouflage_visible_timer = 4.5

            # Movement also reveals Camouflage.  He is meant to vanish only when
            # he is actually still; even one lifting/stepping leg should make him
            # pop back into the visible overlay layer.  Do not reveal a spider that
            # is fully hidden behind a real desktop window/folder portal.
            if not getattr(creature, "_desktop_fully_hidden", False):
                moving = False
                try:
                    speed_threshold = max(1.2, float(getattr(creature, "size", 24.0)) * 0.035)
                    moving = (
                        abs(float(getattr(creature, "vel_x", 0.0))) > speed_threshold
                        or abs(float(getattr(creature, "vel_y", 0.0))) > speed_threshold
                        or float(getattr(creature, "current_speed", 0.0)) > speed_threshold
                        or abs(float(getattr(creature, "inertia_vx", 0.0))) > speed_threshold
                        or abs(float(getattr(creature, "inertia_vy", 0.0))) > speed_threshold
                    )
                except Exception:
                    moving = False
                if not moving:
                    try:
                        for leg in getattr(creature, "legs", []):
                            if (
                                bool(getattr(leg, "stepping", False))
                                or bool(getattr(leg, "pending_step", False))
                                or float(getattr(leg, "lift", 0.0)) > 0.012
                            ):
                                moving = True
                                break
                    except Exception:
                        moving = False
                if moving:
                    try:
                        visible_time = float(creature.personality.get("camouflage_motion_visible_time", 1.25))
                    except Exception:
                        visible_time = 1.25
                    try:
                        creature.register_camouflage_touch(visible_time=max(0.35, min(3.0, visible_time)))
                    except Exception:
                        creature._camouflage_strength = 0.0
                        creature._camouflage_idle_timer = 0.0
                        creature._camouflage_visible_timer = max(0.35, min(3.0, visible_time))

            visible_timer = max(0.0, float(getattr(creature, "_camouflage_visible_timer", 0.0)))
            if visible_timer > 0.0:
                creature._camouflage_visible_timer = max(0.0, visible_timer - sample_dt)
                creature._camouflage_idle_timer = 0.0
                target_strength = 0.0
            else:
                creature._camouflage_idle_timer = max(0.0, float(getattr(creature, "_camouflage_idle_timer", 0.0)) + sample_dt)
                try:
                    idle_delay = max(0.0, float(creature.personality.get("camouflage_idle_delay", 5.0)))
                except Exception:
                    idle_delay = 5.0
                try:
                    fade_in_time = max(0.25, float(creature.personality.get("camouflage_hide_fade_time", 3.0)))
                except Exception:
                    fade_in_time = 3.0
                progress = max(0.0, min(1.0, (creature._camouflage_idle_timer - idle_delay) / fade_in_time))
                # smootherstep without importing extra helpers into this module.
                progress = progress * progress * progress * (progress * (progress * 6.0 - 15.0) + 10.0)
                target_strength = max_strength * progress

            current = max(0.0, min(1.0, float(getattr(creature, "_camouflage_strength", 0.0))))
            if target_strength <= current:
                # Touch/reveal should feel immediate.
                try:
                    speed = max(0.08, min(1.0, float(creature.personality.get("camouflage_reveal_speed", 0.90))))
                except Exception:
                    speed = 0.90
            else:
                try:
                    speed = max(0.02, min(1.0, float(creature.personality.get("camouflage_fade_speed", 0.18))))
                except Exception:
                    speed = 0.18
            creature._camouflage_strength = current + (target_strength - current) * speed
            if abs(creature._camouflage_strength) < 0.002:
                creature._camouflage_strength = 0.0
            creature._camouflage_color = None

    def nativeEvent(self, event_type, message):  # noqa: N802 - Qt API name
        # Let desktop clicks pass through empty transparent space, but allow the
        # OS to deliver mouse input when the cursor is over a spider.
        if sys.platform.startswith("win"):
            try:
                import ctypes
                from ctypes import wintypes

                class MSG(ctypes.Structure):
                    _fields_ = [
                        ("hwnd", wintypes.HWND),
                        ("message", wintypes.UINT),
                        ("wParam", wintypes.WPARAM),
                        ("lParam", wintypes.LPARAM),
                        ("time", wintypes.DWORD),
                        ("pt", wintypes.POINT),
                    ]

                msg = MSG.from_address(int(message))
                WM_NCHITTEST = 0x0084
                HTCLIENT = 1
                HTTRANSPARENT = -1
                if msg.message == WM_NCHITTEST:
                    global_pos = QCursor.pos()
                    local = global_pos - self.geometry_rect.topLeft()
                    mx = float(local.x())
                    my = float(local.y())
                    if self.manager.wants_mouse(mx, my):
                        return True, HTCLIENT
                    return True, HTTRANSPARENT
            except Exception:
                pass
        return super().nativeEvent(event_type, message)

    def _update_input_transparency(self) -> None:
        """GL overlay only: pass input through unless the cursor is over
        something interactive (DC-79; see overlay_win32.apply_click_through).

        The same question nativeEvent answers per hit-test, asked once a frame
        from the cursor position instead -- once the window is transparent to
        input it receives no hit-tests to answer. Written only on change.
        """
        if not GL_OVERLAY:
            return
        local = QCursor.pos() - self.geometry_rect.topLeft()
        transparent = not self.manager.wants_mouse(float(local.x()), float(local.y()))
        if transparent != getattr(self, "_input_transparent", None):
            set_input_transparent(self, transparent)

    def tick(self) -> None:
        self.profiler.begin_frame()
        current_ms = self.elapsed.elapsed()
        dt = max(0.001, min(0.05, (current_ms - self.last_ms) / 1000.0))
        self.last_ms = current_ms
        # Ask, a couple of times a second, whether this machine still deserves
        # the full frame rate. `poll` returns a number only when it changed.
        decided = self.frame_policy.poll(current_ms, exclude_hwnd=self._own_hwnd())
        if decided is not None:
            log.info("frame rate now %.0f FPS (%s)", decided, self.frame_policy.reason)
            self._apply_fps(decided)
        # Checking monitor geometry every frame is unnecessary work; it only
        # needs to react when displays are added/removed or resolution changes.
        if current_ms - self._last_screen_check_ms >= SCREEN_GEOMETRY_CHECK_MS:
            self._last_screen_check_ms = current_ms
            rect = virtual_screen_geometry()
            if rect != self.geometry_rect:
                self.geometry_rect = rect
                self.setGeometry(rect)
                self.manager.resize(self.width(), self.height())
                self.manager.set_screen_rects(screen_rects_local(rect.topLeft()))
                apply_click_through(self)
                self._request_full_repaint()
        if current_ms - self._last_desktop_surface_check_ms >= DESKTOP_SURFACE_CHECK_MS:
            self._last_desktop_surface_check_ms = current_ms
            # Timed separately because it still runs on the frame thread; DC-13
            # is the package that moves it off, and this is the evidence for it.
            with self.profiler.section("desktop-probe"):
                self._refresh_desktop_surfaces()
        if current_ms - self._last_preset_check_ms >= PRESET_WATCH_MS:
            self._last_preset_check_ms = current_ms
            if self._check_stop_request():
                return
            self._check_preset_reload()
        global_pos = QCursor.pos()
        local = global_pos - self.geometry_rect.topLeft()
        mx = float(local.x())
        my = float(local.y())

        if current_ms - self._last_camouflage_sample_ms >= CAMOUFLAGE_SAMPLE_MS:
            self._last_camouflage_sample_ms = current_ms
            self._refresh_camouflage_samples()

        mouse_down = left_mouse_button_down()
        mouse_pressed = mouse_down and not self.last_left_mouse_down
        mouse_released = self.last_left_mouse_down and not mouse_down
        self.last_left_mouse_down = mouse_down

        # A spider may be trapping or shoving the pointer with sticky silk. The
        # manager returns the overlay-local position the pointer should be forced
        # to this frame (or None to leave it alone). Only the engine can move the
        # real OS pointer, so apply it here, converting back to global pixels.
        desired = self.manager.update(dt, mx, my, mouse_down=mouse_down, mouse_pressed=mouse_pressed, mouse_released=mouse_released)
        if desired is not None:
            origin = self.geometry_rect.topLeft()
            logical_x = origin.x() + desired[0]
            logical_y = origin.y() + desired[1]
            # geometry_rect/desired are Qt logical (device-independent) pixels,
            # but SetCursorPos is a raw Win32 call that always takes native
            # pixels, so convert using the device pixel ratio of the screen
            # the target point actually falls on (DC-14).
            dpr = screen_device_pixel_ratio_at(logical_x, logical_y)
            physical_x, physical_y = logical_to_physical(logical_x, logical_y, dpr)
            set_cursor_pos(int(round(physical_x)), int(round(physical_y)))
        self._update_hover_and_cursor(mx, my)
        with self.profiler.section("repaint-region"):
            self.request_repaint()
        # Painting happens after this returns, when Qt delivers the paint event,
        # so it records a sample of its own rather than joining this frame.
        self.profiler.end_frame()

    def _own_hwnd(self):
        """This overlay's window handle, or None where there is not one."""
        try:
            return int(self.winId())
        except Exception:
            return None

    # ------------------------------------------------------------------
    # Hover labels and cursor feedback
    # ------------------------------------------------------------------
    def _update_hover_and_cursor(self, mx: float, my: float) -> None:
        if self.manager.naming_enabled or self.manager.always_show_names:
            self.manager.update_hover(mx, my)
        else:
            self.manager.clear_hover()

        # Cursor hints for cage manipulation / spider grabbing.
        kind = self.manager.cage_hover_kind(mx, my)
        cursor = None
        if kind == "move":
            cursor = Qt.SizeAllCursor
        elif kind in ("resize-nw", "resize-se"):
            cursor = Qt.SizeFDiagCursor
        elif kind in ("resize-ne", "resize-sw"):
            cursor = Qt.SizeBDiagCursor
        elif self.manager.interferable and self.manager.creature_at(mx, my) is not None:
            cursor = Qt.OpenHandCursor
        if kind != self._last_hover_kind or cursor is None:
            self._last_hover_kind = kind
            if cursor is None:
                self.unsetCursor()
            else:
                self.setCursor(QCursor(cursor))
        elif cursor is not None:
            self.setCursor(QCursor(cursor))

    # ------------------------------------------------------------------
    # Partial repaint
    # ------------------------------------------------------------------
    def request_repaint(self) -> None:
        self._frames_since_full_repaint += 1
        if self._full_repaint_pending or self._frames_since_full_repaint >= FULL_REPAINT_SAFETY_FRAMES:
            self._full_repaint_pending = False
            self._frames_since_full_repaint = 0
            self._prev_region = self._current_paint_region()
            self.update()
            return
        cur = self._current_paint_region()
        dirty = QRegion(cur)
        dirty += self._prev_region
        self._prev_region = cur
        self.update(dirty)

    @staticmethod
    def _rect_from_bbox(bbox) -> QRect:
        x0, y0, x1, y1 = bbox
        left = int(math.floor(x0))
        top = int(math.floor(y0))
        right = int(math.ceil(x1))
        bottom = int(math.ceil(y1))
        return QRect(left, top, max(1, right - left) + 1, max(1, bottom - top) + 1)

    @staticmethod
    def _expand_rect(rect: QRect, pad: int) -> QRect:
        if pad <= 0:
            return rect
        return rect.adjusted(-pad, -pad, pad, pad)

    @staticmethod
    def _rect_from_xywh(rect) -> QRect:
        x, y, w, h = rect
        return QRect(int(math.floor(x)), int(math.floor(y)),
                     int(math.ceil(w)) + 1, int(math.ceil(h)) + 1)

    def _current_paint_region(self) -> QRegion:
        region = QRegion()
        for creature in self.manager.creatures:
            bbox = creature.bounding_rect(self.manager.always_show_names)
            # Be deliberately generous here.  Antennae are thin anti-aliased
            # strokes with round bulbs; their old pixels are very noticeable on
            # a dark desktop if the dirty rect misses by even 1-2 px.  The extra
            # pad is small compared with the screen, but large enough for fast
            # feeler sway, jump squash/stretch and sprite transforms.
            extra_pad = int(max(CREATURE_REPAINT_EXTRA_PAD_PX, creature.size * CREATURE_REPAINT_EXTRA_PAD_SIZE_MULT))
            r = self._expand_rect(self._rect_from_bbox(bbox), extra_pad)
            region += r
            prev = creature._bbox_prev
            if prev is not None:
                pr = self._expand_rect(self._rect_from_bbox(prev), extra_pad)
                if not r.intersects(pr):
                    # Cover the streak between two non-overlapping positions so a
                    # hard throw or panic jump leaves no one-frame trail behind.
                    region += r.united(pr)
                else:
                    region += pr
            creature._bbox_prev = bbox
        new_fp_prev = {}
        for cage in self.manager.cages:
            key = id(cage)
            fp = cage.footprint_rect()
            prev_fp = self._cage_fp_prev.get(key)
            if prev_fp is not None and prev_fp != fp:
                # The cage moved or was resized this frame. Repaint the whole old and
                # new footprints (not just the thin border) so the translucent
                # interior wash is cleared at the old position and redrawn at the new
                # one, leaving no ghost rectangle behind.
                region += self._rect_from_xywh(prev_fp)
                region += self._rect_from_xywh(fp)
            else:
                # Stationary cage: only the thin fence bands need repainting, which
                # keeps the big open interior untouched and the repaint cheap.
                for band in cage.border_rects():
                    region += self._rect_from_xywh(band)
            new_fp_prev[key] = fp
        self._cage_fp_prev = new_fp_prev
        # Webs that are building, bouncing, or were just removed expose their
        # footprint for one or more frames; finished/idle webs stay on the
        # backing store and are only redrawn when an overlapping rect (e.g. a
        # walking spider) already forces a repaint of that area.
        web_world = getattr(self.manager, "web_world", None)
        if web_world is not None:
            for fp in web_world.dirty_rects():
                region += self._rect_from_xywh(fp)
        # Cursor-silk (the flying glob, the trap splat, the wall shove) moves with
        # the pointer every frame, so it always exposes its current and previous
        # footprint while active.
        mouse_web_world = getattr(self.manager, "mouse_web_world", None)
        if mouse_web_world is not None:
            for fp in mouse_web_world.dirty_rects():
                region += self._rect_from_xywh(fp)
        # Flies move every frame, so each active fly always exposes its current
        # (and previous) footprint; removed flies expose their last one.
        fly_world = getattr(self.manager, "fly_world", None)
        if fly_world is not None:
            for fp in fly_world.dirty_rects():
                region += self._rect_from_xywh(fp)
        if self.show_profile_hud:
            # The HUD changes every frame, so its panel has to be exposed every
            # frame or the old numbers stay on the backing store underneath.
            region += self._hud_rect()
        return region

    if GL_OVERLAY:
        def paintGL(self):  # noqa: N802 - Qt API name
            # No exposed region here: QOpenGLWidget redraws its whole
            # framebuffer, so the dirty-region culling in CreatureManager.render
            # has nothing to narrow. Measured, that costs nothing -- at twenty
            # spiders the dirty region is 30.6% of the screen in 101 rects and
            # clipping to it was 2.4% *slower* than painting the lot.
            with self.profiler.section("paint"):
                self._paint(QRegion(self.rect()))
    else:
        def paintEvent(self, event):  # noqa: N802 - Qt API name
            with self.profiler.section("paint"):
                self._paint(event.region())

    def _paint(self, region: QRegion) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.Antialiasing, True)
        # Clip to the exposed region so the manager can cull off-region spiders
        # and so clearing remains partial instead of wiping the whole overlay.
        painter.setClipRegion(region)

        # Transparent overlays with WA_NoSystemBackground are not automatically
        # erased before partial repaints.  Clear the exact exposed region with
        # CompositionMode_Clear; it is more reliable than painting a transparent
        # colour when the backing store is premultiplied-alpha.  This removes
        # stale antenna/leg endpoint pixels before the current frame is drawn.
        painter.setCompositionMode(QPainter.CompositionMode_Clear)
        try:
            rects = region.rects()
        except Exception:
            rects = [self.rect()]
        if not rects:
            rects = [self.rect()]
        for rect in rects:
            painter.fillRect(rect, Qt.transparent)
        painter.setCompositionMode(QPainter.CompositionMode_SourceOver)

        self.manager.render(painter)
        if self.show_profile_hud:
            self._draw_profile_hud(painter)
        painter.end()

    # The HUD is a debugging aid, not a feature: it only exists while
    # DESKTOP_BUG_PROFILE is set, and it is what makes a claim about frame cost
    # checkable while you watch the overlay rather than only in a benchmark.
    HUD_ORIGIN = (24, 24)
    HUD_LINE_HEIGHT = 15
    HUD_WIDTH = 260
    # Room for the header line plus every section the HUD will show.
    HUD_MAX_LINES = 9

    def _hud_rect(self) -> QRect:
        left, top = self.HUD_ORIGIN
        return QRect(left - 8, top - 8, self.HUD_WIDTH,
                     self.HUD_LINE_HEIGHT * self.HUD_MAX_LINES + 14)

    def _hud_header(self) -> str:
        return (f"{len(self.manager.creatures)} spiders @ {self.current_fps:.0f} FPS"
                f" ({self.frame_policy.reason})")

    def _draw_profile_hud(self, painter) -> None:
        lines = self.profiler.hud_lines()
        if not lines:
            return
        lines = [self._hud_header()] + lines
        left, top = self.HUD_ORIGIN
        painter.save()
        painter.setClipping(False)
        painter.fillRect(self._hud_rect(), QColor(0, 0, 0, 170))
        font = painter.font()
        font.setFamily("Consolas")
        font.setPointSizeF(8.5)
        painter.setFont(font)
        painter.setPen(QColor(210, 255, 210))
        for i, line in enumerate(lines):
            painter.drawText(left, top + 4 + self.HUD_LINE_HEIGHT * (i + 1), line)
        painter.restore()

    # ------------------------------------------------------------------
    # Right-click: name spiders and manage cages
    # ------------------------------------------------------------------
    def mousePressEvent(self, event):  # noqa: N802 - Qt API name
        if event.button() != Qt.RightButton:
            super().mousePressEvent(event)
            return
        gp = event.globalPos()
        local = gp - self.geometry_rect.topLeft()
        mx = float(local.x())
        my = float(local.y())
        self._show_context_menu(gp, mx, my)
        event.accept()

    def _show_context_menu(self, global_pos, mx: float, my: float) -> None:
        menu = QMenu(self)
        creature = self.manager.creature_at(mx, my)

        if creature is not None:
            inspect = menu.addAction("Inspect progression, inventory, and stats…")
            inspect.setToolTip("View level, XP, skill tree, armor, health, energy, and team relations.")
            inspect.triggered.connect(lambda: self._show_inspector(creature))
            pin = menu.addAction("Pin level above name")
            pin.setCheckable(True)
            pin.setChecked(creature.level_label_pinned)
            pin.triggered.connect(lambda enabled, c=creature: self._announce(self.manager.set_creature_level_pin(c, enabled)))
            menu.addSeparator()
            current = creature.name
            if current:
                rename = menu.addAction(f"Rename \u201c{current}\u201d\u2026")
                rename.triggered.connect(lambda: self._prompt_name(creature))
                clear = menu.addAction("Clear name")
                clear.triggered.connect(lambda: self._apply_name(creature, ""))
            else:
                name_it = menu.addAction("Name this spider\u2026")
                name_it.triggered.connect(lambda: self._prompt_name(creature))

            skills_menu = menu.addMenu("Skills for this spider")
            for skill in SKILLS:
                action = skills_menu.addAction(skill.display_name)
                action.setCheckable(True)
                action.setChecked(creature.has_skill(skill.id))
                action.setToolTip(skill.description)
                action.toggled.connect(
                    lambda enabled, sid=skill.id, c=creature: self._announce(self.manager.set_creature_skill(c, sid, enabled))
                )
            menu.addSeparator()

        # A base under the cursor gets its own entry, above the cage ones, so
        # the nearest thing to what was actually right-clicked comes first.
        base = self.manager.base_at(mx, my)
        # A base already picked up puts its destination first, because that is
        # the only thing the next click is for.
        carried = self._carried_base()
        if carried is not None and carried is not base:
            carried_name = team_label(carried.team_id, self.manager.team_profiles)
            drop = menu.addAction(f"Put the {carried_name} base down here")
            drop.setToolTip("Move the base you picked up to this spot, earth and all.")
            drop.triggered.connect(
                lambda _checked=False, site=carried, x=mx, y=my: self._drop_base(site, x, y))
            cancel = menu.addAction("Leave it where it is")
            cancel.triggered.connect(lambda _checked=False: self._cancel_base_move())
            menu.addSeparator()
        if base is not None:
            team_name = team_label(base.team_id, self.manager.team_profiles)
            if carried is base:
                cancel_here = menu.addAction(f"Leave the {team_name} base where it is")
                cancel_here.triggered.connect(lambda _checked=False: self._cancel_base_move())
            else:
                move_base = menu.addAction(f"Move the {team_name} base…")
                move_base.setToolTip(
                    "Pick this base up, then right-click where it should go. "
                    "It keeps its level, its food and the earth already dug. "
                    "You can also just drag the earth with the left button, "
                    "when dragging is switched on."
                )
                move_base.triggered.connect(
                    lambda _checked=False, site=base: self._pick_up_base(site))
            remove_base = menu.addAction(f"Remove the {team_name} base here")
            remove_base.setToolTip(
                "Delete this base. Its team keeps its spiders and its food; a "
                "Builder will found a new one."
            )
            remove_base.triggered.connect(
                lambda _checked=False, site=base: self._announce(self.manager.remove_base(site)))
            menu.addSeparator()
        if getattr(self.manager, "base_world", None) is not None and self.manager.base_world.bases:
            remove_all_bases = menu.addAction("Remove every base")
            remove_all_bases.setToolTip(
                "Clear the desktop of colonies. Builders start again from nothing."
            )
            remove_all_bases.triggered.connect(
                lambda _checked=False: self._announce(self.manager.remove_bases()))
            menu.addSeparator()

        add_cage = menu.addAction("Add a cage here")
        add_cage.triggered.connect(lambda: self._announce(self.manager.add_cage(mx, my)))
        if self.manager.cages:
            remove_cage = menu.addAction("Remove all cages")
            remove_cage.triggered.connect(lambda: self._announce(self.manager.remove_cages()))

        menu.addSeparator()
        _add_label_switches(menu, self.manager, self._announce)

        # Keep the overlay above other windows and force a clean full repaint
        # after the menu closes so new labels/cages appear immediately.
        menu.aboutToHide.connect(self._request_full_repaint)
        menu.exec_(global_pos)
        apply_click_through(self)

    def _carried_base(self):
        """The base waiting to be put down, if it still exists."""
        site_id = self._moving_base_id
        if site_id is None:
            return None
        world = getattr(self.manager, "base_world", None)
        if world is None:
            self._moving_base_id = None
            return None
        for site in world.bases.values():
            if site.id == site_id:
                return site
        # Removed while it was being carried; forget it rather than offering
        # to put down something that is gone.
        self._moving_base_id = None
        return None

    def _pick_up_base(self, site) -> None:
        self._moving_base_id = site.id
        name = team_label(site.team_id, self.manager.team_profiles)
        self._announce(f"Picked up the {name} base. Right-click where it should go.")

    def _cancel_base_move(self) -> None:
        self._moving_base_id = None
        self._announce("Left the base where it is.")

    def _drop_base(self, site, x: float, y: float) -> None:
        self._moving_base_id = None
        self._announce(self.manager.move_base(site, x, y))
        self._request_full_repaint()

    def _show_inspector(self, creature) -> None:
        dialog = CreatureInspectorDialog(self, creature)
        # The dialog is parented to the overlay, so Python dropping the local
        # reference does not destroy it. Without this, every inspector opened
        # during a session stays alive as a hidden child widget.
        dialog.setAttribute(Qt.WA_DeleteOnClose, True)
        dialog.exec_()
        self._request_full_repaint()
        apply_click_through(self)

    def _prompt_name(self, creature) -> None:
        text, ok = QInputDialog.getText(
            self,
            "Name this spider",
            "Spider name:",
            QLineEdit.Normal,
            creature.name,
        )
        if ok:
            self._apply_name(creature, text)
        apply_click_through(self)

    def _apply_name(self, creature, name: str) -> None:
        self._announce(self.manager.name_creature(creature, name))

    def _announce(self, message: str) -> None:
        try:
            if getattr(self, "_tray", None) is not None:
                self._tray.showMessage("Desktop Bug Companion", message, QSystemTrayIcon.Information, 1400)
        except Exception:
            pass
        self._request_full_repaint()
        apply_click_through(self)
        self._broadcast_session_state()

    def _notify_unwritable_state_dir(self, path) -> None:
        """Tell the user progress will not be saved this session (D3).

        `state_dir_is_writable` and the log open both failing mean the same
        thing: nothing dirtied this run reaches disk. Better said once, in a
        balloon a windowed build otherwise has no way to show, than left as a
        log line saved to a place that was just shown not to be writable.
        """
        tray = getattr(self, "_tray", None)
        if tray is None:
            return
        try:
            tray.showMessage(
                "Desktop Bug Companion",
                f"Could not write to {path}. Progress will not be saved this session.",
                QSystemTrayIcon.Warning,
                6000,
            )
        except Exception:
            log.debug("Could not show the unwritable-state-dir notification", exc_info=True)

    def _notify_crash(self, summary: str) -> None:
        """Tell the user something broke, since a windowed build shows nothing.

        Deliberately quiet about the detail: the tray balloon names the failure
        and points at the log, which is the thing worth sending on.
        """
        tray = getattr(self, "_tray", None)
        if tray is None:
            return
        where = getattr(self, "_log_path", None)
        detail = f"{summary}\n\nDetails were written to the log." if where is None else f"{summary}\n\nSee {where}"
        try:
            tray.showMessage("Desktop Bug Companion hit a problem", detail, QSystemTrayIcon.Warning, 6000)
        except Exception:
            log.debug("Could not show the crash notification", exc_info=True)

    def _request_full_repaint(self) -> None:
        self._full_repaint_pending = True

    def _do_graceful_stop(self) -> None:
        """Save and quit. Shared by the file-based and channel-based stop paths.

        This is the path that used to be a bare ``TerminateProcess``, which
        killed the overlay before anything could be written (DC-04). DC-16
        adds a second, faster way to reach it -- a ``stop_request`` over the
        live channel -- alongside the original stop-request file, so an old
        settings window or a channel that failed to bind still works exactly
        as before.
        """
        if self._stop_requested:
            return
        self._stop_requested = True
        self.timer.stop()
        # Save here rather than relying only on aboutToQuit, so the state is on
        # disk even if the event loop never gets to shut down cleanly.
        try:
            self.manager.save_runtime_state()
        except Exception:
            log.exception("Could not save runtime state while stopping")
        server = getattr(self, "_channel_server", None)
        if server is not None:
            server.close()
        app = QApplication.instance()
        if app is not None:
            app.quit()

    def _check_stop_request(self) -> bool:
        """Save and quit if the settings window asked the overlay to stop.

        Returns whether a stop was handled, so the caller can skip the rest
        of the frame. This is the file-based fallback; `_on_channel_message`
        handles the same request arriving over the live channel instead.
        """
        if self._stop_requested:
            return True
        if not consume_stop_request(self._state_dir):
            return False
        self._do_graceful_stop()
        return True

    def _on_channel_message(self, message: dict) -> None:
        """Handle a JSON message pushed by a connected settings window."""
        mtype = message.get("type")
        if mtype == "preset_update":
            self._apply_pushed_preset(message.get("data"), message.get("preset_path"))
        elif mtype == "stop_request":
            self._do_graceful_stop()
        elif mtype == "hello":
            # `client_connected` already queued a fresh broadcast; nothing else to do.
            pass
        else:
            log.debug("Ignoring unknown live-channel message type %r", mtype)

    def _apply_pushed_preset(self, data, preset_path) -> None:
        """Apply a preset the settings window pushed live over the channel.

        Mirrors `_check_preset_reload`'s file-based path so the two stay in
        sync, but takes effect immediately instead of waiting up to
        PRESET_WATCH_MS for the next poll.
        """
        if not isinstance(data, dict):
            return
        if preset_path:
            try:
                if Path(preset_path).resolve() != self._preset_path.resolve():
                    return
            except OSError:
                pass
        try:
            message = self.manager.reload_from_preset_data(data)
        except Exception:
            log.exception("Live reload via the live channel failed")
            return
        for warning in self.manager.warnings:
            log.warning("%s", warning)
        log.info("%s", message)
        # Keep the mtime bookkeeping in step so the file-polling fallback does
        # not immediately reload the same content again a moment later.
        try:
            self._last_preset_mtime = self._preset_path.stat().st_mtime
        except OSError:
            pass
        self._request_full_repaint()
        self._broadcast_session_state()

    def _broadcast_session_state(self) -> None:
        """Publish the tray-changeable state to every connected settings window."""
        server = getattr(self, "_channel_server", None)
        if server is None or server.client_count == 0:
            return
        try:
            server.broadcast({"type": "session_state", "state": self.manager.session_snapshot()})
        except Exception:
            log.debug("Could not broadcast session state", exc_info=True)

    def closeEvent(self, event):  # noqa: N802 - Qt API name
        # Closing the window is another exit that must not lose progress.
        try:
            self.manager.save_runtime_state()
        except Exception:
            log.exception("Could not save runtime state while closing")
        server = getattr(self, "_channel_server", None)
        if server is not None:
            server.close()
        super().closeEvent(event)

    def _check_preset_reload(self) -> None:
        """Reload the launched preset if its file changed, applying edits live."""
        try:
            mtime = self._preset_path.stat().st_mtime
        except OSError:
            return
        if mtime == self._last_preset_mtime:
            return
        try:
            data = load_preset(self._preset_path)
        except Exception:
            # The settings window may be mid-write; leave the mtime unchanged so
            # this retries on the next check once the file is complete.
            return
        self._last_preset_mtime = mtime
        try:
            message = self.manager.reload_from_preset_data(data)
        except Exception:
            log.exception("Live reload of %s failed", self._preset_path)
            return
        for warning in self.manager.warnings:
            log.warning("%s", warning)
        log.info("%s", message)
        self._request_full_repaint()


def _install_qt_message_handler() -> None:
    """Send Qt's own warnings to the log instead of a console nobody sees."""
    from PyQt5.QtCore import QtCriticalMsg, QtFatalMsg, QtWarningMsg, qInstallMessageHandler

    qt_log = get_logger("qt")
    levels = {QtWarningMsg: logging.WARNING, QtCriticalMsg: logging.ERROR, QtFatalMsg: logging.CRITICAL}

    def handler(mode, context, message):
        qt_log.log(levels.get(mode, logging.DEBUG), "%s", message)

    try:
        qInstallMessageHandler(handler)
    except Exception:
        qt_log.debug("Could not install the Qt message handler", exc_info=True)


def _fallback_tray_icon() -> QIcon:
    pixmap = QPixmap(32, 32)
    pixmap.fill(Qt.transparent)
    painter = QPainter(pixmap)
    painter.setRenderHint(QPainter.Antialiasing, True)
    painter.setPen(Qt.NoPen)
    painter.setBrush(QColor(40, 30, 24))
    painter.drawEllipse(7, 9, 18, 15)
    painter.setBrush(QColor(150, 55, 45))
    painter.drawEllipse(20, 12, 4, 4)
    painter.end()
    return QIcon(pixmap)



def _add_label_switches(parent, manager, announce) -> None:
    """The three "always show" switches, in one place.

    They appear in the overlay's right-click menu and in the tray menu, and
    having built them twice once already, the second copy is where the two
    drifted apart.
    """
    entries = (
        ("Always show names", "always_show_names", manager.set_always_show_names,
         "Show every spider's name without having to hover it."),
        ("Always show levels", "always_show_levels", manager.set_always_show_levels,
         "Show every spider's level beside its name."),
        ("Always show health bars", "always_show_health", manager.set_always_show_health,
         "Show every spider's health bar, not just ones pinned individually."),
        ("Always show XP bars", "always_show_xp", manager.set_always_show_xp,
         "Show every spider's progress to its next level under its health bar."),
    )
    for text, attribute, setter, tip in entries:
        action = parent.addAction(text)
        action.setCheckable(True)
        action.setChecked(bool(getattr(manager, attribute, False)))
        action.setToolTip(tip)
        action.toggled.connect(lambda on, fn=setter: announce(fn(on)))


def create_tray(app: QApplication, window: OverlayWindow) -> QSystemTrayIcon:
    icon = QIcon.fromTheme("applications-games")
    if icon.isNull():
        icon = _fallback_tray_icon()
    tray = QSystemTrayIcon(icon, app)
    tray.setToolTip("Desktop Bug Companion overlay")
    menu = QMenu("Desktop Bug Companion")
    try:
        menu.setToolTipsVisible(True)
    except Exception:
        pass

    def add_note(parent: QMenu, text: str):
        action = parent.addAction(text)
        action.setEnabled(False)
        return action

    initial_status = "Running - dragging is on." if window.manager.interferable else "Running - clicks pass through spiders."
    status_action = add_note(menu, initial_status)
    menu.addSeparator()

    def announce(message: str) -> None:
        status_action.setText(message)
        try:
            tray.showMessage("Desktop Bug Companion", message, QSystemTrayIcon.Information, 1400)
        except Exception:
            pass
        apply_click_through(window)
        window._broadcast_session_state()

    performance_menu = menu.addMenu("Performance")
    fps_group = QActionGroup(menu)
    fps_group.setExclusive(True)
    fps_options = [
        ("Smooth 60 FPS", 60),
        ("Still smooth 45 FPS", 45),
        ("Balanced 30 FPS", 30),
        ("Battery 20 FPS", 20),
    ]
    for label, fps in fps_options:
        action = performance_menu.addAction(label)
        action.setCheckable(True)
        # Check the rate the user chose, not the one the policy may have
        # throttled to, so a fullscreen game does not appear to move the tick.
        action.setChecked(abs(window.frame_policy.target_fps - fps) <= 2.0)
        fps_group.addAction(action)
        action.triggered.connect(lambda checked=False, f=fps, text=label: (window.set_target_fps(f), announce(f"Performance set to {text}.")))
    add_note(performance_menu, "Tip: fewer/lower-size spiders matter more than FPS.")

    appearance_menu = menu.addMenu("Appearance and mood")

    size_menu = appearance_menu.addMenu("Creature size")
    size_group = QActionGroup(menu)
    size_group.setExclusive(True)
    size_options = [
        ("Tiny (60%)", 0.60),
        ("Small (80%)", 0.80),
        ("Normal (100%)", 1.00),
        ("Large (125%)", 1.25),
        ("Huge (160%)", 1.60),
    ]
    for label, scale in size_options:
        action = size_menu.addAction(label)
        action.setCheckable(True)
        action.setChecked(abs(scale - window.manager.size_scale) < 1e-6)
        size_group.addAction(action)
        action.triggered.connect(lambda checked=False, s=scale, text=label: (window.manager.set_size_scale(s), announce(f"Size set to {text}.")))

    # DC-52: "Mood override" is gone from here as well as from the settings
    # window. It set every spider in the scene to one mood, over the top of
    # the temperament that already names one, and the owner asked for moods
    # to be consolidated into the temperament rather than overridden beside
    # it. `set_mood_mode` remains for a preset that still carries the key.

    random_menu = menu.addMenu("Randomize creatures")
    add_note(random_menu, "Changes apply immediately to the running overlay.")
    random_menu.addSeparator()

    random_model_action = random_menu.addAction("Randomize models")
    random_model_action.setToolTip("Keep the current count and personalities, but swap creature models.")
    random_model_action.triggered.connect(lambda: announce(window.manager.randomize_models()))

    random_personality_action = random_menu.addAction("Randomize personalities")
    random_personality_action.setToolTip("Keep the current models and count, but swap personalities.")
    random_personality_action.triggered.connect(lambda: announce(window.manager.randomize_personalities()))

    random_count_action = random_menu.addAction("Randomize count (1-10)")
    random_count_action.setToolTip("Keep the current model/personality pattern and pick a new total count.")
    random_count_action.triggered.connect(lambda: announce(window.manager.randomize_count(1, 10)))

    random_menu.addSeparator()
    random_all_action = random_menu.addAction("Surprise me: randomize everything")
    random_all_action.setToolTip("Pick a fresh count, models, personalities, and positions.")
    random_all_action.triggered.connect(lambda: announce(window.manager.randomize_everything()))

    interaction_menu = menu.addMenu("Interaction")
    interferable_action = interaction_menu.addAction("Allow dragging spiders")
    interferable_action.setCheckable(True)
    interferable_action.setChecked(window.manager.interferable)
    interferable_action.setToolTip("Empty overlay space stays click-through; this only affects spider pixels.")
    interferable_action.toggled.connect(
        lambda enabled: (
            window.manager.set_interferable(enabled),
            announce("Dragging enabled; spiders can be grabbed." if enabled else "Dragging disabled; clicks pass through spiders."),
        )
    )

    # DC-52: social play is a per-spider skill, weighted by each
    # temperament's sociability, and the per-spider "Skills for this spider"
    # submenu on the overlay's own right-click still lists it. This was a
    # master switch on top of that, and all it could do was make a sociable
    # spider antisocial.

    mouse_web_action = interaction_menu.addAction("Let spiders web-trap the mouse")
    mouse_web_action.setCheckable(True)
    mouse_web_action.setChecked(window.manager.allow_mouse_capture)
    mouse_web_action.setToolTip(
        "When enabled, spiders with the web-trap or wall-pin skill can shoot sticky "
        "silk that briefly catches the pointer. Wiggle the mouse to break free. "
        "Turn this off to stop spiders ever moving your pointer."
    )
    mouse_web_action.toggled.connect(lambda enabled: announce(window.manager.set_allow_mouse_capture(enabled)))

    naming_action = interaction_menu.addAction("Right-click a spider to name it")
    naming_action.setCheckable(True)
    naming_action.setChecked(window.manager.naming_enabled)
    naming_action.setToolTip("Lets you right-click a spider to give it a name. Disable to make spiders click-through when dragging is also off.")
    naming_action.toggled.connect(lambda enabled: announce(window.manager.set_naming_enabled(enabled)))

    # Names, levels and health bars, from the one place that builds them, so
    # this menu and the overlay's right-click menu cannot drift apart. The
    # repaint matters here: a label appearing or vanishing changes each
    # spider's bounding box, and partial repaints would leave the old one
    # behind on the desktop.
    def _announce_and_repaint(message):
        window._request_full_repaint()
        return announce(message)

    _add_label_switches(interaction_menu, window.manager, _announce_and_repaint)

    flies_menu = menu.addMenu("Flies")
    add_note(flies_menu, "Flies buzz around for the spiders to hunt, trap, and eat.")
    flies_menu.addSeparator()

    flies_on_action = flies_menu.addAction("Flies on")
    flies_on_action.setCheckable(True)
    flies_on_action.setChecked(window.manager.flies_enabled)
    flies_on_action.setToolTip("Turn the fly spawner on or off. Spiders only hunt when flies are on.")
    flies_on_action.toggled.connect(lambda enabled: announce(window.manager.set_flies_enabled(enabled)))

    flies_menu.addSeparator()
    rate_menu = flies_menu.addMenu("Spawn rate")
    rate_group = QActionGroup(menu)
    rate_group.setExclusive(True)
    rate_options = [
        ("Sparse (every 9-18s, up to 3)", "sparse"),
        ("Normal (every 4-9s, up to 6)", "normal"),
        ("Swarm (every 1-3s, up to 14)", "swarm"),
    ]
    for label, key in rate_options:
        action = rate_menu.addAction(label)
        action.setCheckable(True)
        rate_group.addAction(action)
        action.triggered.connect(
            lambda checked=False, k=key: announce(window.manager.set_fly_rate_preset(k))
        )
    add_note(rate_menu, "Fine-grained spawn timing lives in the settings window.")

    release_action = flies_menu.addAction("Release a fly now")
    release_action.setToolTip("Drop a single fly onto the screen immediately.")
    release_action.triggered.connect(lambda: announce(window.manager.release_fly()))

    flies_menu.addSeparator()
    nest_action = flies_menu.addAction("Flies emerge from a nest")
    nest_action.setCheckable(True)
    nest_action.setChecked(getattr(window.manager, "flies_spawner", True))
    nest_action.setToolTip("When on, flies crawl out of a movable nest you can drag around. "
                           "When off, they drift in from the screen edges.")
    nest_action.toggled.connect(
        lambda enabled: (window._request_full_repaint(),
                         announce(window.manager.set_fly_spawner_enabled(enabled)))
    )
    add_nest_action = flies_menu.addAction("Add another nest")
    add_nest_action.setToolTip("Drop an extra fly nest on the screen. Drag any nest to move it.")
    add_nest_action.triggered.connect(
        lambda: (window._request_full_repaint(), announce(window.manager.add_fly_spawner()))
    )
    reset_nest_action = flies_menu.addAction("Reset nest position")
    reset_nest_action.setToolTip("Remove extra nests and put one back in its default spot.")
    reset_nest_action.triggered.connect(
        lambda: (window._request_full_repaint(), announce(window.manager.reset_fly_spawner()))
    )

    cages_menu = menu.addMenu("Cages")
    add_note(cages_menu, "A cage keeps any spider you drop inside from wandering out.")
    cages_menu.addSeparator()
    add_cage_action = cages_menu.addAction("Add a cage")
    add_cage_action.setToolTip("Drop a resizable cage in the middle of the screen. Drag its frame to move it, corners to resize.")
    add_cage_action.triggered.connect(
        lambda: (window._request_full_repaint(), announce(window.manager.add_cage()))
    )
    remove_cage_action = cages_menu.addAction("Remove all cages")
    remove_cage_action.setToolTip("Delete every cage. The spiders inside roam freely again.")
    remove_cage_action.triggered.connect(
        lambda: (window._request_full_repaint(), announce(window.manager.remove_cages()))
    )

    help_menu = menu.addMenu("Help")
    add_note(help_menu, "Right-click this tray icon for controls.")
    add_note(help_menu, "Empty overlay space stays click-through.")
    add_note(help_menu, "Enable dragging to grab a spider by its body.")
    add_note(help_menu, "Right-click a spider to name it; hover to see the name.")
    add_note(help_menu, "Add a cage, then drop spiders inside to keep them there.")

    menu.addSeparator()
    quit_action = menu.addAction("Quit overlay")
    quit_action.triggered.connect(app.quit)

    tray.setContextMenu(menu)
    tray.show()
    return tray

def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Run the transparent Desktop Bug Companion overlay")
    parser.add_argument("--preset", default="presets/default.json", help="Path to preset JSON")
    parser.add_argument("--verbose", action="store_true", help="Log debug detail as well")
    parser.add_argument(
        "--seed", type=int, default=None,
        help="Seed every spider's randomness so the run can be replayed",
    )
    args = parser.parse_args(argv)

    # Before configure_logging, so a first run after DC-15 logs to the new
    # location rather than the one it just copied out of.
    migrate_legacy_state_dir()

    # Before anything that can fail, so a startup problem is recorded rather
    # than lost: a windowed build has no console to print it to.
    written_to = configure_logging(state_dir(), logging.DEBUG if args.verbose else logging.INFO)
    log.info("Desktop Bug Companion %s overlay starting (frozen=%s)", __version__, getattr(sys, "frozen", False))
    if written_to is None:
        log.warning("No log file could be opened under %s", state_dir())

    enable_high_dpi_scaling()
    if configure_gl_surface():
        log.info("Painting the overlay onto an OpenGL surface (DESKTOP_BUG_GL=0 to turn off)")
    app = QApplication.instance() or QApplication(sys.argv[:1])
    app.setQuitOnLastWindowClosed(False)
    # Must search the bundled data too. A one-file build keeps its presets in
    # the directory it extracts itself into, not beside the executable.
    preset = resolve_preset_path(args.preset)
    window = OverlayWindow(preset, seed=args.seed)
    window.show()
    apply_click_through(window)
    app.aboutToQuit.connect(window.manager.save_runtime_state)
    tray = create_tray(app, window)
    # Keep a reference alive.
    window._tray = tray
    if written_to is None:
        # A log that could not be opened means state_dir() is not writable
        # either (D3): the same directory holds both. A windowed build has no
        # console, so without this the only trace is a log line nobody sees.
        window._notify_unwritable_state_dir(state_dir())

    # The tray does not exist until now, so the notifier finds it lazily; a
    # crash before this point still reaches the log.
    install_excepthook(notify=window._notify_crash)
    _install_qt_message_handler()
    log.info("Overlay ready: preset=%s seed=%s log=%s", preset, args.seed, written_to)

    # Let Ctrl+C work in development consoles.
    try:
        signal.signal(signal.SIGINT, lambda *_: app.quit())
    except Exception:
        log.debug("Could not install a SIGINT handler", exc_info=True)
    return app.exec_()


if __name__ == "__main__":
    raise SystemExit(main())
