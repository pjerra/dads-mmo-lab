"""T107: a fresh Tortoise install keeps its character and account tables on InnoDB.

Upstream's `create_databases.sql` (tortoise-wow `1181dev` @187af788) creates 59 of `tw_char`'s 93
tables and 18 of `tw_logon`'s 41 as MyISAM, `characters` and `character_inventory` among them.
MyISAM has no crash recovery: a SIGKILL or a power cut loses rows or rolls a table back (T98's lab
measurement). So the import converts every MyISAM table the catalog's `convert_to_innodb` block
names, after the dumps and before `verify()` and the marker, and then asks again that none is
left. `tw_world` is not named: it is static content a re-import restores, and the core's own
world migrations re-create MyISAM tables at first start.

**The spelling is the load-bearing half.** 19 of the `tw_char` tables and 3 of the `tw_logon`
ones carry `ROW_FORMAT=FIXED`, which InnoDB does not have. With `innodb_strict_mode` on
(MariaDB's default, and `mariadb:10.6` runs with it) a bare `ALTER TABLE t ENGINE=InnoDB` keeps
that option and is refused. Measured on a test machine, 2026-09-27, `mariadb:10.6.28`:
`ERROR 1005 (HY000): Can't create table ... (errno: 140 "Wrong create options")` with the table
left MyISAM, while `ENGINE=InnoDB, ROW_FORMAT=DYNAMIC` converts it with no warning. The database
double below models exactly that, and
`test_the_double_refuses_the_bare_spelling_the_way_mariadb_does` checks that it does, so a change
that drops `ROW_FORMAT=DYNAMIC` fails here and not at a user's install.

Driven through the real `CmangosInstaller._import()` over the shipped `wow-tortoise` entry: the
seams are doubles, the engine, the plan and `sqlplan` are the real ones.
"""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
from collections.abc import Mapping, Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import BinaryIO

import pytest

from tests.support_native import ABSENT, Recorder
from yulon import resources
from yulon.catalog import composegen, native
from yulon.catalog.catalog import CatalogEntry, SqlPlan, load_catalog
from yulon.catalog.families import sqlplan
from yulon.catalog.families.cmangos import CmangosInstaller
from yulon.catalog.installer import InstallerError

TORTOISE = load_catalog().get("wow-tortoise")
_NATIVE = TORTOISE.install.native
assert _NATIVE is not None and _NATIVE.cmangos is not None, "wow-tortoise must be a cmangos entry"
PLAN = _NATIVE.cmangos.sql
DB = TORTOISE.databases
DB_PASSWORD = "tortoise-0123456789abcdef"

_ALTER = re.compile(r"^ALTER TABLE `(?P<schema>[^`]+)`\.`(?P<table>[^`]+)` (?P<options>.*);$")


def _one_line(statement: str) -> str:
    return " ".join(statement.split())


