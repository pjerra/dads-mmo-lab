"""T778: the stdlib script that packs a WotLK server's accounts and characters inside a WSL distro.

The script is a player's program, run without Yu'lon. Here it runs as a real subprocess with
a fake `docker` (and, for one case, fake `mysql`/`mysqldump` programs) first on PATH, so the
command lines, the environment and the file modes it produces are what is checked. Its package
then goes through the real reader (`move.read_package`), the real dump check
(`maintenance.verify_dump`) and the real import planner (`move_flows.plan_import`) against a
fresh server of the same game, version and modules.
"""

from __future__ import annotations

import importlib.util
import json
import re
import stat
import subprocess
import sys
from collections.abc import Iterable, Mapping
from datetime import datetime
from pathlib import Path
from types import ModuleType
from typing import Any

import pytest

from tests.test_move_flows import Box
from yulon import move, move_flows
from yulon.catalog.catalog import load_catalog
from yulon.controller_wow_wotlk import maintenance

REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "archive" / "guides" / "wow-wotlk" / "export-accounts-and-characters-wsl.py"
PASSWORD = "sekrit-test-pw"

FIXTURE: dict[str, Any] = {
    "daemon": True,
    "password": PASSWORD,
    "containers": {
        "ac-database": True,
        "ac-authserver": False,
        "ac-worldserver": False,
    },
    "databases": [
        "information_schema",
        "acore_auth",
        "acore_characters",
        "acore_world",
        "mysql",
        "performance_schema",
        "sys",
    ],
    "tables": {
        "acore_auth": ["account", "account_access", "realmlist", "updates"],
        "acore_characters": ["characters", "character_inventory", "updates"],
        "acore_world": ["creature", "updates"],
    },
    "rows": {
        "acore_auth": {
            "account": ["(1,'ALICE')", "(2,'BOB')", "(3,'RNDBOT1')"],
            "realmlist": ["(1,'Local Realm')"],
        },
        "acore_characters": {
            "characters": [
                "(10,1,'Thrall')",
                "(11,2,'Jaina')",
                "(12,2,'Sylvanas')",
                "(20,3,'RNDBOT1Lok')",
                "(21,3,'RNDBOT1Two')",
            ],
        },
        "acore_world": {"creature": ["(500,'world only')"]},
    },
    "updates": {
        "acore_auth": ["2024_01_a", "2024_01_b"],
        "acore_characters": ["2024_01_c"],
        "acore_world": ["2024_02_w"],
    },
    "module_rows": {"acore_characters": [["mod-transmog", "9F2C"]]},
    "bot_prefix": "RNDBOT",
    "truncate": [],
    "no_banner": [],
    "counts": [2, 3, 1, 2],
    "realm": "Local Realm",
}
"""What the fake server holds: two player accounts with three characters, one bot account with
one bot character, the release names of each ledger, and the one module row."""

