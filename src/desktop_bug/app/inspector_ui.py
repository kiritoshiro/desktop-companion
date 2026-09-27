"""The window a right-click on a spider opens: who it is, how it is doing, its
skills and its armour.

The owner: *"the status after right click, the window seems to be spaced way
too wide. fix it. and make nicer interface. not just like random stats, but
make them look relevant. and inventory / armor page should also have the bag
with all the items and the spider anatomy and could be equipped."*

So: a header with a live portrait, name, team and XP; an Overview of
painted vitals and stat tiles beside the team and relationships; the same
skill tree and the same anatomy doll and bag as the Adventure Character
window (character_ui, armour_ui), wired to this living spider.
"""
from __future__ import annotations

from PyQt5.QtCore import QRectF, Qt, QTimer
from PyQt5.QtGui import QColor, QFont, QLinearGradient, QPainter, QPixmap
from PyQt5.QtWidgets import (QCheckBox, QComboBox, QDialog, QFrame, QGridLayout, QHBoxLayout, QLabel,
                             QProgressBar, QPushButton, QScrollArea, QTabWidget, QVBoxLayout, QWidget)

from . import wood_theme
from .armour_ui import InventoryBag, SpiderDoll
from .character_ui import SkillTree, character_qss
from .skill_art import stat_icon
from .stat_text import TIER_COLORS, effect_lines, item_effects, short_effect
from ..state.progression import ARMOR_BY_ID, ARMOR_CATALOG, MAX_LEVEL, xp_to_next_level
from ..state.teams import HOSTILITY_NOTE, normalize_team_id, team_label
from ..world.jobs import job_definition

SLOT_COUNT = len({item.slot for item in ARMOR_CATALOG})
RELATION_COLOURS = {"friend": "#7ec46a", "neutral": "#c9b48a", "foe": "#e0604c"}
PORTRAIT = 104


def inspector_qss() -> str:
    t = wood_theme
    return character_qss() + f"""
        QLabel#spiderTitle {{ color: {t.CREAM}; font-size: 17pt; font-weight: 900; }}
        QLabel#spiderSub {{ color: {t.CREAM_SOFT}; font-size: 9.5pt; }}
        QLabel#portrait {{ background: qradialgradient(cx:0.5, cy:0.45, radius:0.7,
            stop:0 rgba(120, 86, 50, 230), stop:1 rgba(30, 18, 8, 240));
            border: 2px solid {t.BRASS_DEEP}; border-radius: 14px; }}
        QLabel#teamChip {{ color: {t.WALNUT_DEEP}; font-size: 8.5pt; font-weight: 800;
            border-radius: 9px; padding: 2px 10px; }}
        QFrame#statTile {{ background: rgba(63, 38, 22, 200); border: 1px solid {t.BRASS_DEEP};
            border-radius: 10px; }}
        QLabel#statValue {{ color: {t.CREAM}; font-size: 15pt; font-weight: 900; background: transparent; }}
        QLabel#statCaption {{ color: {t.CREAM_SOFT}; font-size: 8pt; background: transparent; }}
        QLabel#relationName {{ color: {t.CREAM}; font-size: 9.5pt; font-weight: 700; }}
        QCheckBox {{ color: {t.CREAM}; spacing: 8px; }}
        QCheckBox::indicator {{ width: 16px; height: 16px; border: 2px solid {t.BRASS}; border-radius: 4px;
            background: {t.WALNUT_DEEP}; }}
        QCheckBox::indicator:checked {{ background: {t.BRASS}; border-color: {t.CREAM}; }}
        QScrollArea#plainScroll, QScrollArea#plainScroll > QWidget > QWidget {{ background: transparent; border: none; }}
    """


