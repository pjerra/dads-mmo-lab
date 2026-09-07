"""8.1d's last clause: the player count moves when a client logs in and out.

Polls this install's dashboard, logs every CHANGE in what the Server tab would
show, and grabs the tab itself the first tick a player is on and again once the
count has gone back down. The character row is read alongside, so the number has
a name attached to it rather than being a bare integer.

Usage:  python gate81d_login.py
"""

from __future__ import annotations

import os
import subprocess
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate81d" / "pylauncher"))

from yulon import dashboard  # noqa: E402
from yulon.apply import DockerSql  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "tortoise-server"
ENTRY = load_catalog().get("wow-tortoise")
SPEC = ENTRY.container_spec()
SHOTS = Path.home() / "gate81d-login-shots"
PASSWORD = (
    (SERVER_DIR / (ENTRY.install.password.file or "")).read_text(encoding="utf-8").strip()
    if ENTRY.install.password.file
    else ""
)


def say(text: str) -> None:
    print(f"[{datetime.now(UTC).strftime('%H:%M:%SZ')}] {text}", flush=True)


def sql_seam() -> DockerSql:
    return DockerSql(
        SPEC.db,
        PASSWORD,
        schemas=ENTRY.schema_map(),
        client=ENTRY.install.native.db.client if ENTRY.install.native else None,
    )


def hand(statement: str) -> str:
    client = ENTRY.install.native.db.client if ENTRY.install.native else "mysql"
    argv = [
        "docker", "exec", "-i", SPEC.db, client or "mysql", "-uroot", f"-p{PASSWORD}",
        "--batch", "--skip-column-names", "-e", statement,
    ]
    proc = subprocess.run(argv, capture_output=True, text=True, timeout=120)
    return (proc.stdout.strip() or proc.stderr.strip()).replace("\n", " | ")


def shot(watch: dashboard.Dashboard, name: str) -> None:
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
    say(f"  wrote {SHOTS / name}   verdict: {view.verdict_label.text()!r}")


def main() -> None:
    watch = dashboard.Dashboard(SPEC, ENTRY, SERVER_DIR, sql=sql_seam())
    chars = f"{ENTRY.databases.characters}.{ENTRY.observability.characters.table}"
    say("watching; every line below is a CHANGE in what the Server tab would show")
    last = None
    seen_online = False
    back_down = False
    deadline = time.monotonic() + 30 * 60
    while time.monotonic() < deadline:
        verdict = watch.tick()
        line = dashboard.line(verdict)
        if line != last:
            say(line)
            last = line
        if verdict.players and not seen_online:
            seen_online = True
            say(f"  A PLAYER IS ON: players={verdict.players}")
            say("  the character rows: " + hand(
                f"SELECT guid, name, level, race, class, online FROM {chars} WHERE online = 1;"
            ))
            say("  the account row  : " + hand(
                "SELECT id, username, last_login FROM tw_logon.account "
                "WHERE UPPER(username) = 'TORTGATE';"
            ))
            shot(watch, "1-a-player-is-on.png")
        if seen_online and not verdict.players and not back_down:
            back_down = True
            say("  AND BACK DOWN: players=0")
            shot(watch, "2-and-back-down.png")
            say("  the character rows now: " + hand(
                f"SELECT guid, name, level, online FROM {chars} "
                "WHERE account IN (SELECT id FROM tw_logon.account "
                "WHERE UPPER(username) = 'TORTGATE');"
            ))
            break
        time.sleep(3)
    say(f"done: seen_online={seen_online} back_down={back_down}")


if __name__ == "__main__":
    main()
