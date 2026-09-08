"""Wait for this install's own ready marker, and print how long it took.

This replaces the `sleep 180` the 2026-09-05 runner put between the restore driver and the
rest. That folder's own README records what the blind sleep cost: `sweep_driver2.py` asked
`server info` 44 s after a start and the world finished initializing 10.8 s AFTER the call, so
the console check passed on a reply with no lines in it. A fixed sleep answers the same
whether the world is up or not; `wait_server_ready()` is the question itself.

It is the game's own spelling, taken from the same table `gate-79-controller-surface.py` uses
rather than invented here: WotLK's `wait_server_ready()` takes a realm host and port.
"""

from __future__ import annotations

import sys
import time
from pathlib import Path

from yulon.catalog.catalog import load_catalog
from yulon.controller_wow_wotlk import docker_ctl


def main() -> int:
    entry = load_catalog().get("wow-wotlk")
    auth_port = entry.container_spec().ports[0]
    print(f"waiting for wow-wotlk's ready marker on 127.0.0.1:{auth_port} "
          f"({time.strftime('%FT%T%z')})", flush=True)
    started = time.monotonic()
    ready = bool(docker_ctl.wait_server_ready("127.0.0.1", auth_port))
    took = time.monotonic() - started
    print(f"ready={ready} after {took:.1f}s ({time.strftime('%FT%T%z')})", flush=True)
    return 0 if ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
