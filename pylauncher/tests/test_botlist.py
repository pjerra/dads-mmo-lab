"""Browsing this server's bots (8.5a).

The counts already exist — 8.1a's `population()` reports how many there are.
What this adds is the list itself: paged, filtered, and saying for each row
WHICH signal identified it as a bot, because on this tree there are two and they
disagree in the cases that matter.
"""

from __future__ import annotations

import pytest

from yulon import botlist, dbreads
from yulon.catalog.catalog import load_catalog

WOTLK = load_catalog().get("wow-wotlk")
MARKER = dbreads.Marker(prefix="rndbot", source="conf")


class _Reader:
    def __init__(self, *answers: str) -> None:
        self.answers = list(answers)
        self.asked: list[tuple[str, str]] = []

    def query(self, db: str, statement: str) -> str:
        self.asked.append((db, statement))
        return self.answers.pop(0) if self.answers else ""

    def run_statement(self, db: str, statement: str) -> None:  # pragma: no cover
        raise AssertionError("browsing must not write")


def test_a_page_is_the_rows_the_server_has_with_the_signal_that_found_each() -> None:
    sql = _Reader("2\t1\t1", "Guglu\t14\t1\tregistry\nRitdy\t3\t0\tprefix\n")

    page = botlist.page(sql, WOTLK, MARKER)

    assert [(b.name, b.level, b.online, b.source) for b in page.bots] == [
        ("Guglu", 14, True, "registry"),
        ("Ritdy", 3, False, "prefix"),
    ]
    assert page.total == 2
    assert page.problem == ""


def test_the_split_by_signal_is_shown_because_the_two_can_disagree() -> None:
    """The registry and the name prefix are two answers to one question.

    An install whose prefix was changed after its bots were made has rows the
    registry knows about and the prefix does not; showing one number hides
    exactly that.
    """
    sql = _Reader("500\t480\t20", "")

    page = botlist.page(sql, WOTLK, MARKER)

    assert page.total == 500
    assert page.by_registry == 480
    assert page.by_prefix == 20


def test_a_marker_that_could_not_be_read_refuses_rather_than_reporting_none() -> None:
    """8.1a's rule, on a second surface.

    Zero bots and "the marker could not be read" look identical in a list, and
    on this server one of them is never true.
    """
    page = botlist.page(_Reader(), WOTLK, dbreads.Marker(prefix="", source="conf"))

    assert page.bots == []
    assert page.total is None
    assert "marker" in page.problem


def test_a_marker_matching_nothing_while_characters_exist_warns() -> None:
    sql = _Reader("0\t0\t0", "", "900")

    page = botlist.page(sql, WOTLK, MARKER)

    assert page.total == 0
    assert "900" in page.warning


def test_a_page_asks_for_one_page_and_not_the_whole_table() -> None:
    """500 bots today and the owner has said the number is his to raise."""
    sql = _Reader("500\t500\t0", "")

    botlist.page(sql, WOTLK, MARKER, offset=40, limit=20)

    rows = sql.asked[1][1]
    assert "LIMIT 20" in rows
    assert "OFFSET 40" in rows


def test_the_filter_is_escaped_with_a_character_a_stricter_sql_mode_keeps() -> None:
    """A backslash is not a safe escape here.

    `NO_BACKSLASH_ESCAPES` is a real MySQL mode and a real MariaDB one, and
    under it a backslash is an ordinary character — so a filter escaped with
    one stops escaping and starts matching. The wildcards a person can type are
    `%` and `_`, and a name with an apostrophe is a name, not an injection.
    """
    sql = _Reader("1\t1\t0", "")

    botlist.page(sql, WOTLK, MARKER, name_like="10%_o'brien")

    rows = sql.asked[1][1]
    # The whole pattern, not a substring of it: asserting that "%" appears is
    # true whether or not it was escaped, which is how the first version of
    # this test survived a mutation that turned the escaping off.
    assert "LIKE '10!%!_o''brien%' ESCAPE '!'" in rows
    assert "\\" not in rows, "a backslash escape stops escaping under NO_BACKSLASH_ESCAPES"


def test_a_database_that_cannot_be_read_says_so() -> None:
    class _Broken(_Reader):
        def query(self, db: str, statement: str) -> str:
            raise RuntimeError("the database container is not running")

    page = botlist.page(_Broken(), WOTLK, MARKER)

    assert page.total is None
    assert "not running" in page.problem


def test_a_row_that_will_not_parse_fails_the_page_rather_than_vanishing() -> None:
    sql = _Reader("2\t2\t0", "Guglu\t14\t1\tregistry\nnonsense\n")

    page = botlist.page(sql, WOTLK, MARKER)

    assert page.bots == []
    assert "nonsense" in page.problem


def test_a_game_with_no_measured_marker_refuses() -> None:
    entry = load_catalog().get("wow-tortoise")
    if entry.observability is not None:  # pragma: no cover - measured since
        pytest.skip("wow-tortoise has been measured")

    page = botlist.page(_Reader(), entry, MARKER)

    assert page.total is None
    assert page.problem
