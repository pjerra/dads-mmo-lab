"""CMaNGOS's four seeded accounts are locked before a TBC or Vanilla server starts (T668).

`realmd.sql` ships ADMINISTRATOR (GM 3), GAMEMASTER (2), MODERATOR (1) and PLAYER (0), each
with password = username, and the auth port is open to the network. These tests go through
the paths that reach a start -- `seeded_accounts.settle()` itself, `Controller.start()`, the
install's `up` and a rebuild's recreate -- over a fake `docker.sql_query` that is a small
stateful `realmd.account` table: it answers the SELECT from its rows and applies an UPDATE
only if the UPDATE's own WHERE still matches, as MySQL would. So a second run is answered by
what the first one wrote, and a repeated change shows as a second UPDATE.
"""

from __future__ import annotations

import logging
import re
from pathlib import Path

import pytest

from tests.support_native import Recorder
from tests.test_families_cmangos import context as cmangos_context
from tests.test_families_cmangos import engine as cmangos_engine
from yulon import docker, seeded_accounts
from yulon.catalog.catalog import CatalogEntry, load_catalog
from yulon.controller import Controller, PortConflictError
from yulon.controller_wow_wotlk.accounts import mangos_srp6_credentials

CATALOG = load_catalog()
TBC = CATALOG.get("wow-tbc")
VANILLA = CATALOG.get("wow-vanilla")
TORTOISE = CATALOG.get("wow-tortoise")
WOTLK = CATALOG.get("wow-wotlk")
PASSWORD = "tbc-0123456789abcdef"
SEEDED = ("ADMINISTRATOR", "GAMEMASTER", "MODERATOR", "PLAYER")

# The two pairs the live TBC and Vanilla servers both shipped (tests/test_accounts.py pins
# them); GAMEMASTER and MODERATOR are made with the same helper from a fixed salt.
_MEASURED = {
    "ADMINISTRATOR": (
        "8EB5DE915AA3D805FA7099CF61C0BB8A77990EA869078A0C5B9EEE55828F4505",
        "312B99EEF1C0196BB73B79D114CE161C5D089319E6EF54FAA6117DAB8B672C14",
    ),
    "PLAYER": (
        "EBA23AF194D89B8061CA7FEBA06D336B1C38D8FBDABA76F2C51D45141362D881",
        "3738EC7E7C731FD431C716990C6D97CA5C1D50EF0DA7DE9819076DE1D03AA891",
    ),
}


def _credentials(name: str, password: str, salt_byte: int) -> tuple[str, str]:
    return mangos_srp6_credentials(name, password, salt=bytes([salt_byte]) * 32)


def seeded_rows() -> dict[str, dict[str, object]]:
    rows: dict[str, dict[str, object]] = {}
    for index, (name, gm) in enumerate(zip(SEEDED, (3, 2, 1, 0), strict=True), start=1):
        s, v = _MEASURED[name] if name in _MEASURED else _credentials(name, name, 0x40 + index)
        rows[name] = {"id": index, "s": s, "v": v, "gmlevel": gm}
    return rows


def _decoded(statement: str) -> list[str]:
    return [bytes.fromhex(h).decode() for h in re.findall(r"X'([0-9A-Fa-f]*)'", statement)]


class AuthDb:
    """`realmd.account` as `docker.sql_query` sees it: statements in, rows out."""

    def __init__(self, rows: dict[str, dict[str, object]] | None = None) -> None:
        self.rows = rows if rows is not None else seeded_rows()
        self.statements: list[str] = []
        self.fail_with: Exception | None = None
        self.before_update: object = None
        self.databases_started = 0

    def add_player(self, name: str, password: str, gm: int = 0) -> None:
        s, v = _credentials(name, password, 0x70 + len(self.rows))
        self.rows[name] = {"id": len(self.rows) + 1, "s": s, "v": v, "gmlevel": gm}

    def sql_query(
        self,
        container: str,
        client: str,
        password: str,
        schema: str | None,
        statement: str,
        *,
        wsl_distro: str | None = None,
    ) -> str:
        assert password == PASSWORD, "root's password comes from the install's own file"
        self.statements.append(statement)
        if self.fail_with is not None:
            raise self.fail_with
        if statement.startswith("SELECT"):
            assert re.fullmatch(
                r"SELECT username, s, v FROM \w+\.account WHERE username IN \(.*\);?",
                statement,
            ), statement
            wanted = {n.upper() for n in _decoded(statement)}
            out = [
                f"{name}\t{row['s']}\t{row['v']}\n"
                for name, row in self.rows.items()
                if name.upper() in wanted
            ]
            if callable(self.before_update):
                self.before_update()
            return "".join(out)
        match = re.fullmatch(
            r"UPDATE \w+\.account SET v = _utf8mb4 X'(?P<v>[0-9A-F]+)',"
            r" s = _utf8mb4 X'(?P<s>[0-9A-F]+)'"
            r" WHERE username = _utf8mb4 X'(?P<name>[0-9A-F]+)'"
            r" AND v = _utf8mb4 X'(?P<oldv>[0-9A-F]+)'"
            r" AND s = _utf8mb4 X'(?P<olds>[0-9A-F]+)';?",
            statement,
        )
        assert match is not None, f"a statement this table does not model: {statement}"
        text = {k: bytes.fromhex(g).decode() for k, g in match.groupdict().items()}
        row = next((r for n, r in self.rows.items() if n.upper() == text["name"].upper()), None)
        if row is not None and row["v"] == text["oldv"] and row["s"] == text["olds"]:
            row["v"], row["s"] = text["v"], text["s"]
        return ""

    def updates(self) -> list[str]:
        return [s for s in self.statements if s.startswith("UPDATE")]

    def updated_names(self) -> list[str]:
        return [_decoded(s)[2] for s in self.updates()]

    def seeded_state(self, name: str) -> bool:
        row = self.rows[name]
        s = str(row["s"])
        raw = bytes.fromhex(s)[::-1]
        return row["v"] == mangos_srp6_credentials(name, name, salt=raw)[1]


