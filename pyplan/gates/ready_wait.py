"""How each game's own `wait_server_ready()` is called, spelled once for every gate script.

**WHY THIS IS A MODULE AND NOT A TABLE INSIDE ONE SCRIPT.** It was a table inside
`gate-79-controller-surface.py`, and `pyplan/gates/7.10-rerun-ubuntu2-2026-09-08/
wait_ready.py` -- written the same night to replace a blind `sleep 180` -- copied
the WotLK row by hand rather than importing it. Both then carried the same defect,
and the second one is where it was measured: killed at `00:44:06` after **8 m 19 s**
against a world that had been up the whole time (`ready-after-restore.txt`). One
spelling in one place is the fix for the copy as much as for the row.

**WHAT THE DEFECT WAS.** The WotLK row typed a realm address instead of asking for
one:

    "wow-wotlk": lambda module, auth_port: bool(
        module.wait_server_ready("127.0.0.1", auth_port)
    ),

`docker.azerothcore_ready()` builds the AUTH marker as `f"{realm_host}:{realm_port}"`
(`pylauncher/yulon/docker.py`), and an AzerothCore authserver prints that pair in
exactly one line -- `Added realm "<name>" at <address>:<port>.` -- with both halves
read from `acore_auth.realmlist`: the ADDRESS column, and the PORT column, which is
the WORLD port. So the typed pair was wrong twice over. Measured on `yulon-ubuntu2`
2026-09-09, both calls against the same server in the same minute
(`ready-marker-probe.txt`):

| call | result |
|---|---|
| `wait_server_ready("127.0.0.1", 3724)` -- typed | still waiting after 8 m 19 s, killed |
| `wait_server_ready("172.30.48.189", 8085)` -- the row's own | `ready=True` after **0.3 s** |

Occurrences of `127.0.0.1:3724` in that auth log: **0**. Of `172.30.48.189:8085`:
**1**. `ready...` appeared 4 times in the world log throughout, so nothing was slow.

**WHY IT SURVIVED 7.9 GREEN.** The other three rows pass nothing that reaches a
marker: TBC and Vanilla take no realm arguments at all, and Tortoise declares
`ready.auth: null` with a `regex: true` world marker that names no `{{TOKEN}}`
(read from `catalog.json`, 2026-09-09) -- so its two arguments are filled into
markers that contain neither. `wow-wotlk` is the only row whose arguments have to
be RIGHT, and it had never been exercised: 7.9 ran the three CMaNGOS games.

**THE SIBLING FIX, WHICH THIS MUST NOT UNDO.** `native.REALM_ADDRESS_PATTERN`
(`5a7e8baa`) answers the same question for the app's own management waits by
making `{{REALM_HOST}}` a WILDCARD and keeping the port exact, because
`_advertise_realm()` is an install's last act and the row it writes is not the
address the installer used. That is the right answer where a pattern can be
built. It is not available here: `wait_server_ready()` -> `azerothcore_ready()`
escapes the host, so a wildcard handed to it arrives as the literal characters
`\\S+`. A gate can do the other thing the app cannot -- ask the database -- and
that is what this does.

Nothing here starts, stops or writes anything. The one statement it sends is a
SELECT.
"""

from __future__ import annotations

import importlib
from collections.abc import Callable
from pathlib import Path
from types import ModuleType
from typing import NamedTuple, Protocol

from yulon import docker
from yulon.catalog.catalog import CatalogEntry

REALM_PORT_COLUMN = "port"
"""The realmlist column holding the port the authserver advertises.

**The catalog is SILENT on this and that is why it is named here.**
`catalog.Realmlist` declares `table`, `address_column`, `local_address_column`
and `realm_id` -- everything `networking.realmlist_sql()` WRITES -- and no port
column, because the app never writes one. This is a gate reading a column the app
only reads, so the name lives with the reader that needs it rather than being
added to a model four packages share.

`port` is the spelling on both cores in the catalog (AzerothCore's
`acore_auth.realmlist` and CMaNGOS's `realmd.realmlist`), and the row this was
measured against is in `ready-marker-probe.txt`:
`address='172.30.48.189' port='8085'`. A core that spelled it differently would
fail the SELECT with the client's own "Unknown column" message, which names the
column and this constant is the one place to change.
"""


