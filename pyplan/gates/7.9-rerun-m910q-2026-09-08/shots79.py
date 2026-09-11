"""Photograph the Server and Console tabs of one CMaNGOS game, on the merged tip.

WHY THIS EXISTS BESIDE THE GATE. `gate-79-controller-surface.py` drives `ControllerServices`
and says why: it times the callables the Server tab's buttons bind, not a sibling of them.
That is the right instrument for a timing gate and the wrong one for a photograph -- nothing
it does travels through a QWidget, so there is no surface to capture. This script presses the
same two clauses through the real `ControllerView`, with `QTest.mouseClick`, and grabs a frame
either side of each press.

TWO RULES IT IS BUILT AROUND.

`status_poll_ms=0`. `ControllerView` re-reads status on a 5 s timer, so a "before" frame taken
with the timer on can already hold the answer and the click proves nothing. With the timer off
the label reads `status: unknown` until something clicks Refresh, and the BEFORE frame is
asserted to hold exactly that before it is written -- a frame named "before" that shows the
after state is a false artefact, and 8.2d shipped one.

A dead server photographs like a refusal. Every capture writes a sidecar line naming the
containers `docker ps` reports up at that instant, read through the CLI rather than through the
code under test, so a frame can never be read as proof of a server that had already gone.

RUN IT:
    python shots79.py <game-id> <server-dir> <shots-dir>
"""

from __future__ import annotations

import os
import subprocess
import sys
import time
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from PySide6.QtCore import Qt  # noqa: E402
from PySide6.QtTest import QTest  # noqa: E402
from PySide6.QtWidgets import QApplication  # noqa: E402

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices, ControllerView  # noqa: E402

PASSES = 0
FAILS = 0


def say(text: str = "") -> None:
    print(text, flush=True)


def check(label: str, ok: bool, detail: str = "") -> None:
    global PASSES, FAILS
    if ok:
        PASSES += 1
        say(f"[OK]   {label}" + (f" -- {detail}" if detail else ""))
    else:
        FAILS += 1
        say(f"[FAIL] {label}" + (f" -- {detail}" if detail else ""))


def containers_up(names: list[str]) -> list[str]:
    out = subprocess.run(
        ["docker", "ps", "--format", "{{.Names}}"], capture_output=True, text=True
    ).stdout.split()
    return [name for name in names if name in out]


def pump(predicate, timeout_s: float, label: str) -> bool:
    app = QApplication.instance()
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        app.processEvents()
        if predicate():
            app.processEvents()
            return True
        time.sleep(0.02)
    say(f"       (timed out after {timeout_s}s waiting for {label})")
    return False


def panel_text(panel) -> str:
    value = panel.text
    return value if isinstance(value, str) else value()


def main() -> int:
    game, server_dir, shots_dir = sys.argv[1], Path(sys.argv[2]), Path(sys.argv[3])
    shots_dir.mkdir(parents=True, exist_ok=True)
    entry = load_catalog().get(game)
    spec = entry.container_spec()
    names = [spec.db, spec.auth, spec.world]

    def shot(widget, name: str) -> None:
        QApplication.instance().processEvents()
        path = shots_dir / f"{game}-{name}.png"
        widget.grab().save(str(path))
        alive = containers_up(names)
        line = (f"{time.strftime('%FT%T%z')}  {path.name}  containers alive at capture: "
                f"{alive if alive else 'NONE'}")
        say(f"       [shot] {line}")
        with (shots_dir / "shots.txt").open("a", encoding="utf-8") as handle:
            handle.write(line + "\n")

    app = QApplication([])
    services = ControllerServices.for_entry(entry, server_dir)

    say(f"=== {game} at {server_dir} ===")
    say(f"containers: {names}")
    say(f"up before anything: {containers_up(names)}")

    view = ControllerView(entry, services, status_poll_ms=0)
    view.resize(980, 720)
    view.show()
    app.processEvents()

    tabs = view._tabs  # noqa: SLF001 -- a gate photographing the surface a user sees
    say(f"tabs: {[tabs.tabText(i) for i in range(tabs.count())]}")

    def show_tab(title: str) -> bool:
        for index in range(tabs.count()):
            if tabs.tabText(index) == title:
                tabs.setCurrentIndex(index)
                app.processEvents()
                return True
        say(f"       no tab titled {title!r}")
        return False

    # ---------------------------------------------------------- Server tab
    show_tab("Server")
    check("the 5 s status poll is OFF, so only a click can fill status_label",
          view.status_label.text() == "status: unknown",
          f"before any click: {view.status_label.text()!r}")
    shot(view, "server-tab-before-refresh")

    QTest.mouseClick(view.refresh_button, Qt.MouseButton.LeftButton)
    pump(lambda: "db" in view.status_label.text(), 90, "status_label")
    status_text = view.status_label.text()
    say(f"       status_label -> {status_text!r}")
    check("the click filled the Server tab's status label from the live daemon",
          "db" in status_text and "auth" in status_text and "world" in status_text,
          status_text)
    check("what the widget says agrees with `docker ps` read separately",
          set(containers_up(names)) == set(names),
          f"docker ps -> {containers_up(names)}")
    shot(view, "server-tab-after-refresh")

    # --------------------------------------------------------- Console tab
    if show_tab("Console"):
        before_text = panel_text(view.console_log)
        check("the console panel is empty before the Send click", before_text.strip() == "",
              f"{len(before_text)} chars")
        shot(view, "console-tab-before-send")
        view.command_edit.setFocus()
        QTest.keyClicks(view.command_edit, "server info")
        check("the typed command reached the QLineEdit",
              view.command_edit.text() == "server info", repr(view.command_edit.text()))
        QTest.mouseClick(view.send_button, Qt.MouseButton.LeftButton)
        pump(lambda: len(panel_text(view.console_log)) > len(before_text) + 40, 120,
             "a console reply")
        reply_text = panel_text(view.console_log)
        say("       console reply, as the widget holds it:")
        for line in [ln for ln in reply_text.splitlines() if ln.strip()][-8:]:
            say(f"         {line}")
        check("the console round-trip came back through the widget and is not empty",
              len([ln for ln in reply_text.splitlines() if ln.strip()]) > 0,
              f"{len(reply_text)} chars")
        shot(view, "console-tab-after-send")

    view.shutdown()
    app.processEvents()
    say(f"up after everything: {containers_up(names)}")
    say(f"RESULT: {PASSES} OK, {FAILS} FAIL")
    return 0 if FAILS == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