@pytest.fixture
def db(monkeypatch: pytest.MonkeyPatch) -> AuthDb:
    fake = AuthDb()
    monkeypatch.setattr(docker, "sql_query", fake.sql_query)

    def start_database(*_a: object, **_k: object) -> bool:
        fake.databases_started += 1
        return True

    monkeypatch.setattr(docker, "start_database", start_database)
    monkeypatch.setattr(docker, "status", lambda **_k: [])
    return fake


def an_install(tmp_path: Path, entry: CatalogEntry = TBC) -> Path:
    (tmp_path / (entry.install.password.file or ".db_password")).write_text(PASSWORD)
    return tmp_path


def settle(tmp_path: Path, entry: CatalogEntry = TBC) -> str | None:
    return seeded_accounts.settle(entry, entry.container_spec(), an_install(tmp_path, entry))


# -- the rows are changed ---------------------------------------------------------------


@pytest.mark.parametrize("entry", [TBC, VANILLA], ids=lambda e: e.id)
def test_the_four_seeded_accounts_stop_taking_their_own_name_as_the_password(
    db: AuthDb, tmp_path: Path, entry: CatalogEntry
) -> None:
    before = {n: dict(r) for n, r in db.rows.items()}
    assert all(db.seeded_state(n) for n in SEEDED), "the fixture starts seeded"

    said = settle(tmp_path, entry)

    assert not any(db.seeded_state(n) for n in SEEDED)
    for name in SEEDED:
        assert db.rows[name]["v"] != before[name]["v"] and db.rows[name]["s"] != before[name]["s"]
        assert db.rows[name]["id"] == before[name]["id"], "the row is kept, not deleted"
        assert db.rows[name]["gmlevel"] == before[name]["gmlevel"]
        assert re.fullmatch(r"[0-9A-F]{64}", str(db.rows[name]["v"]))
        assert re.fullmatch(r"[0-9A-F]{64}", str(db.rows[name]["s"]))
    assert said is not None
    assert all(name in said for name in SEEDED)
    assert "ADMINISTRATOR" in said and "password" in said.lower()


def test_each_account_gets_its_own_random_password(db: AuthDb, tmp_path: Path) -> None:
    settle(tmp_path)
    first = {n: (db.rows[n]["s"], db.rows[n]["v"]) for n in SEEDED}
    assert len({s for s, _v in first.values()}) == 4 and len({v for _s, v in first.values()}) == 4
    db.rows = seeded_rows()  # the same seeded rows again: a new draw, not the same one
    settle(tmp_path)
    assert {n: (db.rows[n]["s"], db.rows[n]["v"]) for n in SEEDED} != first


def test_the_statements_carry_hex_literals_and_no_password(db: AuthDb, tmp_path: Path) -> None:
    settle(tmp_path)
    assert len(db.updates()) == 4
    for statement in db.updates():
        assert re.fullmatch(
            r"UPDATE realmd\.account SET v = _utf8mb4 X'[0-9A-F]+', s = _utf8mb4 X'[0-9A-F]+'"
            r" WHERE username = _utf8mb4 X'[0-9A-F]+' AND v = _utf8mb4 X'[0-9A-F]+'"
            r" AND s = _utf8mb4 X'[0-9A-F]+';",
            statement,
        ), statement
        assert "'ADMINISTRATOR'" not in statement and "PLAYER'" not in statement
    assert any(s.startswith("SELECT") for s in db.statements)
    assert all(s.startswith(("SELECT", "UPDATE")) for s in db.statements), "never a DELETE"


