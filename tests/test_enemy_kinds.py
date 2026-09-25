"""Enemy spawn contract and offscreen visual evidence. Never operates the mouse."""
from copy import deepcopy
import os
from pathlib import Path
import random

import pytest
from PyQt5.QtCore import Qt
from PyQt5.QtGui import QColor, QFont, QImage, QPainter

from desktop_bug.content.body_plans import body_plan_legs
from desktop_bug.content.discovery import discover_models
from desktop_bug.content.enemy_kinds import ENEMY_KINDS, kinds_for_tier, pick_skin
from desktop_bug.content.palettes import PALETTE_KEYS
from desktop_bug.manager import CreatureManager
from desktop_bug.state.progression import ARMOR_CATALOG


@pytest.fixture
def manager(qapp, state_dir):
    return CreatureManager(Path("presets/tarantula.json"), 1200, 900, seed=41)


def spawn(manager, kind, skin):
    models, warnings = discover_models()
    assert not warnings
    creature = manager._create_creature(models[kind.model_id], manager.creatures[0].personality,
                                       99, pos=(600, 450), color_overrides=deepcopy(skin))
    creature.set_size_scale(kind.size_scale)
    creature.heading = 0
    creature._initialize_legs()
    for attr in ("force_show_name", "force_show_level", "force_show_health", "force_show_xp",
                 "force_show_stamina"):
        setattr(creature, attr, False)
    return creature


def render(creature, zoom=1.5):
    image = QImage(420, 320, QImage.Format_ARGB32_Premultiplied)
    image.fill(Qt.transparent)
    painter = QPainter(image)
    painter.setRenderHint(QPainter.Antialiasing)
    painter.translate(210, 160)
    painter.scale(zoom, zoom)
    painter.translate(-creature.x, -creature.y)
    creature.render(painter)
    painter.end()
    return image


def image_bytes(image):
    return image.constBits().asstring(image.sizeInBytes())


def test_catalogue_is_pure_data_and_has_deterministic_isolated_skins():
    assert len([k for k in ENEMY_KINDS.values() if not k.boss]) >= 8
    assert len(kinds_for_tier(3, boss=True)) >= 3
    assert not kinds_for_tier(0)
    assert all(k.tier <= 2 and not k.boss for k in kinds_for_tier(2))
    state = random.getstate()
    for kind in ENEMY_KINDS.values():
        assert len(kind.skins) >= 3
        for skin in kind.skins:
            assert set(PALETTE_KEYS) <= skin.keys()
            assert all(len(rgb) == 3 and all(isinstance(v, int) and 0 <= v <= 255 for v in rgb)
                       for rgb in skin.values())
        first = pick_skin(kind, 932)
        assert first == pick_skin(kind, 932)
        first["body"][0] = -1
        assert pick_skin(kind, 932)["body"][0] >= 0
        assert len({repr(pick_skin(kind, seed)) for seed in range(60)}) == len(kind.skins)
    assert random.getstate() == state


@pytest.mark.parametrize("kind", ENEMY_KINDS.values(), ids=ENEMY_KINDS)
def test_every_skin_spawns_renders_and_wears_each_legendary_piece(manager, kind, tmp_path):
    pictures = []
    sun = [item for item in ARMOR_CATALOG if item.id.startswith("sun_")]
    assert len(sun) == 5
    for index, skin in enumerate(kind.skins):
        creature = spawn(manager, kind, skin)
        assert len(creature.legs) == 8
        bare = render(creature)
        assert bare.save(str(tmp_path / f"{kind.id}-{index}.png"))
        pictures.append(image_bytes(bare))
        assert any(bare.pixelColor(x, y).alpha() for x in range(170, 250) for y in range(130, 190))
        # Transparent edges catch clipping of long legs, crowns and spines.
        assert all(bare.pixelColor(x, y).alpha() == 0
                   for x in range(bare.width()) for y in (0, bare.height() - 1))
        assert all(bare.pixelColor(x, y).alpha() == 0
                   for y in range(bare.height()) for x in (0, bare.width() - 1))
        for item in sun:
            creature.progression.equipped.clear()
            creature.progression.add_item(item.id)
            assert creature.progression.equip(item.id)
            assert image_bytes(render(creature)) != pictures[-1], (kind.id, item.id)
        for item in sun:
            creature.progression.equip(item.id)
        assert image_bytes(render(creature)) != pictures[-1]
    assert len(set(pictures)) == len(kind.skins)
    # Exercise each rig while turning and moving; no real cursor events.
    manager.creatures = [creature]
    manager.set_flies_enabled(False)
    for _ in range(90):
        manager.update(1 / 60, -100000, -100000)
    assert not render(creature).isNull()


def test_custom_rigs_are_independent_and_distinct():
    crab, orb = body_plan_legs("enemy_crab"), body_plan_legs("enemy_orb")
    assert crab != orb
    assert crab[0]["reach"] > crab[-1]["reach"] * 1.8
    crab[0]["reach"] = 100
    assert body_plan_legs("enemy_crab")[0]["reach"] < 3


def test_render_contact_sheet(manager, tmp_path):
    # Set ENEMY_PREVIEW_DIR to preserve the evidence outside pytest's temp dir.
    folder = Path(os.environ.get("ENEMY_PREVIEW_DIR", str(tmp_path)))
    folder.mkdir(parents=True, exist_ok=True)
    sheet = QImage(1260, 370 * len(ENEMY_KINDS), QImage.Format_RGB32)
    sheet.fill(QColor(43, 49, 55))
    painter = QPainter(sheet)
    painter.setFont(QFont("Arial", 12))
    for row, kind in enumerate(ENEMY_KINDS.values()):
        for column, skin in enumerate(kind.skins):
            creature = spawn(manager, kind, skin)
            image = render(creature)
            assert image.save(str(folder / f"{kind.id}-{column}.png"))
            x, y = column * 420, row * 370
            painter.drawImage(x, y + 32, image)
            painter.setPen(QColor(235, 235, 225))
            painter.drawText(x + 12, y + 24, f"{kind.name} / {column + 1}" + (" / BOSS" if kind.boss else ""))
            if kind.boss:
                for item in ARMOR_CATALOG:
                    if item.id.startswith("sun_"):
                        creature.progression.add_item(item.id)
                        creature.progression.equip(item.id)
                assert render(creature).save(str(folder / f"{kind.id}-{column}-armoured.png"))
    painter.end()
    assert sheet.save(str(folder / "contact-sheet.png"))
