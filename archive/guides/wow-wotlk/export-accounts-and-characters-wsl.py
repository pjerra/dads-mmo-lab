#!/usr/bin/env python3
"""Pack a WotLK server's accounts and characters for a move to Yu'lon, from inside WSL.

Run this inside the WSL distro where the server lives, with no Yu'lon installed there. It reads
the server's database through the distro's Docker (the server's own database container), or
through the MySQL client programs (`mysql`, `mysqldump`) when Docker is not there, and writes one
file: `yulon-move-wow-wotlk-<stamp>-keep-private.zip`. On the other computer, a fresh WotLK
server in Yu'lon takes it through Maintenance > "Bring in accounts and characters…".

What the file holds: the login accounts (names, password verifiers, GM levels), the characters,
and the bot and Lua-engine databases where the server has them. Not the world database, not the
config files, not the modules. The database password is never written into the file or printed.

**Secrets.** The file holds every account's login verifier and salt. Keep it private, and delete
it once the move is done. It is created readable by you only.

Standard library only, so any distro with Python 3.11 or newer runs it. Exit status: 0 packed,
2 refused (the reason is printed in plain words and nothing was packed).
"""

from __future__ import annotations

import argparse
import getpass
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path

FORMAT = 1
MANIFEST_NAME = "yulon-move.json"
KEEP_PRIVATE = "keep-private"
SCRIPT_TAG = "wsl-export"
"""What `made_by.yulon` says: this file came from this script, not from a Yu'lon build."""

GAME_ID = "wow-wotlk"
GAME_NAME = "WoW WotLK"
SCHEMA = {
    "auth": "acore_auth",
    "characters": "acore_characters",
    "playerbots": "acore_playerbots",
    "ale": "acore_ale",
}
CONTAINERS = {"db": "ac-database", "auth": "ac-authserver", "world": "ac-worldserver"}
REALMLIST_TABLE = "realmlist"
REALM_ID = 1
CHARACTERS_TABLE = "characters"
CHARACTERS_ACCOUNT = "account"
DEFAULT_BOT_PREFIX = "rndbot"
BOT_PREFIX_KEYS = (
    "AiPlayerbot.RandomBotAccountPrefix",
    "Playerbots.RandomBotAccountPrefix",
)
BOT_CONF = Path("env/dist/etc/modules/playerbots.conf")
ACCOUNT_PREFIX = "YULON_"
"""Yu'lon's own command-channel account starts with this; it is not a player."""

SYSTEM_SCHEMAS = frozenset({"information_schema", "performance_schema", "mysql", "sys"})
ROLES = ("auth", "characters", "playerbots", "ale")
EVIDENCE_ROLES = ("auth", "characters")
OPTIONAL_EVIDENCE_ROLES = ("playerbots", "ale")
LEDGER_NAMES = ("updates", "migrations", "character_db_version", "realmd_db_version")
GAME_RECORD_TEMPLATE = "-- yulon-backup: game={game}"
SECRETS = (
    "This file holds every account's login verifier and salt, so anyone who has it can try to "
    "work out the passwords. Keep it private and delete it once the move is done. The database "
    "password and the command-channel credential are not in it."
)
PASSWORD_VARIABLE = "MYSQL_ROOT_PASSWORD"
PASSWORD_FILE = ".db_password"
STOPPED_CONTAINERS = ("auth", "world")
DUMP_FLAGS = ("--single-transaction", "--routines", "--events", "--triggers")
CHUNK = 1 << 20

