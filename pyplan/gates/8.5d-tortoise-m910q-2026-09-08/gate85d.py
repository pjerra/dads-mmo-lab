"""The 8.5d live gate: browsing this server's bots on WoW Tortoise.

RUN 2026-09-07 23:13Z-23:14Z on m910q against the live Tortoise stack (which had
been up 24 minutes when the first stage started). The ten server-side stages all
passed; `transcript.txt` beside this file is their output with the clock on every
line, and `README.md` is the record. Clause (4)'s client half was NOT driven --
this lane held m910q but not the game client on `vmhost` -- so everything below
about `/who` is still a prediction, and everything about the database, the confs,
the DBC and the app is now a reading.

Written 2026-09-08 (local) with the stack DOWN, which is why the header used to
say "NOT YET RUN"; the predictions it carried were checked one at a time by the
stages, and the four that were wrong are named in README.md.

Usage:  ~/gate81b-venv/bin/python gate85d.py <stage>

    total       clause (1): the total against the same clause run by hand, three ways
    refuse      clause (2): a marker that cannot be read, on the live SQL with the conf swapped
    warn        clause (3): a readable marker matching nothing, read back off disk
    filterzero  the OTHER cause of a zero, so `warn`'s sentence has exactly one
    nosplit     the inherited 8.5b reading: no registry named where there is no registry
    page        paging and filtering, which the list needs to be usable at all
    races       the race -> faction map, parsed from THIS install's own ChrRaces.dbc
    wholaw      the who-list rules THIS fork and THIS conf really apply
    realmrow    the realm row the client will be sent to, before the client is started
    who         clause (4): choose the who-list subjects from what the app listed, and say why

    still-online <name>   re-check a chosen subject immediately before the client types

**This is 8.5b's shape (`pyplan/gates/8.5b-tbc-m910q-2026-09-07/gate85b.py`),
with the two stages 8.5c added.** It is deliberately NOT a copy of `gate85a.py`:
that file still pages with `browser.page(offset=...)`, a signature the
adversarial review of its own box replaced with the `(name, guid)` cursor, and
copying it produces a TypeError mid-gate on a box where a server had to be
started for the run.

READ-ONLY. Nothing here starts, stops or reconfigures a server, and the live
`~/tortoise-server/etc/aiplayerbot.conf` is never edited: the substituted
markers live in copies under /tmp and only the bot-browsing seam is pointed at
them. The SQL reader stays the live one -- a fake install directory would take
the database password from /tmp, find none, and fail every query, and
`total is None` is true of that failure too, so the refusal would be
photographed for the wrong reason (8.5b learned that on the box).

WHAT WAS MEASURED BEFORE THIS FILE WAS WRITTEN, all of it off the real install
at `/home/pk/tortoise-server` on m910q with the stack DOWN, on 2026-09-08:

* `etc/aiplayerbot.conf:63` reads `AiPlayerbot.RandomBotAccountPrefix = RNDBOT`,
  once, at column 0; `:64` `RandomBotAccountCount = 100`; `:57-58` Min/Max
  RandomBots = 500. So `resolve_marker` should answer
  `Marker(prefix='RNDBOT', source='conf')`.
* the module's own compiled default is `"rndbot"`
  (`modules/mod-playerbots/src/playerbot/PlayerbotAIConfig.cpp:545`), which is
  what the catalog's `account_prefix` fallback holds -- this fork's own value,
  not a sibling's.
* the schemas on disk are `tw_char, tw_logon, tw_logs, tw_world` plus the MySQL
  system ones. There is no `playerbots` schema, so `has_registry` is False.
* `tw_char.characters` is **MyISAM** on this fork and its `.MYI` header read
  901 records / 39 deleted; `tw_logon.account` holds `RNDBOT0 … RNDBOT97`.
  Both were read off the raw files, and both are re-asked here through mysqld.
* `etc/mangosd.conf:919` `AllowTwoSide.WhoList = 1` -- the OPPOSITE of the
  Vanilla install 8.5c gated -- and `:1644` `GM.InWhoList.Level = 3`.
* `data/dbc/ChrRaces.dbc` holds **10** races: this client has Goblin (9, HORDE)
  and High Elf (10, ALLIANCE), and Goblin is HORDE here while the Vanilla
  install's own DBC put Goblin on the ALLIANCE side. A faction map inherited
  from a sibling gate would be wrong on this tree.
* the who handler is `src/game/Handlers/MiscHandler.cpp` -- the filters at
  `:125-133` and the **30-second cooldown at :253-255**, which is this fork's
  and matters more to the client half than anything else on this page.

Every one of those is re-asked by a stage below rather than trusted, and the
line numbers are claims like any other: re-check them on the box before the
README quotes them.
"""

