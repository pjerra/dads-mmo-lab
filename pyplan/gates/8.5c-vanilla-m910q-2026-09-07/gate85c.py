"""The 8.5c live gate: browsing this server's bots on WoW Vanilla (CMaNGOS classic).

Usage:  <venv>/bin/python gate85c.py <stage>

    total       the app's total against the same clause run by hand, three ways
    refuse      a marker that cannot be read, on the live SQL with only the conf swapped
    warn        a readable marker matching nothing -- read off disk, not built in Python
    filterzero  the OTHER way a zero happens, so `warn`'s sentence has one cause
    nosplit     no registry named on a tree that has no registry
    page        paging and filtering, against the rows the server has
    races       the race -> faction map, parsed from THIS install's own ChrRaces.dbc
    who         chooses the who-list subjects from what the app listed, and says why

Read-only throughout. No server is started, stopped or reconfigured by this
script, and the live `~/vanilla-75b/etc/aiplayerbot.conf` is never edited: the
substituted markers live in copies under /tmp and only the bot-browsing seam is
pointed at them. The SQL reader stays the live one -- a fake install directory
would take the database password from /tmp, find none, and fail every query, and
`total is None` is true of that failure too, so the refusal would have been
photographed for the wrong reason (8.5b learned that on the box).

**This is 8.5b's shape, not 8.5a's file.** `gate85a.py` still pages with
`browser.page(offset=...)`, a signature the adversarial review of its own box
replaced with the `(name, guid)` cursor; copying it produces a TypeError on the
paging stage, mid-gate, on a box where the server had to be started for the run.
Nobody should copy that file again.

RUN, end to end, on 2026-09-07 between 20:59:53Z and 21:05:44Z with the stack up
and its pool full; `transcript.txt` beside this file is the run. What follows was
read off `~/vanilla-75b` with the server DOWN earlier that day and is asserted
here rather than discovered -- every one of them held when the server came up:

  * the marker is `RNDBOT`, `etc/aiplayerbot.conf:57`, with Min/Max 500 at :51-52
  * there is no `playerbots` schema in the install's volume, so `registry: null`
    in the catalog is right and the attribution clause is the constant `1 = 0`
  * `characters.characters` HAS a `race` column -- `characters.sql:644`,
    `tinyint(3) unsigned` -- so the faction pick needs no console fallback
  * `/who` is faction-filtered HERE because of a config line, not because
    AzerothCore behaved that way: `MiscHandler.cpp:159`
    `if (pl->GetTeam() != team && !allowTwoSideWhoList) continue;`, and this
    install's `etc/mangosd.conf:931` reads `AllowTwoSide.WhoList = 0`. The same
    handler also honours `GM.InWhoList.Level` (`mangosd.conf:1278` = 3) and the
    client's own level range.

Min/Max 500 is a number of SESSIONS and not of characters, which the run settled:
the tab counted 900 bot characters while 500 were online, at 20:59:53Z and again
at 21:05:07Z. The pool was full for the whole run, so no reading here is a
half-filled server being read early.

Every line number above and below was re-checked against this tree on 2026-09-07
after the run. Four inherited citations were WRONG and are corrected in place --
`dbreads.py:132-137` for the catalog-default fallback is really :144-158, the
resolver's blank refusal :166-173 is really :174-181, `botlist.py:138-144` is
really :147-153, and `apply.py:499-501` is really :500-503. They were all
plausible and all off; a citation is a claim like any other.
"""

from __future__ import annotations

import os
import shutil
import struct
import sys
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate85c" / "pylauncher"))
# Its OWN copy of the tree, and not the `~/gate84c/pylauncher` the box already
# had. That copy's `yulon/ui/controller_view.py` was a commit behind (md5
# 5b10b774 against HEAD's 1db1d8b1, 2026-09-07): the manifest-prompt work landed
# after it was taken. Nothing on the bot path differs, which is the point --
# a gate cannot report that a difference was harmless without first knowing
# there was one, and a shared checkout on a box where other lanes run is a
# thing that moves under the run.

from yulon import botlist, dbreads  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "vanilla-75b"
ENTRY = load_catalog().get("wow-vanilla")
SHOTS = Path.home() / "gate85c-shots"

