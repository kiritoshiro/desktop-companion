"""Admin mode: the owner's switches for testing Adventure.

The owner: *"add admin mode. so i could also reset my own progress or enter
all the maps too."*

- **Admin mode**: every map is open, whatever has been won.
- **Reset my Adventure progress**: the hero, companions, armoury, amber and
  map results start again (the Companion colony is not touched). Admin mode
  and the screen choices stay as they were.
- Quick gifts for testing: amber, every armour piece, every companion, a
  hero level.

Everything here writes ``adventure-hero.json`` through adventure_profile.
"""
from __future__ import annotations

from PyQt5.QtWidgets import (QCheckBox, QDialog, QHBoxLayout, QLabel, QMessageBox, QPushButton, QSpinBox,
                             QVBoxLayout)

from . import armoury
from .adventure_profile import (fresh_profile, hero_progression, load_profile, save_profile,
                                store_progression, unlock_companion)
from .campaign import COMPANIONS
from ..state.progression import ARMOR_CATALOG, MAX_LEVEL


def reset_progress() -> dict:
    """A new Adventure profile, keeping only the player's settings."""
    old = load_profile()
    profile = fresh_profile()
    for key in ("admin", "all_screens", "disabled_screens"):
        profile[key] = old.get(key, profile[key])
    save_profile(profile)
    return profile


def give_amber(amount: int) -> int:
    profile = load_profile()
    store = profile.setdefault("armoury", {})
    store["amber"] = int(store.get("amber", 0)) + int(amount)
    save_profile(profile)
    return store["amber"]


def give_all_armour() -> int:
    profile = load_profile()
    added = sum(armoury.add_loot(profile, item.id) == "new" for item in ARMOR_CATALOG)
    save_profile(profile)
    return added


def unlock_all_companions() -> int:
    profile = load_profile()
    added = sum(unlock_companion(profile, c.id) for c in COMPANIONS)
    save_profile(profile)
    return added


def set_hero_level(level: int) -> int:
    profile = load_profile()
    state = hero_progression(profile)
    level = max(1, min(MAX_LEVEL, int(level)))
    # Levels gained bring their skill points, as if earned; levels taken away
    # take only points still unspent.
    state.skill_points = max(0, state.skill_points + level - state.level)
    state.level = level
    state.xp = 0
    store_progression(profile, "hero", state)
    save_profile(profile)
    return state.level


def set_admin(on: bool) -> None:
    profile = load_profile()
    profile["admin"] = bool(on)
    save_profile(profile)


class AdminDialog(QDialog):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Admin")
        from .map_editor import CHECKBOXES

        self.setStyleSheet(CHECKBOXES)
        box = QVBoxLayout(self)
        profile = load_profile()
        self.admin = QCheckBox("Admin mode: every map is open")
        self.admin.setChecked(bool(profile.get("admin", False)))
        self.admin.toggled.connect(set_admin)
        box.addWidget(self.admin)
        self.status = QLabel()
        self.status.setWordWrap(True)
        rows = (("Reset my Adventure progress…", self._reset),
                ("Give 1000 amber", lambda: self._say(f"Amber: {give_amber(1000)}")),
                ("Give every armour piece", lambda: self._say(f"{give_all_armour()} new pieces added")),
                ("Unlock every companion", lambda: self._say(f"{unlock_all_companions()} companions joined")))
        self.buttons = {}
        for text, slot in rows:
            button = QPushButton(text)
            button.clicked.connect(lambda _=False, slot=slot: slot())
            box.addWidget(button)
            self.buttons[text] = button
        row = QHBoxLayout()
        self.level = QSpinBox()
        self.level.setRange(1, MAX_LEVEL)
        self.level.setValue(hero_progression(profile).level)
        set_level = QPushButton("Set hero level")
        set_level.clicked.connect(lambda: self._say(f"Hero is level {set_hero_level(self.level.value())}"))
        row.addWidget(self.level)
        row.addWidget(set_level)
        box.addLayout(row)
        box.addWidget(self.status)
        close = QPushButton("Close")
        close.clicked.connect(self.accept)
        box.addWidget(close)

    def _say(self, text):
        self.status.setText(text)

    def _reset(self, confirm=True):
        if confirm and QMessageBox.question(
                self, "Reset progress",
                "Start Adventure again? The hero, companions, armour, amber and map results are "
                "reset. Your Companion colony is not touched.") != QMessageBox.Yes:
            return
        reset_progress()
        self.level.setValue(1)
        self._say("Adventure progress reset.")
