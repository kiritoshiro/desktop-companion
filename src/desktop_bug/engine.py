from __future__ import annotations

import argparse
import logging
import math
import os
import signal
import sys
from pathlib import Path

from PyQt5.QtCore import QElapsedTimer, QRect, QTimer, Qt
from PyQt5.QtGui import QColor, QCursor, QGuiApplication, QIcon, QPainter, QPixmap, QRegion
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
    QPushButton,
    QProgressBar,
    QTabWidget,
    QVBoxLayout,
    QSystemTrayIcon,
    QWidget,
)

from . import __version__
from .discovery import resolve_preset_path, state_dir
from .logging_setup import configure_logging, get_logger, install_excepthook, log_path
from .session_control import clear_stop_request, consume_stop_request
from .manager import CreatureManager
from .preset_io import load_preset
from .overlay_win32 import apply_click_through, set_cursor_pos
from .desktop_environment import snapshot_desktop_surfaces
from .frame_policy import FramePolicy
from .profiling import hud_requested, profiler_from_env
from .skills import SKILLS
from .progression import ABILITY_TREE, ARMOR_CATALOG, normalize_team_id, xp_to_next_level
from .teams import HOSTILITY_NOTE, team_label
from .jobs import job_definition


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


def _frame_interval_ms_for_fps(fps: float) -> int:
    return max(16, int(round(1000.0 / max(1.0, fps))))


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
    screens = QGuiApplication.screens()
    if not screens:
        return QRect(0, 0, 1280, 720)
    rect = screens[0].geometry()
    for screen in screens[1:]:
        rect = rect.united(screen.geometry())
    return rect


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


