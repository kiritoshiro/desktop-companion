"""The Adventure hero's character window: name, skill tree and armour.

Armour is worn on an anatomy doll (armour_ui); every skill and piece says on
hover exactly what it gives (stat_text).

The owner: *"in adventure window create the character whole skill tree, and
armor, character name. all of it as a window with nice designs."* It edits
adventure-hero.json (adventure_profile) and saves on every change. The hero
banks its skill points while it plays (``chooses_own_skills``), so this is
where they are spent.
"""
from __future__ import annotations

from PyQt5.QtCore import QPointF, Qt, pyqtSignal
from PyQt5.QtGui import QColor, QPainter, QPainterPath, QPen
from PyQt5.QtWidgets import (QDialog, QFrame, QHBoxLayout, QLabel, QLineEdit, QProgressBar,
                             QPushButton, QVBoxLayout, QWidget)

from . import wood_theme
from .armour_ui import InventoryBag, SpiderDoll
from .adventure_profile import (MAX_NAME_LENGTH, clean_name, hero_progression, load_profile,
                                save_profile)
from .stat_text import effect_lines, item_effects, set_line, short_effect, skill_tooltip
from ..state.progression import (ABILITY_BY_ID, ABILITY_TREE, ARMOR_BY_ID, ARMOR_SETS, MAX_LEVEL,
                                 set_bonus_effects, xp_to_next_level)

# Where each skill sits: (column, row). One column per branch, rows by depth,
# so a prerequisite is always drawn above what it opens. A skill added to the
# tree later without a place here goes in a spare column rather than vanishing.
TREE_LAYOUT = {
    "vitality": (0, 0), "carapace_harden": (0, 1),
    "power_strike": (1, 0), "apex_predator": (1, 2),
    "quick_step": (2, 0), "long_stride": (2, 1),
    "silk_sense": (3, 0), "web_crafter": (3, 1), "silk_tracking": (3, 2),
}
NODE_W, NODE_H = 172, 62
COL_GAP, ROW_GAP = 22, 38


def character_qss() -> str:
    t = wood_theme
    return t.dialog_qss() + f"""
        QWidget {{ font-family: "{t.UI_FONT}"; }}
        QLineEdit#heroName {{ background: {t.WALNUT_DEEP}; color: {t.CREAM};
            border: 2px solid {t.BRASS_DEEP}; border-radius: 8px; padding: 4px 10px;
            font-size: 16pt; font-weight: 800; }}
        QLabel#levelBadge {{ background: qradialgradient(cx:0.5, cy:0.4, radius:0.7,
            stop:0 {t.BRASS}, stop:1 {t.BRASS_DEEP}); color: {t.WALNUT_DEEP};
            border: 2px solid {t.WALNUT_DEEP}; border-radius: 26px; font-size: 16pt;
            font-weight: 900; min-width: 52px; max-width: 52px; min-height: 52px; max-height: 52px; }}
        QLabel#sectionTitle {{ color: {t.BRASS}; font-size: 13pt; font-weight: 800; }}
        QLabel#pointsLabel {{ color: {t.CREAM}; font-size: 10pt; font-weight: 700; }}
        QLabel#statLine {{ color: {t.CREAM_SOFT}; font-size: 9pt; }}
        QFrame#sheetPanel {{ background: rgba(20, 12, 6, 150); border: 1px solid {t.BRASS_DEEP};
            border-radius: 10px; }}
        QScrollArea#inventoryBag {{ background: rgba(20, 12, 6, 200); border: 1px solid {t.INK_SOFT};
            border-radius: 8px; }}
        QWidget#bagInner {{ background: transparent; }}
        QFrame#itemTile {{ background: qlineargradient(x1:0, y1:0, x2:0, y2:1,
            stop:0 rgba(90, 58, 32, 230), stop:1 rgba(52, 32, 18, 230));
            border: 1px solid {t.BRASS_DEEP}; border-radius: 8px; }}
        QFrame#itemTile:hover {{ border: 2px solid {t.BRASS}; }}
        QLabel#tileName {{ color: {t.CREAM}; font-size: 8pt; font-weight: 700; background: transparent; }}
        QPushButton#skillNode {{ border-radius: 10px; padding: 4px; font-size: 9pt;
            text-align: center; }}
        QPushButton#skillNode[state="unlocked"] {{ background: qlineargradient(x1:0, y1:0, x2:0, y2:1,
            stop:0 {t.BRASS}, stop:1 {t.BRASS_DEEP}); color: {t.WALNUT_DEEP};
            border: 2px solid {t.WALNUT_DEEP}; font-weight: 800; }}
        QPushButton#skillNode[state="available"] {{ background: {t.WALNUT}; color: {t.CREAM};
            border: 2px solid {t.BRASS}; font-weight: 800; }}
        QPushButton#skillNode[state="available"]:hover {{ background: #5a3820; }}
        QPushButton#skillNode[state="locked"] {{ background: rgba(42, 24, 12, 170); color: {t.INK_SOFT};
            border: 2px dashed {t.INK_SOFT}; }}
        QProgressBar#xpBar {{ background: {t.WALNUT_DEEP}; border: 1px solid {t.BRASS_DEEP};
            border-radius: 6px; color: {t.CREAM}; text-align: center; height: 16px; }}
        QProgressBar#xpBar::chunk {{ background: qlineargradient(x1:0, y1:0, x2:1, y2:0,
            stop:0 #c9a044, stop:1 #f2d488); border-radius: 5px; }}
    """


