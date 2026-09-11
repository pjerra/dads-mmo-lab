"""The 8.4c live gate: the Characters tab's actions, on WoW Vanilla.

Usage:  python gate84c.py <stage>

    pick      choose the characters this box acts on, and say who
    names     what this server does about two characters with one name
    verb      which teleport verb this tree takes, measured by SENDING both
    cap       how many items ONE mail carries here -- the reason this box exists
    equipped  every id the app's equipped query returns resolves as an item
    offline   every action on an offline character, each read back in the row
    online    the same on one that is logged in
    revive    the corpse clause: a corpse must exist BEFORE the command
    gear      N worn pieces promise N mails on a cap-of-one tree, and send N
    case      a name typed in the wrong case still finds the character
    takeover  point the realm at a reachable address and set a password, both
              through the app's own features, so the client lane can log in
    shots     the Characters tab, rendered from the real view

8.4b's shape, not 8.4b's file. The differences are this tree's, and each was
asked of THIS server rather than carried over: the mail cap, the teleport verb,
what a character here is wearing, and what a revive does to a corpse.
"""

from __future__ import annotations

import json
import os
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate84c" / "pylauncher"))

from yulon import commands, networking, play  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "vanilla-75b"
ENTRY = load_catalog().get("wow-vanilla")
SHOTS = Path.home() / "gate84c-shots"
CHOSEN = Path.home() / "gate84c-chosen.json"

TELEPORT_TO = ("Orgrimmar", "Stormwind")
"""Two places, and the box uses whichever the character is NOT at.

Both are rows in this server's own `mangos.game_tele` (ids 118 and 246, read
2026-09-07); a name that is not in that table is refused by the server and the
gate would be reading its own typo.
"""
WRONG_VERB = "teleport name"
"""AzerothCore's verb, sent here ON PURPOSE so its refusal is a reading."""

NEW_LEVEL = 60
"""Vanilla's cap. `character level` takes 1..255 whatever the server allows."""
GROUND_LEVEL = NEW_LEVEL - 7
GOLD = 5
TAKEOVER_PASSWORD = "gate84c-p@ss"
REACHABLE = "100.78.24.50"
"""m910q's Tailscale address.

The realm row advertises `192.168.10.134`, the LAN address, which the Hyper-V
host running the client cannot reach -- the trap that cost 8.4b an evening. It
is corrected through Yu'lon's own Networking plan and apply, which is a live
press of that feature rather than a row written by hand.
"""

CAP_PROBE_ITEMS = (2589, 3110)
"""Linen Cloth and Tunnel Rat Ear, both in this server's shipped world DB.

TWO different ids and three sends, because on this tree an unknown item id and
an over-cap send take the SAME exit -- a `return false` that reaches SOAP as a
closed connection with nothing in it. A two-probe version could not tell "the
cap is one" from "the second id does not exist", so each id is sent alone first.
"""


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def services() -> ControllerServices:
    return ControllerServices.for_entry(ENTRY, SERVER_DIR)


def channel():
    setup = services().channel_setup
    assert setup is not None, "this install has no command channel"
    live = setup.live_channel()
    assert live is not None, "no live command channel"
    return live


def sql(db: str, statement: str) -> str:
    from yulon.ui import controller_view as view_module  # noqa: PLC0415

    reader = view_module._sql_for(
        ENTRY, view_module._db_password(ENTRY, SERVER_DIR), wsl_distro=None
    )
    return reader.query(db, statement).strip()


def run_statement(db: str, statement: str) -> None:
    from yulon.ui import controller_view as view_module  # noqa: PLC0415

    writer = view_module._sql_for(
        ENTRY, view_module._db_password(ENTRY, SERVER_DIR), wsl_distro=None
    )
    writer.run_statement(db, statement)


def schemas() -> dict[str, str]:
    return ENTRY.schema_map()


ROW_KEYS = ("guid", "name", "level", "online", "x", "y", "map", "at_login", "money", "health")


