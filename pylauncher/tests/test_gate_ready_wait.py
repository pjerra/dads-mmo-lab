"""Tests for `pyplan/gates/ready_wait.py`: the ready wait every gate script shares.

**WHY A GATE SCRIPT HAS TESTS AT ALL.** `pyplan/gates/` is normally records --
scripts that ran once, beside what they measured. This one is not: it is the
spelling three gate scripts import, and the reason it exists is that two of them
had the WotLK row wrong at the same time and only one of them was ever run
(`pyplan/gates/7.10-rerun-ubuntu2-2026-09-08/README.md` section 2). A wait that
can never return costs a re-run the whole of its budget -- 8 m 19 s before it was
killed by hand, `MANAGEMENT_FLOOR_SECONDS` unattended -- and then reports the
server as never having come up. That is a defect with a cheap unit test and an
expensive live one.

**Driven through the call, never through the source text.** Every assertion here
reads what `wait_server_ready()` was CALLED with, via a fake module that records
its arguments. A test that grepped the table for `127.0.0.1` would pass the moment
the same address arrived from a variable (`audit-by-argv-not-by-string`).
"""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path
from typing import Any

import pytest

from yulon.catalog.catalog import load_catalog

GATE_MODULE = Path(__file__).resolve().parents[2] / "pyplan" / "gates" / "ready_wait.py"
"""The module under test, addressed by path.

`pyplan/` is not a package and is not on the path in a normal run -- the gate
scripts add their own directory. Loading it by path is what lets this test exist
without making `pyplan` importable for everything else.
"""

SERVER_DIR = Path("/nonexistent/server/dir")
"""A path no test reaches. `wow-wotlk`'s password is `fixed`, so nothing reads it."""


def _ready_wait() -> Any:
    spec = importlib.util.spec_from_file_location("gate_ready_wait", GATE_MODULE)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


@pytest.fixture
def ready_wait() -> Any:
    return _ready_wait()


class _FakeDockerCtl:
    """A stand-in for a game's `docker_ctl`, recording how its ready wait was called."""

    def __init__(self) -> None:
        self.calls: list[tuple[tuple[object, ...], dict[str, object]]] = []

    def wait_server_ready(self, *args: object, **kwargs: object) -> bool:
        self.calls.append((args, kwargs))
        return True


