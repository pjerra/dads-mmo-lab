"""The 8.3c live gate: the account list and the two changes, on WoW Vanilla.

Usage:  python gate83c.py <stage>

    list      the tab's list against the same question asked by hand
    password  a real password change, read back from the account's own row
    level     a real GM level change, in the row AND in the server's own words
    own       the app's own account: absent from the list, refused by both actions
    shots     the Accounts tab, rendered from the real view

Every reading is taken from the machine, and this tree has NO account
query at all: `help account` lists characters, create, delete, onlinelist,
lock, set and password. The 8.3a script asked for `account info` because
AzerothCore has one, and on this core that closes the connection.

8.3b's own script with the constants changed, and TWO readings that are this
tree's own rather than inherited:

    the GM level is a COLUMN on the account row (`account.gmlevel`), so the
    by-hand query joins nothing and `account_access` is asserted absent

    `account set gmlevel` takes NO realm argument here (`Level3.cpp:1041-1090`, measured again on this tree), so
    the command the app sends is checked as well as its effect
"""

from __future__ import annotations

import os
import sys
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate83b" / "pylauncher"))

from yulon import channel_setup, commands, soap, useraccounts  # noqa: E402
from yulon.catalog import composegen  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "vanilla-75b"
ENTRY = load_catalog().get("wow-vanilla")
SHOTS = Path.home() / "gate83c-shots"
INSTALL_ID = composegen.install_id(SERVER_DIR)
APP_ACCOUNT = channel_setup.account_name(INSTALL_ID)

TARGET = "GATE83C"
OLD_PASSWORD = "old-p@ss12"
NEW_PASSWORD = "n3w-p@ss34"


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def services() -> ControllerServices:
    return ControllerServices.for_entry(ENTRY, SERVER_DIR)


def sql(statement: str) -> str:
    from yulon.ui import controller_view as view_module  # noqa: PLC0415

    password = view_module._db_password(ENTRY, SERVER_DIR)
    reader = view_module._sql_for(ENTRY, password, wsl_distro=None)
    return reader.query("auth", statement).strip()


def ask_the_server(command: str) -> str:
    saved = channel_setup.load_credential(ENTRY.id, INSTALL_ID)
    assert saved is not None, "there is no command channel credential on this box"
    reply = soap.execute(saved, command)
    assert reply.outcome == "answered", f"{command!r} came back {reply.outcome}: {reply.text}"
    return reply.text


# -- the list ---------------------------------------------------------------


def stage_list() -> None:
    listing = services().accounts.listing()
    assert listing.problem == "", listing.problem
    shown = [(a.id, a.username, a.gm_level) for a in listing.accounts]
    say(f"the tab lists {len(shown)} accounts: {shown}")

    # The same question, asked by hand, from the other side: every account,
    # minus the ones whose name carries the live bot marker, minus this app's
    # own. THIS tree has no playerbots registry table -- its marker is the
    # account prefix and nothing else (8.1b) -- so there is one arm here where
    # WotLK's query has two, and no GROUP BY because there is nothing joined.
    schemas = ENTRY.schema_map()
    auth = schemas["auth"]
    assert "playerbots" not in schemas, f"this tree grew a registry schema: {schemas}"
    by_hand = sql(
        "SELECT a.id, a.username, COALESCE(a.gmlevel, 0) "
        f"FROM {auth}.account a "
        "WHERE UPPER(a.username) NOT LIKE 'RNDBOT%' "
        f"AND UPPER(a.username) <> '{APP_ACCOUNT}' "
        "ORDER BY a.username;"
    )
    rows = [
        (int(f[0]), f[1], int(f[2]))
        for f in (line.split("\t") for line in by_hand.splitlines() if line.strip())
    ]
    say(f"by hand: {rows}")
    assert shown == rows, "the list and the database disagree"

    total = sql(f"SELECT COUNT(*) FROM {ENTRY.schema_map()['auth']}.account;")
    say(f"accounts in the table altogether: {total}")
    say(f"clause 1 PASSED: {len(shown)} shown out of {total}")


# -- a password change ------------------------------------------------------


def stage_password() -> None:
    made = services().create_account(TARGET, OLD_PASSWORD, 0)
    say(f"target account: {TARGET} ({'created' if made.created else 'already existed'})")
    # THIS core's own credential columns: `v` and `s`, not AzerothCore's
    # `salt`/`verifier`. Reading the wrong pair here would not fail loudly on a
    # row that has both -- it would simply always report "unchanged".
    before = sql(
        f"SELECT HEX(v), HEX(s) FROM {ENTRY.schema_map()['auth']}.account "
        f"WHERE username = '{TARGET}';"
    )
    say(f"v/s before: {before[:32]}…")

    outcome = services().accounts.set_password(TARGET, NEW_PASSWORD)
    say(f"set_password said: done={outcome.done} {outcome.text.strip() or outcome.problem}")
    assert outcome.done, outcome.problem

    after = sql(
        f"SELECT HEX(v), HEX(s) FROM {ENTRY.schema_map()['auth']}.account "
        f"WHERE username = '{TARGET}';"
    )
    say(f"v/s after:  {after[:32]}…")
    assert after != before, "the row did not change"
    # THIS tree has no `account info`; asking for it closes the
    # connection. What is asked instead is whether the channel is still
    # answering, because the command that just worked reported a failure.
    alive = ask_the_server("server info").splitlines()[0].strip()
    say(f"the channel a moment later: {alive!r}")
    say("clause 2 (server half) PASSED: the server changed its own row")
    say(f"THE CLIENT HALF: log in as {TARGET} with {NEW_PASSWORD!r}; {OLD_PASSWORD!r} must fail")


