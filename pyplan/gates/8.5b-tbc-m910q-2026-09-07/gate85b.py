"""The 8.5b live gate: browsing this server's bots on a tree with ONE signal.

Usage:  ~/gate81b-venv/bin/python gate85b.py <stage>

    total     the app's total against the same clause run by hand, twice over
    refuse    a marker that cannot be read, on the live SQL with only the conf swapped
    warn      a readable marker matching nothing — the clause 8.5a could not reach
    nosplit   this box's own clause: no registry named where there is no registry
    page      paging and filtering, against the rows the server has
    who       chooses the subject for the who-list half, by what the app listed

Read-only throughout. No server is started, stopped or reconfigured, and the
live `~/tbc-7.4c/etc/aiplayerbot.conf` is never edited: the two substituted
markers live in copies under /tmp, and only the bot-browsing seam is pointed at
them. The SQL reader stays the live one, which is the whole point — a wrong
password would make `refuse` pass for the wrong reason (adversarial review,
2026-09-07).
"""

from __future__ import annotations

import os
import shutil
import sys
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate85b" / "pylauncher"))

from yulon import botlist, dbreads  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "tbc-7.4c"
ENTRY = load_catalog().get("wow-tbc")
SHOTS = Path.home() / "gate85b-shots"

OPS = ENTRY.observability
TABLE = f"{ENTRY.databases.characters}.{OPS.characters.table}"


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
    return reader().query("characters", statement).strip()


def marker() -> dbreads.Marker:
    answer = dbreads.resolve_marker(ENTRY, SERVER_DIR)
    assert answer.marker is not None, answer.problem
    return answer.marker


def fake_conf(name: str, value: str) -> Path:
    """A copy of the live install directory with one setting rewritten.

    The stanza is gate81b's, and it exists so that the live conf is never
    touched. `value` may be empty, which is the state clause (2) is about.
    """
    root = Path("/tmp") / name
    shutil.rmtree(root, ignore_errors=True)
    (root / "etc").mkdir(parents=True)
    key = OPS.bots.prefix_conf_key
    live = (SERVER_DIR / OPS.bots.prefix_conf_file).read_text(encoding="utf-8", errors="replace")
    out = []
    for line in live.splitlines():
        out.append(f"{key} = {value}" if line.startswith(key) else line)
    (root / OPS.bots.prefix_conf_file).write_text("\n".join(out) + "\n", encoding="utf-8")
    # The password lives beside the conf in the real directory and the fake one
    # must not be asked for it -- see `with_marker`, which never reads from here.
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
    """The total equals the same clause run by hand.

    Three readings and not two: the app's, the app's own clause run by hand,
    and a statement typed out here with no reference to the code under test. If
    the app ever answered with the character count instead of the bot count,
    900 and 901 are what separate them. All three inside one minute, with the
    clock beside each, because the bot pool is written by the module at runtime
    and a mismatch caused by the pool moving must not be read as a defect.
    """
    live = marker()
    say(f"the marker this install is running with: prefix={live.prefix!r} source={live.source!r}")
    assert live.prefix == "RNDBOT" and live.source == "conf"

    page = services().bots.page()
    say(f"the tab says: total={page.total} by_registry={page.by_registry} "
        f"by_prefix={page.by_prefix} rows={len(page.bots)}")

    clause = dbreads.bot_clause(ENTRY, live)
    say(f"the app's identity clause: {clause}")
    assert " OR " not in clause, "this tree has one arm; a second one is a registry it has not got"
    from_clause = by_hand(f"SELECT COUNT(*) FROM {TABLE} WHERE ({clause});")
    say(f"by hand, with the app's own clause: {from_clause}")

    # Typed out here, on purpose, with nothing taken from the module under test.
    typed = by_hand(
        "SELECT COUNT(*) FROM characters.characters WHERE account IN "
        "(SELECT id FROM realmd.account WHERE UPPER(username) LIKE 'RNDBOT%');"
    )
    say(f"by hand, written independently of the app: {typed}")

    everyone = by_hand(f"SELECT COUNT(*) FROM {TABLE};")
    say(f"characters on this server altogether: {everyone}")

    assert page.total == int(from_clause) == int(typed), "the tab and the database disagree"
    assert int(everyone) > page.total, "every character is a bot; the count proves nothing"
    say(f"CLAUSE 1 PASSED: {page.total} bots of {everyone} characters, three ways")


