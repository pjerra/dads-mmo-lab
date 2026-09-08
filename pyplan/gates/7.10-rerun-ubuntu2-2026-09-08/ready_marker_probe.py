"""Which arguments make `wait_server_ready()` answer for an AzerothCore install — A against B.

WHAT THIS IS FOR. `wait_ready.py` was run with the arguments
`pyplan/gates/gate-79-controller-surface.py:150` uses for this game —
`wait_server_ready("127.0.0.1", auth_port)` — and it did not come back. This probe reads the
two markers that wait is really looking for, out of the two container logs, and then runs the
SAME function with the arguments the realm row says, so the difference is a measurement rather
than an argument.

`docker.azerothcore_ready(realm_host, realm_port)` builds a `ReadySpec` whose `world` marker is
the literal `ready...` and whose `auth` marker is `f"{realm_host}:{realm_port}"`
(`pylauncher/yulon/docker.py:2736-2741`). The authserver prints that pair in one line only —
`Added realm "<name>" at <address>:<port>.` — and both halves of it come from
`acore_auth.realmlist`: the ADDRESS column and the PORT column, which is the world port. So
`("127.0.0.1", auth_port)` names a string no AzerothCore authserver ever prints.

Nothing here writes anything. Both calls are read-only polls of two container logs.
"""

from __future__ import annotations

import re
import subprocess
import time
from pathlib import Path

from yulon.catalog.catalog import load_catalog
from yulon.controller_wow_wotlk import docker_ctl


def docker_logs(container: str) -> str:
    return subprocess.run(
        ["docker", "logs", container], capture_output=True, text=True
    ).stdout + subprocess.run(
        ["docker", "logs", container], capture_output=True, text=True
    ).stderr


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
    for line in sorted({m.group(0) for m in re.finditer(r'at [0-9.]+:[0-9]+', auth_log)}):
        print(f"  {line}", flush=True)
    print(f"  occurrences of '127.0.0.1:{auth_port}' : "
          f"{auth_log.count(f'127.0.0.1:{auth_port}')}", flush=True)
    print(f"  occurrences of '{address}:{realm_port}' : "
          f"{auth_log.count(f'{address}:{realm_port}')}", flush=True)
    print(f"\n--- the WORLD marker 'ready...' occurrences: {world_log.count('ready...')}",
          flush=True)

    print(f"\n=== B: wait_server_ready({address!r}, {realm_port}) -- the realm row's own pair ===",
          flush=True)
    started = time.monotonic()
    ready = bool(docker_ctl.wait_server_ready(address, int(realm_port)))
    took = time.monotonic() - started
    print(f"ready={ready} after {took:.1f}s", flush=True)
    return 0 if ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
