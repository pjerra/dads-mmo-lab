"""The 8.2e live gate: the command channel on WoW Tortoise, which is the console.

This core links neither gsoap nor RASocket, so there is nothing to press, no
port to publish and no account to authenticate. What this gate asks is the four
things the box asks:

    the tab carries the console sentence and no set-up button exists here
    a command sent from the Server tab's own probe answers within about 5 s
    a reply window with no prompt reads as could-not-ask, never as failure
    the app never offers a mutation it cannot verify on this transport

Usage:  python gate82e.py <stage>
        tab      the Server tab: the sentence, and every button on it
        probe    the probe pressed for real, and how long the console took
        noprompt the same probe against a world that prints no prompt
        surface  what this tree is and is NOT offered, read from the services
"""

from __future__ import annotations

import os
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate82e" / "pylauncher"))

from yulon import channel as channel_module  # noqa: E402
from yulon import commands, docker  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "tortoise-server"
ENTRY = load_catalog().get("wow-tortoise")
SPEC = ENTRY.container_spec()
SHOTS = Path.home() / "gate82e-shots"


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def services() -> ControllerServices:
    return ControllerServices.for_entry(ENTRY, SERVER_DIR)


def view():
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    QApplication.instance() or QApplication([])
    built = ControllerView(ENTRY, services(), status_poll_ms=0, job_runner=run_inline)
    built.refresh_status()
    built.refresh_verdict()
    built.refresh_channel()
    return built


def shoot(built, name: str) -> None:
    SHOTS.mkdir(parents=True, exist_ok=True)
    built.resize(940, 560)
    built.grab().save(str(SHOTS / f"{name}.png"))
    say(f"wrote {SHOTS / name}.png")


def stage_tab() -> None:
    """The sentence, and the absence the box asks for."""
    built = view()
    say(f"world: {docker.container_state(SPEC.world).status}")
    say(f"channel line: {built.channel_label.text()!r}")
    say(f"channel line shown: {built.channel_label.isVisibleTo(built)}")
    say(f"ENABLE button exists at all: {built.enable_channel_button.isVisibleTo(built)}")
    say(f"REPAIR button exists at all: {built.repair_channel_button.isVisibleTo(built)}")
    say(f"TEST button: {built.test_console_button.isVisibleTo(built)}")
    assert not built.enable_channel_button.isVisibleTo(built), "there is a set-up button"
    assert not built.repair_channel_button.isVisibleTo(built), "there is a repair button"
    assert "console" in built.channel_label.text().lower()
    shoot(built, "1-the-tab")


def stage_probe() -> None:
    """The probe pressed for real, and the clock the box puts on it."""
    built = view()
    started = time.monotonic()
    built.test_console()
    took = time.monotonic() - started
    said = built.console_probe_label.text()
    say(f"the probe took {took:.1f}s")
    for line in said.splitlines()[:8]:
        say(f"    {line}")
    shoot(built, "2-the-probe-answered")
    assert took < 5.0, f"the console took {took:.1f}s, and the box says about five"
    assert "could not ask" not in said.lower(), said
    assert said.strip(), "the probe said nothing at all"


def stage_noprompt() -> None:
    """A reply window with no prompt in it, produced the way the box means it.

    A short window does NOT produce one on this fork: it prints so much of its
    own SQL that a prompt from an earlier command is usually already sitting in
    the buffer, and a 0.05s window came back fully delimited. The honest case is
    the one the transport's own docstring names -- a worldserver that is UP and
    has not printed its first prompt yet, because it is still loading. That is
    minutes on this core, and it is what a person hits if they press the button
    right after Start.

    So: stop, start, and ask while it loads. Nothing is written, and the world
    is left running.
    """
    svc = services()
    say("stopping the world, then starting it, to ask while it loads")
    svc.controller.stop()
    svc.controller.start()
    for _ in range(60):
        if docker.container_state(SPEC.world).status == "running":
            break
        time.sleep(2)
    say(f"world: {docker.container_state(SPEC.world).status}, and still loading")

    built = view()
    built.test_console()
    said = built.console_probe_label.text()
    say(f"the tab says: {said!r}")
    shoot(built, "3-no-prompt")
    assert said.lower().startswith("could not ask"), said
    assert "may still have run" in said.lower(), said
    assert said.lower().count("may still have run") == 1, said
    for claim in ("the command failed", "failed to", "could not run"):
        assert claim not in said.lower(), said
    say("PASSED: an undelimited window reads as could-not-ask, and says why")
    say("leaving the world to finish loading")


def stage_surface() -> None:
    """What this tree is offered, and what it is not, read from the services."""
    svc = services()
    say(f"channel_setup : {svc.channel_setup!r} (nothing to set up on this core)")
    say(f"console_probe : {'wired' if svc.console_probe is not None else 'MISSING'}")
    say(f"accounts      : {svc.accounts!r} (8.3d, not this box)")
    say(f"bots          : {'wired' if svc.bots is not None else 'not wired'} (8.5d)")
    say(f"operations    : {ENTRY.operations}")
    assert svc.channel_setup is None, "a core with no listener was given a set-up state machine"
    assert svc.console_probe is not None, "the only Phase 8 surface on this tree is missing"
    assert ENTRY.operations is not None and ENTRY.operations.channel == "attach"
    assert ENTRY.operations.port is None and ENTRY.operations.namespace is None
    say("PASSED: a channel with nothing to switch on, and one probe that uses it")


if __name__ == "__main__":
    {
        "tab": stage_tab,
        "probe": stage_probe,
        "noprompt": stage_noprompt,
        "surface": stage_surface,
    }[sys.argv[1]]()
