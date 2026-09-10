"""`docker ps -a`, read-only, with the box's own clock. `sys.argv[1]` is the label."""

from __future__ import annotations

import subprocess
import sys
from datetime import datetime

print(f"=== containers: {sys.argv[1]} ===  {datetime.now().astimezone():%Y-%m-%d %H:%M:%S %Z}")
proc = subprocess.run(
    ["docker", "ps", "-a", "--format", "{{.Names}}|{{.Status}}|{{.Image}}"],
    capture_output=True,
    text=True,
)
for line in proc.stdout.splitlines():
    print("  >", line)
