"""The 8.4a live gate: the Characters tab's seven actions, on WoW WotLK.

Usage:  python gate84a.py <stage>

    pick      choose an offline character and an online one, and say who
    offline   every action on the offline one, each read back in the row
    online    every action on the online one, each read back in the row
    case      a name typed in the wrong case still finds the character
    gear      a nineteen-piece set sends the number of mails the button promised
    takeover  set a password on the account owning the gate character, so a
              real client can log in and SEE the effects
    shots     the Characters tab, rendered from the real view

Every reading is taken from the machine: the row after each action, and the
server's own sentence. The client half is `client-play.ps1` on the Hyper-V host.
"""

from __future__ import annotations

import json
import os
import sys
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate81a" / "pylauncher"))

from yulon import play  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "wowserver"
ENTRY = load_catalog().get("wow-wotlk")
SHOTS = Path.home() / "gate84a-shots"
CHOSEN = Path.home() / "gate84a-chosen.json"
TELEPORT_TO = ("Orgrimmar", "Stormwind")
"""Two places, and the box uses whichever the character is NOT at.

A gate that always teleported to Stormwind passed the first time and failed the
second, on a character already standing there -- the app was right and the box
was wrong, which is the kind of failure that gets a working feature "fixed".
"""
NEW_LEVEL = 60
GOLD = 5
TAKEOVER_PASSWORD = "gate84a-p@ss"


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def services() -> ControllerServices:
    return ControllerServices.for_entry(ENTRY, SERVER_DIR)


def sql(db: str, statement: str) -> str:
    from yulon.ui import controller_view as view_module  # noqa: PLC0415

    reader = view_module._sql_for(
        ENTRY, view_module._db_password(ENTRY, SERVER_DIR), wsl_distro=None
    )
    return reader.query(db, statement).strip()


def schemas() -> dict[str, str]:
    return ENTRY.schema_map()


def row(name: str) -> dict[str, str]:
    """One character's row, by the columns this box reads back."""
    fields = sql(
        "characters",
        f"SELECT guid, name, level, online, position_x, position_y, map, at_login, money "
        f"FROM {schemas()['characters']}.characters WHERE name = '{name}';",
    ).split("\t")
    keys = ("guid", "name", "level", "online", "x", "y", "map", "at_login", "money")
    return dict(zip(keys, fields, strict=False))


def mails(name: str) -> int:
    return int(
        sql(
            "characters",
            f"SELECT COUNT(*) FROM {schemas()['characters']}.mail m "
            f"JOIN {schemas()['characters']}.characters c ON c.guid = m.receiver "
            f"WHERE c.name = '{name}';",
        )
        or 0
    )


def chosen() -> dict[str, str]:
    return json.loads(CHOSEN.read_text())


# -- who this box acts on ---------------------------------------------------


def stage_pick() -> None:
    """One offline character with a full set of gear, and one online.

    Both are bot characters, which is what this server has: 1001 of them and no
    people. That is not a shortcut -- a bot character is a row in exactly the
    same table, wearing exactly the same items, and the client half takes over
    its account so a person can see the effects.
    """
    def best_wearer(online: int) -> tuple[str, str]:
        answer = sql(
            "characters",
            f"SELECT c.name, COUNT(*) FROM {schemas()['characters']}.characters c "
            f"JOIN {schemas()['characters']}.character_inventory i ON i.guid = c.guid "
            f"WHERE c.online = {online} AND i.bag = 0 AND i.slot < 19 "
            "GROUP BY c.name ORDER BY COUNT(*) DESC, c.name LIMIT 1;",
        ).split(chr(9))
        return (answer[0], answer[1])

    # The best-dressed of each, because the gear clause needs a set that does
    # NOT fit in one mail and this server's offline characters wear twelve --
    # exactly the cap, which would prove nothing about the second mail.
    offline_name, offline_pieces = best_wearer(0)
    online_name, online_pieces = best_wearer(1)
    picked = {
        "offline": offline_name,
        "offline_pieces": offline_pieces,
        "online": online_name,
        "online_pieces": online_pieces,
    }
    CHOSEN.write_text(json.dumps(picked))
    say(f"offline: {offline_name} wearing {offline_pieces} pieces")
    say(f"online : {online_name} wearing {online_pieces} pieces")
    account = sql(
        "characters",
        f"SELECT account FROM {schemas()['characters']}.characters "
        f"WHERE name = '{picked['offline']}';",
    )
    username = sql("auth", f"SELECT username FROM {schemas()['auth']}.account WHERE id = {account};")
    say(f"the offline character's account is {username} (id {account})")