from __future__ import annotations

import os
import shutil
import struct
import sys
from datetime import UTC, datetime
from pathlib import Path

# Its OWN copy of the tree, taken at the moment of the run, and not a copy an
# earlier box left behind: m910q is shared, `~/gate84c/pylauncher` was a commit
# behind when 8.5c looked (md5 5b10b774 against HEAD's 1db1d8b1), and a gate
# cannot report that a difference was harmless without first knowing there was
# one. Record the md5 of botlist.py, dbreads.py and controller_view.py in the
# transcript, as 8.5c did.
sys.path.insert(0, str(Path.home() / "gate85d" / "pylauncher"))

from yulon import botlist, dbreads  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "tortoise-server"
ENTRY = load_catalog().get("wow-tortoise")
SHOTS = Path.home() / "gate85d-shots"

OPS = ENTRY.observability
TABLE = f"{ENTRY.databases.characters}.{OPS.characters.table}"
AUTH = ENTRY.schema_map()["auth"]


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def reader():
    """The SQL seam the tab itself uses, built the way 8.4b's gate built it."""
    from yulon.ui import controller_view as view_module  # noqa: PLC0415

    password = view_module._db_password(ENTRY, SERVER_DIR)
    return view_module._sql_for(ENTRY, password, wsl_distro=None)


def services() -> ControllerServices:
    return ControllerServices.for_entry(ENTRY, SERVER_DIR)


def by_hand(statement: str) -> str:
    """A hand query through the app's own reader.

    Named for what it is: this shares `DockerSql` with the code under test, so
    it proves the SQL and not the transport. The genuinely independent reading
    is the one typed at a shell and pasted into `transcript.txt`:

        MYSQL_PWD=$(cat ~/tortoise-server/.db_password) \\
          docker exec -e MYSQL_PWD -i tortoise-db mariadb -uroot -N -B -e '<sql>'

    with the password in the ENVIRONMENT and never in argv. `apply.py:500-503`
    refuses to put a statement in argv for that reason -- argv is world-readable
    (`ps`, /proc/<pid>/cmdline) -- and m910q is a shared box where other lanes
    run at the same time as this one.
    """
    return reader().query("characters", statement).strip()


def marker() -> dbreads.Marker:
    answer = dbreads.resolve_marker(ENTRY, SERVER_DIR)
    assert answer.marker is not None, answer.problem
    return answer.marker


