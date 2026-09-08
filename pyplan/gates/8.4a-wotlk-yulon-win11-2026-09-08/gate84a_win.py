"""8.4a live gate, the WINDOWS half: the Characters tab's actions on WoW WotLK,
pressed on a real Windows install.

This is `pyplan/gates/8.4a-wotlk-yulon-ubuntu-2026-09-07/gate84a.py` with its
constants changed -- the source root, the server dir, the shots directory, the
activity log -- plus the ground-clearing the adversarial review demanded of that
box AFTER its script was committed. The committed Linux script still sets level
60 on whatever level the character happens to be and sets the rename flag on
whatever `at_login` happens to be; both assertions can be true before their
command runs. Every stage below reads the ground, records it, and REFUSES to
run a step whose result is the state it started in.

Usage:  python gate84a_win.py <stage>

    pick      choose an offline character and a named online one, and say who
    offline   every action the tab draws for an OFFLINE character, ground first
    online    the same on one standing in the world, plus the revive clause
              proved the only way it can be: kill it, then bring it back
    case      a name typed in the wrong case still finds the character
    gear      a set larger than one mail sends the mails the button promised
    withheld  the Revive button is absent for an offline character and says why
    takeover  set a password on the client account, through the app's own button
    shots     the Characters tab, rendered from the real view

Nothing here fakes a seam that reaches the machine: the real `ControllerServices`,
the real SQL, the real command channel, the real `ControllerView`.
"""

from __future__ import annotations

import json
import os
import platform as pyplatform
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
# 8.9a's Windows half measured this on this very box: Qt's offscreen platform
# uses its own font database and PySide6 ships no fonts, so without a font
# directory every glyph in every screenshot is a tofu box -- an artefact that
# shows nothing while looking like evidence.
os.environ.setdefault("QT_QPA_FONTDIR", r"C:\Windows\Fonts")

HERE = Path(__file__).resolve().parent
sys.path.insert(0, r"C:\gate\src84a\pylauncher")

SERVER_DIR = Path(r"D:\wow-server")
SHOTS = HERE / "shots"
CHOSEN = Path(r"C:\gate\gate84a-chosen.json")
ACTIVITY = Path(r"C:\gate\gate84a-activity.log")

TELEPORT_TO = ("Orgrimmar", "Stormwind")
"""Two places, and the box uses whichever the character is NOT at.

The Linux box's own note: a gate that always teleported to Stormwind passed the
first time and failed the second, on a character already standing there -- the
app was right and the box was wrong.
"""
GOLD = 5
TAKEOVER_PASSWORD = "gate84w-p@ss"

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

ENTRY = load_catalog().get("wow-wotlk")


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def announce(text: str) -> None:
    """This box has no `claude-say`; the announcement goes to a file and the log."""
    say(f"ANNOUNCE: {text}")
    try:
        ACTIVITY.parent.mkdir(parents=True, exist_ok=True)
        with ACTIVITY.open("a", encoding="utf-8") as handle:
            handle.write(f"[{stamp()}] {text}\n")
    except OSError as exc:  # pragma: no cover - the announcement is not the gate
        say(f"(could not write the activity log: {exc})")


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


ROW_KEYS = ("guid", "name", "level", "online", "x", "y", "map", "at_login", "money", "health")


def row(name: str) -> dict[str, str]:
    """One character's row, by the columns this box reads back."""
    fields = sql(
        "characters",
        f"SELECT guid, name, level, online, position_x, position_y, map, at_login, money, health "
        f"FROM {schemas()['characters']}.characters WHERE name = '{name}';",
    ).split("\t")
    return dict(zip(ROW_KEYS, fields, strict=False))


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


def ground(title: str, name: str) -> dict[str, str]:
    """Read the state an action is about to change, and print it.

    Named `ground` rather than `before` because a frame or a line called
    "before" that carries the post-press state is the artefact 8.2d shipped.
    """
    before = row(name)
    say(f"===== GROUND: {title} -- {name} BEFORE the press =====")
    for key in ROW_KEYS:
        say(f"    {key:9s} = {before.get(key)!r}")
    say(f"    mails     = {mails(name)}")
    say("===== end ground =====")
    return before