OPS = ENTRY.observability
TABLE = f"{ENTRY.databases.characters}.{OPS.characters.table}"
AUTH = ENTRY.schema_map()["auth"]


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def reader():
    """The SQL seam the tab itself uses."""
    from yulon.ui import controller_view as view_module  # noqa: PLC0415

    password = view_module._db_password(ENTRY, SERVER_DIR)
    return view_module._sql_for(ENTRY, password, wsl_distro=None)


def services() -> ControllerServices:
    return ControllerServices.for_entry(ENTRY, SERVER_DIR)


def by_hand(statement: str) -> str:
    """A hand query through the app's own reader.

    Named for what it is: this shares `DockerSql` with the code under test, so
    it proves the SQL and not the transport. The genuinely independent reading
    is the one typed at a shell in `transcript.txt`, and it is written as

        MYSQL_PWD=$(cat ~/vanilla-75b/.db_password) \\
          docker exec -e MYSQL_PWD -i vanilla-db mariadb -uroot -N -B -e '<sql>'

    with the password in the ENVIRONMENT and never in argv. `apply.py:500-503`
    refuses to put a statement in argv for exactly this reason -- argv is
    world-readable (`ps`, Task Manager, /proc/<pid>/cmdline) -- and m910q is a
    shared box where other gates run at the same time.
    """
    return reader().query("characters", statement).strip()


def marker() -> dbreads.Marker:
    answer = dbreads.resolve_marker(ENTRY, SERVER_DIR)
    assert answer.marker is not None, answer.problem
    return answer.marker


def fake_conf(name: str, value: str) -> Path:
    """A copy of the live install directory with one setting rewritten.

    A COPY OF THE DIRECTORY, not of the file: `dbreads.resolve_marker` opens
    `<dir>/etc/aiplayerbot.conf`, so a conf dropped at the root of a temporary
    directory is simply not found -- and a not-found conf is NOT a refusal here.
    `dbreads.py:144-158` returns the catalog default on FileNotFoundError, and
    wow-vanilla's catalog default is `rndbot`, which is this install's live
    value. The blank-marker stage would then have answered with a working marker
    and a plausible non-zero total (adversarial review of 8.5c's plan,
    2026-09-07). `gate81c.py:92-97` had the shape right; this is it.
    """
    root = Path("/tmp") / name
    shutil.rmtree(root, ignore_errors=True)
    (root / "etc").mkdir(parents=True)
    key = OPS.bots.prefix_conf_key
    live = (SERVER_DIR / OPS.bots.prefix_conf_file).read_text(encoding="utf-8", errors="replace")
    out = [f"{key} = {value}" if line.startswith(key) else line for line in live.splitlines()]
    (root / OPS.bots.prefix_conf_file).write_text("\n".join(out) + "\n", encoding="utf-8")
    return root


def with_marker(value: str, name: str) -> ControllerServices:
    """Live services with ONLY the conf substituted."""
    from yulon.ui import controller_view as view_module  # noqa: PLC0415

    made = services()
    made.bots = view_module._BotBrowser(ENTRY, fake_conf(name, value), reader())
    return made


def view_for(made: ControllerServices):
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    app = QApplication.instance() or QApplication([])
    view = ControllerView(ENTRY, made, status_poll_ms=0, job_runner=run_inline)
    view.resize(940, 640)
    view._tabs.setCurrentIndex(
        next(i for i in range(view._tabs.count()) if view._tabs.tabText(i) == "Bots")
    )
    view._gate_app = app
    SHOTS.mkdir(parents=True, exist_ok=True)
    return view


# --------------------------------------------------------------- clause (1)


