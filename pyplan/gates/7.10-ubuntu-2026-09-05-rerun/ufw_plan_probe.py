"""Read-only: what network_apply('lan') WOULD run on this box, before anything runs it.

sweep_driver4.py calls `services.network_apply(plan)`, and this box's ufw is INACTIVE.
`ufw --force enable` on a default-deny firewall with no allow for the port ssh is on is the
7.1 lockout. The merged engine has §39's guard (`decide_lockout()` / `detect_ssh_route()`),
so the plan is supposed to place an allow for the live sshd port BEFORE the enable. This
prints the plan and says whether that ordering holds, and applies nothing.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, "/home/pk/p7/checkout/pylauncher")

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.networking import detect_ssh_route  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path("/home/pk/wowserver")

entry = load_catalog().get("wow-wotlk")
services = ControllerServices.for_wotlk(entry, SERVER_DIR)

print("detect_ssh_route():", detect_ssh_route())
plan = services.network_plan("lan")
print("plan.ready       :", plan.ready)
print("plan.warnings    :", plan.warnings)
print("plan.manual_steps:", plan.manual_steps)
print("firewall_commands:")
for i, cmd in enumerate(plan.firewall_commands):
    print(f"  [{i}] {cmd}")
print("realmlist_sql    :", plan.realmlist_sql)

cmds = [tuple(c) for c in plan.firewall_commands]
enable_at = next((i for i, c in enumerate(cmds) if "enable" in c), None)
allow22_at = [i for i, c in enumerate(cmds) if any(str(p).startswith("22/") for p in c)]
print()
print("enable at index  :", enable_at)
print("ssh allow at     :", allow22_at)
if enable_at is None:
    print("VERDICT: no enable in the plan -- nothing to lock this box out")
elif allow22_at and min(allow22_at) < enable_at:
    print("VERDICT: SAFE -- an allow for the ssh port is placed before the enable")
else:
    print("VERDICT: UNSAFE -- the enable is not preceded by an allow for the ssh port")
