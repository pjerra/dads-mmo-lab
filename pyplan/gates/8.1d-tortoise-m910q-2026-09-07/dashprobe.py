"""Live proof of the recreate fix, on m910q, against a real docker daemon.

The unit tests hand `Dashboard` container states. This asks the daemon for them,
and it asks about a stand-in container rather than the Tortoise worldserver:
what is in question is docker's own bookkeeping across a recreate, and a busybox
answers that in seconds where a worldserver costs a quarter of an hour of map
loading and would disturb the account waiting for 8.1d's last clause.

The dashboard object is the SAME one across the recreate -- that is the whole
defect. A fresh one always read `up`.

Usage:  python dashprobe.py
"""

from __future__ import annotations

import subprocess
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate81d" / "pylauncher"))

from yulon import dashboard, docker  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402

NAME = "dash-probe"
ENTRY = load_catalog().get("wow-tortoise")
SERVER_DIR = Path.home() / "tortoise-server"
SPEC = docker.ContainerSpec(db=NAME, auth=NAME, world=NAME, ports=())


class FakeSql:
    """The population is not what is being measured; the verdict's STATE is."""

    def query(self, db: str, statement: str) -> str:
        return "0\t510\t510\t900\n"


def say(text: str) -> None:
    print(f"[{datetime.now(UTC).strftime('%H:%M:%SZ')}] {text}", flush=True)


def run(*argv: str) -> str:
    proc = subprocess.run(list(argv), capture_output=True, text=True, timeout=120)
    return (proc.stdout + proc.stderr).strip()


def crash_looping() -> None:
    """A container that exits at once under `unless-stopped`: a real restart loop."""
    run("docker", "rm", "-f", NAME)
    run(
        "docker", "run", "-d", "--name", NAME, "--restart", "unless-stopped",
        "busybox:latest", "sh", "-c", "exit 1",
    )


def healthy() -> None:
    """The recreate: same name, new container, and docker's count starts over."""
    run("docker", "rm", "-f", NAME)
    run(
        "docker", "run", "-d", "--name", NAME, "--restart", "unless-stopped",
        "busybox:latest", "sh", "-c", "while true; do sleep 5; done",
    )


def main() -> None:
    watch = dashboard.Dashboard(SPEC, ENTRY, SERVER_DIR, sql=FakeSql())
    try:
        say("starting a container that exits as soon as it runs")
        crash_looping()
        for poll in range(1, 13):
            time.sleep(5)
            verdict = watch.tick()
            state = docker.container_state(NAME)
            say(
                f"poll {poll:2}  : {dashboard.line(verdict)}   "
                f"[state={verdict.state} docker={state.status!r}/{state.restart_count}]"
            )
            if verdict.state == "restart_loop" and poll >= 3:
                break
        assert verdict.state == "restart_loop", "the stand-in never looped; nothing was proved"

        say("recreating the container -- same name, new container")
        healthy()
        after = docker.container_state(NAME)
        say(f"docker now says: status={after.status!r} restart_count={after.restart_count}")

        say("the SAME dashboard object, ticking on:")
        for poll in range(1, 5):
            time.sleep(5)
            verdict = watch.tick()
            say(
                f"poll {poll:2}  : {dashboard.line(verdict)}   "
                f"[state={verdict.state} stable={verdict.stable}]"
            )
        fresh = dashboard.Dashboard(SPEC, ENTRY, SERVER_DIR, sql=FakeSql()).tick()
        say(f"a dashboard made fresh at this moment: {dashboard.line(fresh)}")
        assert verdict.state == "up", f"still called {verdict.state} after the restart"
        assert fresh.state == verdict.state, "the old watcher and a fresh one disagree"
        assert not verdict.stable, "a reset count is not evidence the crash cause is gone"
        assert "crash" in dashboard.line(verdict), "the tab must say why it is holding back"
        say("PROVED: no longer called a loop, and not yet called steady either")
    finally:
        run("docker", "rm", "-f", NAME)
        say(f"cleaned up: {NAME} removed")


if __name__ == "__main__":
    main()
