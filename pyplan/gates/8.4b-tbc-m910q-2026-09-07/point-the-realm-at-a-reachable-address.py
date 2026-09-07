"""Point this install's realm at the address the client can actually reach.

Through the app's own Networking plan/apply -- the same press 8.3a used on the
WotLK box -- rather than by writing the row, because the row is what the feature
is for and a hand-written one would prove nothing about it.
"""
import sys
from pathlib import Path

sys.path.insert(0, "/home/pk/gate83b/pylauncher")
from yulon import networking  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui import controller_view as v  # noqa: E402

E = load_catalog().get("wow-tbc")
D = Path.home() / "tbc-7.4c"
WANT = sys.argv[1] if len(sys.argv) > 1 else "100.78.24.50"

sql = v._sql_for(E, v._db_password(E, D), wsl_distro=None)
print("realm before:", sql.query("auth", "SELECT id, name, address, port FROM realmd.realmlist;").strip())

plan = networking.plan(E, "lan", lan_ip=WANT, firewall="none", steamos=False, wsl=False)
print("the plan says:", plan.summary if hasattr(plan, "summary") else plan)
svc = v.ControllerServices.for_entry(E, D)
report = svc.network_apply(plan)
for line in getattr(report, "done", ()):
    print("  applied:", line)
print("restart required:", getattr(report, "restart_required", None))
print("realm after :", sql.query("auth", "SELECT id, name, address, port FROM realmd.realmlist;").strip())