@dataclass
class InfoSchemaRecorder(Recorder):
    """The Recorder, plus a MariaDB 10.6 that answers `information_schema.TABLES` and `ALTER`.

    `tables` is schema -> table -> [engine, row_format]. Only what the conversion touches is
    modelled, and each part the way the server behaves:

    * `ALTER TABLE ... ENGINE=InnoDB` on a `FIXED` table WITHOUT a `ROW_FORMAT` of its own is
      refused with the measured error, and the table is left as it was (strict mode keeps the
      old create option, and InnoDB has no FIXED).
    * The client stops at the first refused statement and exits 1; the ones before it stay done.
    * `ignore_alters` makes every ALTER exit 0 and change nothing, for the check that asks again.
    * `refuse_table` makes the ALTER of that one table fail whatever it says.
    * `alter_raises` makes the script of ALTERs raise instead of exiting, as a pipe can.
    """

    tables: dict[str, dict[str, list[str]]] = field(default_factory=dict)
    ignore_alters: bool = False
    refuse_table: str | None = None
    alter_raises: OSError | None = None
    alters: list[str] = field(default_factory=list)

    def exec_stdin(
        self,
        container: str,
        argv: Sequence[str],
        source: BinaryIO,
        *,
        env: Mapping[str, str],
        wsl_distro: str | None = None,
    ) -> subprocess.CompletedProcess[str]:
        text = source.read().decode("utf-8")
        if self.alter_raises is not None and text.startswith("ALTER TABLE "):
            raise self.alter_raises
        done = super().exec_stdin(
            container, argv, _Replay(text.encode("utf-8")), env=env, wsl_distro=wsl_distro
        )
        if done.returncode != 0:
            return done
        for number, line in enumerate(text.splitlines(), start=1):
            match = _ALTER.match(line.strip())
            if match is None:
                continue
            self.alters.append(line.strip())
            schema, table, options = match["schema"], match["table"], match["options"]
            row = self.tables[schema][table]
            refused = table == self.refuse_table or (
                "ENGINE=InnoDB" in options and row[1] == "FIXED" and "ROW_FORMAT=" not in options
            )
            if refused:
                return subprocess.CompletedProcess(
                    list(argv),
                    1,
                    "",
                    f"ERROR 1005 (HY000) at line {number}: Can't create table `{schema}`.`{table}` "
                    '(errno: 140 "Wrong create options")\n',
                )
            if self.ignore_alters:
                continue
            if "ENGINE=InnoDB" in options:
                row[0] = "InnoDB"
            format_given = re.search(r"ROW_FORMAT=(\w+)", options)
            if format_given:
                row[1] = format_given.group(1)
        return done

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
        said = _one_line(statement)
        if "ENGINE = 'MyISAM'" not in said and "ENGINE <> 'InnoDB'" not in said:
            if said == "SELECT COUNT(*) FROM realmlist":
                # Tortoise's own verify rule. `Recorder` answers anything naming the realm
                # table with the realm guard's address row, which is not a count.
                statement_seen = super().sql_query(
                    container, client, password, schema, statement, wsl_distro=wsl_distro
                )
                assert statement_seen
                return "1\n"
            return super().sql_query(
                container, client, password, schema, statement, wsl_distro=wsl_distro
            )
        self.calls.append("query")
        self.sql_calls.append(statement)
        self.sql_secrets.append(password)
        self.distros.append(wsl_distro)
        named = re.search(r"TABLE_SCHEMA = '([^']+)'", said)
        assert named is not None, said
        assert "TABLE_TYPE = 'BASE TABLE'" in said, said
        rows = sorted(self.tables.get(named.group(1), {}).items())
        if "ENGINE = 'MyISAM'" in said:
            return "".join(f"{name}\n" for name, (engine, _) in rows if engine == "MyISAM")
        if "ENGINE <> 'InnoDB'" in said:
            left = [f"{name} ({engine})" for name, (engine, _) in rows if engine != "InnoDB"]
            return f"{len(left)}\t{', '.join(left)}\n"
        raise AssertionError(f"the double does not model this question: {said}")


class _Replay:
    """The bytes already read, handed on to `Recorder.exec_stdin()` for its own record."""

    def __init__(self, data: bytes) -> None:
        self._data = data

    def read(self) -> bytes:
        return self._data


def lab_tables() -> dict[str, dict[str, list[str]]]:
    """A cut of the pinned dump: each schema's engines and row formats, one of each kind.

    `auction` and `rbac_account_permissions` are two of the 22 `ROW_FORMAT=FIXED` MyISAM tables
    the conversion has to spell out; `characters` and `realmlist` are DYNAMIC MyISAM, which a
    bare ALTER would convert; `item_instance` and `account` are InnoDB already (COMPRESSED, as
    the dump has them) and must not be touched. `creature` is a FIXED MyISAM `tw_world` table,
    which the plan does not name and which stays as upstream ships it.
    """
    return {
        DB.characters: {
            "auction": ["MyISAM", "FIXED"],
            "characters": ["MyISAM", "DYNAMIC"],
            "item_instance": ["InnoDB", "COMPRESSED"],
        },
        DB.auth: {
            "account": ["InnoDB", "COMPRESSED"],
            "rbac_account_permissions": ["MyISAM", "FIXED"],
            "realmlist": ["MyISAM", "DYNAMIC"],
        },
        DB.world: {"creature": ["MyISAM", "FIXED"]},
        "tw_logs": {"character_loot": ["InnoDB", "COMPRESSED"]},
    }


