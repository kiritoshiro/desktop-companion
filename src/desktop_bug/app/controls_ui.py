"""The Adventure controls editor: aim cone, every button, and rebinding.

Shown from the Adventure page of the settings window and from the in-game
Settings (pause menu). Every change is saved to ``controls.json`` at once,
and a running overlay picks the file up, so there is no separate Apply.
"""

from __future__ import annotations

from PyQt5.QtCore import Qt, pyqtSignal
from PyQt5.QtWidgets import (QCheckBox, QComboBox, QDialog, QGridLayout, QHBoxLayout, QLabel,
                             QPushButton, QVBoxLayout, QWidget)

from . import wood_theme
from .controls import (ACTIONS, AIM_CONES, ControlSettings, key_name, load_controls, mouse_name,
                       save_controls)

CONE_LABELS = {60: "60°  narrow", 90: "90°  (recommended)", 120: "120°  wide",
               180: "180°  half circle", 360: "360°  free aim"}


class BindingButton(QPushButton):
    """Click it, then press a key or a mouse button to bind it. Esc cancels."""

    captured = pyqtSignal(str, str)          # action, binding name

    def __init__(self, action: str, name: str, parent=None):
        super().__init__(name, parent)
        self.action = action
        self.name = name
        self.listening = False
        self.setObjectName("bindingButton")
        self.setMinimumWidth(130)
        self.setFocusPolicy(Qt.StrongFocus)
        self.setToolTip("Click, then press a key or mouse button. Esc cancels.")
        self.clicked.connect(self._listen)

    def set_name(self, name: str) -> None:
        self.name = name
        self.listening = False
        self.setText(name)

    def _listen(self) -> None:
        if self.listening:
            return
        self.listening = True
        self.setText("Press a key or click…")
        self.setFocus()

    def cancel(self) -> None:
        self.set_name(self.name)

    def keyPressEvent(self, event):  # noqa: N802 - Qt API name
        if not self.listening:
            super().keyPressEvent(event)
            return
        event.accept()
        if event.isAutoRepeat():
            return
        if event.key() == Qt.Key_Escape:
            self.cancel()
            return
        self.listening = False
        self.captured.emit(self.action, key_name(event.key()))

    def mousePressEvent(self, event):  # noqa: N802 - Qt API name
        if not self.listening:
            super().mousePressEvent(event)
            return
        name = mouse_name(int(event.button()))
        event.accept()
        if name is not None:
            self.listening = False
            self.captured.emit(self.action, name)

    def focusOutEvent(self, event):  # noqa: N802 - Qt API name
        if self.listening:
            self.cancel()
        super().focusOutEvent(event)


class ControlsEditor(QWidget):
    """Aim cone, facing option, and one rebindable row per action."""

    changed = pyqtSignal()

    def __init__(self, settings: ControlSettings | None = None, path=None, parent=None):
        super().__init__(parent)
        self.path = path
        self.settings = settings if settings is not None else load_controls(path)
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(10)

        aim_row = QHBoxLayout()
        aim_row.addWidget(QLabel("Aim cone:"))
        self.cone = QComboBox()
        for degrees in AIM_CONES:
            self.cone.addItem(CONE_LABELS[degrees], degrees)
        self.cone.setToolTip(
            "How far from where your spider is looking you can aim. The mouse only "
            "aims; it never moves the spider.")
        aim_row.addWidget(self.cone)
        aim_row.addStretch(1)
        layout.addLayout(aim_row)
        self.face_mouse = QCheckBox("Turn to face the mouse when standing still")
        self.face_mouse.setToolTip(
            "Off: the spider keeps facing the way it last walked. On: standing "
            "still, it turns towards the pointer.")
        layout.addWidget(self.face_mouse)
        hint = QLabel("The mouse aims inside the cone in front of your spider; walk to turn. "
                      "Click a button below, then press the new key or mouse button. A button "
                      "already in use swaps places. Esc always pauses.")
        hint.setObjectName("controlsHint")
        hint.setWordWrap(True)
        layout.addWidget(hint)

        grid = QGridLayout()
        grid.setHorizontalSpacing(12)
        grid.setVerticalSpacing(6)
        self.buttons: dict[str, BindingButton] = {}
        for row, (action, label, text) in enumerate(ACTIONS):
            name_label = QLabel(f"<b>{label}</b>")
            grid.addWidget(name_label, row, 0)
            button = BindingButton(action, self.settings.binding(action))
            button.captured.connect(self._rebind)
            grid.addWidget(button, row, 1)
            what = QLabel(text)
            what.setWordWrap(True)
            what.setObjectName("controlsHint")
            grid.addWidget(what, row, 2)
            self.buttons[action] = button
        grid.setColumnStretch(2, 1)
        layout.addLayout(grid)

        reset = QPushButton("Reset to defaults")
        reset.clicked.connect(self.reset)
        row = QHBoxLayout()
        row.addStretch(1)
        row.addWidget(reset)
        layout.addLayout(row)
        self.reset_button = reset

        self._sync()
        self.cone.currentIndexChanged.connect(self._cone_changed)
        self.face_mouse.toggled.connect(self._face_changed)

    def _sync(self) -> None:
        self.cone.blockSignals(True)
        index = self.cone.findData(int(self.settings.aim_cone))
        self.cone.setCurrentIndex(index if index >= 0 else self.cone.findData(90))
        self.cone.blockSignals(False)
        self.face_mouse.blockSignals(True)
        self.face_mouse.setChecked(self.settings.face_mouse_when_still)
        self.face_mouse.blockSignals(False)
        for action, button in self.buttons.items():
            button.set_name(self.settings.binding(action))

    def _save(self) -> None:
        save_controls(self.settings, self.path)
        self.changed.emit()

    def _rebind(self, action: str, name: str) -> None:
        self.settings.rebind(action, name)
        self._sync()
        self._save()

    def _cone_changed(self) -> None:
        self.settings.aim_cone = int(self.cone.currentData())
        self._save()

    def _face_changed(self, checked: bool) -> None:
        self.settings.face_mouse_when_still = bool(checked)
        self._save()

    def reset(self) -> None:
        self.settings.reset()
        self._sync()
        self._save()


def controls_qss() -> str:
    return f"""
        QLabel#controlsHint {{ color: {wood_theme.CREAM_SOFT}; }}
        QPushButton#bindingButton {{ font-family: Consolas, monospace; }}
        QCheckBox {{ color: {wood_theme.CREAM}; }}
    """


class ControlsDialog(QDialog):
    """The controls editor on its own walnut board."""

    def __init__(self, parent=None, path=None):
        super().__init__(parent)
        self.setWindowTitle("Adventure controls")
        self.setMinimumWidth(640)
        self.setStyleSheet(wood_theme.dialog_qss() + controls_qss())
        layout = QVBoxLayout(self)
        layout.setContentsMargins(22, 18, 22, 20)
        title = QLabel("Adventure controls")
        title.setObjectName("dialogTitle")
        layout.addWidget(title)
        self.editor = ControlsEditor(path=path)
        layout.addWidget(self.editor)
        done = QPushButton("Done")
        done.clicked.connect(self.accept)
        layout.addWidget(done)
