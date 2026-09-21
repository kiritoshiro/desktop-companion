"""Two-way live channel between the settings window and the running overlay.

Before this module the two processes only talked one way: the settings
window rewrote the launched preset file, and the overlay noticed the change
by polling the file's modification time every 700 ms (``engine.PRESET_WATCH_MS``).
Tray-driven changes -- mood, size, flies, social play, a spider's name, its
team -- were never written back anywhere, so the settings window's idea of
the session drifted from what was actually running (C7), and the next Save
from the settings window would silently overwrite them.

This adds a ``QLocalServer``/``QLocalSocket`` channel carrying small
newline-delimited JSON messages both ways over one connection per settings
window:

* the settings window pushes a ``preset_update`` (and can ask for a
  ``stop_request``) to the overlay;
* the overlay pushes a ``session_state`` snapshot back whenever a tray
  action changes something, and again as soon as a settings window connects.

The overlay is the server (``OverlayChannelServer``): it is the one
long-running process, and any number of settings windows may come and go.
The settings window is the client (``SettingsChannelClient``): it connects
when it wants to talk to a running overlay and treats a failed connection as
routine, not an error -- there may be no overlay running yet, it may be an
older build with no server, or the socket name may be taken by a stuck
process. Every caller of this module keeps its existing file-based fallback
(the preset-file poll for live edits, the stop-request file from
``session_control`` for shutdown) for exactly those cases; this channel is a
faster, richer path layered on top, not a replacement that has to work.

Message envelope
-----------------
Every message is one JSON object followed by ``"\\n"``. All messages carry a
``"v"`` protocol version (``PROTOCOL_VERSION``) and a ``"type"``. A receiver
that sees a ``"v"`` it does not understand drops the connection rather than
guess at an incompatible schema, which is what makes "old settings window,
new overlay" or vice versa fall back to file polling instead of misapplying
a message.

============  ==========  ===================================================
type          direction   payload
============  ==========  ===================================================
hello         C -> S      (none) sent right after connecting
session_state S -> C      ``state``: ``CreatureManager.session_snapshot()``
preset_update C -> S      ``preset_path``, ``data`` (a full preset dict)
stop_request  C -> S      (none) ask the overlay to save and quit
============  ==========  ===================================================

(C = settings window / client, S = overlay / server.)
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Iterator

from PyQt5.QtCore import QObject, pyqtSignal
from PyQt5.QtNetwork import QLocalServer, QLocalSocket

from ..support.logging_setup import get_logger

log = get_logger("live_channel")

PROTOCOL_VERSION = 1


def channel_name(state_dir) -> str:
    """A ``QLocalServer`` name unique to one state directory.

    An overlay and a settings window that share a state directory are one
    session and should share one channel. A different install (a portable
    copy alongside an installed one, or two test runs each with their own
    ``DESKTOP_BUG_STATE_DIR``) must not cross-talk, so the name is derived
    from the resolved state directory instead of being fixed.
    """
    digest = hashlib.sha1(
        str(Path(state_dir).resolve()).encode("utf-8", "replace")
    ).hexdigest()
    return f"DesktopBugCompanion-{digest[:16]}"


def _stamped(message: dict) -> dict:
    out = dict(message)
    out.setdefault("v", PROTOCOL_VERSION)
    return out


def _encode(message: dict) -> bytes:
    return (json.dumps(_stamped(message), sort_keys=True) + "\n").encode("utf-8")


class _LineReader:
    """Buffers bytes from a socket and yields complete JSON objects.

    A socket delivers arbitrary chunks, not whole messages -- a pushed
    preset is comfortably bigger than one read on a busy pipe. Newline
    delimited JSON is the simplest framing that survives that.
    """

    def __init__(self) -> None:
        self._buf = b""

    def feed(self, chunk: bytes) -> Iterator[dict]:
        self._buf += chunk
        *lines, self._buf = self._buf.split(b"\n")
        for line in lines:
            line = line.strip()
            if not line:
                continue
            try:
                message = json.loads(line.decode("utf-8"))
            except (UnicodeDecodeError, json.JSONDecodeError):
                log.warning("Dropped an unparsable live-channel message")
                continue
            if isinstance(message, dict):
                yield message


class OverlayChannelServer(QObject):
    """The overlay's end of the channel: one server, any number of clients."""

    message_received = pyqtSignal(dict)
    client_connected = pyqtSignal()
    client_disconnected = pyqtSignal()

    def __init__(self, name: str, parent=None) -> None:
        super().__init__(parent)
        self._name = name
        self._server = QLocalServer(self)
        self._server.newConnection.connect(self._accept_pending)
        self._sockets: list[QLocalSocket] = []
        self._readers: dict[int, _LineReader] = {}

    def listen(self) -> bool:
        # A crashed process can leave a stale socket file behind on POSIX
        # (a harmless no-op against Windows named pipes); clear it first or
        # every relaunch after a crash would fail to bind.
        QLocalServer.removeServer(self._name)
        ok = self._server.listen(self._name)
        if not ok:
            log.warning(
                "Could not open the live channel (%s): %s",
                self._name,
                self._server.errorString(),
            )
        return ok

    def is_listening(self) -> bool:
        return self._server.isListening()

    @property
    def client_count(self) -> int:
        return len(self._sockets)

    def close(self) -> None:
        for sock in list(self._sockets):
            sock.close()
        self._sockets.clear()
        self._readers.clear()
        self._server.close()

    def broadcast(self, message: dict) -> int:
        """Send to every connected settings window. Returns how many got it."""
        payload = _encode(message)
        sent = 0
        for sock in list(self._sockets):
            if sock.state() == QLocalSocket.ConnectedState:
                sock.write(payload)
                sock.flush()
                sent += 1
        return sent

    def _accept_pending(self) -> None:
        while self._server.hasPendingConnections():
            sock = self._server.nextPendingConnection()
            if sock is None:
                continue
            self._sockets.append(sock)
            self._readers[id(sock)] = _LineReader()
            sock.readyRead.connect(lambda s=sock: self._on_ready_read(s))
            sock.disconnected.connect(lambda s=sock: self._on_disconnected(s))
            self.client_connected.emit()

    def _on_ready_read(self, sock: QLocalSocket) -> None:
        reader = self._readers.get(id(sock))
        if reader is None:
            return
        for message in reader.feed(bytes(sock.readAll())):
            if message.get("v") != PROTOCOL_VERSION:
                # A settings window from an incompatible build: do not guess
                # at its message shape. Closing forces it back onto file
                # polling, which works across any version.
                log.warning(
                    "Live channel protocol mismatch (got %r, want %r); closing",
                    message.get("v"), PROTOCOL_VERSION,
                )
                sock.close()
                return
            self.message_received.emit(message)

    def _on_disconnected(self, sock: QLocalSocket) -> None:
        if sock in self._sockets:
            self._sockets.remove(sock)
        self._readers.pop(id(sock), None)
        sock.deleteLater()
        self.client_disconnected.emit()


