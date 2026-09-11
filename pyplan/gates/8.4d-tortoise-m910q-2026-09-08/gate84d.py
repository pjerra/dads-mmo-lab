"""The 8.4d live gate: Play, on WoW Tortoise, over the attach console.

RUN 2026-09-08 on m910q against the live Tortoise stack. `transcript.txt` beside
this file is the output with the clock on every line and `README.md` is the
record.

Usage:  ~/gate81b-venv/bin/python gate84d.py <stage>

    ground      G2 + G3: the console answers, and the subjects are read and pinned
    levelroute  S1: the four console routes to a level that do NOT reach one
    initprobe   S1: `rndbot init`, the closest thing this tree has to a set-level
    levelcreate S1: the fifth, which DOES -- by making a character (it writes one)
    absent      S2: the tab draws no set-level control, and the sentence in its place
    resetlevel  S3: `reset level` online (works) and offline (refuses)
    teleport    S4: this tree's `tele name`, and a place the server does not know
    rename      S5: this fork's TOP-LEVEL `rename`, sent by the app, on an ONLINE character
    renameoff   S6: the offline rename WITHHELD, at the button and at the seam
    revive      S7: an offline revive that removes a corpse
    gear        S8: one item per message -- N worn pieces arrive as N mails
    gold        S9: a successful send, reported as one
    catchup     S10: the list shows the new state within its bounded re-reads
    flatread    S11: the equipped read is this fork's FLAT one, ids that exist
    deadchannel S12: LEAVES THE SERVER DOWN, then puts it back. Run it LAST.

    dorta-setup <place> <gold>   the client half's ground: teleport + gold to Dorta
    dorta-read                   read Dorta's row back
    dorta-rename                 flag Dorta for rename (AFTER the /who session)

READ-MOSTLY. Nine stages write to the server through the app's own seam, which
is what a Play gate is; nothing here edits a conf, and only `deadchannel` stops
a container -- and it starts it again.

Every stage takes GROUND first and asserts a CHANGE. `RandomPlayerbotMgr`
re-levels and re-positions its bots at runtime, so every reading carries its
clock and a subject is re-read immediately before it is acted on.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate84d" / "pylauncher"))

from yulon import play as play_module  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.controller_wow_tortoise import console as tortoise_console  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "tortoise-server"
ENTRY = load_catalog().get("wow-tortoise")
_SQL_TRACE = re.compile(r"^\s*\[\d+ ms\] SQL: ")

# NOT `~/gate84d-shots`: 8.4c's client lane already owns that directory on this
# box and its 60 frames are in it. A gate that writes into a folder another
# gate's evidence lives in is one whose folder cannot be copied wholesale.
SHOTS = Path.home() / "gate84d-tortoise-shots"
PINS = Path.home() / "gate84d-pins.json"


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def reader():
    from yulon.ui import controller_view as view_module  # noqa: PLC0415

    password = view_module._db_password(ENTRY, SERVER_DIR)
    return view_module._sql_for(ENTRY, password, wsl_distro=None)


def services() -> ControllerServices:
    return ControllerServices.for_entry(ENTRY, SERVER_DIR)


def sql(statement: str) -> str:
    """A hand query through the app's own reader.

    Shares the transport with the code under test on purpose: it proves the SQL
    and not the connection. The genuinely independent reading is the one typed
    at a shell and pasted into `transcript.txt`.
    """
    return reader().query("characters", statement).strip()


def row(guid: int) -> dict[str, str]:
    """One character's whole row, as the columns every stage below asserts on."""
    text = sql(
        "SELECT guid,name,level,map,zone,online,at_login,"
        "ROUND(position_x,2),ROUND(position_y,2),ROUND(position_z,2) "
        f"FROM tw_char.characters WHERE guid = {guid};"
    )
    if not text:
        return {}
    fields = text.split("\t")
    keys = ("guid", "name", "level", "map", "zone", "online", "at_login", "x", "y", "z")
    return dict(zip(keys, fields, strict=False))


def mails(guid: int) -> int:
    return int(sql(f"SELECT COUNT(*) FROM tw_char.mail WHERE receiver = {guid};") or 0)


def worn(guid: int) -> int:
    return int(
        sql(
            "SELECT COUNT(*) FROM tw_char.character_inventory "
            f"WHERE guid = {guid} AND bag = 0 AND slot < 19;"
        )
        or 0
    )


def corpse_rows(guid: int) -> int:
    return int(sql(f"SELECT COUNT(*) FROM tw_char.corpse WHERE player = {guid};") or 0)


def console(command: str, window: float = 8.0) -> tuple[bool, str]:
    """Type one line at the worldserver console and return (prompted, text).

    The gate's OWN channel, deliberately separate from the app's: a stage that
    asks the server a question through the seam under test cannot then use the
    answer as a check on that seam.
    """
    reply = tortoise_console.send(command, window=window)
    return bool(reply.prompted), "\n".join(reply.lines)


def quiet(text: str) -> list[str]:
    """A console reply with THIS install's SQL trace dropped.

    `mangosd.conf` has SQL logging on here, so 500 bots put hundreds of
    `[0 ms] SQL: ...` lines into every reply window and the command's own answer
    is one line among them. Dropped for the TRANSCRIPT only -- every assertion
    in this file is made against the full text.
    """
    return [line for line in text.splitlines() if not _SQL_TRACE.match(line)]