def fake_conf(name: str, value: str) -> Path:
    """A copy of the live install DIRECTORY with one setting rewritten.

    A copy of the directory, not of the file, and the path is built from the
    catalog rather than spelled `etc/`:

        target = root / entry.observability.bots.prefix_conf_file
        target.parent.mkdir(parents=True)

    which is `gate81c.py:93-107`'s shape. `dbreads.resolve_marker` opens
    `<server_dir>/<prefix_conf_file>`; a conf dropped at the root of a temporary
    directory is simply NOT FOUND, and a not-found conf is not a refusal --
    `dbreads.py:144-158` returns the catalog default on FileNotFoundError.

    On THIS tree that failure is invisible rather than loud. The catalog default
    is `rndbot`, `bot_clause` upper-cases both sides, and this install's real
    accounts are `RNDBOT0…`: so a mislocated conf resolves to a WORKING marker
    and the blank-marker stage would answer with a full, plausible bot count and
    refuse nothing -- while photographing itself as proof of the opposite. A
    lane hit exactly that earlier tonight on another box.
    """
    root = Path("/tmp") / name
    shutil.rmtree(root, ignore_errors=True)
    target = root / OPS.bots.prefix_conf_file
    target.parent.mkdir(parents=True)
    key = OPS.bots.prefix_conf_key
    live = (SERVER_DIR / OPS.bots.prefix_conf_file).read_text(encoding="utf-8", errors="replace")
    out = [f"{key} = {value}" if line.startswith(key) else line for line in live.splitlines()]
    target.write_text("\n".join(out) + "\n", encoding="utf-8")
    # Assert the copy is where the resolver will look, rather than trusting the
    # two lines above: this is the exact mistake the docstring is about.
    assert (root / OPS.bots.prefix_conf_file).is_file(), root
    return root


def with_marker(value: str, name: str) -> ControllerServices:
    """Live services with ONLY the conf substituted.

    `ControllerServices.for_entry(ENTRY, fake_dir)` would read the database
    password out of /tmp, find none, fall back to a literal and fail every
    query -- and `total is None` is true of that failure as well as of the
    refusal this gate is trying to prove. So the services are built against the
    real install and only the bot-browsing seam is repointed.
    """
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
    view._gate_app = app  # keep it alive for as long as the view is
    SHOTS.mkdir(parents=True, exist_ok=True)
    return view


# --------------------------------------------------------------- clause (1)


