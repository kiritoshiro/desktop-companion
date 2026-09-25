"""The carved-wood look shared by every window (menus, editor, HUD, dialogs).

The pictures and textures are PNGs in ``assets/ui/``, made by
``tools/generate_ui_art.py``; this module only finds them and says how they
are used. If an asset is missing -- a stripped build, a moved checkout --
everything falls back to the plain colours below, so the interface still
works and still reads as wood, just without the grain.
"""

from __future__ import annotations

from functools import lru_cache
from pathlib import Path

from ..content.discovery import find_data_file

# Palette. Light text sits on walnut, dark text on parchment.
WALNUT = "#3f2616"
WALNUT_DEEP = "#2a180c"
OAK = "#a8743f"
OAK_LIGHT = "#c99a5e"
PARCHMENT = "#f4e7cc"
PARCHMENT_DEEP = "#e8d3ab"
INK = "#2e1d10"
INK_SOFT = "#6a4a2c"
CREAM = "#f6e2b8"
CREAM_SOFT = "#dcc39a"
BRASS = "#d6a448"
BRASS_DEEP = "#9c6d22"
HEALTH = "#b8472f"
STAMINA = "#7f9e43"

MODE_ART = {
    "companion": "mode_companion.png",
    "skirmish": "mode_adventure.png",
    "strategy": "mode_strategy.png",
}


@lru_cache(maxsize=None)
def asset_path(name: str) -> Path | None:
    path = find_data_file("assets", "ui", name)
    return path if path.exists() else None


def _url(name: str) -> str | None:
    path = asset_path(name)
    return path.as_posix() if path is not None else None


def _background(name: str, fallback: str) -> str:
    url = _url(name)
    if url is None:
        return f"background: {fallback};"
    return f"background-color: {fallback}; background-image: url({url});"


def _button(selector: str, top: str, bottom: str, text: str, border: str) -> str:
    return f"""
        {selector} {{
            color: {text}; font-weight: 700;
            border: 1px solid {border}; border-radius: 7px;
            border-bottom: 3px solid {border};
            background: qlineargradient(x1:0, y1:0, x2:0, y2:1,
                                        stop:0 {top}, stop:1 {bottom});
            padding: 6px 14px;
        }}
        {selector}:hover {{
            background: qlineargradient(x1:0, y1:0, x2:0, y2:1,
                                        stop:0 {_lift(top)}, stop:1 {_lift(bottom)});
        }}
        {selector}:pressed {{
            border-bottom: 1px solid {border}; padding-top: 8px;
            background: qlineargradient(x1:0, y1:0, x2:0, y2:1,
                                        stop:0 {bottom}, stop:1 {top});
        }}
        {selector}:disabled {{ color: #9c8a70; background: #5a4331; }}
    """


def _lift(color: str, amount: int = 22) -> str:
    color = color.lstrip("#")
    r, g, b = (min(255, int(color[i:i + 2], 16) + amount) for i in (0, 2, 4))
    return f"#{r:02x}{g:02x}{b:02x}"


def wood_button(selector: str = "QPushButton") -> str:
    return _button(selector, OAK_LIGHT, "#8e5d30", "#fff5e0", WALNUT_DEEP)


def brass_button(selector: str) -> str:
    return _button(selector, "#e8bd62", BRASS_DEEP, WALNUT_DEEP, "#5a3a10")


def card_frame_css() -> str:
    url = _url("card_frame.png")
    if url is None:
        return f"background: {WALNUT}; border: 3px solid {WALNUT_DEEP}; border-radius: 16px;"
    return f"border-image: url({url}) 30 30 30 30 stretch stretch; border-width: 30px;"


def scrollbar_qss() -> str:
    """A walnut groove with an oak handle, instead of Qt's hatched default."""
    return f"""
        QScrollBar:vertical {{ background: {WALNUT}; width: 12px; margin: 0;
                               border-radius: 6px; }}
        QScrollBar:horizontal {{ background: {WALNUT}; height: 12px; margin: 0;
                                 border-radius: 6px; }}
        QScrollBar::handle:vertical, QScrollBar::handle:horizontal {{
            background: qlineargradient(x1:0, y1:0, x2:1, y2:1,
                                        stop:0 {OAK_LIGHT}, stop:1 #8e5d30);
            border: 1px solid {WALNUT_DEEP}; border-radius: 5px;
            min-height: 28px; min-width: 28px;
        }}
        QScrollBar::add-line, QScrollBar::sub-line {{ width: 0; height: 0; }}
        QScrollBar::add-page, QScrollBar::sub-page {{ background: transparent; }}
    """


