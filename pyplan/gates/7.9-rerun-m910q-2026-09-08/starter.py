"""Start one game's stack through the Start button's own path, and wait for ITS ready marker.

The photograph pass needs a server that is up AND loaded: a frame of a Console tab taken
against a worldserver still printing its load progress is a picture of a scheduling failure,
which is the exact mistake the 2026-09-04 TBC run made and the exact mistake a blind sleep
would repeat here. So the wait is `wait_server_ready()`, in this game's own spelling, taken
from the same table `gate-79-controller-surface.py` keeps rather than invented again.
"""

from __future__ import annotations

import sys
import time
from pathlib import Path

# AMENDED 2026-09-09. The four spellings differ for real, and the table used to be COPIED here
# -- "copied verbatim from gate-79-controller-surface.py:143-153 so the two passes cannot drift
# apart", which is what a copy always promises. Three scripts held that copy and all three held
# the same defect: the WotLK row typed a realm address, and WotLK is the one game whose ready
# marker IS that address. Measured on yulon-ubuntu2 2026-09-09 in the 7.10 folder -- 8 m 19 s
# of waiting against a world that was up, against 0.3 s from the realm row's own pair. The
# table now lives in pyplan/gates/ready_wait.py and is imported, not copied; its module
# docstring carries the measurement.
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from ready_wait import READY_CALLS, wait_ready_for_game  # noqa: E402

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402


def main() -> int:
    game, server_dir = sys.argv[1], Path(sys.argv[2])
    if game not in READY_CALLS:
        print(f"[FAIL] no wait_server_ready() spelling wired for {game!r}", flush=True)
        return 2
    entry = load_catalog().get(game)
    services = ControllerServices.for_entry(entry, server_dir)

    before = services.controller.status()
    print(f"status before start: db={before.db} auth={before.auth} world={before.world}",
          flush=True)
    started = time.monotonic()
    services.controller.start()
    print(f"start() returned in {time.monotonic() - started:.1f}s", flush=True)
    after = services.controller.status()
    print(f"status after start : db={after.db} auth={after.auth} world={after.world}", flush=True)

    started = time.monotonic()
    ready = wait_ready_for_game(entry, server_dir)
    print(f"ready={ready} after {time.monotonic() - started:.1f}s", flush=True)
    return 0 if ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
