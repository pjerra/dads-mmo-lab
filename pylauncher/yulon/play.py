"""The reads the Characters tab is built on (8.4a).

Reads live here and writes go to the server through `yulon.commands`, which is
owner answer 7 rather than a layering choice: this app reads rows and the server
changes them.

Three of these guard a failure that does not raise, which is why each has a test
naming what it would look like:

* a character name matched in the wrong case answers "no such character" for a
  character standing in Stormwind -- this tree's `characters.name` is
  case-sensitive, and it was the prior art's own bug;
* a search pattern with a `%` in it matches every item in the game, because
  `%` and `_` are LIKE wildcards and a person typing "50% off" did not mean
  that;
* a gear-set query that joins one table where this tree needs two answers a
  list of instance guids that look exactly like item ids.
"""

from __future__ import annotations

from dataclasses import dataclass

from yulon.catalog.catalog import CatalogEntry
from yulon.dbreads import SqlReader
from yulon.log import get_logger

logger = get_logger(__name__)


class NotMeasured(LookupError):
    """This tree has no Play block, so there is nothing here to read it with.

    Raised rather than guessed. The two shapes differ by one join, and the wrong
    one answers a list of instance guids that look exactly like item ids -- so a
    default would be a silent wrong answer on whichever tree it did not fit.
    """


_ESCAPE = "!"
"""The LIKE escape character, NAMED IN THE STATEMENT.

MySQL's default is the backslash, and `NO_BACKSLASH_ESCAPES` turns that off --
so a pattern escaped with backslashes silently stops being escaped on a server
whose sql_mode somebody tightened, and every search matches everything. `!` is
not special anywhere in this path, and `ESCAPE '!'` says so out loud.
"""

EQUIPPED_SLOTS = 19
"""Bag 0, slots 0 to 18: head to tabard, what a character is wearing.

Bags and the bank are also in `character_inventory`, and a set that swept them
up would mail somebody their whole life rather than their gear.
"""

_ITEM_SEARCH_LIMIT = 50
"""46096 items on the WotLK install. A tab that asked for all of them would
freeze on the answer rather than on the query."""


@dataclass(frozen=True)
class Character:
    """One row of the character list."""

    guid: int
    name: str
    level: int
    online: bool
    account: str


@dataclass(frozen=True)
class Item:
    """One row of an item search."""

    item_id: int
    name: str
    quality: int


def literal(text: str) -> str:
    """One string, as a hex blob, so nothing a person types is ever parsed.

    The same shape `yulon.useraccounts` uses. A name typed as
    `guglu' OR 1=1 --` reaches the server as forty hex digits.
    """
    return "_utf8mb4 X'" + text.encode("utf-8").hex().upper() + "'"


def like_literal(text: str) -> str:
    """A LIKE pattern that searches for what was typed, wildcards and all.

    `%` and `_` are escaped with `_ESCAPE`, and the escape character itself is
    escaped first -- otherwise a person typing `!` would break the next one.
    """
    escaped = text.replace(_ESCAPE, _ESCAPE * 2).replace("%", f"{_ESCAPE}%")
    escaped = escaped.replace("_", f"{_ESCAPE}_")
    return literal(f"%{escaped}%")


def canonical_character(sql: SqlReader, entry: CatalogEntry, typed: str) -> str | None:
    """The stored spelling of a character's name, or `None` if there is none.

    This tree's name column is case-sensitive and so is the server's own lookup,
    so `guglu` is not `Guglu` to either of them -- the prior art answered "not
    online" for an online character on exactly that. Every command this app
    builds uses the answer from here rather than what somebody typed.
    """
    characters = entry.schema_map()["characters"]
    found = sql.query(
        "characters",
        f"SELECT name FROM {characters}.characters "
        f"WHERE UPPER(name) = UPPER({literal(typed)}) LIMIT 1;",
    ).strip()
    return found.splitlines()[0].strip() if found else None


def characters(sql: SqlReader, entry: CatalogEntry) -> tuple[Character, ...]:
    """Every character on this server, with the account each belongs to.

    Ordered by name AND guid: `ORDER BY name` alone is not a total order, and
    8.5a's review found that equal keys can put one row on two pages and another
    on none.
    """
    schemas = entry.schema_map()
    rows = sql.query(
        "characters",
        f"SELECT c.guid, c.name, c.level, c.online, a.username "
        f"FROM {schemas['characters']}.characters c "
        f"JOIN {schemas['auth']}.account a ON a.id = c.account "
        "ORDER BY c.name, c.guid;",
    )
    listed = []
    for line in rows.splitlines():
        fields = line.split("\t")
        if len(fields) != 5:
            continue
        guid, name, level, online, account = fields
        listed.append(Character(int(guid), name, int(level), online == "1", account))
    return tuple(listed)


def find_items(sql: SqlReader, entry: CatalogEntry, text: str) -> tuple[Item, ...]:
    """Items whose name contains what was typed, bounded and escaped."""
    world = entry.schema_map()["world"]
    rows = sql.query(
        "world",
        f"SELECT entry, name, Quality FROM {world}.item_template "
        f"WHERE name LIKE {like_literal(text)} ESCAPE '{_ESCAPE}' "
        f"ORDER BY name, entry LIMIT {_ITEM_SEARCH_LIMIT};",
    )
    found = []
    for line in rows.splitlines():
        fields = line.split("\t")
        if len(fields) != 3:
            continue
        item_id, name, quality = fields
        found.append(Item(int(item_id), name, int(quality)))
    return tuple(found)


def equipped(sql: SqlReader, entry: CatalogEntry, character: str) -> tuple[int, ...]:
    """The item ids this character is wearing, in slot order.

    Ids and not id/count pairs: equipped gear does not stack, and a caller that
    mails this set sends each piece as one.

    The shape of the query is the catalog's, because the trees disagree about
    where the item id lives. AzerothCore's inventory row carries the item
    INSTANCE guid and the template id is a join away in `item_instance`;
    CMaNGOS's inventory row carries the template id itself. A query built for
    the wrong one does not fail -- it answers a list of guids that look exactly
    like item ids.
    """
    schemas = entry.schema_map()
    if entry.play is None:
        raise NotMeasured(
            f"{entry.name} has not measured where it keeps a character's equipped items, "
            "so this app cannot read them"
        )
    block = entry.play.equipped
    inventory = f"{schemas['characters']}.character_inventory"
    where = (
        f"WHERE ci.guid = (SELECT guid FROM {schemas['characters']}.characters "
        f"WHERE name = {literal(character)}) "
        f"AND ci.bag = 0 AND ci.slot < {EQUIPPED_SLOTS}"
    )
    if block.instance_table is None:
        statement = (
            f"SELECT ci.{block.template_column} FROM {inventory} ci {where} ORDER BY ci.slot;"
        )
    else:
        instances = f"{schemas['characters']}.{block.instance_table}"
        statement = (
            f"SELECT ii.{block.template_column} FROM {inventory} ci "
            f"JOIN {instances} ii ON ii.guid = ci.{block.inventory_column} "
            f"{where} ORDER BY ci.slot;"
        )
    rows = sql.query("characters", statement)
    return tuple(int(line.split("\t")[0]) for line in rows.splitlines() if line.strip())
