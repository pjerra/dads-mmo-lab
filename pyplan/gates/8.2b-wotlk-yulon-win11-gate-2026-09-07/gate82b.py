"""The 8.2b live gate: the command channel on NATIVE WINDOWS.

8.2b adds no code. What it asks is whether the same press works on a host where
this app has never been able to send a command at all -- the attach console
reaches a container's stdin through a pty, and on Windows there is no pty to
reach through, which is why the Console tab could show output here and never
send. The channel is the route that does not need one.

Everything below is 8.2a's own gate with two constants changed, deliberately
rather than rewritten: if the Windows run needed a different script, the claim
"no new code" would be false.

    D:\\gate\\wotlk-server77   the 7.7 install, still on this box
    C:\\gate\\src82b            this tree at c5576aa4

Usage:  python gate82b.py <stage>
        ports    every LISTEN port inside the container, from /proc/net/tcp
        before   the tab and the channel state before anything is pressed
        press    enable, with the world stopped, and what changed in the file
        prove    the ordinary Start, the account, and a round trip FROM THE HOST
        rows     the account row AND its access row, at level 3
        marker   the world's restart count and THIS run's ready marker
        break    a deliberately wrong credential, read back through the tab
        repair   the repair path, and the account count either side of it
        port     an occupied 7878: the start fails, the press is rolled back,
                 and the server starts anyway
        after    the ports again, and the tab as it ends up
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

sys.path.insert(0, r"C:\gate\src82b\pylauncher")

from yulon import channel_setup, commands, docker, resources, soap  # noqa: E402
from yulon.catalog import composegen  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path(r"D:\gate\wotlk-server77")
ENTRY = load_catalog().get("wow-wotlk")
SPEC = ENTRY.container_spec()
OPERATIONS = ENTRY.operations
SHOTS = Path(r"C:\gate\gate82b-shots")
INSTALL_ID = composegen.install_id(SERVER_DIR)
ACCOUNT = channel_setup.account_name(INSTALL_ID)

# `_` is a single-character wildcard in LIKE, so the app's own prefix `YULON_`
# would also match an account named `YULONGATE`. Compared as a plain prefix
# instead, for the reason 8.2a's script gives: an escape inside a LIKE inside an
# f-string inside a shell is three places to get it wrong.
PREFIX_MATCH = "LEFT(username, 6) = 'YULON_'"


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def sql(database: str, statement: str) -> str:
    """Ask this install's own database, through the seam the tab uses."""
    from yulon.ui import controller_view as view_module  # noqa: PLC0415

    password = view_module._db_password(ENTRY, SERVER_DIR)
    reader = view_module._sql_for(ENTRY, password, wsl_distro=None)
    return reader.query(database, statement).strip()


def services() -> ControllerServices:
    return ControllerServices.for_entry(ENTRY, SERVER_DIR)


def listening_ports() -> list[int]:
    """Every LISTEN port inside the worldserver container, from /proc/net/tcp.

    Read this way because the image ships neither `ss` nor `curl`, and
    installing a tool to take a measurement changes the thing being measured.
    The file is the container's, so this is identical on Windows: the container
    is Linux either way, and only the daemon underneath it differs.
    """
    ports: list[int] = []
    for name in ("/proc/net/tcp", "/proc/net/tcp6"):
        proc = subprocess.run(
            ["docker", "exec", SPEC.world, "cat", name],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=60,
        )
        for line in proc.stdout.splitlines()[1:]:
            fields = line.split()
            if len(fields) < 4 or fields[3] != "0A":  # 0A = TCP_LISTEN
                continue
            ports.append(int(fields[1].split(":")[1], 16))
    return sorted(set(ports))