def shell_qss() -> str:
    """The mode menu and the pages around the Companion editor."""
    return f"""
        QWidget#modeShell {{ {_background("wood_tile.png", OAK)} color: {CREAM}; }}
        QFrame#modeCard {{ {card_frame_css()} }}
        QLabel#modeTitle {{ color: {CREAM}; font-size: 17pt; font-weight: 800;
                            background: transparent; }}
        QLabel#modeHeading {{ color: {WALNUT_DEEP}; font-size: 30pt; font-weight: 800;
                              background: transparent; }}
        QLabel#modeSubheading {{ color: {WALNUT}; font-size: 12pt; font-weight: 600;
                                 background: transparent; }}
        QLabel#modeText {{ color: {CREAM_SOFT}; font-size: 10pt; background: transparent; }}
        QLabel#modeArt {{ background: transparent; border: 0; }}
        QFrame#modePlaque {{ {card_frame_css()} }}
        QFrame#pageHeader {{ background: {WALNUT}; border: 2px solid {WALNUT_DEEP};
                             border-radius: 12px; }}
        {brass_button("QPushButton#modeChoice")}
        QPushButton#modeChoice {{ font-size: 11pt; padding: 9px 18px; }}
        QLabel#missionHeading {{ color: {WALNUT_DEEP}; font-size: 14pt; font-weight: 800;
                                 background: transparent; }}
        QFrame#missionCard {{ background: rgba(42, 24, 12, 215); border: 2px solid {BRASS_DEEP};
                              border-radius: 10px; }}
        QFrame#missionCard[locked="true"] {{ background: rgba(42, 24, 12, 130);
                                             border: 2px dashed {INK_SOFT}; }}
        QLabel#missionTitle {{ color: {CREAM}; font-size: 11pt; font-weight: 800;
                               background: transparent; }}
        QLabel#missionText {{ color: {CREAM_SOFT}; font-size: 9pt; background: transparent; }}
        QLabel#missionLocked {{ color: {OAK_LIGHT}; font-size: 9pt; font-style: italic;
                                background: transparent; }}
        QFrame#heroStrip {{ background: rgba(42, 24, 12, 215); border: 2px solid {BRASS};
                            border-radius: 12px; }}
        QLabel#missionDone {{ color: #9fdcbf; font-size: 9pt; font-weight: 800;
                              background: transparent; }}
        QLabel#missionDone[won="false"] {{ color: {OAK_LIGHT}; font-weight: 600; }}
        QLabel#controlsHintLine {{ color: {WALNUT_DEEP}; font-size: 9pt; background: transparent; }}
        {wood_button("QPushButton#modeBack")}
        {scrollbar_qss()}
    """


def dialog_qss() -> str:
    """Pause menu, Adventure settings: a dark walnut board."""
    return f"""
        QDialog {{ {_background("wood_dark_tile.png", WALNUT)} color: {CREAM}; }}
        QLabel {{ color: {CREAM}; background: transparent; }}
        QLabel#dialogTitle {{ font-size: 16pt; font-weight: 800; color: {BRASS}; }}
        {wood_button()}
        QPushButton {{ min-height: 30px; }}
        QComboBox {{ background: {PARCHMENT}; color: {INK}; border: 1px solid {WALNUT_DEEP};
                     border-radius: 6px; padding: 4px 8px; }}
        QComboBox QAbstractItemView {{ background: {PARCHMENT}; color: {INK};
                                       selection-background-color: {OAK}; }}
    """


