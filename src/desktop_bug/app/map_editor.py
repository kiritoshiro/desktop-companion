"""The map editor: make your own Adventure maps.

The owner: *"create editor tool. so i could create my self the map. with
spiders/bosses, buildings and the settings in the buildings and spider
enemies armor and everything else."*

On the left, the map: the main screen and the second screen side by side.
Drag a building or an enemy to move it, click to select it. On the right,
four tabs:

- **Map**: title, one line, tier, which armour qualities random enemies wear,
  the companion a first win rewards, the spider cap, reinforcement timing;
- **Buildings**: add and remove buildings; the selected one's name, kind,
  who holds it at the start, which screen, Hatchery reserves, healing supply,
  and its guards -- each with its own kind, level, health and armour;
- **Enemies**: spiders standing on their own anywhere on the map;
- **Boss**: the guardian's name, kind, strength and armour.

Maps are saved with custom_maps.py and appear on the Adventure page under
"Your maps". A building placed on the second screen stands on the main one
for a player with only one monitor.
"""
from __future__ import annotations

import copy

from PyQt5.QtCore import QPointF, QRectF, Qt, pyqtSignal
from PyQt5.QtGui import QColor, QFont, QPainter, QPen
from PyQt5.QtWidgets import (QCheckBox, QComboBox, QDialog, QDoubleSpinBox, QFormLayout, QGridLayout,
                             QHBoxLayout, QLabel, QLineEdit, QListWidget, QMessageBox, QPushButton, QSpinBox,
                             QTabWidget, QVBoxLayout, QWidget)

from . import custom_maps as cm
from . import wood_theme
from .campaign import COMPANIONS
from .mission_art import building_art
from ..content.enemy_kinds import ENEMY_KINDS
from ..state.progression import ARMOR_CATALOG, ARMOR_TIERS, MAX_ITEM_LEVEL

# The nominal screen the map is drawn on, and the mission's own placement
# rule (mission._build_sites): y = 115 + (height - 360) * fy.
NOMINAL_W, NOMINAL_H = 1920.0, 1080.0
TOP, SPAN = 115.0, NOMINAL_H - 360.0
# Every tick box shows its box, ticked or not (the wood theme only drew the tick).
CHECKBOXES = """
QLabel, QCheckBox, QRadioButton, QGroupBox { color: #f6e2b8; background: transparent; }
QGroupBox::title { color: #e8c170; }
QCheckBox::indicator { width: 14px; height: 14px; border: 1px solid #c9a36a; border-radius: 3px;
                       background: #f5e6c8; }
QCheckBox::indicator:checked { background: #9a6a34; image: none; }
QCheckBox::indicator:disabled { background: #8a7a64; }
"""
SLOTS = list(dict.fromkeys(item.slot for item in ARMOR_CATALOG))
ROLE_NAMES = {"guard": "Guard (holds ground)", "weaver": "Weaver (shoots silk)",
              "hunter": "Hunter (pounces)", "spitter": "Spitter (acid)"}


