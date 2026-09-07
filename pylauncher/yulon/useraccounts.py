"""The accounts a person may act on, and what they may do to them (Phase 8.3a).

A read, and what it leaves OUT is the point. A WotLK install with the owner's
settings carries 500 bot accounts and one account this app made for itself. A
list of all 502 is a list nobody can use, and one of those rows is the
launcher's own credential — handing a user a Set-password button for it would
break the channel every other feature in Phase 8 rides on.

Both exclusions are in the WHERE rather than applied to the answer. Filtering
afterwards leaves the rows on the wire and in any log of the statement, and puts
the burden on every future caller to remember what this one knew.

**Where the GM level lives is a per-tree fact.** AzerothCore keeps it in
`account_access` keyed `id`; the CMaNGOS trees keep it on the account row, under
two different names. Reading the wrong one does not fail — it shows every
account as level 0, which is a lie shaped like an answer — so an entry whose
level store has not been measured refuses rather than guessing.
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path

from yulon import commands
from yulon.catalog.catalog import CatalogEntry
from yulon.dbreads import Marker, SqlReader, bot_clause, resolve_marker
from yulon.log import get_logger

logger = get_logger(__name__)


@dataclass(frozen=True)
class Account:
    """One account a person may act on."""

    id: int
    username: str
    gm_level: int


@dataclass(frozen=True)
class Listing:
    """The accounts, or why there are none to show.

    `problem` is not decoration: an empty list and an unreadable database look
    identical on screen, and one of them means "this server has no accounts"
    while the other means "do not trust what you are looking at".
    """

    accounts: list[Account] = field(default_factory=list)
    problem: str = ""


APP_PREFIX = "YULON_"
"""The prefix every account this app makes for itself carries.

The guard reads THIS and not one install's own name, because two installs can
share an auth database: `YULON_AAAAAAAA` and `YULON_BBBBBBBB` are two channel
accounts, and a guard that knows only its own would list the neighbour's and
offer to change its password -- ending that install's command channel while
saying nothing about it (adversarial review, 2026-09-07).