def stage_total() -> None:
    """The total equals the same clause run by hand.

    Three readings: the app's, the app's own clause run by hand, and a statement
    typed out here with nothing taken from the module under test. The clock is
    printed beside each because the bot pool is written by the module at runtime
    and a mismatch caused by the pool moving must not be read as a defect.
    """
    live = marker()
    say(f"the marker this install is running with: prefix={live.prefix!r} source={live.source!r}")
    assert live.prefix == "RNDBOT" and live.source == "conf", (
        "etc/aiplayerbot.conf:57 read RNDBOT on 2026-09-07; re-read it before going on"
    )

    page = services().bots.page()
    say(
        f"the tab says: total={page.total} by_registry={page.by_registry} "
        f"by_prefix={page.by_prefix} rows={len(page.bots)}"
    )

    clause = dbreads.bot_clause(ENTRY, live)
    say(f"the app's identity clause: {clause}")
    assert " OR " not in clause, "this tree has one arm; a second one is a registry it has not got"
    from_clause = by_hand(f"SELECT COUNT(*) FROM {TABLE} WHERE ({clause});")
    say(f"by hand, with the app's own clause: {from_clause}")

    typed = by_hand(
        "SELECT COUNT(*) FROM characters.characters WHERE account IN "
        "(SELECT id FROM realmd.account WHERE UPPER(username) LIKE 'RNDBOT%');"
    )
    say(f"by hand, written independently of the app: {typed}")

    everyone = by_hand(f"SELECT COUNT(*) FROM {TABLE};")
    say(f"characters on this server altogether: {everyone}")

    assert page.total == int(from_clause) == int(typed), "the tab and the database disagree"
    # 8.1c read 500 bots of 900 characters here on 2026-09-06, so these are two
    # different numbers on this install and the equality above is a constraint
    # rather than a tautology.
    assert page.total not in (0, int(everyone)), (
        f"total {page.total} against {everyone} characters: this proves nothing either way"
    )
    say(f"CLAUSE 1 PASSED: {page.total} bots of {everyone} characters, three ways")


# --------------------------------------------------------------- clause (2)


def stage_refuse() -> None:
    """An unreadable marker refuses to answer rather than reporting zero.

    TWO refusals live on this path and they are not the same one. Through
    `_BotBrowser.page()` the resolver refuses first, and its sentence names the
    conf KEY and the FILE (`dbreads.py:174-181`); `botlist.page`'s own refusal
    (`botlist.py:147-153`, "an empty marker would match every account") is
    reached only by handing it a `Marker(prefix='')` directly, which is what
    8.5a did. Both are exercised here and each is labelled, because "the marker
    refused" said of a screenshot does not say which of them.
    """
    made = with_marker("", "gate85c-blank")
    page = made.bots.page()
    say(f"a blank marker: total={page.total} rows={len(page.bots)} problem={page.problem!r}")
    assert page.total is None and page.bots == []
    assert OPS.bots.prefix_conf_key in page.problem
    assert "aiplayerbot.conf" in page.problem
    assert "blank" in page.problem

    # The second refusal: botlist's own, with the resolver bypassed.
    direct = botlist.page(reader(), ENTRY, dbreads.Marker(prefix="", source="conf"))
    say(f"and botlist's own refusal, given an empty Marker: problem={direct.problem!r}")
    assert direct.total is None and "every account" in direct.problem

    # What the live install answers at the same moment, so the refusal is known
    # to come from the substituted conf and not from a sick server.
    say(f"the live install at the same moment: total={services().bots.page().total}")

    view = view_for(made)
    view.refresh_bots()
    view.grab().save(str(SHOTS / "2-refused.png"))
    say(f"the tab reads: {view.bot_summary.text()!r}")
    assert view.bot_list.count() == 0
    assert OPS.bots.prefix_conf_key in view.bot_summary.text()
    say("CLAUSE 2 PASSED: both refusals named what stopped them, and neither showed a zero")


# --------------------------------------------------------------- clause (3)