class SettingsChannelClient(QObject):
    """The settings window's end of the channel: connects to a running overlay."""

    message_received = pyqtSignal(dict)
    connected = pyqtSignal()
    disconnected = pyqtSignal()

    def __init__(self, name: str, parent=None) -> None:
        super().__init__(parent)
        self._name = name
        self._socket = QLocalSocket(self)
        self._reader = _LineReader()
        self._socket.readyRead.connect(self._on_ready_read)
        self._socket.connected.connect(self._on_connected)
        self._socket.disconnected.connect(self.disconnected.emit)

    def try_connect(self, timeout_ms: int = 200) -> bool:
        """Connect if not already connected.

        Failure is routine, not exceptional: no overlay may be running yet,
        it may be an old build with no server, or the name may belong to a
        stuck process. Every caller must have a working fallback for a
        ``False`` return, exactly as it did before this channel existed.
        """
        if self.is_connected():
            return True
        self._socket.abort()
        self._socket.connectToServer(self._name)
        return self._socket.waitForConnected(timeout_ms)

    def is_connected(self) -> bool:
        return self._socket.state() == QLocalSocket.ConnectedState

    def send(self, message: dict) -> bool:
        if not self.is_connected():
            return False
        self._socket.write(_encode(message))
        return self._socket.flush()

    def close(self) -> None:
        self._socket.disconnectFromServer()

    def _on_connected(self) -> None:
        self.send({"type": "hello"})
        self.connected.emit()

    def _on_ready_read(self) -> None:
        for message in self._reader.feed(bytes(self._socket.readAll())):
            if message.get("v") != PROTOCOL_VERSION:
                log.warning(
                    "Live channel protocol mismatch (got %r, want %r); ignoring",
                    message.get("v"), PROTOCOL_VERSION,
                )
                continue
            self.message_received.emit(message)