def ready(**overrides: object) -> InfoSchemaRecorder:
    rec = InfoSchemaRecorder(tables=lab_tables(), **overrides)  # type: ignore[arg-type]
    rec.db_started = True
    rec.probe_answers = [ABSENT]
    return rec


def tortoise_engine(rec: Recorder, entry: CatalogEntry = TORTOISE) -> CmangosInstaller:
    eng = CmangosInstaller(
        entry,
        installers_root=resources.installers_dir(),
        seams=rec.seams(platform_id=lambda: "linux"),
    )
    eng._test_gate = native.CallableGate(rec.probe, rec.reset)  # type: ignore[attr-defined]
    return eng


def server_dir(tmp_path: Path, plan: SqlPlan = PLAN) -> Path:
    """Every file the plan's globs name, laid where the plan looks, one line naming itself."""
    root = tmp_path / "srv"
    for phase in plan.phases:
        for pattern in list(phase.files) + list((phase.into_each or {}).values()):
            names = [pattern]
            if "*" in pattern:
                names = [pattern.replace("*", f"{n:04d}") for n in (1, 2)]
            for name in names:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(f"-- {name}\nSELECT 1;\n", encoding="utf-8")
    return root


def context(root: Path, entry: CatalogEntry = TORTOISE) -> native.StageContext:
    return native.StageContext(
        server_dir=root,
        client_dir=None,
        state=native.InstallState(
            game_id=entry.id,
            install_id=composegen.install_id(root, platform_id=lambda: "linux"),
            family="cmangos",
            completed=(),
        ),
        cancel=None,
        secrets=native.Secrets(db_password=DB_PASSWORD),
    )


def run_import(rec: InfoSchemaRecorder, tmp_path: Path) -> list[str]:
    return list(tortoise_engine(rec)._import(context(server_dir(tmp_path))))


def marker_written(rec: Recorder) -> bool:
    return any(sqlplan.MARKER_TABLE in script for script in rec.sql_scripts)


# -- the catalog --------------------------------------------------------------------------


def test_the_tortoise_plan_converts_its_character_and_account_schemas_and_not_its_world() -> None:
    """The block names the entry's own characters and auth databases, and nothing else."""
    block = PLAN.convert_to_innodb
    assert block is not None, "wow-tortoise declares no convert_to_innodb block"
    assert set(block.schemas) == {DB.characters, DB.auth}, block.schemas
    assert DB.world not in block.schemas


@pytest.mark.parametrize("game", ["wow-tbc", "wow-vanilla"])
def test_no_other_entry_declares_a_conversion(game: str) -> None:
    """Scoped to Tortoise on purpose: the other games' schemas are not what T107 measured."""
    native_block = load_catalog().get(game).install.native
    assert native_block is not None and native_block.cmangos is not None, game
    assert native_block.cmangos.sql.convert_to_innodb is None


def test_a_plan_without_the_block_hashes_exactly_as_it_did_before_the_field_existed() -> None:
    """An entry that does not declare it keeps the marker hash its installs recorded.

    The canonical dump of a plan with no block is the dump the field did not exist in; if the
    new field's `None` leaked into it, every TBC and Vanilla install would start to read "made
    by an older plan" in the log over a plan nobody changed.
    """
    native_block = load_catalog().get("wow-tbc").install.native
    assert native_block is not None and native_block.cmangos is not None
    plan = native_block.cmangos.sql
    before = {k: v for k, v in plan.model_dump(mode="json").items() if k != "convert_to_innodb"}
    canonical = json.dumps(before, sort_keys=True, separators=(",", ":"))
    assert plan.plan_hash() == hashlib.sha256(canonical.encode("utf-8")).hexdigest()[:16]
    assert (
        PLAN.plan_hash()
        != hashlib.sha256(
            json.dumps(
                {k: v for k, v in PLAN.model_dump(mode="json").items() if k != "convert_to_innodb"},
                sort_keys=True,
                separators=(",", ":"),
            ).encode("utf-8")
        ).hexdigest()[:16]
    ), "Tortoise's block must be part of its hash: its plan did change"


