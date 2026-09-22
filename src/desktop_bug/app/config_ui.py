from __future__ import annotations

import argparse
import os
import random
import subprocess
import sys
import time
import uuid
from pathlib import Path

from PyQt5.QtCore import QSize, QTimer, Qt
from PyQt5.QtGui import QColor, QIcon, QPainter, QPen, QPixmap
from PyQt5.QtWidgets import (
    QApplication,
    QCheckBox,
    QComboBox,
    QColorDialog,
    QDialog,
    QDialogButtonBox,
    QDoubleSpinBox,
    QGridLayout,
    QGroupBox,
    QHBoxLayout,
    QHeaderView,
    QLabel,
    QLineEdit,
    QMainWindow,
    QInputDialog,
    QMessageBox,
    QPushButton,
    QSpinBox,
    QTableWidget,
    QTableWidgetItem,
    QVBoxLayout,
    QWidget,
)

from .. import __version__
from ..content.discovery import app_root, discover_models, discover_personalities, discover_presets, find_data_file, migrate_legacy_state_dir, state_dir, user_presets_dir
from ..support.dpi import enable_high_dpi_scaling
from ..support.logging_setup import configure_logging, get_logger
from .session_control import clear_stop_request, stop_process
from .live_channel import SettingsChannelClient, channel_name
from ..content.preset_io import load_preset, save_preset, safe_preset_filename, validate_preset
from ..world.jobs import JOB_OPTIONS, job_ability_ids, normalize_job_id
from ..content.personality_profiles import selectable_personality_ids
from ..state.progression import normalize_team_id, normalize_team_stances
from ..state.teams import (
    HOSTILITY_NOTE,
    STANCE_LABELS,
    TeamProfile,
    default_color,
    describe_stance,
    normalize_team_name,
    normalize_teams,
    minimal_stances,
    stance_pairs,
    team_label,
    teams_payload,
)
from ..content.body_plans import BODY_PLAN_IDS
from ..content.skills import (
    SKILLS,
    compact_ability_summary,
    normalize_ability_ids,
    normalize_skill_ids,
    skills_with_default_abilities,
    skills_with_selected_abilities,
    COMMON_SKILL_IDS,
)


log = get_logger("config_ui")

RANDOM_MODEL_ID = "__random_model__"
RANDOM_PERSONALITY_ID = "__random_personality__"
MODEL_ICON_SIZE = 56
MODEL_ICON_CANVAS = 96
SIZE_OPTIONS = [
    ("Tiny (60%)", 0.60),
    ("Small (80%)", 0.80),
    ("Normal (100%)", 1.00),
    ("Large (125%)", 1.25),
    ("Huge (160%)", 1.60),
]
MOOD_OPTIONS = [
    ("Auto - use each personality", "auto"),
    ("Playful", "playful"),
    ("Cuddly", "cuddly"),
    ("Curious", "curious"),
    ("Calm", "calm"),
]
MOVEMENT_OPTIONS = [
    ("Classic - original gait", "classic"),
    ("Lively - lifts legs + feels objects", "lively"),
    ("Skitter - rapid bursts + tiny stops", "skitter"),
]

COLOR_KEYS = (
    ("body", "Body"),
    ("legs", "Legs"),
    ("highlight", "Highlights"),
    ("eyes", "Eyes"),
    ("leg_band", "Leg bands"),
    ("leg_dark", "Leg shadows"),
    ("leg_tip", "Leg tips"),
)


def _normalize_color_overrides(value):
    """Return safe RGB lists for the optional per-slot palette."""
    if not isinstance(value, dict):
        return {}
    normalized = {}
    for key, rgb in value.items():
        if not isinstance(key, str) or not key.strip() or not isinstance(rgb, (list, tuple)) or len(rgb) != 3:
            continue
        try:
            channels = [max(0, min(255, int(float(channel)))) for channel in rgb]
        except (TypeError, ValueError):
            continue
        normalized[key.strip()] = channels
    return normalized


class NoScrollComboBox(QComboBox):
    """Combo box that ignores mouse-wheel scrolling.

    Dropdowns sit inside the scrollable creature list, so a stray wheel turn
    while the cursor passes over one used to silently change its value (and
    swallow the scroll) instead of moving the list. Ignoring the wheel here
    lets the event bubble up so the list of spiders scrolls as expected; the
    value can still be changed by clicking or with the keyboard.
    """

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        # Drop WheelFocus so hovering + scrolling never grabs focus either.
        self.setFocusPolicy(Qt.StrongFocus)

    def wheelEvent(self, event):  # noqa: N802 - Qt API name
        event.ignore()


class NoScrollSpinBox(QSpinBox):
    """Spin box that ignores mouse-wheel scrolling (see NoScrollComboBox)."""

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.setFocusPolicy(Qt.StrongFocus)

    def wheelEvent(self, event):  # noqa: N802 - Qt API name
        event.ignore()


class NoScrollDoubleSpinBox(QDoubleSpinBox):
    """Double spin box that ignores mouse-wheel scrolling (see NoScrollComboBox)."""

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.setFocusPolicy(Qt.StrongFocus)

    def wheelEvent(self, event):  # noqa: N802 - Qt API name
        event.ignore()


# The slot table's columns, in order. Spelled out once because the row builder,
# collect_slots and update_summary all index into them by number, and the
# "Pick 1-10" removal had to renumber every one of those call sites.
# DC-49 reduced forty-one leg rigs to four body plans, which made the single
# "Creature model" dropdown of forty-nine entries the wrong shape: a person
# picking a spider is really choosing a *kind* and then a *look*. The column
# splits into Category (the body plan, four choices) and Skin (the models
# built on it), with the colour swatch beside them, so those three columns
# together are the whole appearance of a slot.
SLOT_HEADERS = ["Category", "Skin", "Colors", "Temperament", "How many",
                "Abilities", "Team", "Job", ""]
SLOT_COLUMNS = len(SLOT_HEADERS)
(COL_CATEGORY, COL_SKIN, COL_COLORS, COL_TEMPERAMENT, COL_COUNT,
 COL_ABILITIES, COL_TEAM, COL_JOB, COL_REMOVE) = range(SLOT_COLUMNS)
# The skin dropdown is the one that still carries a model id, so everything
# that used to read the model column reads this one.
COL_MODEL = COL_SKIN
RANDOM_CATEGORY_ID = "__random_category__"
BODY_PLAN_LABELS = {
    "bug": "Bug",
    "segmented": "Segmented",
    "jumper": "Jumper",
    "tarantula": "Tarantula",
}


class SlotTable(QTableWidget):
    def __init__(self, parent=None):
        super().__init__(0, SLOT_COLUMNS, parent)
        self.setHorizontalHeaderLabels(SLOT_HEADERS)
        # The last two columns hold icon-only controls; the header text would be
        # wider than the button underneath it.
        self.horizontalHeaderItem(SLOT_COLUMNS - 1).setToolTip("Remove a creature slot")
        self.horizontalHeader().setStretchLastSection(False)
        # Skin and Temperament carry the long names, so they take the slack.
        self.horizontalHeader().setSectionResizeMode(COL_SKIN, QHeaderView.Stretch)
        self.horizontalHeader().setSectionResizeMode(COL_TEMPERAMENT, QHeaderView.Stretch)
        for col in (COL_CATEGORY, COL_COLORS, COL_COUNT, COL_ABILITIES,
                    COL_TEAM, COL_JOB, COL_REMOVE):
            self.horizontalHeader().setSectionResizeMode(col, QHeaderView.ResizeToContents)
        self.verticalHeader().setVisible(False)
        self.setAlternatingRowColors(True)
        self.setSelectionBehavior(self.SelectRows)
        self.setSelectionMode(self.SingleSelection)
        self.setEditTriggers(self.NoEditTriggers)
        self.setShowGrid(False)
        # Taller by default so several creature rows are visible at once and the
        # model thumbnails are not crowded by their labels.
        self.setMinimumHeight(240)


