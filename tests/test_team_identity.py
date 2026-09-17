"""A team must be something a person can name, colour, and understand.

`pack_a`, `pack_b`, `hunters`, `rivals` were ids out of a fixed list. They say
nothing about what a group is for, two of them promise a fight the overlay
cannot have, and nothing on screen told two teams apart -- a preset with two
teams looked exactly like a preset with one.

These checks cover all three: the data model behind a named, coloured team; that
the colour actually reaches the desktop, asserted by rendering and looking at the
pixels rather than by trusting the call; and that the interface and the README
say plainly what hostility does, which is alert a Guard, not start a fight.
"""

import json
import math
import random


from desktop_bug import teams
from desktop_bug.manager import CreatureManager
from desktop_bug.progression import normalize_team_stances, team_stance
from support import ROOT
import pytest


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Every check in this module needs the one Qt application object.

    Each of these files used to build its own, and several dropped the only
    reference to it on the same line. In one process per test that was merely
    wasteful; in one process for the whole suite it is an access violation,
    because the next module inherits a pointer to an application that has
    already been collected. `conftest.qapp` owns it now.
    """


def qt_app():
    """The one application object, owned by the `qapp` fixture in conftest.

    It used to be created here, and the two ways of getting that wrong are
    written up in conftest: dropping the only reference on the same line, and
    creating a QGuiApplication that then makes every widget check in the
    process abort.
    """
    from PyQt5.QtWidgets import QApplication

    app = QApplication.instance()
    assert app is not None, "the qapp fixture has not run; nothing owns the application"
    return app


def color_distance(left, right) -> float:
    return math.dist(tuple(left)[:3], tuple(right)[:3])


def test_profiles_from_a_preset_block() -> None:
    profiles = teams.normalize_teams({
        "porch_guard": {"name": "Porch guard", "color": "#3fa9d9"},
        "shed": "Shed crew",                      # a bare name, written by hand
        "neutral": {"name": "Nope"},              # not a team; cannot be named
    })
    assert set(profiles) == {"porch_guard", "shed"}, sorted(profiles)
    assert profiles["porch_guard"].name == "Porch guard"
    assert profiles["porch_guard"].color == (63, 169, 217)
    assert profiles["shed"].name == "Shed crew"
    assert profiles["shed"].color == teams.default_color("shed")

    # A slot can point at a team the block never mentions, including in an older
    # preset with no block at all. It still has to have an identity.
    profiles = teams.normalize_teams(None, ["pack_a", "neutral", "attic_watch"])
    assert set(profiles) == {"pack_a", "attic_watch"}, sorted(profiles)
    assert profiles["pack_a"].name == "Pack A", "a shipped team lost the name it had"
    assert profiles["attic_watch"].name == "Attic Watch"


def test_ids_are_case_folded() -> None:
    """The fifth instance of a trap that has already caused four bugs."""
    profiles = teams.normalize_teams({"Porch Guard": {"name": "Porch guard"}},
                                     ["PORCH_guard", "porch guard"])
    assert list(profiles) == ["porch_guard"], (
        "three spellings of one team became more than one team: " + str(list(profiles))
    )
    assert teams.team_label("RIVALS") == teams.team_label("rivals")
    assert teams.team_color("Rivals") == teams.team_color("rivals")


def test_colors_parse_and_differ() -> None:
    assert teams.parse_color("#3fa9d9") == (63, 169, 217)
    assert teams.parse_color("3fa9d9") == (63, 169, 217)
    assert teams.parse_color("#abc") == (170, 187, 204)
    assert teams.parse_color([10, 20, 30]) == (10, 20, 30)
    assert teams.parse_color("not a colour") is None
    assert teams.parse_color(None) is None
    assert teams.color_to_hex((63, 169, 217)) == "#3fa9d9"

    # Two teams in one scene must not look the same, including two a person
    # invents. A hash alone cannot promise that: `porch_guard` and `shed_crew`
    # landed on exactly the same colour, so the colours are assigned as a set
    # and nudged apart.
    invented = ["attic_watch", "porch_guard", "shed_crew", "garden_patrol", "roof",
                "hearth", "wanderers", "night_shift"]
    profiles = teams.normalize_teams(None, invented)
    for index, left in enumerate(invented):
        for right in invented[index + 1:]:
            gap = color_distance(profiles[left].color, profiles[right].color)
            assert gap >= teams.MIN_COLOR_SEPARATION, f"{left} and {right} look alike: {gap:.0f}"

    # Stable between runs, because it ends up written into a preset, and not
    # dependent on the order the teams happened to be listed in.
    assert teams.normalize_teams(None, invented)["roof"].color == profiles["roof"].color
    assert teams.normalize_teams(None, list(reversed(invented)))["roof"].color         == profiles["roof"].color

    # A colour a person chose is never moved, even when it sits on top of
    # another team's.
    chosen = teams.normalize_teams(
        {"a_team": {"color": "#3fa9d9"}, "b_team": {"color": "#3fa9d9"}})
    assert chosen["a_team"].color == chosen["b_team"].color == (63, 169, 217)


def test_shipped_colony_shows_two_teams() -> None:
    random.seed(3)
    manager = CreatureManager(ROOT / "presets" / "colony.json", 1600, 900)
    profiles = manager.team_profiles
    assert set(profiles) == {"pack_a", "rivals"}, sorted(profiles)
    names = {profile.name for profile in profiles.values()}
    assert names == {"Home colony", "Intruders"}, names
    assert not any(name.lower().startswith("pack_") for name in names), names
    gap = color_distance(profiles["pack_a"].color, profiles["rivals"].color)
    assert gap > 80.0, f"the two Colony teams are not visibly distinct: {gap:.0f}"

    # Every spider can reach the same table, so a rename reaches the scene.
    for creature in manager.creatures:
        assert creature.team_profiles is manager.team_profiles

    renamed = manager.set_team_profiles({"pack_a": {"name": "Hearth"}}, ["pack_a", "rivals"])
    assert "2" in renamed, renamed
    assert manager.team_profiles["pack_a"].name == "Hearth"
    for creature in manager.creatures:
        assert creature.team_profiles is manager.team_profiles, "a live rename missed a spider"


def test_team_color_reaches_the_screen() -> None:
    """Render a spider and look at the pixels, rather than trusting the call."""
    from PyQt5.QtGui import QColor, QImage, QPainter

    from desktop_bug.creature import Creature

    qt_app()
    model = json.loads((ROOT / "models" / "tarantula" / "model.json").read_text(encoding="utf-8"))
    personality = json.loads((ROOT / "personalities" / "mellow.json").read_text(encoding="utf-8"))

    marker = (255, 0, 255)  # nothing in the spider art is this colour

    def paint(team_id: str) -> QImage:
        random.seed(12)
        creature = Creature(model, personality, 400, 400)
        creature.x, creature.y = 200.0, 200.0
        creature._initialize_legs()
        creature.set_team(team_id)
        creature.team_profiles = teams.normalize_teams({team_id: {"color": "#ff00ff"}}) \
            if team_id != "neutral" else {}
        image = QImage(400, 400, QImage.Format_ARGB32_Premultiplied)
        image.fill(QColor(0, 0, 0, 0))
        painter = QPainter(image)
        painter.setRenderHint(QPainter.Antialiasing, True)
        creature.render(painter)
        painter.end()
        return image

    def marker_pixels(image: QImage) -> int:
        """Count marker-coloured pixels in the band where the ring sits.

        Every pixel in that band, not a sampled grid: the ring is a thin stroke
        and a stride of two walked straight past most of it, which made the
        first version of this check fail for the wrong reason.
        """
        found = 0
        for y in range(180, 260):
            for x in range(150, 250):
                pixel = image.pixelColor(x, y)
                if pixel.alpha() > 40 and color_distance(
                        (pixel.red(), pixel.green(), pixel.blue()), marker) < 90:
                    found += 1
        return found

    on_team = marker_pixels(paint("porch_guard"))
    assert on_team > 40, f"a spider on a team wore no sign of it: {on_team} pixels"

    alone = marker_pixels(paint("neutral"))
    assert alone == 0, f"a spider on no team was marked anyway: {alone} pixels"


def test_base_ring_uses_the_team_color() -> None:
    from PyQt5.QtGui import QColor, QImage, QPainter

    from desktop_bug.jobs import MAX_BUILD_PROGRESS, BaseSite, BaseWorld

    qt_app()
    world = BaseWorld(600, 600)
    world.team_profiles = teams.normalize_teams({"porch_guard": {"color": "#ff00ff"}})
    site = BaseSite(id="team:porch_guard", owner_id="test", team_id="porch_guard",
                    x=300.0, y=300.0)
    site.build_progress = MAX_BUILD_PROGRESS
    site.level = 3
    world.bases[site.id] = site

    image = QImage(600, 600, QImage.Format_ARGB32_Premultiplied)
    image.fill(QColor(0, 0, 0, 0))
    painter = QPainter(image)
    painter.setRenderHint(QPainter.Antialiasing, True)
    world.render(painter)
    painter.end()

    found = 0
    for y in range(0, 600, 2):
        for x in range(0, 600, 2):
            pixel = image.pixelColor(x, y)
            if pixel.alpha() > 40 and color_distance(
                    (pixel.red(), pixel.green(), pixel.blue()), (255, 0, 255)) < 110:
                found += 1
    assert found > 12, f"the base ring was not drawn in its team colour: {found} pixels"


def test_stances_round_trip_minimally() -> None:
    declared = normalize_team_stances({"pack_a": {"rivals": "foe"}})
    assert declared == {"pack_a": {"rivals": "foe"}, "rivals": {"pack_a": "foe"}}
    # Saved one way round, so a hand-written preset is not rewritten on first save.
    assert teams.minimal_stances(declared) == {"pack_a": {"rivals": "foe"}}
    # And it survives the trip back out.
    assert normalize_team_stances(teams.minimal_stances(declared)) == declared

    # An explicit "ignore each other" is a decision, not an absence. Dropping it
    # would let the Rivals default quietly restore the hostility next launch.
    peace = normalize_team_stances({"pack_a": {"rivals": "neutral"}})
    assert teams.minimal_stances(peace) == {"pack_a": {"rivals": "neutral"}}
    assert team_stance("pack_a", "rivals", peace) == "neutral", (
        "a declared truce did not beat the Rivals default"
    )
    assert team_stance("pack_a", "rivals", {}) == "foe", "the Rivals default stopped working"


def test_settings_window_round_trips_names_and_colours() -> None:
    from desktop_bug.config_ui import ConfigWindow

    app = qt_app()
    assert app is not None
    window = ConfigWindow()
    try:
        preset = json.loads((ROOT / "presets" / "colony.json").read_text(encoding="utf-8"))
        window.load_preset_data(preset) if hasattr(window, "load_preset_data") else None
        window.apply_settings_to_ui(preset["settings"])
        saved = window.current_settings_data()

        block = saved.get("teams") or {}
        assert block.get("pack_a", {}).get("name") == "Home colony", block
        assert block.get("rivals", {}).get("color") == "#d1534f", block
        assert saved.get("team_relations") == {"pack_a": {"rivals": "foe"}}, saved["team_relations"]

        # A rename and a recolour survive a save.
        window._rename_team("pack_a", "Hearth")
        window._team_profiles["rivals"] = teams.TeamProfile("rivals", "Intruders", (10, 20, 30))
        saved = window.current_settings_data()
        assert saved["teams"]["pack_a"]["name"] == "Hearth", saved["teams"]
        assert saved["teams"]["rivals"]["color"] == "#0a141e", saved["teams"]

        # Reading it back gives exactly what was written.
        window.apply_settings_to_ui(saved)
        assert window._team_profiles["pack_a"].name == "Hearth"
        assert window._team_profiles["rivals"].color == (10, 20, 30)

        # The Teams panel has to actually exist, with a swatch, an editable name
        # and a stance control per pair. Asserting only on the saved data would
        # pass with no panel at all.
        from PyQt5.QtWidgets import QComboBox, QLineEdit, QPushButton

        window.update_summary()
        widgets = [window.teams_layout.itemAt(i).widget()
                   for i in range(window.teams_layout.count())]
        assert any(isinstance(w, QPushButton) for w in widgets), "no colour swatch"
        names = [w for w in widgets if isinstance(w, QLineEdit)]
        assert len(names) >= 2, f"expected an editable name per team, got {len(names)}"
        stances = [w for w in widgets if isinstance(w, QComboBox)]
        assert stances, "no control for what stands between two teams"
        assert [stances[0].itemText(i) for i in range(stances[0].count())] == [
            "Allies", "Ignore each other", "Foes"], "the stance choices read as jargon"
        assert teams.HOSTILITY_NOTE in window.teams_note.text()

        # A team invented from a typed name gets a case-folded, unique id.
        window._team_profiles["porch_guard"] = teams.TeamProfile(
            "porch_guard", "Porch guard", (1, 2, 3))
        assert window._unique_team_id("Porch Guard!") == "porch_guard_2"
        assert window._unique_team_id("  Attic Watch  ") == "attic_watch"
        assert window._unique_team_id("neutral") == "team", "a team cannot be called neutral"
    finally:
        window.close()
        window.deleteLater()


def test_the_interface_says_what_hostility_does() -> None:
    """The one sentence that stops the words over-promising."""
    note = teams.HOSTILITY_NOTE.lower()
    assert "no combat" in note, teams.HOSTILITY_NOTE
    assert "guard" in note and "intercept" in note, teams.HOSTILITY_NOTE
    assert "damage" in note, teams.HOSTILITY_NOTE

    sentence = teams.describe_stance("pack_a", "rivals", "foe")
    assert "damage" in sentence.lower(), sentence
    assert "Pack A" in sentence and "Rivals" in sentence, sentence
    assert teams.describe_stance("a", "b", "friend").endswith("company.")

    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    assert "There is no combat yet" in readme, "the README does not say combat is absent"
    assert "Nothing takes damage" in readme, readme[:0]
    # And it documents the block a person would otherwise have to guess at.
    assert '"teams": {' in readme, "the README does not document the teams block"