# --------------------------------------------------------------- clause (2)


def stage_refuse() -> None:
    """An unreadable marker refuses to answer rather than reporting zero.

    The sentence is pinned, not its topic. `total is None` and `bots == []` are
    true of every failure this seam has, including a wrong password, so the
    assertion names the three things only the blank-prefix refusal says.
    """
    made = with_marker("", "gate85b-blank")
    page = made.bots.page()
    say(f"a blank marker: total={page.total} rows={len(page.bots)} problem={page.problem!r}")
    assert page.total is None and page.bots == []
    assert OPS.bots.prefix_conf_key in page.problem
    assert "aiplayerbot.conf" in page.problem
    assert "blank" in page.problem
    # And what the live install answers at the same moment, so the refusal is
    # known to come from the substituted conf and not from a sick server.
    say(f"the live install at the same moment: total={services().bots.page().total}")

    view = view_for(made)
    view.refresh_bots()
    view.grab().save(str(SHOTS / "2-refused.png"))
    say(f"the tab reads: {view.bot_summary.text()!r}")
    assert view.bot_list.count() == 0
    assert OPS.bots.prefix_conf_key in view.bot_summary.text()
    say("CLAUSE 2 PASSED: it named what stopped it and showed no rows")


# --------------------------------------------------------------- clause (3)


def stage_warn() -> None:
    """A readable marker matching nothing warns, on the tree where it is reachable.

    8.5a asked the same question of WotLK and got 1000 back, because the
    registry arm answers whether or not the prefix does; its entry defers this
    clause to the trees that have only the prefix. This is one of them.
    """
    made = with_marker("NOSUCHBOTPREFIX", "gate85b-nomatch")
    page = made.bots.page()
    say(f"a marker matching nothing: total={page.total} by_registry={page.by_registry} "
        f"by_prefix={page.by_prefix} problem={page.problem!r} warning={page.warning!r}")

    hand_zero = by_hand(
        "SELECT COUNT(*) FROM characters.characters WHERE account IN "
        "(SELECT id FROM realmd.account WHERE UPPER(username) LIKE 'NOSUCHBOTPREFIX%');"
    )
    everyone = by_hand(f"SELECT COUNT(*) FROM {TABLE};")
    say(f"by hand, the same marker: {hand_zero}; characters on the server: {everyone}")

    assert page.problem == "", "it failed rather than answering"
    assert page.total == 0 == int(hand_zero)
    assert "NOSUCHBOTPREFIX" in page.warning
    assert everyone in page.warning, "the warning must carry the number that makes zero a lie"

    view = view_for(made)
    view.refresh_bots()
    view.grab().save(str(SHOTS / "3-warned.png"))
    said = view.bot_summary.text()
    say(f"the tab reads: {said!r}")
    assert "no character matched" in said
    assert everyone in said
    assert "registry" not in said, "it named a table this install has not got"
    # "warns, NEITHER reporting zero". The first run of this stage read
    # "0 bots. Page 1, 0 shown. no character matched …", which reports the zero
    # first and corrects it afterwards; the tab now leads with the warning.
    assert said.startswith("no character matched"), said
    assert "0 bots" not in said
    say("CLAUSE 3 PASSED: the warning leads, the zero is not reported, and no registry is named")


# ----------------------------------------------------- this box's own clause