def start_level() -> int:
    """`StartPlayerLevel` from THIS install's conf, read at the time of the run."""
    conf = (SERVER_DIR / "etc" / "mangosd.conf").read_text(encoding="utf-8", errors="replace")
    for line in conf.splitlines():
        stripped = line.strip()
        if stripped.startswith("StartPlayerLevel"):
            return int(stripped.split("=", 1)[1].strip())
    raise AssertionError("no StartPlayerLevel in etc/mangosd.conf")


def play() -> play_module.InstallPlay:
    made = services()
    assert made.play is not None, "the Tortoise entry carries no play block"
    return made.play


def view_for(made: ControllerServices, entry=None):
    """The real window, on the entry it is about.

    `entry` is a parameter and not `ENTRY` because of a mistake this gate made
    on its first run: `absent`'s GROUND builds a view for the tree that HAS a
    set-level control, and with the entry hard-coded it built a TORTOISE view
    wearing WotLK's services -- so the ground photographed the absence it was
    there to contrast with, and the stage failed on the app's behalf. Whatever
    the view is asked about, it must be asked with the entry that goes with it.
    """
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    app = QApplication.instance() or QApplication([])
    view = ControllerView(entry or ENTRY, made, status_poll_ms=0, job_runner=run_inline)
    view.resize(980, 760)
    view._tabs.setCurrentIndex(
        next(i for i in range(view._tabs.count()) if view._tabs.tabText(i) == "Characters")
    )
    view._gate_app = app
    SHOTS.mkdir(parents=True, exist_ok=True)
    return view


def select(view, name: str) -> int:
    """Choose a character in the tab's own list and return its row.

    Through the list rather than by calling `_character_chosen`: the naming, the
    enabling and the withholding all hang off the SELECTION, and a stage that
    calls the slot directly photographs a state a person cannot reach.
    """
    from PySide6.QtCore import Qt  # noqa: PLC0415

    for i in range(view.character_list.count()):
        item = view.character_list.item(i)
        if str(item.data(Qt.ItemDataRole.UserRole) or "") == name:
            view.character_list.setCurrentRow(i)
            return i
    raise AssertionError(f"{name} is not in the tab's character list")


# --------------------------------------------------------------- G2 and G3


def stage_ground() -> None:
    """G2: the console answers. G3: the subjects, read with their clocks.

    Without G2 every "the command did nothing" below is ambiguous: a console
    that was never reached looks exactly like a command that ran and changed
    nothing.
    """
    prompted, text = console("server info")
    say(f"server info: prompted={prompted}")
    for line in quiet(text):
        say(f"  | {line}")
    assert prompted, "the console printed no prompt -- every reading below would be ambiguous"
    assert "Server uptime" in text, text

    say(f"StartPlayerLevel from etc/mangosd.conf: {start_level()}")
    say(f"characters on this server: {sql('SELECT COUNT(*) FROM tw_char.characters;')}")
    say(f"online right now: {sql('SELECT SUM(online) FROM tw_char.characters;')}")

    # Chosen by the properties each step needs, not by name -- the pool moves.
    online = sql(
        "SELECT guid,name,level FROM tw_char.characters "
        "WHERE online = 1 AND level > 1 AND (at_login & 1) = 0 ORDER BY guid LIMIT 3;"
    )
    say(f"online candidates (level > 1, rename flag clear):\n{online}")
    offline = sql(
        "SELECT guid,name,level FROM tw_char.characters "
        "WHERE online = 0 AND (at_login & 1) = 0 ORDER BY guid LIMIT 3;"
    )
    say(f"offline candidates:\n{offline}")
    dead = sql(
        "SELECT c.guid,c.name,c.level FROM tw_char.corpse co "
        "JOIN tw_char.characters c ON c.guid = co.player WHERE c.online = 0 LIMIT 3;"
    )
    say(f"offline with a corpse:\n{dead}")
    wearers = sql(
        "SELECT ci.guid, c.name, COUNT(*) n FROM tw_char.character_inventory ci "
        "JOIN tw_char.characters c ON c.guid = ci.guid "
        "WHERE ci.bag = 0 AND ci.slot < 19 AND c.online = 0 "
        "GROUP BY ci.guid HAVING n BETWEEN 2 AND 6 ORDER BY ci.guid LIMIT 3;"
    )
    say(f"offline wearers of 2-6 pieces:\n{wearers}")

    dorta = row(901)
    say(f"Dorta (the one non-bot character, and the client half's subject): {dorta}")
    say(f"  worn={worn(901)} mails={mails(901)}")


# --------------------------------------------------------------- S1