def row(name: str) -> dict[str, str]:
    fields = sql(
        "characters",
        "SELECT guid, name, level, online, position_x, position_y, map, at_login, money, health "
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


def corpse(guid: int) -> tuple[int, str]:
    """How many corpses this CHARACTER has, and the lowest corpse_type.

    `corpse.guid` is the CORPSE's own guid and not the character's -- 8.4c's
    adversarial review caught a clause that would have joined on it and read
    somebody else's death. The column that names the player is `player`.
    """
    answer = sql(
        "characters",
        f"SELECT COUNT(*), IFNULL(MIN(corpse_type), -1) FROM {schemas()['characters']}.corpse "
        f"WHERE player = {guid};",
    ).split("\t")
    return (int(answer[0]), answer[1])


def chosen() -> dict[str, str]:
    return json.loads(CHOSEN.read_text())


def settled(read, want, seconds: float = 25.0, nudge=None):
    """Wait for the server to write what it has already reported.

    The command answers before its row lands, and for an ONLINE character the
    world holds the truth until it is asked to write it down -- which is what
    `nudge` is for.
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
    answer = channel().send("saveall")
    say(f"saveall   : {answer.outcome} {(answer.text or '').strip()[:60]!r}")


def population() -> tuple[int, int]:
    total = int(sql("characters", f"SELECT COUNT(*) FROM {schemas()['characters']}.characters;"))
    online = int(
        sql(
            "characters",
            f"SELECT COUNT(*) FROM {schemas()['characters']}.characters WHERE online = 1;",
        )
    )
    return (total, online)


# -- who this box acts on ---------------------------------------------------

CLIENT_CHARACTER = "Asdff"
"""The one character on this server a PERSON owns (account VANGATE, guid 901).

Its 900 neighbours are bots, and a bot's account is not a spare seat: the
playerbots module owns those sessions, so a client logging into one
authenticates and never reaches the world. The offline half acts on this one so
the client lane can SEE what was done.
"""


def stage_pick() -> None:
    total, online = population()
    say(f"the server holds {total} characters and {online} of them are online right now")

    offline_name = CLIENT_CHARACTER
    state = row(offline_name)
    assert state, f"{offline_name} is not on this server after all"
    assert state["online"] == "0", f"{offline_name} is logged in; the offline half needs it out"

    def best_wearer(online_flag: int) -> tuple[str, str]:
        """The best-dressed character whose name NOBODY ELSE HAS.

        The exclusion is not tidiness: this server has two characters called
        Joleta and two called Dalnaal, and every read and every command in this
        app addresses a character by name. A gate that picked one of them would
        be measuring the app's ambiguity rather than the clause under test --
        which is `stage_names`' job, on purpose, further down.
        """
        characters = f"{schemas()['characters']}.characters"
        answer = sql(
            "characters",
            f"SELECT c.name, COUNT(*) FROM {characters} c "
            f"JOIN {schemas()['characters']}.character_inventory i ON i.guid = c.guid "
            f"WHERE c.online = {online_flag} AND i.bag = 0 AND i.slot < {play.EQUIPPED_SLOTS} "
            f"AND c.name NOT IN (SELECT name FROM {characters} "
            "GROUP BY name HAVING COUNT(*) > 1) "
            "GROUP BY c.name ORDER BY COUNT(*) DESC, c.name LIMIT 1;",
        ).split("\t")
        return (answer[0], answer[1])

    online_name, online_pieces = best_wearer(1)
    wearer_name, wearer_pieces = best_wearer(0)
    picked = {
        "offline": offline_name,
        "offline_guid": state["guid"],
        "online": online_name,
        "online_pieces": online_pieces,
        "wearer": wearer_name,
        "wearer_pieces": wearer_pieces,
        "picked_at": stamp(),
        "online_population": online,
    }
    CHOSEN.write_text(json.dumps(picked, indent=2))
    say(f"offline: {offline_name} (guid {state['guid']}, level {state['level']}, map {state['map']})")
    say(f"online : {online_name}, wearing {online_pieces} pieces")
    say(f"wearer : {wearer_name}, wearing {wearer_pieces} pieces -- the gear clause's subject")
    account = sql(
        "characters",
        f"SELECT account FROM {schemas()['characters']}.characters WHERE name = '{offline_name}';",
    )
    username = sql("auth", f"SELECT username FROM {schemas()['auth']}.account WHERE id = {account};")
    say(f"{offline_name}'s account is {username} (id {account})")


# -- two characters, one name -----------------------------------------------


def stage_names() -> None:
    """What this app does when two characters share a name.

    Found here rather than reasoned about: `stage_pick` chose the best-dressed
    online character and the very next read failed with *"Subquery returns more
    than 1 row"*. `characters.name` on this tree is `idx_name`, a NON-unique
    index, and the bot generator collided.

    Every read and every command in this feature addresses a character by name,
    so this stage records exactly which of them break and how loudly.
    """
    from yulon.ui import controller_view as view_module  # noqa: PLC0415

    reader = view_module._sql_for(
        ENTRY, view_module._db_password(ENTRY, SERVER_DIR), wsl_distro=None
    )
    dupes = sql(
        "characters",
        f"SELECT name, COUNT(*) FROM {schemas()['characters']}.characters "
        "GROUP BY name HAVING COUNT(*) > 1 ORDER BY name;",
    )
    say(f"names more than one character has:\n{dupes}")
    if not dupes:
        say("NOT RUN: no duplicate names on this server right now")
        return
    name = dupes.splitlines()[0].split("\t")[0]
    rows = sql(
        "characters",
        f"SELECT guid, name, level, online, account FROM {schemas()['characters']}.characters "
        f"WHERE name = '{name}';",
    )
    say(f"the {name}s:\n{rows}")

    say(f"canonical_character({name.lower()!r}) answers "
        f"{play.canonical_character(reader, ENTRY, name.lower())!r} -- one of them, by LIMIT 1")
    try:
        ids = play.equipped(reader, ENTRY, name)
    except Exception as exc:  # noqa: BLE001 - the reading IS what it raises
        say(f"equipped({name!r}) RAISES {type(exc).__name__}: {str(exc).strip().splitlines()[-1]}")
    else:
        say(f"equipped({name!r}) answers {list(ids)}")

    tab = services().play
    try:
        say(f"gear_set_size({name!r}) answers {tab.gear_set_size(name)}")
    except Exception as exc:  # noqa: BLE001 - the reading IS what it raises
        say(f"gear_set_size({name!r}) RAISES {type(exc).__name__}: "
            f"{str(exc).strip().splitlines()[-1]}")
    # And what the TAB draws for that character, from the real view. Before
    # 8.4c this branch fell through to "is wearing nothing" -- a false sentence
    # about the character in place of a true one about the server.
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
    hit = [
        i
        for i in range(view.character_list.count())
        if view.character_list.item(i).text().startswith(name + " ")
    ]
    assert hit, f"the list does not hold {name}"
    view.character_list.setCurrentRow(hit[0])
    said = view.send_gear_button.text()
    say(f"the tab's gear button on {name} reads: {said!r} (enabled="
        f"{view.send_gear_button.isEnabled()})")
    SHOTS.mkdir(parents=True, exist_ok=True)
    view.grab().save(str(SHOTS / "4-two-of-one-name.png"))
    assert "wearing nothing" not in said, "the tab still calls an unreadable set an empty one"
    assert name in said, said
    del app
    say("PASSED: the tab names the ambiguity rather than calling the character naked")


# -- the verb ---------------------------------------------------------------


def stage_verb() -> None:
    """Which teleport verb this tree takes, measured by SENDING both.

    Not by reading a header and not by inheriting TBC's answer: the catalog
    carries `tele name` for this entry and this stage is what makes that a
    measurement. The ground is the character's own position, which neither send
    can have produced.
    """
    name = chosen()["offline"]
    before = row(name)
    at_stormwind = float(before["x"]) < -8000 and before["map"] == "0"
    where = TELEPORT_TO[0] if at_stormwind else TELEPORT_TO[1]
    say(f"{name} starts at ({before['x']}, {before['y']}) map {before['map']}; sending it to {where}")

    wrong = channel().send(f"{WRONG_VERB} {name} {where}")
    still = row(name)
    say(f"{WRONG_VERB!r}: {wrong.outcome} {(wrong.text or wrong.problem or '').strip()[:120]!r}")
    say(f"            the row is still ({still['x']}, {still['y']}) map {still['map']}")
    assert (still["x"], still["y"], still["map"]) == (before["x"], before["y"], before["map"]), (
        "the wrong verb moved the character"
    )
    helped = channel().send("help teleport")
    say(f"'help teleport' answers: {(helped.text or '').strip()[:60]!r}")

    verb = ENTRY.play.teleport_command
    say(f"the catalog says this tree's verb is {verb!r}; sending it through the app")
    tab = services().play
    out = tab.teleport(name, where)
    after = settled(lambda: row(name), lambda r: (r["x"], r["y"]) != (before["x"], before["y"]))
    say(f"{verb!r}: done={out.done} {(out.text or out.problem).strip()[:120]!r}")
    say(f"            now at ({after['x']}, {after['y']}) map {after['map']}")
    assert out.done, out.problem
    assert (after["x"], after["y"]) != (before["x"], before["y"]), "the right verb did not move it"
    say("PASSED: the verb was measured by use -- one moved the character and one was refused")


# -- the cap, which is what this box exists for -----------------------------


def _mail_ids(name: str) -> list[str]:
    rows = sql(
        "characters",
        "SELECT m.id, m.subject, m.has_items, "
        f"(SELECT COUNT(*) FROM {schemas()['characters']}.mail_items mi WHERE mi.mail_id = m.id) "
        f"FROM {schemas()['characters']}.mail m "
        f"JOIN {schemas()['characters']}.characters c ON c.guid = m.receiver "
        f"WHERE c.name = '{name}' ORDER BY m.id DESC LIMIT 5;",
    )
    return [line for line in rows.splitlines() if line.strip()]


def stage_cap() -> None:
    """How many attachments does ONE mail carry here? Asked by sending them.

    The catalog's 1 is a SOURCE-CITED PREDICTION -- `MAX_MAIL_ITEMS` in this
    tree's own `src/game/Mails/Mail.h:49`, where the TBC install's identical
    header reads 12. A prediction the server has never been asked is not a
    measurement, and this stage is the asking.

    Three sends, not two, and the third is the one that decides:

        2589:1            -- one item, an id this tree has
        3110:1            -- one item, a DIFFERENT id this tree has
        2589:1 3110:1     -- two items, both already proved to exist

    If the first two arrive and the third does not, the cap is one and nothing
    about the ids explains it.
    """
    name = chosen()["offline"]
    line = channel()
    first, second = CAP_PROBE_ITEMS
    probes = (
        (f"{first}:1", 1),
        (f"{second}:1", 1),
        (f"{first}:1 {second}:1", 2),
    )
    readings = []
    for attachments, count in probes:
        had = mails(name)
        answer = line.send(f'send items {name} "cap {count}" "how many fit" {attachments}')
        now = settled(lambda had=had: mails(name), lambda n, had=had: n > had, seconds=8)
        arrived = now - had
        readings.append((count, answer.outcome, (answer.text or "").strip(), arrived))
        say(
            f"{count} item{'' if count == 1 else 's'} -> {answer.outcome}: "
            f"{(answer.text or answer.problem or '').strip()[:70]!r}   mails {had} -> {now}"
        )
    for row_ in _mail_ids(name):
        say(f"   mail: {row_}")

    one_a, one_b, two = readings
    assert one_a[3] == 1, f"a single {first} did not arrive: {one_a}"
    assert one_b[3] == 1, f"a single {second} did not arrive: {one_b}"
    measured_cap = 1 if two[3] == 0 else 2
    say(f"MEASURED CAP: {measured_cap}")
    declared = ENTRY.play.mail_item_cap
    say(f"the catalog declares {declared}")
    if measured_cap != declared:
        raise AssertionError(
            f"THE CATALOG IS WRONG: it declares mail_item_cap {declared} and this server "
            f"took {measured_cap}. Fix the entry before anything else in this gate is believed."
        )

    # And the number is not just written down -- it reaches the button. The
    # app's own command builder refuses the two-item line this server refuses.
    try:
        commands.mail_items(
            name, subject="two", body="two", items=((first, 1), (second, 1)), cap=declared
        )
    except commands.CommandError as exc:
        say(f"the app refuses the two-item line itself: {exc}")
    else:
        raise AssertionError("the app built a two-item line for a server that takes one")
    say("PASSED: the cap was measured against the running server and the catalog agrees")


# -- what a character is wearing --------------------------------------------


def stage_equipped() -> None:
    """Every id the app's equipped query returns resolves as an item HERE.

    Not "the two catalog shapes differ" -- on this tree they cannot, because
    `character_inventory` carries both the instance guid and the template id and
    `Player.cpp:3832` writes them from one Item. What CAN be wrong is the column
    NAME, and the reading that shows it is resolution: the app's column resolves
    in `mangos.item_template` and the instance-guid column does not.
    """
    from yulon.ui import controller_view as view_module  # noqa: PLC0415

    reader = view_module._sql_for(
        ENTRY, view_module._db_password(ENTRY, SERVER_DIR), wsl_distro=None
    )
    name = chosen()["wearer"]
    block = ENTRY.play.equipped
    say(f"the catalog reads {name}'s gear from {block.template_column!r} "
        f"(instance_table {block.instance_table!r})")
    ids = play.equipped(reader, ENTRY, name)
    say(f"the app answers {len(ids)} ids: {list(ids)}")
    assert ids, f"{name} is wearing nothing"

    listed = ",".join(str(i) for i in sorted(set(ids)))
    resolved = int(
        sql("world", f"SELECT COUNT(*) FROM {schemas()['world']}.item_template WHERE entry IN ({listed});")
    )
    say(f"{resolved} of {len(set(ids))} distinct ids resolve in {schemas()['world']}.item_template")
    assert resolved == len(set(ids)), "an id the app answered is not an item on this server"

    guids = sql(
        "characters",
        f"SELECT ci.item FROM {schemas()['characters']}.character_inventory ci "
        f"WHERE ci.guid = (SELECT guid FROM {schemas()['characters']}.characters "
        f"WHERE name = '{name}') AND ci.bag = 0 AND ci.slot < {play.EQUIPPED_SLOTS} "
        "ORDER BY ci.slot;",
    ).splitlines()
    guid_list = ",".join(g.strip() for g in guids if g.strip())
    guid_resolved = int(
        sql("world", f"SELECT COUNT(*) FROM {schemas()['world']}.item_template WHERE entry IN ({guid_list});")
    )
    say(f"the INSTANCE guid column would answer {guids} -- {guid_resolved} of which are items")
    assert guid_resolved == 0, (
        "the instance guids happen to resolve as item ids here, so this reading "
        "cannot tell the two columns apart"
    )
    say("PASSED: the app's column answers items; the neighbouring column answers numbers")


# -- the actions ------------------------------------------------------------


def clear_the_ground(name: str, *, nudge=None) -> dict[str, str]:
    """Put the character in a state the actions do NOT produce.

    A step whose assertion was already true before the command proves nothing --
    that was 8.4a's own defect. So the level is set somewhere else first and the
    rename flag is cleared, each through the server's own command where there is
    one and through the row where there is not.
    """
    tab = services().play
    out = tab.set_level(name, GROUND_LEVEL)
    say(f"ground: level -> {GROUND_LEVEL} (done={out.done})")
    assert out.done, out.problem
    settled(lambda: row(name), lambda r: r["level"] == str(GROUND_LEVEL), nudge=nudge)
    run_statement(
        "characters",
        f"UPDATE {schemas()['characters']}.characters SET at_login = at_login & ~1 "
        f"WHERE name = '{name}';",
    )
    if nudge is not None:
        # For an ONLINE character the row is not the ground -- the world is.
        # Measured here, 2026-09-07: this stage cleared `at_login` in the row on
        # a character it had renamed an hour earlier, the next `saveall` wrote
        # the player object's own flag straight back, and the clause failed with
        # "the rename flag is ALREADY set". There is no command that clears the
        # flag, so a rename cannot be re-run on the same live character; the
        # stage picks one that has not been renamed instead.
        nudge()
    before = row(name)
    say(f"ground: level {before['level']}, at_login {before['at_login']}, money {before['money']}")
    assert before["level"] == str(GROUND_LEVEL), before
    assert not int(before["at_login"] or 0) & 1, "the rename flag is still set"
    assert before["level"] != str(NEW_LEVEL), "the ground is the level the action sets"
    return before


def _each_action(name: str, *, online: bool) -> None:
    tab = services().play
    nudge = save_everyone if online else None
    before = clear_the_ground(name, nudge=nudge)

    at_stormwind = float(before["x"]) < -8000 and before["map"] == "0"
    where = TELEPORT_TO[0] if at_stormwind else TELEPORT_TO[1]
    out = tab.teleport(name, where)
    after = settled(
        lambda: row(name), lambda r: (r["x"], r["y"]) != (before["x"], before["y"]), nudge=nudge
    )
    say(f"teleport  : to {where}, done={out.done} {(out.text or out.problem).strip()[:90]!r}")
    say(f"            ({before['x']}, {before['y']}) map {before['map']}"
        f" -> ({after['x']}, {after['y']}) map {after['map']}")
    assert out.done, out.problem
    assert (after["x"], after["y"]) != (before["x"], before["y"]), "the character did not move"

    out = tab.set_level(name, NEW_LEVEL)
    after = settled(lambda: row(name), lambda r: r["level"] == str(NEW_LEVEL), nudge=nudge)
    say(f"set level : done={out.done} {(out.text or out.problem).strip()[:90]!r}")
    say(f"            the row moved from level {before['level']} to {after['level']}")
    assert out.done, out.problem
    assert after["level"] == str(NEW_LEVEL), after

    flag_before = int(row(name)["at_login"] or 0)
    assert not flag_before & 1, "the rename flag is ALREADY set"
    out = tab.rename(name)
    after = settled(lambda: row(name), lambda r: int(r["at_login"] or 0) & 1, nudge=nudge)
    say(f"rename    : done={out.done} at_login {flag_before} -> {after['at_login']}")
    assert out.done, out.problem
    assert int(after["at_login"]) & 1, "the rename flag is not set on the row"

    had = mails(name)
    money_before = row(name)["money"]
    out = tab.mail_gold(name, gold=GOLD, subject="A gift", body="From the server owner")
    now = settled(lambda: mails(name), lambda n: n > had)
    say(f"send gold : done={out.done} mails {had} -> {now} (the money is IN the mail, "
        f"not on the row, which still reads {money_before})")
    assert out.done, out.problem
    assert now == had + 1, "no mail arrived"


def stage_offline() -> None:
    name = chosen()["offline"]
    say(f"the offline character: {name}")
    assert row(name)["online"] == "0", "this character is online after all"
    _each_action(name, online=False)
    assert row(name)["online"] == "0", "it logged in while the box was working on it"
    say("PASSED: every action on an offline character, each read back in the row")


def _a_fresh_online_character() -> str:
    """An online character, wearing gear, whose name is its own and whose rename
    flag has never been set."""
    characters = f"{schemas()['characters']}.characters"
    answer = sql(
        "characters",
        f"SELECT c.name FROM {characters} c "
        f"JOIN {schemas()['characters']}.character_inventory i ON i.guid = c.guid "
        f"WHERE c.online = 1 AND (c.at_login & 1) = 0 "
        f"AND i.bag = 0 AND i.slot < {play.EQUIPPED_SLOTS} "
        f"AND c.name NOT IN (SELECT name FROM {characters} GROUP BY name HAVING COUNT(*) > 1) "
        "GROUP BY c.name ORDER BY COUNT(*) DESC, c.name LIMIT 1;",
    ).strip()
    assert answer, "every online character has already been renamed"
    return answer.splitlines()[0].split("\t")[0]


def stage_online() -> None:
    """The same, on one that IS logged in.

    Every character online here is a bot, and the module is a SECOND AUTHOR: it
    writes levels and positions of its own. So each assertion is `settled` with
    a `saveall` nudge and read immediately, and the level chosen is one no bot
    would arrive at by playing within the window.

    **The character is chosen HERE and not in `pick`**, because the rename
    clause needs one that has never been renamed. The flag lives on the live
    player object, no command clears it, and a row UPDATE is undone by the next
    save -- so this stage is not repeatable on the same character, and it asks
    the server for a fresh one each time rather than pretending it is.
    """
    name = sys.argv[2] if len(sys.argv) > 2 else _a_fresh_online_character()
    say(f"the online character: {name}")
    assert row(name)["online"] == "1", "this character is not online"
    _each_action(name, online=True)
    say("PASSED: every action on an online character, each read back in the row")


# -- revive, which needs a corpse -------------------------------------------


def _with_a_corpse(online_flag: int) -> tuple[str, int] | None:
    answer = sql(
        "characters",
        "SELECT c.name, c.guid FROM "
        f"{schemas()['characters']}.corpse co "
        f"JOIN {schemas()['characters']}.characters c ON c.guid = co.player "
        f"WHERE c.online = {online_flag} ORDER BY co.time DESC LIMIT 1;",
    ).strip()
    if not answer:
        return None
    name, guid = answer.splitlines()[0].split("\t")
    return (name, int(guid))


def stage_revive() -> None:
    """A corpse must EXIST before the command, or this stage refuses to run.

    `revive` acts on a live player object, so its effect is a corpse that stops
    existing. Reading "no corpse" after a command sent to a character that never
    had one is the shape of assertion 8.4a was caught by.

    The second author is real -- a bot revives itself -- so the corpse is
    watched for the SAME window before the command as after it. A corpse that
    sat still for that window and vanished within it once the command was sent
    is the reading; a corpse that vanished on its own before the command makes
    the stage say so and stop.

    **The OFFLINE half refutes the app**, and this stage is written to record
    that rather than to assert 8.4b's answer. `Level3.cpp:3281-3297` on this
    tree has an offline branch -- `ConvertCorpseForPlayer(target_guid)`, "will
    resurrected at login without corpse" -- so the corpse GOES and the character
    logs in alive. What does not change is `characters.health`, which is the
    only thing 8.4a and 8.4b looked at, and that is why both concluded the
    command does nothing. Whether the character was logged in is read at both
    moments, so a bot that logged itself in cannot be mistaken for the command.
    """
    watch = 12.0

    def online_of(guid: int) -> str:
        return sql(
            "characters",
            f"SELECT online FROM {schemas()['characters']}.characters WHERE guid = {guid};",
        )

    for label, flag in (("ONLINE", 1), ("OFFLINE", 0)):
        found = _with_a_corpse(flag)
        if found is None:
            say(f"NOT RUN ({label}): no {label.lower()} character on this server has a corpse row")
            continue
        name, guid = found
        count, kind = corpse(guid)
        say(f"{label}: {name} (guid {guid}) has {count} corpse row(s), corpse_type {kind}, "
            f"online={online_of(guid)}")
        assert count > 0, "this stage refuses to run without a corpse"

        time.sleep(watch)
        control_count, _ = corpse(guid)
        say(f"   after {watch:.0f}s untouched it still has {control_count} "
            f"(online={online_of(guid)})")
        if control_count == 0:
            say(f"   NOT RUN ({label}): the corpse went on its own, so nothing here is the command's")
            continue

        tab = services().play
        out = tab.revive(name)
        say(f"   revive {name}: done={out.done} {(out.text or out.problem).strip()[:100]!r}")
        after = settled(lambda: corpse(guid)[0], lambda n: n == 0, seconds=watch)
        still = online_of(guid)
        say(f"   {watch:.0f}s after the command it has {after} corpse rows (online={still})")
        assert out.done, out.problem
        assert after == 0, f"the corpse survived the revive ({control_count} -> {after})"
        if flag == 1:
            say(f"   ONLINE: the corpse sat still for {watch:.0f}s and was gone {watch:.0f}s "
                "after the command")
        else:
            assert still == "0", (
                f"{name} logged in during the window, so the corpse going is not the command's"
            )
            health = row(name).get("health")
            say(f"   OFFLINE: the corpse is gone and {name} never logged in. Its row health is "
                f"still {health} -- which is the ONLY thing 8.4a and 8.4b read, and why both "
                "concluded an offline revive does nothing")
    say("revive stage finished")


# -- the arithmetic ---------------------------------------------------------


def stage_gear() -> None:
    """N worn pieces promise N mails on a cap-of-one tree, and send N.

    8.4b's clause names nineteen pieces because that is what a WotLK-shaped bot
    wears. The best-dressed character on THIS server wears twelve, and nothing
    can make it wear more without writing inventory rows by hand -- which would
    be the gate inventing its own evidence. What the clause is actually about is
    a set that does not fit in one mail, and on a cap of one that is any set of
    two or more. Both the twelve-piece wearer and the client lane's own
    character are pressed, because the second is the one a person will look at.
    """
    for who in (chosen()["wearer"], chosen()["offline"]):
        tab = services().play
        pieces, promised = tab.gear_set_size(who)
        had = mails(who)
        say(f"{who} is wearing {pieces} pieces and the button promises {promised} mails")
        assert pieces > 1, f"{who} wears {pieces} pieces, which fits in one mail and proves nothing"
        assert promised == pieces, (
            f"on a cap-of-{ENTRY.play.mail_item_cap} tree {pieces} pieces is {pieces} mails, "
            f"and the button promised {promised}"
        )
        out = tab.send_gear_set(
            who, to=who, subject="Your gear", body="Everything you were wearing"
        )
        arrived = settled(lambda: mails(who), lambda n: n >= had + promised, seconds=40) - had
        say(f"send gear : done={out.done} {(out.text or out.problem).strip()[:120]!r}")
        say(f"            mails that arrived: {arrived}")
        assert out.done, out.problem
        assert arrived == promised, f"promised {promised}, {arrived} arrived"
        with_items = int(
            sql(
                "characters",
                f"SELECT COUNT(*) FROM {schemas()['characters']}.mail m "
                f"JOIN {schemas()['characters']}.characters c ON c.guid = m.receiver "
                f"WHERE c.name = '{who}' AND m.has_items = 1;",
            )
        )
        one_each = sql(
            "characters",
            f"SELECT MAX(n) FROM (SELECT COUNT(*) AS n FROM {schemas()['characters']}.mail_items mi "
            f"JOIN {schemas()['characters']}.mail m ON m.id = mi.mail_id "
            f"JOIN {schemas()['characters']}.characters c ON c.guid = m.receiver "
            f"WHERE c.name = '{who}' GROUP BY mi.mail_id) AS per;",
        )
        say(f"            {with_items} of its mails carry items, and the fullest holds {one_each}")
        assert one_each == "1", f"a mail on this tree carried {one_each} items"
        say(f"clause PASSED for {who}: {pieces} pieces, {promised} promised, {arrived} arrived, "
            "one item in each")


def stage_case() -> None:
    """A name typed in the wrong case still finds the character.

    Pressed through `set_level` rather than `revive`: revive answers success for
    anything on this tree, so a lookup that failed would be invisible in it. The
    level is set to the value it already holds, so the READING is the server's
    sentence naming the character -- a wrong-case lookup that fell through would
    answer about a character that does not exist.
    """
    name = chosen()["offline"]
    tab = services().play
    for typed in (name.lower(), name.upper(), name.swapcase()):
        out = tab.set_level(typed, NEW_LEVEL)
        said = (out.text or out.problem).strip()
        say(f"typed {typed!r}: done={out.done} {said[:90]!r}")
        assert out.done, f"{typed!r} was not found: {out.problem}"
        assert name in said, f"the server's answer does not name {name}: {said!r}"
    missing = tab.set_level("NoSuchPersonHere", NEW_LEVEL)
    say(f"typed a name nobody has: done={missing.done} {missing.problem[:90]!r}")
    assert missing.done is False
    assert "NoSuchPersonHere" in missing.problem
    say("PASSED: every case finds the character, and a name nobody has is refused before "
        "the server is asked")


# -- what the client lane needs ---------------------------------------------


def stage_takeover() -> None:
    """Two presses of two other features, so a real client can see all of this.

    The realm row and the account password, both through the app -- a row
    written by hand here would leave the network fix unproved and the client
    lane blaming the client.
    """
    realm = lambda: sql(  # noqa: E731
        "auth", f"SELECT id, name, address, port FROM {schemas()['auth']}.realmlist;"
    )
    say(f"realm at the start: {realm()}")

    def press(mode: str, **extra) -> str:
        plan = networking.plan(ENTRY, mode, firewall="none", steamos=False, wsl=False, **extra)
        report = services().network_apply(plan)
        for line in getattr(report, "done", ()):
            say(f"  applied ({mode}): {line}")
        say(f"  restart required: {getattr(report, 'restart_required', None)}")
        return realm()

    # The row already carried the reachable address when this stage first ran --
    # an earlier gate had applied it -- so the press would have "passed" on a
    # state it did not produce. The ground is made by the SAME feature: a
    # loopback plan, which is a real thing a person picks and which writes an
    # address the Hyper-V host cannot possibly reach.
    grounded = press("loopback")
    say(f"realm after the loopback plan: {grounded}")
    assert REACHABLE not in grounded, "the loopback plan left the reachable address in the row"

    after = press("lan", lan_ip=REACHABLE)
    say(f"realm after the lan plan   : {after}")
    assert REACHABLE in after, "the realm still does not advertise an address the client can reach"

    name = chosen()["offline"]
    account_id = sql(
        "characters",
        f"SELECT account FROM {schemas()['characters']}.characters WHERE name = '{name}';",
    )
    username = sql(
        "auth", f"SELECT username FROM {schemas()['auth']}.account WHERE id = {account_id};"
    )
    verifier_before = sql(
        "auth", f"SELECT v FROM {schemas()['auth']}.account WHERE id = {account_id};"
    )
    admin = services().accounts
    assert admin is not None, "this install has no accounts feature wired"
    out = admin.set_password(username, TAKEOVER_PASSWORD)
    say(f"set_password on {username}: done={out.done} {(out.text or out.problem).strip()[:90]!r}")
    assert out.done, out.problem
    verifier_after = sql(
        "auth", f"SELECT v FROM {schemas()['auth']}.account WHERE id = {account_id};"
    )
    say(f"the verifier changed: {verifier_before[:16]}... -> {verifier_after[:16]}...")
    assert verifier_after != verifier_before, "the stored verifier did not change"
    state = row(name)
    say(f"THE CLIENT HALF: {username} / {TAKEOVER_PASSWORD}, character {name}, "
        f"level {state['level']}, map {state['map']}, at_login {state['at_login']}, "
        f"{mails(name)} mails waiting")


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

    wearer = chosen()["wearer"]
    hit = [
        i
        for i in range(view.character_list.count())
        if view.character_list.item(i).text().startswith(wearer)
    ]
    if hit:
        view.character_list.setCurrentRow(hit[0])
        view.grab().save(str(SHOTS / "3-the-promise.png"))
        say(f"the gear button on {wearer} reads: {view.send_gear_button.text()!r}")
    del app


if __name__ == "__main__":
    {
        "pick": stage_pick,
        "names": stage_names,
        "verb": stage_verb,
        "cap": stage_cap,
        "equipped": stage_equipped,
        "offline": stage_offline,
        "online": stage_online,
        "revive": stage_revive,
        "gear": stage_gear,
        "case": stage_case,
        "takeover": stage_takeover,
        "shots": stage_shots,
    }[sys.argv[1]]()
