"""Ask the install's own import gate what it reads, read-only, before any press.

`MarkerGate.probe()` is exactly what `_only_the_rerunnable_phases()` calls; asking
it here says in advance whether the press proceeds or refuses, and writes nothing.
Also locates the marker table across every schema on the server.
"""

from __future__ import annotations

import subprocess
from pathlib import Path

from yulon.catalog import native
from yulon.catalog.catalog import load_catalog
from yulon.catalog.families import sqlplan
from yulon.install_wiring import installer_for_app

SERVER = Path("/home/pk/tortoise-server")
entry = load_catalog().get("wow-tortoise")
spec = entry.container_spec()
native_block = entry.install.native
assert native_block is not None and native_block.cmangos is not None
plan = native_block.cmangos.sql
db = native_block.db
password = (SERVER / (entry.install.password.file or ".db_password")).read_text().strip()

print("MARKER_TABLE:", sqlplan.MARKER_TABLE, " marker_db:", plan.marker_db)
found = subprocess.run(
    ["docker", "exec", "-i", spec.db, db.client, "-uroot", f"-p{password}", "-N", "-B"],
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