def stage_total() -> None:
    """CLAUSE (1): the total equals the same clause run by hand.

    GROUND: the live install, which this stage does not write to. Three
    readings and not two -- the app's, the app's own clause run by hand, and a
    statement typed out here with nothing taken from the module under test --
    with the clock beside each, because the bot pool is written by the module at
    runtime and a mismatch caused by the pool moving must not be read as a
    defect.
    """
    live = marker()
    say(f"the marker this install is running with: prefix={live.prefix!r} source={live.source!r}")
    assert live.prefix == "RNDBOT" and live.source == "conf", (
        "etc/aiplayerbot.conf:63 read `AiPlayerbot.RandomBotAccountPrefix = RNDBOT` on "
        "2026-09-08 with the stack down; re-read it before going on"
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

    # Typed out here on purpose, with nothing taken from the module under test,
    # and with THIS fork's schema names: tw_char / tw_logon, never
    # characters / realmd, which is what the two sibling gates say.
    typed = by_hand(
        "SELECT COUNT(*) FROM tw_char.characters WHERE account IN "
        "(SELECT id FROM tw_logon.account WHERE UPPER(username) LIKE 'RNDBOT%');"
    )
    say(f"by hand, written independently of the app: {typed}")

    everyone = by_hand(f"SELECT COUNT(*) FROM {TABLE};")
    say(f"characters on this server altogether: {everyone}")

    assert page.total == int(from_clause) == int(typed), "the tab and the database disagree"
    # The .MYI header read 901 records with the stack down on 2026-09-08 and
    # nothing on disk said how many of them are bots. If the two numbers turn
    # out to be equal, the equality above is a tautology and this clause has
    # been proved of nothing -- which is a finding, not a pass.
    assert page.total not in (0, int(everyone)), (
        f"total {page.total} against {everyone} characters: this proves nothing either way"
    )
    say(f"CLAUSE 1 PASSED: {page.total} bots of {everyone} characters, three ways")


# --------------------------------------------------------------- clause (2)


def stage_refuse() -> None:
    """CLAUSE (2): an unreadable marker refuses to answer rather than reporting zero.

    GROUND: the live install answering at the same moment through the same
    seam, which is what says the refusal came from the substituted conf and not
    from a sick server or a wrong password.

    TWO refusals live on this path and they are not the same one. Through
    `_BotBrowser.page()` the resolver refuses first and its sentence names the
    conf KEY and the FILE (`dbreads.py:174-181`); `botlist.page`'s own refusal
    (`botlist.py:147-153`, "an empty marker would match every account") is
    reached only by handing it a `Marker(prefix='')` directly. Both are
    exercised, and each is labelled, because "the marker refused" said of a
    screenshot does not say which of them.
    """
    made = with_marker("", "gate85d-blank")
    page = made.bots.page()
    say(f"a blank marker: total={page.total} rows={len(page.bots)} problem={page.problem!r}")
    assert page.total is None and page.bots == []
    assert OPS.bots.prefix_conf_key in page.problem
    assert "aiplayerbot.conf" in page.problem
    assert "blank" in page.problem

    direct = botlist.page(reader(), ENTRY, dbreads.Marker(prefix="", source="conf"))
    say(f"and botlist's own refusal, given an empty Marker: problem={direct.problem!r}")
    assert direct.total is None and "every account" in direct.problem

    live = services().bots.page()
    say(f"the live install at the same moment: total={live.total} problem={live.problem!r}")
    assert live.total, "the live install is not answering either; this stage proves nothing"

    view = view_for(made)
    view.refresh_bots()
    view.grab().save(str(SHOTS / "2-refused.png"))
    say(f"the tab reads: {view.bot_summary.text()!r}")
    assert view.bot_list.count() == 0
    assert OPS.bots.prefix_conf_key in view.bot_summary.text()
    say("CLAUSE 2 PASSED: it named what stopped it, showed no rows, and reported no zero")


# --------------------------------------------------------------- clause (3)


def stage_warn() -> None:
    """CLAUSE (3): a READABLE marker matching nothing warns, neither reporting zero.

    GROUND: the live marker answering a non-zero total at the same moment, and
    a hand count of the substituted marker taken through the same seam.

    "Readable" is the load-bearing word and it is why the marker is written into
    a conf copy and READ BACK through `resolve_marker` rather than constructed
    in Python. A `Marker('NOSUCHBOTPREFIX', 'conf')` built here would exercise
    `botlist.page`'s `if total == 0` branch, which CI already covers; the live
    gate adds something only when the resolver, the clause builder, the server
    and the sentence are all in the loop.

    8.5b settled the wording on TBC -- the tab LEADS with the warning and prints
    no count -- and 8.5c re-measured it on Vanilla. This tree is the third
    reading of the same behaviour and gates it as a regression.
    """
    root = fake_conf("gate85d-nomatch", "NOSUCHBOTPREFIX")
    resolved = dbreads.resolve_marker(ENTRY, root)
    say(f"the marker READ BACK off the conf copy at {root}: {resolved.marker}")
    assert resolved.marker is not None and resolved.marker.source == "conf"
    assert resolved.marker.prefix == "NOSUCHBOTPREFIX", (
        "the resolver did not read the copy -- most likely it fell back to the catalog "
        "default `rndbot`, which on this install is a WORKING marker"
    )

    made = with_marker("NOSUCHBOTPREFIX", "gate85d-nomatch")
    page = made.bots.page()
    say(
        f"a marker matching nothing: total={page.total} by_registry={page.by_registry} "
        f"by_prefix={page.by_prefix} problem={page.problem!r} warning={page.warning!r}"
    )

    hand_zero = by_hand(
        "SELECT COUNT(*) FROM tw_char.characters WHERE account IN "
        "(SELECT id FROM tw_logon.account WHERE UPPER(username) LIKE 'NOSUCHBOTPREFIX%');"
    )
    everyone = by_hand(f"SELECT COUNT(*) FROM {TABLE};")
    say(f"by hand, the same marker: {hand_zero}; characters on the server: {everyone}")

    live = services().bots.page()
    say(f"the live install at the same moment: total={live.total} warning={live.warning!r}")
    assert live.total and live.warning == "", (
        "the live marker is not answering either; this stage would prove nothing"
    )

    assert page.problem == "", "it failed rather than answering"
    assert page.total == 0 == int(hand_zero)
    assert "NOSUCHBOTPREFIX" in page.warning
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
    """The OTHER cause of a zero, so clause (3)'s sentence has exactly one.

    GROUND: the LIVE marker, which is answering a full count -- that is what
    makes the point that the filter is what found nothing.

    Before 8.5c the name filter was folded into the same WHERE the total is
    counted over, so a filter matching nothing drove `total == 0` down the
    marker's branch: the tab said "no character matched the bot marker 'RNDBOT'"
    about a marker that had matched every bot on the server a keystroke earlier.
    Two things were wrong at once -- the sentence was false, and it was the SAME
    sentence `warn` photographs as the whole proof of clause (3), so the gate
    could not have said which cause it caught (`botlist._why_zero`).
    """
    browser = services().bots
    unfiltered = browser.page()
    say(f"the live marker, unfiltered: total={unfiltered.total} warning={unfiltered.warning!r}")
    assert unfiltered.total, "there are no bots here at all; nothing to filter"
    assert unfiltered.warning == ""

    # `%` and `_` are the LIKE wildcards, and this is the escaping (`!`, because
    # NO_BACKSLASH_ESCAPES is real) exercised against a live MariaDB rather than
    # a stub. Unescaped, `%_` matches every name of at least one character.
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
    say(f"the tab reads: {view.bot_summary.text()!r}")
    assert "no character matched" not in view.bot_summary.text(), (
        "the tab blamed the marker for the filter"
    )
    say("PASSED: a filtered zero and a marker zero are two different sentences")


# ------------------------------------------------- the box's inherited reading


def stage_nosplit() -> None:
    """"As 8.5b" minus the type split, read off the real widgets.

    GROUND: the schema list asked of the running server, which this stage does
    not create, plus the catalog reading that both the SQL and the view take.

    Not a clause of the definition of done -- checklist.md:2499 inherits 8.5b's
    four clauses and the split is simply not among them -- but it is behaviour
    8.5b shipped to every registry-less tree, so this, the fourth tree and the
    third one-signal tree, gates it as a REGRESSION rather than as new work.
    The presence assertions sit beside the absence ones deliberately:
    `"registry" not in ""` is true of a label that says nothing at all.
    """
    say(f"has_registry({ENTRY.id}) = {botlist.has_registry(ENTRY)}")
    say(f"the attribution clause on this tree: {botlist._registry_clause(ENTRY, OPS)!r}")
    assert botlist.has_registry(ENTRY) is False
    assert botlist._registry_clause(ENTRY, OPS) == "1 = 0"

    schemas = by_hand("SELECT GROUP_CONCAT(schema_name) FROM information_schema.schemata;")
    say(f"the schemas this install really has: {schemas}")
    assert "playerbots" not in schemas.lower(), "there IS a registry here; re-measure the catalog"
    assert "tw_logon" in schemas and "tw_char" in schemas, (
        "this is not the Tortoise install: its databases are tw_logon/tw_char, "
        "not realmd/characters"
    )

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
    """Paging and filtering, which the list needs to be usable at all.

    GROUND: page one's cursor, which the server chose; the filter string is
    taken from page TWO's rows, so it is data this stage did not pick.
    """
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

    # The hand query is the app's WHOLE where and not the LIKE alone: the LIKE
    # alone also counts any character here that is not a bot, and the gate would
    # then fail by one on a correct app (8.5b, 2026-09-07).
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


# ------------------------------------------- the maps and rules, measured here


def _chr_races() -> tuple[dict[int, int], dict[int, str]]:
    """`ChrRaces.dbc` as this install extracted it: race -> baseLanguage, and names.

    The field indices are THIS fork's, read off its own header on 2026-09-08:
    `src/game/Database/DBCStructure.h:147` puts `baseLanguage` (7 Alliance,
    1 Horde) at index 8 and `:156` puts `m_name_lang`'s enUS slot at 17.
    `Player::TeamForRace` (`src/game/Objects/Player.cpp:7943`) switches on that
    same `baseLanguage`, so the map is a property of the DBC this server loaded
    rather than of the expansion.

    Parsed rather than pasted, because it is not the sibling gates' map: read on
    2026-09-08 this file holds TEN races -- 1 Human, 3 Dwarf, 4 Night Elf,
    7 Gnome and **10 High Elf** Alliance; 2 Orc, 5 Undead, 6 Tauren, 8 Troll and
    **9 Goblin** Horde. The Vanilla install's own DBC put Goblin on the
    ALLIANCE side, which is exactly why this is read per tree.
    """
    path = SERVER_DIR / "data" / "dbc" / "ChrRaces.dbc"
    blob = path.read_bytes()
    magic, count, fields, size, _strings = struct.unpack("<4siiii", blob[:20])
    assert magic == b"WDBC", f"{path} is not a DBC"
    block = 20 + count * size
    teams: dict[int, int] = {}
    names: dict[int, str] = {}
    for i in range(count):
        row = struct.unpack(f"<{fields}i", blob[20 + i * size : 20 + (i + 1) * size])
        teams[row[0]] = row[8]
        start = block + row[17]
        names[row[0]] = blob[start : blob.index(b"\0", start)].decode("utf-8", "replace")
    return teams, names


def races() -> dict[int, str]:
    teams, _ = _chr_races()
    sides = {7: "ALLIANCE", 1: "HORDE"}
    return {race: sides.get(lang, f"lang{lang}") for race, lang in teams.items()}


def stage_races() -> None:
    """The race -> faction map, off this install's own extracted client data.

    GROUND: `data/dbc/ChrRaces.dbc`, written by the extractor at install time
    and not by this gate.
    """
    teams, names = _chr_races()
    said = races()
    for race in sorted(said):
        say(f"race {race} ({names[race]}): {said[race]} (baseLanguage {teams[race]})")
    assert names[1] == "Human" and names[6] == "Tauren", (
        f"the name block does not read as a race list: {names}"
    )
    # The two that make this tree's map its own. If either of these fails, the
    # client's data changed and every faction choice below has to be re-made.
    assert names.get(9) == "Goblin" and said[9] == "HORDE", (
        "Goblin is not race 9 HORDE here; the Vanilla install's DBC called race 9 ALLIANCE, "
        "so re-read the file before choosing a corroborating subject"
    )
    assert names.get(10) == "High Elf", "race 10 is not High Elf; this is not the Turtle DBC"
    say(f"PASSED: the map came off {SERVER_DIR / 'data/dbc/ChrRaces.dbc'}, not off a sibling gate")


def stage_wholaw() -> None:
    """The who-list rules THIS fork and THIS conf really apply.

    GROUND: `etc/mangosd.conf` as the server read it at start-up, and the fork's
    own source. Nothing here is inherited from 8.5b or 8.5c: their trees answer
    this question differently, and getting it wrong makes a correct list look
    like a defect.

    The four that matter, `src/game/Handlers/MiscHandler.cpp`:

      :122  `if (security == SEC_PLAYER)` -- the guard the next two sit inside
      :125  faction filter, and ONLY when the asker is SEC_PLAYER
      :129  a subject above `GM.InWhoList.Level` is dropped
      :142  a subject outside the LEVEL RANGE THE CLIENT SENT is dropped.
            Re-read on the box 2026-09-07 23:16Z: this file said `:140` when it
            was written blind, and :140 is not the line.
      :253  a 30-SECOND COOLDOWN: a second /who inside 30 s returns silently,
            with no answer at all. Re-read on the box: the CHECK at :253 applies
            to every asker unless the player carries
            `CUSTOM_PLAYER_FLAG_BYPASS_WHO_COOLDOWN`; what is SEC_PLAYER-only is
            the STAMP, `:326-327` `if (GetSecurity() == SEC_PLAYER)
            m_lastWhoRequest = time(nullptr);` -- so a GM asker never refreshes
            the timestamp and therefore always passes. The conclusion this file
            was written with is right; the line it cited for it was the check.
            The asker this run found is rank 0, so the cooldown fully applies to
            it and 35 s between names is not optional. That is
            this fork's own, it is not on the two sibling trees, and it looks
            exactly like a miss on a screenshot.
    """
    conf = (SERVER_DIR / "etc" / "mangosd.conf").read_text(encoding="utf-8", errors="replace")
    values: dict[str, str] = {}
    for line in conf.splitlines():
        for key in ("AllowTwoSide.WhoList", "GM.InWhoList.Level"):
            if line.startswith(key):
                values[key] = line.split("=", 1)[1].strip()
                say(f"{key} = {values[key]} (from etc/mangosd.conf, at column 0)")
    assert set(values) == {"AllowTwoSide.WhoList", "GM.InWhoList.Level"}, values
    # Read 1 and 3 on 2026-09-08 with the stack down. `1` means the faction
    # filter is OFF here -- the opposite of the Vanilla install 8.5c gated,
    # where it is 0 -- and it must be re-read rather than carried over.
    say(
        "so on this install a SEC_PLAYER asker sees BOTH factions"
        if values["AllowTwoSide.WhoList"] == "1"
        else "so on this install a SEC_PLAYER asker sees only its own faction"
    )
    say(
        "the 30-second /who cooldown: the CHECK (MiscHandler.cpp:253) applies to every asker "
        "that has not got CUSTOM_PLAYER_FLAG_BYPASS_WHO_COOLDOWN, but the STAMP (:326-327) is "
        "written only for a SEC_PLAYER asker, so a GM one never refreshes it and always passes; "
        "the driver waits 35 s between names either way"
    )
    say("PASSED: the rules were read off this install, not inherited")


def stage_realmrow() -> None:
    """The realm row the client will be sent to, read BEFORE the client is started.

    GROUND: `tw_logon.realmlist`, written at install time and by Yu'lon's
    Networking apply -- never by this gate.

    8.5c found this trap live on the Vanilla install: the row held
    `192.168.10.134`, which the Hyper-V host that runs the client cannot reach,
    while `100.78.24.50` (Tailscale) is reachable from both. A client sent to an
    unreachable world address photographs as "the who-list found nothing".

    This stage only READS. If the address is wrong, the fix is Yu'lon's own
    Networking feature and not a hand UPDATE -- `networking.plan(entry, "lan",
    lan_ip="100.78.24.50")` then `services.network_apply(plan)`, which is what
    8.5c's `network-fix.py` does, and the Tailscale address has to be *said*
    because `network_plan("lan")` detects the LAN one.
    """
    rows = by_hand(f"SELECT id, name, address, port FROM {AUTH}.realmlist;")
    say(f"the realm row(s) this server offers:\n{rows}")
    assert rows.strip(), "there is no realm row at all; the client cannot get past the realm list"
    for line in rows.splitlines():
        address = line.split("\t")[2]
        assert not address.startswith("127."), (
            f"the realm address is {address}: a client on another machine cannot reach it"
        )
        say(f"realm address: {address} -- check it from the CLIENT box before driving the client")
    say("PASSED (read-only): the row exists and is not loopback")


# --------------------------------------------------------------- clause (4)


def stage_who() -> None:
    """CLAUSE (4): choose the who-list subjects from what the app listed, and say why.

    GROUND: the app's own Bots page, and the characters table read through the
    same seam. The clause is carried by the POSITIVE find -- a bot this tab
    listed as online, named in the client's `/who` answer, with the client alive
    in the driver's log at the moment of the photograph. Anything else is
    corroboration: "0 players total" is equally what a client that never entered
    the world, a chat line that did not open, a mistyped name, a bot that logged
    out, or (on this fork) a second `/who` inside 30 seconds produces.

    The primary subject is picked with `race = <the asking character's race>`,
    so it is the asker's own team by construction and needs no faction map at
    all. The map is used only for the corroborating subject.
    """
    asker = by_hand(
        "SELECT c.guid, c.name, c.race, c.level, c.online, c.at_login, a.username, a.rank "
        f"FROM {TABLE} c JOIN {AUTH}.account a ON a.id = c.account "
        f"WHERE UPPER(a.username) NOT LIKE '{marker().prefix.upper()}%';"
    )
    say(f"the characters here that are NOT bots (the asker is one of these):\n{asker}")
    rows = [line.split("\t") for line in asker.splitlines() if line.strip()]
    assert rows, (
        "THERE IS NO NON-BOT CHARACTER ON THIS SERVER. Nothing on disk said whether one "
        "existed (tw_char.characters is MyISAM and the account of each row is inside the "
        "record blocks), so this is the reading that settles it. Clause 4 then has no route "
        "until a character is CREATED with the 1.18.1 client -- see README.md, 'The client "
        "half'. Do not borrow a bot's account: the playerbots module owns those sessions."
    )
    asker_race = int(rows[0][2])
    asker_level = int(rows[0][3])
    _, names = _chr_races()
    team = races()[asker_race]
    say(
        f"asking character: {rows[0][1]}, race {asker_race} ({names[asker_race]}, {team}), "
        f"level {asker_level}, account {rows[0][6]} rank {rows[0][7]}, at_login {rows[0][5]}"
    )
    if rows[0][5] != "0":
        say(
            "at_login is NOT 0: a rename or customise prompt stands between the character "
            "screen and the world (8.5b photographed its way past one on TBC). The driver "
            "takes -RenameTo for exactly this."
        )
    if int(rows[0][7]) > 3:
        say(
            "the asker's account rank is above GM.InWhoList.Level=3: harmless for the "
            "subjects, and it also BYPASSES the 30-second /who cooldown "
            "(MiscHandler.cpp:253-255 only touches m_lastWhoRequest for SEC_PLAYER)"
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

    names_in = ", ".join(f"'{n}'" for n in online)
    same = by_hand(
        f"SELECT name, level, race FROM {TABLE} WHERE name IN ({names_in}) "
        f"AND online = 1 AND race = {asker_race} "
        # CAST because `level` is UNSIGNED and `level - N` on a low-level bot
        # underflows: "BIGINT UNSIGNED value is out of range" (8.5b, on the box).
        f"ORDER BY ABS(CAST(level AS SIGNED) - {asker_level}), name;"
    )
    say(f"online bots of the asker's OWN race, nearest its level first:\n{same}")
    assert same.strip(), "no online bot shares the asker's race on these pages; ask again"

    opposite = sorted(r for r, t in races().items() if t != team and t in ("ALLIANCE", "HORDE"))
    say(f"races on the other side, per this install's DBC: {opposite}")
    across = by_hand(
        f"SELECT name, level, race FROM {TABLE} WHERE name IN ({names_in}) "
        f"AND online = 1 AND race IN ({', '.join(str(r) for r in opposite)}) ORDER BY name LIMIT 1;"
    )
    say(f"an online bot on the other side (corroboration only):\n{across}")
    say(
        "NOTE: AllowTwoSide.WhoList reads 1 on this install (stage `wholaw`), so the "
        "opposite-side subject is EXPECTED TO BE FOUND here -- the reverse of Vanilla. "
        "An empty answer for it is a finding about the config or the client, not a filter "
        "working as designed."
    )

    say(
        "PASS CRITERION: the same-race subject appears in the client's /who answer, with the "
        "client alive in the driver's log at the shot. The client's own wording is READ OFF "
        "the screenshot; 8.5b's '1 player total' is a 2.4.3 string and is not asserted here. "
        "Ask about ONE name at a time with at least 35 s between them, and re-run "
        "`still-online <name>` immediately before each."
    )


def stage_still_online(name: str) -> None:
    """Re-check by hand, immediately before the client types."""
    answer = by_hand(f"SELECT name, level, race, online FROM {TABLE} WHERE name = '{name}';")
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
            "wholaw": stage_wholaw,
            "realmrow": stage_realmrow,
            "who": stage_who,
        }[sys.argv[1]]()
