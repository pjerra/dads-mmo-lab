"""The 8.1d defect, reproduced and closed against the real Tortoise worldserver.

`dashprobe.py` proved the mechanism with a busybox stand-in in forty seconds.
This does the same thing to the subject the defect was found on: the database is
taken away under the world, which is what produced a real restart loop on this
box on 2026-09-07, and then the world is brought back and the SAME dashboard
object is asked what it sees.

The screenshots are the Server tab itself, drawn offscreen, with this watcher
behind it -- the line in them is the line a person would be reading.

Usage:  python dash81d.py
"""

from __future__ import annotations

import os
import subprocess
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate81d" / "pylauncher"))

from yulon import dashboard, docker  # noqa: E402
from yulon.apply import DockerSql  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "tortoise-server"
ENTRY = load_catalog().get("wow-tortoise")
SPEC = ENTRY.container_spec()
SHOTS = Path.home() / "gate81d-fix-shots"
PASSWORD = (
    (SERVER_DIR / (ENTRY.install.password.file or "")).read_text(encoding="utf-8").strip()
    if ENTRY.install.password.file
    else ""
)


def say(text: str) -> None:
    print(f"[{datetime.now(UTC).strftime('%H:%M:%SZ')}] {text}", flush=True)


def run(*argv: str, cwd: Path | None = None, timeout: float = 180) -> str:
    proc = subprocess.run(list(argv), cwd=cwd, capture_output=True, text=True, timeout=timeout)
    return (proc.stdout + proc.stderr).strip()


def sql_seam() -> DockerSql:
    return DockerSql(
        SPEC.db,
        PASSWORD,
        schemas=ENTRY.schema_map(),
        client=ENTRY.install.native.db.client if ENTRY.install.native else None,
    )


def shot(watch: dashboard.Dashboard, name: str) -> None:
    """The Server tab as a person would see it, with THIS watcher behind it.

    One QApplication for the whole run, made once and never deleted: a second
    one built after the first was dropped takes Qt down with it.
    """
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    QApplication.instance() or QApplication([])
    services = ControllerServices.for_entry(ENTRY, SERVER_DIR)
    services.dashboard = watch.tick
    view = ControllerView(ENTRY, services, status_poll_ms=5000, job_runner=run_inline)
    SHOTS.mkdir(parents=True, exist_ok=True)
    view.resize(940, 560)
    view.grab().save(str(SHOTS / name))
    say(f"  wrote {SHOTS / name}   status={view.status_label.text()!r}")
    say(f"  verdict on the tab: {view.verdict_label.text()!r}")


def main() -> None:
    watch = dashboard.Dashboard(SPEC, ENTRY, SERVER_DIR, sql=sql_seam())
    say(f"before anything: {dashboard.line(watch.tick())}")

    say("taking the database away under the world (8.1d's own recipe)")
    run("docker", "stop", "-t", "60", SPEC.db)
    run("docker", "start", SPEC.world)
    verdict = watch.tick()
    for poll in range(1, 13):
        time.sleep(10)
        verdict = watch.tick()
        state = docker.container_state(SPEC.world)
        say(
            f"poll {poll:2}  : {dashboard.line(verdict)}   "
            f"[{verdict.state} docker={state.status!r}/{state.restart_count}]"
        )
        if verdict.state == "restart_loop" and poll >= 2:
            break
    assert verdict.state == "restart_loop", "the world never looped; nothing is being proved"
    say("the tab, mid-loop:")
    shot(watch, "1-restart-loop.png")

    say("putting it back: world down, database up, then the ordinary start")
    run("docker", "stop", "-t", "60", SPEC.world)
    run("docker", "start", SPEC.db)
    for _ in range(30):
        time.sleep(5)
        if docker.container_state(SPEC.db).status == "running":
            break
    time.sleep(20)
    out = run("docker", "compose", "up", "-d", cwd=SERVER_DIR)
    say(f"compose up -d: {out.splitlines()[-1] if out else '(no output)'}")
    after = docker.container_state(SPEC.world)
    say(f"docker now says: status={after.status!r} restart_count={after.restart_count}")

    say("the SAME dashboard object, ticking on:")
    for poll in range(1, 4):
        time.sleep(10)
        verdict = watch.tick()
        say(
            f"poll {poll:2}  : {dashboard.line(verdict)}   "
            f"[{verdict.state} stable={verdict.stable}]"
        )
    fresh = dashboard.Dashboard(SPEC, ENTRY, SERVER_DIR, sql=sql_seam()).tick()
    say(f"a dashboard made fresh at this moment: {dashboard.line(fresh)}")
    assert verdict.state == "up", f"still called {verdict.state} after the world came back"
    assert fresh.state == verdict.state, "the old watcher and a fresh one disagree"
    assert not verdict.stable, "a reset count is not evidence the crash cause is gone"
    assert "crash" in dashboard.line(verdict), "the tab must say why it is holding back"
    say("the tab, restarted but not yet steady:")
    shot(watch, "2-restarted-not-yet-steady.png")

    # And now the part no rule can stand in for: sitting through SETTLED_AFTER
    # and watching the interlock open on its own, against the real clock.
    say(f"waiting out SETTLED_AFTER ({dashboard.SETTLED_AFTER}) on this run:")
    deadline = time.monotonic() + 13 * 60
    while time.monotonic() < deadline:
        time.sleep(60)
        verdict = watch.tick()
        say(f"  {dashboard.line(verdict)}   [stable={verdict.stable}]")
        if verdict.stable:
            break
    assert verdict.stable, "the interlock never opened; SETTLED_AFTER did not fire"
    say("the tab, steady again:")
    shot(watch, "3-steady-again.png")
    say("PROVED on the real worldserver: not a loop, not yet steady, then steady")


if __name__ == "__main__":
    main()