class RealmRowError(Exception):
    """The realm row could not be read, so this game's ready marker cannot be spelled."""


class UnknownGameError(Exception):
    """A catalog id no gate script has a `wait_server_ready()` spelling wired for."""


class SqlQuery(Protocol):
    """`docker.sql_query`'s shape, so a test can drive the reader without a database."""

    def __call__(
        self,
        container: str,
        client: str,
        password: str,
        schema: str | None,
        statement: str,
        /,
    ) -> str: ...


def realm_address_query(entry: CatalogEntry) -> str:
    """The SELECT that answers what this install's authserver advertises.

    Built from the entry rather than typed, so the schema, the table, the address
    column and the realm id are the same facts the app writes through
    `networking.realmlist_sql()`. `networking.realmlist_address_query()` is the
    app's own reader of that row and would have been reused, but it selects the
    columns the UPDATE writes -- address and localAddress -- and the port is what
    the marker's second half needs.
    """
    rl = entry.realmlist
    return (
        f"SELECT {rl.address_column}, {REALM_PORT_COLUMN} "
        f"FROM {entry.databases.auth}.{rl.table} WHERE id={rl.realm_id};"
    )


def realm_pair(
    entry: CatalogEntry,
    server_dir: Path,
    *,
    query: SqlQuery = docker.sql_query,
    wsl_distro: str | None = None,
) -> tuple[str, int]:
    """(address, port) as THIS install's auth database has them. Never a guess.

    Read through `docker.sql_query()` -- the app's own seam, `--batch
    --skip-column-names`, statement on stdin and the password in `MYSQL_PWD`, so
    no secret is ever in an argv a gate log could catch (`gate-logs-carry-
    generated-passwords`). The password comes from
    `entry.install.db_password(server_dir)`, which knows both the fixed and the
    generated plans; it is never printed.

    **It refuses rather than falling back.** A default pair is how the defect
    above worked: something plausible went in, the wait could never match, and
    the gate reported six hours of nothing as a server that never came up. A
    refusal here is booked as that section's FAIL with the reason in it, which is
    an honest report; a guess is not.

    Raises:
        RealmRowError: the password is not knowable, the query failed, or the
            answer was not one row of two fields with an integer port.
    """
    password = entry.install.db_password(server_dir)
    if password is None:
        raise RealmRowError(
            f"{entry.name}'s database password is not knowable from {server_dir} "
            f"(the entry names a generated password file and it could not be read), so the "
            f"realm row cannot be read and this game's ready marker cannot be spelled. "
            f"Nothing was started."
        )
    statement = realm_address_query(entry)
    try:
        answer = query(
            entry.container_spec().db,
            entry.install.native.db.client,
            password,
            None,
            statement,
        )
    except Exception as exc:  # noqa: BLE001 - every failure has the same consequence here
        raise RealmRowError(
            f"the realm row could not be read ({type(exc).__name__}: {exc}). The statement was "
            f"{statement!r}. Without it this game's auth marker cannot be spelled, and a typed "
            f"address is what this replaced."
        ) from exc
    rows = [row for row in answer.splitlines() if row.strip()]
    if len(rows) != 1:
        raise RealmRowError(
            f"{statement!r} answered {len(rows)} rows, not one. No rows means a realm id this "
            f"install does not have; more than one is not one realm's row. Neither is an "
            f"address to wait for."
        )
    fields = [field.strip() for field in rows[0].split("\t")]
    if len(fields) != 2:
        raise RealmRowError(
            f"{statement!r} answered {len(fields)} fields, not two: {fields!r}. The reader wants "
            f"an address and a port."
        )
    address, port = fields
    if not address:
        raise RealmRowError(
            f"the realm row's {entry.realmlist.address_column} is empty, so the authserver has no "
            f"address to advertise and no marker can name one."
        )
    try:
        return address, int(port)
    except ValueError as exc:
        raise RealmRowError(
            f"the realm row's {REALM_PORT_COLUMN} is {port!r}, which is not a port number."
        ) from exc


