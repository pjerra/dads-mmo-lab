"""Wait for this install's own ready marker, and print how long it took.

This replaces the `sleep 180` the 2026-09-05 runner put between the restore driver and the
rest. That folder's own README records what the blind sleep cost: `sweep_driver2.py` asked
`server info` 44 s after a start and the world finished initializing 10.8 s AFTER the call, so
the console check passed on a reply with no lines in it. A fixed sleep answers the same
whether the world is up or not; `wait_server_ready()` is the question itself.

**AMENDED 2026-09-09, and the run this folder records was made BEFORE the amendment.**
`ready-after-restore.txt` is what the version below it printed: it typed
`wait_server_ready("127.0.0.1", auth_port)` -- the spelling
`gate-79-controller-surface.py` used for this game -- and was killed after 8 m 19 s against a
world that had been up the whole time. `ready-marker-probe.txt`, run in the same minute on
the same server, got `ready=True` in 0.3 s from the realm row's own pair. Those two files are
the measurement and are unchanged; this script is the fix, so that the next run of
`run-710-rerun.sh` asks the question the right way instead of re-measuring a known answer.

It no longer has a spelling of its own. Copying gate-79's WotLK row by hand is what put the
same defect in two places at once, so both now import `pyplan/gates/ready_wait.py` -- whose
module docstring carries the measurement, the reason the other three games were unaffected,
and the sibling fix it must not undo.
"""

from __future__ import annotations

import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from ready_wait import realm_pair, wait_ready_for_game  # noqa: E402

from yulon.catalog.catalog import load_catalog  # noqa: E402

SERVER_DIR = Path("/home/pk/wowserver")
"""The install this lane drives, the same one `log_panel_stop_probe.py` names.

Needed now that the wait reads this install's own realm row: the password comes from the
entry and the row comes from the database the server dir identifies.
"""


def main() -> int:
    entry = load_catalog().get("wow-wotlk")
    # THE PAIR IS READ AND PRINTED BEFORE THE WAIT, not inside it. The old line
    # here said "on 127.0.0.1:3724" and it was the only clue in eight minutes of
    # silence; a run that prints what it is waiting for says on its first line
    # whether the wait can ever match. Read once and handed on, so the log and
    # the marker cannot be about different rows.
    address, port = realm_pair(entry, SERVER_DIR)
    print(
        f"{entry.id} advertises {address}:{port} in its realm row; waiting for its own "
        f"ready marker ({time.strftime('%FT%T%z')})",
        flush=True,
    )
    started = time.monotonic()
    ready = wait_ready_for_game(entry, SERVER_DIR, pair=lambda: (address, port))
    took = time.monotonic() - started
    print(f"ready={ready} after {took:.1f}s ({time.strftime('%FT%T%z')})", flush=True)
    return 0 if ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