def shoot(name: str) -> Path:
    """Render the Server tab offscreen, so the sentence can be looked at."""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    QApplication.instance() or QApplication([])
    view = ControllerView(ENTRY, services(), status_poll_ms=5000, job_runner=run_inline)
    view.refresh_channel()
    view.resize(940, 520)
    SHOTS.mkdir(parents=True, exist_ok=True)
    path = SHOTS / f"{name}.png"
    view.grab().save(str(path))
    say(f"{name}: status={view.status_label.text()!r}")
    say(f"{name}: verdict={view.verdict_label.text()!r}")
    say(f"{name}: channel={view.channel_label.text()!r}")
    say(f"{name}: enable enabled={view.enable_channel_button.isEnabled()}")
    say(f"{name}: repair offered={view.repair_channel_button.isVisibleTo(view)}")
    say(f"{name}: wrote {path}")
    return path


def shoot_view(view: object, name: str) -> Path:
    """Photograph a view ALREADY in the state being proved, not a fresh one."""
    SHOTS.mkdir(parents=True, exist_ok=True)
    path = SHOTS / f"{name}.png"
    view.resize(940, 520)
    view.grab().save(str(path))
    say(f"{name}: problem={view.problem_label.text()!r}")
    say(f"{name}: wrote {path}")
    return path


def stage_ports() -> None:
    ports = listening_ports()
    say(f"listening inside {SPEC.world}: {ports}")
    for port in (*OPERATIONS.must_not_listen, OPERATIONS.port):
        say(f"  {port}: {'LISTENING' if port in ports else 'silent'}")


def stage_before() -> None:
    say(f"install id: {INSTALL_ID}   the account this press would make: {ACCOUNT}")
    say(f"world: {docker.container_state(SPEC.world)}")
    shoot("1-before")


def stage_press() -> None:
    svc = services()
    setup = svc.channel_setup
    assert setup is not None
    running = docker.container_state(SPEC.world).status == "running"
    say(f"world running: {running}")
    if running:
        try:
            setup.enable(world_running=True)
        except channel_setup.EnableRefused as exc:
            say(f"refused while running, correctly: {exc}")
        say("stopping the world, as the press requires")
        svc.controller.stop()

    result = setup.enable(world_running=False)
    say(f"pressed: changed={result.changed} path={result.path}")
    again = setup.enable(world_running=False)
    say(f"pressed again: changed={again.changed} (a second press changes nothing)")
    say("the keys now in the override:")
    for line in result.path.read_text(encoding="utf-8").splitlines():
        if "AC_SOAP" in line or "COMMAND_SERVER" in line or "AC_RA_" in line:
            say(f"    {line.strip()}")


def stage_prove() -> None:
    svc = services()
    setup = svc.channel_setup
    assert setup is not None
    say("starting the server the ordinary way")
    svc.controller.start()
    for _ in range(120):
        if docker.container_state(SPEC.world).status == "running":
            break
        time.sleep(5)
    say(f"world: {docker.container_state(SPEC.world)}")

    say("waiting for the world to answer")
    for attempt in range(1, 61):
        state = setup.prove()
        say(f"  attempt {attempt}: {type(state).__name__}")
        if isinstance(state, channel_setup.Verified):
            say(f"VERIFIED as {state.account} at {state.at}")
            break
        if isinstance(state, channel_setup.GaveUp):
            say(f"gave up: {state.reason}")
            break
        time.sleep(20)

    saved = channel_setup.load_credential(ENTRY.id, INSTALL_ID)
    say(f"credential on disk: {channel_setup.credential_path(ENTRY.id, INSTALL_ID)}")
    if saved is not None:
        reply = soap.execute(saved, commands.SERVER_INFO)
        say(f"round trip FROM THE HOST: {reply.outcome}")
        for line in reply.text.splitlines()[:6]:
            say(f"    {line}")
    shoot("2-after")