def companion_qss() -> str:
    """The Companion editor: parchment panels on an oak board."""
    tabs = {
        "presetGroup": "#6b3f1f",
        "creaturesGroup": "#7a4a26",
        "teamsGroup": "#5e3522",
        "behaviorGroup": "#71502b",
        "fliesGroup": "#8a5a22",
        "launchGroup": "#4f5f25",
    }
    group_titles = "\n".join(
        f"QGroupBox#{name}::title {{ background: {color}; }}" for name, color in tabs.items())
    return f"""
        QWidget {{ font-size: 10pt; color: {INK}; }}
        QMainWindow, QMainWindow > QWidget {{ {_background("wood_tile.png", OAK)} }}
        /* The panels live inside a scroll area, so they are not direct
           children of the window and the rule above misses them. */
        QScrollArea {{ background: transparent; border: none; }}
        QScrollArea > QWidget > QWidget {{ background: transparent; }}

        QGroupBox {{
            font-weight: 700;
            border: 2px solid #8a5a2e;
            border-radius: 10px;
            margin-top: 14px;
            padding: 10px 8px 6px 8px;
            background: {PARCHMENT};
        }}
        QGroupBox::title {{
            subcontrol-origin: margin;
            subcontrol-position: top left;
            left: 12px;
            padding: 3px 10px;
            border-radius: 6px;
            border: 1px solid {WALNUT_DEEP};
            color: {CREAM};
        }}
        {group_titles}
        QPushButton#removeSlotButton {{ padding: 0px; }}
        QLabel#teamsNote {{ color: {INK_SOFT}; }}

        QLabel#pageTitle {{
            font-size: 17pt; font-weight: 800; color: {WALNUT_DEEP};
            padding: 2px;
        }}
        QLabel#hintLabel {{ color: {INK_SOFT}; }}
        QLabel#summaryLabel, QLabel#statusLabel {{
            color: {INK};
            background: {PARCHMENT_DEEP};
            border: 1px solid #b08658;
            border-radius: 6px;
            padding: 8px;
        }}

        {wood_button()}
        QPushButton {{ padding: 5px 11px; font-weight: 600; }}
        {brass_button("QPushButton#primaryButton")}
        QPushButton#primaryButton {{ padding: 7px 16px; }}
        QPushButton#stopButton:hover {{ background: #8c3a24; color: #fff0e0; }}

        QComboBox, QSpinBox, QDoubleSpinBox, QLineEdit {{
            border: 1px solid #b08658;
            border-radius: 6px;
            padding: 3px 6px;
            background: #fffaf0;
            color: {INK};
        }}
        QComboBox:focus, QSpinBox:focus, QDoubleSpinBox:focus, QLineEdit:focus {{
            border-color: {WALNUT};
        }}
        QComboBox QAbstractItemView {{
            border: 1px solid #8a5a2e;
            background: #fffaf0;
            selection-background-color: {OAK};
            selection-color: #fff5e0;
            outline: none;
        }}

        QCheckBox {{ spacing: 6px; }}
        QCheckBox#fliesToggle {{ font-weight: 700; color: #8a5a22; }}

        QTableWidget {{
            border: 1px solid #b08658;
            border-radius: 8px;
            background: #fffaf0;
            gridline-color: #ead9b8;
            selection-background-color: #e4c690;
            selection-color: {INK};
        }}
        QTableWidget::item {{ padding: 2px; }}
        QTableWidget::item:alternate {{ background: #faf1de; }}
        QHeaderView::section {{
            background: {PARCHMENT_DEEP};
            color: #4a2c14;
            font-weight: 700;
            border: none;
            border-right: 1px solid #d3b98d;
            padding: 6px 6px;
        }}

        {scrollbar_qss()}
        QStatusBar {{ background: {WALNUT}; color: {CREAM}; }}
        QStatusBar QLabel {{ color: {CREAM}; }}
    """


