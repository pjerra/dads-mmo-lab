"""The press as a PERSON reaches it: through the button, not through the seam.

8.2a proved the mechanism by calling `InstallChannel.enable()` directly. This
walks the only route a user has -- the "Turn on the command channel" button on
the Server tab -- and records what it does in each of the two states that
matter, because the press and its interlock disagree about which state that is:

    the press refuses while the world is RUNNING (8.2a's whole shape: a failed
    bind is not atomic, and on the CMaNGOS trees it costs character saves)

    the button is enabled on `Verdict.stable`, which is only ever true while
    the world IS running

Usage:  python gate82b_gui.py <stage>
        stopped  the tab with the world down: is the press reachable?
        running  start the world, then press the button and read what it says
        after    stop the world again and ask the same question once more
"""

from __future__ import annotations

import os
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, r"C:\gate\src82b\pylauncher")

from yulon import docker  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path(r"D:\gate\wotlk-server77")
ENTRY = load_catalog().get("wow-wotlk")
SPEC = ENTRY.container_spec()
SHOTS = Path(r"C:\gate\gate82b-shots")


def say(text: str) -> None:
    print(f"[{datetime.now(UTC).strftime('%H:%M:%SZ')}] {text}", flush=True)


def build():
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    QApplication.instance() or QApplication([])
    services = ControllerServices.for_entry(ENTRY, SERVER_DIR)
    view = ControllerView(ENTRY, services, status_poll_ms=0, job_runner=run_inline)
    # Everything the five-second timer does, so the buttons are in the state a
    # person would find them in and not in their constructed defaults.
    view.refresh_status()
    view.refresh_verdict()
    view.refresh_channel()
    return services, view


def report(view, label: str) -> None:
    say(f"{label}: verdict   = {view.verdict_label.text()!r}")
    say(f"{label}: channel   = {view.channel_label.text()!r}")
    say(f"{label}: ENABLE button enabled = {view.enable_channel_button.isEnabled()}")
    say(f"{label}: Stop button enabled   = {view.stop_button.isEnabled()}")
    if view.problem_label.text():
        say(f"{label}: problem   = {view.problem_label.text()!r}")
    SHOTS.mkdir(parents=True, exist_ok=True)
    view.resize(940, 520)
    view.grab().save(str(SHOTS / f"{label}.png"))
    say(f"{label}: wrote {SHOTS / label}.png")


def stage_stopped() -> None:
    say(f"world: {docker.container_state(SPEC.world).status}")
    _, view = build()
    report(view, "7-stopped-button-dead")
    say(
        "READING: the press is what turns the channel on, it requires the world "
        "stopped, and with the world stopped it cannot be pressed."
    )


def stage_running() -> None:
    services, view = build()
    say("starting the world the ordinary way")
    services.controller.start()
    for _ in range(150):
        if docker.container_state(SPEC.world).status == "running":
            break
        time.sleep(5)
    say(f"world: {docker.container_state(SPEC.world)}")
    say("waiting for the world to finish loading before asking the tab")
    state = docker.container_state(SPEC.world)
    for _ in range(120):
        logs = docker.log_tail(SPEC.world, 200)
        if docker.AZEROTHCORE_READY_WORLD in logs:
            break
        time.sleep(10)
    say(f"ready marker seen; world state {state.status}")

    view.refresh_status()
    view.refresh_verdict()
    view.refresh_channel()
    report(view, "8-running-button-live")

    say("pressing the button, which is the only route a person has")
    view.enable_channel()
    report(view, "9-pressed-while-running")


def stage_after() -> None:
    services, view = build()
    say("stopping the world, which is what the refusal asked for")
    services.controller.stop()
    view.refresh_status()
    view.refresh_verdict()
    view.refresh_channel()
    report(view, "10-stopped-again")


def stage_press() -> None:
    """The whole press, as a person does it: Stop, then the button, then Start."""
    services, view = build()
    if view.stop_button.isEnabled():
        say("pressing Stop, which is what the refusal asked for")
        view.stop_server()
        for _ in range(60):
            if docker.container_state(SPEC.world).status != "running":
                break
            time.sleep(5)
    view.refresh_status()
    view.refresh_verdict()
    view.refresh_channel()
    report(view, "10-stopped-button-live")
    assert view.enable_channel_button.isEnabled(), "the press is STILL unreachable"

    say("pressing the button")
    view.enable_channel()
    report(view, "11-pressed")
    override = SERVER_DIR / "docker-compose.override.yml"
    say("the keys the press wrote:")
    for line in override.read_text(encoding="utf-8").splitlines():
        if "AC_SOAP" in line or "COMMAND_SERVER" in line or "AC_RA_" in line:
            say(f"    {line.strip()}")


def stage_start() -> None:
    """The ordinary Start, and what the tab says once the world answers."""
    services, view = build()
    say("pressing Start")
    view.start_server()
    for _ in range(150):
        if docker.container_state(SPEC.world).status == "running":
            break
        time.sleep(5)
    say(f"world: {docker.container_state(SPEC.world)}")
    for _ in range(120):
        if docker.AZEROTHCORE_READY_WORLD in docker.log_tail(SPEC.world, 300):
            break
        time.sleep(10)
    say("this run's ready marker is in the log")

    setup = services.channel_setup
    for attempt in range(1, 31):
        state = setup.settle()
        say(f"  settle attempt {attempt}: {type(state).__name__}")
        if type(state).__name__ in {"Verified", "GaveUp"}:
            break
        time.sleep(20)
    view.refresh_status()
    view.refresh_verdict()
    view.refresh_channel()
    report(view, "12-verified")


if __name__ == "__main__":
    {
        "stopped": stage_stopped,
        "running": stage_running,
        "press": stage_press,
        "start": stage_start,
        "after": stage_after,
    }[sys.argv[1]]()
