"""Browsing this server's bots, one page at a time (Phase 8.5a).

8.1a already counts them. What this adds is the list, and one fact per row that
the count cannot carry: **which signal identified it**. On this tree there are
two — the playerbots registry and the account-name prefix — and they disagree in
exactly the case that matters. An install whose prefix was changed after its
bots were made has rows the registry knows about and the prefix does not; a
single number hides that, and a person looking for a missing bot has nowhere to
look.

Paged because the owner runs 500 and has said the number is his to raise. Read
live, never cached: the population moves while the tab is open.

**A page is a snapshot, and the population moves.** The order carries a
tiebreak — `name, guid` — because `ORDER BY name` alone is not a total order and
rows with equal keys may come back in any order, which would put one bot on two
pages and another on none. That fixes the ordering; it does not fix `OFFSET`
over a table that is changing, where a row inserted before the boundary shifts
every later page by one. Keyset pagination is the answer to that and is not
written yet (adversarial review, 2026-09-07); what is here is a live list read
fresh on every press, which is what the box asked for.

**The filter is escaped with `!`, not with a backslash.** `NO_BACKSLASH_ESCAPES`
is a real mode on both MySQL and MariaDB, and under it a backslash is an
ordinary character — so a filter escaped with one silently stops escaping and
starts matching. A name with an apostrophe is a name, and it is doubled rather
than stripped.
"""

from __future__ import annotations

from dataclasses import dataclass, field

from yulon.catalog.catalog import CatalogEntry
from yulon.dbreads import Marker, SqlReader, bot_clause
from yulon.log import get_logger

logger = get_logger(__name__)

PAGE_SIZE = 50

ESCAPE = "!"
"""The LIKE escape character, chosen for what it is NOT.

A backslash means nothing under `NO_BACKSLASH_ESCAPES`, which both MySQL and
MariaDB really ship, so an escaped filter would quietly become an unescaped one.
`!` has no meaning in either mode.
"""


@dataclass(frozen=True)
class Bot:
    """One bot character, and the signal that said so."""

    name: str
    level: int
    online: bool
    source: str
    """`registry` or `prefix` — which of this tree's two signals found it."""


@dataclass(frozen=True)
class Page:
    """One page of bots, or why there is none.

    `total` is `None` rather than 0 when the answer is not known, for the reason
    8.1a's `Population` gives: on this server zero is never true, so reporting
    it turns a broken read into a confident lie.
    """

    bots: list[Bot] = field(default_factory=list)
    total: int | None = None
    by_registry: int = 0
    by_prefix: int = 0
    problem: str = ""
    warning: str = ""


def escape_like(text: str) -> str:
    """One user-typed filter, safe inside a quoted LIKE pattern.

    Three characters and no more: the two LIKE wildcards, the escape itself, and
    the quote that ends the literal. Anything else a person types is a
    character in a character's name.
    """
    out = text.replace(ESCAPE, ESCAPE + ESCAPE)
    out = out.replace("%", ESCAPE + "%").replace("_", ESCAPE + "_")
    return out.replace("'", "''")


