"""Read the worldserver's own log for THIS game's ready marker, read-only.

`sys.argv[1]` is a UNIX epoch second (`date +%s`), `sys.argv[2]` the label. The
window is given to `docker logs --since` as an epoch, never as a zone-less
stamp: a zone-less value is read in the CLIENT's local zone, which on this
+02:00 box opens the window two hours early (`docker-logs-since-is-client-local`).

The marker pattern is not typed here -- it is the catalog entry's own
`ready.world` regex, the one `Controller.start()` waits for.

Run BEFORE the start as well as after: the same query over the same window
returning nothing is what proves the line found afterwards belongs to this
start and is not a leftover from an earlier one.
"""

from __future__ import annotations

import re
import subprocess
import sys
from datetime import datetime

from yulon.catalog.catalog import load_catalog

SINCE = sys.argv[1]
LABEL = sys.argv[2]

entry = load_catalog().get("wow-tortoise")
spec = entry.container_spec()
ready = entry.install.native.ready if entry.install.native is not None else None
pattern = ready.world if ready is not None else None
assert pattern, "this entry declares no world ready marker"

print(f"=== worldlog: {LABEL} ===  {datetime.now().astimezone():%Y-%m-%d %H:%M:%S %Z}")
print("container:", spec.world)
print("--since (epoch seconds):", SINCE)
print("ready.world pattern (from the catalog entry):", pattern)

proc = subprocess.run(
    ["docker", "logs", "--since", SINCE, spec.world],
    capture_output=True,
    text=True,
)
lines = (proc.stdout or "") + (proc.stderr or "")
rows = lines.splitlines()
print("lines in the window:", len(rows))
matches = [row for row in rows if re.search(pattern, row)]
print("ready-marker matches in the window:", len(matches))
for row in matches:
    print("  MARKER >", row.rstrip())
print("last 12 lines in the window:")
for row in rows[-12:]:
    print("  >", row.rstrip())