The underscore is load-bearing. `YULONGATE` is a person's account on the gate
box and is theirs to change; `LEFT(username, 6)` tells them apart, where a LIKE
would not because `_` is a wildcard.
"""


def accounts(sql: SqlReader, entry: CatalogEntry, marker: Marker, *, app_account: str) -> Listing:
    """This install's accounts, without the bots and without the app's own."""
    level = entry.accounts.level
    if level is None:
        return Listing(
            problem=(
                f"{entry.name} keeps its GM levels somewhere this app has not measured yet, "
                "so it will not guess at them"
            )
        )
    schemas = entry.schema_map()
    auth = schemas["auth"]
    bots = _bot_accounts_clause(entry, marker)
    if level.table is None:
        # The level is a column on the account row itself, so there is nothing
        # to join and no realm to consider.
        selected = f"COALESCE(a.{level.level_column}, 0)"
        join = ""
    else:
        selected = f"COALESCE(MAX(x.{level.level_column}), 0)"
        join = f" LEFT JOIN {auth}.{level.table} x ON x.{level.account_column} = a.id"
    group = "" if level.table is None else " GROUP BY a.id, a.username"
    statement = (
        f"SELECT a.id, a.username, {selected} FROM {auth}.account a{join} "
        f"WHERE NOT ({bots}) AND LEFT(a.username, {len(APP_PREFIX)}) <> '{APP_PREFIX}'"
        f"{group} ORDER BY a.username;"
    )
    try:
        raw = sql.query("auth", statement)
    except Exception as exc:  # noqa: BLE001 - every seam failure is one answer here
        logger.warning(f"could not read {entry.id}'s accounts: {exc}")
        return Listing(problem=f"could not read this server's accounts: {exc}")
    return _rows(raw)


def _bot_accounts_clause(entry: CatalogEntry, marker: Marker) -> str:
    """`bot_clause()`, but true of a row in `account` rather than in `characters`.

    The same two arms and the same reasons — the registry first where there is
    one, the name prefix always — expressed against the account's own `id` and
    `username` instead of a character's `account` column.
    """
    ops = entry.observability
    arms = [f"UPPER(a.username) LIKE '{marker.prefix.upper()}%'"]
    if ops is not None:
        registry = ops.bots.registry
        schemas = entry.schema_map()
        if registry is not None and registry.database in schemas:
            types = ", ".join(str(t) for t in registry.types)
            arms.insert(
                0,
                f"a.id IN (SELECT {registry.account_column} FROM "
                f"{schemas[registry.database]}.{registry.table} "
                f"WHERE {registry.type_column} IN ({types}))",
            )
    _ = bot_clause  # the characters-table sibling; kept named so the pair is findable
    return " OR ".join(arms)


def _rows(raw: str) -> Listing:
    """Three tab-separated fields per line, and nothing halfway.

    A line that will not parse fails the whole listing rather than being
    skipped: a list quietly missing one account is worse than no list, because
    nothing on screen says an account is missing.
    """
    out: list[Account] = []
    for line in raw.splitlines():
        if not line.strip():
            continue
        fields = line.split("\t")
        if len(fields) != 3 or not fields[0].strip().isdigit():
            return Listing(problem=f"an account row came back as {line.strip()!r}")
        level = fields[2].strip()
        if not level.lstrip("-").isdigit():
            return Listing(problem=f"an account row came back as {line.strip()!r}")
        out.append(Account(id=int(fields[0]), username=fields[1].strip(), gm_level=int(level)))
    return Listing(accounts=out)


# -- what a person may do to one, and to which ------------------------------


@dataclass(frozen=True)
class Outcome:
    """What happened, in the three shapes an answer can take.

    `done` false with an empty `problem` never happens; `done` false and a
    problem naming an unreachable server is NOT the same as one naming a
    refusal, and the tab says which. Reporting "could not ask" as "did not
    work" would have the user believe a password is unchanged when nobody
    knows whether it is.
    """

    done: bool
    text: str = ""
    problem: str = ""
    indeterminate: bool = False
    """The command may have run, and nobody knows whether it did.

    A SOAP timeout is not a failure: the listener queues onto the world thread
    and blocks until the command finishes, so a client giving up says nothing
    about whether the server did. Reported as a plain failure, a person retypes
    the old password and is locked out of an account whose password has already
    changed (adversarial review, 2026-09-07).
    """


def set_password(channel: object, *, account: str, password: str, app_account: str) -> Outcome:
    """`account set password <user> <pass> <pass>`, through the server itself.

    Owner answer 7: the server performs its own character-database writes. A
    password written here as a row is a row this app would have to get exactly
    right on every core, forever, and getting it wrong inserts something that
    looks correct and can never log in.
    """
    refusal = _not_our_own(account, app_account, "have its password changed")
    if refusal is not None:
        return refusal
    try:
        line = commands.account_set_password(account, password)
    except commands.CommandError as exc:
        return Outcome(False, problem=str(exc))
    return _send(channel, line)


def set_gm_level(
    channel: object, *, account: str, level: int, app_account: str, realms: bool
) -> Outcome:
    """`account set gmlevel <user> <n>`, with the realm argument where there are realms.

    `realms` comes from the entry -- a tree whose level is a column on the
    account row has no realm to name -- and is passed rather than defaulted, for
    the reason `commands.account_set_gm_level()` gives.
    """
    refusal = _not_our_own(account, app_account, "have its GM level changed")
    if refusal is not None:
        return refusal
    try:
        line = commands.account_set_gm_level(account, level, realms=realms)
    except commands.CommandError as exc:
        return Outcome(False, problem=str(exc))
    return _send(channel, line)


def _not_our_own(account: str, app_account: str, what: str) -> Outcome | None:
    """The app's own account is not the user's to change.

    Here rather than in the tab, where it would be one forgotten `if` away from
    being gone. Changing this account's password breaks the command channel
    every other Phase 8 feature rides on; dropping its level below administrator
    breaks it just as thoroughly, and neither failure says what it was.
    """
    name = account.strip().upper()
    if not name.startswith(APP_PREFIX) and name != app_account.strip().upper():
        return None
    mine = name == app_account.strip().upper()
    whose = "its own command channel" if mine else "another install's command channel"
    return Outcome(
        False,
        problem=(
            f"{account} is an account this app made for {whose}, and it cannot {what} from "
            "here — the channel it belongs to would stop working"
        ),
    )


def _send(channel: object, line: str) -> Outcome:
    answer = channel.send(line)  # type: ignore[attr-defined]
    outcome = getattr(answer, "outcome", "")
    if outcome == "yes":
        return Outcome(True, text=getattr(answer, "text", ""))
    if outcome == "no":
        return Outcome(False, problem=getattr(answer, "text", "the server refused"))
    reason = getattr(answer, "reason", "") or "the server could not be asked"
    if getattr(answer, "indeterminate", False):
        return Outcome(
            False,
            indeterminate=True,
            problem=(
                f"{reason}. The change may already have been made — the server keeps working "
                "on a command after this app stops waiting. Check before trying it again."
            ),
        )
    return Outcome(False, problem=reason)


# -- what the tab is handed --------------------------------------------------


class InstallAccounts:
    """One install's accounts, as the Accounts tab sees them (8.3a).

    Reads go to the database and writes go to the server, and that split is
    owner answer 7 rather than an implementation detail: this app reads rows
    and the server changes them.

    Both halves need the app's own account name, for opposite reasons — the
    read leaves it out of the list, the writes refuse it — so it is held here
    once instead of being passed in at every call site.
    """

    def __init__(
        self,
        entry: CatalogEntry,
        server_dir: Path,
        *,
        sql: SqlReader,
        channel_for_saved: Callable[[], object | None],
        app_account: str,
    ) -> None:
        self.entry = entry
        self.server_dir = server_dir
        self._sql = sql
        self._channel_for_saved = channel_for_saved
        self.app_account = app_account

    def listing(self) -> Listing:
        """The accounts, with this install's live bot marker."""
        answer = resolve_marker(self.entry, self.server_dir)
        if answer.marker is None:
            return Listing(problem=answer.problem or "this install's bot marker could not be read")
        return accounts(self._sql, self.entry, answer.marker, app_account=self.app_account)

    def set_password(self, account: str, password: str) -> Outcome:
        channel = self._channel()
        if channel is None:
            return Outcome(False, problem=_NO_CHANNEL)
        return set_password(
            channel, account=account, password=password, app_account=self.app_account
        )

    def set_gm_level(self, account: str, level: int) -> Outcome:
        channel = self._channel()
        if channel is None:
            return Outcome(False, problem=_NO_CHANNEL)
        # From the entry, not from a default: a tree whose level lives on the
        # account row has no realm to name (8.3b).
        level_block = self.entry.accounts.level
        return set_gm_level(
            channel,
            account=account,
            level=level,
            app_account=self.app_account,
            realms=level_block is not None and level_block.table is not None,
        )

    def _channel(self) -> object | None:
        return self._channel_for_saved()


_NO_CHANNEL = (
    "the command channel is not set up for this install yet, and these changes are made by "
    "the server rather than by writing rows. Turn it on from the Server tab."
)