_BOT_PREFIX = re.compile(r"[A-Za-z0-9_.-]{1,64}")
_DUMP_HEADER = re.compile(rb"(?:\A|\n)--\s+(MySQL|MariaDB)\s+dump\s")
_DUMP_TRAILER = b"-- Dump completed"
_USE_LINE = re.compile(rb"(?:\A|\n)USE\s+`([^`\n]+)`")
_CREATE_DB_LINE = re.compile(rb"(?:\A|\n)CREATE DATABASE[^\n`]*`([^`\n]+)`")
_USE_AT_START = re.compile(rb"USE\s+`([^`\n]+)`")
_TABLE_LINE = re.compile(
    rb"(?:/\*!\d+\s+)?(?:CREATE\s+TABLE|CREATE\s+VIEW|VIEW)\s+`((?:[^`\n]|``)+)`"
)
_EDGE_BYTES = 8192


class Refusal(Exception):
    """A reason the script stops, in a sentence a player can act on. Nothing was packed."""


@dataclass
class Access:
    """How this distro reaches the server's database: through its container, or the client."""

    mode: str  # "docker" or "host"
    container: str | None = None
    env: dict[str, str] = field(default_factory=dict)

    def argv(self, tool: str, extra: list[str], *, stdin: bool = False) -> list[str]:
        """The command for one client run. MYSQL_PWD is passed by name, never by value."""
        if self.mode == "docker":
            flags = ["-i"] if stdin else []
            return [
                "docker",
                "exec",
                *flags,
                "-e",
                "MYSQL_PWD",
                str(self.container),
                tool,
                *extra,
            ]
        return [tool, "-h", "127.0.0.1", "-P", "3306", *extra]


DOCKER_SILENT_HINT = (
    " Docker is installed here but is not answering: start it (sudo service docker start), "
    "or add this user to the docker group, then run this again."
)


def _lit(value: str) -> str:
    """A SQL string literal's content: backslashes and quotes doubled, as Yu'lon writes them."""
    return value.replace("\\", "\\\\").replace("'", "''")


def _text(data: bytes) -> str:
    return data.decode("utf-8", "replace")


# ---------------------------------------------------------------- the database


def _choose_access(server_dir: Path | None, stopped_flag: bool) -> Access:
    """Docker through the server's database container when it answers, else the MySQL client."""
    docker = shutil.which("docker")
    docker_silent = False
    if docker is not None:
        ready = subprocess.run(
            [docker, "info", "--format", "{{.ServerVersion}}"],
            capture_output=True,
            text=True,
            check=False,
        )
        docker_silent = ready.returncode != 0
        if ready.returncode == 0:
            inspect = subprocess.run(
                [docker, "inspect", "-f", "{{.State.Running}}", CONTAINERS["db"]],
                capture_output=True,
                text=True,
                check=False,
            )
            if inspect.returncode == 0:
                if inspect.stdout.strip() != "true":
                    raise Refusal(
                        f"The server's database container {CONTAINERS['db']} is stopped, so "
                        "there is nothing to read. Start the server with its docker compose "
                        "files, then run this again. Nothing was packed."
                    )
                for name in STOPPED_CONTAINERS:
                    state = subprocess.run(
                        [
                            docker,
                            "inspect",
                            "-f",
                            "{{.State.Running}}",
                            CONTAINERS[name],
                        ],
                        capture_output=True,
                        text=True,
                        check=False,
                    )
                    if state.returncode == 0 and state.stdout.strip() == "true":
                        raise Refusal(
                            "The server is running. Stop the server first (its docker compose "
                            "stop), so the copies agree, then run this again. Nothing was packed."
                        )
                return Access(mode="docker", container=CONTAINERS["db"])
    if shutil.which("mysql") and shutil.which("mysqldump"):
        if not stopped_flag:
            raise Refusal(
                "This script can only see the database through the MySQL client, and that "
                "cannot tell whether the server is running. Stop the server first, then run "
                "this again with --server-stopped. Nothing was packed."
            )
        return Access(mode="host")
    raise Refusal(
        "Yu'lon's script found neither Docker with the server's database container nor the "
        "MySQL client programs (mysql and mysqldump) in this distro, so it cannot read the "
        "database. Start the server's Docker, or install the MySQL client with "
        "sudo apt install mysql-client, then run this again. Nothing was packed."
        + (DOCKER_SILENT_HINT if docker_silent else "")
    )