# -- the actions ------------------------------------------------------------


def settled(read, want, seconds: float = 20.0, nudge=None):
    """Wait for the server to write what it has already reported.

    Measured here, 2026-09-07: `character level` answers in about 0.15s and its
    own row lands about 0.1s after that. A gate that read the row the moment the
    command answered called a working teleport a failure -- and so would a tab
    that refreshed its list there, which is why `_ROW_SETTLE_MS` exists.
    """
    import time  # noqa: PLC0415

    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        got = read()
        if want(got):
            return got
        if nudge is not None:
            # For an ONLINE character the world holds the truth and the row is
            # written when the player is saved, so asking once before the world
            # has applied the change saves the OLD value. Asked each round.
            nudge()
        time.sleep(0.5)
    return read()


def save_everyone() -> None:
    """Make the world write what it is holding, so the rows can be read.

    Measured 2026-09-07: for an ONLINE character the effect of these commands
    is in the world and not yet in the row -- the teleport moved Airaani and
    `characters.position_x` did not change, because a logged-in player's row is
    written when the player is saved. AzerothCore saves on its own schedule
    (fifteen minutes by default), so this box asks for it: `.saveall` answers
    "All players saved." and the row is then the world's.

    This is the limit of 8.3b's "the row is the answer": it holds for a
    character nobody is playing, and for one who IS playing the world holds the
    truth until it is asked to write it down.
    """
    svc = services()
    assert svc.channel_setup is not None
    channel = svc.channel_setup.live_channel()
    assert channel is not None, "no live channel to ask for a save"
    answer = channel.send("saveall")
    say(f"saveall   : {answer.outcome} {(answer.text or '').strip()[:60]!r}")


def _each_action(name: str, *, online: bool) -> None:
    tab = services().play
    assert tab is not None, "this tree has no Characters tab wired"

    nudge = save_everyone if online else None

    before = row(name)
    say(f"{name} before: level {before['level']} at ({before['x']}, {before['y']}) "
        f"map {before['map']} at_login {before['at_login']} money {before['money']}")

    # Stormwind is around x=-8833; anywhere near it, go to the other one.
    at_stormwind = float(before["x"]) < -8000 and before["map"] == "0"
    where = TELEPORT_TO[0] if at_stormwind else TELEPORT_TO[1]
    out = tab.teleport(name, where)
    after = settled(lambda: row(name), lambda r: (r["x"], r["y"]) != (before["x"], before["y"]), nudge=nudge)
    say(f"teleport  : to {where}, done={out.done} {(out.text or out.problem).strip()[:90]!r}")
    say(f"            now at ({after['x']}, {after['y']}) map {after['map']}")
    assert out.done, out.problem
    assert (after["x"], after["y"]) != (before["x"], before["y"]), "the character did not move"

    out = tab.set_level(name, NEW_LEVEL)
    after = settled(lambda: row(name), lambda r: r["level"] == str(NEW_LEVEL), nudge=nudge)
    say(f"set level : done={out.done} {(out.text or out.problem).strip()[:90]!r}")
    say(f"            the row reads level {after['level']}")
    assert out.done, out.problem
    assert after["level"] == str(NEW_LEVEL), after

    out = tab.rename(name)
    after = settled(lambda: row(name), lambda r: int(r["at_login"] or 0) & 1, nudge=nudge)
    say(f"rename    : done={out.done} at_login is now {after['at_login']}")
    assert out.done, out.problem
    assert int(after["at_login"]) & 1, "the rename flag is not set on the row"

    out = tab.revive(name)
    say(f"revive    : done={out.done} {(out.text or out.problem).strip()[:90]!r}")

    had = mails(name)
    out = tab.mail_gold(name, gold=GOLD, subject="A gift", body="From the server owner")
    now = settled(lambda: mails(name), lambda n: n > had)
    say(f"send gold : done={out.done} mails {had} -> {now}")
    assert out.done, out.problem
    assert now == had + 1, "no mail arrived"


def stage_offline() -> None:
    """Every action on a character who is NOT logged in."""
    name = chosen()["offline"]
    say(f"the offline character: {name}")
    assert row(name)["online"] == "0", "this character is online after all"
    _each_action(name, online=False)
    say("PASSED: every action on an offline character, each read back in the row")


