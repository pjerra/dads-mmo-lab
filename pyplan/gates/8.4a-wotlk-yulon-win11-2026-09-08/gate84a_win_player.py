"""Give this box a character with no second author, for the ONLINE clause.

8.4a measured on `yulon-ubuntu` why the online arm cannot be run on a bot: a
taken-over bot account authenticates and never reaches the world, because the
playerbots module owns those sessions -- and an online bot's level is rewritten
by the module within the minute, so an assertion on it proves nothing about the
app. That box had a character a person had made by hand. **This box has none:
1000 characters and 1000 of them are bots**, which the 8.5a half measured an
hour earlier.

So one is made here, and this file is the box's SETUP rather than one of its
clauses. Two steps, and only the first is the app:

* `account` -- the account, through the app's own `create_account` seam, the
  one 8.3a's Create-account control calls;
* `adopt`   -- one OFFLINE bot character's `account` column moved to it, by
  hand, which is what takes the character off the module's roster: the roster
  is built from the registry's accounts, and a character on an account that is
  not in it is nobody's bot.

The move is written down rather than hidden because it changes what the 8.5a
half counted: 1000 of 1000 becomes 999 of 1000, and the re-read is in that
folder.

Usage:  python gate84a_win_player.py account|adopt|state
"""

from __future__ import annotations

import sys
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, r"C:\gate\src84a\pylauncher")

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.controller_wow_wotlk import accounts as wotlk_accounts  # noqa: E402
from yulon.ui import controller_view as view_module  # noqa: E402

SERVER_DIR = Path(r"D:\wow-server")
ENTRY = load_catalog().get("wow-wotlk")
ACCOUNT = "GATE84W"
PASSWORD = "gate84w-p@ss"


def say(text: str) -> None:
    print(f"[{datetime.now(UTC).strftime('%H:%M:%SZ')}] {text}", flush=True)


def reader():
    return view_module._sql_for(ENTRY, view_module._db_password(ENTRY, SERVER_DIR), wsl_distro=None)


def query(db: str, statement: str) -> str:
    return reader().query(db, statement).strip()


def stage_account() -> None:
    before = query("auth", f"SELECT id, username FROM acore_auth.account WHERE username = '{ACCOUNT}';")
    say(f"===== GROUND: the account row for {ACCOUNT} before the press: {before!r} =====")
    result = wotlk_accounts.create_account(reader(), ACCOUNT, PASSWORD, gm_level=0)
    say(f"create_account: {result}")
    after = query("auth", f"SELECT id, username FROM acore_auth.account WHERE username = '{ACCOUNT}';")
    say(f"the account row now: {after!r}")
    assert after, "no account row"


def _account_id() -> str:
    return query("auth", f"SELECT id FROM acore_auth.account WHERE username = '{ACCOUNT}';")


def stage_adopt() -> None:
    """Move one offline, unflagged bot character onto this account."""
    account_id = _account_id()
    assert account_id, "run the `account` stage first"
    wanted = query(
        "characters",
        "SELECT c.name, c.guid, c.account, c.level, c.at_login FROM acore_characters.characters c "
        "JOIN acore_characters.character_inventory i ON i.guid = c.guid "
        "WHERE c.online = 0 AND c.at_login = 0 AND i.bag = 0 AND i.slot < 19 "
        "GROUP BY c.name, c.guid, c.account, c.level, c.at_login "
        "ORDER BY COUNT(*) DESC, c.name LIMIT 1;",
    ).split("\t")
    name, guid, old_account, level, at_login = wanted
    say(f"===== GROUND: {name} (guid {guid}) is on account {old_account}, level {level}, "
        f"at_login {at_login} =====")
    bots_before = query(
        "characters",
        "SELECT COUNT(*) FROM acore_characters.characters WHERE account IN "
        "(SELECT account_id FROM acore_playerbots.playerbots_account_type WHERE account_type IN (1,2,3));",
    )
    say(f"characters on registry accounts before the move: {bots_before}")
    assert old_account != account_id, "this character is already on the gate account"
    reader().run_statement(
        "characters",
        f"UPDATE acore_characters.characters SET account = {int(account_id)} "
        f"WHERE guid = {int(guid)};",
    )
    now = query(
        "characters",
        f"SELECT account FROM acore_characters.characters WHERE guid = {int(guid)};",
    )
    say(f"{name}'s account column: {old_account} -> {now} (the gate account is {account_id})")
    assert now == account_id
    say(f"THE CLIENT HALF: log in as {ACCOUNT} / {PASSWORD} and play {name}")


def stage_state() -> None:
    account_id = _account_id()
    say(f"account {ACCOUNT} is id {account_id!r}")
    say(query(
        "characters",
        f"SELECT name, guid, level, online, at_login FROM acore_characters.characters "
        f"WHERE account = {int(account_id)};",
    ))


if __name__ == "__main__":
    {"account": stage_account, "adopt": stage_adopt, "state": stage_state}[sys.argv[1]]()
