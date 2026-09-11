"""Read-only readings of the owner's Tortoise databases, through the app's own seams.

Every statement here is a SELECT / SHOW. Nothing is written, nothing is created,
nothing is dropped. `sys.argv[1]` is the label the output is filed under.

T14's `readings.py` handed the client `-p<password>` on the command line, which
put the install's password in an argv (its reviewer said so). Here the client is
given it through `MYSQL_PWD` in the container's environment -- `docker exec -e
MYSQL_PWD` forwards the NAME and docker copies the value out of this process's
environment -- so the password is in no argv and in no line this script prints.
"""

from __future__ import annotations

import os
import subprocess
import sys
from datetime import datetime
from pathlib import Path

from yulon.catalog.catalog import load_catalog
from yulon.catalog.families import sqlplan

SERVER = Path("/home/pk/tortoise-server")
LABEL = sys.argv[1]

entry = load_catalog().get("wow-tortoise")
spec = entry.container_spec()
native_block = entry.install.native
assert native_block is not None and native_block.cmangos is not None
plan = native_block.cmangos.sql
client = native_block.db.client
os.environ["MYSQL_PWD"] = (
    (SERVER / (entry.install.password.file or ".db_password")).read_text().strip()
)

QUERIES: list[tuple[str, str, str]] = [
    (
        "tw_char table count",
        entry.databases.characters,
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='tw_char'",
    ),
    (
        "character_inventory_copy present",
        entry.databases.characters,
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='tw_char' "
        "AND table_name='character_inventory_copy'",
    ),
    (
        "guild_bank_money.money column type",
        entry.databases.characters,
        "SELECT COLUMN_TYPE FROM information_schema.columns WHERE table_schema='tw_char' "
        "AND table_name='guild_bank_money' AND column_name='money'",
    ),
    (
        "ai_playerbot_random_bots indexes",
        entry.databases.characters,
        "SELECT DISTINCT INDEX_NAME FROM information_schema.statistics "
        "WHERE table_schema='tw_char' AND table_name='ai_playerbot_random_bots' "
        "ORDER BY INDEX_NAME",
    ),
    (
        "idx_owner_bot_event columns",
        entry.databases.characters,
        "SELECT INDEX_NAME, SEQ_IN_INDEX, COLUMN_NAME, NON_UNIQUE "
        "FROM information_schema.statistics WHERE table_schema='tw_char' "
        "AND table_name='ai_playerbot_random_bots' AND index_name='idx_owner_bot_event' "
        "ORDER BY SEQ_IN_INDEX",
    ),
    (
        "character count",
        entry.databases.characters,
        "SELECT COUNT(*) FROM characters",
    ),
    (
        "guild_bank_money rows and minimum balance",
        entry.databases.characters,
        "SELECT COUNT(*), MIN(money) FROM guild_bank_money",
    ),
    (
        "account count",
        entry.databases.auth,
        "SELECT COUNT(*) FROM account",
    ),
    (
        f"{sqlplan.MARKER_TABLE} anywhere on this server",
        plan.marker_db,
        "SELECT table_schema, table_name FROM information_schema.tables "
        f"WHERE table_name='{sqlplan.MARKER_TABLE}'",
    ),
    (
        f"{sqlplan.MARKER_TABLE} row",
        plan.marker_db,
        f"SELECT * FROM {sqlplan.MARKER_TABLE}",
    ),
    (
        f"{sqlplan.MARKER_TABLE} table definition",
        plan.marker_db,
        f"SELECT COLUMN_NAME, COLUMN_TYPE FROM information_schema.columns "
        f"WHERE table_schema='{plan.marker_db}' AND table_name='{sqlplan.MARKER_TABLE}' "
        f"ORDER BY ORDINAL_POSITION",
    ),
]


def ask(schema: str, query: str) -> str:
    proc = subprocess.run(
        ["docker", "exec", "-i", "-e", "MYSQL_PWD", spec.db, client, "-uroot", "-N", "-B", schema],
        input=query + ";\n",
        capture_output=True,
        text=True,
    )
    out = (proc.stdout or "").strip()
    err = "\n".join(
        line
        for line in (proc.stderr or "").splitlines()
        if "Using a password on the command line" not in line
    ).strip()
    return out if not err else f"{out}\n!! {err}"


print(f"=== readings: {LABEL} ===  {datetime.now().astimezone():%Y-%m-%d %H:%M:%S %Z}")
for name, schema, query in QUERIES:
    print(f"\n[{name}]  ({schema})")
    print(f"  Q: {query}")
    for line in ask(schema, query).splitlines() or [""]:
        print(f"  > {line}")

state = SERVER / ".yulon-install.json"
print("\n[.yulon-install.json]")
for line in state.read_text().splitlines():
    print(f"  > {line}")

print("\n[containers]")
ps = subprocess.run(
    ["docker", "ps", "-a", "--format", "{{.Names}}|{{.Status}}"], capture_output=True, text=True
)
for line in ps.stdout.splitlines():
    print(f"  > {line}")
