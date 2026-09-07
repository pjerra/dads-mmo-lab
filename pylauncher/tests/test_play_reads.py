"""Tests for `yulon.play`'s reads — the lists the Characters tab is built on.

Three of these guard a failure that does not raise: a name matched in the wrong
case answers "no such character" for a character that is right there, a search
pattern with `%` in it matches everything, and a gear-set query that joins one
table where this tree needs two answers zero items for a fully equipped
character.
"""

from __future__ import annotations

from yulon import play
from yulon.catalog.catalog import load_catalog

WOTLK = load_catalog().get("wow-wotlk")
TBC = load_catalog().get("wow-tbc")


class _Reader:
    """A SQL seam that answers from a script and records what it was asked."""

    def __init__(self, *answers: str) -> None:
        self.answers = list(answers)
        self.asked: list[tuple[str, str]] = []

    def query(self, db: str, statement: str) -> str:
        self.asked.append((db, statement))
        return self.answers.pop(0) if self.answers else ""

    def run_statement(self, db: str, statement: str) -> None:  # pragma: no cover
        raise AssertionError("a read must not write")


# -- finding the character somebody typed -----------------------------------


def test_a_name_typed_in_the_wrong_case_still_finds_the_character() -> None:
    """The prior art's own bug, recorded in this box's line: this tree's name
    column is case-sensitive, so `guglu` answered "not online" for a character
    called `Guglu` who was standing in Stormwind.

    The lookup folds the case and returns THE STORED FORM, which is then what
    every command is built with — the server is just as case-sensitive as its
    column.
    """
    sql = _Reader("Guglu\n")

    assert play.canonical_character(sql, WOTLK, "guglu") == "Guglu"
    assert "UPPER" in sql.asked[0][1], sql.asked[0][1]


def test_a_name_that_is_not_there_answers_nothing_rather_than_itself() -> None:
    """Returning the typed name would send a command for a character that does
    not exist, and the server's refusal would name a character nobody has."""
    assert play.canonical_character(_Reader(""), WOTLK, "nobody") is None


def test_a_name_lookup_sends_the_typed_text_as_a_literal_and_not_as_sql() -> None:
    sql = _Reader("Guglu\n")

    play.canonical_character(sql, WOTLK, "guglu' OR 1=1 -- ")

    statement = sql.asked[0][1]
    assert "OR 1=1" not in statement, statement
    assert "X'" in statement, "the name is not sent as a hex literal"


# -- the character list -----------------------------------------------------


def test_the_character_list_carries_what_the_tab_shows() -> None:
    sql = _Reader("1\tGuglu\t78\t1\tADMIN\n2\tGanaar\t7\t0\tPLAYER\n")

    listed = play.characters(sql, WOTLK)

    assert [(c.name, c.level, c.online, c.account) for c in listed] == [
        ("Guglu", 78, True, "ADMIN"),
        ("Ganaar", 7, False, "PLAYER"),
    ]


def test_the_character_list_is_ordered_by_something_that_is_a_total_order() -> None:
    """8.5a's review, applied here before it can bite: `ORDER BY name` alone is
    not a total order, and two characters can share a name across realms. The
    guid is the tiebreak, so a paged list cannot show one twice and another not
    at all."""
    sql = _Reader("")

    play.characters(sql, WOTLK)

    assert "ORDER BY" in sql.asked[0][1]
    assert "guid" in sql.asked[0][1].split("ORDER BY")[1]


# -- searching for an item --------------------------------------------------


def test_an_item_search_matches_the_middle_of_a_name() -> None:
    sql = _Reader("6948\tHearthstone\t1\n")

    found = play.find_items(sql, WOTLK, "hearth")

    assert [(i.item_id, i.name, i.quality) for i in found] == [(6948, "Hearthstone", 1)]


def _pattern(statement: str) -> str:
    """The LIKE pattern the server will actually see, decoded from the statement.

    The whole point of sending it as a hex blob is that the text never appears
    in the statement, so a test that greps the statement for the escaping cannot
    see whether any happened. This decodes it and reads what was sent -- a
    mutation that removed the escaping survived the earlier version of this
    test, which only asserted that the word ESCAPE was present.
    """
    body = statement.split("X'")[1].split("'")[0]
    return bytes.fromhex(body).decode("utf-8")


