"""Mode selection around the existing Companion settings page.

Three modes: Companion (the living desktop colony), Adventure (take control
of one spider; skirmish missions grow here) and Strategy (colony command,
later). Dressed in the carved-wood theme from ``wood_theme``.
"""

from __future__ import annotations

from PyQt5.QtCore import Qt
from PyQt5.QtGui import QColor, QGuiApplication
from PyQt5.QtWidgets import (QCheckBox, QFrame, QGraphicsDropShadowEffect, QGridLayout, QHBoxLayout, QLabel,
                             QLayout, QPushButton, QScrollArea, QStackedWidget, QVBoxLayout, QWidget)

from . import wood_theme
from .adventure_profile import hero_progression, load_profile, mission_record, save_profile
from .campaign import MAP_BY_ID, MAPS, map_unlocked
from .map_art import map_picture
from .controls import load_controls

MODES = (
    ("companion", "Companion",
     "Your spiders live on the desktop: they hunt flies, spin webs, build bases "
     "and grow. Manage the colony."),
    ("skirmish", "Adventure",
     "Take control of one spider. Hunt, fight and level up with every skill it "
     "has. Skirmish missions grow here."),
    ("strategy", "Strategy",
     "Command a colony against rival teams: build bases, direct squads, take "
     "territory. Coming later."),
)


def _engrave(label: QLabel, light: bool = True) -> QLabel:
    """A soft lip under the letters, as if cut into the board."""
    effect = QGraphicsDropShadowEffect(label)
    effect.setOffset(0, 1.5)
    effect.setBlurRadius(2)
    effect.setColor(QColor(255, 226, 170, 150) if light else QColor(20, 9, 2, 200))
    label.setGraphicsEffect(effect)
    return label


def _art(kind: str, width: int) -> QLabel:
    label = QLabel()
    label.setObjectName("modeArt")
    label.setAlignment(Qt.AlignCenter)
    pixmap = wood_theme.mode_art(kind)
    if pixmap is not None:
        label.setPixmap(pixmap.scaledToWidth(width, Qt.SmoothTransformation))
    return label


# Skirmish missions on the Adventure page. The owner: "it should list more
# skirmish missions in smaller rectangles and player would choose one." The
# campaign's maps come first, each unlocked by winning the one before (the
# owner: "on other maps better armor"); the rest are placeholders.
# (id, title, one line, playable)
SKIRMISH_MISSIONS = tuple((m.id, m.title, m.blurb, True) for m in MAPS) + (
    ("burrow", "Hold the burrow", "Wave after wave comes for your home. Keep it.", False),
)
MISSION_COLUMNS = 3


