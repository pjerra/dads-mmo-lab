"""The 8.2d live gate: the command channel on WoW Vanilla (CMaNGOS), on m910q.

8.2c's gate with THIS tree's own facts. Same family, and every fact below is
read from this install rather than carried over from TBC -- including the SOAP
namespace, which `soapprobe.py` measures against this listener.

    the enable route is a CONF FILE, not the container environment -- CMaNGOS
    reads no environment anywhere in its config reader

    the GM level is a COLUMN on the account row, `account.gmlevel`; there is no
    `account_access` table on this tree and nothing here looks for one

    the ready marker is this core's own (`Avg Diff:`), not AzerothCore's
    `ready...`

    the port is published by the OVERRIDE, because no CMaNGOS compose file
    binds 7878 at all

Usage:  python gate82d.py <stage>
        ports    every LISTEN port inside the container, from /proc/net/tcp
        before   the tab and the channel state before anything is pressed
        press    enable with the world stopped, and what changed in the conf
        prove    the ordinary Start, the account, and a round trip FROM THE HOST
        rows     the account row at gmlevel 3, in this tree's own column
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

sys.path.insert(0, str(Path.home() / "gate82d" / "pylauncher"))

from yulon import channel_setup, commands, docker, resources, soap  # noqa: E402
from yulon.catalog import composegen  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "vanilla-75b"
ENTRY = load_catalog().get("wow-vanilla")
SPEC = ENTRY.container_spec()
OPERATIONS = ENTRY.operations
SHOTS = Path.home() / "gate82d-shots"
INSTALL_ID = composegen.install_id(SERVER_DIR)
ACCOUNT = channel_setup.account_name(INSTALL_ID)
READY = ENTRY.install.native.ready.world if ENTRY.install.native else ""

# `_` is a single-character wildcard in LIKE, so the app's own prefix `YULON_`
# would also match `YULONGATE`. Compared as a plain prefix instead, exactly as
# 8.2a's gate does and for the reason it gives.
PREFIX_MATCH = "LEFT(username, 6) = 'YULON_'"


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def sql(statement: str) -> str:
    """Ask this install's own auth database, through the seam the tab uses."""
    from yulon.ui import controller_view as view_module  # noqa: PLC0415

    password = view_module._db_password(ENTRY, SERVER_DIR)
    reader = view_module._sql_for(ENTRY, password, wsl_distro=None)
    return reader.query("auth", statement).strip()


def services() -> ControllerServices:
    return ControllerServices.for_entry(ENTRY, SERVER_DIR)


def run(*argv: str, cwd: Path | None = None, timeout: float = 180) -> str:
    proc = subprocess.run(
        list(argv),
        cwd=cwd,
        capture_output=True,
        text=True,
        # As the app's own runner does everywhere (`runner.py:331`): a gate that
        # decodes differently from the app measures its own bug.
        encoding="utf-8",
        errors="replace",
        timeout=timeout,
    )
    return (proc.stdout + proc.stderr).strip()


def listening_ports() -> list[int]:
    """Every LISTEN port inside the world container, from `/proc/net/tcp`.

    Read this way for the reason 8.2a gives: installing a tool to take a
    measurement changes the thing being measured. This image DOES ship
    `netcat-openbsd` (`wow-vanilla/native/Dockerfile.tmpl:110`) and the box's line
    allows using it -- `/proc/net/tcp` is used anyway, because it needs nothing
    at all and gives the same answer on both families.
    """
    ports: list[int] = []
    for name in ("/proc/net/tcp", "/proc/net/tcp6"):
        out = run("docker", "exec", SPEC.world, "cat", name, timeout=60)
        for line in out.splitlines()[1:]:
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
    view.refresh_status()
    view.refresh_verdict()
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


def conf_path() -> Path:
    assert OPERATIONS is not None and OPERATIONS.enable_conf is not None
    return SERVER_DIR / OPERATIONS.enable_conf.file


