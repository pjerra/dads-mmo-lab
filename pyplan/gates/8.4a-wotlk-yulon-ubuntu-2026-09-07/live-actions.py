"""The actions, on a character a person is actually playing, spaced for frames.

The client photographs itself every fifteen seconds; these are spaced so each
effect lands inside its own frame.
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
NAME = "Asfgg"
sql = v._sql_for(E, v._db_password(E, D), wsl_distro=None)
tab = ControllerServices.for_entry(E, D).play


def row() -> str:
    return sql.query(
        "characters",
        f"SELECT level, online, map, position_x FROM acore_characters.characters "
        f"WHERE name = '{NAME}';",
    ).strip()


def stamp(text: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] {text}", flush=True)


stamp(f"before: {row()}")
for what, run in (
    ("teleport to Orgrimmar", lambda: tab.teleport(NAME, "Orgrimmar")),
    ("set level 20", lambda: tab.set_level(NAME, 20)),
    ("send 7 gold", lambda: tab.mail_gold(NAME, gold=7, subject="A gift", body="From the owner")),
    ("teleport to Stormwind", lambda: tab.teleport(NAME, "Stormwind")),
):
    out = run()
    stamp(f"{what}: done={out.done} {(out.text or out.problem).strip()[:80]!r}")
    time.sleep(30)
    stamp(f"   row now: {row()}")
