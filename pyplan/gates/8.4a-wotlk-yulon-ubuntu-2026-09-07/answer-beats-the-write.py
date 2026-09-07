"""How long after the server's own YES does its own row change?

The gate read the row immediately after "You are teleporting Aevret (offline) to
Stormwind." and found the old position; a second later it was the new one. If
the tab re-reads the moment the command answers, it shows a person the state
BEFORE the thing they just did.
"""
import sys
import time
from pathlib import Path

sys.path.insert(0, "/home/pk/gate81a/pylauncher")
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui import controller_view as v  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

E = load_catalog().get("wow-wotlk")
D = Path.home() / "wowserver"
sql = v._sql_for(E, v._db_password(E, D), wsl_distro=None)
S = E.schema_map()
NAME = sys.argv[1] if len(sys.argv) > 1 else "Aevret"
tab = ControllerServices.for_entry(E, D).play


def level() -> str:
    return sql.query(
        "characters", f"SELECT level FROM {S['characters']}.characters WHERE name = '{NAME}';"
    ).strip()


for target in (58, 59):
    before = level()
    started = time.monotonic()
    out = tab.set_level(NAME, target)
    answered = time.monotonic() - started
    waited = None
    while time.monotonic() - started < 20:
        if level() == str(target):
            waited = time.monotonic() - started
            break
        time.sleep(0.1)
    print(
        f"level {before} -> {target}: the server answered in {answered:.2f}s "
        f"({out.done}), the row changed after {waited if waited is None else round(waited, 2)}s"
    )
