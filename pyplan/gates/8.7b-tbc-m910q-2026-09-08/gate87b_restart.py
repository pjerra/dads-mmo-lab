"""8.7b step 7: the restart the report asked for, pressed on the Server tab.

The Modules report said *"Press Stop and then Start on the Server tab to apply
this."* This presses exactly those two buttons, through `ControllerView`, and
photographs the Server tab in each state — so the restart is the one the app
named, not a `docker compose restart` typed beside it.
"""
from __future__ import annotations

import os
import subprocess
import sys
import time
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
sys.path.insert(0, "/home/pk/gate87b/pylauncher")

from PySide6.QtWidgets import QApplication  # noqa: E402

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerView, ControllerServices  # noqa: E402

SHOTS = Path("/home/pk/gate87b/evidence")
SERVER = Path("/home/pk/tbc-7.4c")


def state() -> str:
    r = subprocess.run(
        ["docker", "inspect", "-f", "{{.Name}} running={{.State.Running}} started={{.State.StartedAt}}",
         "tbc-mangosd"], capture_output=True, text=True)
    return (r.stdout.strip() or r.stderr.strip())


def pump(app, view, seconds):
    end = time.time() + seconds
    while view._busy and time.time() < end:
        app.processEvents()
        time.sleep(0.1)
    for _ in range(20):
        app.processEvents()
        time.sleep(0.02)


def main() -> int:
    app = QApplication.instance() or QApplication([])
    entry = load_catalog().get("wow-tbc")
    view = ControllerView(entry, ControllerServices.for_entry(entry, SERVER))
    view.resize(1100, 780)
    view._tabs.setCurrentIndex(0)
    view.refresh_status()
    pump(app, view, 60)
    print("container before Stop:", state())
    print("status label:", view.status_label.text())

    print("\n--- pressing Stop (stop_grace_period is 300 s; this can take minutes) ---")
    t0 = time.time()
    view.stop_server()
    pump(app, view, 900)
    print("Stop returned after %.0f s" % (time.time() - t0))
    print("container after Stop:", state())
    print("status label:", view.status_label.text())
    view.grab().save(str(SHOTS / "6-server-tab-stopped.png"))

    print("\n--- pressing Start ---")
    t1 = time.time()
    view.start_server()
    pump(app, view, 900)
    print("Start returned after %.0f s" % (time.time() - t1))
    print("container after Start:", state())
    print("status label:", view.status_label.text())
    view.grab().save(str(SHOTS / "7-server-tab-started.png"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