class MapCanvas(QWidget):
    """The two screens, the buildings, the enemies; drag to move."""

    picked = pyqtSignal(str, int)       # ("building" | "enemy", index)
    moved = pyqtSignal()

    def __init__(self, editor):
        super().__init__()
        self.editor = editor
        self.setMinimumSize(560, 600)
        self.dragging = None

    def screens(self):
        """The main screen above, the second below, as large as fits."""
        h = (self.height() - 64) / 2
        w = min(self.width() - 24, h * NOMINAL_W / NOMINAL_H)
        h = w * NOMINAL_H / NOMINAL_W
        x = (self.width() - w) / 2
        return QRectF(x, 24, w, h), QRectF(x, 24 + h + 30, w, h)

    def to_canvas(self, fx, fy, far):
        rect = self.screens()[1 if far else 0]
        s = rect.width() / NOMINAL_W
        x = min(max(NOMINAL_W * fx, 85), NOMINAL_W - 85)
        y = min(max(TOP + SPAN * fy, 85), NOMINAL_H - 85)
        return QPointF(rect.x() + x * s, rect.y() + y * s)

    def from_canvas(self, point):
        main, far_rect = self.screens()
        far = far_rect.contains(point) or (not main.contains(point) and point.y() > main.bottom())
        rect = far_rect if far else main
        s = rect.width() / NOMINAL_W
        fx = (point.x() - rect.x()) / s / NOMINAL_W
        fy = ((point.y() - rect.y()) / s - TOP) / SPAN
        return min(max(fx, 0.0), 1.0), min(max(fy, 0.0), 1.0), far

    def items(self):
        data = self.editor.data
        for i, b in enumerate(data["buildings"]):
            yield "building", i, self.to_canvas(b["fx"], b["fy"], b["far"])
        for i, e in enumerate(data["enemies"]):
            yield "enemy", i, self.to_canvas(e["fx"], e["fy"], e["far"])

    def item_at(self, point):
        best, best_d = None, 26.0
        for kind, index, at in self.items():
            d = ((at.x() - point.x()) ** 2 + (at.y() - point.y()) ** 2) ** 0.5
            if d < best_d:
                best, best_d = (kind, index), d
        return best

    def mousePressEvent(self, event):  # noqa: N802 - Qt API name
        hit = self.item_at(QPointF(event.pos()))
        if hit is not None:
            self.dragging = hit
            self.picked.emit(*hit)

    def mouseMoveEvent(self, event):  # noqa: N802 - Qt API name
        if self.dragging is None:
            return
        kind, index = self.dragging
        fx, fy, far = self.from_canvas(QPointF(event.pos()))
        entry = self.editor.data["buildings" if kind == "building" else "enemies"][index]
        entry["fx"], entry["fy"], entry["far"] = round(fx, 3), round(fy, 3), far
        self.moved.emit()
        self.update()

    def mouseReleaseEvent(self, event):  # noqa: N802 - Qt API name
        self.dragging = None

    def paintEvent(self, event):  # noqa: N802 - Qt API name
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing)
        p.fillRect(self.rect(), QColor(24, 20, 16))
        p.setFont(QFont("Segoe UI", 9, QFont.Bold))
        for rect, label in zip(self.screens(), ("Main screen", "Second screen (if you have one)")):
            p.fillRect(rect, QColor(88, 104, 80))
            p.setPen(QPen(QColor("#9a7951"), 1.5))
            p.drawRect(rect)
            p.setPen(QColor("#f7df93"))
            p.drawText(QRectF(rect.x(), rect.y() - 20, rect.width(), 18), Qt.AlignCenter, label)
        selected = self.editor.selected
        data = self.editor.data
        for i, b in enumerate(data["buildings"]):
            at = self.to_canvas(b["fx"], b["fy"], b["far"])
            art = building_art(b["kind"], b["owned"])
            s = max(0.3, min(0.6, self.screens()[0].width() / 1400))
            p.drawImage(QRectF(at.x() - 120 * s, at.y() - 139 * s, 240 * s, 190 * s), art)
            p.setPen(QColor("#fff0cd"))
            p.setFont(QFont("Segoe UI", 8))
            p.drawText(QRectF(at.x() - 60, at.y() + 8, 120, 14), Qt.AlignCenter, b["name"])
            for g in range(len(b["guards"])):
                p.setPen(Qt.NoPen)
                p.setBrush(QColor("#e0604a"))
                p.drawEllipse(QPointF(at.x() - 18 + g * 9, at.y() + 26), 3.5, 3.5)
            if b["kind"] == "nest":
                p.setPen(QColor("#f6cf6a"))
                p.drawText(QRectF(at.x() - 60, at.y() - 58, 120, 14), Qt.AlignCenter,
                           "♛ " + data["boss"].get("name", "Guardian"))
            if selected == ("building", i):
                p.setPen(QPen(QColor("#f7df93"), 2, Qt.DashLine))
                p.setBrush(Qt.NoBrush)
                p.drawEllipse(at, 30, 22)
        for i, e in enumerate(data["enemies"]):
            at = self.to_canvas(e["fx"], e["fy"], e["far"])
            p.setPen(QPen(QColor("#2a0f0a"), 1.5))
            p.setBrush(QColor("#d0493a"))
            p.drawEllipse(at, 8, 8)
            p.setPen(QColor("#fff0cd"))
            p.setFont(QFont("Segoe UI", 7, QFont.Bold))
            p.drawText(QRectF(at.x() - 8, at.y() - 8, 16, 16), Qt.AlignCenter, e["role"][0].upper())
            if selected == ("enemy", i):
                p.setPen(QPen(QColor("#f7df93"), 2, Qt.DashLine))
                p.setBrush(Qt.NoBrush)
                p.drawEllipse(at, 14, 14)
        p.end()