def page(
    sql: SqlReader,
    entry: CatalogEntry,
    marker: Marker,
    *,
    offset: int = 0,
    limit: int = PAGE_SIZE,
    name_like: str = "",
) -> Page:
    """One page of this install's bots, with the split by signal.

    Two statements and not one: the totals are the answer to "how many, and by
    which signal", and the page is the answer to "show me these fifty". Asking
    them together would mean either counting 500 rows to show 50 or showing 50
    and guessing at the total.
    """
    ops = entry.observability
    if ops is None:
        return Page(problem=f"{entry.name} has no measured bot marker yet, so it will not guess")
    if not marker.prefix.strip():
        return Page(
            problem=(
                "this install's bot marker could not be read, and an empty marker would match "
                "every account on the server"
            )
        )
    clause = bot_clause(entry, marker)
    registry = _registry_clause(entry, ops)
    table = f"{entry.databases.characters}.{ops.characters.table}"
    online = ops.characters.online
    where = f"({clause})"
    if name_like:
        where += f" AND name LIKE '{escape_like(name_like)}%' ESCAPE '{ESCAPE}'"

    try:
        counts = sql.query(
            "characters",
            "SELECT COUNT(*), "
            f"SUM({registry}), "
            f"SUM(NOT ({registry})) "
            f"FROM {table} WHERE {where};",
        )
        rows = sql.query(
            "characters",
            f"SELECT name, level, {online}, "
            f"CASE WHEN {registry} THEN 'registry' ELSE 'prefix' END "
            f"FROM {table} WHERE {where} ORDER BY name, guid "
            f"LIMIT {int(limit)} OFFSET {int(offset)};",
        )
    except Exception as exc:  # noqa: BLE001 - every seam failure is one answer here
        logger.warning(f"could not browse {entry.id}'s bots: {exc}")
        return Page(problem=f"could not read this server's bots: {exc}")

    numbers = _three(counts)
    if numbers is None:
        return Page(
            problem=f"the bot count came back as {counts.strip()!r}, which is not three numbers"
        )
    total, by_registry, by_prefix = numbers

    warning = ""
    if total == 0:
        try:
            characters = sql.query("characters", f"SELECT COUNT(*) FROM {table};").strip()
        except Exception as exc:  # noqa: BLE001
            characters = ""
            logger.info(f"could not ask how many characters exist: {exc}")
        if characters.isdigit() and int(characters) > 0:
            warning = (
                f"no character matched the bot marker {marker.prefix!r}, though this server has "
                f"{characters} characters"
            )

    bots = _rows(rows)
    if isinstance(bots, str):
        return Page(problem=bots, total=None)
    return Page(
        bots=bots,
        total=total,
        by_registry=by_registry,
        by_prefix=by_prefix,
        warning=warning,
    )


def _registry_clause(entry: CatalogEntry, ops: object) -> str:
    """True of a character whose account the playerbots registry owns.

    `1 = 0` where there is no registry, so the CASE still parses and every row
    is attributed to the prefix — which is the truth on a tree that has only
    that signal.
    """
    registry = ops.bots.registry  # type: ignore[attr-defined]
    schemas = entry.schema_map()
    if registry is None or registry.database not in schemas:
        return "1 = 0"
    account = ops.characters.account  # type: ignore[attr-defined]
    types = ", ".join(str(t) for t in registry.types)
    return (
        f"{account} IN (SELECT {registry.account_column} FROM "
        f"{schemas[registry.database]}.{registry.table} "
        f"WHERE {registry.type_column} IN ({types}))"
    )


def _three(raw: str) -> tuple[int, int, int] | None:
    rows = [line for line in raw.splitlines() if line.strip()]
    if len(rows) != 1:
        return None
    fields = rows[0].split("\t")
    if len(fields) != 3:
        return None
    out: list[int] = []
    for value in (f.strip() for f in fields):
        # `SUM()` over no rows is SQL NULL, which is a zero here: a server with
        # no bots at all is a real state, not a broken query.
        if value in ("NULL", ""):
            out.append(0)
        elif value.lstrip("-").isdigit():
            out.append(int(value))
        else:
            return None
    return out[0], out[1], out[2]


def _rows(raw: str) -> list[Bot] | str:
    """The page's rows, or the sentence explaining why there are none."""
    bots: list[Bot] = []
    for line in raw.splitlines():
        if not line.strip():
            continue
        fields = line.split("\t")
        if len(fields) != 4 or not fields[1].strip().lstrip("-").isdigit():
            return f"a bot row came back as {line.strip()!r}"
        bots.append(
            Bot(
                name=fields[0].strip(),
                level=int(fields[1]),
                online=fields[2].strip() == "1",
                source=fields[3].strip(),
            )
        )
    return bots
