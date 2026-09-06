"""bug-checklist §41, the way back: pick LAN in the same tab and the record stops saying loopback.

Run on yulon-ubuntu after the two Install presses, against /home/pk/wowserver.

The gate asks how a user undoes a loopback choice without deleting a file by
hand. `networking.record_network_intent()` stores the LAST APPLIED mode rather
than only `loopback`, so the answer is the same two buttons the install log's
sentence names: pick another radio, Show plan, Apply. This driver presses them
for real (`QTest.mouseClick` on the tab's own widgets) and reads the record back
afterwards, so the answer in the checklist entry is a measurement and not a
reading of the source.

It is also how this box is put back: the LAN Apply rewrites the realm row to the
address it held before this lane touched it. The record file itself is removed
afterwards by the run script, because the folder had none when the lane started.
"""

from __future__ import annotations

import os
import subprocess
import sys
import time
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

CHECKOUT = Path(__file__).resolve().parents[3] / "pylauncher"
sys.path.insert(0, str(CHECKOUT))

from PySide6.QtCore import Qt  # noqa: E402
from PySide6.QtTest import QTest  # noqa: E402
from PySide6.QtWidgets import QApplication  # noqa: E402

from yulon import networking  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices, ControllerView  # noqa: E402

SERVER_DIR = Path("/home/pk/wowserver")
GAME = "wow-wotlk"
DB_PASSWORD = "password"
DB_CONTAINER = "ac-database"
ROW_QUERY = "SELECT address, localAddress FROM acore_auth.realmlist WHERE id=1;"

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


def db_read(statement: str) -> str:
    done = subprocess.run(
        ["docker", "exec", "-e", f"MYSQL_PWD={DB_PASSWORD}", DB_CONTAINER,
         "mysql", "-uroot", "-N", "-B", "-e", statement],
        capture_output=True, text=True,
    )
    return done.stdout.strip()


def pump(predicate, timeout_s: float = 120.0, label: str = "") -> bool:
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


def main() -> int:
    app = QApplication([])
    entry = load_catalog().get(GAME)
    services = ControllerServices.for_entry(entry, SERVER_DIR, None)
    view = ControllerView(entry, services, status_poll_ms=0)
    view.resize(1100, 800)
    view.show()
    app.processEvents()

    say("=" * 78)
    say(f"BEFORE THE UNDO  ({time.strftime('%Y-%m-%dT%H:%M:%S%z')})")
    say(f"  realm row = {db_read(ROW_QUERY)!r}")

    # The starting point this half is about: a server whose owner HAS chosen the
    # loopback. The press half removed the record to prove the other clause, so
    # it is written again here through the same tab.
    QTest.mouseClick(view.loopback_radio, Qt.MouseButton.LeftButton)
    app.processEvents()
    QTest.mouseClick(view.plan_button, Qt.MouseButton.LeftButton)
    pump(lambda: view._plan is not None, 180.0, "the loopback plan")
    QTest.mouseClick(view.apply_button, Qt.MouseButton.LeftButton)
    pump(lambda: "Applied:" in view.network_text.toPlainText(), 240.0, "the loopback apply")
    chosen = networking.read_network_intent(SERVER_DIR)
    check("the loopback is chosen again, through the tab",
          chosen is not None and chosen.mode == "loopback", str(chosen))
    check("and the row is the loopback again", db_read(ROW_QUERY) == "127.0.0.1\t127.0.0.1",
          db_read(ROW_QUERY))

    say("")
    say("=" * 78)
    say("THE UNDO -- 'LAN (same Wi-Fi)', Show plan, Apply, all clicked")
    QTest.mouseClick(view.lan_radio, Qt.MouseButton.LeftButton)
    app.processEvents()
    check("mode after the click is lan", view.network_mode() == "lan", view.network_mode())
    QTest.mouseClick(view.plan_button, Qt.MouseButton.LeftButton)
    pump(lambda: view._plan is not None and view._plan.mode == "lan", 180.0, "the lan plan")
    QTest.mouseClick(view.apply_button, Qt.MouseButton.LeftButton)
    pump(lambda: "Applied:" in view.network_text.toPlainText(), 240.0, "the lan apply")
    say("  ---- the widget's own text ----")
    for line in view.network_text.toPlainText().splitlines():
        say(f"  | {line}")
    say("  -------------------------------")

    undone = networking.read_network_intent(SERVER_DIR)
    check("the record no longer says loopback",
          undone is not None and undone.mode == "lan", str(undone))
    row = db_read(ROW_QUERY)
    say(f"  realm row now = {row!r}")
    check("the row advertises a reachable address again", "127." not in row, row)

    intent_path = SERVER_DIR / networking.INTENT_FILE
    if intent_path.is_file():
        say(f"  ---- {intent_path} ----")
        for line in intent_path.read_text(encoding="utf-8").splitlines():
            say(f"  | {line}")
        say("  -------------------------------")

    view.close()
    say("")
    say("=" * 78)
    say(f"{PASSES} passed, {FAILS} failed")
    return 1 if FAILS else 0


if __name__ == "__main__":
    raise SystemExit(main())
