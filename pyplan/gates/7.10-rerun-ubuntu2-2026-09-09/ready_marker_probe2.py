"""Which arguments make `wait_server_ready()` answer for this AzerothCore install — A, B and C.

2026-09-08's finding 2 measured that `gate-79-controller-surface.py`'s WotLK row typed
`wait_server_ready("127.0.0.1", auth_port)`, and that this call never returns: it was killed
after 8 m 19 s against a world that had been up the whole time. `576a3f93` is the fix — the
row now asks `realm_pair()` and the table lives in `pyplan/gates/ready_wait.py`, imported by
all three scripts that used to hold hand copies of it.

This probe re-measures all of it on this box tonight, in one process against one server:

  A. `wait_server_ready("127.0.0.1", auth_port)` — the OLD, typed spelling, with a BOUNDED
     quiet budget so the ground can be read without spending eight minutes on it again. This
     is the ground: if A answered True, the fix would be proving nothing.
  B. `wait_server_ready(address, port)` from the realm row, read out of the database. The
     2026-09-08 measurement.
  C. `ready_wait.wait_ready_for_game(entry, server_dir)` — THE COMMITTED FIX ITSELF, called
     the way `gate-79-controller-surface.py` now calls it, with no pair typed anywhere in
     this file. C is what the merged tip actually does.

The occurrence counts in the authserver's own log are printed first, because they are what
decides all three: a marker that appears zero times cannot be waited for.

Nothing here writes anything. The one statement it sends is a SELECT.
"""

from __future__ import annotations

import re
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, "/home/pk/lane710b/checkout/pyplan/gates")

from ready_wait import realm_pair, wait_ready_for_game  # noqa: E402

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.controller_wow_wotlk import docker_ctl  # noqa: E402

SERVER_DIR = Path("/home/pk/wowserver")
A_BUDGET_SECONDS = 60.0
"""How long the OLD spelling is given before it is called hung.

Bounded, and the bound is stated: 2026-09-08 gave it 8 m 19 s and killed it. A ground reading
does not need to re-spend that — it needs to be long enough that "it did not answer" is not a
statement about the budget. The world's own `ready...` is already in its log four times over
and B answers in under a second, so anything past a few seconds is the marker never matching.
"""


def docker_logs(container: str) -> str:
    done = subprocess.run(["docker", "logs", container], capture_output=True, text=True)
    return done.stdout + done.stderr


def db_read(statement: str) -> str:
    return subprocess.run(
        ["docker", "exec", "ac-database", "mysql", "-uroot", "-ppassword", "-N", "-B",
         "-e", statement],
        capture_output=True, text=True,
    ).stdout.strip()


def main() -> int:
    import yulon

    print("yulon package under test:", Path(yulon.__file__).resolve(), flush=True)
    entry = load_catalog().get("wow-wotlk")
    spec = entry.container_spec()
    auth_port = spec.ports[0]

    row = db_read("SELECT address, port FROM acore_auth.realmlist WHERE id=1;")
    address, realm_port = row.split("\t")
    print(f"\nrealmlist row (read through docker exec, not through the app): "
          f"address={address!r} port={realm_port!r}", flush=True)
    print(f"catalog container_spec ports: {spec.ports}  (auth_port = {auth_port})", flush=True)

    auth_log = docker_logs(spec.auth)
    world_log = docker_logs(spec.world)
    print("\n--- what the AUTH log actually prints ---", flush=True)
    for line in sorted({m.group(0) for m in re.finditer(r"at [0-9.]+:[0-9]+", auth_log)}):
        print(f"  {line}", flush=True)
    print(f"  occurrences of '127.0.0.1:{auth_port}' : "
          f"{auth_log.count(f'127.0.0.1:{auth_port}')}", flush=True)
    print(f"  occurrences of '{address}:{realm_port}' : "
          f"{auth_log.count(f'{address}:{realm_port}')}", flush=True)
    print(f"\n--- the WORLD marker 'ready...' occurrences: {world_log.count('ready...')}",
          flush=True)

    print(f"\n=== A (THE GROUND): wait_server_ready('127.0.0.1', {auth_port}) -- the spelling "
          f"the row carried until 576a3f93, bounded at {A_BUDGET_SECONDS:.0f}s ===", flush=True)
    started = time.monotonic()
    a_ready = bool(docker_ctl.wait_server_ready("127.0.0.1", auth_port,
                                                timeout=A_BUDGET_SECONDS))
    a_took = time.monotonic() - started
    print(f"ready={a_ready} after {a_took:.1f}s", flush=True)

    print(f"\n=== B: wait_server_ready({address!r}, {realm_port}) -- the realm row's own pair "
          f"===", flush=True)
    started = time.monotonic()
    b_ready = bool(docker_ctl.wait_server_ready(address, int(realm_port)))
    b_took = time.monotonic() - started
    print(f"ready={b_ready} after {b_took:.1f}s", flush=True)

    print("\n=== C (THE FIX): ready_wait.wait_ready_for_game(entry, server_dir) -- no pair "
          "typed in this file at all ===", flush=True)
    pair = realm_pair(entry, SERVER_DIR)
    print(f"realm_pair() asked the install and got {pair!r}", flush=True)
    started = time.monotonic()
    c_ready = bool(wait_ready_for_game(entry, SERVER_DIR))
    c_took = time.monotonic() - started
    print(f"ready={c_ready} after {c_took:.1f}s", flush=True)

    print("\n" + "=" * 78, flush=True)
    print(f"A typed 127.0.0.1:{auth_port:<6} ready={a_ready!s:<5} {a_took:7.1f}s", flush=True)
    print(f"B realm row pair       ready={b_ready!s:<5} {b_took:7.1f}s", flush=True)
    print(f"C the committed fix    ready={c_ready!s:<5} {c_took:7.1f}s", flush=True)
    print("=" * 78, flush=True)
    # A must be False for B and C to mean anything: a ground that already answers True is a
    # step whose result is its start state.
    return 0 if (c_ready and b_ready and not a_ready) else 1


if __name__ == "__main__":
    raise SystemExit(main())
