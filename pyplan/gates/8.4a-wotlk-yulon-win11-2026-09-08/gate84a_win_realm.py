"""Point this install's realm at the address the Hyper-V host can reach.

Through the app's own Networking plan/apply, not by writing the row: the client
half is meant to reach this box the way a user's would. This is 8.3a's
`gate83a_win_realm.py` with the source root and the server dir changed.

Usage:  python gate84a_win_realm.py <lan_ip>|show
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, r"C:\gate\src84a\pylauncher")

from yulon import networking  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui import controller_view as view_module  # noqa: E402

SERVER_DIR = Path(r"D:\wow-server")
ENTRY = load_catalog().get("wow-wotlk")


def reader():
    return view_module._sql_for(ENTRY, view_module._db_password(ENTRY, SERVER_DIR), wsl_distro=None)


def main() -> None:
    what = sys.argv[1]
    print(
        "realmlist before:",
        reader().query("auth", "SELECT id, name, address, port FROM realmlist;").strip(),
    )
    if what == "show":
        return
    made = networking.plan(ENTRY, "lan", lan_ip=what, elevate=False, enable_firewall=False)
    print("plan realmlist ->", made.realmlist_sql)
    report = networking.apply(made, sql=reader(), elevate=False, server_dir=SERVER_DIR)
    print("done   :", list(report.done))
    print("skipped:", list(report.skipped)[:4])
    print(
        "realmlist after :",
        reader().query("auth", "SELECT id, name, address, port FROM realmlist;").strip(),
    )


if __name__ == "__main__":
    main()
