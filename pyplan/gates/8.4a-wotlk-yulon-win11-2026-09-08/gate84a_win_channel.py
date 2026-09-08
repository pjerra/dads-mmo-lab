"""Turn this install's command channel on, so the Characters tab has a route.

8.4a's actions are console commands and on Windows the console cannot send them
(8.2b: there is no pty to reach through), so the channel 8.2b built is the only
route the tab has here. This is 8.2b's own `press`/`prove` pair with the source
root and the server dir changed, run against `D:\\wow-server` -- a second live
press of that feature, and the prerequisite for everything 8.4a does.

Usage:  python gate84a_win_channel.py press|prove|state
"""

from __future__ import annotations

import sys
import time
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, r"C:\gate\src84a\pylauncher")

from yulon import channel_setup, docker  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path(r"D:\wow-server")
ENTRY = load_catalog().get("wow-wotlk")


def say(text: str) -> None:
    print(f"[{datetime.now(UTC).strftime('%H:%M:%SZ')}] {text}", flush=True)


def services() -> ControllerServices:
    return ControllerServices.for_entry(ENTRY, SERVER_DIR)


def world() -> str:
    return docker.container_state(ENTRY.container_spec().world).status


def stage_state() -> None:
    setup = services().channel_setup
    assert setup is not None
    say(f"world: {world()}")
    say(f"settle(): {type(setup.settle()).__name__}")


def stage_press() -> None:
    svc = services()
    setup = svc.channel_setup
    assert setup is not None
    running = world() == "running"
    say(f"===== GROUND: world running={running}, settle()={type(setup.settle()).__name__} =====")
    if running:
        try:
            setup.enable(world_running=True)
            say("NOT REFUSED while running -- that is 8.2a's whole shape broken")
        except channel_setup.EnableRefused as exc:
            say(f"refused while running, correctly: {exc}")
        say("stopping the world, as the press requires")
        svc.controller.stop()
    result = setup.enable(world_running=False)
    say(f"pressed: changed={result.changed} path={result.path}")
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
        if world() == "running":
            break
        time.sleep(5)
    say(f"world: {world()}")
    _settle_until_verified(setup)


def stage_settle() -> None:
    """Verify against a world that is ALREADY up, repairing if the account is known.

    The first `prove` run of this box gave up while the world was still loading
    -- three tries in fifteen seconds against a world that needs a minute and a
    half -- and gave up correctly, saving nothing. The run after that is the
    trap `ensure()` documents: with no credential on disk it starts from `Idle`,
    generates a NEW password, and `create` keeps the password of an account that
    already exists, so every round trip is a 401. A 401 is `denied` and not
    silence, which is what makes it repairable rather than terminal.
    """
    setup = services().channel_setup
    assert setup is not None
    say(f"===== GROUND: world={world()}, setup_state={type(setup.setup_state()).__name__} =====")
    _settle_until_verified(setup)


def _settle_until_verified(setup) -> None:  # type: ignore[no-untyped-def]
    for attempt in range(1, 61):
        state = setup.prove()
        if isinstance(state, channel_setup.Verified):
            say(f"attempt {attempt}: VERIFIED as {state.account} at {state.at}")
            break
        if isinstance(state, channel_setup.Refused):
            say(f"attempt {attempt}: REFUSED -- {state.reason}")
            say("pressing the repair the refusal is for")
            state = setup.repair()
            say(f"after repair: {type(state).__name__}")
            if isinstance(state, channel_setup.Verified):
                break
            raise SystemExit(f"the repair did not verify: {state}")
        if isinstance(state, channel_setup.GaveUp):
            raise SystemExit(f"gave up: {state.reason}")
        if attempt % 5 == 0:
            say(f"attempt {attempt}: {type(state).__name__}")
        time.sleep(10)
    else:
        raise SystemExit("the channel never verified")
    channel = setup.live_channel()
    assert channel is not None
    answer = channel.send("server info")
    say(f"round trip: {answer.outcome} {(answer.text or '').strip()[:200]!r}")


if __name__ == "__main__":
    {"press": stage_press, "prove": stage_prove, "settle": stage_settle, "state": stage_state}[sys.argv[1]]()
