"""Shipped presets must not silently strip a spider's abilities.

A slot that never customised its abilities is personality-driven: it keeps the
common ``web_walk`` plus whatever specialty its personality grants, and adds
whatever its job requires.  Only an explicit (possibly empty) ``abilities`` list
in the preset overrides that.
"""

import json
import os
import random
import sys
import tempfile
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
# Keep a test run from rewriting a real player's saved spiders.
os.environ.setdefault("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="desktop-bug-test-"))
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from PyQt5.QtWidgets import QApplication

from desktop_bug.manager import CreatureManager
from desktop_bug.skills import (
    ABILITY_SKILL_IDS,
    COMMON_ABILITY_IDS,
    default_ability_ids,
    skills_with_default_abilities,
    skills_with_selected_abilities,
)
from desktop_bug.discovery import discover_personalities


def abilities_of(creature):
    return [skill_id for skill_id in creature.skill_ids() if skill_id in ABILITY_SKILL_IDS]


def main() -> int:
    random.seed(11)
    QApplication.instance() or QApplication([])
    personalities, warnings = discover_personalities(ROOT)
    assert not warnings, warnings

    # Unit level: "no choice made" keeps the personality's own abilities, while
    # an explicit empty selection still means none.
    assert default_ability_ids(personalities["webber"]) == ["weave_web", "web_walk"]
    assert set(skills_with_default_abilities(personalities["trapper"])) >= {
        "web_walk", "shoot_web", "wall_web",
    }
    assert "drift" in skills_with_default_abilities(personalities["drifter"])
    assert not set(ABILITY_SKILL_IDS).intersection(
        skills_with_selected_abilities(personalities["webber"], [])
    )
    # A job adds its required ability on top of the personality's own.
    webber_job = skills_with_default_abilities(personalities["curious"], ("weave_web",))
    assert {"weave_web", "web_walk"}.issubset(webber_job), webber_job

    # End to end: every shipped preset launches spiders that can still act.
    checked = 0
    for preset_path in sorted((ROOT / "presets").glob("*.json")):
        preset = json.loads(preset_path.read_text(encoding="utf-8"))
        explicit = {
            index for index, slot in enumerate(preset.get("slots", []))
            if slot.get("abilities") is not None or slot.get("skills") is not None
        }
        manager = CreatureManager(preset_path, 1280, 720)
        assert manager.creatures, preset_path
        for creature in manager.creatures:
            checked += 1
            if explicit:
                # Mixed presets: only assert on the personality-driven spiders.
                continue
            abilities = abilities_of(creature)
            for common in COMMON_ABILITY_IDS:
                assert common in abilities, (preset_path, creature.personality["id"], abilities)
            for expected in default_ability_ids(creature.personality):
                assert expected in abilities, (preset_path, creature.personality["id"], abilities)

    print(f"preset-abilities-smoke: presets ok, spiders checked={checked}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