# -- a row that is no longer seeded is left alone -----------------------------------------


def test_an_administrator_whose_password_was_changed_keeps_it(db: AuthDb, tmp_path: Path) -> None:
    s, v = _credentials("ADMINISTRATOR", "my-own-long-secret", 0x55)
    db.rows["ADMINISTRATOR"]["s"], db.rows["ADMINISTRATOR"]["v"] = s, v

    said = settle(tmp_path)

    assert (db.rows["ADMINISTRATOR"]["s"], db.rows["ADMINISTRATOR"]["v"]) == (s, v)
    assert "ADMINISTRATOR" not in db.updated_names()
    assert sorted(db.updated_names()) == ["GAMEMASTER", "MODERATOR", "PLAYER"]
    assert said is not None and "ADMINISTRATOR" not in said


def test_a_row_with_the_seeded_verifier_but_another_salt_is_not_the_seeded_row(
    db: AuthDb, tmp_path: Path
) -> None:
    """The stored v is judged against the row's OWN salt, not against a pinned pair."""
    db.rows["GAMEMASTER"]["v"] = _credentials("GAMEMASTER", "GAMEMASTER", 0x41)[1]
    db.rows["GAMEMASTER"]["s"] = _credentials("GAMEMASTER", "GAMEMASTER", 0x99)[0]
    settle(tmp_path)
    assert "GAMEMASTER" not in db.updated_names()


def test_a_row_without_a_verifier_is_left_alone(db: AuthDb, tmp_path: Path) -> None:
    db.rows["MODERATOR"]["v"] = "NULL"
    db.rows["MODERATOR"]["s"] = ""
    settle(tmp_path)
    assert "MODERATOR" not in db.updated_names()


def test_a_password_changed_between_the_read_and_the_write_is_not_overwritten(
    db: AuthDb, tmp_path: Path
) -> None:
    """The UPDATE's own WHERE names the verifier it read, so a late change wins."""

    def player_changes_it() -> None:
        s, v = _credentials("ADMINISTRATOR", "changed-in-the-meantime", 0x56)
        db.rows["ADMINISTRATOR"]["s"], db.rows["ADMINISTRATOR"]["v"] = s, v
        db.before_update = None

    db.before_update = player_changes_it
    settle(tmp_path)
    s, v = _credentials("ADMINISTRATOR", "changed-in-the-meantime", 0x56)
    assert (db.rows["ADMINISTRATOR"]["s"], db.rows["ADMINISTRATOR"]["v"]) == (s, v)


# -- no other account is touched ---------------------------------------------------------


def test_real_player_accounts_are_never_touched(db: AuthDb, tmp_path: Path) -> None:
    db.add_player("ALICE", "hunter22")
    db.add_player("BOBBY", "BOBBY")  # a weak password of his own is still his account
    db.add_player("YULON_CHANNEL", "x" * 12, gm=3)
    db.add_player("RNDBOT1", "RNDBOT1")
    before = {n: dict(db.rows[n]) for n in ("ALICE", "BOBBY", "YULON_CHANNEL", "RNDBOT1")}

    settle(tmp_path)

    for name, row in before.items():
        assert db.rows[name] == row, name
    assert set(db.updated_names()) == set(SEEDED)
    queried = [n for s in db.statements if s.startswith("SELECT") for n in _decoded(s)]
    assert set(queried) == set(SEEDED), "only the four names are even read"


def test_the_names_are_the_ones_the_catalog_leaves_out_of_the_player_count() -> None:
    for entry in (TBC, VANILLA):
        native = entry.install.native
        assert native is not None and native.cmangos is not None
        excluded = [
            tuple(item.exclude_usernames)
            for item in native.cmangos.sql.player_data
            if item.exclude_usernames
        ]
        assert excluded == [seeded_accounts.NAMES], entry.id
    assert seeded_accounts.NAMES == SEEDED


# -- running it twice does nothing the second time ------------------------------------------


def test_the_second_run_changes_nothing(db: AuthDb, tmp_path: Path) -> None:
    first = settle(tmp_path)
    after_first = {n: dict(r) for n, r in db.rows.items()}
    sent = len(db.updates())
    assert first is not None and sent == 4

    second = settle(tmp_path)

    assert second is None, "nothing locked, nothing said"
    assert len(db.updates()) == sent, "no UPDATE on the second run"
    assert db.rows == after_first, "the new random verifiers are not rolled again"


# -- engines it must leave alone ------------------------------------------------------------