def stage_warn() -> None:
    """A READABLE marker matching nothing warns, neither reporting zero.

    "Readable" is the load-bearing word and it is why the marker is written into
    a conf copy and read back through `resolve_marker` rather than constructed
    in Python. A `Marker(prefix='NOSUCHBOT', source='conf')` built here would
    exercise `botlist.page`'s `if total == 0` branch, which
    `test_a_marker_matching_nothing_while_characters_exist_warns` already covers
    in CI; the live gate adds something only when the resolver, the clause
    builder, the server and the sentence are all in the loop (adversarial review
    of 8.5c's plan, 2026-09-07). `gate81c.py:92-105` did it this way on this same
    install.

    8.5b reached this clause first, on TBC, and settled its wording: the tab
    LEADS with the warning and prints no count, because "warns, neither
    reporting zero" was read at its word. This tree inherits that reading and
    re-measures the behaviour.
    """
    made = with_marker("NOSUCHBOT", "gate85c-nomatch")
    resolved = dbreads.resolve_marker(ENTRY, fake_conf("gate85c-nomatch", "NOSUCHBOT"))
    say(f"the marker READ BACK off the conf copy: {resolved.marker}")
    assert resolved.marker is not None and resolved.marker.source == "conf"

    page = made.bots.page()
    say(
        f"a marker matching nothing: total={page.total} by_registry={page.by_registry} "
        f"by_prefix={page.by_prefix} problem={page.problem!r} warning={page.warning!r}"
    )

    hand_zero = by_hand(
        "SELECT COUNT(*) FROM characters.characters WHERE account IN "
        "(SELECT id FROM realmd.account WHERE UPPER(username) LIKE 'NOSUCHBOT%');"
    )
    everyone = by_hand(f"SELECT COUNT(*) FROM {TABLE};")
    say(f"by hand, the same marker: {hand_zero}; characters on the server: {everyone}")

    # The ground this stage stands on. Without it the warning is a sentence
    # about a server that might simply have no bots, and "the assertion was
    # already true before the command" is 8.4a's own defect. Read at the same
    # moment through the same seam, with only the conf swapped.
    live = services().bots.page()
    say(f"the live install at the same moment: total={live.total} warning={live.warning!r}")
    assert live.total and live.warning == "", (
        "the live marker is not answering either; this stage would prove nothing"
    )

    assert page.problem == "", "it failed rather than answering"
    assert page.total == 0 == int(hand_zero)
    assert "NOSUCHBOT" in page.warning
    assert everyone in page.warning, "the warning must carry the number that makes zero a lie"

    view = view_for(made)
    view.refresh_bots()
    view.grab().save(str(SHOTS / "3-warned.png"))
    said = view.bot_summary.text()
    say(f"the tab reads: {said!r}")
    assert said.startswith("no character matched"), said
    assert everyone in said
    assert "0 bots" not in said
    assert "registry" not in said, "it named a table this install has not got"
    say("CLAUSE 3 PASSED: the warning leads, the zero is not reported, and no registry is named")


def stage_filterzero() -> None:
    """The OTHER way a zero happens, so clause 3's sentence has exactly one cause.

    The name filter is folded into the same WHERE the total is counted over, so
    before 8.5c a filter matching nothing drove `total == 0` down the marker's
    branch: the tab said *"no character matched the bot marker 'RNDBOT'"* about
    a marker that had matched every bot on the server a keystroke earlier. Two
    things were wrong with that at once -- the sentence was false, and it was
    the SAME sentence `warn` above photographs as the whole proof of clause 3,
    so the gate could not have said which cause it had caught.

    This stage is the evidence that they are now two answers. It runs with the
    LIVE marker, which is what makes the point: the marker is fine and the
    filter is the thing that found nothing.
    """
    browser = services().bots
    unfiltered = browser.page()
    say(f"the live marker, unfiltered: total={unfiltered.total} warning={unfiltered.warning!r}")
    assert unfiltered.total, "there are no bots here at all; nothing to filter"
    assert unfiltered.warning == ""

    # `%` and `_` are the two LIKE wildcards, and this is the escaping
    # (`botlist.py:116-125`, `!` because NO_BACKSLASH_ESCAPES is real) exercised
    # against a live MariaDB rather than against a stub. Unescaped, `%_` matches
    # every name of at least one character and the total would be the whole
    # list; escaped, it is a literal two-character name nobody has.
    for pattern in ("%_", "Zzzqqx"):
        found = browser.page(name_like=pattern)
        say(f"filtered on {pattern!r}: total={found.total} warning={found.warning!r}")
        assert found.total == 0, f"{pattern!r} matched {found.total} rows; the escaping is off"
        assert "marker" not in found.warning, found.warning
        assert repr(pattern) in found.warning or pattern in found.warning, found.warning
        assert str(unfiltered.total) in found.warning, (
            "the filtered sentence must count against the BOTS it excluded, "
            "not against the characters table"
        )

    view = view_for(services())
    view.refresh_bots()
    view.bot_filter.setText("%_")
    view.filter_bots()
    view.grab().save(str(SHOTS / "6-filter-found-nothing.png"))
    said = view.bot_summary.text()
    say(f"the tab reads: {said!r}")
    assert "no character matched" not in said, "the tab blamed the marker for the filter"
    say("PASSED: a filtered zero and a marker zero are two different sentences")