def say_the_conf(label: str) -> None:
    wanted = ("SOAP.Enabled", "SOAP.IP", "SOAP.Port", "Ra.Enable")
    say(f"{label}, in {conf_path().name}:")
    for line in conf_path().read_text(encoding="utf-8").splitlines():
        if any(line.startswith(key) for key in wanted):
            say(f"    {line.strip()}")


def stage_ports() -> None:
    ports = listening_ports()
    say(f"listening inside {SPEC.world}: {ports}")
    assert OPERATIONS is not None
    for port in (*OPERATIONS.must_not_listen, OPERATIONS.port):
        say(f"  {port}: {'LISTENING' if port in ports else 'silent'}")


def stage_before() -> None:
    say(f"install id: {INSTALL_ID}   the account this press would make: {ACCOUNT}")
    say(f"world: {docker.container_state(SPEC.world)}")
    say_the_conf("before the press")
    stage_ports()
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
    say_the_conf("after the press")
    backup = conf_path().with_name(conf_path().name + channel_setup.BACKUP_SUFFIX)
    say(f"the backup the first press kept: {backup.name} ({backup.stat().st_size} bytes)")
    say("the port the override now publishes:")
    for line in result.path.read_text(encoding="utf-8").splitlines():
        if "7878" in line or "ports" in line or SPEC.world in line:
            say(f"    {line.rstrip()}")


def stage_prove() -> None:
    svc = services()
    setup = svc.channel_setup
    assert setup is not None
    say("starting the server the ordinary way")
    svc.controller.start()
    for _ in range(150):
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
        if isinstance(state, channel_setup.Refused):
            # A rejection is terminal for this loop as well as for `ensure()`:
            # asking again cannot change a password the server already said no
            # to. The way out is the repair, which is `recover` below.
            say(f"refused: {state.reason}")
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
    """This tree keeps the level on the account row; there is no access table."""
    rows = sql(f"SELECT id, username, gmlevel FROM account WHERE {PREFIX_MATCH};")
    say(f"accounts matching the app's own prefix: {rows!r}")
    assert rows and rows.count("\n") == 0, "there must be exactly one, and there is not"
    fields = rows.split("\t")
    assert fields[2] == "3", f"the account is at gmlevel {fields[2]}, not 3"
    tables = sql("SHOW TABLES LIKE 'account_access';")
    say(f"account_access on this tree: {tables!r} (there is none, and none is looked for)")
    assert tables == ""
    say("PASSED: one account, gmlevel 3, on the account row itself")


