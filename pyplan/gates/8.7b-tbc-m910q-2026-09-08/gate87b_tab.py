"""8.7b step 4: the Modules tab's own Install press, photographed before and after.

Not `applier.install(...)` — the BUTTON, through `ControllerView._module_action`,
so the shots are of the surface a user sees and the value written is the value a
user's press really produces.
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
SHOTS.mkdir(parents=True, exist_ok=True)
SERVER = Path("/home/pk/tbc-7.4c")


def alive() -> str:
    r = subprocess.run(
        ["docker", "inspect", "-f", "{{.State.Running}}", "tbc-mangosd"],
        capture_output=True, text=True,
    )
    return r.stdout.strip() or "?"


def pump(view: ControllerView, app: QApplication, seconds: float = 120.0) -> None:
    """Spin the GUI loop until the module action reports back, or time runs out."""
    end = time.time() + seconds
    while view._module_pending is not None and time.time() < end:
        app.processEvents()
        time.sleep(0.05)
    for _ in range(20):
        app.processEvents()
        time.sleep(0.02)


def main() -> int:
    app = QApplication.instance() or QApplication([])
    entry = load_catalog().get("wow-tbc")
    services = ControllerServices.for_entry(entry, SERVER)
    view = ControllerView(entry, services)
    view.resize(1100, 780)
    # The Modules tab, by the label the user reads, not by index.
    tabs = view._tabs
    idx = [i for i in range(tabs.count()) if tabs.tabText(i) == "Modules"][0]
    tabs.setCurrentIndex(idx)

    print("mangosd alive at BEFORE capture:", alive())
    print("rows the tab lists:")
    for i in range(view.module_list.count()):
        print("   ", view.module_list.item(i).text())

    # Select the motd row the way a click does.
    target = [
        i for i in range(view.module_list.count())
        if view.module_list.item(i).data(256) == "motd"
    ][0]
    view.module_list.setCurrentRow(target)
    app.processEvents()
    print("selected:", view.selected_manifest().id)
    print("report box before:", repr(view.module_report.toPlainText()))
    view.grab().save(str(SHOTS / "4-modules-tab-before-install.png"))

    conf_before = (SERVER / "etc/mangosd.conf").read_text(encoding="utf-8", errors="replace")
    motd_before = [ln for ln in conf_before.splitlines() if ln.startswith("Motd")]
    print("Motd line in the file BEFORE the press:", motd_before)

    print("\n--- pressing 'Install selected' ---")
    view.install_module_button.click()
    pump(view, app)

    print("mangosd alive at AFTER capture:", alive())
    print("report box after the press:")
    print(view.module_report.toPlainText())
    view.grab().save(str(SHOTS / "5-modules-tab-after-install.png"))

    conf_after = (SERVER / "etc/mangosd.conf").read_text(encoding="utf-8", errors="replace")
    motd_after = [ln for ln in conf_after.splitlines() if ln.startswith("Motd")]
    print("Motd line in the file AFTER the press:", motd_after)
    print("no dialog was shown: the tab asked nothing, because motd's prompt has a default.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