FAKE_DB = r"""
import json, os, re, sys
from pathlib import Path

FIXTURE = json.loads(Path(os.environ["FAKE_DB_FIXTURE"]).read_text(encoding="utf-8"))
LOG = Path(os.environ["FAKE_DB_LOG"])


def log(**entry):
    with LOG.open("a", encoding="utf-8") as fh:
        fh.write(json.dumps(entry) + "\n")


def fail(message, code=1):
    sys.stderr.write(message + "\n")
    sys.exit(code)


def dump(database, ignore):
    named = "acore_world" if database in FIXTURE.get("wrong_name", []) else database
    lines = [
        "-- Not a dump" if database in FIXTURE["no_banner"]
        else "-- MySQL dump 10.13  Distrib 8.4.3, for Linux (x86_64)",
        "--",
        f"-- Host: localhost    Database: {database}",
        "-- ------------------------------------------------------",
        "-- Server version\t8.4.3",
        "",
        f"CREATE DATABASE /*!32312 IF NOT EXISTS*/ `{named}` "
        "/*!40100 DEFAULT CHARACTER SET utf8mb4 */;",
        f"USE `{named}`;",
    ]
    if database in FIXTURE.get("statement_first", []):
        lines.insert(0, "DROP DATABASE `other`;")
    for table in FIXTURE["tables"].get(database, []):
        if f"{database}.{table}" in ignore:
            continue
        lines += [
            f"DROP TABLE IF EXISTS `{table}`;",
            f"CREATE TABLE `{table}` (",
            "  `id` int NOT NULL",
            ");",
        ]
        rows = FIXTURE["rows"].get(database, {}).get(table, [])
        if rows:
            lines.append(f"INSERT INTO `{table}` VALUES " + ",".join(rows) + ";")
    if database not in FIXTURE["truncate"]:
        lines.append("-- Dump completed on 2026-10-12 21:00:00")
    return "\n".join(lines) + "\n"


def answer(sql):
    if sql.strip().upper().startswith("SHOW DATABASES"):
        return "".join(f"{name}\n" for name in FIXTURE["databases"])
    if "information_schema.TABLES" in sql:
        schema = re.search(r"TABLE_SCHEMA = '([^']+)'", sql).group(1)
        names = FIXTURE["tables"].get(schema, [])
        wanted = ("updates", "migrations", "character_db_version", "realmd_db_version")
        return "".join(f"{n}\n" for n in wanted if n in names)
    if "`updates` WHERE `state` IN" in sql:
        schema = re.search(r"FROM `(\w+)`\.`updates`", sql).group(1)
        return "".join(f"{n}\n" for n in FIXTURE["updates"].get(schema, []))
    if "`updates` WHERE `state` NOT IN" in sql:
        schema = re.search(r"FROM `(\w+)`\.`updates`", sql).group(1)
        return "".join(f"{n}\t{h}\n" for n, h in FIXTURE["module_rows"].get(schema, []))
    if "SELECT (SELECT COUNT(*)" in sql:
        if f"= '{FIXTURE['bot_prefix'].upper()}'" not in sql:
            fail("unscripted count query: the bot clause is not the server prefix")
        return "\t".join(str(n) for n in FIXTURE["counts"]) + "\n"
    if "FROM `acore_auth`.`realmlist`" in sql:
        return FIXTURE["realm"] + "\n"
    fail("unscripted query: " + sql)


def docker(args):
    if args[:1] in (["info"], ["version"]):
        if not FIXTURE["daemon"]:
            fail("Cannot connect to the Docker daemon", 1)
        return print("27.0.0")
    if args[:1] == ["inspect"]:
        name = args[-1]
        if name not in FIXTURE["containers"]:
            fail(f"Error: No such object: {name}", 1)
        return print("true" if FIXTURE["containers"][name] else "false")
    if args[:1] == ["exec"]:
        rest = args[1:]
        if rest[:1] == ["-i"]:
            rest = rest[1:]
        if rest[:1] == ["-e"]:
            rest = rest[2:]
        container, tool, tool_args = rest[0], rest[1], rest[2:]
        if container not in FIXTURE["containers"]:
            fail(f"Error: No such container: {container}", 1)
        if not FIXTURE["containers"][container]:
            fail(f"Error response from daemon: container {container} is not running", 1)
        return client(tool, tool_args, container)
    fail("unscripted docker call: " + " ".join(args))


def client(tool, args, container):
    password = os.environ.get("MYSQL_PWD")
    log(tool=tool, argv=args, container=container, pwd_present=password is not None,
        pwd_right=password == FIXTURE["password"], stdout_mode=oct(os.fstat(1).st_mode & 0o777))
    if password != FIXTURE["password"]:
        fail("ERROR 1045 (28000): Access denied for user 'root'@'localhost'", 1)
    if tool in ("mysql", "mariadb"):
        return sys.stdout.write(answer(sys.stdin.read()))
    if tool in ("mysqldump", "mariadb-dump"):
        for flag in ("--single-transaction", "--routines", "--events", "--triggers"):
            if flag not in args:
                fail("mysqldump was not asked for " + flag, 1)
        database = args[args.index("--databases") + 1]
        ignore = [a.split("=", 1)[1] for a in args if a.startswith("--ignore-table=")]
        return sys.stdout.write(dump(database, ignore))
    fail("unscripted tool: " + tool)


if __name__ == "__main__":
    tool, args = sys.argv[1], sys.argv[2:]
    if tool == "docker":
        docker(args)
    else:
        client(tool, args, None)
"""


