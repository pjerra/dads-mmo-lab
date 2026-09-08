"""8.7a independent confirmation: the refusal, counted in rows, from a second tree.

Non-destructive by construction. It presses the Modules tab's "Apply module SQL"
button with the world UP, which `docker.apply_module_sql()` refuses before it
starts anything, and it REFUSES TO RUN AT ALL unless the worldserver is up at
the moment of the press -- because the same press against a stopped world is the
applying half, which a second concurrent lane had already run on this box on
2026-09-08 and which this run must not repeat while that lane is finishing.

Every capture records whether ac-worldserver was alive at that instant, because
a screenshot of a client or a server that has DIED photographs exactly like a
refusal.
"""

from __future__ import annotations

import os
import subprocess
import sys
import time
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

SERVER_DIR = Path("/home/pk/wowserver")
SHOTS = Path(__file__).resolve().parent / "shots"
SHOTS.mkdir(parents=True, exist_ok=True)

sys.path.insert(0, str(Path(__file__).resolve().parent / "pylauncher"))

from PySide6.QtWidgets import QApplication  # noqa: E402

from yulon import docker  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices, ControllerView  # noqa: E402


def sh(*argv: str) -> str:
    return subprocess.run(list(argv), capture_output=True, text=True).stdout.strip()


def containers() -> set[str]:
    return {c for c in sh("docker", "ps", "-a", "--format", "{{.Names}}").split() if c}


def world_alive() -> bool:
    return "ac-worldserver" in sh("docker", "ps", "--format", "{{.Names}}").split()


def ledger() -> dict[str, int]:
    out = sh(
        "docker",
        "exec",
        "ac-database",
        "mysql",
        "-uroot",
        "-ppassword",
        "-N",
        "-B",
        "-e",
        "select count(*) from acore_world.updates; "
        "select count(*) from acore_characters.updates; "
        "select count(*) from acore_auth.updates;",
    )
    rows = [int(n) for n in out.split() if n.isdigit()]
    return dict(zip(("world", "characters", "auth"), rows))


def shot(view: ControllerView, name: str) -> None:
    alive = world_alive()
    path = SHOTS / name
    view.grab().save(str(path))
    print(f"SHOT {path.name}  ac-worldserver alive at capture: {alive}")


def main() -> int:
    print("=== 8.7a confirmation: refusal with the world UP, counted in rows ===")
    print(f"containers now: {sorted(containers())}")

    if not world_alive():
        print(
            "ABORT: ac-worldserver is NOT running. This script gates the REFUSAL only; "
            "the same press with the world down is the applying half, which another lane owns."
        )
        return 2

    entry = load_catalog().get("wow-wotlk")
    spec = entry.container_spec()
    print(f"import service: {spec.import_service!r}")
    print(f"allowed_modules({SERVER_DIR}) = {docker.allowed_modules(SERVER_DIR)!r}")
    sees = docker.importer_sees_modules(spec.import_service, SERVER_DIR)
    print(f"importer_sees_modules() = {sees!r}   (None = could not tell, False = no mount)")

    app = QApplication.instance() or QApplication([])
    services = ControllerServices.for_entry(entry, SERVER_DIR)
    print(f"services.module_sql wired: {services.module_sql is not None}")
    view = ControllerView(entry, services, status_poll_ms=0)
    for i in range(view._tabs.count()):
        if view._tabs.tabText(i) == "Modules":
            view._tabs.setCurrentIndex(i)
    view.resize(1100, 820)
    view.show()
    app.processEvents()
    print(
        f"button label: {view.module_sql_button.text()!r} "
        f"enabled={view.module_sql_button.isEnabled()}"
    )
    print(f"tooltip: {view.module_sql_button.toolTip()}")
    shot(view, "1-modules-tab-before-press.png")

    before_rows = ledger()
    before_containers = containers()
    print(f"ledger BEFORE the press: {before_rows}")

    failures: list[str] = []
    view.action_failed.connect(failures.append)

    if not world_alive():
        print("ABORT: the world went down between the check and the press.")
        return 2
    started = time.monotonic()
    view.module_sql_button.click()
    while time.monotonic() - started < 120 and not failures:
        app.processEvents()
        time.sleep(0.05)
    took = time.monotonic() - started

    after_rows = ledger()
    after_containers = containers()
    app.processEvents()
    shot(view, "2-refused-world-up.png")

    report = view.module_report.toPlainText()
    print("--- what the tab shows ---")
    print(report)
    print("--- end ---")
    print(f"ledger AFTER the press:  {after_rows}")
    written = {k: after_rows[k] - before_rows[k] for k in after_rows}
    print(f"rows written by the refused press: {written}")
    print(f"containers created by the press: {sorted(after_containers - before_containers)}")
    print(f"button enabled again: {view.module_sql_button.isEnabled()}  took {took:.1f}s")
    print(f"ac-worldserver alive at the end: {world_alive()}")

    ok = (
        bool(failures)
        and "running" in report
        and "Press Stop first" in report
        and "FAILED" in report
        and not (after_containers - before_containers)
        and after_rows == before_rows
        and world_alive()
    )
    print("VERDICT:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
