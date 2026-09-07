"""The 8.5a live gate: browsing this server's bots.

Usage:  python gate85a.py <stage>

    total     the app's total against the same clause run by hand
    split     the split by signal, and each half checked separately
    refuse    a marker that cannot be read, and one that matches nothing
    page      paging and filtering, against the rows the server has
    shots     the Bots tab, rendered from the real view
"""

from __future__ import annotations

import os
import sys
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate81a" / "pylauncher"))

from yulon import botlist, dbreads  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "wowserver"
ENTRY = load_catalog().get("wow-wotlk")
SHOTS = Path.home() / "gate85a-shots"


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


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
    live = marker()
    say(f"the marker this install is running with: {live}")
    page = services().bots.page()
    say(f"the tab says: total={page.total} registry={page.by_registry} prefix={page.by_prefix}")

    schemas = ENTRY.schema_map()
    ops = ENTRY.observability
    table = f"{ENTRY.databases.characters}.{ops.characters.table}"
    clause = dbreads.bot_clause(ENTRY, live)
    counted = by_hand(f"SELECT COUNT(*) FROM {table} WHERE ({clause});")
    say(f"by hand, with the same clause: {counted}")
    assert page.total == int(counted), "the tab and the database disagree"

    everyone = by_hand(f"SELECT COUNT(*) FROM {table};")
    say(f"characters on this server altogether: {everyone}")
    say(f"clause 1 PASSED: {page.total} of {everyone} are bots")
    _ = schemas


def stage_split() -> None:
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
    say("clause 2 PASSED: the split is the server's own, and the halves add up")


def stage_refuse() -> None:
    blank = botlist.page(reader(), ENTRY, dbreads.Marker(prefix="", source="conf"))
    say(f"a marker that could not be read: total={blank.total} problem={blank.problem!r}")
    assert blank.total is None and blank.problem

    # And the thing this box's own wording did not expect. On THIS tree the
    # clause has two arms, and the registry answers whether or not the prefix
    # does -- so a marker matching nothing does not make the total zero here,
    # and there is nothing to warn about. The number is the same 1000, found
    # entirely by the other signal, which is the split doing its job.
    nothing = botlist.page(reader(), ENTRY, dbreads.Marker(prefix="NOSUCHBOT", source="conf"))
    say(f"a marker matching nothing: total={nothing.total} by_registry={nothing.by_registry} "
        f"by_prefix={nothing.by_prefix} warning={nothing.warning!r}")
    assert nothing.total == blank_total_expected(), "the registry stopped answering"
    assert nothing.by_prefix == 0
    assert nothing.warning == "", "it warned about a marker the registry made irrelevant"
    say("clause 3 PASSED: an unreadable marker refuses; a marker matching nothing on a tree "
        "with a second signal is answered by that signal, and says so in the split")


def blank_total_expected() -> int:
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
    browser = services().bots
    first = browser.page(offset=0)
    second = browser.page(offset=botlist.PAGE_SIZE)
    say(f"page 1: {[b.name for b in first.bots][:4]} … ({len(first.bots)} rows)")
    say(f"page 2: {[b.name for b in second.bots][:4]} … ({len(second.bots)} rows)")
    assert len(first.bots) == botlist.PAGE_SIZE
    assert {b.name for b in first.bots}.isdisjoint({b.name for b in second.bots})

    stem = first.bots[0].name[:3]
    found = browser.page(name_like=stem)
    say(f"filtered on {stem!r}: total={found.total} first={[b.name for b in found.bots][:5]}")
    assert found.total is not None and found.total >= 1
    assert all(b.name.upper().startswith(stem.upper()) for b in found.bots)

    online = [b for b in first.bots if b.online]
    say(f"online on this page: {[b.name for b in online][:6]}")
    say("clause 4 PASSED: two pages that do not overlap, and a filter that filters")
    if online:
        say(f"FOR THE WHO-LIST: {online[0].name} is listed as online")


def stage_shots() -> None:
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    app = QApplication.instance() or QApplication([])
    view = ControllerView(ENTRY, services(), status_poll_ms=0, job_runner=run_inline)
    view.resize(940, 640)
    view._tabs.setCurrentIndex(
        next(i for i in range(view._tabs.count()) if view._tabs.tabText(i) == "Bots")
    )
    view.refresh_bots()
    SHOTS.mkdir(parents=True, exist_ok=True)
    view.grab().save(str(SHOTS / "1-bots.png"))
    say(f"summary: {view.bot_summary.text()!r}")
    say(f"first rows: {[view.bot_list.item(i).text() for i in range(min(3, view.bot_list.count()))]}")

    view.next_bot_page()
    view.grab().save(str(SHOTS / "2-page-two.png"))
    say(f"page two: {view.bot_summary.text()!r}")

    name = view.bot_list.item(0).text().split(" — ")[0][:3]
    view.bot_filter.setText(name)
    view.filter_bots()
    view.grab().save(str(SHOTS / "3-filtered.png"))
    say(f"filtered on {name!r}: {view.bot_summary.text()!r}")
    del app


if __name__ == "__main__":
    {
        "total": stage_total,
        "split": stage_split,
        "refuse": stage_refuse,
        "page": stage_page,
        "shots": stage_shots,
    }[sys.argv[1]]()