def test_a_block_naming_a_database_the_game_does_not_have_is_refused_before_anything_runs(
    tmp_path: Path,
) -> None:
    plan = PLAN.model_copy(
        update={
            "convert_to_innodb": PLAN.convert_to_innodb.model_copy(  # type: ignore[union-attr]
                update={"schemas": (DB.characters, "acore_characters")}
            )
        }
    )
    with pytest.raises(InstallerError, match="convert_to_innodb.*acore_characters"):
        sqlplan.expand(plan, tmp_path, {n: n for n in (DB.auth, DB.characters, DB.world)}, {})


# -- the double ---------------------------------------------------------------------------


def test_the_double_refuses_the_bare_spelling_the_way_mariadb_does() -> None:
    """The fidelity check: without it a dropped `ROW_FORMAT=DYNAMIC` could pass against a double."""
    rec = ready()
    text = f"ALTER TABLE `{DB.characters}`.`auction` ENGINE=InnoDB;\n"
    import io

    proc = rec.exec_stdin("db", ["mariadb"], io.BytesIO(text.encode()), env={})
    assert proc.returncode == 1
    assert "errno: 140" in proc.stderr
    assert rec.tables[DB.characters]["auction"] == ["MyISAM", "FIXED"]
    # ...and a DYNAMIC MyISAM table IS converted by the bare spelling, so the refusal above is
    # about the row format and not about the double refusing everything.
    text = f"ALTER TABLE `{DB.characters}`.`characters` ENGINE=InnoDB;\n"
    assert rec.exec_stdin("db", ["mariadb"], io.BytesIO(text.encode()), env={}).returncode == 0
    assert rec.tables[DB.characters]["characters"][0] == "InnoDB"


# -- the import ---------------------------------------------------------------------------


def test_a_fresh_import_leaves_every_character_and_account_table_on_innodb(
    tmp_path: Path,
) -> None:
    rec = ready()
    said = run_import(rec, tmp_path)
    for schema in (DB.characters, DB.auth):
        engines = {name: row[0] for name, row in rec.tables[schema].items()}
        assert set(engines.values()) == {"InnoDB"}, (schema, engines)
    assert said[-1] == "The databases are imported and marked complete."
    assert marker_written(rec)


def test_the_fixed_tables_are_converted_with_a_row_format_innodb_has(tmp_path: Path) -> None:
    rec = ready()
    run_import(rec, tmp_path)
    assert rec.tables[DB.characters]["auction"] == ["InnoDB", "DYNAMIC"]
    assert rec.tables[DB.auth]["rbac_account_permissions"] == ["InnoDB", "DYNAMIC"]


def test_the_world_and_the_tables_already_on_innodb_are_not_touched(tmp_path: Path) -> None:
    rec = ready()
    run_import(rec, tmp_path)
    assert rec.tables[DB.world]["creature"] == ["MyISAM", "FIXED"]
    assert rec.tables[DB.characters]["item_instance"] == ["InnoDB", "COMPRESSED"]
    assert rec.tables[DB.auth]["account"] == ["InnoDB", "COMPRESSED"]
    altered = {_ALTER.match(line)["table"] for line in rec.alters}  # type: ignore[index]
    assert altered == {"auction", "characters", "rbac_account_permissions", "realmlist"}