@pytest.mark.parametrize("entry", [TORTOISE, WOTLK], ids=lambda e: e.id)
def test_other_cores_are_not_asked_anything(
    db: AuthDb, tmp_path: Path, entry: CatalogEntry
) -> None:
    assert settle(tmp_path, entry) is None
    assert db.statements == [] and db.databases_started == 0


# -- failure ---------------------------------------------------------------------------------


def test_a_database_that_cannot_be_asked_says_so_and_never_raises(
    db: AuthDb, tmp_path: Path
) -> None:
    db.fail_with = docker.DockerCommandError("container is not running")
    said = settle(tmp_path)
    assert said is not None and "could not" in said and "ADMINISTRATOR" in said
    assert "still" in said


def test_no_password_on_file_asks_nothing(db: AuthDb, tmp_path: Path) -> None:
    said = seeded_accounts.settle(TBC, TBC.container_spec(), tmp_path)
    assert said is None and db.statements == []


# -- the paths that reach a start -----------------------------------------------------------


def _controller_start(
    monkeypatch: pytest.MonkeyPatch, server_dir: Path, entry: CatalogEntry = TBC
) -> Controller:
    monkeypatch.setattr(docker, "start_staged", lambda *a, **k: True)
    controller = Controller(entry.container_spec(), server_dir)
    controller.entry = entry
    monkeypatch.setattr(controller, "port_conflicts", lambda: [])
    controller.start()
    return controller


@pytest.mark.parametrize("entry", [TBC, VANILLA], ids=lambda e: e.id)
def test_an_existing_install_is_repaired_by_its_next_start_and_the_start_says_so(
    db: AuthDb, tmp_path: Path, monkeypatch: pytest.MonkeyPatch, entry: CatalogEntry
) -> None:
    server_dir = an_install(tmp_path, entry)

    controller = _controller_start(monkeypatch, server_dir, entry)

    assert not any(db.seeded_state(n) for n in SEEDED)
    said = controller.seeded_accounts_locked
    assert said is not None and "ADMINISTRATOR" in said
    again = _controller_start(monkeypatch, server_dir, entry)
    assert again.seeded_accounts_locked is None
    assert len(db.updates()) == 4, "the second start rolled nothing"


def test_a_start_that_is_refused_locks_nothing(
    db: AuthDb, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    server_dir = an_install(tmp_path)
    monkeypatch.setattr(docker, "start_staged", lambda *a, **k: True)
    controller = Controller(TBC.container_spec(), server_dir)
    controller.entry = TBC
    monkeypatch.setattr(controller, "port_conflicts", lambda: ["3724"])
    monkeypatch.setattr(controller, "_owners_of", lambda conflicts: {})
    with pytest.raises(PortConflictError):
        controller.start()
    assert db.updates() == []


@pytest.mark.parametrize("entry", [TBC, VANILLA], ids=lambda e: e.id)
def test_the_installs_up_locks_them_before_the_servers_start(
    db: AuthDb, tmp_path: Path, entry: CatalogEntry
) -> None:
    """A new install: the import has just laid the seeded rows and `up` is the next stage."""
    order: list[str] = []
    started = Recorder()
    server_dir = an_install(tmp_path, entry)
    eng = cmangos_engine(
        started,
        entry=entry,
        lock_seeded_accounts=seeded_accounts.settle,
        start=lambda spec, where: order.append(f"start after {len(db.updates())} updates") or True,
    )

    said = list(eng.stage_up(cmangos_context(server_dir)))

    assert order == ["start after 4 updates"], "locked first, then the servers start"
    assert not any(db.seeded_state(n) for n in SEEDED)
    assert any("ADMINISTRATOR" in line for line in said), said


def test_a_rebuilds_recreate_locks_them_too(db: AuthDb, tmp_path: Path) -> None:
    server_dir = an_install(tmp_path)
    eng = cmangos_engine(
        Recorder(),
        docker_ready=lambda: True,
        recreate=lambda *a, **k: True,
        lock_seeded_accounts=seeded_accounts.settle,
    )
    said = list(eng.stage_recreate(cmangos_context(server_dir)))
    assert not any(db.seeded_state(n) for n in SEEDED)
    assert any("ADMINISTRATOR" in line for line in said), said


def test_the_shipped_seam_is_the_real_one() -> None:
    from yulon.catalog import native

    assert native.Seams.__dataclass_fields__["lock_seeded_accounts"].default is (
        seeded_accounts.settle
    )


def test_the_log_carries_no_verifier_or_salt(
    db: AuthDb, tmp_path: Path, caplog: pytest.LogCaptureFixture
) -> None:
    caplog.set_level(logging.DEBUG)
    settle(tmp_path)
    logged = caplog.text
    for name in SEEDED:
        assert str(db.rows[name]["v"]) not in logged and str(db.rows[name]["s"]) not in logged
