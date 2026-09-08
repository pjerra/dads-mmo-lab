"""Start one game's stack through the Start button's own path, and wait for ITS ready marker.

The photograph pass needs a server that is up AND loaded: a frame of a Console tab taken
against a worldserver still printing its load progress is a picture of a scheduling failure,
which is the exact mistake the 2026-09-04 TBC run made and the exact mistake a blind sleep
would repeat here. So the wait is `wait_server_ready()`, in this game's own spelling, taken
from the same table `gate-79-controller-surface.py` keeps rather than invented again.
"""

from __future__ import annotations

import importlib
import sys
import time
from collections.abc import Callable
from pathlib import Path
from types import ModuleType

from yulon.catalog.catalog import load_catalog
from yulon.ui.controller_view import ControllerServices

# The four spellings differ for real: TBC and Vanilla take no realm arguments, Tortoise and
# WotLK take a host and a port. Copied verbatim from gate-79-controller-surface.py:143-153 so
# the two passes cannot drift apart.
_READY_CALLS: dict[str, Callable[[ModuleType, int], bool]] = {
    "wow-tbc": lambda module, auth_port: bool(module.wait_server_ready()),
    "wow-vanilla": lambda module, auth_port: bool(module.wait_server_ready()),
    "wow-tortoise": lambda module, auth_port: bool(module.wait_server_ready("127.0.0.1", auth_port)),
    "wow-wotlk": lambda module, auth_port: bool(module.wait_server_ready("127.0.0.1", auth_port)),
}


def main() -> int:
    game, server_dir = sys.argv[1], Path(sys.argv[2])
    call = _READY_CALLS.get(game)
    if call is None:
        print(f"[FAIL] no wait_server_ready() spelling wired for {game!r}", flush=True)
        return 2
    entry = load_catalog().get(game)
    services = ControllerServices.for_entry(entry, server_dir)
    auth_port = entry.container_spec().ports[0]

    before = services.controller.status()
    print(f"status before start: db={before.db} auth={before.auth} world={before.world}",
          flush=True)
    started = time.monotonic()
    services.controller.start()
    print(f"start() returned in {time.monotonic() - started:.1f}s", flush=True)
    after = services.controller.status()
    print(f"status after start : db={after.db} auth={after.auth} world={after.world}", flush=True)

    module = importlib.import_module(f"yulon.controller_{game.replace('-', '_')}.docker_ctl")
    started = time.monotonic()
    ready = call(module, auth_port)
    print(f"ready={ready} after {time.monotonic() - started:.1f}s", flush=True)
    return 0 if ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
