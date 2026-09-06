"""bug-checklist §39 / 7.1 clause 15: the account the LAN login will use, made by the widget.

The brief allows `ControllerServices.create_account` -- the seam the Accounts tile calls.
This goes one step further and presses the tile: the name, the password and the GM level
are typed into the real QLineEdits with `QTest.keyClicks` and `Create` is clicked with
`QTest.mouseClick`, so the account the client logs in with is the one a user's hands would
have made. The row is then read back by `docker exec mysql`, which is not the app.
"""

from __future__ import annotations

import os
import subprocess
import sys
import time
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
sys.path.insert(0, "/home/pk/p7/checkout/pylauncher")

from PySide6.QtCore import Qt  # noqa: E402
from PySide6.QtTest import QTest  # noqa: E402
from PySide6.QtWidgets import QApplication  # noqa: E402

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices, ControllerView  # noqa: E402

SERVER_DIR = Path("/home/pk/wowserver")
ACCOUNT = "LANGATE"
PASSWORD = "langate1"
GM = 0


def say(text: str = "") -> None:
    print(text, flush=True)


def mysql(statement: str) -> str:
    done = subprocess.run(
        ["docker", "exec", "ac-database", "mysql", "-uroot", "-ppassword", "-N", "-B",
         "-e", statement],
        capture_output=True, text=True,
    )
    say(f"$ docker exec ac-database mysql -e {statement!r}")
    say(f"  exit {done.returncode}")
    for line in done.stdout.rstrip().splitlines():
        say(f"  {line}")
    say()
    return done.stdout.strip()


def main() -> int:
    say(f"--- {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())} UTC / "
        f"{time.strftime('%Y-%m-%dT%H:%M:%S%z')} local")
    before = mysql(f"SELECT COUNT(*) FROM acore_auth.account WHERE username='{ACCOUNT}';")
    if before != "0":
        say(f"REFUSING: {ACCOUNT} already exists (count={before})")
        return 2

    app = QApplication([])
    entry = load_catalog().get("wow-wotlk")
    services = ControllerServices.for_entry(entry, SERVER_DIR)
    view = ControllerView(entry, services, status_poll_ms=0)
    view.resize(900, 700)
    view.show()
    app.processEvents()

    view.account_name.setFocus()
    QTest.keyClicks(view.account_name, ACCOUNT)
    view.account_password.setFocus()
    QTest.keyClicks(view.account_password, PASSWORD)
    say(f"typed into the real QLineEdits: name={view.account_name.text()!r} "
        f"password length={len(view.account_password.text())} gm={view.account_gm.value()}")
    assert view.account_name.text() == ACCOUNT
    assert view.account_gm.value() == GM, view.account_gm.value()

    QTest.mouseClick(view.create_account_button, Qt.MouseButton.LeftButton)
    deadline = time.time() + 180
    while time.time() < deadline:
        app.processEvents()
        text = view.account_report.text()
        if text.strip() and not text.startswith("Creating "):
            break
        time.sleep(0.02)
    say(f"account_report -> {view.account_report.text()!r}")
    say()

    row = mysql(
        "SELECT a.id, a.username, LENGTH(a.salt), LENGTH(a.verifier), "
        "IFNULL(aa.gmlevel,-99), a.last_login, IFNULL(a.last_ip,'<null>'), a.failed_logins, "
        "a.joindate FROM acore_auth.account a "
        "LEFT JOIN acore_auth.account_access aa ON aa.id=a.id "
        f"WHERE a.username='{ACCOUNT}';"
    )
    parts = row.split("\t")
    ok = len(parts) >= 5 and parts[2] == "32" and parts[3] == "32"
    say(f"SRP6 row shape (salt 32, verifier 32): {'OK' if ok else 'WRONG'} -- {parts[:5]}")
    say(f"account_access gmlevel: {parts[4] if len(parts) > 4 else '?'} "
        "(-99 means no account_access row, which is what GM 0 should leave)")
    say(f"--- {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())} UTC done")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