def stage_levelroute() -> None:
    """S1: every console route to a level, pressed.

    This is the step the shipped sentence rests on. Reading the command table
    and concluding is not the same as sending the command, and the first version
    of this plan concluded a sentence that its own tree refuted.
    """
    prompted, _ = console("server info")
    assert prompted, "no console -- nothing below would mean anything"

    # NOT a level-60 subject. The number every command below is handed is 60,
    # and a subject already at 60 would end at 60 whether the number was obeyed
    # or ignored -- an assertion true before the press.
    name = sql(
        "SELECT name FROM tw_char.characters WHERE online = 1 AND level BETWEEN 5 AND 45 "
        "ORDER BY guid LIMIT 1;"
    ).split("\t")[0]
    before = row(int(sql(f"SELECT guid FROM tw_char.characters WHERE name = '{name}';")))
    say(f"the subject: {before} -- every command below asks for level 60")
    level_before = before["level"]
    assert int(level_before) != 60, "a subject already at 60 cannot show a number being ignored"

    for command in (f"levelup {name} 60", f"character level {name} 60"):
        ok, text = console(command)
        say(f"`{command}` -> prompted={ok}")
        for line in quiet(text):
            say(f"  | {line}")

    ok, text = console(f"rndbot {name} level 60")
    say(f"`rndbot {name} level 60` -> prompted={ok}")
    for line in quiet(text):
        say(f"  | {line}")
    time.sleep(3)
    after = row(int(before["guid"]))
    say(f"the subject after the three: level {level_before} -> {after['level']}")
    say(
        "ASSERTION: none of the three moved an existing character to the level "
        "it was handed. `levelcreate` presses the one route that reaches a "
        "chosen level, and shows what it costs."
    )


def stage_initprobe() -> None:
    """S1's fourth probe: `rndbot init`, which the plan predicted would be a no-op.

    It is not. On the first run (07:27Z) it moved an online bot from level 22 to
    level 18 -- so this fork's console CAN move an existing character's level.
    The stage stays because the sentence it tests survives that: `init` takes no
    number, and the level it lands on is the module's choice, not the operator's.
    "Nothing moves a character to a level YOU PICK" is the claim, and this is the
    command that comes closest to refuting it.
    """
    name, guid = sql(
        "SELECT name,guid FROM tw_char.characters WHERE online = 1 AND level > 1 "
        "ORDER BY guid LIMIT 1;"
    ).split("\t")[:2]
    before = row(int(guid))
    say(f"GROUND: {before}")
    ok, text = console(f"rndbot init {name}")
    say(f"`rndbot init {name}` -> prompted={ok}")
    for line in quiet(text):
        say(f"  | {line}")
    time.sleep(3)
    after = row(int(guid))
    say(f"after: level {before['level']} -> {after['level']}")
    say(
        "the command took NO level argument, so whatever it wrote was the "
        "module's number and not an operator's -- which is what the shipped "
        "sentence claims about this tree."
    )


def stage_levelcreate() -> None:
    """S1's fifth probe, the one that is EXPECTED to work.

    A separate stage because it WRITES A CHARACTER, and the four probes above it
    do not: a stage that is safe to re-run and one that leaves a row behind
    should not share a name.

    The point of it is the shape of the answer, not the answer: a level a person
    picks is reachable on this tree only by making a character that was not
    there before, which is not what a "Set level" button offers.
    """
    prompted, _ = console("server info")
    assert prompted, "no console -- nothing below would mean anything"
    made = f"Gate{int(time.time()) % 100000}"
    everyone_before = int(sql("SELECT COUNT(*) FROM tw_char.characters;"))
    ok, text = console(f"rndbot create level=30 class=1 race=1 name={made}", window=20.0)
    say(f"`rndbot create level=30 class=1 race=1 name={made}` -> prompted={ok}")
    for line in quiet(text):
        say(f"  | {line}")
    time.sleep(6)
    fresh = sql(f"SELECT guid,name,level FROM tw_char.characters WHERE name = '{made}';")
    everyone_after = int(sql("SELECT COUNT(*) FROM tw_char.characters;"))
    say(f"the new row: {fresh!r}; characters {everyone_before} -> {everyone_after}")
    assert fresh, "the create did not land -- this stage's whole point is that it does"
    assert everyone_after == everyone_before + 1, (
        f"{everyone_after - everyone_before} characters appeared, not one"
    )
    say(
        "ASSERTION: the only console route to a chosen level MADE A CHARACTER. "
        "It did not move one, which is what a set-level control would offer."
    )


# --------------------------------------------------------------- S2