class TargetDb:
    """A fresh WotLK server of the same version and modules, answering the planner's questions.

    The planner only reads from it: the databases, their tables, the ledgers, the counts and
    the realm row. An empty server, so nothing it holds would be lost to the import.
    """

    def __init__(self, events: list[str], *, updates: tuple[str, ...] | None = None) -> None:
        self.events: list[str] = events
        self.updates = updates

    def databases(self) -> tuple[str, ...]:
        return tuple(FIXTURE["databases"])

    def dump_into(self, database: str, sink: Any, ignore: tuple[str, ...] = ()) -> None:
        raise AssertionError("the planner never dumps")

    def load_from(self, source: Any) -> None:
        raise AssertionError("the planner never loads")

    def execute(self, sql: str) -> None:
        raise AssertionError("the planner never writes")

    def tables(self, database: str) -> tuple[tuple[str, str], ...]:
        return tuple((name, "BASE TABLE") for name in FIXTURE["tables"].get(database, []))

    def query(self, sql: str) -> str:
        if "information_schema.TABLES" in sql:
            schema = re.search(r"TABLE_SCHEMA = '([^']+)'", sql).group(1)  # type: ignore[union-attr]
            return "updates\n" if "updates" in FIXTURE["tables"].get(schema, []) else ""
        if "`updates` WHERE `state` IN" in sql:
            schema = re.search(r"FROM `(\w+)`\.`updates`", sql).group(1)  # type: ignore[union-attr]
            names = self.updates if self.updates is not None else FIXTURE["updates"][schema]
            return "".join(f"{n}\n" for n in names)
        if "`updates` WHERE `state` NOT IN" in sql:
            schema = re.search(r"FROM `(\w+)`\.`updates`", sql).group(1)  # type: ignore[union-attr]
            return "".join(f"{n}\t{h}\n" for n, h in FIXTURE["module_rows"].get(schema, []))
        if "SELECT (SELECT COUNT(*)" in sql:
            return "0\t0\t0\t0\n"
        if "COUNT(*)" in sql:
            return "0\n"
        if "FROM `acore_auth`.`realmlist`" in sql:
            return "Target Realm\n"
        raise AssertionError(f"unscripted target query: {sql}")


def _script() -> ModuleType:
    spec = importlib.util.spec_from_file_location("wsl_characters_export", SCRIPT)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    # Registered before it runs: the script's dataclasses look their module up by name.
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def _world(tmp_path: Path, db: TargetDb | None = None) -> Box:
    tmp_path.mkdir(parents=True, exist_ok=True)
    return Box(tmp_path, "wow-wotlk", db=db or TargetDb([]))


def _unpack(package: move.Package, schema: str, root: Path) -> Path:
    root.mkdir(parents=True, exist_ok=True)
    return package.extract(schema, root)


def _write_fake_tools(root: Path, names: Iterable[str]) -> Path:
    """`bin/<name>` shims that run the fake database program as that tool."""
    bin_dir = root / "bin"
    bin_dir.mkdir(parents=True, exist_ok=True)
    fake = root / "fake_db.py"
    fake.write_text(FAKE_DB, encoding="utf-8")
    for name in names:
        shim = bin_dir / name
        shim.write_text(
            f'#!/bin/sh\nexec "{sys.executable}" "{fake}" {name} "$@"\n',
            encoding="utf-8",
        )
        shim.chmod(0o755)
    return bin_dir


def _run(
    tmp_path: Path,
    *,
    tools: Iterable[str] = ("docker",),
    fixture: Mapping[str, Any] | None = None,
    args: Iterable[str] = (),
    conf: str | None = None,
) -> tuple[subprocess.CompletedProcess[str], Path, Path, Path]:
    """Run the script; returns (process, server dir, out dir, the fake's log)."""
    root = tmp_path / "run"
    root.mkdir(parents=True, exist_ok=True)
    server = root / "server"
    server.mkdir(exist_ok=True)
    (server / ".env").write_text(f"MYSQL_ROOT_PASSWORD={PASSWORD}\n", encoding="utf-8")
    if conf is not None:
        conf_file = server / "env" / "dist" / "etc" / "modules" / "playerbots.conf"
        conf_file.parent.mkdir(parents=True, exist_ok=True)
        conf_file.write_text(conf, encoding="utf-8")
    out = root / "out"
    out.mkdir(exist_ok=True)
    data = {**FIXTURE, **(fixture or {})}
    fixture_path = root / "fixture.json"
    fixture_path.write_text(json.dumps(data), encoding="utf-8")
    log = root / "fake.log"
    bin_dir = _write_fake_tools(root, tools)
    env = {
        "PATH": str(bin_dir),
        "HOME": str(root / "home"),
        "LANG": "C.UTF-8",
        "FAKE_DB_FIXTURE": str(fixture_path),
        "FAKE_DB_LOG": str(log),
    }
    process = subprocess.run(
        [
            sys.executable,
            str(SCRIPT),
            "--server-dir",
            str(server),
            "--out",
            str(out),
            *args,
        ],
        env=env,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        timeout=120,
        check=False,
    )
    return process, server, out, log


