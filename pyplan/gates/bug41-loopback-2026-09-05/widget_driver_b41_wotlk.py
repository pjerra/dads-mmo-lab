"""bug-checklist §41 gate, half one on yulon-ubuntu: choose the loopback through the REAL tab.

The WotLK sibling of `widget_driver_b41.py` (which was run on m910q's CMaNGOS
Vanilla install on 2026-09-05). It exists because the gate's own second clause —
"press Install again on the finished install" — could not run on m910q: the
CMaNGOS engine's `patch-sources` stage refuses a folder built before the doodad
patch, and m910q's preflight refused the press for disk (19 GB free, 40 GB
demanded) on 2026-09-05 at 22:20 box-local. yulon-ubuntu's AzerothCore install
at /home/pk/wowserver (install_id 243c46e3) is a finished install on a box with
50 GB free, so both halves of §41 run there.

Nothing here calls `networking.plan()` or `networking.apply()` itself. It builds
the real `ControllerServices.for_entry()` wiring and the real `ControllerView`
on a real offscreen `QApplication`, and presses the tab's real
`QRadioButton`/`QPushButton`s with `QTest.mouseClick`, which delivers a press and
a release to the widget exactly as a finger does. The row it then checks is read
back through `docker exec ... mysql`, a route the widget never touches.
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
# `wow-wotlk`'s catalogue entry has `install.password.mode == "fixed"`, value
# "password" — acore-docker's default root password. There is no `.db_password`
# file in this server folder (`ls -a /home/pk/wowserver` on 2026-09-06 listed
# only `.yulon-install.json` among this app's own files).
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
    """Read the realm row by a route that is NOT the widget under test."""
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
    say(f"BEFORE  ({time.strftime('%Y-%m-%dT%H:%M:%S%z')})")
    before = db_read(ROW_QUERY)
    say(f"  realm row (address, localAddress) = {before!r}")
    intent_path = SERVER_DIR / networking.INTENT_FILE
    say(f"  {networking.INTENT_FILE} present = {intent_path.exists()}")
    check("the row starts on a reachable address",
          bool(before) and "127." not in before, before)
    check("nothing had chosen a mode yet", not intent_path.exists())

    say("")
    say("=" * 78)
    say("PART 1 -- the third radio, clicked with QTest.mouseClick")
    say(f"  loopback radio label = {view.loopback_radio.text()!r}")
    check("the tab has a loopback radio", hasattr(view, "loopback_radio"))
    check("its label carries the address", "127.0.0.1" in view.loopback_radio.text())
    check("mode before the click is lan", view.network_mode() == "lan", view.network_mode())

    QTest.mouseClick(view.loopback_radio, Qt.MouseButton.LeftButton)
    app.processEvents()
    check("a real click selected it", view.loopback_radio.isChecked() is True)
    check("mode after the click is loopback", view.network_mode() == "loopback",
          view.network_mode())
    check("the other two are off",
          not view.lan_radio.isChecked() and not view.internet_radio.isChecked())

    say("")
    say("=" * 78)
    say("PART 2 -- Show plan, pressed")
    check("Apply is disabled before a plan exists", view.apply_button.isEnabled() is False)
    QTest.mouseClick(view.plan_button, Qt.MouseButton.LeftButton)
    pump(lambda: view._plan is not None, 180.0, "the plan")
    plan = view._plan
    say("  ---- the widget's own text ----")
    for line in view.network_text.toPlainText().splitlines():
        say(f"  | {line}")
    say("  -------------------------------")
    check("a plan arrived", plan is not None)
    if plan is not None:
        check("the plan's mode is loopback", plan.mode == "loopback", str(plan.mode))
        check("its realmlist UPDATE writes 127.0.0.1",
              plan.realmlist_sql is not None and "'127.0.0.1'" in plan.realmlist_sql,
              str(plan.realmlist_sql))
        check("the plan says what it costs",
              any("no other machine" in w for w in plan.warnings))
        say(f"  firewall commands this plan carries: "
            f"{[' '.join(c) for c in plan.firewall_commands]}")
        say(f"  portproxy commands: {[' '.join(c) for c in plan.portproxy_commands]}")
    check("Apply is enabled once a plan exists", view.apply_button.isEnabled() is True)

    say("")
    say("=" * 78)
    say("PART 3 -- Apply, pressed")
    QTest.mouseClick(view.apply_button, Qt.MouseButton.LeftButton)
    pump(lambda: "Applied:" in view.network_text.toPlainText(), 240.0, "the apply report")
    say("  ---- the widget's own text ----")
    for line in view.network_text.toPlainText().splitlines():
        say(f"  | {line}")
    say("  -------------------------------")

    after = db_read(ROW_QUERY)
    say(f"  realm row now = {after!r}   (read through docker exec, not the widget)")
    check("both address columns are the loopback", after == "127.0.0.1\t127.0.0.1", after)

    check("the choice was written to a file", intent_path.is_file(), str(intent_path))
    if intent_path.is_file():
        say(f"  ---- {intent_path} ----")
        for line in intent_path.read_text(encoding="utf-8").splitlines():
            say(f"  | {line}")
        say("  -------------------------------")
    intent = networking.read_network_intent(SERVER_DIR)
    check("it reads back as the loopback mode",
          intent is not None and intent.mode == "loopback", str(intent))

    view.close()
    say("")
    say("=" * 78)
    say(f"{PASSES} passed, {FAILS} failed")
    return 1 if FAILS else 0


if __name__ == "__main__":
    raise SystemExit(main())