class ModeShell(QWidget):
    def __init__(self, companion: QWidget, start_adventure, parent=None, leave_adventure=None):
        super().__init__(parent)
        # Called when the player navigates away from the Adventure page, so
        # the Adventure overlay does not stay on the desktop behind them.
        self.leave_adventure = leave_adventure
        self.setObjectName("modeShell")
        # A QWidget subclass only paints a style-sheet background when asked.
        self.setAttribute(Qt.WA_StyledBackground, True)
        self.setStyleSheet(wood_theme.shell_qss())
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        self.stack = QStackedWidget()
        outer.addWidget(self.stack)
        self.home = self._home()
        self.stack.addWidget(self.home)
        self.companion = self._page("Companion", "Your living desktop colony", companion)
        self.skirmish = self._adventure_page(start_adventure)
        self.strategy = self._strategy_page()
        for page in (self.companion, self.skirmish, self.strategy):
            self.stack.addWidget(page)
        self.stack.setCurrentWidget(self.home)
        self._current_page = self.home
        self.stack.currentChanged.connect(self._page_changed)

    def _page_changed(self, _index: int) -> None:
        previous, self._current_page = self._current_page, self.stack.currentWidget()
        if previous is self.skirmish and self._current_page is not self.skirmish:
            if self.leave_adventure is not None:
                self.leave_adventure()

    def show_mode(self, mode: str):
        self.stack.setCurrentWidget(getattr(self, mode))

    def _home(self):
        page = QWidget()
        layout = QVBoxLayout(page)
        layout.setContentsMargins(36, 30, 36, 34)
        layout.setSpacing(8)
        layout.addStretch(1)
        title = QLabel("Desktop Companion")
        title.setObjectName("modeHeading")
        title.setAlignment(Qt.AlignCenter)
        layout.addWidget(_engrave(title))
        sub = QLabel("Choose how to play")
        sub.setObjectName("modeSubheading")
        sub.setAlignment(Qt.AlignCenter)
        layout.addWidget(_engrave(sub))
        layout.addSpacing(14)
        cards = QHBoxLayout()
        cards.setSpacing(18)
        self.mode_buttons = {}
        for kind, title_text, description in MODES:
            card = QFrame()
            card.setObjectName("modeCard")
            card.setMinimumWidth(250)
            card.setMaximumWidth(340)
            cell = QVBoxLayout(card)
            cell.setContentsMargins(0, 0, 0, 0)
            cell.setSpacing(10)
            cell.addWidget(_art(kind, 250))
            heading = QLabel(title_text)
            heading.setObjectName("modeTitle")
            cell.addWidget(_engrave(heading, light=False))
            desc = QLabel(description)
            desc.setWordWrap(True)
            desc.setObjectName("modeText")
            cell.addWidget(desc)
            cell.addStretch()
            button = QPushButton("Preview" if kind == "strategy" else "Play " + title_text)
            button.setObjectName("modeChoice")
            button.setCursor(Qt.PointingHandCursor)
            button.clicked.connect(lambda _=False, value=kind: self.show_mode(value))
            cell.addWidget(button)
            self.mode_buttons[kind] = button
            cards.addWidget(card, 1)
        layout.addLayout(cards)
        layout.addStretch(1)
        return page

    def _page(self, title, subtitle, body):
        page = QWidget()
        layout = QVBoxLayout(page)
        layout.setContentsMargins(16, 12, 16, 12)
        bar = QFrame()
        bar.setObjectName("pageHeader")
        header = QHBoxLayout(bar)
        header.setContentsMargins(10, 6, 14, 6)
        back = QPushButton("← Modes")
        back.setObjectName("modeBack")
        back.setCursor(Qt.PointingHandCursor)
        back.clicked.connect(lambda: self.stack.setCurrentWidget(self.home))
        header.addWidget(back)
        label = QLabel(title + "  ·  " + subtitle)
        label.setObjectName("modeTitle")
        label.setWordWrap(True)
        header.addWidget(_engrave(label, light=False), 1)
        layout.addWidget(bar)
        if isinstance(body, QLabel):
            body.setObjectName("modeText")
            body.setWordWrap(True)
        layout.addWidget(body, 1)
        return page

    def _plaque(self, kind: str, lines, button: QPushButton | None = None) -> QWidget:
        """A picture, and a carved plaque of text below it."""
        panel = QWidget()
        layout = QVBoxLayout(panel)
        layout.setContentsMargins(24, 16, 24, 16)
        layout.setSpacing(14)
        layout.addWidget(_art(kind, 520), alignment=Qt.AlignHCenter)
        plaque = QFrame()
        plaque.setObjectName("modePlaque")
        plaque.setFixedWidth(620)
        text = QVBoxLayout(plaque)
        # Always its full height: the scroll area shrinks the page to its
        # minimum when space is short, and squeezed, the lines clipped.
        text.setSizeConstraint(QLayout.SetFixedSize)
        text.setContentsMargins(0, 0, 0, 0)
        text.setSpacing(8)
        # Wrapped text is measured at the frame's full width, not inside its
        # 30px carved border, so it came out a line short and clipped. At a
        # fixed inner width it measures true.
        inner = 620 - 2 * 30
        for line in lines:
            label = line if isinstance(line, QWidget) else QLabel(line)
            if isinstance(label, QLabel):
                label.setObjectName("modeText")
                label.setWordWrap(True)
                label.setFixedWidth(inner)
            text.addWidget(label)
        if button is not None:
            text.addSpacing(6)
            text.addWidget(button)
        layout.addWidget(plaque, alignment=Qt.AlignHCenter)
        layout.addStretch()
        # Scrolls rather than squeezes when the window is short.
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.NoFrame)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        scroll.setStyleSheet("QScrollArea, QScrollArea > QWidget > QWidget { background: transparent; }")
        scroll.setWidget(panel)
        return scroll

    def _adventure_page(self, start_adventure):
        self._start_adventure = start_adventure
        panel = QWidget()
        layout = QVBoxLayout(panel)
        layout.setContentsMargins(24, 12, 24, 16)
        layout.setSpacing(10)
        layout.addWidget(_art("skirmish", 200), alignment=Qt.AlignHCenter)
        # The hero: named and levelled in the character window (the owner).
        hero = QFrame()
        hero.setObjectName("heroStrip")
        strip = QHBoxLayout(hero)
        strip.setContentsMargins(14, 8, 10, 8)
        self.hero_label = QLabel()
        self.hero_label.setObjectName("missionTitle")
        self.hero_label.setWordWrap(True)
        strip.addWidget(self.hero_label, 1)
        character = QPushButton("Character\u2026")
        character.setObjectName("modeChoice")
        character.setCursor(Qt.PointingHandCursor)
        character.clicked.connect(self.open_character)
        self.character_button = character
        strip.addWidget(character)
        hero.setMaximumWidth(740)
        layout.addWidget(hero, alignment=Qt.AlignHCenter)

        heading = QLabel("Skirmish missions")
        heading.setObjectName("missionHeading")
        heading.setAlignment(Qt.AlignCenter)
        layout.addWidget(_engrave(heading))

        grid = QGridLayout()
        grid.setHorizontalSpacing(12)
        grid.setVerticalSpacing(12)
        self.mission_cards = {}
        self.mission_status = {}
        self.map_buttons = {}
        self.map_locks = {}
        self.map_pictures = {}
        self.adventure_launch = None
        for index, (mission_id, title, blurb, playable) in enumerate(SKIRMISH_MISSIONS):
            card = self._mission_card(mission_id, title, blurb, playable, start_adventure)
            grid.addWidget(card, index // MISSION_COLUMNS, index % MISSION_COLUMNS)
        holder = QWidget()
        holder.setLayout(grid)
        holder.setMaximumWidth(760)
        layout.addWidget(holder, alignment=Qt.AlignHCenter)

        # Maps made in the map editor (the owner: "create editor tool. so i
        # could create my self the map"). Rebuilt on every refresh.
        self.custom_heading = QLabel("Your maps")
        self.custom_heading.setObjectName("missionHeading")
        self.custom_heading.setAlignment(Qt.AlignCenter)
        layout.addWidget(_engrave(self.custom_heading))
        self.custom_grid = QGridLayout()
        self.custom_grid.setHorizontalSpacing(12)
        self.custom_grid.setVerticalSpacing(12)
        self.custom_cards = {}
        custom_holder = QWidget()
        custom_holder.setLayout(self.custom_grid)
        custom_holder.setMaximumWidth(760)
        layout.addWidget(custom_holder, alignment=Qt.AlignHCenter)

        # Multi-screen raids (the owner: "make multi screen missions too. to
        # recognise automatically where are the screens"). On by default;
        # Reclaim the desktop always freezes every screen.
        self.all_screens_check = QCheckBox()
        self.all_screens_check.setObjectName("allScreens")
        self.all_screens_check.setCursor(Qt.PointingHandCursor)
        self.all_screens_check.toggled.connect(self._set_all_screens)
        layout.addWidget(self.all_screens_check, alignment=Qt.AlignHCenter)
        # One switch per other screen: a monitor asleep or off is skipped by
        # itself; one showing another PC can be switched off here (the owner:
        # "only when the second/or other screens are active only then
        # populate them").
        self.screen_box = QVBoxLayout()
        self.screen_checks = {}
        screen_holder = QWidget()
        screen_holder.setLayout(self.screen_box)
        layout.addWidget(screen_holder, alignment=Qt.AlignHCenter)

        # One short line instead of the old instructions (the owner: "the
        # instructions could be smaller too, and maybe unnecessary"). The
        # whole list lives behind Controls.
        controls = QPushButton("Controls\u2026")
        controls.setObjectName("modeBack")
        controls.setCursor(Qt.PointingHandCursor)
        controls.clicked.connect(self.open_controls)
        self.controls_button = controls
        self.controls_line = QLabel()
        self.controls_line.setObjectName("controlsHintLine")
        self.controls_line.setWordWrap(True)
        self.controls_line.setAlignment(Qt.AlignCenter)
        self.controls_line.setFixedWidth(740)
        self.refresh_controls_summary()
        self.refresh_adventure()
        layout.addWidget(self.controls_line, alignment=Qt.AlignHCenter)
        tools = QHBoxLayout()
        tools.addStretch(1)
        tools.addWidget(controls)
        for text, slot, attr in (("Map editor\u2026", self.open_editor, "editor_button"),
                                 ("Admin\u2026", self.open_admin, "admin_button")):
            button = QPushButton(text)
            button.setObjectName("modeBack")
            button.setCursor(Qt.PointingHandCursor)
            button.clicked.connect(lambda _=False, slot=slot: slot())
            setattr(self, attr, button)
            tools.addWidget(button)
        tools.addStretch(1)
        layout.addLayout(tools)
        layout.addStretch(1)

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.NoFrame)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        scroll.setStyleSheet("QScrollArea, QScrollArea > QWidget > QWidget { background: transparent; }")
        scroll.setWidget(panel)
        return self._page("Adventure", "Play as your spider", scroll)

    def _mission_card(self, mission_id, title, blurb, playable, start_adventure) -> QFrame:
        card = QFrame()
        card.setObjectName("missionCard")
        card.setProperty("locked", not playable)
        card.setMinimumWidth(220)
        card.setMaximumWidth(240)
        cell = QVBoxLayout(card)
        cell.setContentsMargins(12, 10, 12, 10)
        cell.setSpacing(6)
        picture = QLabel()
        picture.setObjectName("missionPicture")
        picture.setPixmap(map_picture(mission_id, MAP_BY_ID[mission_id].kind if mission_id in MAP_BY_ID else "raid",
                                      not playable))
        cell.addWidget(picture, alignment=Qt.AlignHCenter)
        self.map_pictures[mission_id] = picture
        name = QLabel(title)
        name.setObjectName("missionTitle")
        name.setWordWrap(True)
        cell.addWidget(name)
        text = QLabel(blurb)
        text.setObjectName("missionText")
        text.setWordWrap(True)
        text.setAlignment(Qt.AlignLeft | Qt.AlignTop)
        cell.addWidget(text, 1)
        if playable:
            status = QLabel()
            status.setObjectName("missionDone")
            status.setWordWrap(True)
            cell.addWidget(status)
            self.mission_status[mission_id] = status
            button = QPushButton("Play")
            button.setObjectName("modeChoice")
            button.setCursor(Qt.PointingHandCursor)
            button.clicked.connect(lambda _=False, mid=mission_id: self.play_map(mid, start_adventure))
            if self.adventure_launch is None:
                self.adventure_launch = button
            cell.addWidget(button)
            self.map_buttons[mission_id] = button
            lock = QLabel()
            lock.setObjectName("missionLocked")
            lock.setWordWrap(True)
            cell.addWidget(lock)
            self.map_locks[mission_id] = lock
        else:
            soon = QLabel("Coming soon")
            soon.setObjectName("missionLocked")
            cell.addWidget(soon)
        self.mission_cards[mission_id] = card
        return card

    def refresh_adventure(self) -> None:
        """Hero name, level and mission results from adventure-hero.json."""
        profile = load_profile()
        state = hero_progression(profile)
        points = state.skill_points
        spend = f"  \u00b7  {points} point{'s' if points != 1 else ''} to spend" if points else ""
        companions = len(profile.get("companions") or {})
        amber = int((profile.get("armoury") or {}).get("amber", 0))
        admin = bool(profile.get("admin", False))
        self.hero_label.setText(f"{profile['name']}  \u00b7  Level {state.level}{spend}"
                                f"  \u00b7  {companions} companion{'s' if companions != 1 else ''}"
                                f"  \u00b7  {amber} amber" + ("  \u00b7  ADMIN" if admin else ""))
        screens = len(QGuiApplication.screens())
        check = getattr(self, "all_screens_check", None)
        if check is not None:
            check.blockSignals(True)
            check.setChecked(bool(profile.get("all_screens", True)))
            check.blockSignals(False)
            found = f"{screens} screens found" if screens != 1 else "1 screen found"
            check.setText(f"Raids use every screen  ({found}; outposts wait on the others)")
            check.setEnabled(screens > 1)
        records = profile.get("missions") or {}
        for mission_id, button in self.map_buttons.items():
            open_ = map_unlocked(records, mission_id, admin)
            button.setVisible(open_)
            lock = self.map_locks[mission_id]
            before = MAP_BY_ID[mission_id].unlock_after
            lock.setText("" if open_ else f"Win \u201c{MAP_BY_ID[before].title}\u201d to unlock")
            lock.setVisible(not open_)
            self.map_pictures[mission_id].setPixmap(map_picture(mission_id, MAP_BY_ID[mission_id].kind, not open_))
            card = self.mission_cards[mission_id]
            card.setProperty("locked", not open_)
            card.style().unpolish(card)
            card.style().polish(card)
        for mission_id, label in self.mission_status.items():
            record = mission_record(profile, mission_id)
            if record["victories"]:
                wins = record["victories"]
                best = record["best_seconds"]
                time = f" \u00b7 best {int(best // 60)}:{int(best % 60):02d}" if best is not None else ""
                label.setText(f"\u2714 Won \u00d7{wins}{time}")
                label.setProperty("won", True)
            elif record["defeats"]:
                label.setText("Not won yet")
                label.setProperty("won", False)
            else:
                label.setText("")
                label.setProperty("won", False)
            label.style().unpolish(label)
            label.style().polish(label)
        self._refresh_screens(profile)
        self._refresh_custom_maps(profile)

    def _refresh_screens(self, profile) -> None:
        box = getattr(self, "screen_box", None)
        if box is None:
            return
        from .screen_activity import monitor_states

        while box.count():
            item = box.takeAt(0)
            if item.widget() is not None:
                item.widget().deleteLater()
        self.screen_checks = {}
        screens = QGuiApplication.screens()
        primary = QGuiApplication.primaryScreen()
        if len(screens) < 2:
            return
        states = monitor_states()
        disabled = set(profile.get("disabled_screens") or [])
        for number, screen in enumerate(screens, start=1):
            if screen is primary:
                continue
            g = screen.geometry()
            state = states.get(screen.name(), "active")
            note = {"asleep": " - asleep, skipped", "off": " - off, skipped"}.get(state, "")
            check = QCheckBox(f"Use screen {number} ({g.width()}\u00d7{g.height()}) in missions{note}")
            check.setChecked(screen.name() not in disabled)
            check.setEnabled(state == "active")
            check.setToolTip("Switch off a screen that shows another computer; asleep or off "
                             "screens are skipped by themselves.")
            check.toggled.connect(lambda on, name=screen.name(): self._set_screen(name, on))
            box.addWidget(check, alignment=Qt.AlignHCenter)
            self.screen_checks[screen.name()] = check

    def _set_screen(self, name: str, on: bool) -> None:
        profile = load_profile()
        disabled = [n for n in (profile.get("disabled_screens") or []) if n != name]
        if not on:
            disabled.append(name)
        profile["disabled_screens"] = disabled
        save_profile(profile)

    def _refresh_custom_maps(self, profile) -> None:
        grid = getattr(self, "custom_grid", None)
        if grid is None:
            return
        from . import custom_maps

        while grid.count():
            item = grid.takeAt(0)
            if item.widget() is not None:
                item.widget().deleteLater()
        self.custom_cards = {}
        maps = custom_maps.list_maps()
        self.custom_heading.setVisible(bool(maps))
        for index, data in enumerate(maps):
            card = QFrame()
            card.setObjectName("missionCard")
            card.setMinimumWidth(220)
            card.setMaximumWidth(240)
            cell = QVBoxLayout(card)
            cell.setContentsMargins(12, 10, 12, 10)
            picture = QLabel()
            picture.setObjectName("missionPicture")
            picture.setPixmap(map_picture(data["id"], data.get("kind", "raid")))
            cell.addWidget(picture, alignment=Qt.AlignHCenter)
            name = QLabel(data["title"])
            name.setObjectName("missionTitle")
            name.setWordWrap(True)
            cell.addWidget(name)
            text = QLabel(data["blurb"] or f"Tier {data['tier']}, {len(data['buildings'])} buildings")
            text.setObjectName("missionText")
            text.setWordWrap(True)
            cell.addWidget(text, 1)
            record = mission_record(profile, data["id"])
            if record["victories"]:
                won = QLabel(f"\u2714 Won \u00d7{record['victories']}")
                won.setObjectName("missionDone")
                cell.addWidget(won)
            row = QHBoxLayout()
            play = QPushButton("Play")
            play.setObjectName("modeChoice")
            play.setEnabled(not custom_maps.problems(data))
            play.clicked.connect(lambda _=False, mid=data["id"]: self.play_map(mid, self._start_adventure))
            edit = QPushButton("Edit")
            edit.setObjectName("modeBack")
            edit.clicked.connect(lambda _=False, mid=data["id"]: self.open_editor(mid))
            row.addWidget(play)
            row.addWidget(edit)
            cell.addLayout(row)
            grid.addWidget(card, index // MISSION_COLUMNS, index % MISSION_COLUMNS)
            self.custom_cards[data["id"]] = card

    def open_editor(self, map_id=None) -> None:
        from .map_editor import MapEditor

        editor = MapEditor(self, play=lambda mid: self.play_map(mid, self._start_adventure), map_id=map_id)
        editor.exec_()
        self.refresh_adventure()

    def open_admin(self) -> None:
        from .admin_ui import AdminDialog

        AdminDialog(self).exec_()
        self.refresh_adventure()

    def _set_all_screens(self, on: bool) -> None:
        profile = load_profile()
        profile["all_screens"] = bool(on)
        save_profile(profile)

    def play_map(self, map_id: str, start_adventure) -> None:
        """Remember the chosen map for the overlay, then launch it."""
        profile = load_profile()
        if not map_unlocked(profile.get("missions") or {}, map_id, bool(profile.get("admin", False))):
            return
        profile["selected_map"] = map_id
        save_profile(profile)
        start_adventure()

    def open_character(self) -> None:
        from .character_ui import CharacterDialog

        CharacterDialog(self).exec_()
        self.refresh_adventure()

    def refresh_controls_summary(self) -> None:
        """One line of the essentials, from the saved bindings."""
        settings = load_controls()
        keys = [settings.binding(a) for a in ("move_up", "move_left", "move_down", "move_right")]
        walk = "".join(keys) if all(len(k) == 1 for k in keys) else "/".join(keys)
        walking = "walk and turn" if settings.turn_movement else "walk"
        parts = [f"<b>{walk}</b> {walking}"]
        for action in ("sprint", "jump", "shoot", "bite", "pause"):
            label, _ = settings.action_text(action)
            parts.append(f"<b>{settings.binding(action)}</b> {label.lower()}")
        cone = "free aim" if settings.aim_cone >= 360 else f"mouse aims in a {settings.aim_cone}° cone"
        self.controls_line.setText("  ·  ".join(parts) + "  ·  " + cone)

    def open_controls(self) -> None:
        from .controls_ui import ControlsDialog

        dialog = ControlsDialog(self)
        dialog.exec_()
        self.refresh_controls_summary()

    def _strategy_page(self):
        panel = self._plaque("strategy", (
            "Build a base, direct squads and take rival territory.",
            "Colony command is being developed. Coming later.",
        ))
        return self._page("Strategy", "Colony command", panel)