def _the_package(out: Path) -> Path:
    found = sorted(out.glob("yulon-move-wow-wotlk-*-keep-private.zip"))
    assert len(found) == 1, f"expected one package in {out}, found {found}"
    return found[0]


def _entries(log: Path) -> list[dict[str, Any]]:
    if not log.exists():
        return []
    return [json.loads(line) for line in log.read_text(encoding="utf-8").splitlines()]


# ================================================================ the package


def test_docker_mode_packs_accounts_and_characters_that_the_real_planner_accepts(
    tmp_path: Path,
) -> None:
    process, _server, out, _log = _run(tmp_path)
    assert process.returncode == 0, process.stderr
    package = _the_package(out)

    read = move.read_package(package)
    manifest = read.manifest
    assert manifest.kind == "characters"
    assert manifest.game.id == "wow-wotlk"
    assert [m.schema_name for m in manifest.databases] == [
        "acore_auth",
        "acore_characters",
    ]
    assert manifest.counts == move.Counts(
        accounts=2, characters=3, bot_accounts=1, bot_characters=2
    )
    assert manifest.realm_name == "Local Realm"
    assert manifest.channel_account is None
    assert manifest.bot_prefix == "rndbot"
    assert manifest.excluded == (
        "acore_world (not part of accounts and characters)",
        "realmlist of acore_auth (the realm's own address)",
    )
    assert manifest.secrets == move.SECRETS

    # Each dump is read as Yu'lon's own restore reads it, and holds the right characters.
    characters = _unpack(read, "acore_characters", tmp_path / "unpacked")
    maintenance.verify_dump(characters, "acore_characters")
    text = characters.read_text(encoding="utf-8")
    assert "'Thrall'" in text and "'Sylvanas'" in text and "'RNDBOT1Lok'" in text
    assert "world only" not in text

    # The real planner accepts the package on a fresh server of the same game and version.
    box = _world(tmp_path / "target")
    plan = move_flows.plan_import(box.world, package)
    assert plan.refusals == (), plan.refusals
    assert plan.manifest is not None
    assert plan.manifest.counts.characters == 3


def test_the_package_layout_is_the_export_layout_byte_for_byte(tmp_path: Path) -> None:
    process, _server, out, _log = _run(tmp_path)
    assert process.returncode == 0, process.stderr
    package = _the_package(out)

    import zipfile

    with zipfile.ZipFile(package) as archive:
        names = archive.namelist()
        raw = archive.read(move.MANIFEST_NAME).decode("utf-8")
    assert names == [move.MANIFEST_NAME, "db/acore_auth.sql", "db/acore_characters.sql"]
    # The manifest's text is exactly what Yu'lon's own model writes for the same content.
    model = move.Manifest.model_validate(json.loads(raw))
    assert raw == model.model_dump_json(by_alias=True, indent=2)
    assert json.loads(raw)["format"] == move.FORMAT
    assert json.loads(raw)["server"] is None
    assert package.name.startswith("yulon-move-wow-wotlk-")


def test_the_auth_dump_leaves_the_realm_row_out_and_the_password_stays_out_of_argv(
    tmp_path: Path,
) -> None:
    process, _server, out, log = _run(tmp_path)
    assert process.returncode == 0, process.stderr
    package = _the_package(out)
    read = move.read_package(package)
    auth = _unpack(read, "acore_auth", tmp_path / "unpacked")
    assert "CREATE TABLE `realmlist`" not in auth.read_text(encoding="utf-8")

    entries = _entries(log)
    dumps = [e for e in entries if e["tool"] == "mysqldump"]
    assert ["--ignore-table=acore_auth.realmlist" in e["argv"] for e in dumps] == [
        True,
        False,
    ]
    assert all(e["container"] == "ac-database" for e in entries)
    assert all(e["pwd_present"] and e["pwd_right"] for e in entries)
    combined = json.dumps(entries) + process.stdout + process.stderr
    assert PASSWORD not in combined