def channel_send(command: str) -> object:
    svc = services()
    assert svc.channel_setup is not None
    channel = svc.channel_setup.live_channel()
    assert channel is not None, "no live channel on this install"
    answer = channel.send(command)
    say(f"  channel  : {command!r} -> {answer.outcome} {(answer.text or '').strip()[:80]!r}")
    return answer


def settled(read, want, seconds: float = 25.0, nudge=None):
    """Wait for the server to write what it has already reported.

    Measured on the Linux box 2026-09-07: `character level` answers in about
    0.15s and its own row lands about 0.1s after that, which is why
    `_ROW_SETTLE_MS` exists at all.
    """
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        got = read()
        if want(got):
            return got
        if nudge is not None:
            nudge()
        time.sleep(0.5)
    return read()


def save_everyone() -> None:
    """Make the world write down what it is holding, so the row can be read.

    For an ONLINE character the effect of these commands is in the world and
    not yet in the row: a logged-in player's row is written when the player is
    saved, and AzerothCore saves on its own schedule. This is the limit of
    8.3b's "the row is the answer".
    """
    channel_send("saveall")


# -- who this box acts on ---------------------------------------------------


def stage_pick() -> None:
    """The offline character, and the online one the client half is driving.

    The online name is given on the command line, because on this box the ONLY
    character that is not a bot is the one a person made through the client --
    and 8.4a already measured that an online BOT is no good for the online
    clause: the playerbots module rewrites its level within the minute.
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

    offline_name, offline_pieces = best_wearer(0)
    online_name = sys.argv[2] if len(sys.argv) > 2 else ""
    picked = {
        "offline": offline_name,
        "offline_pieces": offline_pieces,
        "online": online_name,
    }
    CHOSEN.write_text(json.dumps(picked))
    say(f"offline: {offline_name} wearing {offline_pieces} pieces")
    say(f"online : {online_name or '(none yet -- the client half names it)'}")
    account = sql(
        "characters",
        f"SELECT account FROM {schemas()['characters']}.characters WHERE name = '{offline_name}';",
    )
    username = sql("auth", f"SELECT username FROM {schemas()['auth']}.account WHERE id = {account};")
    say(f"the offline character's account is {username} (id {account})")


# -- the actions ------------------------------------------------------------


def _teleport(tab, name: str, before: dict[str, str], nudge) -> None:
    at_stormwind = float(before["x"]) < -8000 and before["map"] == "0"
    where = TELEPORT_TO[0] if at_stormwind else TELEPORT_TO[1]
    say(f"teleport  : the character is NOT at {where}, so this move can be seen")
    out = tab.teleport(name, where)
    after = settled(
        lambda: row(name),
        lambda r: (r["x"], r["y"]) != (before["x"], before["y"]),
        nudge=nudge,
    )
    say(f"teleport  : to {where}, done={out.done} {(out.text or out.problem).strip()[:90]!r}")
    say(f"            ({before['x']}, {before['y']}) map {before['map']}"
        f"  ->  ({after['x']}, {after['y']}) map {after['map']}")
    assert out.done, out.problem
    assert (after["x"], after["y"]) != (before["x"], before["y"]), "the character did not move"


def _set_level(tab, name: str, before: dict[str, str], nudge) -> None:
    was = int(before["level"])
    want = 60 if was != 60 else 55
    assert want != was, "this step would assert the state it starts in"
    say(f"set level : the row reads {was}, so the press asks for {want}")
    out = tab.set_level(name, want)
    after = settled(lambda: row(name), lambda r: r["level"] == str(want), nudge=nudge)
    say(f"set level : done={out.done} {(out.text or out.problem).strip()[:90]!r}")
    say(f"            level {was} -> {after['level']}")
    assert out.done, out.problem
    assert after["level"] == str(want), after


def _rename(tab, name: str, nudge, *, read_row: bool = True) -> None:
    """Clear the flag first, so the press is what sets it.

    The review's finding on the Linux box, in one function: the character was
    already flagged when the rename flag was set, and the assertion passed
    without demonstrating anything.

    `read_row=False` for a character who is logged in. Measured here: the
    command answers `done=True` with no text at all and `at_login` stays 0 for
    the twenty-five seconds this box waited -- the same place the level goes,
    because for a live player this tree writes the row at logout. The online
    rename's readings are the client's own prompt at the next login and the row
    after that logout (`stage_logout`), not a row read at the press.
    """
    flag = int(row(name)["at_login"] or 0)
    if flag & 1:
        say(f"rename    : at_login is {flag}, which ALREADY carries the flag -- clearing it first")
        sql(
            "characters",
            f"UPDATE {schemas()['characters']}.characters SET at_login = at_login & ~1 "
            f"WHERE name = '{name}';",
        )
        flag = int(row(name)["at_login"] or 0)
    say(f"rename    : ground at_login = {flag}, flag bit clear = {not flag & 1}")
    assert not flag & 1, "the ground was not cleared; this press would prove nothing"
    out = tab.rename(name)
    if not read_row:
        say(f"rename    : done={out.done} {(out.text or out.problem).strip()[:80]!r} "
            f"-- the row is read after the logout, not here")
        assert out.done, out.problem
        return
    after = settled(lambda: row(name), lambda r: int(r["at_login"] or 0) & 1, nudge=nudge)
    say(f"rename    : done={out.done} at_login {flag} -> {after['at_login']}")
    assert out.done, out.problem
    assert int(after["at_login"]) & 1, "the rename flag is not set on the row"


def _mail_gold(tab, name: str) -> None:
    had = mails(name)
    say(f"send gold : ground -- {name} has {had} mails")
    out = tab.mail_gold(name, gold=GOLD, subject="A gift", body="From the server owner")
    now = settled(lambda: mails(name), lambda n: n > had)
    say(f"send gold : done={out.done} mails {had} -> {now}")
    assert out.done, out.problem
    assert now == had + 1, "no mail arrived"


def stage_offline() -> None:
    """Every action the tab draws for a character who is NOT logged in.

    Revive is not one of them, and that is the shipped behaviour rather than an
    omission: 8.4a measured that an offline revive on AzerothCore answers
    success and does nothing. `withheld` is the stage that proves the button is
    gone and says why.
    """
    name = chosen()["offline"]
    announce(f"8.4a Windows: every offline action on {name}, ground read first")
    before = ground("the offline arm", name)
    assert before["online"] == "0", "this character is online after all"
    tab = services().play
    assert tab is not None, "this tree has no Characters tab wired"
    _teleport(tab, name, before, None)
    _set_level(tab, name, before, None)
    _rename(tab, name, None)
    _mail_gold(tab, name)
    say("PASSED: every action the tab draws offline, each read back in the row")


def pinfo(name: str) -> dict[str, str]:
    """What the WORLD says about a live character, in the server's own words.

    The online arm is read here rather than in the row, and that is a
    measurement this box had to make rather than inherit. 8.4a's Linux gate
    nudged the row along with `saveall` and asserted on it. On THIS tree
    `saveall` answers *"All players saved."* and does not write a logged-in
    player's level: the world reported level 61, four `saveall`s over twenty
    seconds left the row at 60, and the row caught up 178 seconds later with
    nothing asked of the server at all -- at `logout_time`, which is the write
    that actually moved it (`the-row-and-the-world.py`, `watch_row.py`).

    A nudge that does not nudge is worse than no nudge: it makes a gate fail on
    the app's behalf. So the live reading is `pinfo`, which is the server
    answering about its own world, and the row is asserted after the logout
    that writes it (`logout` stage).
    """
    answer = channel_send(f"pinfo {name}")
    fields: dict[str, str] = {}
    for line in (getattr(answer, "text", "") or "").splitlines():
        stripped = line.strip().lstrip("|").strip()
        if ":" in stripped:
            key, _, value = stripped.partition(":")
            fields.setdefault(key.strip(), value.strip().rstrip(","))
    return fields


def stage_online() -> None:
    """Every action on a character standing in the world, read from the world.

    Revive is proved the only way it can be proved: the character is KILLED
    first (`.die` through the same channel), the world is read as dead, and the
    button then brings it back. An assertion on a living character is true
    before the press.
    """
    name = chosen()["online"]
    assert name, "stage_pick was not given the online character's name"
    announce(f"8.4a Windows: every online action on {name}, ground read first")
    before = ground("the online arm", name)
    assert before["online"] == "1", f"{name} is not logged in"
    tab = services().play
    assert tab is not None
    world_before = pinfo(name)
    say(f"===== GROUND from the world: zone={world_before.get('Zone')!r} "
        f"level={world_before.get('Level')!r} alive={world_before.get('Alive ?')!r} "
        f"mails={world_before.get('Mails')!r} =====")

    # -- teleport
    at_stormwind = "Stormwind" in (world_before.get("Map") or "")
    where = TELEPORT_TO[0] if at_stormwind else TELEPORT_TO[1]
    say(f"teleport  : the world says {world_before.get('Map')!r}, so the press asks for {where}")
    out = tab.teleport(name, where)
    time.sleep(3)
    world_after = pinfo(name)
    say(f"teleport  : done={out.done} {(out.text or out.problem).strip()[:80]!r}")
    say(f"            Map {world_before.get('Map')!r} -> {world_after.get('Map')!r}")
    assert out.done, out.problem
    assert world_after.get("Map") != world_before.get("Map"), "the world did not move the character"

    # -- level
    was = int((world_before.get("Level") or "0").split()[0])
    want = 60 if was != 60 else 55
    say(f"set level : the world says level {was}, so the press asks for {want}")
    out = tab.set_level(name, want)
    time.sleep(3)
    world_after = pinfo(name)
    got = int((world_after.get("Level") or "0").split()[0])
    say(f"set level : done={out.done} {(out.text or out.problem).strip()[:80]!r}")
    say(f"            the world reads level {was} -> {got}")
    assert out.done, out.problem
    assert got == want, f"the world still reads {got}"

    # -- mailed gold; mail rows ARE written at once, so this one is the row's
    had = mails(name)
    say(f"send gold : ground -- {name} has {had} mail rows and the world says "
        f"{pinfo(name).get('Mails')!r}")
    out = tab.mail_gold(name, gold=GOLD, subject="A gift", body="From the server owner")
    now = settled(lambda: mails(name), lambda n: n > had)
    say(f"send gold : done={out.done} mail rows {had} -> {now}")
    assert out.done, out.problem
    assert now == had + 1, "no mail arrived"

    # -- revive, with a ground that is not already the answer
    # The kill is the CLIENT's: `client-play.ps1 -Die` targets self with F1 and
    # sends `.die` from the character's own chat line, on an account the app's
    # own 8.3a button put at GM level 3. This line is the probe that says why,
    # and its refusal is the measurement: a SOAP session selects nothing, so the
    # one command that kills cannot be reached from the console at all.
    say("revive    : the client has already killed the character; this asks the console too")
    channel_send(f"die {name}")
    time.sleep(3)
    dead = pinfo(name)
    say(f"revive    : ground -- the world says alive={dead.get('Alive ?')!r}")
    assert dead.get("Alive ?") == "No", "the character did not die; a revive here proves nothing"
    out = tab.revive(name)
    time.sleep(3)
    alive = pinfo(name)
    say(f"revive    : done={out.done} {(out.text or out.problem).strip()[:80]!r}")
    say(f"            alive {dead.get('Alive ?')!r} -> {alive.get('Alive ?')!r}")
    assert out.done, out.problem
    assert alive.get("Alive ?") == "Yes", "the character is still dead"

    # Rename LAST: the prompt it raises is consumed by the next login, and the
    # character cannot enter the world again until it is answered. The rename
    # flag is one of the columns the command writes DIRECTLY, so the row is the
    # reading here and the ground is cleared first.
    _rename(tab, name, None, read_row=False)
    expected = {"level": str(want), "map": "0" if where == "Stormwind" else "1", "where": where}
    say(f"what the row should carry after the logout: {expected}")
    Path(r"C:\gate\gate84a-online-expected.json").write_text(json.dumps(expected))
    say("PASSED: every action on a character being played, read from the world itself")


def stage_logout() -> None:
    """The row, after the logout that writes it.

    The definition of done asks that the action *changes the named row*. For a
    character nobody is playing that is true at the press; for one who IS being
    played this tree writes the row at `logout_time`, so the reading belongs
    after the client has gone -- and this stage refuses to run while it is still
    there, rather than reporting a row nobody has written yet.
    """
    name = chosen()["online"]
    expected = json.loads(Path(r"C:\gate\gate84a-online-expected.json").read_text())
    want = expected["level"]
    now = row(name)
    say(f"===== GROUND: {name} online={now['online']!r}, what the world was given: {expected} =====")
    assert now["online"] == "0", "the character is still logged in; the row is not written yet"
    logout = sql(
        "characters",
        f"SELECT FROM_UNIXTIME(logout_time) FROM {schemas()['characters']}.characters "
        f"WHERE name = '{name}';",
    )
    say(f"the row now: level {now['level']}, map {now['map']}, at_login {now['at_login']}, "
        f"health {now['health']}, logout_time {logout}")
    assert now["level"] == want, f"the row reads {now['level']}, not the {want} the world was given"
    assert int(now["at_login"]) & 1, "the rename flag is not on the row"
    assert now["map"] == expected["map"], (
        f"the row reads map {now['map']}, not the {expected['where']} the world was sent to"
    )
    say("PASSED: the row carries every online action, written at the logout")


def stage_case() -> None:
    """A name typed in the wrong case still finds the character.

    This tree's `characters.name` is case-sensitive and so is the server's own
    lookup. `teleport` rather than `revive` here, because on THIS tree revive is
    refused for an offline character -- a refusal that would hide the answer the
    stage is asking for.
    """
    name = chosen()["offline"]
    tab = services().play
    assert tab is not None
    announce(f"8.4a Windows: three spellings of {name}")
    where = TELEPORT_TO[1] if float(row(name)["x"]) > -8000 else TELEPORT_TO[0]
    for typed in (name.lower(), name.upper(), name.swapcase()):
        out = tab.teleport(typed, where)
        say(f"typed {typed!r}: done={out.done} {(out.text or out.problem).strip()[:70]!r}")
        assert out.done, f"{typed!r} was not found: {out.problem}"
    missing = tab.teleport("NoSuchPersonHere", where)
    say(f"typed a name nobody has: done={missing.done} {missing.problem[:90]!r}")
    assert missing.done is False
    say("PASSED: every case finds the character, and a name nobody has is refused here")


def stage_gear() -> None:
    """A set larger than one mail sends the mails the button promised."""
    name = sys.argv[2] if len(sys.argv) > 2 else chosen()["offline"]
    tab = services().play
    assert tab is not None
    announce(f"8.4a Windows: {name}'s gear set, against the number the button promises")
    pieces, promised = tab.gear_set_size(name)
    had = mails(name)
    say(f"===== GROUND: {name} wears {pieces} pieces, has {had} mails, "
        f"and the button promises {promised} mails =====")
    assert pieces > ENTRY.play.mail_item_cap, (
        f"{pieces} pieces fits in one mail on a cap of {ENTRY.play.mail_item_cap}; "
        "this stage would prove nothing about a second mail"
    )
    out = tab.send_gear_set(name, to=name, subject="Your gear", body="Everything you were wearing")
    arrived = settled(lambda: mails(name), lambda n: n >= had + promised, seconds=40.0) - had
    say(f"send gear : done={out.done} {(out.text or out.problem).strip()[:120]!r}")
    say(f"            mails that arrived: {arrived}")
    assert out.done, out.problem
    assert arrived == promised, f"promised {promised}, {arrived} arrived"
    say(f"PASSED: {pieces} pieces, {promised} mails promised, {arrived} arrived")


def stage_withheld() -> None:
    """The Revive button, for an offline character, is drawn disabled and says why.

    Read from the real view rather than from the catalog: the fact the view
    consults (`revive_offline`) is null on this tree, and the sentence a person
    reads is the thing being gated.
    """
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    announce("8.4a Windows: the Revive button withheld for an offline character")
    app = QApplication.instance() or QApplication([])
    view = ControllerView(ENTRY, services(), status_poll_ms=0, job_runner=run_inline)
    view.resize(1000, 720)
    view._tabs.setCurrentIndex(
        next(i for i in range(view._tabs.count()) if view._tabs.tabText(i) == "Characters")
    )
    view.refresh_characters()
    name = chosen()["offline"]
    match = [
        i
        for i in range(view.character_list.count())
        if view.character_list.item(i).text().startswith(name)
    ]
    assert match, f"{name} is not in the list the tab read"
    view.character_list.setCurrentRow(match[0])
    SHOTS.mkdir(parents=True, exist_ok=True)
    view.grab().save(str(SHOTS / "4-revive-withheld-offline.png"))
    say(f"the offline character {name}: revive enabled={view.revive_button.isEnabled()} "
        f"label={view.revive_button.text()!r}")
    assert not view.revive_button.isEnabled()
    assert "logged in" in view.revive_button.text()
    say("PASSED: the button is withheld and the sentence says why")
    del app


def stage_takeover() -> None:
    """Give an account a password, through the app's own 8.3a button."""
    username = sys.argv[2]
    admin = services().accounts
    assert admin is not None
    announce(f"8.4a Windows: setting {username}'s password through the app's own button")
    out = admin.set_password(username, TAKEOVER_PASSWORD)
    say(f"set_password on {username}: done={out.done} {(out.text or out.problem).strip()[:90]!r}")
    assert out.done, out.problem
    say(f"THE CLIENT HALF: log in as {username} / {TAKEOVER_PASSWORD}")