def stage_nosplit() -> None:
    """As 8.5a minus the type split, read off the real widgets.

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
    say("BOX CLAUSE PASSED: the count, the page and the rows, with no signal named")


# ------------------------------------------------------------------- paging


def stage_page() -> None:
    """Paging and filtering, which the list needs to be usable at all."""
    browser = services().bots
    first = browser.page()
    say(f"page 1: {[b.name for b in first.bots][:4]} … ({len(first.bots)} rows), "
        f"next_after={first.next_after}")
    assert len(first.bots) == botlist.PAGE_SIZE and first.next_after is not None

    second = browser.page(after=first.next_after)
    say(f"page 2: {[b.name for b in second.bots][:4]} … ({len(second.bots)} rows)")
    assert len(second.bots) == botlist.PAGE_SIZE
    assert {b.name for b in first.bots}.isdisjoint({b.name for b in second.bots})
    assert second.next_after > first.next_after

    # The filter string comes from data this gate did not choose: page 2's rows,
    # which came from page 1's cursor.
    stem = second.bots[0].name[:3]
    found = browser.page(name_like=stem)
    say(f"filtered on {stem!r}: total={found.total} first={[b.name for b in found.bots][:5]}")
    assert all(b.name.upper().startswith(stem.upper()) for b in found.bots)

    # The hand query is the app's whole WHERE and not the LIKE alone: the LIKE
    # alone would also count `Ddsasd`, the one character here that is not a bot,
    # whenever the three letters matched it -- and the gate would fail by one on
    # a correct app (adversarial review, 2026-09-07).
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
    say("PAGING PASSED: two pages that do not overlap, and a filter checked by hand")


# --------------------------------------------------------------- clause (4)


def stage_who() -> None:
    """Choose the who-list subject from what the app listed, and say why.

    Two subjects, nearest-level first. 8.5a measured on a 3.3.5a AzerothCore
    client that /who is faction-filtered; whether a 2.4.3 CMaNGOS client also
    filters by level is unmeasured on this tree, and the asking character is
    level 25 while the online bots run to 70. So the primary subject is the
    online Alliance bot NEAREST level 25 and the second is a high one, and the
    two answers together say which filter the client applies.
    """
    browser = services().bots
    online: list[tuple[str, int]] = []
    after = None
    for _ in range(6):
        page = browser.page(after=after)
        online += [(b.name, b.level) for b in page.bots if b.online]
        after = page.next_after
        if after is None:
            break
    say(f"the app listed {len(online)} online bots in the pages it was asked for")

    names = ", ".join(f"'{n}'" for n, _ in online)
    alliance = by_hand(
        f"SELECT name, level, race FROM {TABLE} WHERE name IN ({names}) "
        # CAST, because this tree's `level` is UNSIGNED and `level - 25` on a
        # level-1 bot underflows: "BIGINT UNSIGNED value is out of range"
        # (measured here, 2026-09-07, on the gate's first run).
        "AND online = 1 AND race IN (1, 3, 4, 7, 11) "
        "ORDER BY ABS(CAST(level AS SIGNED) - 25), name;"
    )
    say("online ALLIANCE bots among them, nearest level 25 first (race is not on the row, "
        f"so it is asked for here):\n{alliance}")
    rows = [line.split("\t") for line in alliance.splitlines() if line.strip()]
    assert rows, "no online Alliance bot on these pages; ask again, the pool rotates"

    asker = by_hand(
        "SELECT c.guid, c.name, c.race, c.level, c.online, c.at_login, a.username "
        f"FROM {TABLE} c JOIN realmd.account a ON a.id = c.account "
        "WHERE UPPER(a.username) NOT LIKE 'RNDBOT%';"
    )
    say(f"the asking character (the one row here that is not a bot):\n{asker}")

    say(f"SUBJECT 1 (nearest the asker's level): {rows[0][0]} level {rows[0][1]} race {rows[0][2]}")
    say(f"SUBJECT 2 (the highest of them):       {rows[-1][0]} level {rows[-1][1]} race "
        f"{rows[-1][2]}")
    say("PASS CRITERION: the chosen name appears in the client's /who answer. The client's own "
        "wording is read off the screenshot, not asserted from 8.5a's 3.3.5a string.")


def stage_still_online(name: str) -> None:
    """Re-check by hand, immediately before the client types."""
    answer = by_hand(
        f"SELECT name, level, online FROM {TABLE} WHERE name = '{name}';"
    )
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
            "nosplit": stage_nosplit,
            "page": stage_page,
            "who": stage_who,
        }[sys.argv[1]]()