class ReadyArgs(NamedTuple):
    """What a row may ask for, so that no row can be handed what it must not use.

    `realm_pair` is a THUNK and not a value: the three CMaNGOS rows never call it,
    so their gates never send a SELECT, and a refusal to read the row is raised
    for the one game whose marker needs it rather than for all four.
    """

    auth_port: int
    realm_pair: Callable[[], tuple[str, int]]


READY_CALLS: dict[str, Callable[[ModuleType, ReadyArgs], bool]] = {
    # No realm arguments: this entry has no auth marker to spell, and its
    # `wait_server_ready()` raises `TypeError` on anything but timeout/interval.
    "wow-tbc": lambda module, args: bool(module.wait_server_ready()),
    # `realm_host` defaults and no port is accepted -- `ready.auth` is null.
    "wow-vanilla": lambda module, args: bool(module.wait_server_ready()),
    # Both required by the signature, and NEITHER reaches a marker: this entry
    # declares `ready.auth: null` and its `regex: true` world marker names no
    # `{{TOKEN}}` (read from catalog.json 2026-09-09). Left as a typed pair
    # deliberately -- reading the realm row for it would send a SELECT whose
    # answer nothing can use, and a failure to read it would fail a gate for a
    # value the wait never looks at.
    "wow-tortoise": lambda module, args: bool(
        module.wait_server_ready("127.0.0.1", args.auth_port)
    ),
    # THE REALM ROW'S OWN PAIR, because this is the one row whose arguments ARE
    # the marker: `azerothcore_ready()` waits for `<address>:<world port>` in the
    # authserver's log and the authserver prints what `acore_auth.realmlist`
    # holds. The typed pair this replaced could not match on any install whose
    # address had moved off the installer's loopback -- which is every install,
    # after `_advertise_realm()`.
    "wow-wotlk": lambda module, args: bool(module.wait_server_ready(*args.realm_pair())),
}


def unknown_game_error(game: str) -> UnknownGameError:
    """The refusal for an id with no `READY_CALLS` entry, worded once."""
    return UnknownGameError(
        f"this harness has no wait_server_ready() spelling wired for {game!r}, so it cannot "
        f"tell whether that game's server is up and must not report on one. The ids it is "
        f"wired for are: {', '.join(sorted(READY_CALLS))}. To add {game!r}, give it its own "
        f"READY_CALLS entry with the arguments that game's own wait_server_ready() takes; "
        f"borrowing another game's spelling is what this replaced."
    )


def wait_ready_for_game(
    entry: CatalogEntry,
    server_dir: Path,
    *,
    pair: Callable[[], tuple[str, int]] | None = None,
) -> bool:
    """That game's own `wait_server_ready()`, with the arguments it actually takes.

    `pair` is the seam the tests drive; left None it reads the install's own
    realm row, lazily, and only for a row that asks for it.

    Raises:
        UnknownGameError: `entry.id` has no entry in `READY_CALLS`.
        RealmRowError: the row was needed and could not be read.
    """
    call = READY_CALLS.get(entry.id)
    if call is None:
        raise unknown_game_error(entry.id)
    module = importlib.import_module(f"yulon.controller_{entry.id.replace('-', '_')}.docker_ctl")
    args = ReadyArgs(
        auth_port=entry.container_spec().ports[0],
        realm_pair=pair if pair is not None else lambda: realm_pair(entry, server_dir),
    )
    return call(module, args)