def stage_online() -> None:
    """The same, on one who IS -- where the server also has to tell the client."""
    name = chosen()["online"]
    say(f"the online character: {name}")
    assert row(name)["online"] == "1", "this character is not online"
    _each_action(name, online=True)
    say("PASSED: every action on an online character, each read back in the row")


def stage_case() -> None:
    """A name typed in the wrong case still finds the character.

    This tree's `characters.name` is case-sensitive and so is the server's own
    lookup: the prior art answered "not online" for a character standing in
    Stormwind. The tab canonicalises first, so what reaches the server is the
    stored spelling whatever was typed.
    """
    name = chosen()["offline"]
    tab = services().play
    assert tab is not None
    for typed in (name.lower(), name.upper(), name.swapcase()):
        out = tab.revive(typed)
        say(f"typed {typed!r}: done={out.done} {(out.text or out.problem).strip()[:70]!r}")
        assert out.done, f"{typed!r} was not found: {out.problem}"
    missing = tab.revive("NoSuchPersonHere")
    say(f"typed a name nobody has: done={missing.done} {missing.problem[:80]!r}")
    assert missing.done is False
    say("PASSED: every case finds the character, and a name nobody has is refused here")


def stage_gear() -> None:
    """A set larger than one mail sends the mails the button promised.

    The ONLINE character, because this server's offline ones wear exactly
    twelve pieces -- the cap itself, which would prove nothing about a second
    mail. The online ones wear nineteen.
    """
    name = chosen()["online"]
    tab = services().play
    assert tab is not None
    pieces, promised = tab.gear_set_size(name)
    had = mails(name)
    say(f"{name} is wearing {pieces} pieces, and the button promises {promised} mails")
    out = tab.send_gear_set(name, to=name, subject="Your gear", body="Everything you were wearing")
    arrived = settled(lambda: mails(name), lambda n: n >= had + promised) - had
    say(f"send gear : done={out.done} {(out.text or out.problem).strip()[:120]!r}")
    say(f"            mails that arrived: {arrived}")
    assert out.done, out.problem
    assert arrived == promised, f"promised {promised}, {arrived} arrived"
    say(f"clause PASSED: {pieces} pieces, {promised} mails promised, {arrived} arrived")


def stage_takeover() -> None:
    """Give the gate character's account a password, so a client can log in.

    Through the app's own 8.3a button, not by writing a row -- which is also a
    second live press of that feature.
    """
    name = chosen()["offline"]
    account_id = sql(
        "characters",
        f"SELECT account FROM {schemas()['characters']}.characters WHERE name = '{name}';",
    )
    username = sql(
        "auth", f"SELECT username FROM {schemas()['auth']}.account WHERE id = {account_id};"
    )
    admin = services().accounts
    assert admin is not None
    out = admin.set_password(username, TAKEOVER_PASSWORD)
    say(f"set_password on {username}: done={out.done} {(out.text or out.problem).strip()[:90]!r}")
    assert out.done, out.problem
    say(f"THE CLIENT HALF: log in as {username} / {TAKEOVER_PASSWORD} and look at {name}")


def stage_shots() -> None:
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    app = QApplication.instance() or QApplication([])
    view = ControllerView(ENTRY, services(), status_poll_ms=0, job_runner=run_inline)
    view.resize(980, 700)
    view._tabs.setCurrentIndex(
        next(i for i in range(view._tabs.count()) if view._tabs.tabText(i) == "Characters")
    )
    view.refresh_characters()
    SHOTS.mkdir(parents=True, exist_ok=True)
    view.grab().save(str(SHOTS / "1-characters.png"))
    say(f"the list holds {view.character_list.count()} characters")

    name = chosen()["offline"]
    match = [
        i
        for i in range(view.character_list.count())
        if view.character_list.item(i).text().startswith(name)
    ]
    view.character_list.setCurrentRow(match[0])
    view.grab().save(str(SHOTS / "2-chosen.png"))
    say(f"chosen: {view.character_list.currentItem().text()!r}")
    for button in view.character_buttons():
        say(f"   button: {button.text()!r} enabled={button.isEnabled()}")

    view.teleport_where.setText(TELEPORT_TO[0])
    view.teleport_character()
    view.grab().save(str(SHOTS / "3-teleported.png"))
    say(f"after the teleport the tab says: {view.character_report.text()!r}")
    del app


if __name__ == "__main__":
    {
        "pick": stage_pick,
        "offline": stage_offline,
        "online": stage_online,
        "case": stage_case,
        "gear": stage_gear,
        "takeover": stage_takeover,
        "shots": stage_shots,
    }[sys.argv[1]]()
