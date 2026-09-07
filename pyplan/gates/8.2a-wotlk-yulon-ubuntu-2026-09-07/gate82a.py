"""The 8.2a live gate, on yulon-ubuntu against the finished 7.2 WotLK install.

Usage:  python gate82a.py <stage>
        ports    read /proc/net/tcp inside the container, no tools installed
        before   the tab and the channel state before anything is pressed
        press    stop the world, enable, and say what changed
        prove    start, create the account, verify by a round trip FROM THE HOST
        after    the ports again, and the tab again
"""

from __future__ import annotations

import os
import subprocess
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate81a" / "pylauncher"))

from yulon import channel_setup, commands, docker, soap  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "wowserver"
ENTRY = load_catalog().get("wow-wotlk")
SPEC = ENTRY.container_spec()
SHOTS = Path.home() / "gate82a-shots"


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def listening_ports() -> list[int]:
    """Every LISTEN port inside the worldserver container, from /proc/net/tcp.

    Read this way because the image ships neither `ss` nor `curl`
    (`apps/docker/Dockerfile:112`, `:122-126`), and installing a tool to take a
    measurement changes the thing being measured.
    """
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
            if len(fields) < 4 or fields[3] != "0A":  # 0A = TCP_LISTEN
                continue
            ports.append(int(fields[1].split(":")[1], 16))
    return sorted(set(ports))


def stage_ports() -> None:
    ports = listening_ports()
    print(f"[{stamp()}] listening inside {SPEC.world}: {ports}")
    for port in (8888, 3443, 7878):
        print(f"[{stamp()}]   {port}: {'LISTENING' if port in ports else 'silent'}")


def _services() -> ControllerServices:
    return ControllerServices.for_entry(ENTRY, SERVER_DIR)


def _shoot(name: str) -> Path:
    """Render the Server tab to a PNG, offscreen, so it can be looked at."""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    app = QApplication.instance() or QApplication([])
    view = ControllerView(ENTRY, _services(), status_poll_ms=5000, job_runner=run_inline)
    view.refresh_channel()
    view.resize(900, 460)
    SHOTS.mkdir(parents=True, exist_ok=True)
    path = SHOTS / f"{name}.png"
    view.grab().save(str(path))
    print(f"[{stamp()}] {name}: status={view.status_label.text()!r}")
    print(f"[{stamp()}] {name}: verdict={view.verdict_label.text()!r}")
    print(f"[{stamp()}] {name}: channel={view.channel_label.text()!r}")
    print(f"[{stamp()}] {name}: enable button enabled={view.enable_channel_button.isEnabled()}")
    print(f"[{stamp()}] wrote {path}")
    del app
    return path


def stage_before() -> None:
    stage_ports()
    _shoot("1-before")


def stage_press() -> None:
    services = _services()
    setup = services.channel_setup
    assert setup is not None
    running = docker.container_state(SPEC.world).status == "running"
    print(f"[{stamp()}] world running: {running}")

    if running:
        try:
            setup.enable(world_running=True)
        except channel_setup.EnableRefused as exc:
            print(f"[{stamp()}] refused, correctly: {exc}")
        print(f"[{stamp()}] stopping the world, as the press requires")
        services.controller.stop()

    result = setup.enable(world_running=False)
    print(f"[{stamp()}] pressed: changed={result.changed} path={result.path}")
    again = setup.enable(world_running=False)
    print(f"[{stamp()}] pressed again: changed={again.changed} (a second press changes nothing)")
    print(f"[{stamp()}] the keys now in the override:")
    for line in result.path.read_text(encoding="utf-8").splitlines():
        if "AC_SOAP" in line or "COMMAND_SERVER" in line or "AC_RA_" in line:
            print(f"    {line.strip()}")


def stage_prove() -> None:
    services = _services()
    setup = services.channel_setup
    assert setup is not None
    print(f"[{stamp()}] starting the server the ordinary way")
    services.controller.start()
    for _ in range(120):
        if docker.container_state(SPEC.world).status == "running":
            break
        time.sleep(5)
    print(f"[{stamp()}] world: {docker.container_state(SPEC.world)}")

    print(f"[{stamp()}] waiting for the world to answer")
    for attempt in range(1, 61):
        state = setup.prove()
        print(f"[{stamp()}]   attempt {attempt}: {type(state).__name__}")
        if isinstance(state, channel_setup.Verified):
            print(f"[{stamp()}] VERIFIED as {state.account}")
            break
        if isinstance(state, channel_setup.GaveUp):
            print(f"[{stamp()}] gave up: {state.reason}")
            break
        time.sleep(20)

    saved = channel_setup.load_credential(
        ENTRY.id, __import__("yulon.catalog.composegen", fromlist=["x"]).install_id(SERVER_DIR)
    )
    print(f"[{stamp()}] credential on disk: {saved}")
    if saved is not None:
        reply = soap.execute(saved, commands.SERVER_INFO)
        print(f"[{stamp()}] round trip FROM THE HOST: {reply.outcome}")
        for line in reply.text.splitlines()[:6]:
            print(f"    {line}")


def stage_after() -> None:
    stage_ports()
    _shoot("2-after")


if __name__ == "__main__":
    {
        "ports": stage_ports,
        "before": stage_before,
        "press": stage_press,
        "prove": stage_prove,
        "after": stage_after,
    }[sys.argv[1]]()