# ----------------------------------------------------- this box's own reading


def stage_nosplit() -> None:
    """As 8.5a minus the type split, read off the real widgets.

    Not a clause of this box's definition of done -- checklist.md:2497-2498 drops
    the split clause, it does not add a suppression requirement -- but it is
    behaviour 8.5b shipped to every registry-less tree, so this tree gates it as
    a REGRESSION rather than as new work. The presence assertions sit beside the
    absence ones deliberately: `"registry" not in ""` is true of a label that
    says nothing at all.
    """
    say(f"has_registry({ENTRY.id}) = {botlist.has_registry(ENTRY)}")
    say(f"the attribution clause on this tree: {botlist._registry_clause(ENTRY, OPS)!r}")
    assert botlist.has_registry(ENTRY) is False
    assert botlist._registry_clause(ENTRY, OPS) == "1 = 0"

    schemas = by_hand("SELECT GROUP_CONCAT(schema_name) FROM information_schema.schemata;")
    say(f"the schemas this install really has: {schemas}")
    assert "playerbots" not in schemas.lower(), "there IS a registry here; re-measure the catalog"

    view = view_for(services())
    view.refresh_bots()
    view.grab().save(str(SHOTS / "1-bots.png"))
    said = view.bot_summary.text()
    rows = [view.bot_list.item(i).text() for i in range(view.bot_list.count())]
    say(f"summary: {said!r}")
    say(f"first rows: {rows[:3]}")

    total = services().bots.page().total
    assert f"{total} bots" in said
    assert "registry" not in said
    assert view.bot_list.count() == botlist.PAGE_SIZE
    assert "prefix" not in rows[0] and "registry" not in rows[0]
    assert rows[0].split(" — ")[0].isalpha(), "the row lost its name along with its suffix"
    say("PASSED: the count, the page and the rows, with no signal named")


# ------------------------------------------------------------------- paging


def stage_page() -> None:
    """Paging and filtering, which the list needs to be usable at all."""
    browser = services().bots
    first = browser.page()
    say(
        f"page 1: {[b.name for b in first.bots][:4]} … ({len(first.bots)} rows), "
        f"next_after={first.next_after}"
    )
    assert len(first.bots) == botlist.PAGE_SIZE and first.next_after is not None

    second = browser.page(after=first.next_after)
    say(f"page 2: {[b.name for b in second.bots][:4]} … ({len(second.bots)} rows)")
    assert len(second.bots) == botlist.PAGE_SIZE
    assert {b.name for b in first.bots}.isdisjoint({b.name for b in second.bots})
    # A cursor read, so an overlap would be a real defect and not OFFSET drift.
    assert second.next_after > first.next_after

    stem = second.bots[0].name[:3]
    found = browser.page(name_like=stem)
    say(f"filtered on {stem!r}: total={found.total} first={[b.name for b in found.bots][:5]}")
    assert all(b.name.upper().startswith(stem.upper()) for b in found.bots)
    assert 0 < found.total < botlist.PAGE_SIZE, (
        f"a filter that returned {found.total} did nothing the unfiltered page had not done"
    )

    # The hand query is the app's whole WHERE and not the LIKE alone: the LIKE
    # alone also counts the characters here that are NOT bots, and the gate
    # would fail by one on a correct app (8.5b, 2026-09-07).
    clause = dbreads.bot_clause(ENTRY, marker())
    hand = by_hand(
        f"SELECT COUNT(*) FROM {TABLE} WHERE ({clause}) AND name LIKE '{stem}%' ESCAPE '!';"
    )
    say(f"by hand, the whole clause and the same LIKE: {hand}")
    assert found.total == int(hand)

    view = view_for(services())
    view.refresh_bots()
    view.next_bot_page()
    view.grab().save(str(SHOTS / "4-page-two.png"))
    say(f"page two on the tab: {view.bot_summary.text()!r}")
    view.bot_filter.setText(stem)
    view.filter_bots()
    view.grab().save(str(SHOTS / "5-filtered.png"))
    say(f"filtered on the tab: {view.bot_summary.text()!r}")
    say("PASSED: two pages that do not overlap, and a filter checked by hand")


