"""8.7a live gate: press the new "Apply module SQL" button against a RUNNING install.

Run on yulon-ubuntu, whose AzerothCore stack is up. Proves the clause the box
owns: module SQL aimed at the character or world database while the world is
running is refused, inside the step every caller passes through — and that the
refusal is what the tab puts on screen rather than a claim that anything was
applied.

Nothing here stops, starts or writes to anything. The one docker command the
guard lets through before refusing is a read.
"""

from __future__ import annotations

import os
import subprocess
import sys
import time
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

SERVER_DIR = Path("/home/pk/wowserver")

sys.path.insert(0, str(Path(__file__).resolve().parent / "pylauncher"))

from PySide6.QtWidgets import QApplication  # noqa: E402

from yulon import docker  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices, ControllerView  # noqa: E402


def containers() -> set[str]:
    out = subprocess.run(
        ["docker", "ps", "-a", "--format", "{{.Names}}"], capture_output=True, text=True
    )
    return {line for line in out.stdout.split() if line}


def main() -> int:
    print("=== 8.7a live: the Modules tab's importer against a running world ===")
    print(f"live containers now: {sorted(containers())}")

    entry = load_catalog().get("wow-wotlk")
    spec = entry.container_spec()
    print(f"import service this game declares: {spec.import_service!r}")

    # What the route would tell the importer, read off the live install's disk.
    print(f"allowed_modules({SERVER_DIR}) = {docker.allowed_modules(SERVER_DIR)!r}")
    sees = docker.importer_sees_modules(spec.import_service, SERVER_DIR)
    print(f"importer_sees_modules() = {sees!r}   (None = could not tell, False = no mount)")

    app = QApplication.instance() or QApplication([])
    services = ControllerServices.for_entry(entry, SERVER_DIR)
    print(f"services.module_sql wired: {services.module_sql is not None}")
    view = ControllerView(entry, services, status_poll_ms=0)
    print(f"button label: {view.module_sql_button.text()!r}")
    print(f"button enabled: {view.module_sql_button.isEnabled()}")
    print(f"tooltip: {view.module_sql_button.toolTip()}")

    failures: list[str] = []
    view.action_failed.connect(failures.append)

    before = containers()
    started = time.monotonic()
    view.module_sql_button.click()  # a real click, on the real wiring
    # Real threaded job runner: pump the loop until the tab says something.
    while time.monotonic() - started < 120 and not failures:
        app.processEvents()
        time.sleep(0.05)
    after = containers()

    print("--- what the tab shows ---")
    print(view.module_report.toPlainText())
    print("--- end ---")
    print(f"action_failed carried: {failures[:1]}")
    print(f"containers created by the press: {sorted(after - before)}")
    print(f"button enabled again: {view.module_sql_button.isEnabled()}")
    print(f"took {time.monotonic() - started:.1f}s")

    report = view.module_report.toPlainText()
    ok = (
        bool(failures)
        and "running" in report
        and "Press Stop first" in report
        and "FAILED" in report
        and not (after - before)
    )
    print("VERDICT:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