def portrait(creature, size: int = PORTRAIT) -> QPixmap:
    """The spider's body and legs as it stands right now, fitted to ``size``,
    without its name, bars or damage numbers."""
    pixmap = QPixmap(size * 2, size * 2)
    pixmap.fill(Qt.transparent)
    xs = [creature.x] + [leg.foot_x for leg in creature.legs]
    ys = [creature.y] + [leg.foot_y for leg in creature.legs]
    pad = creature.size * 0.9
    span = max(max(xs) - min(xs), max(ys) - min(ys)) + pad * 2
    scale = (size * 2 * 0.9) / max(span, 1.0)
    cx, cy = (max(xs) + min(xs)) / 2, (max(ys) + min(ys)) / 2
    painter = QPainter(pixmap)
    painter.setRenderHint(QPainter.Antialiasing)
    painter.translate(size, size)
    painter.scale(scale, scale)
    painter.translate(-cx, -cy)
    try:
        creature._chain_points_cache.clear()
        if creature._is_regal():
            creature._render_regal(painter)
        elif str(creature.model.get("render_mode", "procedural")).lower() == "sprite_rig":
            creature._render_sprite_rig(painter)
        else:
            creature._render_procedural(painter)
    finally:
        painter.end()
    pixmap.setDevicePixelRatio(2)
    return pixmap


class VitalBar(QWidget):
    """A caption, a value and a thick painted bar."""

    def __init__(self, caption: str, colour=None, parent=None):
        super().__init__(parent)
        self.caption = caption
        self.colour = colour
        self.value = self.maximum = 0.0
        self.setFixedHeight(40)
        self.setMinimumWidth(240)

    def set_value(self, value: float, maximum: float, colour=None) -> None:
        self.value, self.maximum = float(value), float(max(maximum, 1e-6))
        if colour is not None:
            self.colour = colour
        self.update()

    def paintEvent(self, event):  # noqa: N802 - Qt API name
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing)
        font = QFont(self.font())
        font.setBold(True)
        font.setPointSizeF(9.0)
        p.setFont(font)
        p.setPen(QColor(wood_theme.CREAM_SOFT))
        p.drawText(QRectF(0, 0, self.width(), 16), Qt.AlignLeft | Qt.AlignVCenter, self.caption.upper())
        p.setPen(QColor(wood_theme.CREAM))
        p.drawText(QRectF(0, 0, self.width(), 16), Qt.AlignRight | Qt.AlignVCenter,
                   f"{self.value:.0f} / {self.maximum:.0f}")
        track = QRectF(0, 20, self.width(), 14)
        p.setPen(QColor(wood_theme.BRASS_DEEP))
        p.setBrush(QColor(20, 12, 6, 220))
        p.drawRoundedRect(track, 7, 7)
        fraction = max(0.0, min(1.0, self.value / self.maximum))
        if fraction > 0:
            fill = QRectF(track.x() + 2, track.y() + 2, (track.width() - 4) * fraction, track.height() - 4)
            base = QColor(self.colour)
            grad = QLinearGradient(fill.topLeft(), fill.bottomLeft())
            grad.setColorAt(0, base.lighter(135))
            grad.setColorAt(1, base.darker(120))
            p.setPen(Qt.NoPen)
            p.setBrush(grad)
            p.drawRoundedRect(fill, 5, 5)
            p.setBrush(QColor(255, 255, 255, 50))
            p.drawRoundedRect(QRectF(fill.x() + 2, fill.y() + 1, max(0.0, fill.width() - 4), 3), 1.5, 1.5)


class StatTile(QFrame):
    """An icon, a big number and what it means."""

    def __init__(self, kind: str, caption: str, parent=None):
        super().__init__(parent)
        self.setObjectName("statTile")
        row = QHBoxLayout(self)
        row.setContentsMargins(10, 8, 12, 8)
        row.setSpacing(10)
        icon = QLabel()
        icon.setPixmap(stat_icon(kind, 34))
        icon.setStyleSheet("background: transparent;")
        row.addWidget(icon)
        text = QVBoxLayout()
        text.setSpacing(0)
        self.value = QLabel("0")
        self.value.setObjectName("statValue")
        self.caption = QLabel(caption)
        self.caption.setObjectName("statCaption")
        self.caption.setWordWrap(True)
        text.addWidget(self.value)
        text.addWidget(self.caption)
        row.addLayout(text, 1)