def stage_absent() -> None:
    """S2: no set-level control is drawn, and the absent group shows its reason.

    GROUND: the SAME app build on a tree that HAS the command, photographed
    first. Without it this stage photographs an empty space and proves nothing.
    """
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    QApplication.instance() or QApplication([])

    # The ground: a sibling entry whose console HAS a set-level verb, drawn by
    # this same class, so the difference photographed below is the tree's and
    # not the build's.
    wotlk = load_catalog().get("wow-wotlk")
    say(f"wow-wotlk set_level_command: {wotlk.play.set_level_command!r}")
    say(f"wow-tortoise set_level_command: {ENTRY.play.set_level_command!r}")
    assert wotlk.play.set_level_command is not None, "the ground tree must HAVE the command"
    assert ENTRY.play.set_level_command is None, "this tree must NOT have it"

    made = services()
    view = view_for(made)
    view.refresh_characters()
    name = sql(
        "SELECT name FROM tw_char.characters WHERE online = 1 AND level > 1 "
        "ORDER BY guid LIMIT 1;"
    ).split("\t")[0]
    select(view, name)

    # `isVisible()` is False for EVERY child of a window that was never shown,
    # so it cannot tell a withheld control from a window off screen -- the first
    # run of this stage asserted it and failed on the sentence it was there to
    # prove. `isHidden()` is the widget's own flag and `isVisibleTo(view)` is
    # "would it be drawn if this window were", which is what the shot shows.
    for label, widget in (
        ("the Level spinbox", view.new_level),
        ("the Set level button", view.set_level_button),
        ("the sentence", view.set_level_absent),
    ):
        say(
            f"{label}: hidden={widget.isHidden()} "
            f"would_be_drawn={widget.isVisibleTo(view)} "
            f"in_a_layout_cell={widget.parentWidget().layout().indexOf(widget) >= 0}"
        )
    say(f"the sentence reads: {view.set_level_absent.text()!r}")
    assert view.new_level.isHidden() and not view.new_level.isVisibleTo(view)
    assert view.set_level_button.isHidden() and not view.set_level_button.isVisibleTo(view)
    assert not view.set_level_absent.isHidden() and view.set_level_absent.isVisibleTo(view)
    assert view.set_level_absent.text() == ENTRY.play.set_level_absent_reason
    # The sentence is not merely present: it is IN the form, in the cell the
    # withheld control would have taken. A label with a parent and no layout
    # cell draws itself at the corner of its parent instead.
    form = view.set_level_absent.parentWidget().layout()
    assert form.indexOf(view.set_level_absent) >= 0, "the sentence is not in the form"
    assert form.indexOf(view.set_level_button) < 0, "the Set level button is still in the form"

    drawn = [b.text() for b in view.character_buttons()]
    say(f"the buttons this tree DOES draw, each naming {name}: {drawn}")
    assert view.set_level_button not in view.character_buttons()
    assert all(name in text for text in drawn), drawn

    SHOTS.mkdir(parents=True, exist_ok=True)
    view.grab().save(str(SHOTS / "2-no-set-level-and-its-reason.png"))
    say("photographed 2-no-set-level-and-its-reason.png")

    # And the ground, in the same build, one line lower: the tree that HAS it.
    ground_view = view_for(ControllerServices.for_entry(wotlk, SERVER_DIR), entry=wotlk)
    say(
        "the ground build's Level row exists: "
        f"spinbox={ground_view.new_level is not None} "
        f"button_in_actions={ground_view.set_level_button in ground_view.character_buttons()}"
    )
    assert ground_view.set_level_button in ground_view.character_buttons()
    ground_view.grab().save(str(SHOTS / "1-ground-a-tree-that-has-it.png"))
    say("photographed 1-ground-a-tree-that-has-it.png")


# --------------------------------------------------------------- S3


def stage_resetlevel() -> None:
    """S3: the sentence names `reset level`, and it is true -- both halves."""
    floor = start_level()
    say(f"StartPlayerLevel = {floor}")

    name, guid = sql(
        "SELECT name,guid FROM tw_char.characters WHERE online = 1 AND level > 5 "
        "ORDER BY guid LIMIT 1;"
    ).split("\t")[:2]
    before = row(int(guid))
    say(f"GROUND, online: {before}")
    assert int(before["level"]) != floor, (
        f"{name} is already at the starting level -- an assertion true before the press"
    )

    ok, text = console(f"reset level {name}")
    say(f"`reset level {name}` -> prompted={ok}")
    say(f"  the WHOLE reply, unfiltered: {text!r}")
    for line in quiet(text):
        say(f"  | {line}")

    # A BOUNDED re-read, not one sleep. The first run of this stage read the row
    # 3 s after the command, saw level 12 unchanged and would have recorded the
    # command as a no-op -- and the NEXT stage's ground, 21 s later, read the
    # same character at level 1. For an ONLINE character the world holds the
    # truth until it is asked to save (8.4a measured that on AzerothCore), so a
    # single fixed wait is a coin toss about how long the save queue is.
    level = before["level"]
    for attempt in range(20):
        time.sleep(3)
        level = row(int(guid))["level"]
        say(f"  re-read {attempt + 1} ({(attempt + 1) * 3} s after the command): level {level}")
        if int(level) == floor:
            break
    say(f"after: level {before['level']} -> {level} (StartPlayerLevel is {floor})")
    assert int(level) == floor, (
        f"{name} did not reach the starting level within 60 s -- the sentence "
        "claims this command does what it says"
    )

    off_name, off_guid = sql(
        "SELECT name,guid FROM tw_char.characters WHERE online = 0 ORDER BY guid LIMIT 1;"
    ).split("\t")[:2]
    off_before = row(int(off_guid))
    say(f"GROUND, offline: {off_before}")
    assert int(off_before["level"]) != floor, "an offline subject already at the floor proves nothing"
    ok, text = console(f"reset level {off_name}")
    say(f"`reset level {off_name}` (OFFLINE) -> prompted={ok}")
    for line in quiet(text):
        say(f"  | {line}")
    time.sleep(6)
    off_after = row(int(off_guid))
    say(f"after: level {off_before['level']} -> {off_after['level']}")
    assert off_after["level"] == off_before["level"], "the offline half was not refused"


# --------------------------------------------------------------- S4


