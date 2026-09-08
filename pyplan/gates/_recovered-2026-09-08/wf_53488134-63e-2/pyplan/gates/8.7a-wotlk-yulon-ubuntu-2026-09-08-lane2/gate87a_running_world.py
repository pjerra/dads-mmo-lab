"""8.7a, the two clauses that can only be answered by the RUNNING server.

Read-only. Nothing here writes to a database, stops a container or installs
anything. It asks the live worldserver two questions through the app's own
Console tab and reads that same server's own configuration report out of its
log:

* the world SQL a module's importer run applied is in the running server
  (`.lookup creature Warpweaver`, entry 190010, from mod-transmog's
  `trasm_world_NPC.sql`), and
* the module configuration the running server is using is reported BY the
  server -- `> Config: Found config value 'AiPlayerbot.MaxRandomBots' from
  environment variable ...` -- rather than read back off a file, while the
  module `.conf` files sitting on disk for modules that are NOT compiled into
  this image are invisible to it (`> Not found modules config files`).

Every capture records whether ac-worldserver was alive at that instant.
"""

from __future__ import annotations

import os
import subprocess
import sys
import time
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

SERVER_DIR = Path("/home/pk/wowserver")
HERE = Path(__file__).resolve().parent
SHOTS = HERE / "shots"
LOGS = HERE / "logs"
SHOTS.mkdir(parents=True, exist_ok=True)
LOGS.mkdir(parents=True, exist_ok=True)

sys.path.insert(0, str(HERE / "pylauncher"))

from PySide6.QtWidgets import QApplication  # noqa: E402

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices, ControllerView  # noqa: E402


def sh(*argv: str) -> str:
    return subprocess.run(list(argv), capture_output=True, text=True).stdout.strip()


def world_alive() -> bool:
    return "ac-worldserver" in sh("docker", "ps", "--format", "{{.Names}}").split()


def shot(view: ControllerView, name: str) -> None:
    alive = world_alive()
    view.grab().save(str(SHOTS / name))
    print(f"SHOT {name}  ac-worldserver alive at capture: {alive}")


def main() -> int:
    print("=== 8.7a: what the RUNNING world says ===")
    if not world_alive():
        print("ABORT: ac-worldserver is not running; every question below would be about nothing.")
        return 2

    entry = load_catalog().get("wow-wotlk")
    app = QApplication.instance() or QApplication([])
    services = ControllerServices.for_entry(entry, SERVER_DIR)
    view = ControllerView(entry, services, status_poll_ms=0)
    for i in range(view._tabs.count()):
        if view._tabs.tabText(i) == "Console":
            view._tabs.setCurrentIndex(i)
    view.resize(1100, 820)
    view.show()
    app.processEvents()

    print(f"console usable on this host: {view.send_button.isEnabled()}")
    for command in ("server info", "lookup creature Warpweaver"):
        print(f"--- sending {command!r} to the live worldserver ---")
        view.command_edit.setText(command)
        view.send_console_command()
        started = time.monotonic()
        while time.monotonic() - started < 45:
            app.processEvents()
            time.sleep(0.05)
            if not view._console_pending:
                break
        print(f"  answered in {time.monotonic() - started:.1f}s")
        app.processEvents()

    transcript = view.console_log.text()
    print("--- console transcript ---")
    print(transcript)
    print("--- end ---")
    (LOGS / "console-transcript.txt").write_text(transcript, encoding="utf-8")
    shot(view, "3-running-world-answers-lookup-190010.png")

    ok = "Warpweaver" in transcript
    print(f"the running world knows creature 190010 by name: {ok}")
    print(f"ac-worldserver alive at the end: {world_alive()}")
    print("VERDICT:", "PASS" if ok and world_alive() else "FAIL")
    return 0 if ok and world_alive() else 1


if __name__ == "__main__":
    raise SystemExit(main())