def _panel(title: str):
    frame = QFrame()
    frame.setObjectName("sheetPanel")
    column = QVBoxLayout(frame)
    column.setContentsMargins(16, 12, 16, 14)
    column.setSpacing(10)
    heading = QLabel(title)
    heading.setObjectName("sectionTitle")
    column.addWidget(heading)
    return frame, column


class CreatureInspectorDialog(QDialog):
    """Runtime inspector for one live spider."""

    def __init__(self, window, creature):
        super().__init__(window)
        self.window = window
        self.creature = creature
        self.setWindowTitle(f"Inspect {creature.display_name}")
        self.setStyleSheet(inspector_qss())
        self.resize(1200, 780)
        root = QVBoxLayout(self)
        root.setContentsMargins(18, 16, 18, 14)
        root.setSpacing(12)
        root.addLayout(self._header())

        self.tabs = QTabWidget(self)
        self.tabs.addTab(self._overview_tab(), "Overview")
        self.tabs.addTab(self._skills_tab(), "Skill tree")
        self.tabs.addTab(self._armour_tab(), "Inventory && armour")
        root.addWidget(self.tabs, 1)
        close = QPushButton("Done")
        close.clicked.connect(self.accept)
        bottom = QHBoxLayout()
        bottom.addStretch(1)
        bottom.addWidget(close)
        root.addLayout(bottom)

        self._abilities_signature = None
        self._inventory_signature = None
        self.selected = None
        self.refresh_timer = QTimer(self)
        self.refresh_timer.timeout.connect(self.refresh)
        self.refresh_timer.start(400)
        self.refresh()

    # -- building -----------------------------------------------------------
    def _header(self):
        head = QHBoxLayout()
        head.setSpacing(14)
        self.portrait = QLabel()
        self.portrait.setObjectName("portrait")
        self.portrait.setFixedSize(PORTRAIT + 12, PORTRAIT + 12)
        self.portrait.setAlignment(Qt.AlignCenter)
        head.addWidget(self.portrait)
        names = QVBoxLayout()
        names.setSpacing(4)
        top = QHBoxLayout()
        self.title = QLabel()
        self.title.setObjectName("spiderTitle")
        top.addWidget(self.title)
        self.team_chip = QLabel()
        self.team_chip.setObjectName("teamChip")
        top.addWidget(self.team_chip, 0, Qt.AlignVCenter)
        top.addStretch(1)
        names.addLayout(top)
        self.subtitle = QLabel()
        self.subtitle.setObjectName("spiderSub")
        names.addWidget(self.subtitle)
        self.xp_label = QLabel()
        self.xp_label.setObjectName("xpLabel")
        names.addWidget(self.xp_label)
        self.xp_bar = QProgressBar()
        self.xp_bar.setObjectName("xpBar")
        self.xp_bar.setTextVisible(False)
        self.xp_bar.setFixedHeight(10)
        names.addWidget(self.xp_bar)
        head.addLayout(names, 1)
        self.level_badge = QLabel()
        self.level_badge.setObjectName("levelBadge")
        self.level_badge.setAlignment(Qt.AlignCenter)
        head.addWidget(self.level_badge, 0, Qt.AlignTop)
        return head

    def _overview_tab(self):
        tab = QWidget()
        row = QHBoxLayout(tab)
        row.setContentsMargins(4, 10, 4, 4)
        row.setSpacing(14)

        vitals, column = _panel("Vitals")
        self.health_bar = VitalBar("Health")
        self.stamina_bar = VitalBar("Stamina", wood_theme.STAMINA)
        column.addWidget(self.health_bar)
        column.addWidget(self.stamina_bar)
        grid = QGridLayout()
        grid.setSpacing(10)
        self.tiles = {
            "armor": StatTile("armor", "Armour: softens every hit"),
            "damage": StatTile("damage", "Damage: each bite"),
            "points": StatTile("points", "Skill points to spend"),
            "worn": StatTile("worn", "Armour pieces worn"),
        }
        for index, key in enumerate(("armor", "damage", "points", "worn")):
            grid.addWidget(self.tiles[key], index // 2, index % 2)
        column.addLayout(grid)
        shown = QLabel("Shown above this spider")
        shown.setObjectName("sectionTitle")
        column.addWidget(shown)
        self.pin_check = QCheckBox("Level and XP beside its name")
        self.pin_check.toggled.connect(self._set_pin)
        column.addWidget(self.pin_check)
        self.health_pin_check = QCheckBox("Health bar (without the name)")
        self.health_pin_check.setToolTip("Keeps a small health bar over this spider. The name stays hidden "
                                         "unless names are switched on.")
        self.health_pin_check.toggled.connect(self._set_health_pin)
        column.addWidget(self.health_pin_check)
        column.addStretch(1)
        row.addWidget(vitals, 1)

        social, column = _panel("Team")
        self.team_combo = QComboBox()
        self.team_combo.setEditable(True)
        self.team_combo.setToolTip(HOSTILITY_NOTE)
        self._team_signature = None
        self._refresh_team_choices()
        self.team_combo.activated.connect(self._commit_team)
        self.team_combo.lineEdit().editingFinished.connect(self._commit_team)
        column.addWidget(self.team_combo)
        note = QLabel(HOSTILITY_NOTE)
        note.setObjectName("statLine")
        note.setWordWrap(True)
        column.addWidget(note)
        relations = QLabel("How it feels about the others")
        relations.setObjectName("sectionTitle")
        column.addWidget(relations)
        holder = QWidget()
        self.relations_layout = QVBoxLayout(holder)
        self.relations_layout.setContentsMargins(0, 0, 6, 0)
        self.relations_layout.setSpacing(6)
        self.relations_layout.setAlignment(Qt.AlignTop)
        scroll = QScrollArea()
        scroll.setObjectName("plainScroll")
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        scroll.setWidget(holder)
        column.addWidget(scroll, 1)
        self._relation_signature = None
        self._relation_controls = {}
        row.addWidget(social, 1)
        return tab

    def _skills_tab(self):
        panel, column = _panel("Skill tree")
        self.points_label = QLabel()
        self.points_label.setObjectName("pointsLabel")
        column.addWidget(self.points_label)
        self.tree = SkillTree()
        self.tree.unlock.connect(self._unlock)
        scroll = QScrollArea()
        scroll.setObjectName("plainScroll")
        scroll.setWidgetResizable(True)
        holder = QWidget()
        centre = QHBoxLayout(holder)
        centre.addWidget(self.tree, 0, Qt.AlignHCenter | Qt.AlignTop)
        scroll.setWidget(holder)
        column.addWidget(scroll, 1)
        return panel

    def _armour_tab(self):
        panel = QFrame()
        panel.setObjectName("sheetPanel")
        row = QHBoxLayout(panel)
        row.setContentsMargins(14, 10, 14, 14)
        row.setSpacing(14)
        doll_col = QVBoxLayout()
        title = QLabel("Armour")
        title.setObjectName("sectionTitle")
        doll_col.addWidget(title)
        hint = QLabel("Drag a piece from the bag onto the spider. Drag it back, or double-click, to take it off.")
        hint.setObjectName("statLine")
        hint.setWordWrap(True)
        doll_col.addWidget(hint)
        self.doll = SpiderDoll()
        self.doll.equip.connect(self._equip)
        self.doll.unequip.connect(self._unequip)
        self.doll.picked.connect(self._select)
        doll_col.addWidget(self.doll, 0, Qt.AlignHCenter)
        doll_col.addStretch(1)
        row.addLayout(doll_col)

        bag_col = QVBoxLayout()
        bag_title = QLabel("Bag")
        bag_title.setObjectName("sectionTitle")
        bag_col.addWidget(bag_title)
        self.bag = InventoryBag()
        self.bag.equip.connect(self._equip)
        self.bag.unequip.connect(self._unequip)
        self.bag.picked.connect(self._select)
        bag_col.addWidget(self.bag, 1)
        self.item_label = QLabel("Pick a piece to see what it gives.")
        self.item_label.setObjectName("statLine")
        self.item_label.setWordWrap(True)
        self.item_label.setTextFormat(Qt.RichText)
        self.item_label.setMaximumWidth(self.bag.width())
        bag_col.addWidget(self.item_label)
        add_row = QHBoxLayout()
        self.catalogue = QComboBox()
        self.catalogue.setMaximumWidth(self.bag.width() - 90)
        add_row.addWidget(self.catalogue, 1)
        add = QPushButton("Add to bag")
        add.clicked.connect(self._add_selected)
        add_row.addWidget(add)
        bag_col.addLayout(add_row)
        row.addLayout(bag_col)
        return panel

    # -- helpers kept from the old inspector --------------------------------
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
        self.window._announce(self.window.manager.set_creature_health_pin(self.creature, enabled))

    def _team_profiles(self) -> dict:
        return getattr(self.window.manager, "team_profiles", {}) or {}

    def _refresh_team_choices(self) -> None:
        """Every named team plus this spider's own; rebuilt only when that set
        changes, since replacing a combo's items closes its open popup."""
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
        # A chosen entry carries its id; a typed name the scene already uses
        # means that team; anything else becomes a new team id.
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
        others = [other for other in self.window.manager.creatures if other is not self.creature]
        signature = tuple(id(other) for other in others)
        if signature != self._relation_signature:
            self._clear_layout(self.relations_layout)
            self._relation_controls = {}
            if not others:
                empty = QLabel("No other spiders on the desktop.")
                empty.setObjectName("statLine")
                self.relations_layout.addWidget(empty)
            for other in others:
                line = QHBoxLayout()
                dot = QLabel()
                dot.setFixedSize(10, 10)
                line.addWidget(dot)
                label = QLabel(other.display_name)
                label.setObjectName("relationName")
                line.addWidget(label, 1)
                combo = QComboBox()
                combo.addItems(["friend", "neutral", "foe"])
                combo.setFixedWidth(110)
                combo.currentTextChanged.connect(
                    lambda relation, target=other: self._set_relation(target, relation))
                line.addWidget(combo)
                self.relations_layout.addLayout(line)
                self._relation_controls[id(other)] = (other, label, combo, dot)
            self._relation_signature = signature
        for other in others:
            entry = self._relation_controls.get(id(other))
            if entry is None:
                continue
            _other, label, combo, dot = entry
            label.setText(other.display_name)
            relation = self.creature.relation_to(other)
            dot.setStyleSheet(f"background: {RELATION_COLOURS.get(relation, '#c9b48a')}; border-radius: 5px;")
            # Leave an open popup alone; it is synchronised once closed.
            if combo.currentText() != relation and not combo.view().isVisible():
                combo.blockSignals(True)
                combo.setCurrentText(relation)
                combo.blockSignals(False)

    # -- changes ------------------------------------------------------------
    def _unlock(self, ability_id: str) -> None:
        self.window._announce(self.window.manager.unlock_creature_ability(self.creature, ability_id))
        self.refresh()

    def _equip(self, item_id: str) -> None:
        self.window._announce(self.window.manager.equip_creature_item(self.creature, item_id))
        self.selected = item_id
        self.refresh()

    def _unequip(self, slot: str) -> None:
        self.window._announce(self.window.manager.unequip_creature_item(self.creature, slot))
        self.refresh()

    def _add_item(self, item_id: str) -> None:
        if self.creature.add_inventory_item(item_id):
            self.window.manager.save_runtime_state()
            self.window._announce("Added armour to this spider's bag.")
        self.refresh()

    def _add_selected(self) -> None:
        item_id = self.catalogue.currentData()
        if item_id:
            self._add_item(item_id)

    def _select(self, item_id: str) -> None:
        self.selected = item_id if item_id in ARMOR_BY_ID else None
        self._refresh_item_label()

    # -- refreshing ---------------------------------------------------------
    def refresh(self) -> None:
        creature = self.creature
        if not creature or creature not in self.window.manager.creatures:
            self.close()
            return
        snap = creature.progression_snapshot()
        profiles = self._team_profiles()
        team_id = normalize_team_id(creature.progression.team_id)
        self.title.setText(creature.display_name)
        team = team_label(creature.progression.team_id, profiles)
        profile = profiles.get(team_id)
        colour = profile.hex_color() if profile is not None else ("#c9b48a" if team_id == "neutral" else "#e8c170")
        self.team_chip.setText(team)
        self.team_chip.setStyleSheet(f"background: {colour};")
        model = str(creature.model.get("display_name") or creature.model.get("name") or "Spider")
        job = job_definition(creature.job_id).display_name
        self.subtitle.setText(f"{model}  ·  {job}")
        self.level_badge.setText(str(creature.level))
        need = xp_to_next_level(creature.level)
        capped = creature.level >= MAX_LEVEL
        self.xp_bar.setRange(0, max(1, need))
        self.xp_bar.setValue(need if capped else min(need, snap["xp"]))
        self.xp_label.setText("Level cap reached" if capped
                              else f"Level {creature.level}  ·  {snap['xp']} / {need} XP")
        self.portrait.setPixmap(portrait(creature))

        fraction = snap["hp"] / max(snap["max_hp"], 1e-6)
        self.health_bar.set_value(snap["hp"], snap["max_hp"], creature.health_bar_color(fraction, QColor))
        self.stamina_bar.set_value(snap["energy"], snap["max_energy"])
        self.tiles["armor"].value.setText(f"{snap['armor']:.1f}")
        self.tiles["damage"].value.setText(f"{snap['damage']:.1f}")
        self.tiles["points"].value.setText(str(snap["skill_points"]))
        self.tiles["worn"].value.setText(f"{len(creature.progression.equipped)} / {SLOT_COUNT}")
        for check, value in ((self.pin_check, creature.level_label_pinned),
                             (self.health_pin_check, creature.health_label_pinned)):
            check.blockSignals(True)
            check.setChecked(value)
            check.blockSignals(False)
        self._refresh_team_choices()
        self._refresh_relations()
        self._refresh_abilities()
        self._refresh_inventory()

    def _refresh_abilities(self) -> None:
        state = self.creature.progression
        signature = (self.creature.level, state.skill_points, tuple(state.unlocked_abilities))
        if signature == self._abilities_signature:
            return
        points = state.skill_points
        self.points_label.setText(f"{points} skill point{'s' if points != 1 else ''} to spend · one per level")
        self.tree.show_state(state)
        self._abilities_signature = signature

    def _refresh_inventory(self) -> None:
        state = self.creature.progression
        signature = (tuple(state.inventory), tuple(sorted(state.equipped.items())),
                     tuple(sorted((getattr(state, "item_levels", None) or {}).items())))
        if signature == self._inventory_signature:
            return
        self.doll.show_state(state)
        self.bag.show_state(state)
        self.catalogue.clear()
        owned = set(state.inventory)
        for item in ARMOR_CATALOG:
            if item.id not in owned:
                self.catalogue.addItem(f"{item.name}  ({item.tier}, {item.slot})", item.id)
        self.catalogue.setEnabled(self.catalogue.count() > 0)
        self._inventory_signature = signature
        self._refresh_item_label()

    def _refresh_item_label(self) -> None:
        item = ARMOR_BY_ID.get(self.selected or "")
        if item is None:
            self.item_label.setText("Pick a piece to see what it gives.")
            return
        colour = TIER_COLORS.get(item.tier, wood_theme.CREAM)
        effects = ", ".join(short_effect(text) for text, _good in effect_lines(item_effects(item)))
        self.item_label.setText(f"<b style='color:{colour}'>{item.name}</b> · {item.slot}"
                                f"<br>{effects or item.description}")

    def closeEvent(self, event):  # noqa: N802 - Qt API name
        self.refresh_timer.stop()
        super().closeEvent(event)
