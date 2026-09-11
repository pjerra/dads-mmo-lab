"""One line per container from `docker inspect`, with the box's own clock.

`sys.argv[1]` is the label the block is filed under; the block is APPENDED to
`states.txt`, so the file reads as the whole session in order. Every stamp is
read here, by this script, from the clock on the box that ran the step — never
typed (the `stamps-come-from-the-clock` rule).

`docker inspect` is read-only. This is the only docker verb this script uses.
"""

from __future__ import annotations

import subprocess
import sys
from datetime import datetime
from pathlib import Path

from yulon.catalog.catalog import load_catalog

OUT = Path("/home/pk/t19-live/out")
OUT.mkdir(parents=True, exist_ok=True)
LABEL = sys.argv[1]

entry = load_catalog().get("wow-tortoise")
spec = entry.container_spec()
FORMAT = "{{.Name}} status={{.State.Status}} running={{.State.Running}} exit={{.State.ExitCode}} started={{.State.StartedAt}} finished={{.State.FinishedAt}}"

stamp = datetime.now().astimezone().strftime("%Y-%m-%d %H:%M:%S %Z")
lines = [f"=== {LABEL} === {stamp}"]
for container in (spec.db, spec.auth, spec.world):
    proc = subprocess.run(
        ["docker", "inspect", "-f", FORMAT, container], capture_output=True, text=True
    )
    lines.append("  " + ((proc.stdout or "").strip() or f"!! {(proc.stderr or '').strip()}"))

block = "\n".join(lines) + "\n"
print(block, end="")
with (OUT / "states.txt").open("a", encoding="utf-8") as handle:
    handle.write(block)