def _root_password(server_dir: Path | None) -> str:
    """The database root password: the server's own .env or compose files, else asked for."""
    if server_dir is not None:
        for name in (
            ".env",
            *sorted(p.name for p in server_dir.glob("*compose*.y*ml")),
        ):
            try:
                text = (server_dir / name).read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            found = re.search(rf"{PASSWORD_VARIABLE}\s*[:=]\s*[\"']?([^\"'\s#]+)", text)
            if (
                found is not None
                and found.group(1)
                and not found.group(1).startswith("$")
            ):
                return found.group(1)
        try:
            kept = (server_dir / PASSWORD_FILE).read_text(encoding="utf-8").strip()
        except (OSError, ValueError):
            kept = ""
        if kept:
            return kept
    if sys.stdin.isatty():
        typed = getpass.getpass("Database root password (typed, not shown): ")
        if typed:
            return typed
    raise Refusal(
        "Yu'lon's script could not find the database password: it is not in the server folder's "
        ".env or compose files, and there is no terminal to ask for it. Run it from a terminal, "
        f"or put {PASSWORD_VARIABLE} in the server folder's .env, then run it again. "
        "Nothing was packed."
    )


def _sql(access: Access, sql: str) -> str:
    argv = access.argv(
        "mysql", ["-uroot", "--batch", "--skip-column-names"], stdin=True
    )
    proc = subprocess.run(
        argv,
        input=sql.encode("utf-8") + b"\n",
        capture_output=True,
        env=access.env,
        check=False,
    )
    if proc.returncode != 0:
        raise Refusal(
            "The database did not answer the question the script asks, so nothing was packed "
            f"({_text(proc.stderr).strip()[:300] or 'no message'})."
        )
    return _text(proc.stdout)


def _databases(access: Access) -> list[str]:
    return [
        line.strip()
        for line in _sql(access, "SHOW DATABASES;").splitlines()
        if line.strip()
    ]


def _names(access: Access, sql: str) -> list[str]:
    return [line.strip() for line in _sql(access, sql).splitlines() if line.strip()]


def _digest(names: list[str]) -> str:
    """Same as Yu'lon's digest_of: SHA-256 of the sorted names, one per line."""
    return hashlib.sha256("\n".join(sorted(names)).encode("utf-8")).hexdigest()


def _evidence(access: Access, schema: str) -> dict[str, object] | None:
    """What the schema says about its own version, as Yu'lon's `_read_version` reads it."""
    try:
        listed = _sql(
            access,
            "SELECT TABLE_NAME FROM information_schema.TABLES "
            f"WHERE TABLE_SCHEMA = '{_lit(schema)}' AND TABLE_NAME IN "
            "('updates', 'migrations', 'character_db_version', 'realmd_db_version');",
        ).split()
        if "updates" not in listed:
            return None
        quoted = f"`{schema}`"
        items = _names(
            access,
            f"SELECT `name` FROM {quoted}.`updates` "
            "WHERE `state` IN ('RELEASED', 'ARCHIVED') ORDER BY `name`;",
        )
        modules = sorted(
            line.strip().replace("\t", "|")
            for line in _sql(
                access,
                f"SELECT `name`, `hash` FROM {quoted}.`updates` "
                "WHERE `state` NOT IN ('RELEASED', 'ARCHIVED') ORDER BY `name`;",
            ).splitlines()
            if line.strip()
        )
    except Refusal:
        return None
    if not items:
        return None
    return {
        "kind": "updates",
        "count": len(items),
        "digest": _digest(items),
        "modules": modules,
    }


