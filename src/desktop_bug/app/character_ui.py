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

from PyQt5.QtCore import QEventLoop, QPointF, QRectF, QSize, Qt, pyqtSignal
from PyQt5.QtGui import QColor, QFont, QIcon, QLinearGradient, QPainter, QPainterPath, QPen
from PyQt5.QtWidgets import (QDialog, QFrame, QHBoxLayout, QLabel, QLineEdit, QProgressBar,
                             QPushButton, QScrollArea, QTabWidget, QVBoxLayout, QWidget)

from . import wood_theme
from .armour_ui import InventoryBag, SpiderDoll
from .skill_art import BRANCH_COLOURS, BRANCHES, branch_crest, skill_icon
from .armoury import (armoury, can_upgrade, level_of, party, sell_price, sell_spares, spares_of,
                      take_off, upgrade, wear, worn_by)
from .adventure_profile import (HERO, PLAYER_TWO, MAX_NAME_LENGTH, clean_name, load_profile,
                                save_profile, spider_progression, store_progression)
from .stat_text import (TIER_COLORS, effect_lines, item_effects, set_line, short_effect,
                        skill_tooltip)
from ..state.progression import (ABILITY_BY_ID, ABILITY_TREE, ARMOR_BY_ID, ARMOR_SETS, MAX_ITEM_LEVEL,
                                 MAX_LEVEL, equipped_items, set_bonus_effects, xp_to_next_level)