def stage_teleport() -> None:
    """S4: this tree's own teleport verb, and a place the server does not know."""
    say(f"this entry's teleport verb: {ENTRY.play.teleport_command!r}")
    name, guid = sql(
        "SELECT name,guid FROM tw_char.characters WHERE online = 0 ORDER BY guid LIMIT 1;"
    ).split("\t")[:2]
    before = row(int(guid))
    say(f"GROUND: {before}")

    place = "orgrimmar" if before["map"] != "1" else "stormwind"
    known = sql(f"SELECT name,map FROM tw_world.game_tele WHERE name = '{place}';")
    say(f"the destination the server knows: {known!r}")
    assert known, f"{place} is not in this server's game_tele"

    outcome = play().teleport(name, place)
    say(f"the app's outcome: done={outcome.done} problem={outcome.problem!r}")
    for line in quiet(outcome.text or ""):
        say(f"  | {line}")
    time.sleep(4)
    after = row(int(guid))
    say(f"after: map {before['map']} -> {after['map']}, x {before['x']} -> {after['x']}")
    assert (after["map"], after["x"]) != (before["map"], before["x"]), "the row did not move"

    # And the refusal, through the TAB, because the question is what a person
    # SEES. On this transport `done` cannot mean "the server agreed": the attach
    # channel answers `yes` for every command the console echoes a prompt after
    # (`channel.py`'s AttachChannel, which says so in its own docstring -- there
    # is no fault code on a console). So the refusal is carried by the server's
    # own words in the report, and this photographs them.
    made = services()
    view = view_for(made)
    view.refresh_characters()
    select(view, name)
    view.teleport_where.setText("Zzznowhere")
    view.teleport_character()
    reported = view.character_report.text()
    say(f"a place nobody has -- the tab reads: {reported!r}")
    say("  the tab's own words, the SQL trace dropped:")
    for line in quiet(reported):
        say(f"  | {line}")
    view.grab().save(str(SHOTS / "4-a-place-the-server-does-not-know.png"))
    say("photographed 4-a-place-the-server-does-not-know.png")
    assert "not found" in reported.lower(), reported
    settled = row(int(guid))
    say(f"and the row did NOT move again: map {after['map']} -> {settled['map']}")

    nowhere = play().teleport(name, "Zzznowhere")
    say(f"the same at the seam: done={nowhere.done} problem={nowhere.problem!r}")
    say(
        "  RECORDED, not asserted: `done` is True for a refusal on this "
        "transport, and it is True for every command the console answers at "
        "all. What the tab shows is the server's sentence, which is the "
        "refusal; a boolean that could tell them apart would need a fault "
        "code, and a console has none."
    )


# --------------------------------------------------------------- S5


def stage_rename() -> None:
    """S5: this fork's TOP-LEVEL rename, sent by the app, on an ONLINE character."""
    say(f"this entry's rename verb: {ENTRY.play.rename_command!r}")
    assert ENTRY.play.rename_command == "rename", "this fork spells it at the top level"

    name, guid = sql(
        "SELECT name,guid FROM tw_char.characters WHERE online = 1 AND (at_login & 1) = 0 "
        "ORDER BY guid LIMIT 1;"
    ).split("\t")[:2]
    before = row(int(guid))
    say(f"GROUND: {before} -- at_login & 1 = {int(before['at_login']) & 1}")
    assert int(before["at_login"]) & 1 == 0, "the flag is already set; the press would prove nothing"

    outcome = play().rename(name)
    say(f"the app's outcome: done={outcome.done} problem={outcome.problem!r}")
    say(f"  text: {outcome.text!r}")
    assert "no such subcommand" not in (outcome.text or "").lower(), outcome.text
    time.sleep(3)
    after = row(int(guid))
    say(f"after: at_login {before['at_login']} -> {after['at_login']}, name {after['name']!r}")
    say(f"  the name is UNCHANGED: {before['name']!r} == {after['name']!r}")


# --------------------------------------------------------------- S6


def stage_renameoff() -> None:
    """S6: the offline rename withheld, at the BUTTON and at the SEAM.

    The destruction itself is argued from source and deliberately NOT run: the
    command it would send is
    `UPDATE characters SET name = guid, at_login = at_login | '1'`.
    A gate that proves a refusal by first proving the thing refused has spent a
    character to learn what the source already says.
    """
    name, guid = sql(
        "SELECT name,guid FROM tw_char.characters WHERE online = 0 ORDER BY guid LIMIT 1;"
    ).split("\t")[:2]
    before = row(int(guid))
    say(f"GROUND, an OFFLINE character: {before}")

    made = services()
    view = view_for(made)
    view.refresh_characters()
    select(view, name)
    say(f"the Rename button reads: {view.rename_button.text()!r}")
    say(f"  enabled: {view.rename_button.isEnabled()}")
    say(f"  its tooltip: {view.rename_button.toolTip()!r}")
    assert not view.rename_button.isEnabled(), "the button must be withheld for an offline row"
    assert view.rename_button.toolTip() == ENTRY.play.rename_offline_refusal
    view.grab().save(str(SHOTS / "6-offline-rename-withheld.png"))
    say("photographed 6-offline-rename-withheld.png")

    # And the seam, which is the half that matters: the button greys itself
    # from a list read minutes ago, and the seam reads the row at the press.
    say("now pressing the SEAM anyway, which is what a stale list would reach:")
    outcome = play().rename(name)
    say(f"  done={outcome.done} problem={outcome.problem!r} text={outcome.text!r}")
    assert not outcome.done, "the seam must refuse an offline rename on this tree"
    after = row(int(guid))
    say(f"after: name {before['name']!r} -> {after['name']!r}, at_login {after['at_login']}")
    assert after["name"] == before["name"], "THE NAME CHANGED -- the refusal did not hold"
    assert after["at_login"] == before["at_login"], "the flag moved; something was sent"
    say("the name and the flag are both untouched, so NOTHING reached the console")


# --------------------------------------------------------------- S7


