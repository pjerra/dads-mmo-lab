"""Read the LAN plan BEFORE any driver is allowed to apply it, on yulon-ubuntu2.

Two reasons, one inherited and one new to this box.

The inherited one is the 7.1 lockout: `network_plan('lan')` once carried
`('ufw', '--force', 'enable')` with no warning, and applying it over an ssh session locked a
box out. `ufw` on this box is INACTIVE, so the plan is read here first and the run only
proceeds if no enable is in it (`pyplan/gates/7.10-ubuntu-2026-09-05-rerun/ufw_plan_probe.py`
is the ancestor of this file).

The new one belongs to yulon-ubuntu2. Its `acore_auth.realmlist` row advertises an address
that the 8.7a rebuild lane wrote, and this box has FOUR local IPv4 addresses
(`eth0 172.30.48.189`, `docker0`, and two compose bridges). `network_apply('lan')` rewrites
that row. So the row is read here, out of the database and not out of the app, and printed
next to the address the plan would write, so the runner can put it back afterwards and a
reader can see whether it had to.

Nothing here writes anything.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

from yulon.catalog.catalog import load_catalog
from yulon.ui.controller_view import ControllerServices

SERVER_DIR = Path("/home/pk/wowserver")


def db_read(statement: str) -> str:
    """Read the DB by a route that is not the code under test."""
    done = subprocess.run(
        ["docker", "exec", "ac-database", "mysql", "-uroot", "-ppassword", "-N", "-B",
         "-e", statement],
        capture_output=True, text=True,
    )
    return done.stdout.strip()


def main() -> int:
    import yulon

    print("yulon package under test:", Path(yulon.__file__).resolve())
    entry = load_catalog().get("wow-wotlk")
    services = ControllerServices.for_entry(entry, SERVER_DIR)

    print("\n--- the realm row as the database holds it right now ---")
    print(db_read("SELECT id, name, address, localAddress, port FROM acore_auth.realmlist;"))

    plan = services.network_plan("lan")
    print("\n--- network_plan('lan') ---")
    print(f"ready         : {plan.ready}")
    print("firewall_commands:")
    enable_at = None
    for index, command in enumerate(plan.firewall_commands):
        print(f"  [{index}] {command}")
        if "enable" in " ".join(command):
            enable_at = index
    print(f"enable at index  : {enable_at}")
    print(f"realmlist_sql : {plan.realmlist_sql}")
    print(f"manual_steps  : {plan.manual_steps}")
    print(f"warnings      : {plan.warnings}")

    verdict_lockout = (
        "VERDICT: no enable in the plan -- nothing to lock this box out"
        if enable_at is None
        else f"VERDICT: REFUSE -- an enable is at index {enable_at}; do not run the driver"
    )
    print("\n" + verdict_lockout)
    return 0 if enable_at is None else 1


if __name__ == "__main__":
    raise SystemExit(main())
