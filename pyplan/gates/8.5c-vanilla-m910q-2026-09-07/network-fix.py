"""Press Yu'lon's own Networking feature so the realm row names an address the
client box can reach.

Not a hand-written UPDATE. The row is the client lane's whole blocker -- 8.4b
lost an evening to it -- and a row typed by hand proves the row, while a row
written by `services.network_apply(plan)` proves the feature that is supposed to
write it. `ufw` is INACTIVE on this box and the plan REFUSES to enable it
(bug-checklist §39), so the firewall half is two rules added to a rule list
nothing is enforcing.
"""
import sys
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate85c" / "pylauncher"))

from yulon import networking
from yulon.catalog.catalog import load_catalog
from yulon.ui.controller_view import ControllerServices

SERVER_DIR = Path.home() / "vanilla-75b"
entry = load_catalog().get("wow-vanilla")
services = ControllerServices.for_entry(entry, SERVER_DIR)


def stamp():
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def realm():
    from yulon.ui import controller_view as view

    sql = view._sql_for(entry, view._db_password(entry, SERVER_DIR), wsl_distro=None)
    return sql.query("auth", "SELECT id, name, address, port FROM realmd.realmlist;").strip()


print(f"[{stamp()}] BEFORE: {realm()!r}")
plan = services.network_plan("lan")
# The seam the tab presses. `network_plan` is the tab's own, so the ip is the
# one the machine detected -- and on this box that is 192.168.10.134, the
# unreachable one. The Tailscale address has to be said out loud.
plan = networking.plan(entry, "lan", lan_ip="100.78.24.50")
print(f"[{stamp()}] the plan's realmlist statement: {plan.realmlist_sql}")
report = services.network_apply(plan)
for name in ("done", "skipped", "refusals", "warnings", "manual_steps"):
    for line in getattr(report, name, []) or []:
        print(f"[{stamp()}] {name}: {line[:200]}")
print(f"[{stamp()}] AFTER: {realm()!r}")
