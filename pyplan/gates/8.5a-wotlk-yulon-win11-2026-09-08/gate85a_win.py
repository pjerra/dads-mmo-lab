"""8.5a live gate, the WINDOWS half: browsing this server's bots, on WoW WotLK.

This is `pyplan/gates/8.5a-wotlk-yulon-ubuntu-2026-09-07/gate85a.py` with its
constants changed -- the source root, the server dir, the shots directory, the
activity log -- and ONE stage rewritten, because the committed Linux script is
stale: `stage_page` there calls `browser.page(offset=...)`, and the paging
argument became a `(name, guid)` cursor after the adversarial review of that
same box. A gate calling the old signature does not fail loudly on this tree, it
fails with a TypeError before it reads anything; the stage below is written
against `after=` and `next_after`, and it asserts the property the cursor exists
for rather than merely that two pages differ.

Usage:  python gate85a_win.py <stage>

    total     the app's total against the same clause run by hand
    split     the split by signal, and each half checked separately
    refuse    a marker that cannot be read, and one that matches nothing
    page      paging by cursor, and a filter that filters
    shots     the Bots tab, rendered from the real view
"""

from __future__ import annotations

import os
import platform as pyplatform
import sys
from datetime import UTC, datetime
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ.setdefault("QT_QPA_FONTDIR", r"C:\Windows\Fonts")

HERE = Path(__file__).resolve().parent
sys.path.insert(0, r"C:\gate\src84a\pylauncher")

SERVER_DIR = Path(r"D:\wow-server")
SHOTS = HERE / "shots"
ACTIVITY = Path(r"C:\gate\gate85a-activity.log")

from yulon import botlist, dbreads  # noqa: E402
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
    except OSError as exc:  # pragma: no cover
        say(f"(could not write the activity log: {exc})")


def services() -> ControllerServices:
    return ControllerServices.for_entry(ENTRY, SERVER_DIR)


def reader():
    from yulon.ui import controller_view as view_module  # noqa: PLC0415

    password = view_module._db_password(ENTRY, SERVER_DIR)
    return view_module._sql_for(ENTRY, password, wsl_distro=None)


def by_hand(statement: str) -> str:
    return reader().query("characters", statement).strip()


def marker() -> dbreads.Marker:
    answer = dbreads.resolve_marker(ENTRY, SERVER_DIR)
    assert answer.marker is not None, answer.problem
    return answer.marker


def stage_total() -> None:
    announce("8.5a Windows: the total, against the same clause run by hand")
    live = marker()
    say(f"the marker this install is running with: {live}")

    ops = ENTRY.observability
    table = f"{ENTRY.databases.characters}.{ops.characters.table}"
    everyone = by_hand(f"SELECT COUNT(*) FROM {table};")
    say(f"===== GROUND: this server holds {everyone} characters altogether =====")
    assert int(everyone) > 0, "an empty server would pass every clause below by saying nothing"

    page = services().bots.page()
    say(f"the tab says: total={page.total} registry={page.by_registry} prefix={page.by_prefix}")

    clause = dbreads.bot_clause(ENTRY, live)
    counted = by_hand(f"SELECT COUNT(*) FROM {table} WHERE ({clause});")
    say(f"by hand, with the same clause: {counted}")
    assert page.total == int(counted), "the tab and the database disagree"
    assert page.total > 0, "a zero total here would be the broken read, not the answer"
    say(f"clause 1 PASSED: {page.total} of {everyone} are bots")


def stage_split() -> None:
    announce("8.5a Windows: the split by signal, each half checked separately")
    live = marker()
    page = services().bots.page()
    ops = ENTRY.observability
    schemas = ENTRY.schema_map()
    table = f"{ENTRY.databases.characters}.{ops.characters.table}"
    registry = ops.bots.registry
    account = ops.characters.account
    types = ", ".join(str(t) for t in registry.types)
    reg_clause = (
        f"{account} IN (SELECT {registry.account_column} FROM "
        f"{schemas[registry.database]}.{registry.table} WHERE {registry.type_column} IN ({types}))"
    )
    clause = dbreads.bot_clause(ENTRY, live)
    hand_reg = by_hand(f"SELECT COUNT(*) FROM {table} WHERE ({clause}) AND ({reg_clause});")
    hand_pre = by_hand(f"SELECT COUNT(*) FROM {table} WHERE ({clause}) AND NOT ({reg_clause});")
    say(f"the tab: registry={page.by_registry} prefix={page.by_prefix}")
    say(f"by hand: registry={hand_reg} prefix={hand_pre}")
    assert page.by_registry == int(hand_reg) and page.by_prefix == int(hand_pre)
    assert page.by_registry + page.by_prefix == page.total
    say(f"and this tree DOES have a registry to split on: has_registry="
        f"{botlist.has_registry(ENTRY)}")
    assert botlist.has_registry(ENTRY)
    say("clause 2 PASSED: the split is the server's own, and the halves add up")