def test_the_package_and_its_working_files_are_owner_only(tmp_path: Path) -> None:
    process, _server, out, _log = _run(tmp_path)
    assert process.returncode == 0, process.stderr
    package = _the_package(out)
    assert stat.S_IMODE(package.stat().st_mode) == 0o600
    assert sorted(p.name for p in out.iterdir()) == [package.name]


# ================================================================ refusals


def test_a_running_server_is_refused_in_docker_mode(tmp_path: Path) -> None:
    process, _server, out, _log = _run(
        tmp_path,
        fixture={
            "containers": {
                "ac-database": True,
                "ac-authserver": True,
                "ac-worldserver": True,
            }
        },
    )
    assert process.returncode == 2
    assert "Stop the server" in process.stderr
    assert list(out.iterdir()) == []


def test_a_stopped_database_container_is_refused_in_plain_words(tmp_path: Path) -> None:
    process, _server, out, _log = _run(
        tmp_path,
        fixture={
            "containers": {
                "ac-database": False,
                "ac-authserver": False,
                "ac-worldserver": False,
            }
        },
    )
    assert process.returncode == 2
    assert "database" in process.stderr and "stopped" in process.stderr
    assert list(out.iterdir()) == []


def test_host_mode_needs_the_server_stopped_flag_and_then_packs(tmp_path: Path) -> None:
    refused, _server, out, _log = _run(tmp_path / "a", tools=("mysql", "mysqldump"))
    assert refused.returncode == 2
    assert "--server-stopped" in refused.stderr
    assert list(out.iterdir()) == []

    packed, _server, out, log = _run(
        tmp_path / "b", tools=("mysql", "mysqldump"), args=("--server-stopped",)
    )
    assert packed.returncode == 0, packed.stderr
    assert {e["tool"] for e in _entries(log)} <= {"mysql", "mysqldump"}
    assert all(e["container"] is None for e in _entries(log))
    assert move.read_package(_the_package(out)).manifest.game.id == "wow-wotlk"


def test_no_docker_and_no_mysql_client_is_refused_in_plain_words(
    tmp_path: Path,
) -> None:
    process, _server, out, _log = _run(tmp_path, tools=())
    assert process.returncode == 2
    assert "docker" in process.stderr.lower() and "mysql" in process.stderr.lower()
    assert list(out.iterdir()) == []


def test_a_server_folder_with_no_password_and_no_terminal_is_refused(
    tmp_path: Path,
) -> None:
    root = tmp_path / "nopass"
    root.mkdir()
    server = root / "server"
    server.mkdir()
    out = root / "out"
    out.mkdir()
    fixture_path = root / "fixture.json"
    fixture_path.write_text(json.dumps(FIXTURE), encoding="utf-8")
    bin_dir = _write_fake_tools(root, ("docker",))
    env = {
        "PATH": str(bin_dir),
        "HOME": str(root / "home"),
        "LANG": "C.UTF-8",
        "FAKE_DB_FIXTURE": str(fixture_path),
        "FAKE_DB_LOG": str(root / "fake.log"),
    }
    process = subprocess.run(
        [sys.executable, str(SCRIPT), "--server-dir", str(server), "--out", str(out)],
        env=env,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        timeout=120,
        check=False,
    )
    assert process.returncode == 2
    assert "password" in process.stderr.lower()
    assert list(out.iterdir()) == []


# ================================================================ the planner is really asked


def test_the_real_planner_refuses_a_fresh_server_on_another_version(
    tmp_path: Path,
) -> None:
    process, _server, out, _log = _run(tmp_path)
    assert process.returncode == 0, process.stderr
    package = _the_package(out)
    box = _world(tmp_path / "other", TargetDb([], updates=("2024_01_a",)))
    plan = move_flows.plan_import(box.world, package)
    assert any("not at the same version" in refusal for refusal in plan.refusals), plan.refusals


# ================================================================ the script and the app agree


