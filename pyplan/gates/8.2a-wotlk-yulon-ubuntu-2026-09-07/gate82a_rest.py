"""The rest of the 8.2a gate: the five clauses the first run left unproved.

Usage:  python gate82a_rest.py <stage>

    rows    the account row AND its access row, at level 3, and how many exist
    marker  the world's restart count and THIS run's ready marker in its log
    break   a deliberately wrong credential file, read back through the tab
    repair  the repair path, and the account count on the other side of it
    port    an occupied 7878: the start fails, the press is rolled back, the
            server starts, and the channel is turned on again afterwards
    after   the ports inside the container, and the tab as it ends up

Every reading is taken from the machine rather than from anything this script
wrote a moment earlier: an artifact records what a program believed, and the
question in each clause is what the server actually did.
"""

from __future__ import annotations

import json
import os
import shutil
import socket
import subprocess
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate81a" / "pylauncher"))

from yulon import channel_setup, docker, resources  # noqa: E402
from yulon.catalog import composegen  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "wowserver"
ENTRY = load_catalog().get("wow-wotlk")
SPEC = ENTRY.container_spec()
OPERATIONS = ENTRY.operations
SHOTS = Path.home() / "gate82a-shots"
INSTALL_ID = composegen.install_id(SERVER_DIR)
ACCOUNT = channel_setup.account_name(INSTALL_ID)


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def sql(database: str, statement: str) -> str:
    """Ask the install's own database, through the app's own seam.

    Not a hand-built `docker exec mysql`: this install has no `.env` at all --
    WotLK carries a fixed password in its compose -- so a gate that read one
    would be measuring a file that does not exist. Going through the seam the
    tab uses also means the reading comes from the same place the feature does.
    """
    from yulon.ui import controller_view as view_module  # noqa: PLC0415

    password = view_module._db_password(ENTRY, SERVER_DIR)
    reader = view_module._sql_for(ENTRY, password, wsl_distro=None)
    return reader.query(database, statement).strip()


def services() -> ControllerServices:
    return ControllerServices.for_entry(ENTRY, SERVER_DIR)


def shoot_view(view: object, name: str) -> Path:
    """Photograph a view that is ALREADY in the state being proved.

    `shoot()` builds a fresh one, which is right for a state that lives on
    disk and wrong for a sentence that lives in a widget: the rollback message
    is what the tab said about what just happened, and a new tab has not had
    anything happen to it.
    """
    SHOTS.mkdir(parents=True, exist_ok=True)
    path = SHOTS / f"{name}.png"
    view.resize(940, 520)
    view.grab().save(str(path))
    say(f"{name}: problem={view.problem_label.text()!r}")
    say(f"{name}: wrote {path}")
    return path


def shoot(name: str) -> Path:
    """Render the Server tab offscreen, so the sentence can be looked at."""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    app = QApplication.instance() or QApplication([])
    view = ControllerView(ENTRY, services(), status_poll_ms=5000, job_runner=run_inline)
    view.refresh_channel()
    view.resize(940, 520)
    SHOTS.mkdir(parents=True, exist_ok=True)
    path = SHOTS / f"{name}.png"
    view.grab().save(str(path))
    say(f"{name}: channel={view.channel_label.text()!r}")
    say(f"{name}: repair offered={view.repair_channel_button.isVisibleTo(view)}")
    say(f"{name}: wrote {path}")
    del app
    return path


# -- clause 2: the account row AND its access row ---------------------------


# `_` is a single-character wildcard in LIKE, so the app's own prefix `YULON_`
# also matches `YULONGATE` -- an account an earlier gate left on this box.
# Compared as a plain prefix instead of escaped, because an escape inside a
# LIKE inside an f-string inside a shell is three places to get it wrong.
# `rust-main` records the same trap against `DMLSOAP_%`, and counting with the
# naive pattern would have made "no second account was created" true by
# accident and false by measurement.
PREFIX_MATCH = "LEFT(username, 6) = 'YULON_'"