def stage_refuse() -> None:
    announce("8.5a Windows: an unreadable marker, and one that matches nothing")
    blank = botlist.page(reader(), ENTRY, dbreads.Marker(prefix="", source="conf"))
    say(f"a marker that could not be read: total={blank.total} problem={blank.problem!r}")
    assert blank.total is None and blank.problem

    # The Linux box's own wording was refuted by this tree and the refutation
    # holds here: the clause has TWO arms, and the registry answers whether or
    # not the prefix does -- so a marker matching nothing does not make the
    # total zero, and there is nothing to warn about. The warn path is a
    # one-signal tree's, gated on 8.5b and 8.5c.
    expected = registry_alone()
    nothing = botlist.page(reader(), ENTRY, dbreads.Marker(prefix="NOSUCHBOT", source="conf"))
    say(f"a marker matching nothing: total={nothing.total} by_registry={nothing.by_registry} "
        f"by_prefix={nothing.by_prefix} warning={nothing.warning!r}")
    say(f"what the registry alone identifies, asked of the server: {expected}")
    assert nothing.total == expected, "the registry stopped answering"
    assert nothing.by_prefix == 0
    assert nothing.warning == "", "it warned about a marker the registry made irrelevant"
    say("clause 3 PASSED: an unreadable marker refuses; a marker matching nothing on a tree "
        "with a second signal is answered by that signal, and says so in the split")


def registry_alone() -> int:
    """What the registry alone identifies, asked of the server."""
    ops = ENTRY.observability
    schemas = ENTRY.schema_map()
    registry = ops.bots.registry
    table = f"{ENTRY.databases.characters}.{ops.characters.table}"
    types = ", ".join(str(t) for t in registry.types)
    return int(
        by_hand(
            f"SELECT COUNT(*) FROM {table} WHERE {ops.characters.account} IN "
            f"(SELECT {registry.account_column} FROM {schemas[registry.database]}."
            f"{registry.table} WHERE {registry.type_column} IN ({types}));"
        )
    )


def stage_page() -> None:
    """Paging by cursor, which is what this argument became.

    The stale Linux script asked for `offset=`. Two things are asserted that an
    offset could not give: the second page begins exactly where the first one's
    cursor says it does, and the cursor is a `(name, guid)` pair -- a total
    order, where `name` alone is not, so two bots sharing a name cannot swap
    places between two reads and put one on both pages.
    """
    announce("8.5a Windows: paging by the (name, guid) cursor, and a filter that filters")
    browser = services().bots
    first = browser.page()
    say(f"===== GROUND: page 1 holds {len(first.bots)} rows, total={first.total}, "
        f"next_after={first.next_after!r} =====")
    assert first.total is not None and first.total > botlist.PAGE_SIZE, (
        "there is only one page of bots here, so paging would prove nothing"
    )
    assert len(first.bots) == botlist.PAGE_SIZE
    assert first.next_after is not None, "the first page offered no cursor to the second"
    assert isinstance(first.next_after, tuple) and len(first.next_after) == 2
    # `Bot` carries no guid, so the second half of the cursor is checked against
    # the server rather than against the object: the key must be the last row of
    # the page it came from, name AND guid.
    last_guid = by_hand(
        f"SELECT guid FROM {ENTRY.databases.characters}."
        f"{ENTRY.observability.characters.table} WHERE name = '{first.bots[-1].name}';"
    )
    say(f"the last row of page 1 is {first.bots[-1].name!r}, guid {last_guid} by the server")
    assert first.next_after == (first.bots[-1].name, int(last_guid)), (
        f"the cursor {first.next_after!r} is not the last row of the page it came from"
    )

    second = browser.page(after=first.next_after)
    say(f"page 1: {[b.name for b in first.bots][:4]} … last={first.bots[-1].name!r}")
    say(f"page 2: {[b.name for b in second.bots][:4]} … ({len(second.bots)} rows)")
    assert {b.name for b in first.bots}.isdisjoint({b.name for b in second.bots})
    assert second.bots[0].name >= first.next_after[0], (
        "page two does not begin after page one's cursor"
    )

    # And the property an OFFSET cannot have: the same cursor read twice gives
    # the same page, whatever the count did in between.
    again = browser.page(after=first.next_after)
    say(f"the same cursor read again: first row {again.bots[0].name!r} "
        f"(page 2 began {second.bots[0].name!r})")
    assert [b.name for b in again.bots] == [b.name for b in second.bots]

    stem = first.bots[0].name[:3]
    found = browser.page(name_like=stem)
    say(f"filtered on {stem!r}: total={found.total} first={[b.name for b in found.bots][:5]}")
    assert found.total is not None and found.total >= 1
    assert found.total < first.total, "the filter matched everything, so it filtered nothing"
    assert all(b.name.upper().startswith(stem.upper()) for b in found.bots)

    # 8.5b's defect, re-proved on the tree 8.5a is ticked on: a filter matching
    # nothing must not blame the marker that had just matched every bot.
    empty = browser.page(name_like="Zzzznosuchbotname")
    say(f"filtered on a name nobody has: total={empty.total} warning={empty.warning!r}")
    assert empty.total == 0
    assert "marker" not in empty.warning.lower(), (
        "a filtered zero blamed the bot marker, which is 8.5b's defect"
    )
    assert "Zzzznosuchbotname" in empty.warning, "the sentence does not name the filter"

    online = [b for b in first.bots if b.online]
    say(f"online on this page: {[b.name for b in online][:6]}")
    say("clause 4 PASSED: pages anchored to a cursor, a filter that filters, and a "
        "filtered zero that does not blame the marker")
    if online:
        say(f"FOR THE WHO-LIST: {online[0].name} is listed as online")
        for bot in online[:8]:
            race = by_hand(
                f"SELECT race, level FROM {ENTRY.databases.characters}."
                f"{ENTRY.observability.characters.table} WHERE name = '{bot.name}';"
            )
            say(f"    {bot.name}: race/level {race}")