class SpiderEditor(QWidget):
    """One spider: how it fights, what it is, how strong, what it wears."""

    changed = pyqtSignal()

    def __init__(self, boss=False):
        super().__init__()
        self.boss = boss
        self._loading = False
        form = QFormLayout(self)
        form.setContentsMargins(0, 0, 0, 0)
        self.role = QComboBox()
        for role in cm.ROLES:
            self.role.addItem(ROLE_NAMES[role], role)
        if not boss:
            form.addRow("Fights as", self.role)
        self.kind = QComboBox()
        self.kind.addItem("Auto (the map picks)", "auto")
        for kind in ENEMY_KINDS.values():
            if kind.boss == boss or boss:
                self.kind.addItem(kind.name + ("  (boss)" if kind.boss else ""), kind.id)
        form.addRow("Kind", self.kind)
        self.level = QSpinBox()
        self.level.setRange(-5, 20)
        self.level.setPrefix("+")
        form.addRow("Levels above the map", self.level)
        self.hp = QDoubleSpinBox()
        self.hp.setRange(0.2, 10.0)
        self.hp.setSingleStep(0.1)
        self.hp.setSuffix(" × health")
        form.addRow("Health", self.hp)
        self.auto_armour = QCheckBox("Armour by the map's rules")
        form.addRow(self.auto_armour)
        self.slots = {}
        for slot in SLOTS:
            combo = QComboBox()
            combo.addItem("— none —", "")
            for item in ARMOR_CATALOG:
                if item.slot == slot:
                    combo.addItem(f"{item.name}  ({item.tier})", item.id)
            self.slots[slot] = combo
            form.addRow(slot.replace("_", " ").capitalize(), combo)
        self.item_level = QSpinBox()
        self.item_level.setRange(1, MAX_ITEM_LEVEL)
        form.addRow("Armour level", self.item_level)
        for widget in (self.role, self.kind, *self.slots.values()):
            widget.currentIndexChanged.connect(self._emit)
        for widget in (self.level, self.item_level):
            widget.valueChanged.connect(self._emit)
        self.hp.valueChanged.connect(self._emit)
        self.auto_armour.toggled.connect(self._emit)

    def _emit(self, *_):
        for combo in self.slots.values():
            combo.setEnabled(not self.auto_armour.isChecked())
        self.item_level.setEnabled(not self.auto_armour.isChecked())
        if not self._loading:
            self.changed.emit()

    def set_spec(self, spec):
        self._loading = True
        _select(self.role, spec.get("role", "guard"))
        _select(self.kind, spec.get("kind", "auto"))
        self.level.setValue(int(spec.get("level_bonus", 0)))
        self.hp.setValue(float(spec.get("hp", 1.0)))
        armour = spec.get("armor", "auto")
        self.auto_armour.setChecked(not isinstance(armour, list))
        worn = {}
        if isinstance(armour, list):
            from ..state.progression import ARMOR_BY_ID

            worn = {ARMOR_BY_ID[i].slot: i for i in armour if i in ARMOR_BY_ID}
        for slot, combo in self.slots.items():
            _select(combo, worn.get(slot, ""))
        self.item_level.setValue(int(spec.get("item_level", 1)))
        self._loading = False
        self._emit()

    def apply_to(self, spec):
        if not self.boss:
            spec["role"] = self.role.currentData()
        spec["kind"] = self.kind.currentData()
        spec["level_bonus"] = self.level.value()
        spec["hp"] = round(self.hp.value(), 2)
        if self.auto_armour.isChecked():
            spec["armor"] = "auto"
        else:
            spec["armor"] = [c.currentData() for c in self.slots.values() if c.currentData()]
        spec["item_level"] = self.item_level.value()