# ------------------------------------------------- the map, measured not typed


def races() -> dict[int, str]:
    """The race -> faction map, parsed from THIS install's extracted client data.

    `Player::TeamForRace` (`Player.cpp:6260-6277`) reads `ChrRacesEntry::TeamID`
    and answers ALLIANCE for 7 and HORDE for 1; `DBCStructure.h:190` puts TeamID
    at field index 8 and `:182` puts the race id at 0. So the map is a property
    of the DBC this server loaded and not of the expansion, and 8.5a's numbers
    (5 Undead, 11 Draenei) are AzerothCore's -- race 11 does not exist in this
    file at all.

    Parsed here rather than pasted so that the gate re-measures it on the box.
    Read on 2026-09-07 from `~/vanilla-75b/data/dbc/ChrRaces.dbc` (9 records, 29
    fields, 116-byte records): 1 Human, 3 Dwarf, 4 Night Elf, 7 Gnome and
    9 Goblin ALLIANCE; 2 Orc, 5 Undead, 6 Tauren, 8 Troll HORDE. Goblin being
    Alliance in this file is the reason the map is read rather than assumed.
    """
    path = SERVER_DIR / "data" / "dbc" / "ChrRaces.dbc"
    blob = path.read_bytes()
    magic, count, fields, size, _strings = struct.unpack("<4siiii", blob[:20])
    assert magic == b"WDBC", f"{path} is not a DBC"
    out: dict[int, str] = {}
    for i in range(count):
        row = struct.unpack(f"<{fields}i", blob[20 + i * size : 20 + (i + 1) * size])
        out[row[0]] = {7: "ALLIANCE", 1: "HORDE"}.get(row[8], f"team{row[8]}")
    return out


def race_names() -> dict[int, str]:
    """Each race's own name, so a person can check the number against a word.

    Field 17 is `m_name_lang`'s enUS slot (`DBCStructure.h:199`), an offset into
    the string block that begins at `records * size + 20`. Added after the first
    run: `races()` answered `race 6: HORDE`, and "6 is Tauren" was a thing this
    session would otherwise have known only by remembering it. A faction handed
    to another lane as a bare number is a fact nobody downstream can check.
    """
    path = SERVER_DIR / "data" / "dbc" / "ChrRaces.dbc"
    blob = path.read_bytes()
    _magic, count, fields, size, _strings = struct.unpack("<4siiii", blob[:20])
    block = 20 + count * size
    out: dict[int, str] = {}
    for i in range(count):
        row = struct.unpack(f"<{fields}i", blob[20 + i * size : 20 + (i + 1) * size])
        start = block + row[17]
        out[row[0]] = blob[start : blob.index(b"\0", start)].decode("utf-8", "replace")
    return out


def stage_races() -> None:
    said, named = races(), race_names()
    for race, team in sorted(said.items()):
        say(f"race {race} ({named[race]}): {team}")
    assert named[1] == "Human" and named[6] == "Tauren", (
        f"the name block does not read as a race list: {named}"
    )
    assert said.get(11) is None, "race 11 exists here; 8.5a's map may apply after all"
    say(f"PASSED: the map came off {SERVER_DIR / 'data/dbc/ChrRaces.dbc'}, not off a sibling gate")


# --------------------------------------------------------------- clause (4)