def stage_rows() -> None:
    rows = sql("auth", f"SELECT id, username FROM account WHERE {PREFIX_MATCH};")
    say(f"accounts matching YULON%: {rows!r}")
    assert rows.count("\n") == 0 and rows, "there must be exactly one, and there is not"
    account_id = rows.split("\t")[0]
    access = sql(
        "auth",
        f"SELECT id, gmlevel, RealmID FROM account_access WHERE id = {account_id};",
    )
    say(f"account_access for {account_id}: {access!r}")
    assert access, "the account exists and has no access row: it is not an administrator"
    assert access.split("\t")[1] == "3", "the access row is not level 3"
    say("clause 2 PASSED: one account, one access row, gmlevel 3")


# -- clause 3: the restart count, and THIS run's ready marker ---------------


def stage_marker() -> None:
    state = docker.container_state(SPEC.world)
    say(f"world: status={state.status} restarts={state.restart_count} started={state.started_at}")
    assert state.status == "running"
    assert state.restart_count == 0, "the world has restarted since it was started"
    # The app's own marker, not one invented here: `AZEROTHCORE_READY_WORLD` is
    # the literal all four shipped scripts grep for, and the scoping is
    # `--since <this run's StartedAt>` so a banner from the PREVIOUS run cannot
    # answer for this one.
    ready = docker.AZEROTHCORE_READY_WORLD
    text = subprocess.run(
        ["docker", "logs", "--since", state.started_at, SPEC.world],
        capture_output=True,
        text=True,
        timeout=120,
    )
    haystack = text.stdout + text.stderr
    found = [line for line in haystack.splitlines() if ready in line]
    say(f"ready marker lines since this run began: {found[-1] if found else None!r}")
    assert found, f"{ready!r} is not in this run's log"
    say("clause 3 PASSED: restart count 0, and this run's ready marker is in its own log")


# -- clause 4a: a deliberately wrong credential -----------------------------


def stage_break() -> None:
    path = channel_setup.credential_path(ENTRY.id, INSTALL_ID)
    backup = path.with_suffix(".json.gate-backup")
    shutil.copy2(path, backup)
    raw = json.loads(path.read_text(encoding="utf-8"))
    say(f"credential before: account={raw['account']} verified_at={raw.get('verified_at')!r}")
    raw["password"] = "wr0ng-p@ssw0rd"
    path.write_text(json.dumps(raw, indent=2), encoding="utf-8")
    say("wrote a deliberately wrong password into the credential file")

    channel = services().channel_setup
    state = channel.check()
    say(f"check() said: {type(state).__name__}")
    assert isinstance(state, channel_setup.Refused), "a wrong credential did not read as refused"
    say(f"reason: {state.reason}")
    shoot("3-refused")
    say("clause 4a PASSED: the tab reads refused, and offers the repair")


# -- clause 4b: the repair, and no second account ---------------------------


def stage_repair() -> None:
    before = sql("auth", f"SELECT COUNT(*) FROM account WHERE {PREFIX_MATCH};")
    say(f"accounts before the repair: {before}")

    channel = services().channel_setup
    refused = channel.check()
    assert isinstance(refused, channel_setup.Refused), f"expected refused, got {refused}"
    state = channel.repair()
    say(f"repair() said: {type(state).__name__}")
    assert isinstance(state, channel_setup.Verified), "the repair did not return to verified"
    say(f"verified as {state.account} at {state.at}")

    after = sql("auth", f"SELECT COUNT(*) FROM account WHERE {PREFIX_MATCH};")
    say(f"accounts after the repair: {after}")
    assert after == before, "the repair created a second account"

    saved = channel_setup.load_credential(ENTRY.id, INSTALL_ID)
    assert saved is not None and saved.password != "wr0ng-p@ssw0rd"
    from yulon import commands, soap  # noqa: PLC0415

    reply = soap.execute(saved, commands.SERVER_INFO)
    say(f"round trip FROM THE HOST with the repaired credential: {reply.outcome}")
    for line in reply.text.splitlines()[:4]:
        say(f"    {line}")
    assert reply.outcome == "answered"
    shoot("4-repaired")
    say("clause 1 and 4b PASSED: verified with the time, repaired, and still one account")


# -- clause 5: an occupied port -------------------------------------------


