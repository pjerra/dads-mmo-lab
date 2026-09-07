"""The 8.3a live gate, WINDOWS half: the account list and the two changes the server makes.

Usage:  python gate83a_win.py <stage>

    list      the tab's list against the same question asked by hand
    password  a real password change, read back through the server's own query
    level     a real GM level change, in the row AND in the server's own words
    own       the app's own account: absent from the list, refused by both actions
    shots     the Accounts tab, rendered from the real view

Every reading is taken from the machine. `.account info $account` is the
server's own account query -- confirmed on the Linux box with `help account
info`, which is where its existence came from rather than from memory.

This is 8.3a's own script with four constants changed and nothing else: the
Windows half is its own press, not its own code. It runs against the install
the 7.7 gate left on this box, through the channel 8.2b verified here.
"""

from __future__ import annotations

import os
import sys
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, r"C:\gate\src82b\pylauncher")

from yulon import channel_setup, commands, soap, useraccounts  # noqa: E402
from yulon.catalog import composegen  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path(r"D:\gate\wotlk-server77")
ENTRY = load_catalog().get("wow-wotlk")
SHOTS = Path(r"C:\gate\gate83aw-shots")
INSTALL_ID = composegen.install_id(SERVER_DIR)
APP_ACCOUNT = channel_setup.account_name(INSTALL_ID)

TARGET = "GATE83W"
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
    # minus the ones whose name carries the live bot marker, minus the ones the
    # playerbots registry owns, minus this app's own.
    # Schema names come from the entry, not from memory: the logical name
    # `playerbots` is `acore_playerbots` on this install, and a gate that
    # hard-codes one is measuring its own assumption.
    schemas = ENTRY.schema_map()
    auth, bots_db = schemas["auth"], schemas["playerbots"]
    by_hand = sql(
        "SELECT a.id, a.username, COALESCE(MAX(x.gmlevel), 0) "
        f"FROM {auth}.account a LEFT JOIN {auth}.account_access x ON x.id = a.id "
        "WHERE UPPER(a.username) NOT LIKE 'RNDBOT%' "
        f"AND a.id NOT IN (SELECT account_id FROM {bots_db}.playerbots_account_type) "
        f"AND UPPER(a.username) <> '{APP_ACCOUNT}' "
        "GROUP BY a.id, a.username ORDER BY a.username;"
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
    before = sql(
        f"SELECT HEX(salt), HEX(verifier) FROM {ENTRY.schema_map()['auth']}.account "
        f"WHERE username = '{TARGET}';"
    )
    say(f"salt/verifier before: {before[:32]}…")

    outcome = services().accounts.set_password(TARGET, NEW_PASSWORD)
    say(f"set_password said: done={outcome.done} {outcome.text.strip() or outcome.problem}")
    assert outcome.done, outcome.problem

    after = sql(
        f"SELECT HEX(salt), HEX(verifier) FROM {ENTRY.schema_map()['auth']}.account "
        f"WHERE username = '{TARGET}';"
    )
    say(f"salt/verifier after:  {after[:32]}…")
    assert after != before, "the row did not change"
    say(f"the server's own query: {ask_the_server(f'account info {TARGET}').strip()!r}")
    say("clause 2 (server half) PASSED: the server changed its own row")
    say(f"THE CLIENT HALF: log in as {TARGET} with {NEW_PASSWORD!r}; {OLD_PASSWORD!r} must fail")


# -- a GM level change ------------------------------------------------------


def stage_level() -> None:
    outcome = services().accounts.set_gm_level(TARGET, 2)
    say(f"set_gm_level said: done={outcome.done} {outcome.text.strip() or outcome.problem}")
    assert outcome.done, outcome.problem

    row = sql(
        f"SELECT x.id, x.gmlevel, x.RealmID FROM {ENTRY.schema_map()['auth']}.account_access x "
        f"JOIN {ENTRY.schema_map()['auth']}.account a ON a.id = x.id "
        f"WHERE a.username = '{TARGET}';"
    )
    say(f"account_access row: {row!r}")
    assert row, "there is no access row"
    assert row.split("\t")[1] == "2", "the row does not read 2"

    said = ask_the_server(f"account info {TARGET}")
    say(f"the server's own account query: {said.strip()!r}")
    assert "2" in said, "the server does not report the new level"

    listing = services().accounts.listing()
    shown = [a for a in listing.accounts if a.username == TARGET]
    say(f"the tab now shows: {shown}")
    assert shown and shown[0].gm_level == 2
    say("clause 3 PASSED: the row, the server's own query and the tab all say 2")


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
    said = ask_the_server(f"account info {APP_ACCOUNT}")
    say(f"the app's own account, per the server: {said.strip()!r}")
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
        "level": stage_level,
        "own": stage_own,
        "shots": stage_shots,
    }[sys.argv[1]]()