class MapEditor(QDialog):
    def __init__(self, parent=None, play=None, map_id=None):
        super().__init__(parent)
        self.setWindowTitle("Map editor")
        self.play_callback = play
        self.data = cm.load_map(map_id) if map_id else None
        self.data = self.data or cm.new_map()
        self.selected = None
        self.guard_index = -1
        self._loading = False
        self.dirty = False

        outer = QVBoxLayout(self)
        bar = QHBoxLayout()
        self.chooser = QComboBox()
        self.chooser.setMinimumWidth(220)
        self.chooser.activated.connect(self._open_chosen)
        bar.addWidget(QLabel("Map"))
        bar.addWidget(self.chooser)
        for name, text, slot in (("new", "New", self.new_map), ("save", "Save", self.save),
                                 ("delete", "Delete", self.delete), ("play", "Save and play", self.play),
                                 ("close", "Close", self.close)):
            button = QPushButton(text)
            button.clicked.connect(lambda _=False, slot=slot: slot())
            bar.addWidget(button)
            setattr(self, f"{name}_button", button)
        bar.addStretch(1)
        outer.addLayout(bar)

        body = QHBoxLayout()
        left = QVBoxLayout()
        self.canvas = MapCanvas(self)
        self.canvas.picked.connect(self.select)
        self.canvas.moved.connect(self._changed)
        left.addWidget(self.canvas, 1)
        hint = QLabel("Drag buildings and enemies to move them; drop one on the lower screen to put it on "
                      "the second screen. Red dots under a building are its guards.")
        hint.setWordWrap(True)
        left.addWidget(hint)
        self.problems = QLabel()
        self.problems.setWordWrap(True)
        self.problems.setStyleSheet("color: #ffb08a;")
        left.addWidget(self.problems)
        body.addLayout(left, 3)
        self.tabs = QTabWidget()
        self.tabs.setMinimumWidth(360)
        self.tabs.addTab(self._map_tab(), "Map")
        self.tabs.addTab(self._building_tab(), "Buildings")
        self.tabs.addTab(self._enemy_tab(), "Enemies")
        self.tabs.addTab(self._boss_tab(), "Boss")
        body.addWidget(self.tabs, 2)
        outer.addLayout(body, 1)
        self.resize(1280, 820)
        # The walnut board of the other dialogs, cream text on it (the owner:
        # "in editor text is invisible almost").
        t = wood_theme
        self.setStyleSheet(t.dialog_qss() + f"""
            QWidget {{ font-family: "{t.UI_FONT}"; }}
            QTabWidget::pane {{ background: rgba(34, 20, 10, 200); border: 1px solid {t.BRASS_DEEP};
                                border-radius: 6px; }}
            QLineEdit, QSpinBox, QDoubleSpinBox, QListWidget {{ background: {t.PARCHMENT}; color: {t.INK};
                border: 1px solid {t.WALNUT_DEEP}; border-radius: 6px; padding: 3px 6px; }}
        """ + CHECKBOXES)
        self._refresh_chooser()
        self.load(self.data)

    # -- tabs --------------------------------------------------------------
    def _map_tab(self):
        tab = QWidget()
        form = QFormLayout(tab)
        self.title = QLineEdit()
        self.blurb = QLineEdit()
        self.tier = QSpinBox()
        self.tier.setRange(1, 6)
        form.addRow("Title", self.title)
        form.addRow("One line", self.blurb)
        form.addRow("Tier (enemy strength)", self.tier)
        grid = QGridLayout()
        self.tier_checks = {}
        for i, tier in enumerate(ARMOR_TIERS):
            check = QCheckBox(tier.capitalize())
            self.tier_checks[tier] = check
            grid.addWidget(check, i // 3, i % 3)
            check.toggled.connect(self._changed)
        form.addRow("Random armour", grid)
        self.reward = QComboBox()
        self.reward.addItem("None", None)
        for companion in COMPANIONS:
            self.reward.addItem(companion.name, companion.id)
        form.addRow("First win rewards", self.reward)
        self.cap = QSpinBox()
        self.cap.setRange(4, 30)
        form.addRow("Most spiders at once", self.cap)
        self.wave = QDoubleSpinBox()
        self.wave.setRange(5, 300)
        self.wave.setSuffix(" s")
        form.addRow("Hatchery sends one every", self.wave)
        for w in (self.title, self.blurb):
            w.textEdited.connect(self._changed)
        for w in (self.tier, self.cap):
            w.valueChanged.connect(self._changed)
        self.wave.valueChanged.connect(self._changed)
        self.reward.currentIndexChanged.connect(self._changed)
        return tab

    def _building_tab(self):
        tab = QWidget()
        box = QVBoxLayout(tab)
        self.building_list = QListWidget()
        self.building_list.currentRowChanged.connect(lambda row: self.select("building", row))
        box.addWidget(self.building_list)
        row = QHBoxLayout()
        self.new_kind = QComboBox()
        for kind in cm.BUILDING_KINDS:
            self.new_kind.addItem(cm.BUILDING_NAMES[kind], kind)
        add = QPushButton("Add building")
        add.clicked.connect(self.add_building)
        remove = QPushButton("Remove")
        remove.clicked.connect(self.remove_building)
        row.addWidget(self.new_kind)
        row.addWidget(add)
        row.addWidget(remove)
        box.addLayout(row)
        form = QFormLayout()
        self.b_name = QLineEdit()
        self.b_kind = QComboBox()
        for kind in cm.BUILDING_KINDS:
            self.b_kind.addItem(cm.BUILDING_NAMES[kind], kind)
        self.b_owned = QCheckBox("Yours at the start")
        self.b_far = QCheckBox("On the second screen")
        self.b_reserves = QSpinBox()
        self.b_reserves.setRange(0, 50)
        self.b_supply = QDoubleSpinBox()
        self.b_supply.setRange(0, 5000)
        form.addRow("Name", self.b_name)
        form.addRow("Kind", self.b_kind)
        form.addRow(self.b_owned)
        form.addRow(self.b_far)
        form.addRow("Hatchery reserves", self.b_reserves)
        form.addRow("Healing supply", self.b_supply)
        box.addLayout(form)
        self.b_name.textEdited.connect(self._building_edited)
        self.b_kind.currentIndexChanged.connect(self._building_edited)
        self.b_owned.toggled.connect(self._building_edited)
        self.b_far.toggled.connect(self._building_edited)
        self.b_reserves.valueChanged.connect(self._building_edited)
        self.b_supply.valueChanged.connect(self._building_edited)
        box.addWidget(QLabel("Guards"))
        self.guard_list = QListWidget()
        self.guard_list.setMaximumHeight(90)
        self.guard_list.currentRowChanged.connect(self._pick_guard)
        box.addWidget(self.guard_list)
        row = QHBoxLayout()
        add_guard = QPushButton("Add guard")
        add_guard.clicked.connect(self.add_guard)
        remove_guard = QPushButton("Remove guard")
        remove_guard.clicked.connect(self.remove_guard)
        row.addWidget(add_guard)
        row.addWidget(remove_guard)
        box.addLayout(row)
        self.guard_editor = SpiderEditor()
        self.guard_editor.changed.connect(self._guard_edited)
        box.addWidget(self.guard_editor)
        box.addStretch(1)
        return tab

    def _enemy_tab(self):
        tab = QWidget()
        box = QVBoxLayout(tab)
        self.enemy_list = QListWidget()
        self.enemy_list.currentRowChanged.connect(lambda row: self.select("enemy", row))
        box.addWidget(self.enemy_list)
        row = QHBoxLayout()
        add = QPushButton("Add enemy")
        add.clicked.connect(self.add_enemy)
        remove = QPushButton("Remove")
        remove.clicked.connect(self.remove_enemy)
        row.addWidget(add)
        row.addWidget(remove)
        box.addLayout(row)
        self.e_far = QCheckBox("On the second screen")
        self.e_far.toggled.connect(self._enemy_edited)
        box.addWidget(self.e_far)
        self.enemy_editor = SpiderEditor()
        self.enemy_editor.changed.connect(self._enemy_edited)
        box.addWidget(self.enemy_editor)
        box.addStretch(1)
        return tab

    def _boss_tab(self):
        tab = QWidget()
        box = QVBoxLayout(tab)
        form = QFormLayout()
        self.boss_name = QLineEdit()
        self.boss_name.textEdited.connect(self._boss_edited)
        form.addRow("Name", self.boss_name)
        box.addLayout(form)
        box.addWidget(QLabel("The boss rises at the enemy nest once the Hatchery is sealed.\n"
                             "Health here multiplies its boss health."))
        self.boss_editor = SpiderEditor(boss=True)
        self.boss_editor.changed.connect(self._boss_edited)
        box.addWidget(self.boss_editor)
        box.addStretch(1)
        return tab

    # -- loading and reading back -----------------------------------------------
    def load(self, data):
        self._loading = True
        self.data = data
        self.title.setText(data["title"])
        self.blurb.setText(data["blurb"])
        self.tier.setValue(data["tier"])
        for tier, check in self.tier_checks.items():
            check.setChecked(tier in data["enemy_tiers"])
        _select(self.reward, data.get("reward_companion"))
        self.cap.setValue(data["cap"])
        self.wave.setValue(data["wave_every"])
        self.boss_name.setText(data["boss"].get("name", "Guardian"))
        self.boss_editor.set_spec(data["boss"])
        self._loading = False
        self.selected = None
        self._refresh_lists()
        self.select("building", 0 if data["buildings"] else -1)
        self._show_problems()
        self.dirty = False

    def _changed(self, *_):
        if self._loading:
            return
        d = self.data
        d["title"] = self.title.text()
        d["blurb"] = self.blurb.text()
        d["tier"] = self.tier.value()
        d["enemy_tiers"] = [t for t, c in self.tier_checks.items() if c.isChecked()]
        d["reward_companion"] = self.reward.currentData()
        d["cap"] = self.cap.value()
        d["wave_every"] = self.wave.value()
        self.dirty = True
        self._refresh_lists(keep=True)
        self._show_problems()
        self.canvas.update()

    def _refresh_lists(self, keep=False):
        self._loading, was = True, self._loading
        rows = (self.building_list.currentRow(), self.enemy_list.currentRow())
        self.building_list.clear()
        for b in self.data["buildings"]:
            where = "second screen" if b["far"] else "main"
            kind = "" if b["name"] == cm.BUILDING_NAMES[b["kind"]] else f"  ·  {cm.BUILDING_NAMES[b['kind']]}"
            self.building_list.addItem(f"{b['name']}{kind}  ·  {len(b['guards'])} guards  ·  {where}")
        self.enemy_list.clear()
        for e in self.data["enemies"]:
            kind = ENEMY_KINDS[e["kind"]].name if e["kind"] in ENEMY_KINDS else "any kind"
            self.enemy_list.addItem(f"{e['role'].capitalize()}  ·  {kind}  ·  +{e['level_bonus']} levels")
        if keep:
            self.building_list.setCurrentRow(rows[0])
            self.enemy_list.setCurrentRow(rows[1])
        self._loading = was

    def _show_problems(self):
        found = cm.problems(self.data)
        self.problems.setText("  ".join(found) if found else "Ready to play.")
        self.play_button.setEnabled(not found)

    # -- selection ---------------------------------------------------------
    def select(self, kind, index):
        if self._loading and self.selected is not None:
            return
        entries = self.data["buildings" if kind == "building" else "enemies"]
        if not 0 <= index < len(entries):
            self.selected = None
            self.canvas.update()
            return
        self.selected = (kind, index)
        self._loading, was = True, self._loading
        if kind == "building":
            b = entries[index]
            self.tabs.setCurrentIndex(1)
            self.building_list.setCurrentRow(index)
            self.b_name.setText(b["name"])
            _select(self.b_kind, b["kind"])
            self.b_owned.setChecked(b["owned"])
            self.b_far.setChecked(b["far"])
            self.b_reserves.setValue(b["reserves"])
            self.b_supply.setValue(b["supply"])
            self._refresh_guards()
        else:
            e = entries[index]
            self.tabs.setCurrentIndex(2)
            self.enemy_list.setCurrentRow(index)
            self.e_far.setChecked(e["far"])
            self.enemy_editor.set_spec(e)
        self._loading = was
        self.canvas.update()

    def _building(self):
        if self.selected and self.selected[0] == "building":
            return self.data["buildings"][self.selected[1]]
        return None

    def _enemy(self):
        if self.selected and self.selected[0] == "enemy":
            return self.data["enemies"][self.selected[1]]
        return None

    # -- buildings -----------------------------------------------------------
    def add_building(self):
        kind = self.new_kind.currentData()
        self.data["buildings"].append({"kind": kind, "name": cm.BUILDING_NAMES[kind], "fx": 0.5, "fy": 0.5,
                                       "far": False, "owned": kind == "home",
                                       "reserves": 6 if kind == "hatchery" else 0, "supply": 180.0,
                                       "guards": [] if kind == "home" else [cm.spider_spec("guard")]})
        self._after_edit()
        self.select("building", len(self.data["buildings"]) - 1)

    def remove_building(self):
        b = self._building()
        if b is not None:
            self.data["buildings"].remove(b)
            self.selected = None
            self._after_edit()

    def _building_edited(self, *_):
        b = self._building()
        if b is None or self._loading:
            return
        b["name"] = self.b_name.text() or cm.BUILDING_NAMES[b["kind"]]
        b["kind"] = self.b_kind.currentData()
        b["owned"] = self.b_owned.isChecked()
        b["far"] = self.b_far.isChecked()
        b["reserves"] = self.b_reserves.value()
        b["supply"] = self.b_supply.value()
        self._after_edit()

    def _refresh_guards(self):
        b = self._building()
        self.guard_list.clear()
        for g in (b["guards"] if b else []):
            kind = ENEMY_KINDS[g["kind"]].name if g["kind"] in ENEMY_KINDS else "any kind"
            armour = "map armour" if g["armor"] == "auto" else f"{len(g['armor'])} pieces"
            self.guard_list.addItem(f"{g['role'].capitalize()}  ·  {kind}  ·  {armour}")
        self.guard_index = 0 if b and b["guards"] else -1
        self.guard_list.setCurrentRow(self.guard_index)
        self.guard_editor.setEnabled(self.guard_index >= 0)
        if self.guard_index >= 0:
            self.guard_editor.set_spec(b["guards"][0])

    def _pick_guard(self, row):
        b = self._building()
        if b is None or not 0 <= row < len(b["guards"]):
            return
        self.guard_index = row
        self.guard_editor.set_spec(b["guards"][row])

    def add_guard(self):
        b = self._building()
        if b is not None and len(b["guards"]) < 8:
            b["guards"].append(cm.spider_spec("guard"))
            self._after_edit()
            self._refresh_guards()
            self.guard_list.setCurrentRow(len(b["guards"]) - 1)

    def remove_guard(self):
        b = self._building()
        if b is not None and 0 <= self.guard_index < len(b["guards"]):
            del b["guards"][self.guard_index]
            self._after_edit()
            self._refresh_guards()

    def _guard_edited(self):
        b = self._building()
        if b is None or not 0 <= self.guard_index < len(b["guards"]):
            return
        self.guard_editor.apply_to(b["guards"][self.guard_index])
        self._after_edit()
        item = self.guard_list.item(self.guard_index)
        g = b["guards"][self.guard_index]
        if item is not None:
            kind = ENEMY_KINDS[g["kind"]].name if g["kind"] in ENEMY_KINDS else "any kind"
            armour = "map armour" if g["armor"] == "auto" else f"{len(g['armor'])} pieces"
            item.setText(f"{g['role'].capitalize()}  ·  {kind}  ·  {armour}")

    # -- enemies -------------------------------------------------------------
    def add_enemy(self):
        spec = cm.spider_spec("hunter")
        spec.update({"fx": 0.6, "fy": 0.5, "far": False})
        self.data["enemies"].append(spec)
        self._after_edit()
        self.select("enemy", len(self.data["enemies"]) - 1)

    def remove_enemy(self):
        e = self._enemy()
        if e is not None:
            self.data["enemies"].remove(e)
            self.selected = None
            self._after_edit()

    def _enemy_edited(self, *_):
        e = self._enemy()
        if e is None or self._loading:
            return
        self.enemy_editor.apply_to(e)
        e["far"] = self.e_far.isChecked()
        self._after_edit()

    def _boss_edited(self, *_):
        if self._loading:
            return
        self.boss_editor.apply_to(self.data["boss"])
        self.data["boss"]["name"] = self.boss_name.text() or "Guardian"
        self._after_edit()

    def _after_edit(self):
        self.dirty = True
        self._refresh_lists(keep=True)
        self._show_problems()
        self.canvas.update()

    # -- files ---------------------------------------------------------------
    def _refresh_chooser(self):
        self.chooser.clear()
        for data in cm.list_maps():
            self.chooser.addItem(data["title"], data["id"])
        self.chooser.addItem("(new, not saved)" if not cm.is_custom(self.data.get("id")) else "", "")
        _select(self.chooser, self.data.get("id") or "")

    def _open_chosen(self, index):
        map_id = self.chooser.itemData(index)
        if map_id and map_id != self.data.get("id"):
            data = cm.load_map(map_id)
            if data is not None:
                self.load(data)

    def new_map(self):
        self.load(cm.new_map())
        self._refresh_chooser()

    def save(self) -> str:
        self.data["id"] = cm.save_map(self.data)
        self.data = cm.load_map(self.data["id"]) or self.data
        self.dirty = False
        self._refresh_chooser()
        return self.data["id"]

    def delete(self, confirm=True):
        map_id = self.data.get("id")
        if not cm.is_custom(map_id):
            return
        if confirm and QMessageBox.question(self, "Delete map", f"Delete “{self.data['title']}”?") \
                != QMessageBox.Yes:
            return
        cm.delete_map(map_id)
        self.new_map()

    def play(self):
        if cm.problems(self.data):
            return
        map_id = self.save()
        if self.play_callback is not None:
            self.accept()
            self.play_callback(map_id)

    def working_copy(self):
        return copy.deepcopy(self.data)


def _select(combo, value):
    index = combo.findData(value)
    if index >= 0:
        combo.setCurrentIndex(index)