def stage_revive() -> None:
    """S7: an offline revive that removes a corpse.

    GROUND is a corpse that is still there ~12 s before the press. Health is
    exactly the column this branch does not touch, so a health reading would
    prove the command did nothing on a tree where it works.
    """
    picked = sql(
        "SELECT c.name, c.guid FROM tw_char.corpse co "
        "JOIN tw_char.characters c ON c.guid = co.player WHERE c.online = 0 LIMIT 1;"
    ).split("\t")
    name, guid = picked[0], int(picked[1])
    say(f"the subject: {name} (guid {guid}), corpse rows = {corpse_rows(guid)}")
    assert corpse_rows(guid) == 1, "no corpse -- there would be nothing for the command to remove"

    say("watching the corpse for 12 s, so its going is the command's doing and not decay:")
    for _ in range(4):
        time.sleep(3)
        say(f"  corpse rows: {corpse_rows(guid)}")
    assert corpse_rows(guid) == 1, "the corpse went by itself; this subject proves nothing"

    outcome = play().revive(name)
    say(f"the app's outcome: done={outcome.done} problem={outcome.problem!r}")
    say(f"  text: {outcome.text!r}")
    time.sleep(3)
    say(f"corpse rows after: {corpse_rows(guid)}")


# --------------------------------------------------------------- S8


def stage_gear() -> None:
    """S8: one item per message. N worn pieces arrive as N mails."""
    say(f"this entry's mail_item_cap: {ENTRY.play.mail_item_cap}")
    assert ENTRY.play.mail_item_cap == 1, "the box's own words are 'one item per message'"

    picked = sql(
        "SELECT c.name, ci.guid, COUNT(*) n FROM tw_char.character_inventory ci "
        "JOIN tw_char.characters c ON c.guid = ci.guid "
        "WHERE ci.bag = 0 AND ci.slot < 19 AND c.online = 0 "
        "GROUP BY ci.guid HAVING n BETWEEN 2 AND 5 ORDER BY ci.guid LIMIT 1;"
    ).split("\t")
    wearer, guid, pieces = picked[0], int(picked[1]), int(picked[2])
    say(f"the wearer: {wearer} (guid {guid}), {pieces} pieces on")
    assert pieces >= 2, "one piece would not exercise the cap"

    # The recipient is the wearer, as 8.4c ran it: the mails are counted on a
    # row whose count is read before and after.
    before_mails = mails(guid)
    say(f"GROUND: {wearer} holds {before_mails} mails")

    made = services()
    view = view_for(made)
    view.refresh_characters()
    select(view, wearer)
    label = view.send_gear_button.text()
    say(f"BEFORE the press the button reads: {label!r}")
    assert f"{pieces} worn items" in label and f"{pieces} mails" in label, label
    view.grab().save(str(SHOTS / "8-the-promise.png"))
    say("photographed 8-the-promise.png")

    outcome = play().send_gear_set(
        wearer, to=wearer, subject="8.4d gate", body="one item per message"
    )
    say(f"the app's outcome: done={outcome.done} problem={outcome.problem!r}")
    for line in quiet(outcome.text or ""):
        say(f"  | {line}")
    time.sleep(4)
    after_mails = mails(guid)
    say(f"mails: {before_mails} -> {after_mails} (promised {pieces})")
    per_mail = sql(
        "SELECT MAX(n) FROM (SELECT COUNT(*) n FROM tw_char.mail_items mi "
        f"JOIN tw_char.mail m ON m.id = mi.mail_id WHERE m.receiver = {guid} "
        "GROUP BY mi.mail_id) t;"
    )
    say(f"MAX(items per mail) for this receiver: {per_mail}")
    sent = (outcome.text or "").lower().count("mail sent")
    say(f"'Mail sent' printed {sent} times")


# --------------------------------------------------------------- S9


def stage_gold() -> None:
    """S9: a successful send, reported as one."""
    name, guid = sql(
        "SELECT name,guid FROM tw_char.characters WHERE online = 0 ORDER BY guid LIMIT 1;"
    ).split("\t")[:2]
    before = mails(int(guid))
    say(f"GROUND: {name} holds {before} mails")
    outcome = play().mail_gold(name, gold=5, subject="8.4d gate", body="five gold")
    say(f"the app's outcome: done={outcome.done} problem={outcome.problem!r}")
    say(f"  text: {outcome.text!r}")
    time.sleep(3)
    say(f"mails: {before} -> {mails(int(guid))}")
    say(
        "the newest mail's money: "
        + sql(
            f"SELECT money FROM tw_char.mail WHERE receiver = {guid} ORDER BY id DESC LIMIT 1;"
        )
    )


# --------------------------------------------------------------- S10