def stage_rows() -> None:
    rows = sql("auth", f"SELECT id, username FROM account WHERE {PREFIX_MATCH};")
    say(f"accounts matching the app's own prefix: {rows!r}")
    assert rows and rows.count("\n") == 0, "there must be exactly one, and there is not"
    account_id = rows.split("\t")[0]
    access = sql("auth", f"SELECT id, gmlevel, RealmID FROM account_access WHERE id = {account_id};")
    say(f"account_access for {account_id}: {access!r}")
    assert access, "the account exists and has no access row: it is not an administrator"
    assert access.split("\t")[1] == "3", "the access row is not level 3"
    say("PASSED: one account, one access row, gmlevel 3")


def stage_marker() -> None:
    state = docker.container_state(SPEC.world)
    say(f"world: status={state.status} restarts={state.restart_count} started={state.started_at}")
    assert state.status == "running"
    assert state.restart_count == 0, "the world has restarted since it was started"
    ready = docker.AZEROTHCORE_READY_WORLD
    text = subprocess.run(
        ["docker", "logs", "--since", state.started_at, SPEC.world],
        capture_output=True,
        text=True,
        # The app's own runner decodes this way everywhere (`runner.py:331`), and a
        # gate that decodes differently measures itself: `text=True` alone picks
        # cp1252 on Windows and dies on the colour bytes in this core's log.
        encoding="utf-8",
        errors="replace",
        timeout=120,
    )
    found = [ln for ln in (text.stdout + text.stderr).splitlines() if ready in ln]
    say(f"ready marker lines since this run began: {found[-1] if found else None!r}")
    assert found, f"{ready!r} is not in this run's log"
    say("PASSED: restart count 0, and this run's ready marker is in its own log")


def stage_break() -> None:
    path = channel_setup.credential_path(ENTRY.id, INSTALL_ID)
    shutil.copy2(path, path.with_suffix(".json.gate-backup"))
    raw = json.loads(path.read_text(encoding="utf-8"))
    say(f"credential before: account={raw['account']} verified_at={raw.get('verified_at')!r}")
    raw["password"] = "wr0ng-p@ssw0rd"
    path.write_text(json.dumps(raw, indent=2), encoding="utf-8")
    say("wrote a deliberately wrong password into the credential file")

    state = services().channel_setup.check()
    say(f"check() said: {type(state).__name__}")
    assert isinstance(state, channel_setup.Refused), "a wrong credential did not read as refused"
    say(f"reason: {state.reason}")
    shoot("3-refused")
    say("PASSED: the tab reads refused, and offers the repair")


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
    reply = soap.execute(saved, commands.SERVER_INFO)
    say(f"round trip FROM THE HOST with the repaired credential: {reply.outcome}")
    for line in reply.text.splitlines()[:4]:
        say(f"    {line}")
    assert reply.outcome == "answered"
    shoot("4-repaired")
    say("PASSED: verified with the time, repaired, and still one account")


def stage_port() -> None:
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    QApplication.instance() or QApplication([])
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
    assert "AC_SOAP_ENABLED" in (SERVER_DIR / composegen.OVERRIDE_FILE).read_text(encoding="utf-8")

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
        say("PASSED: rolled back, and the server started with the port still taken")
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


def stage_after() -> None:
    for _ in range(90):
        if docker.container_state(SPEC.world).status == "running":
            break
        time.sleep(5)
    ports = listening_ports()
    say(f"listening inside {SPEC.world}: {ports}")
    for port in OPERATIONS.must_not_listen:
        say(f"  {port}: {'LISTENING' if port in ports else 'silent'}")
        assert port not in ports, f"{port} is listening and must not be"
    state = services().channel_setup.settle()
    say(f"settle() said: {type(state).__name__}")
    shoot("6-after")


if __name__ == "__main__":
    {
        "ports": stage_ports,
        "before": stage_before,
        "press": stage_press,
        "prove": stage_prove,
        "rows": stage_rows,
        "marker": stage_marker,
        "break": stage_break,
        "repair": stage_repair,
        "port": stage_port,
        "after": stage_after,
    }[sys.argv[1]]()
