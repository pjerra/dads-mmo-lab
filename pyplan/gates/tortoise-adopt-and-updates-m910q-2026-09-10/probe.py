"""Ask the install's own gates what they read, read-only, before any press.

`MarkerGate.probe()` is exactly what `_only_the_rerunnable_phases()` and
`stage_adopt()` call, and `adoption_gaps()` is the presence check the adopt
press makes after it; asking them here says in advance whether each press
proceeds or refuses, and writes nothing. `adopt_state()` is the reading the
Modules tab's button rule is built on.

The password reaches the client through `MYSQL_PWD` in the container's
environment, never an argv.
"""

from __future__ import annotations

import os
import subprocess
from datetime import datetime
from pathlib import Path

from yulon.catalog import native
from yulon.catalog.catalog import load_catalog
from yulon.catalog.families import sqlplan
from yulon.catalog.installer import InstallOptions
from yulon.install_wiring import installer_for_app

SERVER = Path("/home/pk/tortoise-server")
entry = load_catalog().get("wow-tortoise")
spec = entry.container_spec()
native_block = entry.install.native
assert native_block is not None and native_block.cmangos is not None
plan = native_block.cmangos.sql
db = native_block.db
os.environ["MYSQL_PWD"] = (
    (SERVER / (entry.install.password.file or ".db_password")).read_text().strip()
)

print(f"=== probe ===  {datetime.now().astimezone():%Y-%m-%d %H:%M:%S %Z}")
print("MARKER_TABLE:", sqlplan.MARKER_TABLE, " marker_db:", plan.marker_db)
found = subprocess.run(
    ["docker", "exec", "-i", "-e", "MYSQL_PWD", spec.db, db.client, "-uroot", "-N", "-B"],
    input=(
        "SELECT table_schema, table_name FROM information_schema.tables "
        f"WHERE table_name='{sqlplan.MARKER_TABLE}';\n"
    ),
    capture_output=True,
    text=True,
)
print("marker table anywhere on this server:", repr(found.stdout.strip()))

engine = installer_for_app(entry)
ctx = engine._update_context(SERVER, None)
print("ctx.updates_only:", ctx.updates_only)
answer = engine._gate(ctx).probe()
print("probe answer:", answer.state, "| complete:", answer.complete, "| detail:", answer.detail)
print("import_reads_as_finished:", native.import_reads_as_finished(answer))
print("adoption_gaps:", engine.adopt_gate(ctx).adoption_gaps())
print("adopt_state (the button's reading):", engine.adopt_state(InstallOptions(server_dir=SERVER)))
print("marker_row:", engine.marker_row())