def stage_port() -> None:
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415

    app = QApplication.instance() or QApplication([])
    svc = services()
    view = ControllerView(ENTRY, svc, status_poll_ms=0, job_runner=run_inline)

    say("stopping the world, because the press requires it")
    svc.controller.stop()

    say("putting the install's override back to what it was before any press")
    plan = composegen.render(ENTRY, SERVER_DIR, templates_root=resources.installers_dir())
    (SERVER_DIR / composegen.OVERRIDE_FILE).write_text(plan.override, encoding="utf-8")
    composegen.write_dotenv(SERVER_DIR, {channel_setup.HOST_PORT_VAR: "127.0.0.1:7878"})
    off = (SERVER_DIR / composegen.OVERRIDE_FILE).read_text(encoding="utf-8")
    assert "AC_SOAP_ENABLED" not in off
    say("override is back to channel-off")

    result = svc.channel_setup.enable(world_running=False)
    say(f"pressed: changed={result.changed}")
    on = (SERVER_DIR / composegen.OVERRIDE_FILE).read_text(encoding="utf-8")
    assert "AC_SOAP_ENABLED" in on

    holder = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    holder.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    holder.bind(("127.0.0.1", OPERATIONS.port))
    holder.listen(1)
    say(f"something else now holds 127.0.0.1:{OPERATIONS.port}")

    try:
        view.start_server()
        said = view.problem_label.text()
        say(f"the tab said: {said!r}")
        assert str(OPERATIONS.port) in said, "the tab did not name the port"
        assert "command channel" in said.lower(), "the tab did not say what it undid"
        back = (SERVER_DIR / composegen.OVERRIDE_FILE).read_text(encoding="utf-8")
        assert "AC_SOAP_ENABLED" not in back, "the configuration was not rolled back"
        env = (SERVER_DIR / ".env").read_text(encoding="utf-8")
        assert f"{channel_setup.HOST_PORT_VAR}={channel_setup.RELEASED_HOST_PORT}" in env
        say("the configuration is rolled back and the host port released")
        shoot_view(view, "5-rolled-back")

        say("starting again, with the port still held by something else")
        svc.controller.start()
        for _ in range(60):
            if docker.container_state(SPEC.world).status == "running":
                break
            time.sleep(5)
        state = docker.container_state(SPEC.world)
        say(f"world after the rollback: {state.status}")
        assert state.status == "running", "the server is still not startable"
        say("clause 5 PASSED: rolled back, and the server started with the port still taken")
    finally:
        holder.close()
        say("released the port this gate was holding")

    say("putting the install back the way the user would want it")
    svc.controller.stop()
    svc.channel_setup.enable(world_running=False)
    composegen.write_dotenv(SERVER_DIR, {channel_setup.HOST_PORT_VAR: "127.0.0.1:7878"})
    svc.controller.start()
    for _ in range(90):
        if docker.container_state(SPEC.world).status == "running":
            break
        time.sleep(5)
    say(f"world: {docker.container_state(SPEC.world).status}")
    del app


# -- the ports, and the tab as it ends up ----------------------------------


def listening_ports() -> list[int]:
    ports: list[int] = []
    for name in ("/proc/net/tcp", "/proc/net/tcp6"):
        proc = subprocess.run(
            ["docker", "exec", SPEC.world, "cat", name],
            capture_output=True,
            text=True,
            timeout=60,
        )
        for line in proc.stdout.splitlines()[1:]:
            fields = line.split()
            if len(fields) < 4 or fields[3] != "0A":
                continue
            ports.append(int(fields[1].split(":")[1], 16))
    return sorted(set(ports))


def stage_after() -> None:
    for _ in range(90):
        state = docker.container_state(SPEC.world)
        if state.status == "running":
            break
        time.sleep(5)
    ports = listening_ports()
    say(f"listening inside {SPEC.world}: {ports}")
    for port in OPERATIONS.must_not_listen:
        say(f"  {port}: {'LISTENING' if port in ports else 'silent'}")
        assert port not in ports, f"{port} is listening and must not be"
    channel = services().channel_setup
    state = channel.settle()
    say(f"settle() said: {type(state).__name__}")
    shoot("6-after")


if __name__ == "__main__":
    {
        "rows": stage_rows,
        "marker": stage_marker,
        "break": stage_break,
        "repair": stage_repair,
        "port": stage_port,
        "after": stage_after,
    }[sys.argv[1]]()
