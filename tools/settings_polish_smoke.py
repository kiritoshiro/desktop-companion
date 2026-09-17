"""The slot table must read at a glance, and stay usable without the icons.

Two controls in every row said what they were in words: a button reading
"Colors" and one reading "Remove". In a table that already carries a model, a
temperament, a count, abilities, a team and a job, those words are the least
informative thing in the row -- the colour button can simply show the colours,
and the remove button is a cross everywhere else in the world.

Replacing text with a picture is only an improvement if the control stays
reachable for someone who cannot see it, so every check here comes in pairs:
the icon is really drawn, *and* the accessible name and tooltip survived.
"""

import os
import random
import sys
import tempfile
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("DESKTOP_BUG_STATE_DIR", tempfile.mkdtemp(prefix="desktop-bug-test-"))
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from PyQt5.QtCore import QSize  # noqa: E402
from PyQt5.QtWidgets import QApplication  # noqa: E402

from desktop_bug.config_ui import ConfigWindow  # noqa: E402

COLORS_COLUMN = 5
REMOVE_COLUMN = 8


def icon_colors(button, size: int = 34) -> list:
    """Every distinct colour actually painted into a button's icon."""
    image = button.icon().pixmap(QSize(size, size)).toImage()
    found = []
    for y in range(2, image.height() - 2):
        for x in range(2, image.width() - 2):
            pixel = image.pixelColor(x, y)
            if pixel.alpha() < 200:
                continue
            rgb = (pixel.red(), pixel.green(), pixel.blue())
            if rgb not in found:
                found.append(rgb)
    return found


def nearest(colors, target) -> float:
    return min((sum((a - b) ** 2 for a, b in zip(rgb, target)) ** 0.5 for rgb in colors),
               default=float("inf"))


def build_window():
    app = QApplication.instance() or QApplication(sys.argv[:1])
    assert app is not None
    random.seed(9)
    window = ConfigWindow()
    if window.table.rowCount() == 0:
        window.add_slot()
    return app, window


def check_color_swatch_shows_the_slot_palette(window) -> None:
    button = window.table.cellWidget(0, COLORS_COLUMN)
    assert button is not None, "the slot table has no colour control"
    assert button.text() == "", f"the colour button still spells itself out: {button.text()!r}"
    assert not button.icon().isNull(), "the colour button has no swatch"

    # Nothing overridden: the swatch shows the model's own palette, because that
    # is what this slot will actually produce.
    model_box = window.table.cellWidget(0, 0)
    defaults = window._model_color_defaults(model_box)
    painted = icon_colors(button)
    if defaults:
        body = defaults.get("body") or list(defaults.values())[0]
        assert nearest(painted, tuple(body)) < 30.0, (
            f"the swatch does not show the model's palette: wanted {body}, painted {painted}"
        )

    # A custom palette is shown as itself, and says so where it can be read.
    button.setProperty("color_overrides", {"body": [255, 0, 255], "legs": [0, 255, 0]})
    window._refresh_colors_button(button, model_box)
    painted = icon_colors(button)
    assert nearest(painted, (255, 0, 255)) < 20.0, f"the custom body colour is missing: {painted}"
    assert nearest(painted, (0, 255, 0)) < 20.0, f"the custom leg colour is missing: {painted}"
    assert "custom" in button.accessibleName().lower(), button.accessibleName()
    assert "body" in button.toolTip() and "legs" in button.toolTip(), button.toolTip()

    # And back again, so the swatch tracks the slot rather than latching.
    button.setProperty("color_overrides", {})
    window._refresh_colors_button(button, model_box)
    painted = icon_colors(button)
    assert nearest(painted, (255, 0, 255)) > 40.0, (
        f"the swatch kept a palette that was cleared: {painted}"
    )
    assert "default" in button.accessibleName().lower(), button.accessibleName()


def check_controls_stay_reachable_without_sight(window) -> None:
    """An icon-only control with no name is unusable, not minimal."""
    for column, expected in ((COLORS_COLUMN, "color"), (REMOVE_COLUMN, "remove")):
        button = window.table.cellWidget(0, column)
        assert button is not None, f"column {column} has no control"
        assert button.text() == "", f"column {column} still carries a text label"
        assert button.accessibleName().strip(), f"column {column} has no accessible name"
        assert expected in button.accessibleName().lower(), button.accessibleName()
        assert button.toolTip().strip(), f"column {column} has no tooltip"
        assert not button.icon().isNull(), f"column {column} has no icon"


def check_remove_icon_is_drawn_and_works(window) -> None:
    before = window.table.rowCount()
    window.add_slot()
    assert window.table.rowCount() == before + 1

    button = window.table.cellWidget(window.table.rowCount() - 1, REMOVE_COLUMN)
    painted = icon_colors(button, size=24)
    assert painted, "the remove button's icon is blank"
    # Drawn in a warning red, so it does not read as just another grey control.
    assert nearest(painted, (196, 72, 72)) < 60.0, f"the remove icon is not the warning colour: {painted}"

    # The picture has to still do the job the word did.
    button.click()
    assert window.table.rowCount() == before, "clicking the remove icon removed nothing"


def check_swatch_follows_the_model(window) -> None:
    """A slot with no overrides shows its model's colours, so changing the model
    has to change the swatch."""
    button = window.table.cellWidget(0, COLORS_COLUMN)
    model_box = window.table.cellWidget(0, 0)
    button.setProperty("color_overrides", {})

    seen = []
    for index in range(model_box.count()):
        model_box.setCurrentIndex(index)
        palette = window._palette_for_button(button, model_box)
        if palette:
            seen.append(tuple(tuple(color) for color in palette))
        if len(seen) >= 2 and seen[-1] != seen[0]:
            break
    assert len(seen) >= 2, "not enough models with a palette to compare"
    assert seen[-1] != seen[0], (
        "the swatch showed the same colours for two different models, so it is "
        "not reading the model's palette"
    )


def main() -> int:
    app, window = build_window()
    try:
        check_color_swatch_shows_the_slot_palette(window)
        check_controls_stay_reachable_without_sight(window)
        check_remove_icon_is_drawn_and_works(window)
        check_swatch_follows_the_model(window)
    finally:
        window.close()
        window.deleteLater()
        app.processEvents()
    print("settings polish smoke: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