# -- a GM level change ------------------------------------------------------


def stage_command() -> None:
    """The command text this tree gets, which is not the one WotLK gets.

    `Level3.cpp:1041-1090`, measured again on this tree takes `<account> <level>` and no realm: this core keeps
    the level on the account row, so there is no table with realms in it to
    name. Asserted here because the wrong shape would most likely be IGNORED
    rather than refused -- a level that did or did not change, found out later.
    """
    from yulon import commands  # noqa: PLC0415

    level = ENTRY.accounts.level
    say(f"this tree's level block: {level}")
    assert level is not None and level.table is None, level
    mine = commands.account_set_gm_level(TARGET, 2, realms=False)
    theirs = commands.account_set_gm_level(TARGET, 2, realms=True)
    say(f"what this tree is sent : {mine!r}")
    say(f"what WotLK is sent     : {theirs!r}")
    assert mine == f"account set gmlevel {TARGET} 2"
    assert theirs.endswith(" -1")
    say("PASSED: no realm argument on a core with no realms in its level store")


def stage_level() -> None:
    outcome = services().accounts.set_gm_level(TARGET, 2)
    say(f"set_gm_level said: done={outcome.done} {outcome.text.strip() or outcome.problem}")
    assert outcome.done, outcome.problem

    row = sql(
        f"SELECT a.id, a.username, a.gmlevel FROM {ENTRY.schema_map()['auth']}.account a "
        f"WHERE a.username = '{TARGET}';"
    )
    say(f"the account row: {row!r}")
    assert row, "there is no account row"
    assert row.split("\t")[2] == "2", "the row does not read 2"
    absent = sql("SHOW TABLES LIKE 'account_access';")
    say(f"account_access on this tree: {absent!r} (there is none, and none is written)")
    assert absent == "", "this tree grew an access table and nothing here would notice"

    # The server's own words about the level are the ones it said
    # WHILE changing it: this core has no account query to ask after.
    assert "security level" in outcome.text.lower(), outcome.text
    assert "2" in outcome.text, outcome.text

    listing = services().accounts.listing()
    shown = [a for a in listing.accounts if a.username == TARGET]
    say(f"the tab now shows: {shown}")
    assert shown and shown[0].gm_level == 2
    say("clause 3 PASSED: the row, the server's own words while changing it, and the tab all say 2")


# -- the app's own account --------------------------------------------------


def stage_own() -> None:
    admin = services().accounts
    listing = admin.listing()
    names = [a.username for a in listing.accounts]
    say(f"the list holds: {names}")
    assert APP_ACCOUNT not in names, "the app's own account is in the list"

    for what, outcome in (
        ("set_password", admin.set_password(APP_ACCOUNT, "what3ver!")),
        ("set_gm_level", admin.set_gm_level(APP_ACCOUNT.lower(), 0)),
    ):
        say(f"{what} on {APP_ACCOUNT}: done={outcome.done} — {outcome.problem}")
        assert outcome.done is False

    # And it is still an administrator, which is the thing those refusals
    # protect: a level below 3 would end the channel just as surely as a
    # changed password.
    still = sql(
        f"SELECT gmlevel FROM {ENTRY.schema_map()['auth']}.account "
        f"WHERE username = '{APP_ACCOUNT}';"
    )
    say(f"the app's own account is still level {still!r}")
    assert int(still) >= 3, "the channel account is no longer an administrator"
    say("clause 4 PASSED: absent from the list, refused by both actions, still an administrator")


# -- the tab ----------------------------------------------------------------


def stage_shots() -> None:
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    app = QApplication.instance() or QApplication([])
    view = ControllerView(ENTRY, services(), status_poll_ms=0, job_runner=run_inline)
    view.resize(940, 620)
    view._tabs.setCurrentIndex(
        next(i for i in range(view._tabs.count()) if view._tabs.tabText(i) == "Accounts")
    )
    view.refresh_accounts()
    SHOTS.mkdir(parents=True, exist_ok=True)
    view.grab().save(str(SHOTS / "1-list.png"))
    say(f"list: {[view.account_list.item(i).text() for i in range(view.account_list.count())]}")

    row = next(
        i
        for i in range(view.account_list.count())
        if TARGET in view.account_list.item(i).text()
    )
    view.account_list.setCurrentRow(row)
    view.grab().save(str(SHOTS / "2-chosen.png"))
    say(f"chosen: {view.account_list.currentItem().text()!r}")
    say(f"set password enabled: {view.set_password_button.isEnabled()}")

    view.selected_gm.setValue(1)
    view.set_selected_gm_level()
    view.grab().save(str(SHOTS / "3-level-set.png"))
    say(f"after the change the tab says: {view.account_report.text()!r}")
    say(f"list now: {[view.account_list.item(i).text() for i in range(view.account_list.count())]}")

    # And the refusal, on the account the user must never be able to choose:
    # it is not in the list, so this asks the seam directly and shows the
    # sentence the tab would print.
    refused = services().accounts.set_gm_level(APP_ACCOUNT, 0)
    view.account_report.setText(refused.problem)
    view.grab().save(str(SHOTS / "4-refused.png"))
    say(f"the refusal reads: {refused.problem!r}")
    del app


if __name__ == "__main__":
    {
        "list": stage_list,
        "password": stage_password,
        "command": stage_command,
        "level": stage_level,
        "own": stage_own,
        "shots": stage_shots,
    }[sys.argv[1]]()