def _bot_prefix(server_dir: Path | None) -> str:
    """The bot account prefix: the playerbots conf's one value, else the default; else refuse."""
    if server_dir is None:
        return DEFAULT_BOT_PREFIX
    try:
        text = (server_dir / BOT_CONF).read_text(encoding="utf-8", errors="replace")
    except FileNotFoundError:
        return DEFAULT_BOT_PREFIX
    except OSError as exc:
        raise Refusal(
            f"Yu'lon's script could not read {BOT_CONF}: {exc}. Nothing was packed."
        ) from exc
    values: list[str] = []
    for key in BOT_PREFIX_KEYS:
        pattern = re.compile(rf"^\s*{re.escape(key)}\s*=(.*)$")
        for line in text.splitlines():
            hit = pattern.match(line)
            if hit:
                values.append(hit.group(1).strip().strip('"'))
    if not values:
        return DEFAULT_BOT_PREFIX
    if len(set(values)) > 1:
        raise Refusal(
            "The bot account prefix is set more than once in the playerbots conf, to different "
            "values, so the script cannot tell which one the server uses. Nothing was packed."
        )
    if not values[0]:
        raise Refusal(
            "The bot account prefix is blank in the playerbots conf. Set it, then run this again. "
            "Nothing was packed."
        )
    if _BOT_PREFIX.fullmatch(values[0]) is None:
        raise Refusal(
            f"The bot account prefix {values[0]!r} has characters a package cannot hold. "
            "Nothing was packed."
        )
    return values[0]


def _counts(
    access: Access, auth: str, chars: str, prefix: str
) -> tuple[int, int, int, int]:
    bot = f"LEFT(UPPER(`username`), {len(prefix)}) = '{_lit(prefix.upper())}'"
    app = f"LEFT(UPPER(`username`), {len(ACCOUNT_PREFIX)}) = '{ACCOUNT_PREFIX}'"
    out = _sql(
        access,
        "SELECT "
        f"(SELECT COUNT(*) FROM `{auth}`.`account` WHERE NOT ({bot}) AND NOT ({app})), "
        f"(SELECT COUNT(*) FROM `{chars}`.`{CHARACTERS_TABLE}` WHERE `{CHARACTERS_ACCOUNT}` NOT IN "
        f"(SELECT `id` FROM `{auth}`.`account` WHERE ({bot}) OR ({app}))), "
        f"(SELECT COUNT(*) FROM `{auth}`.`account` WHERE ({bot})), "
        f"(SELECT COUNT(*) FROM `{chars}`.`{CHARACTERS_TABLE}` WHERE `{CHARACTERS_ACCOUNT}` IN "
        f"(SELECT `id` FROM `{auth}`.`account` WHERE ({bot})));",
    )
    fields = out.strip().split("\t")
    if len(fields) != 4 or not all(f.strip().isdigit() for f in fields):
        raise Refusal(
            "The database returned the account counts in a shape the script does not "
            "read. Nothing was packed."
        )
    first, second, third, fourth = (int(f.strip()) for f in fields)
    return first, second, third, fourth


def _realm_name(access: Access, auth: str) -> str | None:
    try:
        out = _sql(
            access,
            f"SELECT `name` FROM `{auth}`.`{REALMLIST_TABLE}` WHERE `id` = {REALM_ID};",
        )
    except Refusal:
        return None
    name = out.strip("\r\n").replace("\\\\", "\\")
    return name or None


# ---------------------------------------------------------------- the dumps


def _private_open(path: str, flags: int) -> int:
    """Created 0600 and never reused: the dumps hold every verifier."""
    return os.open(path, flags | os.O_EXCL, 0o600)


def _only_dump_preamble(prefix: bytes) -> bool:
    """Is everything above the banner a comment a dump itself would write? (Yu'lon's rule.)"""
    index, end = 0, len(prefix)
    while index < end:
        if prefix[index : index + 1].isspace():
            index += 1
        elif prefix.startswith(b"--", index):
            newline = prefix.find(b"\n", index)
            if newline == -1:
                return True
            index = newline + 1
        elif prefix.startswith(b"/*", index):
            close = prefix.find(b"*/", index + 2)
            if close == -1:
                return False
            index = close + 2
        else:
            return False
    return True