def tree_positions() -> dict:
    """(x, y) of every skill node, including any the layout does not name."""
    positions = {}
    spare = max((col for col, _ in TREE_LAYOUT.values()), default=-1) + 1
    for index, node in enumerate(ABILITY_TREE):
        col, row = TREE_LAYOUT.get(node.id, (spare + index // 3, index % 3))
        positions[node.id] = (col * (NODE_W + COL_GAP), row * (NODE_H + ROW_GAP))
    return positions


class SkillTree(QWidget):
    """Skill nodes placed by branch, with carved links from each prerequisite."""

    unlock = pyqtSignal(str)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.positions = tree_positions()
        width = max(x for x, _ in self.positions.values()) + NODE_W
        height = max(y for _, y in self.positions.values()) + NODE_H
        self.setFixedSize(width + 8, height + 8)
        self.buttons = {}
        for node in ABILITY_TREE:
            x, y = self.positions[node.id]
            button = QPushButton(self)
            button.setObjectName("skillNode")
            button.setGeometry(x + 4, y + 4, NODE_W, NODE_H)
            button.setCursor(Qt.PointingHandCursor)
            button.clicked.connect(lambda _=False, aid=node.id: self.unlock.emit(aid))
            self.buttons[node.id] = button
        self.state = None

    def show_state(self, state) -> None:
        self.state = state
        for node in ABILITY_TREE:
            button = self.buttons[node.id]
            if node.id in state.unlocked_abilities:
                look, note = "unlocked", "Learned"
            elif state.can_unlock(node.id):
                look, note = "available", f"Learn · {node.cost} pt"
            else:
                look = "locked"
                missing = [ABILITY_BY_ID[r].name for r in node.prerequisites
                           if r not in state.unlocked_abilities]
                note = (f"Level {node.level_required}" if state.level < node.level_required
                        else ("Needs " + ", ".join(missing)) if missing else f"{node.cost} pt")
            button.setProperty("state", look)
            button.setText(f"{node.name}\n{note}")
            button.setToolTip(skill_tooltip(node, state))
            button.style().unpolish(button)
            button.style().polish(button)
        self.update()

    def paintEvent(self, event):  # noqa: N802 - Qt API name
        painter = QPainter(self)
        painter.setRenderHint(QPainter.Antialiasing, True)
        unlocked = set(self.state.unlocked_abilities) if self.state else set()
        for node in ABILITY_TREE:
            for required in node.prerequisites:
                if required not in self.positions:
                    continue
                x0, y0 = self.positions[required]
                x1, y1 = self.positions[node.id]
                start = QPointF(x0 + 4 + NODE_W / 2, y0 + 4 + NODE_H)
                end = QPointF(x1 + 4 + NODE_W / 2, y1 + 4)
                path = QPainterPath(start)
                mid = (start.y() + end.y()) / 2
                path.cubicTo(QPointF(start.x(), mid), QPointF(end.x(), mid), end)
                lit = required in unlocked
                painter.setPen(QPen(QColor(20, 12, 6, 200), 7, Qt.SolidLine, Qt.RoundCap))
                painter.drawPath(path)
                painter.setPen(QPen(QColor(wood_theme.BRASS if lit else wood_theme.INK_SOFT), 3,
                                    Qt.SolidLine, Qt.RoundCap))
                painter.drawPath(path)


class CharacterDialog(QDialog):
    """Name, level, skill tree and armour of the Adventure hero."""

    def __init__(self, parent=None, path=None):
        super().__init__(parent)
        self.path = path
        self.profile = load_profile(path)
        self.state = hero_progression(self.profile)
        self.setWindowTitle("Your Adventure spider")
        self.setStyleSheet(character_qss())
        root = QVBoxLayout(self)
        root.setContentsMargins(18, 16, 18, 16)
        root.setSpacing(12)

        head = QHBoxLayout()
        self.level_badge = QLabel()
        self.level_badge.setObjectName("levelBadge")
        self.level_badge.setAlignment(Qt.AlignCenter)
        head.addWidget(self.level_badge)
        name_box = QVBoxLayout()
        self.name_edit = QLineEdit(self.profile["name"])
        self.name_edit.setObjectName("heroName")
        self.name_edit.setMaxLength(MAX_NAME_LENGTH)
        self.name_edit.setPlaceholderText("Name your spider")
        self.name_edit.editingFinished.connect(self._rename)
        name_box.addWidget(self.name_edit)
        self.xp_bar = QProgressBar()
        self.xp_bar.setObjectName("xpBar")
        name_box.addWidget(self.xp_bar)
        head.addLayout(name_box, 1)
        root.addLayout(head)

        body = QHBoxLayout()
        body.setSpacing(14)
        tree_panel = QFrame()
        tree_panel.setObjectName("sheetPanel")
        tree_col = QVBoxLayout(tree_panel)
        tree_col.setContentsMargins(14, 10, 14, 14)
        title = QLabel("Skill tree")
        title.setObjectName("sectionTitle")
        tree_col.addWidget(title)
        self.points_label = QLabel()
        self.points_label.setObjectName("pointsLabel")
        tree_col.addWidget(self.points_label)
        self.tree = SkillTree()
        self.tree.unlock.connect(self._unlock)
        tree_col.addWidget(self.tree)
        tree_col.addStretch(1)
        body.addWidget(tree_panel)

        armour_panel = QFrame()
        armour_panel.setObjectName("sheetPanel")
        armour_col = QVBoxLayout(armour_panel)
        armour_col.setContentsMargins(14, 10, 14, 14)
        armour_col.setSpacing(6)
        armour_title = QLabel("Armour")
        armour_title.setObjectName("sectionTitle")
        armour_col.addWidget(armour_title)
        hint = QLabel("Drag a piece from the bag onto the spider. Drag it back, or double-click, to take it off.")
        hint.setObjectName("statLine")
        hint.setWordWrap(True)
        armour_col.addWidget(hint)
        self.doll = SpiderDoll()
        self.doll.equip.connect(self._equip)
        self.doll.unequip.connect(self._unequip)
        armour_col.addWidget(self.doll, 0, Qt.AlignHCenter)
        bag_title = QLabel("Bag")
        bag_title.setObjectName("sectionTitle")
        armour_col.addWidget(bag_title)
        self.bag = InventoryBag()
        self.bag.equip.connect(self._equip)
        self.bag.unequip.connect(self._unequip)
        armour_col.addWidget(self.bag)
        self.stats_label = QLabel()
        self.stats_label.setObjectName("statLine")
        self.stats_label.setWordWrap(True)
        self.stats_label.setTextFormat(Qt.RichText)
        armour_col.addWidget(self.stats_label)
        body.addWidget(armour_panel, 1)
        root.addLayout(body)

        close = QPushButton("Done")
        close.clicked.connect(self.accept)
        row = QHBoxLayout()
        row.addStretch(1)
        row.addWidget(close)
        root.addLayout(row)
        self.refresh()

    # -- changes -----------------------------------------------------------
    def _save(self) -> None:
        self.profile["progression"] = self.state.to_dict()
        save_profile(self.profile, self.path)

    def _rename(self) -> None:
        name = clean_name(self.name_edit.text())
        self.name_edit.setText(name)
        if name != self.profile["name"]:
            self.profile["name"] = name
            self._save()

    def _unlock(self, ability_id: str) -> None:
        if not self.state.can_unlock(ability_id):
            return
        node = ABILITY_BY_ID[ability_id]
        self.state.skill_points -= node.cost
        self.state.unlocked_abilities.append(ability_id)
        self._save()
        self.refresh()

    def _equip(self, item_id: str) -> None:
        if self.state.equip(item_id):
            self._save()
            self.refresh()

    def _unequip(self, slot: str) -> None:
        if self.state.unequip(slot):
            self._save()
            self.refresh()

    # -- display -----------------------------------------------------------
    def refresh(self) -> None:
        state = self.state
        self.level_badge.setText(str(state.level))
        need = xp_to_next_level(state.level)
        self.xp_bar.setRange(0, need)
        self.xp_bar.setValue(min(state.xp, need))
        self.xp_bar.setFormat("Max level" if state.level >= MAX_LEVEL
                              else f"Level {state.level}  ·  {state.xp} / {need} XP")
        points = state.skill_points
        self.points_label.setText(
            f"{points} skill point{'s' if points != 1 else ''} to spend · one per level"
            if points else "No points to spend · you earn one each level")
        self.tree.show_state(state)
        self._refresh_slots()
        self.stats_label.setText(self._bonus_text())

    def _refresh_slots(self) -> None:
        self.doll.show_state(self.state)
        self.bag.show_state(self.state)

    def _bonus_text(self) -> str:
        """Everything skills and armour add, then how far each set is."""
        totals: dict[str, float] = {}
        for ability_id in self.state.unlocked_abilities:
            for key, value in ABILITY_BY_ID[ability_id].effects.items():
                totals[key] = totals.get(key, 0.0) + value
        for item_id in self.state.equipped.values():
            item = ARMOR_BY_ID.get(item_id)
            if item is not None:
                for key, value in item_effects(item).items():
                    totals[key] = totals.get(key, 0.0) + value
        for key, value in set_bonus_effects(self.state).items():
            totals[key] = totals.get(key, 0.0) + value
        lines = [short_effect(text) for text, _ in effect_lines(totals)]
        text = ("<b>Bonuses</b>  " + "  ·  ".join(lines)) if lines else \
            "No bonuses yet. Learn skills and wear armour to grow stronger."
        sets = [set_line(set_id, self.state, dim=wood_theme.CREAM_SOFT, good=wood_theme.BRASS)
                for set_id in ARMOR_SETS]
        return text + "<br>" + "<br>".join(sets)