def stage_who() -> None:
    """Choose the who-list subjects from what the app listed, and say why.

    The clause is carried by the POSITIVE find: a bot this tab listed as online,
    named in the client's `/who` answer, with `Wow.exe` alive in the driver's log
    at the moment of the photograph. The opposite-faction shot is CORROBORATION
    and nothing more -- "0 players total" is also what a client that never
    entered the world, a chat line that did not open, a mistyped name or a bot
    that logged out between the query and the keystroke produces, and 8.5a's own
    README records that photograph being read as a defect in the list before the
    faction reason was found. The reason is documented instead:
    `MiscHandler.cpp:159` plus `etc/mangosd.conf:931 AllowTwoSide.WhoList = 0`.

    The primary subject needs no faction map at all: it is picked with
    `race = <the asking character's race>`, and same race is same team by
    construction. The map is used only to choose the corroborating subject, and
    it is measured by `races()` above rather than inherited.
    """
    asker = by_hand(
        "SELECT c.guid, c.name, c.race, c.level, c.online, c.at_login, a.username "
        f"FROM {TABLE} c JOIN {AUTH}.account a ON a.id = c.account "
        f"WHERE UPPER(a.username) NOT LIKE '{marker().prefix.upper()}%';"
    )
    say(f"the characters here that are NOT bots (the asker is one of these):\n{asker}")
    rows = [line.split("\t") for line in asker.splitlines() if line.strip()]
    assert rows, (
        "there is no non-bot character on this server. 8.1c's Asdff (guid 901, account "
        "VANGATE) was last seen 2026-09-06 and 8.3c's GATE83C never entered the world; "
        "one has to exist before the client half can run, and making one through the "
        "1.12.1 creation screen unattended is its own piece of work."
    )
    asker_race = int(rows[0][2])
    team = races()[asker_race]
    say(
        f"asking character: {rows[0][1]}, race {asker_race} "
        f"({race_names()[asker_race]}, {team}), level {rows[0][3]}"
    )

    browser = services().bots
    online: list[str] = []
    after = None
    for _ in range(6):
        listed = browser.page(after=after)
        online += [b.name for b in listed.bots if b.online]
        after = listed.next_after
        if after is None:
            break
    say(f"the app listed {len(online)} online bots in the pages it was asked for")
    assert online, "no online bot on these pages; the pool rotates, ask again"

    names = ", ".join(f"'{n}'" for n in online)
    same = by_hand(
        f"SELECT name, level, race FROM {TABLE} WHERE name IN ({names}) "
        f"AND online = 1 AND race = {asker_race} "
        # CAST because `level` is UNSIGNED on this tree and `level - 1` on a
        # level-1 bot underflows: "BIGINT UNSIGNED value is out of range"
        # (8.5b measured that on its own first run).
        f"ORDER BY ABS(CAST(level AS SIGNED) - {int(rows[0][3])}), name;"
    )
    say(f"online bots of the asker's OWN race, nearest its level first:\n{same}")
    assert same.strip(), "no online bot shares the asker's race on these pages; ask again"

    opposite = sorted(r for r, t in races().items() if t != team and t in ("ALLIANCE", "HORDE"))
    unteamed = sorted(r for r, t in races().items() if t not in ("ALLIANCE", "HORDE"))
    say(f"races on the other side, per this install's DBC: {opposite} (unteamed: {unteamed})")
    across = by_hand(
        f"SELECT name, level, race FROM {TABLE} WHERE name IN ({names}) "
        f"AND online = 1 AND race IN ({', '.join(str(r) for r in opposite)}) ORDER BY name LIMIT 1;"
    )
    say(f"an online bot on the other side (corroboration only):\n{across}")

    say(
        "PASS CRITERION: the same-race subject appears in the client's /who answer, with "
        "Wow.exe alive in the driver's log at the shot. The client's own wording for an "
        "empty list is READ OFF the screenshot; 8.5a's '0 players total' is a 3.3.5a "
        "string and is not asserted here."
    )


def stage_still_online(name: str) -> None:
    """Re-check by hand, immediately before the client types."""
    answer = by_hand(f"SELECT name, level, online FROM {TABLE} WHERE name = '{name}';")
    say(f"{name} right now: {answer!r}")
    assert answer.split("\t")[-1] == "1", f"{name} logged out; choose again"


if __name__ == "__main__":
    if sys.argv[1] == "still-online":
        stage_still_online(sys.argv[2])
    else:
        {
            "total": stage_total,
            "refuse": stage_refuse,
            "warn": stage_warn,
            "filterzero": stage_filterzero,
            "nosplit": stage_nosplit,
            "page": stage_page,
            "races": stage_races,
            "who": stage_who,
        }[sys.argv[1]]()