def stage_shots() -> None:
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    announce("8.5a Windows: the Bots tab, rendered from the real view")
    app = QApplication.instance() or QApplication([])
    view = ControllerView(ENTRY, services(), status_poll_ms=0, job_runner=run_inline)
    view.resize(960, 660)
    view._tabs.setCurrentIndex(
        next(i for i in range(view._tabs.count()) if view._tabs.tabText(i) == "Bots")
    )
    view.refresh_bots()
    SHOTS.mkdir(parents=True, exist_ok=True)
    view.grab().save(str(SHOTS / "1-bots.png"))
    say(f"summary: {view.bot_summary.text()!r}")
    say(f"first rows: {[view.bot_list.item(i).text() for i in range(min(3, view.bot_list.count()))]}")
    say(f"Previous enabled={view.previous_bots_button.isEnabled()} "
        f"Next enabled={view.next_bots_button.isEnabled()}")
    first_rows = [view.bot_list.item(i).text() for i in range(view.bot_list.count())]

    view.next_bot_page()
    view.grab().save(str(SHOTS / "2-page-two.png"))
    say(f"page two: {view.bot_summary.text()!r}")
    say(f"Previous enabled={view.previous_bots_button.isEnabled()}")
    second_rows = [view.bot_list.item(i).text() for i in range(view.bot_list.count())]
    assert set(first_rows).isdisjoint(second_rows)

    view.previous_bot_page()
    back_rows = [view.bot_list.item(i).text() for i in range(view.bot_list.count())]
    view.grab().save(str(SHOTS / "3-back-to-page-one.png"))
    say(f"back on page one: {view.bot_summary.text()!r}")
    assert back_rows == first_rows, "Previous did not return the page it came from"

    name = first_rows[0].split(" — ")[0][:3]
    view.bot_filter.setText(name)
    view.filter_bots()
    view.grab().save(str(SHOTS / "4-filtered.png"))
    say(f"filtered on {name!r}: {view.bot_summary.text()!r}")

    view.bot_filter.setText("Zzzznosuchbotname")
    view.filter_bots()
    view.grab().save(str(SHOTS / "5-filtered-to-nothing.png"))
    say(f"filtered to nothing: {view.bot_summary.text()!r}")
    del app


if __name__ == "__main__":
    say(f"### 8.5a WINDOWS gate driver, stage {sys.argv[1]}, {datetime.now(UTC).isoformat()} ###")
    say(f"### box: {pyplatform.node()}, {pyplatform.platform()}, tree: C:\\gate\\src84a ###")
    {
        "total": stage_total,
        "split": stage_split,
        "refuse": stage_refuse,
        "page": stage_page,
        "shots": stage_shots,
    }[sys.argv[1]]()