def test_the_script_constants_match_the_catalog_and_the_package_format() -> None:
    script = _script()
    entry = load_catalog().get("wow-wotlk")
    schemas = entry.schema_map()
    assert script.SCHEMA["auth"] == schemas["auth"]
    assert script.SCHEMA["characters"] == schemas["characters"]
    assert script.SCHEMA["playerbots"] == schemas["playerbots"]
    assert script.SCHEMA["ale"] == schemas["ale"]
    assert script.CONTAINERS["db"] == entry.container_spec().db
    assert script.CONTAINERS["auth"] == entry.container_spec().auth
    assert script.CONTAINERS["world"] == entry.container_spec().world
    assert script.REALMLIST_TABLE == entry.realmlist.table
    assert script.REALM_ID == entry.realmlist.realm_id
    assert script.CHARACTERS_TABLE == entry.observability.characters.table
    assert script.CHARACTERS_ACCOUNT == entry.observability.characters.account
    assert script.DEFAULT_BOT_PREFIX == entry.observability.bots.account_prefix
    assert script.BOT_PREFIX_KEYS == (
        "AiPlayerbot.RandomBotAccountPrefix",
        "Playerbots.RandomBotAccountPrefix",
    )
    assert entry.observability.bots.prefix_conf_key in script.BOT_PREFIX_KEYS
    assert script.SECRETS == move.SECRETS
    assert script.FORMAT == move.FORMAT
    assert script.MANIFEST_NAME == move.MANIFEST_NAME
    assert script.GAME_RECORD_TEMPLATE.format(game="wow-wotlk") == "-- yulon-backup: game=wow-wotlk"
    assert re.fullmatch(maintenance.GAME_RECORD.pattern, "-- yulon-backup: game=wow-wotlk")


# ================================================================ guards, one per refusal


def test_the_dumps_are_owner_only_while_they_are_written(tmp_path: Path) -> None:
    process, _server, _out, log = _run(tmp_path)
    assert process.returncode == 0, process.stderr
    dumps = [e for e in _entries(log) if e["tool"] == "mysqldump"]
    assert len(dumps) == 2 and all(e["stdout_mode"] == "0o600" for e in dumps), dumps


def test_a_dump_cut_short_is_refused_and_nothing_is_packed(tmp_path: Path) -> None:
    process, _server, out, _log = _run(tmp_path, fixture={"truncate": ["acore_characters"]})
    assert process.returncode == 2
    assert "cut short" in process.stderr
    assert list(out.iterdir()) == []


def test_a_dump_without_mysqldumps_banner_is_refused(tmp_path: Path) -> None:
    process, _server, out, _log = _run(tmp_path, fixture={"no_banner": ["acore_characters"]})
    assert process.returncode == 2
    assert "does not start like a mysqldump" in process.stderr
    assert list(out.iterdir()) == []


def test_the_bot_prefix_comes_from_the_servers_conf(tmp_path: Path) -> None:
    conf = "[worldserver]\nAiPlayerbot.RandomBotAccountPrefix = XBOT\n"
    process, _server, out, _log = _run(tmp_path, fixture={"bot_prefix": "XBOT"}, conf=conf)
    assert process.returncode == 0, process.stderr
    assert move.read_package(_the_package(out)).manifest.bot_prefix == "XBOT"


def test_a_bot_prefix_set_to_two_values_is_refused(tmp_path: Path) -> None:
    conf = "AiPlayerbot.RandomBotAccountPrefix = XBOT\nPlayerbots.RandomBotAccountPrefix = YBOT\n"
    process, _server, out, _log = _run(tmp_path, conf=conf)
    assert process.returncode == 2
    assert "set more than once" in process.stderr
    assert list(out.iterdir()) == []