def stage_catchup() -> None:
    """S10: the list's catch-up, and the honest limit of it ON THIS TREE.

    8.4c's clause is "the list shows the new state within the bounded re-reads".
    The first run of this stage pressed Teleport and watched the row text NOT
    change over six re-reads, and would have recorded a defect. It is not one:
    `play.characters()` selects `guid, name, level, online, account`, so the
    list line prints exactly those -- and every action this TREE draws writes
    `map`, `position_*`, `at_login`, `corpse` or `mail`, none of which is in it.

    The intersection is empty, and it is empty BECAUSE of this box: the one
    drawn action whose effect the line shows is the set-level control, and 8.4d
    is the box that removes it. So the clause is proved in the two halves that
    remain honest -- the machinery runs and is bounded, and the list is live
    enough to show a level change made by another hand.
    """
    made = services()
    view = view_for(made)
    view.refresh_characters()

    line_fields = ("name", "level", "online", "account")
    written_by = {
        "Teleport": ("map", "position_x", "position_y", "position_z"),
        "Rename at next login": ("at_login",),
        "Revive": ("corpse rows",),
        "Send gold to": ("mail",),
        "Send everything worn by": ("mail", "mail_items"),
    }
    say(f"the list line prints: {line_fields}")
    for action, columns in written_by.items():
        overlap = sorted(set(columns) & set(line_fields))
        say(f"  {action} writes {columns} -- overlap with the line: {overlap or 'NONE'}")
    assert not any(set(c) & set(line_fields) for c in written_by.values()), (
        "an action on this tree DOES write a field the line prints -- then press it"
    )
    say(
        "so no action this tree draws can change the line, and the only one "
        "that would is the set-level control this box removes."
    )

    # Half one: the machinery runs after a press, and it STOPS. An unbounded
    # re-read on a list that will never change is the failure mode worth
    # excluding here, and the bound is the app's own number.
    name, guid = sql(
        "SELECT name,guid FROM tw_char.characters WHERE online = 0 ORDER BY guid LIMIT 1;"
    ).split("	")[:2]
    select(view, name)
    before = row(int(guid))
    say(f"GROUND: {before}")
    say(f"the tab's bound on re-reads after an action: {view.refresh_attempts_after_an_action()}")
    place = "orgrimmar" if before["map"] != "1" else "stormwind"
    view.teleport_where.setText(place)
    view.teleport_character()
    for line in quiet(view.character_report.text()):
        say(f"  the tab says: {line}")
    for attempt in range(view.refresh_attempts_after_an_action() + 2):
        time.sleep(2)
        view.refresh_characters()
        select(view, name)
        say(f"  re-read {attempt + 1}: {view.character_list.currentItem().text()!r}")
    after = row(int(guid))
    say(f"the ROW moved: map {before['map']} -> {after['map']}, x {before['x']} -> {after['x']}")
    assert (after["map"], after["x"]) != (before["map"], before["x"])

    # Half two: the list is LIVE. A level changed by another hand -- the
    # console, not the tab, because the tab has no such control here -- appears
    # in the line on the next read. Without this the empty intersection above
    # would be indistinguishable from a list that never re-reads anything.
    floor = start_level()
    victim, vguid = sql(
        "SELECT name,guid FROM tw_char.characters WHERE online = 1 AND level > 5 "
        "ORDER BY guid LIMIT 1;"
    ).split("	")[:2]
    select(view, victim)
    shown_before = view.character_list.currentItem().text()
    say(f"the list shows {victim}: {shown_before!r}")
    ok, _ = console(f"reset level {victim}")
    say(f"`reset level {victim}` on the CONSOLE -> prompted={ok}")
    shown_after = shown_before
    for attempt in range(25):
        time.sleep(3)
        view.refresh_characters()
        select(view, victim)
        shown_after = view.character_list.currentItem().text()
        if shown_after != shown_before:
            say(f"  re-read {attempt + 1} ({(attempt + 1) * 3} s): {shown_after!r} -- CHANGED")
            break
        if attempt % 4 == 0:
            say(f"  re-read {attempt + 1} ({(attempt + 1) * 3} s): {shown_after!r}")
    say(f"the line: {shown_before!r} -> {shown_after!r}")
    assert shown_after != shown_before, (
        "the list did not pick up a level change made outside it -- that WOULD be a defect"
    )
    assert f"level {floor}" in shown_after, shown_after
    view.grab().save(str(SHOTS / "10-the-list-is-live.png"))
    say("photographed 10-the-list-is-live.png")
    say(
        "NOTE: the level took ~50 s to reach the database on the first run of "
        "`resetlevel`, and the tab's own bound is 4 re-reads at 750 ms. So a "
        "set-level control on THIS tree would answer before its row landed -- "
        "which is one more reason the group is absent rather than disabled."
    )


# --------------------------------------------------------------- S11


def stage_flatread() -> None:
    """S11: the equipped read is this fork's FLAT one, and the ids exist."""
    say(f"this entry's equipped.template_column: {ENTRY.play.equipped.template_column!r}")
    picked = sql(
        "SELECT c.name, ci.guid, COUNT(*) n FROM tw_char.character_inventory ci "
        "JOIN tw_char.characters c ON c.guid = ci.guid "
        "WHERE ci.bag = 0 AND ci.slot < 19 GROUP BY ci.guid HAVING n >= 3 "
        "ORDER BY ci.guid LIMIT 1;"
    ).split("\t")
    name, guid, n = picked[0], int(picked[1]), int(picked[2])
    say(f"the wearer: {name} (guid {guid}), SQL says {n} pieces")

    pieces = play_module.equipped(reader(), ENTRY, name)
    say(f"the app's read returned {len(pieces)} ids: {sorted(pieces)[:8]}")
    assert len(pieces) == n, f"the app read {len(pieces)} and the table holds {n}"

    ids = ",".join(str(i) for i in sorted(set(pieces)))
    real = int(sql(f"SELECT COUNT(*) FROM tw_world.item_template WHERE entry IN ({ids});") or 0)
    say(f"of {len(set(pieces))} distinct ids, {real} exist in tw_world.item_template")
    assert real == len(set(pieces)), (
        "ids that are not item templates -- the read answered instance guids, "
        "which would still have returned the right COUNT"
    )
    instances = int(
        sql(f"SELECT COUNT(*) FROM tw_char.character_inventory WHERE guid = {guid} AND item IN ({ids});")
        or 0
    )
    say(f"and the same ids as inventory instance guids match {instances} rows (0 is the point)")