class ConfigWindow(QMainWindow):
    def __init__(self):
        super().__init__()
        self.root = app_root()
        self.models = {}
        self.personalities = {}
        self.model_icon_cache = {}
        self.overlay_process = None
        # Path of the preset the running overlay was launched from.  Saving while
        # the overlay runs rewrites this file, which the overlay watches and
        # reloads, so edits apply live without stopping it.
        self.launched_preset_path = None
        # Live two-way channel to a running overlay (DC-16). Connected lazily
        # (see `_ensure_channel_connected`) because there may be no overlay
        # running yet, or it may be an older build with no server; the preset
        # file write below and `session_control`'s stop-request file both stay
        # in place as the fallback for exactly that case.
        self._channel_client = SettingsChannelClient(channel_name(state_dir()), self)
        self._channel_client.message_received.connect(self._on_channel_message)
        self._live_creature_state: list = []
        self._loaded_settings: dict = {}
        # The teams this preset knows about, and what stands between them. Held
        # here rather than read back out of widgets, because the panel is rebuilt
        # whenever the slots change teams and a rebuild must not lose an edit.
        self._team_profiles: dict = {}
        self._team_stances: dict = {}
        self._teams_signature = None
        # The version belongs somewhere a user can read it off and quote in
        # a bug report; it existed in the source and was shown nowhere.
        self.setWindowTitle(f"Desktop Bug Companion {__version__}")
        self.resize(960, 860)
        self.setMinimumSize(840, 560)
        self._build_ui()
        self.refresh_discovery()
        self.refresh_presets()
        default_path = find_data_file("presets", "default.json", root=self.root)
        if default_path.exists():
            self.load_preset_path(default_path)
        else:
            self.add_slot()

        self.status_timer = QTimer(self)
        self.status_timer.timeout.connect(self.update_process_status)
        self.status_timer.start(1000)

    def _build_ui(self):
        self.setStyleSheet(
            """
            QWidget { font-size: 10pt; color: #1f2430; }
            QMainWindow, QMainWindow > QWidget { background: #eef1f8; }

            QGroupBox {
                font-weight: 600;
                border: 1px solid #cdd5e3;
                border-radius: 10px;
                margin-top: 13px;
                padding: 11px 10px 9px 10px;
                background: #ffffff;
            }
            QGroupBox::title {
                subcontrol-origin: margin;
                subcontrol-position: top left;
                left: 12px;
                padding: 2px 9px;
                border-radius: 7px;
                color: #ffffff;
            }
            /* Each section gets its own accent so it is easy to scan. */
            QGroupBox#presetGroup { border-color: #b8d0f0; }
            QGroupBox#presetGroup::title { background: #3b82c4; }
            QGroupBox#creaturesGroup { border-color: #cbbdf0; }
            QGroupBox#creaturesGroup::title { background: #6d4ed6; }
            QPushButton#removeSlotButton { padding: 0px; }
            QGroupBox#teamsGroup { border-color: #f0c2d8; }
            QGroupBox#teamsGroup::title { background: #b8477e; }
            QLabel#teamsNote { color: #5d6470; }
            QGroupBox#behaviorGroup { border-color: #a9e0d6; }
            QGroupBox#behaviorGroup::title { background: #199e8c; }
            QGroupBox#fliesGroup { border-color: #f2d49b; }
            QGroupBox#fliesGroup::title { background: #d9881a; }
            QGroupBox#launchGroup { border-color: #b3e0bd; }
            QGroupBox#launchGroup::title { background: #2f9e44; }

            QLabel#pageTitle {
                font-size: 17pt;
                font-weight: 800;
                color: #3a2e7a;
                padding: 2px 2px 2px 2px;
            }
            QLabel#hintLabel { color: #57606a; }
            QLabel#summaryLabel, QLabel#statusLabel {
                color: #24292f;
                background: #f6f8fa;
                border: 1px solid #d0d7de;
                border-radius: 6px;
                padding: 8px;
            }

            QPushButton {
                padding: 5px 11px;
                border-radius: 6px;
                background: #f1f4fa;
                border: 1px solid #c7d0de;
                color: #25304a;
            }
            QPushButton:hover { background: #e4ebf6; border-color: #a9b6cc; }
            QPushButton:pressed { background: #d6e0f0; }

            QPushButton#primaryButton {
                font-weight: 700;
                padding: 7px 16px;
                color: #ffffff;
                background: #2f9e44;
                border: 1px solid #2b8a3e;
            }
            QPushButton#primaryButton:hover { background: #2c903d; }
            QPushButton#primaryButton:pressed { background: #277834; }
            QPushButton#stopButton:hover {
                background: #fbe4e4; border-color: #e0a3a3; color: #9c2b2b;
            }

            QComboBox, QSpinBox, QDoubleSpinBox, QLineEdit {
                border: 1px solid #c7d0de;
                border-radius: 6px;
                padding: 3px 6px;
                background: #ffffff;
            }
            QComboBox:focus, QSpinBox:focus, QDoubleSpinBox:focus, QLineEdit:focus {
                border-color: #6d4ed6;
            }
            QComboBox QAbstractItemView {
                border: 1px solid #cbbdf0;
                background: #ffffff;
                selection-background-color: #6d4ed6;
                selection-color: #ffffff;
                outline: none;
            }

            QCheckBox { spacing: 6px; }
            QCheckBox#fliesToggle { font-weight: 700; color: #b56a12; }

            QTableWidget {
                border: 1px solid #cbbdf0;
                border-radius: 8px;
                background: #ffffff;
                gridline-color: #ececf4;
                selection-background-color: #ece7fb;
                selection-color: #1f2430;
            }
            QTableWidget::item { padding: 2px; }
            QTableWidget::item:alternate { background: #faf9fe; }
            QHeaderView::section {
                background: #efeafb;
                color: #4a3da0;
                font-weight: 600;
                border: none;
                border-right: 1px solid #e2dbf4;
                padding: 6px 6px;
            }

            QStatusBar { background: #e7ebf4; }
            """
        )

        root_widget = QWidget(self)
        self.setCentralWidget(root_widget)
        layout = QVBoxLayout(root_widget)
        layout.setContentsMargins(12, 10, 12, 8)
        layout.setSpacing(6)

        title = QLabel("Desktop Bug Companion")
        title.setObjectName("pageTitle")
        intro_tip = (
            "Build a creature preset by choosing a preset, adding creature slots, "
            "tuning overlay behavior, then launching. Settings are saved automatically when you launch."
        )
        title.setToolTip(intro_tip)
        root_widget.setToolTip(intro_tip)
        layout.addWidget(title)

        self.preset_group = QGroupBox("Preset")
        self.preset_group.setObjectName("presetGroup")
        preset_layout = QGridLayout(self.preset_group)
        preset_layout.setColumnStretch(1, 2)
        preset_layout.setColumnStretch(3, 2)
        self.preset_name = QLineEdit("Default")
        self.preset_name.setPlaceholderText("Preset name")
        self.preset_combo = NoScrollComboBox()
        self.load_btn = QPushButton("Load")
        self.save_btn = QPushButton("Save")
        self.refresh_btn = QPushButton("Refresh library")
        preset_layout.addWidget(QLabel("Name:"), 0, 0)
        preset_layout.addWidget(self.preset_name, 0, 1)
        preset_layout.addWidget(QLabel("Saved preset:"), 0, 2)
        preset_layout.addWidget(self.preset_combo, 0, 3)
        preset_layout.addWidget(self.load_btn, 0, 4)
        preset_layout.addWidget(self.save_btn, 0, 5)
        preset_layout.addWidget(self.refresh_btn, 1, 3, 1, 3)
        layout.addWidget(self.preset_group)

        self.creatures_group = QGroupBox("Creatures")
        self.creatures_group.setObjectName("creaturesGroup")
        self.creatures_group.setToolTip(
            "Each row is one creature group. Temperament is stable personality, Job is a separate profession, and Abilities are true capabilities."
        )
        creatures_layout = QVBoxLayout(self.creatures_group)
        creatures_layout.setContentsMargins(8, 8, 8, 8)
        creatures_layout.setSpacing(6)

        quick_row = QHBoxLayout()
        self.random_model_btn = QPushButton("Random models")
        self.random_personality_btn = QPushButton("Random personalities")
        self.random_count_btn = QPushButton("Random counts")
        self.random_all_btn = QPushButton("Surprise me")
        quick_row.addWidget(QLabel("Quick set:"))
        quick_row.addWidget(self.random_model_btn)
        quick_row.addWidget(self.random_personality_btn)
        quick_row.addWidget(self.random_count_btn)
        quick_row.addWidget(self.random_all_btn)
        quick_row.addStretch(1)
        creatures_layout.addLayout(quick_row)

        self.table = SlotTable()
        creatures_layout.addWidget(self.table, 1)

        slot_buttons = QHBoxLayout()
        self.add_slot_btn = QPushButton("Add creature slot")
        self.clear_slots_btn = QPushButton("Clear slots")
        slot_buttons.addWidget(self.add_slot_btn)
        slot_buttons.addWidget(self.clear_slots_btn)
        slot_buttons.addStretch(1)
        creatures_layout.addLayout(slot_buttons)
        layout.addWidget(self.creatures_group, 1)

        self.teams_group = QGroupBox("Teams")
        self.teams_group.setObjectName("teamsGroup")
        teams_outer = QVBoxLayout(self.teams_group)
        teams_outer.setContentsMargins(8, 8, 8, 8)
        teams_outer.setSpacing(6)
        # The honest description of what a team does, in the one place a person
        # picking teams will read it. It comes from the teams module so the
        # window, the tooltips and the README cannot drift apart.
        self.teams_note = QLabel(HOSTILITY_NOTE)
        self.teams_note.setWordWrap(True)
        self.teams_note.setObjectName("teamsNote")
        teams_outer.addWidget(self.teams_note)
        self.teams_panel = QWidget()
        self.teams_layout = QGridLayout(self.teams_panel)
        self.teams_layout.setContentsMargins(2, 2, 2, 0)
        self.teams_layout.setHorizontalSpacing(8)
        self.teams_layout.setVerticalSpacing(4)
        teams_outer.addWidget(self.teams_panel)
        layout.addWidget(self.teams_group)

        self.behavior_group = QGroupBox("Overlay behavior")
        self.behavior_group.setObjectName("behaviorGroup")
        behavior_layout = QGridLayout(self.behavior_group)
        self.size_combo = NoScrollComboBox()
        for label, scale in SIZE_OPTIONS:
            self.size_combo.addItem(label, scale)
        self.size_combo.setCurrentIndex(2)
        self.mood_combo = NoScrollComboBox()
        for label, mode in MOOD_OPTIONS:
            self.mood_combo.addItem(label, mode)
        self.movement_combo = NoScrollComboBox()
        for label, style in MOVEMENT_OPTIONS:
            self.movement_combo.addItem(label, style)
        self.movement_combo.setCurrentIndex(0)
        self.interferable_check = QCheckBox("Allow dragging spiders")
        self.interferable_check.setChecked(True)
        self.social_play_check = QCheckBox("Allow spiders to play together")
        self.social_play_check.setChecked(True)
        behavior_layout.addWidget(QLabel("Size:"), 0, 0)
        behavior_layout.addWidget(self.size_combo, 0, 1)
        behavior_layout.addWidget(QLabel("Mood:"), 0, 2)
        behavior_layout.addWidget(self.mood_combo, 0, 3)
        behavior_layout.addWidget(QLabel("Movement:"), 1, 0)
        behavior_layout.addWidget(self.movement_combo, 1, 1)
        movement_hint = QLabel("Lively lifts and probes. Skitter uses lively legs but moves in quick burst-burst-stop successions like the reference gif.")
        movement_hint.setObjectName("hintLabel")
        movement_hint.setWordWrap(True)
        behavior_layout.addWidget(movement_hint, 1, 2, 1, 2)
        behavior_layout.addWidget(self.interferable_check, 2, 1)
        behavior_layout.addWidget(self.social_play_check, 2, 3)
        behavior_layout.setColumnStretch(1, 1)
        behavior_layout.setColumnStretch(3, 1)
        layout.addWidget(self.behavior_group)

        self.flies_group = QGroupBox("Flies")
        self.flies_group.setObjectName("fliesGroup")
        flies_outer = QVBoxLayout(self.flies_group)
        flies_outer.setContentsMargins(8, 8, 8, 8)
        flies_outer.setSpacing(6)

        self.flies_enabled_check = QCheckBox("Spawn flies for the spiders to hunt")
        self.flies_enabled_check.setObjectName("fliesToggle")
        self.flies_enabled_check.setChecked(False)
        flies_outer.addWidget(self.flies_enabled_check)

        # Timing, fly count, and the nest only matter once flies are on, so they
        # live in a panel that stays hidden until the spawner is enabled. Turning
        # flies on reveals these extra options; turning it off tucks them away.
        self.flies_details = QWidget()
        flies_layout = QGridLayout(self.flies_details)
        flies_layout.setContentsMargins(2, 2, 2, 0)
        self.fly_min_spin = NoScrollDoubleSpinBox()
        self.fly_min_spin.setRange(0.3, 120.0)
        self.fly_min_spin.setDecimals(1)
        self.fly_min_spin.setSingleStep(0.5)
        self.fly_min_spin.setSuffix(" s")
        self.fly_min_spin.setValue(4.0)
        self.fly_max_spin = NoScrollDoubleSpinBox()
        self.fly_max_spin.setRange(0.4, 240.0)
        self.fly_max_spin.setDecimals(1)
        self.fly_max_spin.setSingleStep(0.5)
        self.fly_max_spin.setSuffix(" s")
        self.fly_max_spin.setValue(9.0)
        self.fly_count_spin = NoScrollSpinBox()
        self.fly_count_spin.setRange(0, 40)
        self.fly_count_spin.setValue(6)
        self.fly_spawner_check = QCheckBox("Flies emerge from a movable nest object")
        self.fly_spawner_check.setChecked(False)
        self.fly_spawner_check.setToolTip(
            "When on, flies crawl out of a nest you can drag around the screen. "
            "When off, they drift in from the screen edges."
        )
        flies_layout.addWidget(QLabel("Spawn every (min):"), 0, 0)
        flies_layout.addWidget(self.fly_min_spin, 0, 1)
        flies_layout.addWidget(QLabel("to (max):"), 0, 2)
        flies_layout.addWidget(self.fly_max_spin, 0, 3)
        flies_layout.addWidget(QLabel("Max flies at once:"), 1, 0)
        flies_layout.addWidget(self.fly_count_spin, 1, 1)
        flies_layout.addWidget(self.fly_spawner_check, 2, 0, 1, 4)
        flies_layout.setColumnStretch(1, 1)
        flies_layout.setColumnStretch(3, 1)
        flies_outer.addWidget(self.flies_details)
        layout.addWidget(self.flies_group)

        self.launch_group = QGroupBox("Launch")
        self.launch_group.setObjectName("launchGroup")
        launch_layout = QVBoxLayout(self.launch_group)
        launch_layout.setContentsMargins(8, 8, 8, 8)
        launch_layout.setSpacing(6)
        self.summary = QLabel("")
        self.summary.setVisible(False)

        launch_row = QHBoxLayout()
        self.open_folder_btn = QPushButton("Open project folder")
        self.stop_btn = QPushButton("Stop overlay")
        self.stop_btn.setObjectName("stopButton")
        self.launch_btn = QPushButton("Save and launch overlay")
        self.launch_btn.setObjectName("primaryButton")
        self.launch_btn.setDefault(True)
        launch_row.addWidget(self.open_folder_btn)
        launch_row.addStretch(1)
        launch_row.addWidget(self.stop_btn)
        launch_row.addWidget(self.launch_btn)
        launch_layout.addLayout(launch_row)

        layout.addWidget(self.launch_group)

        self.status = QLabel("")
        self.status.setWordWrap(False)
        self.statusBar().addPermanentWidget(self.status, 1)

        self.add_slot_btn.clicked.connect(self.add_slot)
        self.clear_slots_btn.clicked.connect(self.clear_slots)
        self.save_btn.clicked.connect(self.save_current_preset)
        self.load_btn.clicked.connect(self.load_selected_preset)
        self.launch_btn.clicked.connect(self.launch_engine)
        self.stop_btn.clicked.connect(self.stop_overlay)
        self.refresh_btn.clicked.connect(self.refresh_all)
        self.open_folder_btn.clicked.connect(self.open_project_folder)
        self.random_model_btn.clicked.connect(self.set_random_model_options)
        self.random_personality_btn.clicked.connect(self.set_random_personality_options)
        self.random_count_btn.clicked.connect(self.set_random_count_options)
        self.random_all_btn.clicked.connect(self.set_random_all_options)
        self.size_combo.currentIndexChanged.connect(self.update_summary)
        self.mood_combo.currentIndexChanged.connect(self.update_summary)
        self.movement_combo.currentIndexChanged.connect(self.update_summary)
        self.interferable_check.toggled.connect(self.update_summary)
        self.social_play_check.toggled.connect(self.update_summary)
        self.flies_enabled_check.toggled.connect(self.update_summary)
        self.flies_enabled_check.toggled.connect(self._update_flies_details_visibility)
        self.fly_min_spin.valueChanged.connect(self._on_fly_min_changed)
        self.fly_max_spin.valueChanged.connect(self._on_fly_max_changed)
        self.fly_count_spin.valueChanged.connect(self.update_summary)
        self.fly_spawner_check.toggled.connect(self.update_summary)

        self._set_tooltips()
        self._update_flies_details_visibility()
        self.update_summary()

    def _update_flies_details_visibility(self) -> None:
        # Only show the timing / fly-count / nest options when flies are enabled.
        self.flies_details.setVisible(self.flies_enabled_check.isChecked())

    def _on_fly_min_changed(self, value: float) -> None:
        # Keep the max at or above the min so the spawn range stays valid.
        if self.fly_max_spin.value() < value:
            self.fly_max_spin.blockSignals(True)
            self.fly_max_spin.setValue(value)
            self.fly_max_spin.blockSignals(False)
        self.update_summary()

    def _on_fly_max_changed(self, value: float) -> None:
        if value < self.fly_min_spin.value():
            self.fly_min_spin.blockSignals(True)
            self.fly_min_spin.setValue(value)
            self.fly_min_spin.blockSignals(False)
        self.update_summary()

    def _set_tooltips(self):
        self.preset_group.setToolTip("Name, load, save, or refresh presets from the presets folder.")
        self.behavior_group.setToolTip("Tune global overlay behavior for all launched creatures.")
        self.flies_group.setToolTip("Flies are prey that buzz around the screen; the spiders hunt, trap, and eat them.")
        self.flies_enabled_check.setToolTip("Turn the fly spawner on or off. With flies off the spiders ignore prey.")
        self.fly_min_spin.setToolTip("Shortest gap between fly spawns. Set equal to the max for a fixed timer.")
        self.fly_max_spin.setToolTip("Longest gap between fly spawns. Each spawn waits a random time in this range.")
        self.fly_count_spin.setToolTip("How many live flies may share the screen at once.")
        self.launch_group.setToolTip("Save the current preset and start or stop the overlay.")
        self.table.setToolTip(
            "Each row is one creature group. Temperament is stable personality, "
            "Job is a separate profession, Abilities are capabilities, and Team "
            "groups spiders before launch."
        )
        self.preset_name.setToolTip("This becomes the saved preset file name.")
        self.preset_combo.setToolTip("Choose an existing preset from the presets folder.")
        self.refresh_btn.setToolTip("Reload models, personalities, and presets from disk.")
        self.random_model_btn.setToolTip("Set every slot to choose a random creature model when launched.")
        self.random_personality_btn.setToolTip("Set every slot to choose a random personality when launched.")
        self.random_count_btn.setToolTip("Roll a new count from 1 to 10 into every slot now.")
        self.random_all_btn.setToolTip("Randomize model, personality, and count for every slot.")
        self.size_combo.setToolTip("Scale all creatures in the overlay.")
        self.mood_combo.setToolTip("Override moods, or leave Auto to use personality defaults.")
        self.movement_combo.setToolTip(
            "How the spiders walk. Classic is the original gait. Lively lifts the legs "
            "off the ground while stepping, walks the feet around through turns, and "
            "reaches out with the front legs and pedipalps to feel nearby objects. "
            "Skitter is based on Lively but adds quick burst-burst-stop movement."
        )
        self.interferable_check.setToolTip("When enabled, you can grab spiders; empty overlay space still remains click-through.")
        self.social_play_check.setToolTip("When enabled, multiple spiders may seek each other out and play.")
        self.save_btn.setToolTip("Save the current preset. While the overlay is running, this also applies your changes to it live.")
        self.launch_btn.setToolTip("Start the overlay. While it is already running, this applies your changes live instead of restarting.")
        self.stop_btn.setToolTip("Stop the overlay process launched from this window.")
        self.add_slot_btn.setToolTip("Add another creature group row.")
        self.clear_slots_btn.setToolTip("Remove every creature group row.")
        self.open_folder_btn.setToolTip("Open the project folder where presets, models, and personalities live.")

    def refresh_all(self):
        current_rows = self.collect_slots(silent=True)
        self.refresh_discovery()
        self.refresh_presets()
        self.table.setRowCount(0)
        if current_rows:
            for slot in current_rows:
                self.add_slot(
                    slot.get("model"),
                    slot.get("personality"),
                    slot.get("count", 1),
                    bool(slot.get("count_random", False)),
                    slot.get("skills"),
                    slot.get("abilities"),
                    slot.get("colors"),
                    slot.get("slot_id"),
                    slot.get("team_id", slot.get("team", "neutral")),
                    slot.get("job", "none"),
                )
        else:
            self.add_slot()
        self.update_summary()

    def refresh_discovery(self):
        self.models, model_warnings = discover_models(self.root)
        self.personalities, personality_warnings = discover_personalities(self.root)
        self.model_icon_cache = {}
        warnings = model_warnings + personality_warnings
        if not self.models:
            warnings.append("No valid models found in models/*/model.json")
        if not self.personalities:
            warnings.append("No valid personalities found in personalities/*.json")
        self.status.setText("\n".join(warnings) if warnings else "Ready. Build a preset, then press Save and launch overlay.")

    def refresh_presets(self):
        self.preset_combo.clear()
        for path in discover_presets(self.root):
            try:
                label = load_preset(path).get("name", path.stem)
            except Exception:
                label = path.stem
            self.preset_combo.addItem(str(label), str(path))

    def add_slot(self, model_id=None, personality_id=None, count=1, count_random=False, skills=None, abilities=None, colors=None, slot_id=None, team_id="neutral", job_id="none"):
        row = self.table.rowCount()
        self.table.insertRow(row)
        self.table.setRowHeight(row, max(64, MODEL_ICON_SIZE + 12))

        category_box = NoScrollComboBox()
        category_box.setMinimumWidth(120)
        category_box.addItem("Any kind", RANDOM_CATEGORY_ID)
        for plan in BODY_PLAN_IDS:
            category_box.addItem(BODY_PLAN_LABELS.get(plan, plan.title()), plan)
        category_box.setToolTip(
            "The spider's body plan: how it is built and how it walks. "
            "Every skin below is one of these four."
        )

        # The skin dropdown carries the model id, and so is the one the rest
        # of this window still reads. It is filtered by the category above.
        model_box = NoScrollComboBox()
        model_box.setProperty("slot_id", str(slot_id or f"slot-{uuid.uuid4().hex[:12]}"))
        model_box.setIconSize(QSize(MODEL_ICON_SIZE, MODEL_ICON_SIZE))
        model_box.setMinimumWidth(250)
        model_box.setToolTip(
            "Which artwork this slot uses. Skins with their own PNG art keep "
            "it; every skin can still be recoloured with the swatch beside it."
        )

        resolved_plan = self._plan_of_model(model_id) if model_id else None
        if resolved_plan is not None:
            index = category_box.findData(resolved_plan)
            if index >= 0:
                category_box.setCurrentIndex(index)
        self._fill_skin_box(model_box, category_box.currentData(), model_id)

        personality_box = NoScrollComboBox()
        personality_box.addItem("Random personality at launch", RANDOM_PERSONALITY_ID)
        personality_ids = selectable_personality_ids(self.personalities, personality_id)
        for personality_id_value in personality_ids:
            personality = self.personalities.get(personality_id_value)
            if personality is None:
                continue
            label = personality.get("display_name", personality["id"])
            if not personality.get("_canonical", False):
                label = f"Legacy: {label}"
            personality_box.addItem(label, personality["id"])
            trait_text = personality.get("temperament") or personality.get("traits")
            if isinstance(trait_text, dict):
                values = ", ".join(
                    f"{key} {float(trait_text.get(key, 5)):.0f}/10"
                    for key in ("energy", "curiosity", "boldness", "sociability", "patience", "caution")
                    if key in trait_text
                )
                personality_box.setItemData(
                    personality_box.count() - 1,
                    f"{personality.get('description', '')}\n{values}".strip(),
                    Qt.ToolTipRole,
                )
        if personality_id:
            idx = personality_box.findData(personality_id)
            if idx >= 0:
                personality_box.setCurrentIndex(idx)
        else:
            idx = personality_box.findData("balanced")
            if idx >= 0:
                personality_box.setCurrentIndex(idx)

        count_spin = NoScrollSpinBox()
        count_spin.setMinimum(1)
        count_spin.setMaximum(50)
        count_spin.setValue(max(1, min(50, int(count))))

        # The per-slot "Pick 1-10" checkbox was removed from the table: in a row
        # that already carries a model, a temperament, a count, abilities,
        # colours, a team and a job, it was the least-used control and the one
        # most often mistaken for part of "How many". The preset field still
        # round-trips, so a preset that has it keeps it and the overlay's own
        # right-click "Randomize count (1-10)" is unaffected; it is carried on
        # the spin box rather than shown as a column, and update_summary says so
        # when a loaded preset actually uses it.
        count_spin.setProperty("count_random", bool(count_random))

        skills_btn = QPushButton()
        # A slot with no explicit skills follows its personality's default
        # abilities (common set plus that personality's specialty), so spiders
        # are not all handed every ability.  Only an explicit skills list from a
        # saved preset, or a manual edit, counts as "custom" and sticks when the
        # personality changes.
        if abilities is not None:
            skills_btn.setProperty(
                "skill_ids",
                skills_with_selected_abilities(
                    self.personalities.get(personality_box.currentData()),
                    abilities,
                ),
            )
            skills_btn.setProperty("ability_ids", normalize_ability_ids(abilities))
            skills_btn.setProperty("skills_custom", True)
        elif skills is None:
            effective_pid = personality_box.currentData()
            skills_btn.setProperty("skill_ids", self._default_skills_for(effective_pid))
            skills_btn.setProperty("skills_custom", False)
        else:
            skills_btn.setProperty("skill_ids", normalize_skill_ids(skills))
            # Legacy presets stored behaviours and abilities together.  Keep
            # loading them compatible, but migrate their editable portion to
            # the dedicated abilities field when the preset is saved.
            skills_btn.setProperty("ability_ids", normalize_ability_ids(skills))
            skills_btn.setProperty("skills_custom", True)
        self._refresh_skills_button(skills_btn)
        skills_btn.clicked.connect(lambda _checked=False, button=skills_btn: self.edit_skills_for_button(button))

        colors_btn = QPushButton()
        colors_btn.setFixedSize(QSize(46, 26))
        colors_btn.setProperty("color_overrides", _normalize_color_overrides(colors))
        # The swatch shows *this slot's* colours, which means the model's own
        # palette when nothing has been overridden, so the model box is passed in.
        self._refresh_colors_button(colors_btn, model_box)
        colors_btn.clicked.connect(
            lambda _checked=False, button=colors_btn, mb=model_box: self.edit_colors_for_button(button, mb)
        )

        team_box = NoScrollComboBox()
        team_box.setMinimumWidth(110)
        self._populate_team_box(team_box, team_id)
        team_box.setToolTip(
            "Which group this slot belongs to. Name your teams and set what "
            "stands between them in the Teams panel below."
            "\n\n" + HOSTILITY_NOTE
        )

        job_box = NoScrollComboBox()
        job_box.setMinimumWidth(104)
        for label, value in JOB_OPTIONS:
            job_box.addItem(label, value)
        job_value = normalize_job_id(job_id)
        job_index = job_box.findData(job_value)
        job_box.setCurrentIndex(job_index if job_index >= 0 else 0)
        if not bool(skills_btn.property("skills_custom")):
            skills_btn.setProperty(
                "skill_ids",
                self._default_skills_for(personality_box.currentData(), job_box.currentData()),
            )
            self._refresh_skills_button(skills_btn)
        job_box.setToolTip(
            "A job is separate from temperament. Builders create a shared base; "
            "Guards patrol and protect it from declared foes."
        )

        remove_btn = QPushButton()
        remove_btn.setFixedSize(QSize(26, 26))
        remove_btn.setIcon(self._remove_icon())
        remove_btn.setIconSize(QSize(12, 12))
        # An icon with no words still has to be reachable by someone who cannot
        # see it, and understandable by someone who can but does not recognise
        # it, so both the accessible name and the tooltip stay.
        remove_btn.setAccessibleName("Remove this creature slot")
        remove_btn.setToolTip("Remove this creature slot")
        remove_btn.setObjectName("removeSlotButton")
        remove_btn.clicked.connect(lambda: self.remove_slot_by_button(remove_btn))

        category_box.currentIndexChanged.connect(
            lambda _i=0, cb=category_box, mb=model_box, bt=colors_btn:
            self._on_category_changed(cb, mb, bt))

        self.table.setCellWidget(row, COL_CATEGORY, category_box)
        self.table.setCellWidget(row, COL_SKIN, model_box)
        self.table.setCellWidget(row, COL_COLORS, colors_btn)
        self.table.setCellWidget(row, COL_TEMPERAMENT, personality_box)
        self.table.setCellWidget(row, COL_COUNT, count_spin)
        self.table.setCellWidget(row, COL_ABILITIES, skills_btn)
        self.table.setCellWidget(row, COL_TEAM, team_box)
        self.table.setCellWidget(row, COL_JOB, job_box)
        self.table.setCellWidget(row, COL_REMOVE, remove_btn)
        for col in range(SLOT_COLUMNS):
            self.table.setItem(row, col, QTableWidgetItem(""))

        model_box.currentIndexChanged.connect(self.update_summary)
        model_box.currentIndexChanged.connect(
            lambda _i=0, button=colors_btn, mb=model_box: self._refresh_colors_button(button, mb))
        personality_box.currentIndexChanged.connect(
            lambda _i=0, pb=personality_box, jb=job_box, sb=skills_btn: self._on_personality_changed(pb, jb, sb))
        count_spin.valueChanged.connect(self.update_summary)
        team_box.currentIndexChanged.connect(
            lambda _i=0, box=team_box: self._on_team_box_changed(box))
        job_box.currentIndexChanged.connect(self.update_summary)
        job_box.currentIndexChanged.connect(
            lambda _i=0, pb=personality_box, jb=job_box, sb=skills_btn: self._on_job_changed(pb, jb, sb)
        )
        self.update_summary()

    def _remove_icon(self) -> QIcon:
        """A small cross, drawn rather than shipped as a file."""
        cached = getattr(self, "_remove_icon_cache", None)
        if cached is not None:
            return cached
        pixmap = QPixmap(24, 24)
        pixmap.fill(Qt.transparent)
        painter = QPainter(pixmap)
        painter.setRenderHint(QPainter.Antialiasing, True)
        painter.setPen(QPen(QColor(196, 72, 72), 3.0, Qt.SolidLine, Qt.RoundCap))
        painter.drawLine(7, 7, 17, 17)
        painter.drawLine(17, 7, 7, 17)
        painter.end()
        self._remove_icon_cache = QIcon(pixmap)
        return self._remove_icon_cache

    def _random_model_icon(self) -> QIcon:
        """Small dice-like icon for the random model option."""
        cache_key = RANDOM_MODEL_ID
        if cache_key in self.model_icon_cache:
            return self.model_icon_cache[cache_key]
        pixmap = QPixmap(MODEL_ICON_CANVAS, MODEL_ICON_CANVAS)
        pixmap.fill(Qt.transparent)
        painter = QPainter(pixmap)
        painter.setRenderHint(QPainter.Antialiasing, True)
        painter.setPen(Qt.NoPen)
        painter.setBrush(QColor(52, 47, 60, 235))
        painter.drawRoundedRect(24, 24, 48, 48, 10, 10)
        painter.setBrush(QColor(245, 238, 255, 245))
        for x, y in [(36, 36), (60, 36), (48, 48), (36, 60), (60, 60)]:
            painter.drawEllipse(x - 4, y - 4, 8, 8)
        painter.end()
        icon = QIcon(pixmap)
        self.model_icon_cache[cache_key] = icon
        return icon

    def _model_icon(self, model: dict) -> QIcon:
        """Render a live preview thumbnail for a model and cache it for combo boxes.

        The preview uses the same Creature renderer as the overlay, so procedural
        models and sprite-rig models both get recognizable thumbnails without
        needing hand-made icon files in every model folder.
        """
        cache_key = model.get("id") or model.get("_folder") or repr(model)
        if cache_key in self.model_icon_cache:
            return self.model_icon_cache[cache_key]

        pixmap = QPixmap(MODEL_ICON_CANVAS, MODEL_ICON_CANVAS)
        pixmap.fill(Qt.transparent)
        painter = QPainter(pixmap)
        painter.setRenderHint(QPainter.Antialiasing, True)
        painter.setRenderHint(QPainter.SmoothPixmapTransform, True)
        try:
            # DC-43 moved this module down into app/, which turned `.creature`
            # into the non-existent desktop_bug.app.creature. The ImportError
            # was swallowed by the except below, so every model silently drew
            # its legless fallback icon instead of a spider.
            from ..creature import Creature

            preview_personality = {
                "id": "preview",
                "display_name": "Preview",
                "speed_multiplier": 1.0,
                "reaction_radius": 120,
                "boldness": 0.5,
                "wander_frequency": 0.0,
                "idle_time": [1.0, 2.0],
                "move_time": [1.0, 2.0],
                "mood": model.get("default_personality", "auto"),
            }
            creature = Creature(dict(model), preview_personality, MODEL_ICON_CANVAS, MODEL_ICON_CANVAS, size_scale=1.0)
            creature.x = MODEL_ICON_CANVAS * 0.47
            creature.y = MODEL_ICON_CANVAS * 0.50
            creature.heading = 0.0
            creature.target_heading = 0.0
            creature.current_speed = 0.0
            creature.speed = 0.0
            creature.body_bob = 0.0
            creature.abdomen_pulse = 0.0
            creature.ceph_pulse = 0.0
            # Keep every model readable in the same icon space; this is a visual
            # swatch, not a scale comparison between species.
            creature.size = max(16.0, min(22.0, float(model.get("base_size", 25)) * 0.82))
            creature._initialize_legs()
            creature.render(painter)
        except Exception:
            # The fallback exists for a model whose own data will not render.
            # It is not meant to absorb a broken import, which is what it did
            # silently for two days after DC-43: the icons looked plausible
            # enough that nobody read them as an error. Log it.
            log.exception(
                "Model preview failed for %s; drawing the fallback icon",
                model.get("id") or model.get("_folder") or "<unknown>",
            )
            self._draw_fallback_model_icon(painter, model)
        painter.end()

        icon = QIcon(pixmap)
        self.model_icon_cache[cache_key] = icon
        return icon

    def _draw_fallback_model_icon(self, painter: QPainter, model: dict) -> None:
        """Simple color-based spider thumbnail used only if the full preview fails."""
        colors = model.get("colors", {}) if isinstance(model, dict) else {}

        def qcolor(name: str, default, alpha=255):
            raw = colors.get(name, default)
            return QColor(int(raw[0]), int(raw[1]), int(raw[2]), alpha)

        painter.setPen(Qt.NoPen)
        painter.setBrush(qcolor("legs", [26, 22, 20], 135))
        painter.drawEllipse(18, 39, 60, 25)
        painter.setBrush(qcolor("body", [48, 38, 33], 255))
        painter.drawEllipse(26, 34, 31, 28)
        painter.drawEllipse(51, 37, 24, 22)
        painter.setBrush(qcolor("highlight", [95, 82, 70], 175))
        painter.drawEllipse(32, 39, 15, 10)
        painter.setBrush(qcolor("eyes", [185, 55, 48], 245))
        painter.drawEllipse(64, 43, 5, 5)
        painter.drawEllipse(64, 51, 5, 5)

    def _default_skills_for(self, personality_id, job_id="none"):
        """Default abilities for a personality combo value.

        A real personality resolves to its common-plus-specialty default; a
        Random/unknown selection falls back to the common set (the actual
        personality's specialty is resolved when the overlay launches).
        """
        personality = self.personalities.get(personality_id) if personality_id else None
        if personality is None:
            return normalize_skill_ids(list(COMMON_SKILL_IDS) + list(job_ability_ids(job_id)))
        return skills_with_default_abilities(personality, job_ability_ids(job_id))

    def _on_personality_changed(self, personality_box, job_box, skills_btn) -> None:
        # Follow the new personality's default abilities unless the user has
        # deliberately customised this slot's skills.
        if not bool(skills_btn.property("skills_custom")):
            skills_btn.setProperty("skill_ids", self._default_skills_for(personality_box.currentData(), job_box.currentData()))
            self._refresh_skills_button(skills_btn)
        self.update_summary()

    def _on_job_changed(self, personality_box, job_box, skills_btn) -> None:
        if not bool(skills_btn.property("skills_custom")):
            skills_btn.setProperty(
                "skill_ids",
                self._default_skills_for(personality_box.currentData(), job_box.currentData()),
            )
            self._refresh_skills_button(skills_btn)
        self.update_summary()

    def _refresh_skills_button(self, button: QPushButton) -> None:
        ids = normalize_skill_ids(button.property("skill_ids"))
        button.setProperty("skill_ids", ids)
        button.setText(compact_ability_summary(ids))
        button.setToolTip("Choose which abilities this creature slot can use at launch.")

    # The order colours are shown in, widest part of the creature first, so the
    # swatch reads like the creature rather than like an arbitrary set.
    SWATCH_KEY_ORDER = ("body", "abdomen", "legs", "head", "eyes", "accent")

    def _palette_for_button(self, button: QPushButton, model_box: QComboBox | None) -> list:
        """The colours this slot will actually produce, overrides first."""
        overrides = _normalize_color_overrides(button.property("color_overrides"))
        palette = dict(self._model_color_defaults(model_box) if model_box is not None else {})
        palette.update(overrides)
        ordered = [palette[key] for key in self.SWATCH_KEY_ORDER if key in palette]
        rest = [palette[key] for key in sorted(palette) if key not in self.SWATCH_KEY_ORDER]
        return (ordered + rest)[:4]

    def _swatch_icon(self, colors: list, custom: bool) -> QIcon:
        """A filled swatch of the slot's colours, or a hint when there are none."""
        size = 34
        pixmap = QPixmap(size, size)
        pixmap.fill(Qt.transparent)
        painter = QPainter(pixmap)
        painter.setRenderHint(QPainter.Antialiasing, True)
        if not colors:
            # Nothing to show: an outline, so the control still looks like a
            # control rather than an empty gap in the row.
            painter.setBrush(QColor(255, 255, 255, 20))
            painter.setPen(QPen(QColor(140, 146, 158), 1.4, Qt.DashLine))
            painter.drawRoundedRect(2, 2, size - 4, size - 4, 6, 6)
            painter.end()
            return QIcon(pixmap)
        painter.setPen(Qt.NoPen)
        band = (size - 4) / len(colors)
        for index, color in enumerate(colors):
            painter.setBrush(QColor(*color[:3]))
            top = 2 + band * index
            painter.drawRect(2, int(round(top)), size - 4, int(round(band)) + 1)
        painter.setBrush(Qt.NoBrush)
        # A custom palette is outlined brightly so a row that was changed by hand
        # is obvious at a glance; the count moved out of the label into this.
        painter.setPen(QPen(QColor(255, 255, 255, 230) if custom else QColor(0, 0, 0, 70),
                            2.0 if custom else 1.0))
        painter.drawRoundedRect(2, 2, size - 4, size - 4, 6, 6)
        painter.end()
        return QIcon(pixmap)

    def _refresh_colors_button(self, button: QPushButton, model_box: QComboBox | None = None) -> None:
        overrides = _normalize_color_overrides(button.property("color_overrides"))
        button.setProperty("color_overrides", overrides)
        colors = self._palette_for_button(button, model_box)
        button.setText("")
        button.setStyleSheet("")
        button.setIcon(self._swatch_icon(colors, bool(overrides)))
        button.setIconSize(QSize(34, 34))
        button.setProperty("swatch_colors", colors)
        if overrides:
            description = "Custom palette: " + ", ".join(sorted(overrides)) + ". Click to edit."
        elif colors:
            description = "The model's own palette. Click to choose custom colors for this slot."
        else:
            description = "Use the model's default palette. Click to choose custom colors for this slot."
        button.setToolTip(description)
        button.setAccessibleName(
            f"Colors for this slot: {len(overrides)} custom" if overrides
            else "Colors for this slot: model default")
        button.setAccessibleDescription(description)

    def _plan_of_model(self, model_id: str):
        """The body plan a model is built on, or None if it is not a model."""
        model = self.models.get(str(model_id or ""))
        if not isinstance(model, dict):
            return None
        plan = str(model.get("body_plan", "") or "").strip().lower()
        return plan if plan in BODY_PLAN_IDS else None

    def _fill_skin_box(self, model_box, category, keep_model_id=None) -> None:
        """Repopulate a skin dropdown for one category, keeping the selection.

        Rebuilt rather than filtered in place because the categories have
        very different sizes -- 35 skins on `bug`, one on `tarantula` -- and
        a hidden-item approach leaves the dropdown's height lying about how
        much is in it.
        """
        wanted = str(category or RANDOM_CATEGORY_ID)
        previous = keep_model_id if keep_model_id is not None else model_box.currentData()
        blocked = model_box.blockSignals(True)
        model_box.clear()
        model_box.addItem(self._random_model_icon(), "Random skin at launch", RANDOM_MODEL_ID)

        def newest_first(model):
            try:
                added = Path(model.get("_path", "")).stat().st_ctime
            except (OSError, ValueError):
                added = 0.0
            return (-added, model.get("display_name", model.get("id", "")).casefold())

        for model in sorted(self.models.values(), key=newest_first):
            if wanted != RANDOM_CATEGORY_ID and model.get("body_plan") != wanted:
                continue
            model_box.addItem(self._model_icon(model),
                              model.get("display_name", model["id"]), model["id"])
        model_box.blockSignals(blocked)

        for candidate in (previous, "spider"):
            if candidate is None:
                continue
            index = model_box.findData(candidate)
            if index >= 0:
                model_box.setCurrentIndex(index)
                return
        # The old skin is not in this category: fall to its first real entry
        # rather than to "Random skin", which would quietly change the slot
        # into a random one.
        model_box.setCurrentIndex(1 if model_box.count() > 1 else 0)

    def _on_category_changed(self, category_box, model_box, colors_btn) -> None:
        self._fill_skin_box(model_box, category_box.currentData())
        self._refresh_colors_button(colors_btn, model_box)
        self.update_summary()

    def _model_color_defaults(self, model_box: QComboBox) -> dict:
        model_id = model_box.currentData() if model_box is not None else None
        if model_id == RANDOM_MODEL_ID:
            models = self.models.values()
        else:
            model = self.models.get(model_id) if model_id else None
            models = [model] if model else []
        defaults = {}
        for model in models:
            for key, value in (model.get("colors", {}) or {}).items():
                normalized = _normalize_color_overrides({key: value})
                if key not in defaults and key in normalized:
                    defaults[key] = normalized[key]
        return defaults

    def edit_colors_for_button(self, button: QPushButton, model_box: QComboBox) -> None:
        """Edit a slot palette without changing the model's shared defaults."""
        defaults = self._model_color_defaults(model_box)
        current = _normalize_color_overrides(button.property("color_overrides"))
        keys = list(COLOR_KEYS)
        known = {key for key, _label in keys}
        for key in defaults:
            if key not in known:
                keys.append((key, key.replace("_", " ").title()))

        dialog = QDialog(self)
        dialog.setWindowTitle("Customize spider colors")
        layout = QVBoxLayout(dialog)
        layout.addWidget(QLabel("Choose colors for this creature slot. Unchanged fields use the selected model's defaults."))
        swatches = {}

        def display_color(key: str):
            return current.get(key) or defaults.get(key) or [80, 70, 70]

        def refresh_swatch(key: str, swatch: QPushButton):
            rgb = display_color(key)
            luminance = rgb[0] * 0.299 + rgb[1] * 0.587 + rgb[2] * 0.114
            text_color = "#17202a" if luminance > 155 else "#ffffff"
            swatch.setText("Custom" if key in current else "Model default")
            swatch.setStyleSheet(
                f"QPushButton {{ background: rgb({rgb[0]}, {rgb[1]}, {rgb[2]}); color: {text_color}; }}"
            )

        for key, label in keys:
            row = QHBoxLayout()
            row.addWidget(QLabel(f"{label}:"))
            swatch = QPushButton()
            swatch.setMinimumWidth(125)
            swatches[key] = swatch
            refresh_swatch(key, swatch)

            def choose_color(_checked=False, color_key=key, color_button=swatch):
                rgb = display_color(color_key)
                chosen = QColorDialog.getColor(QColor(*rgb), self, f"Choose {color_key} color")
                if chosen.isValid():
                    current[color_key] = [chosen.red(), chosen.green(), chosen.blue()]
                    refresh_swatch(color_key, color_button)

            swatch.clicked.connect(choose_color)
            row.addWidget(swatch)
            layout.addLayout(row)

        buttons = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        reset_btn = buttons.addButton("Reset to model defaults", QDialogButtonBox.ResetRole)
        reset_btn.clicked.connect(lambda: [current.pop(key, None) for key, _label in keys])
        reset_btn.clicked.connect(lambda: [refresh_swatch(key, swatches[key]) for key, _label in keys])
        buttons.accepted.connect(dialog.accept)
        buttons.rejected.connect(dialog.reject)
        layout.addWidget(buttons)

        if dialog.exec_() == QDialog.Accepted:
            button.setProperty("color_overrides", _normalize_color_overrides(current))
            self._refresh_colors_button(button, model_box)
            self.update_summary()

    def edit_skills_for_button(self, button: QPushButton) -> None:
        current = set(normalize_skill_ids(button.property("skill_ids")))
        dialog = QDialog(self)
        dialog.setWindowTitle("Choose spider abilities")
        layout = QVBoxLayout(dialog)
        dialog.setToolTip("Select the abilities that spiders in this slot may use. Leaving this unchanged keeps the personality's default abilities.")

        checks = []
        heading = QLabel("Abilities")
        heading.setStyleSheet("font-weight: 600; margin-top: 6px;")
        layout.addWidget(heading)
        for skill in SKILLS:
            if skill.category != "Ability":
                continue
            cb = QCheckBox(skill.display_name)
            cb.setChecked(skill.id in current)
            cb.setToolTip(skill.description)
            checks.append((skill.id, cb))
            layout.addWidget(cb)

        buttons = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        all_btn = buttons.addButton("All", QDialogButtonBox.ActionRole)
        none_btn = buttons.addButton("None", QDialogButtonBox.ActionRole)
        all_btn.clicked.connect(lambda: [cb.setChecked(True) for _skill_id, cb in checks])
        none_btn.clicked.connect(lambda: [cb.setChecked(False) for _skill_id, cb in checks])
        buttons.accepted.connect(dialog.accept)
        buttons.rejected.connect(dialog.reject)
        layout.addWidget(buttons)

        if dialog.exec_() == QDialog.Accepted:
            selected = [skill_id for skill_id, cb in checks if cb.isChecked()]
            button.setProperty("ability_ids", selected)
            # Keep the legacy in-memory property complete for callers that
            # still inspect it, while behaviours remain personality-controlled.
            behaviour_ids = [skill_id for skill_id in current if skill_id not in {
                skill.id for skill in SKILLS if skill.category == "Ability"
            }]
            button.setProperty("skill_ids", behaviour_ids + selected)
            button.setProperty("skills_custom", True)
            self._refresh_skills_button(button)
            self.update_summary()

    def clear_slots(self):
        if self.table.rowCount() == 0:
            self.status.setText("There are no creature slots to clear.")
            return
        reply = QMessageBox.question(
            self,
            "Clear creature slots?",
            "Remove every creature slot from this preset?",
            QMessageBox.Yes | QMessageBox.No,
        )
        if reply == QMessageBox.Yes:
            self.table.setRowCount(0)
            self.status.setText("All creature slots cleared. Add a slot before saving or launching.")
            self.update_summary()

    def remove_slot_by_button(self, button):
        for row in range(self.table.rowCount()):
            if self.table.cellWidget(row, COL_REMOVE) is button:
                self.table.removeRow(row)
                break
        self.update_summary()

    def collect_slots(self, silent=False):
        slots = []
        for row in range(self.table.rowCount()):
            model_box = self.table.cellWidget(row, COL_MODEL)
            personality_box = self.table.cellWidget(row, COL_TEMPERAMENT)
            count_spin = self.table.cellWidget(row, COL_COUNT)
            skills_btn = self.table.cellWidget(row, COL_ABILITIES)
            colors_btn = self.table.cellWidget(row, COL_COLORS)
            team_box = self.table.cellWidget(row, COL_TEAM)
            job_box = self.table.cellWidget(row, COL_JOB)
            if not model_box or not personality_box or model_box.currentData() is None or personality_box.currentData() is None:
                continue
            # Carried on the spin box since the column went away, so a preset
            # that uses count_random still round-trips through a save.
            count_random = bool(count_spin.property("count_random")) if count_spin else False
            slot = {
                "model": model_box.currentData(),
                "personality": personality_box.currentData(),
                "count": int(count_spin.value()),
                "count_random": count_random,
            }
            slot["slot_id"] = str(model_box.property("slot_id") or f"slot-{uuid.uuid4().hex[:12]}")
            slot["team"] = str(team_box.currentData() or "neutral") if team_box is not None else "neutral"
            slot["job"] = normalize_job_id(job_box.currentData()) if job_box is not None else "none"
            # Only write an explicit skills list when the user customised it.
            # Otherwise the slot stays personality-driven: the spider uses its
            # personality's default abilities, resolved when the overlay loads.
            if skills_btn is not None and bool(skills_btn.property("skills_custom")):
                slot["abilities"] = normalize_ability_ids(
                    skills_btn.property("ability_ids")
                    if skills_btn.property("ability_ids") is not None
                    else skills_btn.property("skill_ids")
                )
            if colors_btn is not None:
                colors = _normalize_color_overrides(colors_btn.property("color_overrides"))
                if colors:
                    slot["colors"] = colors
            slots.append(slot)
        if not slots and not silent:
            QMessageBox.warning(self, "No creature slots", "Add at least one creature slot before saving or launching.")
        return slots

    def current_settings_data(self):
        # Start from whatever the preset already had. Settings without a
        # widget here -- team relations, the mouse-capture switch -- would
        # otherwise be lost every time the user pressed Save.
        settings = dict(getattr(self, "_loaded_settings", {}))
        settings.update({
            "size_scale": float(self.size_combo.currentData() or 1.0),
            "interferable": bool(self.interferable_check.isChecked()),
            "mood_mode": str(self.mood_combo.currentData() or "auto"),
            "social_play": bool(self.social_play_check.isChecked()),
            "gait_style": str(self.movement_combo.currentData() or "classic"),
            "teams": teams_payload(self._ensure_team_profiles()),
            # Only what was actually declared. Recomputing this from the pairs on
            # screen would drop a stance about a team no slot currently uses, and
            # would write out the implicit "rivals is hostile" rule as though a
            # person had chosen it.
            "team_relations": minimal_stances(self._team_stances),
            "flies": {
                "enabled": bool(self.flies_enabled_check.isChecked()),
                "min_interval": round(float(self.fly_min_spin.value()), 2),
                "max_interval": round(float(self.fly_max_spin.value()), 2),
                "max_flies": int(self.fly_count_spin.value()),
                "spawner": bool(self.fly_spawner_check.isChecked()),
            },
        })
        return settings

    def current_preset_data(self):
        data = {
            "name": self.preset_name.text().strip() or "Default",
            "slots": self.collect_slots(),
            "settings": self.current_settings_data(),
        }
        validate_preset(data)
        return data

    def _combo_set_data(self, combo: QComboBox, value) -> None:
        idx = combo.findData(value)
        if idx >= 0:
            combo.setCurrentIndex(idx)

    def apply_settings_to_ui(self, settings: dict | None) -> None:
        settings = settings if isinstance(settings, dict) else {}
        # Remember everything the preset carried, including settings this
        # window has no widget for, so saving does not silently drop them.
        self._loaded_settings = dict(settings)
        # Teams first: the slot pickers are filled from this, and a preset that
        # renamed its teams has to show those names rather than the ids.
        self._team_profiles = normalize_teams(settings.get("teams"))
        self._team_stances = normalize_team_stances(settings.get("team_relations"))
        self._teams_signature = None
        size_scale = float(settings.get("size_scale", 1.0))
        closest_index = 0
        closest_distance = float("inf")
        for idx in range(self.size_combo.count()):
            distance = abs(float(self.size_combo.itemData(idx)) - size_scale)
            if distance < closest_distance:
                closest_distance = distance
                closest_index = idx
        self.size_combo.setCurrentIndex(closest_index)
        self.interferable_check.setChecked(bool(settings.get("interferable", True)))
        self.social_play_check.setChecked(bool(settings.get("social_play", True)))
        mood_mode = str(settings.get("mood_mode", "auto") or "auto").lower()
        mood_idx = self.mood_combo.findData(mood_mode)
        self.mood_combo.setCurrentIndex(mood_idx if mood_idx >= 0 else 0)

        gait_style = str(settings.get("gait_style", "classic") or "classic").lower()
        gait_idx = self.movement_combo.findData(gait_style)
        self.movement_combo.setCurrentIndex(gait_idx if gait_idx >= 0 else 0)

        flies = settings.get("flies")
        flies = flies if isinstance(flies, dict) else {}
        self.flies_enabled_check.setChecked(bool(flies.get("enabled", False)))
        try:
            mn = float(flies.get("min_interval", 4.0))
            mx = float(flies.get("max_interval", 9.0))
        except (TypeError, ValueError):
            mn, mx = 4.0, 9.0
        if mx < mn:
            mx = mn
        self.fly_min_spin.blockSignals(True)
        self.fly_max_spin.blockSignals(True)
        self.fly_min_spin.setValue(mn)
        self.fly_max_spin.setValue(mx)
        self.fly_min_spin.blockSignals(False)
        self.fly_max_spin.blockSignals(False)
        try:
            self.fly_count_spin.setValue(int(flies.get("max_flies", 6)))
        except (TypeError, ValueError):
            self.fly_count_spin.setValue(6)
        self.fly_spawner_check.setChecked(bool(flies.get("spawner", False)))
        self._update_flies_details_visibility()
        self.update_summary()

    def _iter_row_widgets(self):
        for row in range(self.table.rowCount()):
            yield (
                self.table.cellWidget(row, COL_SKIN),
                self.table.cellWidget(row, COL_TEMPERAMENT),
                self.table.cellWidget(row, COL_COUNT),
                self.table.cellWidget(row, COL_ABILITIES),
            )

    # ------------------------------------------------------------------
    # Teams: names, colours, and what stands between them
    # ------------------------------------------------------------------
    NEW_TEAM_SENTINEL = "__new_team__"

    def _team_ids_in_use(self) -> list:
        """Every non-neutral team id a slot currently points at, in row order."""
        ids = []
        for row in range(self.table.rowCount()):
            box = self.table.cellWidget(row, COL_TEAM)
            if box is None:
                continue
            team_id = normalize_team_id(box.currentData() or "neutral")
            if team_id != "neutral" and team_id not in ids:
                ids.append(team_id)
        return ids

    def _ensure_team_profiles(self) -> dict:
        """Give every team in use an identity, keeping the ones already named."""
        self._team_profiles = normalize_teams(
            teams_payload(self._team_profiles), self._team_ids_in_use())
        return self._team_profiles

    def _populate_team_box(self, box, selected) -> None:
        """Fill one slot's team picker from the teams this preset knows about."""
        selected = normalize_team_id(selected or "neutral")
        box.blockSignals(True)
        box.clear()
        box.addItem("Neutral / solo", "neutral")
        for team_id in sorted(self._team_profiles):
            profile = self._team_profiles[team_id]
            box.addItem(self._team_icon(profile.color), profile.name, team_id)
        if selected != "neutral" and box.findData(selected) < 0:
            # A preset can name a team the block never described; it still has to
            # be selectable, or loading the preset would silently move the slot.
            profile = TeamProfile(selected, selected.replace("_", " ").title(),
                                  default_color(selected))
            self._team_profiles[selected] = profile
            box.addItem(self._team_icon(profile.color), profile.name, selected)
        box.insertSeparator(box.count())
        box.addItem("New team...", self.NEW_TEAM_SENTINEL)
        index = box.findData(selected)
        box.setCurrentIndex(index if index >= 0 else 0)
        box.blockSignals(False)

    @staticmethod
    def _team_icon(color) -> QIcon:
        """A filled swatch, so a team is recognisable in the list at a glance."""
        pixmap = QPixmap(14, 14)
        pixmap.fill(Qt.transparent)
        painter = QPainter(pixmap)
        painter.setRenderHint(QPainter.Antialiasing, True)
        painter.setPen(QColor(0, 0, 0, 90))
        painter.setBrush(QColor(*color))
        painter.drawEllipse(1, 1, 12, 12)
        painter.end()
        return QIcon(pixmap)

    def _repopulate_team_boxes(self) -> None:
        for row in range(self.table.rowCount()):
            box = self.table.cellWidget(row, COL_TEAM)
            if box is not None:
                self._populate_team_box(box, box.currentData())

    def _on_team_box_changed(self, box) -> None:
        if str(box.currentData() or "") == self.NEW_TEAM_SENTINEL:
            team_id = self._prompt_for_new_team()
            # Falls back to the previous choice when the prompt is cancelled,
            # so a stray click cannot leave a slot pointing at the menu entry.
            self._populate_team_box(box, team_id or box.property("last_team") or "neutral")
        box.setProperty("last_team", box.currentData())
        self._ensure_team_profiles()
        self._repopulate_team_boxes()
        self.update_summary()

    def _prompt_for_new_team(self):
        """Ask for a name and turn it into a team, or return None if cancelled."""
        name, accepted = QInputDialog.getText(
            self, "New team", "Name this team (for example: Porch guard)")
        if not accepted or not str(name).strip():
            return None
        team_id = self._unique_team_id(str(name))
        self._team_profiles[team_id] = TeamProfile(
            team_id, normalize_team_name(name, team_id), default_color(team_id))
        return team_id

    def _unique_team_id(self, name: str) -> str:
        """Turn a name into a stable id that no other team is already using.

        Case-folded like every other id in this project, because on Windows a
        team called `Rivals` and one called `rivals` are the same folder, the
        same saved key, and have already been the same bug four times.
        """
        slug = "".join(char if char.isalnum() else "_" for char in str(name).strip().lower())
        slug = "_".join(part for part in slug.split("_") if part)[:32] or "team"
        if slug == "neutral":
            slug = "team"
        candidate = slug
        suffix = 2
        while candidate in self._team_profiles:
            candidate = f"{slug}_{suffix}"[:32]
            suffix += 1
        return candidate

    def _refresh_teams_panel(self) -> None:
        """Rebuild the Teams panel, but only when the set of teams changed.

        Rebuilding on every edit would delete the line edit being typed into and
        close an open stance menu, which is the same trap the runtime inspector
        already had to avoid.
        """
        self._ensure_team_profiles()
        signature = tuple(sorted(
            (team_id, profile.name, profile.color)
            for team_id, profile in self._team_profiles.items()
        ))
        if signature == self._teams_signature:
            return
        self._teams_signature = signature

        while self.teams_layout.count():
            item = self.teams_layout.takeAt(0)
            widget = item.widget()
            if widget is not None:
                widget.deleteLater()

        if not self._team_profiles:
            empty = QLabel("No teams yet. Give a creature slot a team above, and it "
                           "appears here to be named and coloured.")
            empty.setWordWrap(True)
            self.teams_layout.addWidget(empty, 0, 0, 1, 4)
            return

        counts = {}
        for row in range(self.table.rowCount()):
            box = self.table.cellWidget(row, COL_TEAM)
            spin = self.table.cellWidget(row, COL_COUNT)
            if box is None:
                continue
            team_id = normalize_team_id(box.currentData() or "neutral")
            if team_id != "neutral":
                counts[team_id] = counts.get(team_id, 0) + (int(spin.value()) if spin else 1)

        line = 0
        for team_id in sorted(self._team_profiles):
            profile = self._team_profiles[team_id]
            swatch = QPushButton()
            swatch.setFixedSize(QSize(26, 22))
            swatch.setToolTip(f"Colour for {profile.name}")
            swatch.setAccessibleName(f"Colour for {profile.name}")
            swatch.setIcon(self._team_icon(profile.color))
            swatch.clicked.connect(lambda _c=False, tid=team_id: self._pick_team_color(tid))
            name_edit = QLineEdit(profile.name)
            name_edit.setMaxLength(32)
            name_edit.setToolTip("What this team is called, wherever it is named.")
            name_edit.setAccessibleName(f"Name of team {profile.name}")
            name_edit.editingFinished.connect(
                lambda edit=name_edit, tid=team_id: self._rename_team(tid, edit.text()))
            count = counts.get(team_id, 0)
            members = QLabel(f"{count} spider(s)" if count else "no spiders yet")
            self.teams_layout.addWidget(swatch, line, 0)
            self.teams_layout.addWidget(name_edit, line, 1)
            self.teams_layout.addWidget(members, line, 2)
            self.teams_layout.addWidget(QLabel(f"id: {team_id}"), line, 3)
            line += 1

        pairs = stance_pairs(self._team_profiles, self._team_stances)
        if not pairs:
            hint = QLabel("Add a second team to choose what stands between them.")
            hint.setWordWrap(True)
            self.teams_layout.addWidget(hint, line, 0, 1, 4)
            return
        header = QLabel("Between teams")
        header.setStyleSheet("font-weight: 600;")
        self.teams_layout.addWidget(header, line, 0, 1, 4)
        line += 1
        for left, right, relation in pairs:
            label = QLabel(f"{team_label(left, self._team_profiles)} and "
                           f"{team_label(right, self._team_profiles)}")
            combo = NoScrollComboBox()
            for value in ("friend", "neutral", "foe"):
                combo.addItem(STANCE_LABELS[value], value)
            index = combo.findData(relation)
            combo.setCurrentIndex(index if index >= 0 else 1)
            combo.setToolTip(describe_stance(left, right, relation, self._team_profiles))
            combo.setAccessibleName(
                f"Relationship between {team_label(left, self._team_profiles)} and "
                f"{team_label(right, self._team_profiles)}")
            combo.currentIndexChanged.connect(
                lambda _i=0, a=left, b=right, box=combo: self._set_stance(a, b, box))
            self.teams_layout.addWidget(label, line, 0, 1, 2)
            self.teams_layout.addWidget(combo, line, 2, 1, 2)
            line += 1

    def _pick_team_color(self, team_id: str) -> None:
        profile = self._team_profiles.get(team_id)
        if profile is None:
            return
        chosen = QColorDialog.getColor(QColor(*profile.color), self,
                                       f"Colour for {profile.name}")
        if not chosen.isValid():
            return
        self._team_profiles[team_id] = TeamProfile(
            team_id, profile.name, (chosen.red(), chosen.green(), chosen.blue()))
        self._teams_signature = None
        self._refresh_teams_panel()
        self._repopulate_team_boxes()
        self.update_summary()

    def _rename_team(self, team_id: str, text) -> None:
        profile = self._team_profiles.get(team_id)
        if profile is None:
            return
        name = normalize_team_name(text, team_id)
        if name == profile.name:
            return
        self._team_profiles[team_id] = TeamProfile(team_id, name, profile.color)
        self._teams_signature = None
        self._refresh_teams_panel()
        self._repopulate_team_boxes()
        self.update_summary()

    def _set_stance(self, left: str, right: str, combo) -> None:
        """Declare one pair, leaving every other declaration alone.

        Including a chosen "ignore each other": for a team that is hostile by
        default, that is a real decision, and treating it as "nothing declared"
        would quietly restore the hostility on the next launch.
        """
        relation = str(combo.currentData() or "neutral")
        declared = {key: dict(row) for key, row in self._team_stances.items()}
        declared.setdefault(left, {})[right] = relation
        declared.setdefault(right, {})[left] = relation
        self._team_stances = normalize_team_stances(declared)
        combo.setToolTip(describe_stance(left, right, relation, self._team_profiles))
        self.update_summary()

    def update_summary(self):
        rows = self.table.rowCount()
        fixed_count = 0
        random_count_rows = 0
        random_models = 0
        random_personalities = 0
        custom_skill_rows = 0
        for model_box, personality_box, count_spin, skills_btn in self._iter_row_widgets():
            if model_box and model_box.currentData() == RANDOM_MODEL_ID:
                random_models += 1
            if personality_box and personality_box.currentData() == RANDOM_PERSONALITY_ID:
                random_personalities += 1
            if count_spin and bool(count_spin.property("count_random")):
                random_count_rows += 1
            elif count_spin:
                fixed_count += int(count_spin.value())
            if skills_btn and bool(skills_btn.property("skills_custom")):
                custom_skill_rows += 1

        custom_color_rows = 0
        team_counts = {}
        job_counts = {}
        for row in range(self.table.rowCount()):
            colors_btn = self.table.cellWidget(row, COL_COLORS)
            if colors_btn is not None and _normalize_color_overrides(colors_btn.property("color_overrides")):
                custom_color_rows += 1
            team_box = self.table.cellWidget(row, COL_TEAM)
            if team_box is not None:
                team_id = str(team_box.currentData() or "neutral")
                if team_id != "neutral":
                    team_counts[team_id] = team_counts.get(team_id, 0) + 1
            job_box = self.table.cellWidget(row, COL_JOB)
            if job_box is not None:
                job_id = str(job_box.currentData() or "none")
                if job_id != "none":
                    job_counts[job_id] = job_counts.get(job_id, 0) + 1

        if rows == 0:
            creature_text = "No creature slots yet. Add at least one slot to launch."
        else:
            random_text = f" plus {random_count_rows} random-count slot(s)" if random_count_rows else ""
            creature_text = f"{rows} slot(s), {fixed_count} fixed creature(s){random_text}."
            if random_models or random_personalities:
                creature_text += f" Random choices: {random_models} model slot(s), {random_personalities} personality slot(s)."
            if custom_skill_rows:
                creature_text += f" Custom abilities: {custom_skill_rows} slot(s)."
            if custom_color_rows:
                creature_text += f" Custom colors: {custom_color_rows} slot(s)."
            if team_counts:
                creature_text += " Teams: " + ", ".join(
                    f"{team_label(team_id, self._team_profiles)} ({count})"
                    for team_id, count in sorted(team_counts.items())
                ) + "."
            if job_counts:
                creature_text += " Jobs: " + ", ".join(
                    f"{job_id} ({count})" for job_id, count in sorted(job_counts.items())
                ) + "."

        self._refresh_teams_panel()

        size_text = self.size_combo.currentText() if hasattr(self, "size_combo") else "Normal (100%)"
        mood_text = self.mood_combo.currentText() if hasattr(self, "mood_combo") else "Auto"
        move_text = self.movement_combo.currentData() if hasattr(self, "movement_combo") else "classic"
        drag_text = "dragging on" if self.interferable_check.isChecked() else "dragging off"
        social_text = "social play on" if self.social_play_check.isChecked() else "social play off"
        summary = f"Preset summary: {creature_text} Size: {size_text}. Mood: {mood_text}. Movement: {move_text}. {drag_text}; {social_text}."
        self.summary.setText(summary)
        if hasattr(self, "launch_group"):
            self.launch_group.setToolTip(summary)
        if hasattr(self, "launch_btn"):
            self.launch_btn.setToolTip(summary + " Start the overlay, or apply changes live if it is already running.")

    def set_random_model_options(self):
        if self.table.rowCount() == 0:
            self.add_slot(RANDOM_MODEL_ID, None, 1, False)
        for model_box, _personality_box, _count_spin, _skills_btn in self._iter_row_widgets():
            if model_box:
                self._combo_set_data(model_box, RANDOM_MODEL_ID)
        self.status.setText("Every slot will choose a random model when the overlay launches.")
        self.update_summary()

    def set_random_personality_options(self):
        if self.table.rowCount() == 0:
            self.add_slot(None, RANDOM_PERSONALITY_ID, 1, False)
        for _model_box, personality_box, _count_spin, _skills_btn in self._iter_row_widgets():
            if personality_box:
                self._combo_set_data(personality_box, RANDOM_PERSONALITY_ID)
        self.status.setText("Every slot will choose a random personality when the overlay launches.")
        self.update_summary()

    def set_random_count_options(self):
        """Roll each slot's count now, rather than deferring it to launch.

        This used to tick the per-slot "Pick 1-10" box, which deferred the roll
        to launch time and left the visible number lying. With that column gone
        the button rolls a real number into the spin box, so the table shows
        what will actually spawn.
        """
        if self.table.rowCount() == 0:
            self.add_slot(None, None, random.randint(1, 10), False)
        for _model_box, _personality_box, count_spin, _skills_btn in self._iter_row_widgets():
            if count_spin:
                count_spin.setProperty("count_random", False)
                count_spin.setValue(random.randint(1, 10))
        self.status.setText("Rolled a new count from 1 to 10 for every slot.")
        self.update_summary()

    def set_random_all_options(self):
        if self.table.rowCount() == 0:
            self.add_slot(RANDOM_MODEL_ID, RANDOM_PERSONALITY_ID, random.randint(1, 10), False)
        for model_box, personality_box, count_spin, _skills_btn in self._iter_row_widgets():
            if model_box:
                self._combo_set_data(model_box, RANDOM_MODEL_ID)
            if personality_box:
                self._combo_set_data(personality_box, RANDOM_PERSONALITY_ID)
            if count_spin:
                count_spin.setProperty("count_random", False)
                count_spin.setValue(random.randint(1, 10))
        self.status.setText("Surprise mode set: random model, personality, and count for each slot.")
        self.update_summary()

    def _overlay_running(self) -> bool:
        return bool(self.overlay_process and self.overlay_process.poll() is None)

    def _ensure_channel_connected(self, timeout_ms: int = 100) -> bool:
        """Best-effort connect to a running overlay's live channel.

        A failed connection is routine, not an error: the overlay may not be
        up yet, may be an older build with no server, or the channel may
        simply have failed to bind. Every caller falls back to the preset
        file and `session_control`'s stop-request file exactly as before this
        channel existed.
        """
        client = getattr(self, "_channel_client", None)
        if client is None:
            return False
        if client.is_connected():
            return True
        try:
            return client.try_connect(timeout_ms)
        except Exception:
            return False

    def _on_channel_message(self, message: dict) -> None:
        if not isinstance(message, dict):
            return
        if message.get("type") == "session_state":
            self._apply_live_session_state(message.get("state"))

    def _apply_live_session_state(self, state) -> None:
        """A tray-driven change arrived live from the running overlay (C7).

        Updates only the widgets a tray action can actually touch, so this
        never clobbers a slot edit made here that has not been saved or
        applied yet. Per-spider name/team detail is kept on
        `_live_creature_state` for callers (and tests) that want it; the slot
        table edits groups of spiders, not individual runtime creatures, so
        reconciling a rename or a team change back into a specific row is
        left for a later package rather than guessed at here.
        """
        if not isinstance(state, dict):
            return
        if "mood_mode" in state:
            mood_idx = self.mood_combo.findData(str(state["mood_mode"] or "auto").lower())
            if mood_idx >= 0:
                self.mood_combo.setCurrentIndex(mood_idx)
        if "size_scale" in state:
            try:
                size_scale = float(state["size_scale"])
            except (TypeError, ValueError):
                size_scale = None
            if size_scale is not None:
                closest_index, closest_distance = 0, float("inf")
                for idx in range(self.size_combo.count()):
                    distance = abs(float(self.size_combo.itemData(idx)) - size_scale)
                    if distance < closest_distance:
                        closest_index, closest_distance = idx, distance
                self.size_combo.setCurrentIndex(closest_index)
        if "social_play" in state:
            self.social_play_check.setChecked(bool(state["social_play"]))
        if "flies_enabled" in state:
            self.flies_enabled_check.setChecked(bool(state["flies_enabled"]))
        if "interferable" in state:
            self.interferable_check.setChecked(bool(state["interferable"]))
        self._live_creature_state = list(state.get("creatures") or [])
        self.status.setText("Live update received from the running overlay.")

    def _apply_live_if_running(self) -> bool:
        """If the overlay is running, rewrite its preset so it reloads live."""
        if not self._overlay_running() or not self.launched_preset_path:
            return False
        try:
            data = self.current_preset_data()
            save_preset(data, Path(self.launched_preset_path))
            # The file write above is the fallback that always works; push it
            # over the live channel too so the overlay applies it immediately
            # instead of waiting for the next 700 ms poll.
            if self._ensure_channel_connected():
                self._channel_client.send({
                    "type": "preset_update",
                    "preset_path": self.launched_preset_path,
                    "data": data,
                })
            return True
        except Exception as exc:
            QMessageBox.warning(self, "Could not apply live", str(exc))
            return False

    def save_current_preset(self):
        try:
            data = self.current_preset_data()
            path = save_preset(data)
            self.refresh_presets()
            idx = self.preset_combo.findData(str(path))
            if idx >= 0:
                self.preset_combo.setCurrentIndex(idx)
            applied = self._apply_live_if_running()
            if applied:
                self.status.setText(f"Saved your preset to {path}\nApplied changes to the running overlay.")
            else:
                self.status.setText(f"Saved your preset to {path}")
        except Exception as exc:
            QMessageBox.critical(self, "Could not save preset", str(exc))

    def load_selected_preset(self):
        path = self.preset_combo.currentData()
        if path:
            self.load_preset_path(Path(path))

    def load_preset_path(self, path: Path):
        try:
            data = load_preset(path)
            self.preset_name.setText(data.get("name", path.stem))
            self.table.setRowCount(0)
            for slot_index, slot in enumerate(data.get("slots", [])):
                self.add_slot(
                    slot.get("model"),
                    slot.get("personality"),
                    int(slot.get("count", 1)),
                    bool(slot.get("count_random", False)),
                    slot.get("skills"),
                    slot.get("abilities"),
                    slot.get("colors"),
                    slot.get("slot_id") or f"slot-{slot_index}",
                    slot.get("team_id", slot.get("team", "neutral")),
                    slot.get("job", "none"),
                )
            self.apply_settings_to_ui(data.get("settings"))
            self.status.setText(f"Loaded preset: {path}")
        except Exception as exc:
            QMessageBox.critical(self, "Could not load preset", str(exc))

    def ensure_saved_for_launch(self) -> Path:
        # Goes to the user's own preset folder: saving used to write over a
        # preset that shipped with the application, because the filename came
        # from the preset's name and "Default" collides with "default".
        data = self.current_preset_data()
        path = user_presets_dir() / safe_preset_filename(data["name"])
        return save_preset(data, path)

    def launch_engine(self):
        # If an overlay is already running, do not force a stop: rewrite its
        # preset and let it reload the new models, personalities, counts, skills,
        # and settings live.
        if self._overlay_running():
            if self._apply_live_if_running():
                self.status.setText("Applied changes to the running overlay. No restart needed.")
            return
        try:
            preset_path = self.ensure_saved_for_launch()
        except Exception as exc:
            QMessageBox.critical(self, "Could not prepare preset", str(exc))
            return
        if getattr(sys, "frozen", False):
            cmd = [sys.executable, "--engine", "--preset", str(preset_path)]
            env = None
            cwd = str(self.root)
        else:
            # DC-43 moved this module to desktop_bug.app.engine and missed this
            # string, so every launch from a source checkout died in the child
            # with "No module named desktop_bug.engine". A module path in a
            # string is invisible to both the import machinery and ruff;
            # tests/test_module_paths.py now resolves these.
            cmd = [sys.executable, "-m", "desktop_bug.app.engine", "--preset", str(preset_path)]
            env = os.environ.copy()
            existing = env.get("PYTHONPATH", "")
            src_path = str(self.root / "src")
            env["PYTHONPATH"] = src_path + (os.pathsep + existing if existing else "")
            cwd = str(self.root)
        try:
            # A request left behind by a previous session would stop the new
            # overlay the moment it finished loading.
            clear_stop_request(state_dir())
            self.overlay_process = subprocess.Popen(cmd, cwd=cwd, env=env)
        except Exception as exc:
            QMessageBox.critical(self, "Could not launch overlay", str(exc))
            return

        # Popen succeeds as long as the interpreter starts; the overlay failing
        # a moment later is the child's exit code, not an exception here. That
        # is why the DC-43 module-path miss reported "Overlay launched." while
        # nothing had launched. Give it a moment and check it is still alive.
        self._wait_tick(0.6)
        code = self.overlay_process.poll()
        if code is not None:
            self.overlay_process = None
            log.error("Overlay exited immediately with code %s; command was %s", code, cmd)
            QMessageBox.critical(
                self,
                "Overlay stopped immediately",
                f"The overlay process exited with code {code} right after starting.\n\n"
                f"Command: {' '.join(cmd)}\n\n"
                "The log file has the details.",
            )
            self.status.setText(f"Overlay failed to start (exit code {code}). See the log.")
            return

        self.launched_preset_path = str(preset_path)
        self.status.setText("Overlay launched. Edit and press Save to apply changes live, or Stop overlay to close it.")

    def _wait_tick(self, seconds: float) -> None:
        """Sleep without freezing the settings window while the overlay saves."""
        app = QApplication.instance()
        if app is not None:
            app.processEvents()
        time.sleep(seconds)

    def stop_overlay(self):
        if not (self.overlay_process and self.overlay_process.poll() is None):
            self.launched_preset_path = None
            clear_stop_request(state_dir())
            self.status.setText("No overlay process is running from this window.")
            return

        self.status.setText("Stopping overlay...")
        self._wait_tick(0.0)
        # Ask over the live channel first, if it is up: it reaches the overlay
        # immediately rather than on its next poll. `stop_process` below still
        # leaves the stop-request file regardless, so a channel that is down,
        # unreachable, or talking to an older build behaves exactly as it did
        # before this channel existed.
        if self._ensure_channel_connected():
            self._channel_client.send({"type": "stop_request"})
        # Ask the overlay to save and quit before killing it. Terminating it
        # outright discarded any XP, names and base progress that the debounced
        # flush had not yet written.
        outcome = stop_process(self.overlay_process, state_dir(), sleep=self._wait_tick)
        self.launched_preset_path = None
        if outcome == "graceful":
            self.status.setText("Overlay stopped and saved its spiders.")
        elif outcome == "terminated":
            self.status.setText(
                "Overlay did not respond and was closed; recent progress may not have been saved."
            )
        else:
            self.status.setText("No overlay process is running from this window.")

    def update_process_status(self):
        if self.overlay_process and self.overlay_process.poll() is not None:
            code = self.overlay_process.returncode
            self.overlay_process = None
            self.launched_preset_path = None
            self.status.setText(f"Overlay exited with code {code}.")
        if self._overlay_running():
            # A short, non-blocking-in-practice retry: the overlay's server
            # may not have been listening yet the moment it was launched.
            self._ensure_channel_connected(50)

    def open_project_folder(self):
        path = str(self.root)
        try:
            if sys.platform.startswith("win"):
                os.startfile(path)  # type: ignore[attr-defined]
            elif sys.platform == "darwin":
                subprocess.Popen(["open", path])
            else:
                subprocess.Popen(["xdg-open", path])
        except Exception as exc:
            QMessageBox.warning(self, "Could not open folder", str(exc))

    def closeEvent(self, event):  # noqa: N802 - Qt API name
        if self.overlay_process and self.overlay_process.poll() is None:
            reply = QMessageBox.question(
                self,
                "Overlay is running",
                "The spider overlay is still running. Stop it too?",
                QMessageBox.Yes | QMessageBox.No | QMessageBox.Cancel,
            )
            if reply == QMessageBox.Cancel:
                event.ignore()
                return
            if reply == QMessageBox.Yes:
                self.stop_overlay()
        client = getattr(self, "_channel_client", None)
        if client is not None:
            client.close()
        super().closeEvent(event)


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Desktop Bug Companion settings UI")
    parser.add_argument("--engine", action="store_true", help="Internal: run the overlay engine from the packaged executable")
    parser.add_argument("--preset", default=None, help="Preset to use when --engine is present")
    args, remaining = parser.parse_known_args(argv)
    if args.engine:
        from .engine import main as engine_main

        engine_args = []
        if args.preset:
            engine_args.extend(["--preset", args.preset])
        engine_args.extend(remaining)
        return engine_main(engine_args)

    migrate_legacy_state_dir()
    written_to = configure_logging(state_dir())
    log.info("Desktop Bug Companion %s settings window starting (frozen=%s)", __version__, getattr(sys, "frozen", False))
    if written_to is None:
        log.warning("No log file could be opened under %s", state_dir())

    enable_high_dpi_scaling()
    app = QApplication.instance() or QApplication(sys.argv[:1])
    window = ConfigWindow()
    window.show()
    return app.exec_()


if __name__ == "__main__":
    raise SystemExit(main())
