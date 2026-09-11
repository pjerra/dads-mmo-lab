"""Settle 8.4c's one blocked clause: the OFFLINE revive, made to order.

8.4c measured it once, on `Koldoum`, and could not repeat it: this server's only
offline corpse was the one it loaded at start-up, the measurement consumed it,
and no other appeared in twenty-five minutes of polling. Bots die and revive
themselves in seconds and none of them logs out dead. Its README says what
would settle it -- "log Asdff in, die, log out, press Revive" -- and that is
what this does, with the client lane driving the dying and the logging out.

Stages, each read back rather than assumed:

    where          position, health, level, corpse count -- the ground
    kill <where>   send the character somewhere lethal through the app's own
                   teleport, then watch it die (a `saveall` between readings,
                   because for an ONLINE character the row is not the truth)
    watch          poll the corpse row, so the corpse is seen to exist BEFORE
                   the command and seen to sit still while nothing touches it
    revive         press the app's Revive on the OFFLINE character
    after          what the row says once the client has logged in again

Nothing here writes a database row. The dying happens in the world, the
release happens in the client, and every press is a press of a Yu'lon button.

Usage:  python offline-revive-live.py <stage> [argument]
"""

from __future__ import annotations

import sys
import time
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate84c" / "pylauncher"))

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "vanilla-75b"
ENTRY = load_catalog().get("wow-vanilla")
WHO = "Asdffrenamed"
GUID = 901


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def services() -> ControllerServices:
    return ControllerServices.for_entry(ENTRY, SERVER_DIR)


def channel():
    return services().channel_setup.live_channel()


def sql(db: str, statement: str) -> str:
    from yulon.ui import controller_view as view_module

    reader = view_module._sql_for(
        ENTRY, view_module._db_password(ENTRY, SERVER_DIR), wsl_distro=None
    )
    return reader.query(db, statement).strip()


def row(save: bool = True) -> dict[str, str]:
    if save:
        channel().send("saveall")
        time.sleep(1.5)
    fields = sql(
        "characters",
        "SELECT level, health, online, ROUND(position_x,1), ROUND(position_y,1), map, "
        "(SELECT COUNT(*) FROM characters.corpse WHERE player = characters.guid), "
        "(SELECT IFNULL(MIN(corpse_type), -1) FROM characters.corpse WHERE player = characters.guid) "
        f"FROM characters.characters WHERE guid = {GUID};",
    ).split("\t")
    keys = ("level", "health", "online", "x", "y", "map", "corpses", "corpse_type")
    return dict(zip(keys, fields, strict=False))


def stage_where() -> None:
    say(f"{WHO}: {row()}")


def stage_kill(where: str) -> None:
    """Send the character somewhere lethal, through the app's own teleport."""
    before = row()
    say(f"before: {before}")
    assert before["online"] == "1", "the character has to be in the world to die in it"
    assert before["corpses"] == "0", (
        f"it already has {before['corpses']} corpses, so a corpse afterwards would prove nothing"
    )
    out = services().play.teleport(WHO, where)
    say(f"teleport -> {where}: done={out.done} {(out.text or out.problem).strip()[:120]!r}")
    assert out.done, out.problem
    deadline = time.monotonic() + 300
    while time.monotonic() < deadline:
        time.sleep(15)
        now = row()
        say(f"  {now['x']},{now['y']} map {now['map']}  health {now['health']}  corpses {now['corpses']}")
        if now["health"] == "0" or now["corpses"] != "0":
            say("it is dead")
            return
    say("still alive after five minutes -- pick somewhere worse, or drop its level")


def stage_watch() -> None:
    """A corpse must exist BEFORE the command, and must not go on its own.

    8.4c's own review caught the shape this guards against: an assertion that
    was already true, and a corpse that vanished by itself between the reading
    and the command.
    """
    first = row()
    say(f"t+0  : corpses={first['corpses']} type={first['corpse_type']} online={first['online']}")
    assert first["corpses"] != "0", "no corpse -- there is nothing for revive to remove"
    time.sleep(12)
    second = row()
    say(f"t+12 : corpses={second['corpses']} type={second['corpse_type']} online={second['online']}")
    assert second["corpses"] == first["corpses"], (
        "the corpse went on its own in twelve untouched seconds -- NOT RUN"
    )
    say("the corpse is still there after twelve untouched seconds")


def stage_revive() -> None:
    before = row()
    say(f"before: {before}")
    assert before["online"] == "0", "this stage is the OFFLINE half; log the client out first"
    assert before["corpses"] != "0", "no corpse before the command -- NOT RUN"
    out = services().play.revive(WHO)
    say(f"revive: done={out.done} {(out.text or out.problem).strip()[:120]!r}")
    deadline = time.monotonic() + 12
    gone = None
    while time.monotonic() < deadline:
        gone = row(save=False)
        if gone["corpses"] == "0":
            break
        time.sleep(0.5)
    say(f"after : {gone}")
    assert gone is not None and gone["corpses"] == "0", "the corpse survived the command"
    say("the corpse is gone within twelve seconds, and the character never logged in")


def stage_after() -> None:
    say(f"{WHO}: {row()}")


if __name__ == "__main__":
    what = sys.argv[1]
    if what == "kill":
        stage_kill(sys.argv[2])
    else:
        {"where": stage_where, "watch": stage_watch, "revive": stage_revive, "after": stage_after}[
            what
        ]()
