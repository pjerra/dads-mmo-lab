"""Read-only readings of the owner's Tortoise databases, through the app's own seams.

Every statement here is a SELECT / SHOW. Nothing is written, nothing is created,
nothing is dropped. `sys.argv[1]` is the label the output is filed under.
"""

from __future__ import annotations

import subprocess
import sys
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
password = (SERVER / (entry.install.password.file or ".db_password")).read_text().strip()

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
        "idx_owner_bot_event present",
        entry.databases.characters,
        "SELECT COUNT(*) FROM information_schema.statistics WHERE table_schema='tw_char' "
        "AND table_name='ai_playerbot_random_bots' AND index_name='idx_owner_bot_event'",
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
        ["docker", "exec", "-i", spec.db, client, "-uroot", f"-p{password}", "-N", "-B", schema],
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


print(f"=== readings: {LABEL} ===")
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
    if line.startswith(("tortoise-", "vanilla-", "tbc-", "rehearsal-", "r6")):
        print(f"  > {line}")
