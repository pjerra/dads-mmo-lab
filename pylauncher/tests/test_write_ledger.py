"""The write ledger and the walk that enforces it, in both directions (8.1a).

Grafted from design C, which the maintainer judge called the single best test
idea in the three designs: an `ast` enumeration of every place this package
writes, checked against a table a person maintains, failing **both** when a
write is not in the table and when a table row names a write that no longer
exists. One direction alone rots — the first lets a new write appear unnoticed,
the second lets the table fill with lines about code that is gone.

Why it lands in 8.1a rather than at the exit box: this is the step that adds the
first new write sites (the log snapshot), and a ledger written after the writes
is a ledger written from the code rather than from the intent.

The walker's own tests come first here, because a walker that misses a call
makes every row below it a false assurance — the failure mode this project has
already paid for once, recorded in `guards-that-prove-declarations`.
"""

from __future__ import annotations

import ast
from pathlib import Path

from tests.write_sites import find_write_sites, parse_ledger, walk_tree

PACKAGE = Path(__file__).resolve().parents[1] / "yulon"
LEDGER = Path(__file__).resolve().parents[2] / "pyplan" / "write-ledger.md"


def _sites_in(source: str) -> set[str]:
    """Every write site the walker finds in one snippet, as `function::callee`."""
    tree = ast.parse(source)
    return {f"{site.function}::{site.callee}" for site in find_write_sites_in(tree)}


def find_write_sites_in(tree: ast.AST) -> list:
    return walk_tree(tree, module="snippet.py")


# -- the walker ------------------------------------------------------------


def test_a_path_opened_for_writing_is_a_write_even_though_open_reads_too() -> None:
    """`partial.open("wb")` is how `logsnap` writes; a mode-blind walk would miss it."""
    assert _sites_in("def f(p):\n    with p.open('wb') as fh:\n        fh.write(b'x')\n") == {
        "f::open(wb)"
    }


def test_a_path_opened_for_reading_is_not_a_write() -> None:
    assert _sites_in("def f(p):\n    with p.open('rb') as fh:\n        fh.read()\n") == set()


def test_replacing_text_in_a_string_is_not_a_write_to_anything() -> None:
    """`str.replace` outnumbers `os.replace` four to one in this package."""
    assert _sites_in("def f(s):\n    return s.replace('a', 'b')\n") == set()


def test_replacing_a_file_is_a_write() -> None:
    assert _sites_in("import os\ndef f(a, b):\n    os.replace(a, b)\n") == {"f::os.replace"}


def test_the_sql_write_seam_is_a_write_and_the_read_seam_is_not() -> None:
    """`query()` is what `dbreads` is allowed to call; the other two are not."""
    source = (
        "def f(sql):\n"
        "    sql.query('characters', 'SELECT 1')\n"
        "    sql.run_statement('auth', 'UPDATE account SET x = 1')\n"
    )
    assert _sites_in(source) == {"f::run_statement"}


def test_renaming_a_path_over_another_is_a_write_even_spelt_as_a_bare_replace() -> None:
    """`tmp.replace(target)` is how `state.py` saves atomically, and it is not `os.replace`.

    Seven of these exist in the package and the first version of this walker
    missed every one of them, because it excluded bare `.replace(` to keep
    `str.replace` out. One positional argument and no keywords is what separates
    them: `str.replace` needs two, and `datetime.replace(tzinfo=…)` passes none.
    """
    assert _sites_in("def f(tmp, target):\n    tmp.replace(target)\n") == {"f::replace"}


def test_a_datetime_replacing_one_of_its_own_fields_is_not_a_write() -> None:
    source = "def f(t, tz):\n    return t.replace(tzinfo=tz)\n"
    assert _sites_in(source) == set()


def test_deleting_is_a_write_because_the_ledger_is_about_what_can_be_lost() -> None:
    source = "import shutil\ndef f(p):\n    p.unlink()\n    shutil.rmtree(p)\n"
    assert _sites_in(source) == {"f::unlink", "f::shutil.rmtree"}


def test_a_write_inside_a_nested_function_is_attributed_to_that_function() -> None:
    """Otherwise two writes in one module collapse onto one ledger row."""
    source = "def outer():\n    def inner(p):\n        p.write_text('x')\n    return inner\n"
    assert _sites_in(source) == {"inner::write_text"}


# -- the ledger, both directions -------------------------------------------


def test_every_write_in_the_package_is_in_the_ledger() -> None:
    """A new write site is a decision; this is where it stops being a silent one."""
    found = {site.key for site in find_write_sites(PACKAGE)}
    listed = {row.site for row in parse_ledger(LEDGER)}
    missing = sorted(found - listed)
    assert not missing, (
        "these writes are not in pyplan/write-ledger.md:\n  "
        + "\n  ".join(missing)
        + "\nAdd a row saying what each writes and whether the world may be running."
    )


def test_every_ledger_row_still_resolves_to_a_write_in_the_package() -> None:
    """The direction that keeps the table honest when code is deleted."""
    found = {site.key for site in find_write_sites(PACKAGE)}
    listed = {row.site for row in parse_ledger(LEDGER)}
    stale = sorted(listed - found)
    assert not stale, "pyplan/write-ledger.md names writes that no longer exist:\n  " + "\n  ".join(
        stale
    )


def test_every_ledger_row_says_what_it_writes_and_whether_the_world_may_be_up() -> None:
    """A row with an empty cell is a row nobody filled in."""
    for row in parse_ledger(LEDGER):
        assert row.writes, f"{row.site} does not say what it writes"
        assert row.running, f"{row.site} does not say whether the world may be running"


def test_dbreads_appears_in_the_ledger_only_if_it_ever_writes() -> None:
    """Owner answer 7: the module that reads `characters` must never write it.

    Asserted over the syntax tree rather than by reading the module, because
    "it does not call that" is exactly the kind of claim that stops being true
    without anybody noticing.
    """
    writes = [site for site in find_write_sites(PACKAGE) if site.module == "dbreads.py"]
    assert writes == [], f"dbreads.py writes: {[s.key for s in writes]}"