def test_the_conversion_runs_after_the_last_dump_and_before_verify_and_the_marker(
    tmp_path: Path,
) -> None:
    """After the dumps (they create the tables), before the marker (it says they are safe)."""
    rec = ready()
    run_import(rec, tmp_path)
    alter_at = [i for i, s in enumerate(rec.sql_scripts) if s.startswith("ALTER TABLE ")]
    dump_at = [i for i, s in enumerate(rec.sql_scripts) if s.startswith("-- ")]
    marker_at = next(i for i, s in enumerate(rec.sql_scripts) if sqlplan.MARKER_TABLE in s)
    assert alter_at and dump_at
    assert min(alter_at) > max(dump_at)
    assert max(alter_at) < marker_at
    verify_asked = [
        i for i, s in enumerate(rec.sql_calls) if any(s == r.query for r in PLAN.verify)
    ]
    first_alter_call = next(i for i, s in enumerate(rec.sql_calls) if s.startswith("ALTER TABLE "))
    assert verify_asked and min(verify_asked) > first_alter_call


def test_a_conversion_the_database_refuses_stops_the_import_and_writes_no_marker(
    tmp_path: Path,
) -> None:
    rec = ready(refuse_table="characters")
    with pytest.raises(InstallerError) as raised:
        run_import(rec, tmp_path)
    assert "InnoDB" in str(raised.value)
    assert "`characters`" in str(raised.value) and "errno: 140" in str(raised.value)
    assert not marker_written(rec)


def test_a_conversion_that_raises_is_the_installs_own_sentence_and_no_marker(
    tmp_path: Path,
) -> None:
    """Not a traceback: the stage says what it was doing, and the next press imports again."""
    rec = ready(alter_raises=BrokenPipeError(32, "Broken pipe"))
    with pytest.raises(InstallerError, match="converting its tables to InnoDB.*BrokenPipeError"):
        run_import(rec, tmp_path)
    assert not marker_written(rec)


def test_a_table_still_myisam_after_the_conversion_is_a_failed_check_and_no_marker(
    tmp_path: Path,
) -> None:
    """The check asks the database again rather than trusting that the ALTERs exited 0."""
    rec = ready(ignore_alters=True)
    with pytest.raises(InstallerError) as raised:
        run_import(rec, tmp_path)
    assert "not InnoDB" in str(raised.value)
    assert "characters (MyISAM)" in str(raised.value)
    assert not marker_written(rec)


def test_an_entry_without_the_block_asks_nothing_and_alters_nothing(tmp_path: Path) -> None:
    plan = PLAN.model_copy(update={"convert_to_innodb": None})
    assert _NATIVE is not None and _NATIVE.cmangos is not None
    data = _NATIVE.cmangos.model_copy(update={"sql": plan})
    entry = TORTOISE.model_copy(
        update={
            "install": TORTOISE.install.model_copy(
                update={"native": _NATIVE.model_copy(update={"cmangos": data})}
            )
        }
    )
    rec = ready()
    list(tortoise_engine(rec, entry)._import(context(server_dir(tmp_path, plan), entry)))
    assert rec.alters == []
    assert not any("information_schema.TABLES" in s for s in rec.sql_calls)
    assert rec.tables[DB.characters]["characters"] == ["MyISAM", "DYNAMIC"]
    assert marker_written(rec)


def test_a_second_conversion_over_converted_schemas_sends_no_alter() -> None:
    """Idempotent by what it asks: only tables still MyISAM are altered, so a repeat is a no-op."""
    rec = ready()
    kwargs: dict[str, object] = dict(
        container="tortoise-db",
        client="mariadb",
        password=DB_PASSWORD,
        schemas={n: n for n in (DB.auth, DB.characters, DB.world, "tw_logs")},
        exec_stdin=rec.exec_stdin,
        sql_query=rec.sql_query,
        cancel=None,
    )
    list(sqlplan.convert_to_innodb(PLAN, **kwargs))  # type: ignore[arg-type]
    first = len(rec.alters)
    assert first == 4, rec.alters
    list(sqlplan.convert_to_innodb(PLAN, **kwargs))  # type: ignore[arg-type]
    assert len(rec.alters) == first, rec.alters[first:]
