"""8.4b's client half: the actions, on a character a person is playing."""
import sys
import time
from pathlib import Path

sys.path.insert(0, "/home/pk/gate83b/pylauncher")
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui import controller_view as v  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

E = load_catalog().get("wow-tbc")
D = Path.home() / "tbc-7.4c"
NAME = "Ddsasd"
sql = v._sql_for(E, v._db_password(E, D), wsl_distro=None)
tab = ControllerServices.for_entry(E, D).play
Q = chr(39)


def row() -> str:
    return sql.query(
        "characters",
        f"SELECT level, online, map FROM characters.characters WHERE name = {Q}{NAME}{Q};",
    ).strip()


def stamp(text: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] {text}", flush=True)


stamp(f"before: {row()}")
for what, run in (
    ("teleport to Stormwind", lambda: tab.teleport(NAME, "Stormwind")),
    ("set level 25", lambda: tab.set_level(NAME, 25)),
    ("send 9 gold", lambda: tab.mail_gold(NAME, gold=9, subject="A gift", body="From the owner")),
):
    out = run()
    stamp(f"{what}: done={out.done} {(out.text or out.problem).strip()[:70]!r}")
    time.sleep(30)
    stamp(f"   row now: {row()}")