# Where each skill sits: (column, row). One column per branch, rows by depth,
# so a prerequisite is always drawn above what it opens. A skill added to the
# tree later without a place here goes in a spare column rather than vanishing.
TREE_LAYOUT = {
    "vitality": (0, 0), "carapace_harden": (0, 1),
    "power_strike": (1, 0), "apex_predator": (1, 2),
    "quick_step": (2, 0), "long_stride": (2, 1),
    "silk_sense": (3, 0), "web_crafter": (3, 1), "silk_tracking": (3, 2),
}
NODE_W, NODE_H = 196, 68
COL_GAP, ROW_GAP = 22, 38
# Room above the nodes for each branch's crest and name, and beside the
# outer columns for their carved edges.
HEADER_H = 52
EDGE = 10
ICON = 46


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
        QFrame#itemTile[tier="uncommon"] {{ border: 2px solid #6fc24a; }}
        QFrame#itemTile[tier="rare"] {{ border: 2px solid #4f9ae8; }}
        QFrame#itemTile[tier="epic"] {{ border: 2px solid #b066f0; }}
        QFrame#itemTile[tier="legendary"] {{ border: 2px solid #f5a431; }}
        QLabel#tileTier {{ font-size: 7pt; font-weight: 800; background: transparent; }}
        QLabel#tileName {{ color: {t.CREAM}; font-size: 8pt; font-weight: 700; background: transparent; }}
        QLabel#tileBadge {{ color: {t.WALNUT_DEEP}; background: {t.BRASS}; border-radius: 6px;
            font-size: 7pt; font-weight: 900; padding: 0px 4px; }}
        QLabel#amberLabel {{ color: #f5b041; font-size: 10pt; }}
        QFrame#itemPanel {{ background: rgba(63, 38, 22, 210); border: 1px solid {t.BRASS_DEEP};
            border-radius: 8px; }}
        QPushButton#whoButton {{ padding: 3px 10px; }}
        QPushButton#whoButton:checked {{ background: {t.BRASS}; color: {t.WALNUT_DEEP}; }}
        QPushButton#skillNode {{ border-radius: 12px; padding: 4px 8px; font-size: 9pt;
            text-align: left; }}
        QPushButton#skillNode[state="unlocked"] {{ background: qlineargradient(x1:0, y1:0, x2:0, y2:1,
            stop:0 {t.BRASS}, stop:1 {t.BRASS_DEEP}); color: {t.WALNUT_DEEP};
            border: 2px solid {t.WALNUT_DEEP}; font-weight: 800; }}
        QPushButton#skillNode[state="available"] {{ background: {t.WALNUT}; color: {t.CREAM};
            border: 2px solid {t.BRASS}; font-weight: 800; }}
        QPushButton#skillNode[state="available"]:hover {{ background: #5a3820; }}
        QPushButton#skillNode[state="locked"] {{ background: rgba(34, 20, 10, 215); color: #b8a080;
            border: 2px dashed #7a6448; }}
        QLabel#xpLabel {{ color: {t.CREAM}; font-size: 10pt; font-weight: 700; background: transparent; }}
        QProgressBar#xpBar {{ background: {t.WALNUT_DEEP}; border: 1px solid {t.BRASS_DEEP};
            border-radius: 5px; }}
        QProgressBar#xpBar::chunk {{ background: qlineargradient(x1:0, y1:0, x2:1, y2:0,
            stop:0 #c9a044, stop:1 #f2d488); border-radius: 5px; }}
    """


def tree_positions() -> dict:
    """(x, y) of every skill node, including any the layout does not name."""
    positions = {}
    spare = max((col for col, _ in TREE_LAYOUT.values()), default=-1) + 1
    for index, node in enumerate(ABILITY_TREE):
        col, row = TREE_LAYOUT.get(node.id, (spare + index // 3, index % 3))
        positions[node.id] = (EDGE + col * (NODE_W + COL_GAP), HEADER_H + row * (NODE_H + ROW_GAP))
    return positions


class SkillTree(QWidget):
    """Skill nodes placed by branch, with carved links from each prerequisite.

    Each branch is a carved column under its crest; each skill a medallion
    with its own picture (skill_art), glowing once learned."""

    unlock = pyqtSignal(str)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.positions = tree_positions()
        width = max(x for x, _ in self.positions.values()) + NODE_W
        height = max(y for _, y in self.positions.values()) + NODE_H
        self.setFixedSize(width + 8 + EDGE, height + 14)
        self.buttons = {}
        for node in ABILITY_TREE:
            x, y = self.positions[node.id]
            button = QPushButton(self)
            button.setObjectName("skillNode")
            button.setGeometry(x + 4, y + 4, NODE_W, NODE_H)
            button.setIconSize(QSize(ICON, ICON))
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
            button.setIcon(QIcon(skill_icon(node.id, {"unlocked": "learned"}.get(look, look), ICON)))
            button.setText(f"{node.name}\n{note}")
            tooltip = skill_tooltip(node, state)
            button.setToolTip(tooltip)
            button.setAccessibleName(f"{node.name}: {note}")
            button.setAccessibleDescription(f"{node.description} Current state: {note}.")
            button.style().unpolish(button)
            button.style().polish(button)
        self.update()

    def paintEvent(self, event):  # noqa: N802 - Qt API name
        painter = QPainter(self)
        painter.setRenderHint(QPainter.Antialiasing, True)
        self._paint_branches(painter)
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

    def _paint_branches(self, painter) -> None:
        """A carved, tinted column per branch, its crest and name on top."""
        font = QFont(painter.font())
        font.setBold(True)
        font.setPointSizeF(11.0)
        for col, (branch, title) in enumerate(BRANCHES):
            x = EDGE + col * (NODE_W + COL_GAP) + 4
            column = QRectF(x - 6, 2, NODE_W + 12, self.height() - 4)
            tint = QColor(BRANCH_COLOURS[branch])
            grad = QLinearGradient(column.topLeft(), column.bottomLeft())
            top = QColor(tint)
            top.setAlpha(70)
            bottom = QColor(tint)
            bottom.setAlpha(8)
            grad.setColorAt(0, top)
            grad.setColorAt(1, bottom)
            painter.setPen(QPen(QColor(0, 0, 0, 110), 1.2))
            painter.setBrush(grad)
            painter.drawRoundedRect(column, 12, 12)
            crest = branch_crest(branch, 34)
            painter.drawPixmap(QPointF(x + 6, 9), crest)
            painter.setFont(font)
            painter.setPen(QColor(0, 0, 0, 160))
            painter.drawText(QRectF(x + 47, 11, NODE_W - 50, 30), Qt.AlignVCenter | Qt.AlignLeft, title)
            painter.setPen(QColor(wood_theme.BRASS))
            painter.drawText(QRectF(x + 46, 10, NODE_W - 50, 30), Qt.AlignVCenter | Qt.AlignLeft, title)


class CharacterDialog(QDialog):
    """Name, level, skill tree and armour of the Adventure hero."""

    changed = pyqtSignal()

    def __init__(self, parent=None, path=None, profile=None, who=HERO, fixed_who=False):
        super().__init__(parent)
        self.path = path
        self.profile = profile if profile is not None else load_profile(path)
        self.state = spider_progression(self.profile, who)
        self.fixed_who = fixed_who
        # Whose armour the Armour tab dresses: the hero or a companion (the
        # owner: "also ability to put armor on companion/s").
        self.who = who
        self.dress = self.state
        self.selected = None      # the piece shown in the item panel
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
        self.name_edit = QLineEdit(self._entry()["name"])
        self.name_edit.setObjectName("heroName")
        self.name_edit.setMaxLength(MAX_NAME_LENGTH)
        self.name_edit.setPlaceholderText("Name your spider")
        self.name_edit.editingFinished.connect(self._rename)
        name_box.addWidget(self.name_edit)
        self.xp_label = QLabel()
        self.xp_label.setObjectName("xpLabel")
        name_box.addWidget(self.xp_label)
        self.xp_bar = QProgressBar()
        self.xp_bar.setObjectName("xpBar")
        # The number is on the label: on the bar it was cream on pale gold.
        self.xp_bar.setTextVisible(False)
        self.xp_bar.setFixedHeight(10)
        name_box.addWidget(self.xp_bar)
        head.addLayout(name_box, 1)
        root.addLayout(head)

        # Skills and armour each get the whole window: with every set in the
        # bag there is no room for both side by side.
        self.tabs = QTabWidget()
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
        tree_col.addWidget(self.tree, 0, Qt.AlignHCenter)
        tree_col.addStretch(1)

        armour_panel = QFrame()
        armour_panel.setObjectName("sheetPanel")
        armour_row = QHBoxLayout(armour_panel)
        armour_row.setContentsMargins(14, 10, 14, 14)
        armour_row.setSpacing(14)
        doll_col = QVBoxLayout()
        doll_col.setSpacing(6)
        armour_title = QLabel("Armour")
        armour_title.setObjectName("sectionTitle")
        doll_col.addWidget(armour_title)
        self.who_row = QHBoxLayout()
        self.who_row.setSpacing(6)
        self.who_buttons = {}
        doll_col.addLayout(self.who_row)
        hint = QLabel("Drag a piece from the bag onto the spider. Drag it back, or double-click, to take it off.")
        hint.setObjectName("statLine")
        hint.setWordWrap(True)
        doll_col.addWidget(hint)
        self.doll = SpiderDoll(compact=fixed_who)
        self.doll.equip.connect(self._equip)
        self.doll.unequip.connect(self._unequip)
        self.doll.picked.connect(self._select)
        doll_col.addWidget(self.doll, 0, Qt.AlignHCenter)
        self.stats_label = QLabel()
        self.stats_label.setObjectName("statLine")
        self.stats_label.setWordWrap(True)
        self.stats_label.setTextFormat(Qt.RichText)
        doll_col.addStretch(1)
        armour_row.addLayout(doll_col)
        bag_col = QVBoxLayout()
        bag_col.setSpacing(6)
        bag_head = QHBoxLayout()
        bag_title = QLabel("Shared bag" if fixed_who else "Bag")
        bag_title.setObjectName("sectionTitle")
        bag_head.addWidget(bag_title)
        bag_head.addStretch(1)
        self.amber_label = QLabel()
        self.amber_label.setObjectName("amberLabel")
        bag_head.addWidget(self.amber_label)
        # The owner: "at some point a shop could be made where something to buy".
        self.shop_button = QPushButton("Shop")
        self.shop_button.setEnabled(False)
        self.shop_button.setToolTip("Coming later: spend amber on armour and more.")
        bag_head.addWidget(self.shop_button)
        bag_col.addLayout(bag_head)
        self.bag = InventoryBag(columns=3 if fixed_who else None)
        self.bag.equip.connect(self._equip)
        self.bag.unequip.connect(self._unequip)
        self.bag.picked.connect(self._select)
        bag_col.addWidget(self.bag, 1)
        # The selected piece: its level and spares, and what can be done with them.
        self.item_panel = QFrame()
        self.item_panel.setObjectName("itemPanel")
        panel = QVBoxLayout(self.item_panel) if fixed_who else QHBoxLayout(self.item_panel)
        panel.setContentsMargins(10, 6, 10, 6)
        self.item_label = QLabel()
        self.item_label.setObjectName("statLine")
        self.item_label.setTextFormat(Qt.RichText)
        self.item_label.setWordWrap(True)
        panel.addWidget(self.item_label, 1)
        self.upgrade_button = QPushButton("Upgrade")
        self.upgrade_button.clicked.connect(self._upgrade)
        panel.addWidget(self.upgrade_button)
        self.sell_button = QPushButton("Sell spare")
        self.sell_button.clicked.connect(lambda: self._sell(1))
        panel.addWidget(self.sell_button)
        self.sell_all_button = QPushButton("Sell all spares")
        self.sell_all_button.clicked.connect(lambda: self._sell(10 ** 6))
        panel.addWidget(self.sell_all_button)
        self.item_panel.setMaximumWidth(self.bag.width())
        bag_col.addWidget(self.item_panel)
        self.stats_label.setMaximumWidth(self.bag.width())
        bag_col.addWidget(self.stats_label)
        armour_row.addLayout(bag_col)

        self.tabs.addTab(armour_panel, "Armour")
        self.tabs.addTab(tree_panel, "Skills")
        for index, icon_name in enumerate(("armor", "skills")):
            icon = wood_theme.menu_icon(icon_name)
            if icon is not None:
                self.tabs.setTabIcon(index, icon)
        root.addWidget(self.tabs, 1)

        close = QPushButton("Done")
        close.clicked.connect(self.accept)
        row = QHBoxLayout()
        row.addStretch(1)
        row.addWidget(close)
        root.addLayout(row)
        self.refresh()
        if fixed_who:
            # Each window fits half a monitor; detailed armour/skill content scrolls.
            content = QWidget()
            content.setObjectName("playerSheet")
            content.setStyleSheet(f"QWidget#playerSheet {{ background: {wood_theme.WALNUT}; }}")
            content.setLayout(root)
            scroll = QScrollArea()
            scroll.setWidgetResizable(True)
            scroll.setWidget(content)
            outer = QVBoxLayout(self)
            outer.setContentsMargins(0, 0, 0, 0)
            outer.addWidget(scroll)

    def _entry(self):
        if self.who == HERO:
            return self.profile
        if self.who == PLAYER_TWO:
            return self.profile.setdefault(PLAYER_TWO, {"name": "Trailblazer", "progression": None})
        return self.profile["companions"][self.who]

    # -- changes -----------------------------------------------------------
    def _save(self) -> None:
        store_progression(self.profile, self.who, self.state)
        save_profile(self.profile, self.path)
        self.changed.emit()

    def _commit(self) -> None:
        """Save a change made straight to the profile (armoury, who wears what)."""
        save_profile(self.profile, self.path)
        self.changed.emit()
        self.refresh()

    def choose_spider(self, who: str) -> None:
        if who in party(self.profile):
            self.who = who
            self.refresh()

    def _select(self, item_id: str) -> None:
        self.selected = item_id if item_id in ARMOR_BY_ID else None
        self._refresh_item_panel()

    def _upgrade(self) -> None:
        if self.selected and upgrade(self.profile, self.selected):
            self._commit()

    def _sell(self, count: int) -> None:
        if self.selected and sell_spares(self.profile, self.selected, count):
            self._commit()

    def _rename(self) -> None:
        name = clean_name(self.name_edit.text())
        self.name_edit.setText(name)
        if name != self._entry()["name"]:
            self._entry()["name"] = name
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
        """Put a piece on the spider being dressed; whoever wore it takes it off."""
        if wear(self.profile, self.who, item_id):
            self.selected = item_id
            self._commit()

    def _unequip(self, slot: str) -> None:
        if take_off(self.profile, self.who, slot):
            self._commit()

    # -- display -----------------------------------------------------------
    def refresh(self) -> None:
        if self.who not in party(self.profile):
            self.who = HERO
        self.state = spider_progression(self.profile, self.who)
        self.dress = spider_progression(self.profile, self.who)
        if not self.name_edit.hasFocus():
            self.name_edit.setText(self._entry()["name"])
        state = self.state
        self.level_badge.setText(str(state.level))
        need = xp_to_next_level(state.level)
        self.xp_bar.setRange(0, need)
        self.xp_bar.setValue(min(state.xp, need))
        text = "Max level" if state.level >= MAX_LEVEL else f"Level {state.level}  ·  {state.xp} / {need} XP"
        self.xp_bar.setFormat(text)
        self.xp_label.setText(text)
        points = state.skill_points
        self.points_label.setText(
            f"{points} skill point{'s' if points != 1 else ''} to spend · one per level"
            if points else "No points to spend · you earn one each level")
        self.tree.show_state(state)
        self._refresh_slots()
        self.stats_label.setText(self._bonus_text())

    def _refresh_slots(self) -> None:
        self._refresh_who()
        self.doll.show_state(self.dress)
        worn = worn_by(self.profile)
        self.bag.show_state(self.dress, worn=set(worn), spares=armoury(self.profile)["spares"])
        self.amber_label.setText(f"<b>{armoury(self.profile)['amber']}</b> amber")
        self._refresh_item_panel()

    def _refresh_who(self) -> None:
        """One button per spider that can wear armour."""
        names = {HERO: self.profile["name"]}
        second = self.profile.get(PLAYER_TWO) or {}
        if self.profile.get("two_player") or second.get("progression") is not None or self.who == PLAYER_TWO:
            names[PLAYER_TWO] = second.get("name", "Trailblazer")
        for cid, entry in (self.profile.get("companions") or {}).items():
            names[cid] = entry.get("name") or cid
        if list(self.who_buttons) != list(names):
            while self.who_row.count():
                entry = self.who_row.takeAt(0)
                if entry.widget() is not None:
                    entry.widget().setParent(None)
            self.who_buttons = {}
            for who, name in names.items():
                button = QPushButton(name)
                button.setObjectName("whoButton")
                button.setCheckable(True)
                button.clicked.connect(lambda _=False, w=who: self.choose_spider(w))
                self.who_row.addWidget(button)
                self.who_buttons[who] = button
            self.who_row.addStretch(1)
        for who, button in self.who_buttons.items():
            button.setText(names[who])
            button.setChecked(who == self.who)
            button.setVisible(not self.fixed_who or who == self.who)

    def _refresh_item_panel(self) -> None:
        item = ARMOR_BY_ID.get(self.selected or "")
        owned = item is not None and item.id in armoury(self.profile)["owned"]
        self.item_panel.setVisible(owned)
        if not owned:
            return
        level, spares = level_of(self.profile, item.id), spares_of(self.profile, item.id)
        wearer = worn_by(self.profile).get(item.id)
        names = {HERO: self.profile["name"], PLAYER_TWO: self.profile.get(PLAYER_TWO, {}).get("name", "Trailblazer")}
        names.update({cid: e.get("name") or cid for cid, e in (self.profile.get("companions") or {}).items()})
        worn = f" · worn by {names.get(wearer, wearer)}" if wearer else ""
        colour = TIER_COLORS.get(item.tier, wood_theme.CREAM)
        self.item_label.setText(
            f"<b style='color:{colour}'>{item.name}</b><br>Level {level}/{MAX_ITEM_LEVEL}"
            f" · {spares} spare{'s' if spares != 1 else ''}{worn}")
        self.upgrade_button.setEnabled(can_upgrade(self.profile, item.id))
        self.upgrade_button.setToolTip("Stack one spare onto it: +1 level, +20% of its stats."
                                       if level < MAX_ITEM_LEVEL else "Already at the highest level.")
        price = sell_price(item.id)
        self.sell_button.setEnabled(spares > 0)
        self.sell_button.setText(f"Sell spare (+{price})")
        self.sell_all_button.setEnabled(spares > 1)

    def _bonus_text(self) -> str:
        """Everything skills and armour add to the spider being dressed, then its sets."""
        dress = self.dress
        totals: dict[str, float] = {}
        for ability_id in dress.unlocked_abilities:
            for key, value in ABILITY_BY_ID[ability_id].effects.items():
                totals[key] = totals.get(key, 0.0) + value
        for item in equipped_items(dress):
            for key, value in item_effects(item).items():
                totals[key] = totals.get(key, 0.0) + value
        for key, value in set_bonus_effects(dress).items():
            totals[key] = totals.get(key, 0.0) + value
        lines = [short_effect(text) for text, _ in effect_lines(totals)]
        text = ("<b>Bonuses</b>  " + "  ·  ".join(lines)) if lines else \
            "No bonuses yet. Learn skills and wear armour to grow stronger."
        # Only the sets being worn: five lines of 0/5 would bury the one that matters.
        worn_sets = {ARMOR_BY_ID[i].set_id for i in dress.equipped.values() if i in ARMOR_BY_ID}
        sets = [set_line(set_id, dress, dim=wood_theme.CREAM_SOFT, good=wood_theme.BRASS)
                for set_id in ARMOR_SETS if set_id in worn_sets]
        return text + "".join("<br>" + line for line in sets)


def show_party_windows(parent=None, path=None, two_player=None):
    """Two independent, simultaneous sheets with one authoritative shared bag."""
    profile = load_profile(path)
    if not (profile.get("two_player", False) if two_player is None else two_player):
        CharacterDialog(parent, path=path).exec_()
        return
    dialogs = [CharacterDialog(parent, path, profile, who, fixed_who=True)
               for who in (HERO, PLAYER_TWO)]
    was_enabled = parent.isEnabled() if parent is not None else False
    if parent is not None:
        parent.setEnabled(False)
    remaining = set(dialogs)

    def closed(dialog):
        remaining.discard(dialog)

    from PyQt5.QtGui import QGuiApplication
    area = QGuiApplication.primaryScreen().availableGeometry()
    for index, dialog in enumerate(dialogs):
        dialog.setWindowTitle(f"Player {index + 1} · Character, skills & shared bag")
        dialog.setWindowModality(Qt.NonModal)
        dialog.setEnabled(True)
        dialog.changed.connect(lambda: [sheet.refresh() for sheet in dialogs])
        dialog.finished.connect(lambda _result, sheet=dialog: closed(sheet))
        dialog.resize(max(350, area.width() // 2 - 20), min(850, area.height() - 80))
        dialog.move(area.left() + index * area.width() // 2 + 8, area.top() + 30)
        dialog.show()
    # Wait by processing events rather than in a nested QEventLoop: once the
    # application has been asked to quit (a stray last-window-closed), every
    # new nested loop returns at once and the sheets would vanish unseen.
    from PyQt5.QtCore import QCoreApplication
    while remaining and not QCoreApplication.closingDown():
        QCoreApplication.processEvents(QEventLoop.AllEvents | QEventLoop.WaitForMoreEvents)
    if parent is not None:
        parent.setEnabled(was_enabled)
    for dialog in dialogs:
        dialog.deleteLater()