def app_qss() -> str:
    """Every other window of the app: inspector, skill tree, inventory, menus.

    Set on the whole application, so it is scoped to dialogs, menus and
    tooltips and never matches a bare QWidget: the overlay is a QWidget
    covering every monitor, and a background rule reaching it would paint
    over the desktop. Windows with their own sheet (settings, pause menu)
    keep it, since a widget's own sheet wins over the application's.
    """
    d = "QDialog"
    return f"""
        {d} {{ {_background("wood_dark_tile.png", WALNUT)} color: {CREAM}; }}
        {d} QLabel, {d} QCheckBox, {d} QRadioButton {{ color: {CREAM}; background: transparent; }}
        {d} QLabel#teamNote {{ color: {CREAM_SOFT}; }}
        {d} QTabWidget::pane {{
            border: 2px solid {WALNUT_DEEP}; border-radius: 8px; top: -2px;
            background: rgba(20, 10, 3, 110);
        }}
        {d} QTabWidget > QStackedWidget > QWidget {{ background: transparent; }}
        {d} QTabBar::tab {{
            /* Not bold: a tab is sized for its text before the style sheet's
               weight applies, and bold names came out clipped. */
            color: #fff5e0; padding: 5px 12px; margin-right: 3px;
            border: 1px solid {WALNUT_DEEP}; border-bottom: none;
            border-top-left-radius: 7px; border-top-right-radius: 7px;
            background: qlineargradient(x1:0, y1:0, x2:0, y2:1,
                                        stop:0 {OAK_LIGHT}, stop:1 #8e5d30);
        }}
        {d} QTabBar::tab:selected {{
            color: {WALNUT_DEEP};
            background: qlineargradient(x1:0, y1:0, x2:0, y2:1,
                                        stop:0 #e8bd62, stop:1 {BRASS_DEEP});
        }}
        {d} QTabBar::tab:!selected {{ margin-top: 3px; }}
        {wood_button(d + " QPushButton")}
        {d} QPushButton {{ padding: 5px 12px; }}
        {d} QComboBox, {d} QLineEdit, {d} QSpinBox, {d} QDoubleSpinBox, {d} QTextEdit {{
            background: {PARCHMENT}; color: {INK}; border: 1px solid {WALNUT_DEEP};
            border-radius: 6px; padding: 3px 6px;
        }}
        {d} QComboBox QAbstractItemView {{
            background: {PARCHMENT}; color: {INK};
            selection-background-color: {OAK}; selection-color: #fff5e0;
        }}
        {d} QProgressBar {{
            background: rgba(14, 6, 1, 170); color: {CREAM}; font-weight: 700;
            border: 1px solid {WALNUT_DEEP}; border-radius: 6px; text-align: center;
            min-height: 16px;
        }}
        {d} QProgressBar::chunk {{
            border-radius: 5px;
            background: qlineargradient(x1:0, y1:0, x2:0, y2:1,
                                        stop:0 #e8bd62, stop:1 {BRASS_DEEP});
        }}
        {d} QGroupBox {{ color: {BRASS}; font-weight: 700; border: 1px solid {WALNUT_DEEP};
                         border-radius: 8px; margin-top: 12px; padding-top: 8px; }}
        {d} QGroupBox::title {{ subcontrol-origin: margin; left: 10px; padding: 0 4px; }}
        QMenu {{
            {_background("wood_dark_tile.png", WALNUT)}
            color: {CREAM}; border: 2px solid {WALNUT_DEEP}; border-radius: 6px; padding: 5px;
        }}
        QMenu::item {{ padding: 5px 22px 5px 22px; border-radius: 4px; background: transparent; }}
        QMenu::item:selected {{ background: {OAK}; color: #fff5e0; }}
        QMenu::item:disabled {{ color: #9c8a70; }}
        QMenu::separator {{ height: 1px; background: {WALNUT_DEEP}; margin: 4px 8px; }}
        QMenu::indicator {{ width: 14px; height: 14px; left: 4px; }}
        QToolTip {{ background: {PARCHMENT}; color: {INK}; border: 1px solid {WALNUT_DEEP};
                    padding: 4px; }}
        {scrollbar_qss()}
    """


def app_icon():
    """The app's own icon (a tarantula on a walnut medallion), or None."""
    from PyQt5.QtGui import QIcon

    for name in ("app_icon.ico", "app_icon.png"):
        path = asset_path(name)
        if path is not None:
            icon = QIcon(str(path))
            if not icon.isNull():
                return icon
    return None


def apply_app_theme(app) -> None:
    """Dress every dialog and menu of this process in wood, and give it its icon."""
    app.setStyleSheet(app_qss())
    icon = app_icon()
    if icon is not None:
        app.setWindowIcon(icon)
    import sys

    if sys.platform.startswith("win"):
        # Run from source, the process is python.exe and the taskbar shows
        # Python's icon; its own app id makes Windows use the window's icon.
        try:
            import ctypes

            ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(
                "DesktopBugCompanion.App")
        except (AttributeError, OSError):
            pass


@lru_cache(maxsize=None)
def texture(name: str):
    """A QPixmap of a texture, or None; for code that paints its own panels."""
    from PyQt5.QtGui import QPixmap

    path = asset_path(name)
    if path is None:
        return None
    pixmap = QPixmap(str(path))
    return None if pixmap.isNull() else pixmap


def mode_art(kind: str):
    name = MODE_ART.get(kind)
    return texture(name) if name else None
