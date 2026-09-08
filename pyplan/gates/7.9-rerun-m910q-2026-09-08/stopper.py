"""Stop one game's stack through the app's own Stop button path, between games.

m910q's rule is ONE server at a time, and this is the seam that enforces it here:
`Controller.stop()` -- the same callable the Server tab's Stop button binds -- and not a
hand-typed `docker stop`, because an un-ordered `docker stop` of all three containers is the
exact shape that made `tbc-mangosd` exit 139 on 2026-09-04 (checklist 7.9, mechanism 1).

The status is printed BEFORE and AFTER, so a stop over an already-stopped stack is visible as
one rather than reading like a stop that worked.
"""

from __future__ import annotations

import sys
import time
from pathlib import Path

from yulon.catalog.catalog import load_catalog
from yulon.ui.controller_view import ControllerServices


def main() -> int:
    game, server_dir = sys.argv[1], Path(sys.argv[2])
    entry = load_catalog().get(game)
    services = ControllerServices.for_entry(entry, server_dir)
    print(f"=== stopping {game} at {server_dir}  {time.strftime('%FT%T%z')} ===", flush=True)
    before = services.controller.status()
    print(f"status before: db={before.db} auth={before.auth} world={before.world}", flush=True)
    started = time.monotonic()
    returned = services.controller.stop()
    took = time.monotonic() - started
    after = services.controller.status()
    print(f"stop() -> {returned} in {took:.1f}s", flush=True)
    print(f"status after : db={after.db} auth={after.auth} world={after.world}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