def test_a_percent_typed_by_a_person_searches_for_a_percent() -> None:
    """`%` and `_` are wildcards in LIKE, so a person typing `50%` would match
    every item in the game. They are escaped — and with an escape character
    NAMED IN THE STATEMENT rather than the backslash MySQL uses by default,
    because `NO_BACKSLASH_ESCAPES` turns that default off and the escaping would
    silently stop working."""
    sql = _Reader("")

    play.find_items(sql, WOTLK, "50% _ off")

    statement = sql.asked[0][1]
    assert _pattern(statement) == "%50!% !_ off%", _pattern(statement)
    assert "ESCAPE '!'" in statement, statement
    assert "\\" not in statement, "a backslash escape is the one that stops working"


def test_the_escape_character_itself_is_escaped_before_it_escapes_anything() -> None:
    """A person typing `!` would otherwise turn the NEXT character into a
    literal, so `a!%b` would search for `a` followed by anything."""
    sql = _Reader("")

    play.find_items(sql, WOTLK, "a!%b")

    assert _pattern(sql.asked[0][1]) == "%a!!!%b%", _pattern(sql.asked[0][1])


def test_a_search_for_nothing_is_still_a_bounded_search() -> None:
    """An empty box is a person who has not typed yet, not a request for all
    46096 rows -- and `%%` is exactly that request."""
    sql = _Reader("")

    play.find_items(sql, WOTLK, "")

    assert "LIMIT" in sql.asked[0][1]


def test_an_item_search_asks_for_a_bounded_number_of_rows() -> None:
    """46096 items on this install. A tab that asked for all of them would
    freeze on the answer, not on the query."""
    sql = _Reader("")

    play.find_items(sql, WOTLK, "sword")

    assert "LIMIT" in sql.asked[0][1]


# -- the equipped set -------------------------------------------------------


def test_a_gear_set_on_this_tree_joins_the_item_instance_to_get_its_id() -> None:
    """The per-tree fact this box was warned about: AzerothCore's
    `character_inventory` row carries `item`, which is the INSTANCE guid, and
    the template id lives in `item_instance.itemEntry`. A query that read
    `character_inventory` alone would answer a list of guids that look like item
    ids — every one of them wrong, and none of them empty.
    """
    sql = _Reader("6948\n2589\n")

    worn = play.equipped(sql, WOTLK, "Guglu")

    assert worn == (6948, 2589), "item ids, because equipped gear does not stack"
    statement = sql.asked[0][1]
    # The COLUMN THAT IS SELECTED has to come from the joined table, not from
    # the inventory row: `SELECT ci.itemEntry` mentions both `item_instance`
    # and `itemEntry` and is still wrong, and a mutation to exactly that
    # survived the earlier version of this test.
    selected = statement.split("SELECT ", 1)[1].split(" FROM", 1)[0].strip()
    joined_alias = statement.split(" ON ", 1)[0].rsplit(" ", 1)[-1].strip()
    assert selected == f"{joined_alias}.itemEntry", f"{selected!r} from {statement!r}"
    assert "item_instance" in statement, statement


def test_a_gear_set_reads_only_the_equipped_slots() -> None:
    """Bags and the bank are not "what this character is wearing"; equipped
    items are the ones in bag 0, slots 0 to 18."""
    sql = _Reader("")

    play.equipped(sql, WOTLK, "Guglu")

    statement = sql.asked[0][1]
    assert "bag = 0" in statement, statement
    assert "slot" in statement


def test_the_gear_set_query_is_this_trees_and_not_the_other_ones() -> None:
    """TBC's own `character_inventory` carries the template id as a column of
    the inventory row, measured 2026-09-06, so that tree needs one join where
    this one needs two. The shape comes from the catalog rather than from an
    `if` in this function."""
    wotlk_sql, tbc_sql = _Reader(""), _Reader("")

    play.equipped(wotlk_sql, WOTLK, "Guglu")
    play.equipped(tbc_sql, TBC, "Guglu")

    assert "item_instance" in wotlk_sql.asked[0][1]
    assert "item_instance" not in tbc_sql.asked[0][1], tbc_sql.asked[0][1]


def test_a_tree_that_has_not_measured_its_gear_refuses_rather_than_guesses() -> None:
    """Vanilla and Tortoise have no Play block yet — their own boxes measure it.

    Guessing the WotLK shape there would answer a list of instance guids that
    look exactly like item ids, so the read refuses and names the game.
    """
    import pytest

    vanilla = load_catalog().get("wow-vanilla")

    with pytest.raises(play.NotMeasured) as refused:
        play.equipped(_Reader(""), vanilla, "Guglu")

    assert "WoW Vanilla" in str(refused.value)