def test_the_wotlk_row_waits_for_the_realm_rows_own_pair_not_a_typed_one(
    ready_wait: Any, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The 7.10 defect, asserted at the seam it went wrong at.

    Measured on `yulon-ubuntu2` 2026-09-09, both calls against the same live
    server in the same minute
    (`pyplan/gates/7.10-rerun-ubuntu2-2026-09-08/ready-marker-probe.txt`):
    `wait_server_ready("127.0.0.1", 3724)` was killed still waiting after
    8 m 19 s, and `wait_server_ready("172.30.48.189", 8085)` answered
    `ready=True` in 0.3 s. `azerothcore_ready()` waits for `<host>:<port>` in the
    authserver's log, and the authserver prints the address and port
    `acore_auth.realmlist` holds -- so the arguments ARE the marker for this row
    and for no other.

    Mutation this catches: the row this replaced,
    `module.wait_server_ready("127.0.0.1", args.auth_port)`. The recorded call is
    then `("127.0.0.1", 3724)` and the assertion below names both halves of what
    is wrong with it -- the address AND the port, which is the auth port where
    the marker wants the world one.

    Driven through `wait_ready_for_game()` rather than through the row alone, so
    the module it imports is asserted too: that name is built by string
    substitution from the catalog id, and it is the step the 2026-09-06 `else`
    branch got wrong for this exact game.
    """
    entry = load_catalog().get("wow-wotlk")
    fake = _FakeDockerCtl()
    imported: list[str] = []

    def import_module(name: str) -> Any:
        imported.append(name)
        return fake

    monkeypatch.setattr(ready_wait.importlib, "import_module", import_module)

    assert (
        ready_wait.wait_ready_for_game(entry, SERVER_DIR, pair=lambda: ("172.30.48.189", 8085))
        is True
    )
    assert imported == ["yulon.controller_wow_wotlk.docker_ctl"]
    assert fake.calls == [(("172.30.48.189", 8085), {})], fake.calls


def test_the_three_cmangos_rows_ask_for_no_realm_row(ready_wait: Any) -> None:
    """Only the row whose marker names an address may cost a SELECT.

    TBC and Vanilla take no realm arguments -- their `wait_server_ready()` raises
    `TypeError` on anything but timeout/interval, and Vanilla accepts no port at
    all. Tortoise's signature requires both and neither reaches a marker: that
    entry declares `ready.auth: null` and its `regex: true` world marker names no
    `{{TOKEN}}` (read from `catalog.json`, 2026-09-09).

    So the pair is a thunk, and this asserts it is never pulled for those three.
    A row that read the realm row anyway would send a statement whose answer
    nothing uses, and a database that would not answer would fail a gate over a
    value its wait never looks at.

    Mutation this catches: giving any of the three rows `args.realm_pair()`. The
    thunk raises, saying which row asked.
    """

    def refuse() -> tuple[str, int]:
        raise AssertionError("this row must not read the realm row")

    args = ready_wait.ReadyArgs(auth_port=3724, realm_pair=refuse)
    for game, expected in (
        ("wow-tbc", ((), {})),
        ("wow-vanilla", ((), {})),
        ("wow-tortoise", (("127.0.0.1", 3724), {})),
    ):
        fake = _FakeDockerCtl()
        ready_wait.READY_CALLS[game](fake, args)
        assert fake.calls == [expected], (game, fake.calls)


def test_the_query_is_built_from_the_entry_and_names_the_port_column(ready_wait: Any) -> None:
    """The SELECT is the install's own facts, not four literals.

    Every part of it comes from the catalog -- `databases.auth`,
    `realmlist.table`, `realmlist.address_column`, `realmlist.realm_id` -- which
    are the same facts `networking.realmlist_sql()` WRITES that row with. Only the
    port column is named by the gate, because `catalog.Realmlist` declares no
    port column: the app never writes one. That silence is recorded on
    `REALM_PORT_COLUMN` rather than decided quietly.

    Mutation this catches: hardcoding `acore_auth.realmlist` (the probe's own
    spelling). The statement is then identical for `wow-tortoise`, whose auth
    schema is `tw_logon`, and the second assertion fails.
    """
    wotlk = load_catalog().get("wow-wotlk")
    assert (
        ready_wait.realm_address_query(wotlk)
        == "SELECT address, port FROM acore_auth.realmlist WHERE id=1;"
    )
    tortoise = load_catalog().get("wow-tortoise")
    assert (
        ready_wait.realm_address_query(tortoise)
        == "SELECT address, port FROM tw_logon.realmlist WHERE id=1;"
    )


def test_the_realm_pair_is_read_through_the_apps_own_seam_with_no_secret_in_the_argv(
    ready_wait: Any,
) -> None:
    """The container, the client and the password go to `docker.sql_query()` as arguments.

    Which is what keeps the password out of every argv a gate log could catch
    (`gate-logs-carry-generated-passwords`): that seam puts the statement on stdin
    and the password in `MYSQL_PWD`. The client name is data for the same reason
    it is data in the app -- this row does not know whether the image ships
    `mysql` or `mariadb`.

    Mutation this catches: parsing the two fields in the wrong order (the pair
    comes back `(8085, "172.30.48.189")`-shaped and the assertion names it), and
    forgetting `int()` on the port, which reaches `azerothcore_ready()` as a
    string and builds a marker that still matches -- so it is asserted by type
    here rather than left to the live run.
    """
    entry = load_catalog().get("wow-wotlk")
    seen: list[tuple[object, ...]] = []

    def query(container: str, client: str, password: str, schema: object, statement: str) -> str:
        seen.append((container, client, password, schema, statement))
        return "172.30.48.189\t8085\n"

    assert ready_wait.realm_pair(entry, SERVER_DIR, query=query) == ("172.30.48.189", 8085)
    assert len(seen) == 1
    container, client, password, schema, statement = seen[0]
    assert container == "ac-database"
    assert client == "mysql"
    assert password == entry.install.db_password(SERVER_DIR)
    assert schema is None
    assert statement == ready_wait.realm_address_query(entry)


@pytest.mark.parametrize(
    ("answer", "expected"),
    [
        ("", "answered 0 rows"),
        ("a\t1\nb\t2\n", "answered 2 rows"),
        ("172.30.48.189\n", "answered 1 fields"),
        ("\t8085\n", "is empty"),
        ("172.30.48.189\tnot-a-port\n", "is not a port number"),
    ],
)
def test_an_answer_that_is_not_one_realm_row_is_refused_and_never_defaulted(
    ready_wait: Any, answer: str, expected: str
) -> None:
    """Every unusable answer raises, and none of them becomes a plausible pair.

    A DEFAULT IS THE DEFECT. The typed `("127.0.0.1", auth_port)` was exactly a
    plausible pair standing in for an unknown one, and what it produced was
    8 m 19 s of waiting and then a report that the server never came up. So the
    reader refuses with the reason, the gate section books it as a FAIL, and
    nobody reads six hours of nothing as a measurement.

    Mutation this catches: any `except RealmRowError: return ("127.0.0.1", port)`
    added later, and a validation loosened to `len(rows) >= 1` (the two-row case
    then passes and the pair is whichever row sorted first).
    """
    with pytest.raises(ready_wait.RealmRowError, match=expected):
        ready_wait.realm_pair(
            load_catalog().get("wow-wotlk"),
            SERVER_DIR,
            query=lambda *_: answer,
        )


def test_a_failing_query_is_refused_with_the_statement_in_the_message(ready_wait: Any) -> None:
    """A database that will not answer must not read as a server that is not up.

    Mutation this catches: letting the client's own exception out. `run-710`'s
    section then reports `DockerCommandError: ...` with no statement in it, and
    the next reader cannot tell a wrong column name from a stopped container.
    """

    def refuse(*_: object) -> str:
        raise RuntimeError("Unknown column 'port' in 'field list'")

    with pytest.raises(ready_wait.RealmRowError, match="Unknown column"):
        ready_wait.realm_pair(load_catalog().get("wow-wotlk"), SERVER_DIR, query=refuse)


def test_an_id_with_no_row_stops_the_run_and_names_the_ones_that_have_one(
    ready_wait: Any,
) -> None:
    """The refusal `gate-79`'s table was given on 2026-09-06, kept when it moved here.

    A chain of `if`s ending in an `else` had `wow-wotlk` landing in Tortoise's
    branch, which went unnoticed only because the two take the same two
    arguments. An id with no row must stop the run: half a gate reads like a
    whole one.
    """
    entry = load_catalog().get("wow-wotlk")
    with pytest.raises(ready_wait.UnknownGameError, match="wow-tbc, wow-tortoise"):
        raise ready_wait.unknown_game_error("wow-classic-plus")
    assert entry.id in ready_wait.READY_CALLS