def _banner_is_the_files_own(head: bytes) -> bool:
    match = _DUMP_HEADER.search(head)
    return match is not None and _only_dump_preamble(head[: match.start()])


def _databases_named_in(data: bytes) -> set[str]:
    names = {m.group(1).decode("utf-8", "replace") for m in _USE_LINE.finditer(data)}
    names |= {
        m.group(1).decode("utf-8", "replace") for m in _CREATE_DB_LINE.finditer(data)
    }
    return names


def _verify(path: Path, database: str) -> None:
    """Yu'lon's `verify_dump` on the file: non-empty, a dump's banner, the end marker, the name."""
    size = path.stat().st_size
    with path.open("rb") as fh:
        head = fh.read(_EDGE_BYTES)
        fh.seek(max(size - _EDGE_BYTES, 0))
        tail = fh.read(_EDGE_BYTES)
    if size == 0 or not _banner_is_the_files_own(head):
        raise Refusal(
            f"The dump of {database} does not start like a mysqldump. Nothing was packed."
        )
    if _DUMP_TRAILER not in tail:
        raise Refusal(
            f"The dump of {database} stops before mysqldump's end marker, so it was cut short. "
            "Nothing was packed."
        )
    if database not in _databases_named_in(head):
        raise Refusal(f"The dump does not name {database}. Nothing was packed.")


def _tables_of(path: Path, database: str) -> list[str]:
    """The tables and views of one database in a dump, read as Yu'lon's `tables_in_copy` does."""
    found: dict[str, list[str]] = {}
    current: str | None = None
    with path.open("rb") as fh:
        for line in fh:
            if line.startswith(b"USE "):
                use = _USE_AT_START.match(line)
                if use is not None:
                    current = use.group(1).decode("utf-8", "replace")
                    found.setdefault(current, [])
                continue
            if current is None or not line.startswith((b"CREATE ", b"/*!")):
                continue
            table = _TABLE_LINE.match(line)
            if table is not None:
                name = table.group(1).decode("utf-8", "replace").replace("``", "`")
                if name not in found[current]:
                    found[current].append(name)
    tables = found.get(database, [])
    if not tables:
        raise Refusal(f"The dump of {database} lists no table, so nothing was packed.")
    return tables


def _dump(access: Access, database: str, target: Path) -> list[str]:
    """One database through mysqldump into `target`, with the record in front (T603's line)."""
    ignore = (
        [f"--ignore-table={database}.{REALMLIST_TABLE}"]
        if database == SCHEMA["auth"]
        else []
    )
    argv = access.argv(
        "mysqldump", ["-uroot", *DUMP_FLAGS, *ignore, "--databases", database]
    )
    record = (GAME_RECORD_TEMPLATE.format(game=GAME_ID) + "\n").encode("ascii")
    with os.fdopen(_private_open(str(target), os.O_WRONLY | os.O_CREAT), "wb") as sink:
        sink.write(record)
        sink.flush()
        proc = subprocess.run(
            argv, stdout=sink, stderr=subprocess.PIPE, env=access.env, check=False
        )
    if proc.returncode != 0:
        raise Refusal(
            f"mysqldump could not dump {database} ({_text(proc.stderr).strip()[:300]}). "
            "Nothing was packed."
        )
    if target.stat().st_size == len(record):
        raise Refusal(f"mysqldump wrote nothing for {database}. Nothing was packed.")
    _verify(target, database)
    return _tables_of(target, database)


# ---------------------------------------------------------------- the package


def _stamp(made: datetime) -> str:
    return f"{made:%Y%m%d-%H%M}"