def stage_marker() -> None:
    state = docker.container_state(SPEC.world)
    say(f"world: status={state.status} restarts={state.restart_count} started={state.started_at}")
    assert state.status == "running"
    assert state.restart_count == 0, "the world has restarted since it was started"
    say(f"this core's own ready marker: {READY!r}")
    out = run("docker", "logs", "--since", state.started_at, SPEC.world, timeout=120)
    found = [ln for ln in out.splitlines() if READY in ln]
    say(f"ready marker lines since this run began: {found[-1][:110] if found else None!r}")
    assert found, f"{READY!r} is not in this run's log"
    say("PASSED: restart count 0, and this run's own ready marker is in its own log")


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
    before = sql(f"SELECT COUNT(*) FROM account WHERE {PREFIX_MATCH};")
    say(f"accounts before the repair: {before}")
    channel = services().channel_setup
    refused = channel.check()
    assert isinstance(refused, channel_setup.Refused), f"expected refused, got {refused}"
    state = channel.repair()
    say(f"repair() said: {type(state).__name__}")
    assert isinstance(state, channel_setup.Verified), "the repair did not return to verified"
    say(f"verified as {state.account} at {state.at}")
    after = sql(f"SELECT COUNT(*) FROM account WHERE {PREFIX_MATCH};")
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
    assert OPERATIONS is not None

    say("stopping the world, because the press requires it")
    svc.controller.stop()

    say("putting the install back to what it was before any press")
    assert svc.channel_setup.roll_back() in (True, False)
    say_the_conf("rolled back")

    result = svc.channel_setup.enable(world_running=False)
    say(f"pressed again: changed={result.changed}")

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
        say_the_conf("after the rollback the start triggered")
        conf_now = conf_path().read_text(encoding="utf-8")
        assert "SOAP.Enabled = 1" not in conf_now, "the conf was not rolled back"
        env = (SERVER_DIR / ".env").read_text(encoding="utf-8")
        assert f"{channel_setup.HOST_PORT_VAR}={channel_setup.RELEASED_HOST_PORT}" in env
        say("the conf is rolled back and the host port released")
        SHOTS.mkdir(parents=True, exist_ok=True)
        view.resize(940, 520)
        view.grab().save(str(SHOTS / "5-rolled-back.png"))
        say(f"5-rolled-back: problem={view.problem_label.text()!r}")

        say("starting again, with the port still held by something else")
        svc.controller.start()
        for _ in range(90):
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
    composegen.write_dotenv(
        SERVER_DIR, {channel_setup.HOST_PORT_VAR: f"127.0.0.1:{OPERATIONS.port}"}
    )
    svc.controller.start()
    for _ in range(120):
        if docker.container_state(SPEC.world).status == "running":
            break
        time.sleep(5)
    say(f"world: {docker.container_state(SPEC.world).status}")


def stage_after() -> None:
    for _ in range(120):
        if docker.container_state(SPEC.world).status == "running":
            break
        time.sleep(5)
    ports = listening_ports()
    say(f"listening inside {SPEC.world}: {ports}")
    assert OPERATIONS is not None
    for port in OPERATIONS.must_not_listen:
        say(f"  {port}: {'LISTENING' if port in ports else 'silent'}")
        assert port not in ports, f"{port} is listening and must not be"
    say(f"  {OPERATIONS.port}: {'LISTENING' if OPERATIONS.port in ports else 'silent'}")
    state = services().channel_setup.settle()
    say(f"settle() said: {type(state).__name__}")
    shoot("6-after")


def stage_recover() -> None:
    """The dead end 8.2c found, walked forwards: rejected, then repaired.

    In ONE process on purpose. The rejection lives in the object -- nothing is
    written until a round trip answers -- so a fresh `services()` per step would
    read `Idle` off an empty disk and start the whole thing again, which is
    exactly the loop this fix broke.
    """
    svc = services()
    setup = svc.channel_setup
    assert setup is not None

    state = setup.prove()
    say(f"prove() said: {type(state).__name__}")
    assert isinstance(state, channel_setup.Refused), f"expected Refused, got {state}"
    say(f"reason: {state.reason}")

    before = sql(f"SELECT COUNT(*) FROM account WHERE {PREFIX_MATCH};")
    say(f"accounts before the repair: {before}")
    state = setup.repair()
    say(f"repair() said: {type(state).__name__}")
    assert isinstance(state, channel_setup.Verified), f"the repair did not verify: {state}"
    say(f"VERIFIED as {state.account} at {state.at}")
    after = sql(f"SELECT COUNT(*) FROM account WHERE {PREFIX_MATCH};")
    say(f"accounts after the repair: {after}")
    assert after == before, "the repair created a second account"

    saved = channel_setup.load_credential(ENTRY.id, INSTALL_ID)
    assert saved is not None
    say(f"the saved endpoint's namespace: {saved.namespace!r}")
    reply = soap.execute(saved, commands.SERVER_INFO)
    say(f"round trip FROM THE HOST: {reply.outcome}")
    for line in reply.text.splitlines()[:6]:
        say(f"    {line}")
    assert reply.outcome == "answered"
    shoot("2-verified")


if __name__ == "__main__":
    {
        "ports": stage_ports,
        "recover": stage_recover,
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