# --------------------------------------------------------------- S12


def stage_deadchannel() -> None:
    """S12: a channel that cannot be asked is reported as that.

    LEAVES THE SERVER DOWN in the middle and puts it back at the end. It is last
    for that reason: every stage above needs a world.
    """
    name = sql(
        "SELECT name FROM tw_char.characters WHERE online = 0 ORDER BY guid LIMIT 1;"
    ).split("\t")[0]
    ok, _ = console("server info")
    say(f"GROUND: the console answers, prompted={ok}")
    assert ok, "the channel is already dead; the stage would prove nothing"

    subprocess.run(["docker", "stop", "tortoise-mangosd"], check=True, capture_output=True)
    say("stopped tortoise-mangosd")
    time.sleep(4)
    try:
        outcome = play().mail_gold(name, gold=5, subject="8.4d gate", body="into a dead channel")
        say(f"the app's outcome: done={outcome.done}")
        say(f"  problem: {outcome.problem!r}")
        say(f"  text: {outcome.text!r}")
        made = services()
        view = view_for(made)
        view.refresh_characters()
        select(view, name)
        view.gold_amount.setValue(5)
        view.mail_gold()
        say(f"the tab says: {view.character_report.text()!r}")
        view.grab().save(str(SHOTS / "12-the-channel-could-not-be-asked.png"))
        say("photographed 12-the-channel-could-not-be-asked.png")
    finally:
        subprocess.run(["docker", "start", "tortoise-mangosd"], check=True, capture_output=True)
        say("started tortoise-mangosd again; waiting for the world")
        for attempt in range(40):
            time.sleep(10)
            try:
                ok, text = console("server info")
            except Exception as exc:  # noqa: BLE001
                say(f"  attempt {attempt + 1}: {exc}")
                continue
            say(f"  attempt {attempt + 1}: prompted={ok}")
            if ok and "Server uptime" in text:
                say("the world answers again; the box leaves this machine as it found it")
                return
        raise AssertionError("the worldserver did not come back -- SAY SO IN THE REPORT")


# ------------------------------------------------- the client half's ground


def stage_dorta_setup() -> None:
    """The client half's ground: put Dorta somewhere it is not, and mail it gold.

    Both through the app's own seam, so what the client photographs is 8.4d's
    press and not a hand-written row.
    """
    place = sys.argv[2] if len(sys.argv) > 2 else "orgrimmar"
    gold = int(sys.argv[3]) if len(sys.argv) > 3 else 7
    before = row(901)
    say(f"GROUND: {before}")
    say(f"  worn={worn(901)} mails={mails(901)}")
    known = sql(f"SELECT name,map,position_x FROM tw_world.game_tele WHERE name = '{place}';")
    say(f"the destination: {known!r}")
    assert known, f"{place} is not in this server's game_tele"

    outcome = play().teleport("Dorta", place)
    say(f"teleport: done={outcome.done} text={outcome.text!r} problem={outcome.problem!r}")
    time.sleep(3)
    gold_out = play().mail_gold("Dorta", gold=gold, subject="8.4d", body="from the Characters tab")
    say(f"mail gold: done={gold_out.done} text={gold_out.text!r}")
    time.sleep(3)
    after = row(901)
    say(f"after: {after}")
    say(f"  mails {mails(901)}")


def stage_dorta_read() -> None:
    say(f"Dorta: {row(901)}")
    say(f"  worn={worn(901)} mails={mails(901)} corpses={corpse_rows(901)}")
    say(
        "  mail rows: "
        + sql("SELECT id,subject,money,has_items FROM tw_char.mail WHERE receiver = 901;")
    )


def stage_dorta_rename() -> None:
    """AFTER the /who session: flag Dorta for rename, through the app.

    Dorta must be ONLINE for this tree's rename to be sent at all, which is what
    S6 is about -- so this stage refuses while it is out, rather than sending a
    command that would replace the name with the guid.
    """
    before = row(901)
    say(f"GROUND: {before}")
    outcome = play().rename("Dorta")
    say(f"the app's outcome: done={outcome.done} problem={outcome.problem!r} text={outcome.text!r}")
    time.sleep(3)
    say(f"after: {row(901)}")


STAGES = {
    "ground": stage_ground,
    "levelroute": stage_levelroute,
    "initprobe": stage_initprobe,
    "levelcreate": stage_levelcreate,
    "absent": stage_absent,
    "resetlevel": stage_resetlevel,
    "teleport": stage_teleport,
    "rename": stage_rename,
    "renameoff": stage_renameoff,
    "revive": stage_revive,
    "gear": stage_gear,
    "gold": stage_gold,
    "catchup": stage_catchup,
    "flatread": stage_flatread,
    "deadchannel": stage_deadchannel,
    "dorta-setup": stage_dorta_setup,
    "dorta-read": stage_dorta_read,
    "dorta-rename": stage_dorta_rename,
}


if __name__ == "__main__":
    which = sys.argv[1] if len(sys.argv) > 1 else ""
    if which not in STAGES:
        print(f"stages: {' '.join(STAGES)}")
        raise SystemExit(2)
    say(f"=== stage {which} ===")
    STAGES[which]()
    say(f"=== stage {which} finished ===")