def _manifest_text(
    made: datetime,
    members: list[dict[str, object]],
    evidence: dict[str, dict[str, object]],
    realm: str | None,
    counts: tuple[int, int, int, int],
    bot_prefix: str,
    excluded: list[str],
) -> str:
    """The manifest in Yu'lon's own layout: the field order and the indent of its model."""
    accounts, characters, bot_accounts, bot_characters = counts
    manifest: dict[str, object] = {
        "format": FORMAT,
        "kind": "characters",
        "made_by": {"yulon": SCRIPT_TAG, "platform": "linux", "made": made.isoformat()},
        "game": {"id": GAME_ID, "name": GAME_NAME},
        "databases": members,
        "schema_evidence": evidence,
        "realm_name": realm,
        "channel_account": None,
        "bot_prefix": bot_prefix,
        "counts": {
            "accounts": accounts,
            "characters": characters,
            "bot_accounts": bot_accounts,
            "bot_characters": bot_characters,
        },
        "excluded": excluded,
        "secrets": SECRETS,
        "server": None,
    }
    return json.dumps(manifest, indent=2, ensure_ascii=False)


def _write_package(dest: Path, manifest: str, dumps: list[tuple[str, Path]]) -> None:
    """Write `dest` through a `.partial`, read it back against the manifest, then rename it."""
    # A leftover `.partial` goes first: reopening it would keep whatever mode it was made with.
    partial = dest.with_name(dest.name + ".partial")
    partial.unlink(missing_ok=True)
    with (
        os.fdopen(
            os.open(partial, os.O_RDWR | os.O_CREAT | os.O_EXCL, 0o600), "w+b"
        ) as raw,
        zipfile.ZipFile(
            raw, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6, allowZip64=True
        ) as archive,
    ):
        archive.writestr(MANIFEST_NAME, manifest)
        for schema, path in dumps:
            with (
                path.open("rb") as source,
                archive.open(f"db/{schema}.sql", "w", force_zip64=True) as sink,
            ):
                shutil.copyfileobj(source, sink, CHUNK)
    _read_back(partial, manifest)
    os.replace(partial, dest)


def _sha256_of(path: Path) -> str:
    """SHA-256 of a file read in chunks, so a big dump is never held whole in memory."""
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        while chunk := fh.read(CHUNK):
            digest.update(chunk)
    return digest.hexdigest()


def _read_back(partial: Path, manifest: str) -> None:
    """Every member of the written file is checked against the manifest it carries."""
    listed = {
        m["file"]: (m["sha256"], m["bytes"]) for m in json.loads(manifest)["databases"]
    }
    with zipfile.ZipFile(partial) as archive:
        if sorted(archive.namelist()) != sorted([MANIFEST_NAME, *listed]):
            raise Refusal(
                "The package did not read back as it was written. Nothing was packed."
            )
        if archive.read(MANIFEST_NAME).decode("utf-8") != manifest:
            raise Refusal(
                "The package's manifest did not read back as it was written. "
                "Nothing was packed."
            )
        for name, (digest, size) in listed.items():
            hashed = hashlib.sha256()
            length = 0
            with archive.open(name) as member:
                while chunk := member.read(CHUNK):
                    hashed.update(chunk)
                    length += len(chunk)
            if hashed.hexdigest() != digest or length != size:
                raise Refusal(
                    "A part of the package did not read back as it was written. "
                    "Nothing was packed."
                )


def _count_phrase(counts: tuple[int, int, int, int]) -> str:
    accounts, characters, bot_accounts, bot_characters = counts

    def noun(n: int, word: str) -> str:
        return f"{n} {word}" if n == 1 else f"{n} {word}s"

    text = f"{noun(accounts, 'account')} and {noun(characters, 'character')} of players"
    if bot_accounts:
        bots = noun(bot_accounts, "bot account")
        text += f", plus {bots} with {noun(bot_characters, 'bot character')}"
    return text


# ---------------------------------------------------------------- the run