class OverlayWindow(QWidget):
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

        self.geometry_rect = virtual_screen_geometry()
        self._last_screen_check_ms = 0
        self._last_desktop_surface_check_ms = 0
        self._last_camouflage_sample_ms = 0
        self.setGeometry(self.geometry_rect)
        self.manager = CreatureManager(preset_path, self.width(), self.height(), seed=seed)
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
        clear_stop_request(self._state_dir)

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

        # Qt/Windows can rewrite extended styles after show/repaint. Reapply periodically.
        self.style_timer = QTimer(self)
        self.style_timer.timeout.connect(lambda: apply_click_through(self))
        self.style_timer.start(1000)

    def showEvent(self, event):  # noqa: N802 - Qt API name
        super().showEvent(event)
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
        self.manager.set_desktop_surfaces(snapshot_desktop_surfaces(
            exclude_hwnd=hwnd,
            origin_x=int(top_left.x()),
            origin_y=int(top_left.y()),
            screen_w=int(self.width()),
            screen_h=int(self.height()),
        ))

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

        self.manager.update(dt, mx, my, mouse_down=mouse_down, mouse_pressed=mouse_pressed, mouse_released=mouse_released)
        # A spider may be trapping or shoving the pointer with sticky silk. The
        # manager returns the overlay-local position the pointer should be forced
        # to this frame (or None to leave it alone). Only the engine can move the
        # real OS pointer, so apply it here, converting back to global pixels.
        desired = self.manager._desired_cursor
        if desired is not None:
            origin = self.geometry_rect.topLeft()
            set_cursor_pos(int(round(origin.x() + desired[0])),
                           int(round(origin.y() + desired[1])))
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

    def paintEvent(self, event):  # noqa: N802 - Qt API name
        with self.profiler.section("paint"):
            self._paint(event)

    def _paint(self, event) -> None:
        painter = QPainter(self)
        painter.setRenderHint(QPainter.Antialiasing, True)
        # Clip to the exposed region so the manager can cull off-region spiders
        # and so clearing remains partial instead of wiping the whole overlay.
        painter.setClipRegion(event.region())

        # Transparent overlays with WA_NoSystemBackground are not automatically
        # erased before partial repaints.  Clear the exact exposed region with
        # CompositionMode_Clear; it is more reliable than painting a transparent
        # colour when the backing store is premultiplied-alpha.  This removes
        # stale antenna/leg endpoint pixels before the current frame is drawn.
        painter.setCompositionMode(QPainter.CompositionMode_Clear)
        try:
            rects = event.region().rects()
        except Exception:
            rects = [event.rect()]
        if not rects:
            rects = [event.rect()]
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

        add_cage = menu.addAction("Add a cage here")
        add_cage.triggered.connect(lambda: self._announce(self.manager.add_cage(mx, my)))
        if self.manager.cages:
            remove_cage = menu.addAction("Remove all cages")
            remove_cage.triggered.connect(lambda: self._announce(self.manager.remove_cages()))

        menu.addSeparator()
        show_names = menu.addAction("Always show names")
        show_names.setCheckable(True)
        show_names.setChecked(self.manager.always_show_names)
        show_names.toggled.connect(lambda on: self._announce(self.manager.set_always_show_names(on)))

        # Keep the overlay above other windows and force a clean full repaint
        # after the menu closes so new labels/cages appear immediately.
        menu.aboutToHide.connect(self._request_full_repaint)
        menu.exec_(global_pos)
        apply_click_through(self)

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

    def _check_stop_request(self) -> bool:
        """Save and quit if the settings window asked the overlay to stop.

        This is the path that used to be a bare ``TerminateProcess``, which
        killed the overlay before anything could be written. Returns whether a
        stop was handled, so the caller can skip the rest of the frame.
        """
        if self._stop_requested:
            return True
        if not consume_stop_request(self._state_dir):
            return False
        self._stop_requested = True
        self.timer.stop()
        # Save here rather than relying only on aboutToQuit, so the state is on
        # disk even if the event loop never gets to shut down cleanly.
        try:
            self.manager.save_runtime_state()
        except Exception:
            log.exception("Could not save runtime state while stopping")
        app = QApplication.instance()
        if app is not None:
            app.quit()
        return True

    def closeEvent(self, event):  # noqa: N802 - Qt API name
        # Closing the window is another exit that must not lose progress.
        try:
            self.manager.save_runtime_state()
        except Exception:
            log.exception("Could not save runtime state while closing")
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

    mood_menu = appearance_menu.addMenu("Mood override")
    mood_group = QActionGroup(menu)
    mood_group.setExclusive(True)
    mood_options = [
        ("Auto - use each personality", "auto"),
        ("Playful", "playful"),
        ("Cuddly", "cuddly"),
        ("Curious", "curious"),
        ("Calm", "calm"),
    ]
    for label, mode in mood_options:
        action = mood_menu.addAction(label)
        action.setCheckable(True)
        action.setChecked(mode == window.manager.mood_mode)
        mood_group.addAction(action)
        action.triggered.connect(lambda checked=False, m=mode: announce(window.manager.set_mood_mode(m)))

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

    social_action = interaction_menu.addAction("Allow spiders to play together")
    social_action.setCheckable(True)
    social_action.setChecked(window.manager.social_play)
    social_action.setToolTip("When enabled, multiple spiders can seek each other out for social play.")
    social_action.toggled.connect(lambda enabled: announce(window.manager.set_social_play(enabled)))

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

    names_action = interaction_menu.addAction("Always show spider names")
    names_action.setCheckable(True)
    names_action.setChecked(window.manager.always_show_names)
    names_action.setToolTip("Show every named spider's label all the time instead of only on hover.")
    names_action.toggled.connect(
        lambda enabled: (window._request_full_repaint(), announce(window.manager.set_always_show_names(enabled)))
    )

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

    # Before anything that can fail, so a startup problem is recorded rather
    # than lost: a windowed build has no console to print it to.
    written_to = configure_logging(state_dir(), logging.DEBUG if args.verbose else logging.INFO)
    log.info("Desktop Bug Companion %s overlay starting (frozen=%s)", __version__, getattr(sys, "frozen", False))
    if written_to is None:
        log.warning("No log file could be opened under %s", state_dir())

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