def stage_shots() -> None:
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    announce("8.4a Windows: the Characters tab, rendered from the real view")
    app = QApplication.instance() or QApplication([])
    view = ControllerView(ENTRY, services(), status_poll_ms=0, job_runner=run_inline)
    view.resize(1000, 720)
    view._tabs.setCurrentIndex(
        next(i for i in range(view._tabs.count()) if view._tabs.tabText(i) == "Characters")
    )
    view.refresh_characters()
    SHOTS.mkdir(parents=True, exist_ok=True)
    view.grab().save(str(SHOTS / "1-characters.png"))
    say(f"the list holds {view.character_list.count()} characters")

    name = sys.argv[2] if len(sys.argv) > 2 else chosen()["offline"]
    match = [
        i
        for i in range(view.character_list.count())
        if view.character_list.item(i).text().startswith(name)
    ]
    assert match, f"{name} is not in the list"
    view.character_list.setCurrentRow(match[0])
    view.grab().save(str(SHOTS / "2-chosen.png"))
    say(f"chosen: {view.character_list.currentItem().text()!r}")
    for button in view.character_buttons():
        say(f"   button: {button.text()!r} enabled={button.isEnabled()}")

    where = TELEPORT_TO[1] if float(row(name)["x"]) > -8000 else TELEPORT_TO[0]
    say(f"the tab will teleport to {where}, which is not where {name} stands")
    view.teleport_where.setText(where)
    view.teleport_character()
    view.grab().save(str(SHOTS / "3-teleported.png"))
    say(f"after the teleport the tab says: {view.character_report.text()!r}")
    del app


if __name__ == "__main__":
    say(f"### 8.4a WINDOWS gate driver, stage {sys.argv[1]}, {datetime.now(UTC).isoformat()} ###")
    say(f"### box: {pyplatform.node()}, {pyplatform.platform()}, tree: C:\\gate\\src84a ###")
    {
        "pick": stage_pick,
        "offline": stage_offline,
        "online": stage_online,
        "case": stage_case,
        "gear": stage_gear,
        "logout": stage_logout,
        "withheld": stage_withheld,
        "takeover": stage_takeover,
        "shots": stage_shots,
    }[sys.argv[1]]()