def pack(
    server_dir: Path | None, out_dir: Path, stopped_flag: bool
) -> tuple[Path, tuple[int, int, int, int]]:
    access = _choose_access(server_dir, stopped_flag)
    password = _root_password(server_dir)
    access.env = {**os.environ, "MYSQL_PWD": password}
    databases = _databases(access)
    wanted: list[tuple[str, str]] = []
    for role in ROLES:
        schema = SCHEMA[role]
        if schema in databases:
            wanted.append((role, schema))
    schemas = dict(wanted)
    for role in EVIDENCE_ROLES:
        if role not in schemas:
            raise Refusal(
                f"This server has no {SCHEMA[role]}, so there are no accounts and characters to "
                "pack. Nothing was packed."
            )
    evidence: dict[str, dict[str, object]] = {}
    for role, schema in wanted:
        version = _evidence(access, schema)
        if version is None:
            if role in EVIDENCE_ROLES:
                raise Refusal(
                    f"The database version of {schema} could not be read, so a package made from "
                    "it could not be checked on the other computer. Nothing was packed."
                )
            continue
        evidence[schema] = version
    prefix = _bot_prefix(server_dir)
    counts = _counts(access, schemas["auth"], schemas["characters"], prefix)
    realm = _realm_name(access, schemas["auth"])
    excluded = [
        f"{name} (not part of accounts and characters)"
        for name in databases
        if name not in schemas.values() and name not in SYSTEM_SCHEMAS
    ]
    excluded.append(f"{REALMLIST_TABLE} of {schemas['auth']} (the realm's own address)")

    made = datetime.now()
    dest = out_dir / f"yulon-move-{GAME_ID}-{_stamp(made)}-{KEEP_PRIVATE}.zip"
    if dest.exists():
        raise Refusal(
            f"{dest.name} already exists, so the script did not overwrite it."
        )
    work = Path(tempfile.mkdtemp(prefix=".yulon-move-work-", dir=out_dir))
    try:
        members: list[dict[str, object]] = []
        dumps: list[tuple[str, Path]] = []
        for role, schema in wanted:
            target = work / f"{schema}.sql"
            tables = _dump(access, schema, target)
            digest = _sha256_of(target)
            members.append(
                {
                    "schema": schema,
                    "role": role,
                    "file": f"db/{schema}.sql",
                    "bytes": target.stat().st_size,
                    "sha256": digest,
                    "tables": tables,
                }
            )
            dumps.append((schema, target))
        manifest = _manifest_text(
            made, members, evidence, realm, counts, prefix, excluded
        )
        _write_package(dest, manifest, dumps)
    finally:
        shutil.rmtree(work, ignore_errors=True)
    return dest, counts


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Pack a WotLK server's accounts and characters for a move to Yu'lon."
    )
    parser.add_argument(
        "--server-dir",
        type=Path,
        help="the server folder (for its password " "and bot prefix)",
    )
    parser.add_argument(
        "--out", type=Path, help="the folder for the package (default: Documents)"
    )
    parser.add_argument(
        "--server-stopped",
        action="store_true",
        help="say the server is stopped (needed when only the MySQL client is "
        "there)",
    )
    args = parser.parse_args(argv)
    out_dir = args.out or (
        Path.home() / "Documents"
        if (Path.home() / "Documents").is_dir()
        else Path.home()
    )
    try:
        if not out_dir.is_dir():
            raise Refusal(f"The folder {out_dir} does not exist. Nothing was packed.")
        dest, counts = pack(args.server_dir, out_dir, args.server_stopped)
    except Refusal as exc:
        print(str(exc), file=sys.stderr)
        return 2
    print(f"Packed {_count_phrase(counts)} into {dest}.")
    distro = os.environ.get("WSL_DISTRO_NAME")
    if distro:
        print(
            f"In Windows Explorer: \\\\wsl.localhost\\{distro}{str(dest).replace('/', chr(92))}"
        )
    print(
        "Keep it private. On the new computer, open a fresh WotLK server's Maintenance tab in "
        "Yu'lon and choose Bring in accounts and characters."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