def test_bot_and_engine_schemas_travel_when_the_server_has_them(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setitem(FIXTURE, "databases", [*FIXTURE["databases"], "acore_playerbots"])
    monkeypatch.setitem(
        FIXTURE,
        "tables",
        {
            **FIXTURE["tables"],
            "acore_playerbots": ["playerbots_account_type", "updates"],
        },
    )
    monkeypatch.setitem(
        FIXTURE, "updates", {**FIXTURE["updates"], "acore_playerbots": ["2024_01_p"]}
    )
    process, _server, out, _log = _run(tmp_path)
    assert process.returncode == 0, process.stderr
    package = _the_package(out)
    read = move.read_package(package)
    assert [m.schema_name for m in read.manifest.databases] == [
        "acore_auth",
        "acore_characters",
        "acore_playerbots",
    ]
    assert set(read.manifest.schema_evidence) == {
        "acore_auth",
        "acore_characters",
        "acore_playerbots",
    }
    plan = move_flows.plan_import(_world(tmp_path / "target").world, package)
    assert plan.refusals == (), plan.refusals


def test_an_existing_package_of_the_same_minute_is_never_overwritten(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    script = _script()
    root = tmp_path / "exists"
    root.mkdir(parents=True)
    server = root / "server"
    server.mkdir()
    (server / ".env").write_text(f"MYSQL_ROOT_PASSWORD={PASSWORD}\n", encoding="utf-8")
    out = root / "out"
    out.mkdir()
    fixture = root / "fixture.json"
    fixture.write_text(json.dumps(FIXTURE), encoding="utf-8")
    monkeypatch.setenv("PATH", str(_write_fake_tools(root, ("docker",))))
    monkeypatch.setenv("FAKE_DB_FIXTURE", str(fixture))
    monkeypatch.setenv("FAKE_DB_LOG", str(root / "fake.log"))

    class Fixed(datetime):
        @classmethod
        def now(cls, tz: object = None) -> Fixed:  # type: ignore[override]
            return cls(2026, 10, 12, 21, 0, 0)

    monkeypatch.setattr(script, "datetime", Fixed)
    existing = out / "yulon-move-wow-wotlk-20261012-2100-keep-private.zip"
    existing.write_bytes(b"an earlier package")
    with pytest.raises(script.Refusal, match="already exists"):
        script.pack(server, out, False)
    assert existing.read_bytes() == b"an earlier package"
    assert sorted(p.name for p in out.iterdir()) == [existing.name]


def test_a_statement_above_the_banner_is_refused(tmp_path: Path) -> None:
    process, _server, out, _log = _run(tmp_path, fixture={"statement_first": ["acore_characters"]})
    assert process.returncode == 2
    assert "does not start like a mysqldump" in process.stderr
    assert list(out.iterdir()) == []


def test_a_server_without_a_characters_database_is_refused(tmp_path: Path) -> None:
    databases = [d for d in FIXTURE["databases"] if d != "acore_characters"]
    process, _server, out, _log = _run(tmp_path, fixture={"databases": databases})
    assert process.returncode == 2
    assert "has no acore_characters" in process.stderr
    assert list(out.iterdir()) == []


def test_a_ledger_that_cannot_be_read_is_refused(tmp_path: Path) -> None:
    updates = {"acore_auth": ["2024_01_a", "2024_01_b"], "acore_characters": []}
    process, _server, out, _log = _run(tmp_path, fixture={"updates": updates})
    assert process.returncode == 2
    assert "database version of acore_characters could not be read" in process.stderr
    assert list(out.iterdir()) == []


def test_docker_that_does_not_answer_is_said_so(tmp_path: Path) -> None:
    process, _server, out, _log = _run(tmp_path, fixture={"daemon": False})
    assert process.returncode == 2
    assert "not answering" in process.stderr
    assert list(out.iterdir()) == []


def test_a_dump_that_names_another_database_is_refused(tmp_path: Path) -> None:
    process, _server, out, _log = _run(tmp_path, fixture={"wrong_name": ["acore_characters"]})
    assert process.returncode == 2
    assert "does not name acore_characters" in process.stderr
    assert list(out.iterdir()) == []


def test_the_written_package_is_read_back_before_it_takes_its_name(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    script = _script()
    root = tmp_path / "readback"
    root.mkdir(parents=True)
    server = root / "server"
    server.mkdir()
    (server / ".env").write_text(f"MYSQL_ROOT_PASSWORD={PASSWORD}\n", encoding="utf-8")
    out = root / "out"
    out.mkdir()
    fixture = root / "fixture.json"
    fixture.write_text(json.dumps(FIXTURE), encoding="utf-8")
    monkeypatch.setenv("PATH", str(_write_fake_tools(root, ("docker",))))
    monkeypatch.setenv("FAKE_DB_FIXTURE", str(fixture))
    monkeypatch.setenv("FAKE_DB_LOG", str(root / "fake.log"))

    class Fixed(datetime):
        @classmethod
        def now(cls, tz: object = None) -> Fixed:  # type: ignore[override]
            return cls(2026, 10, 12, 21, 0, 0)

    monkeypatch.setattr(script, "datetime", Fixed)
    real_read_back = script._read_back
    seen: list[tuple[str, bool]] = []

    def spy(partial: Path, manifest: str) -> None:
        seen.append((partial.name, partial.exists()))
        real_read_back(partial, manifest)

    monkeypatch.setattr(script, "_read_back", spy)
    dest, _counts = script.pack(server, out, False)
    assert seen == [(dest.name + ".partial", True)]
    assert dest.exists()
    assert not (out / (dest.name + ".partial")).exists()
