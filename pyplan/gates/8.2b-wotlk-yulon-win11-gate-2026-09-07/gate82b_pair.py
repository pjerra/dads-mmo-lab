"""The button, with the world stopped, before and after the one-line fix.

Both shots are of the SAME state -- this install, world down -- and differ only
in the predicate behind `enable_channel_button.setEnabled(...)`. The line is put
back the way it was rather than a different build being installed, so what the
pair shows is the fix and nothing else.

Usage:  python gate82b_pair.py
"""

from __future__ import annotations

import os
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

SRC = Path(r"C:\gate\src82b\pylauncher")
VIEW = SRC / "yulon" / "ui" / "controller_view.py"
FIXED = "self.enable_channel_button.setEnabled(_press_is_allowed(result))"
BEFORE = "self.enable_channel_button.setEnabled(result.stable)"

sys.path.insert(0, str(SRC))

from yulon import docker  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402

SERVER_DIR = Path(r"D:\gate\wotlk-server77")
ENTRY = load_catalog().get("wow-wotlk")
SPEC = ENTRY.container_spec()
SHOTS = Path(r"C:\gate\gate82b-shots")


def say(text: str) -> None:
    print(f"[{datetime.now(UTC).strftime('%H:%M:%SZ')}] {text}", flush=True)


def shot(label: str) -> None:
    """A fresh interpreter each time: the module is imported, so it must be reloaded."""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerServices, ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    QApplication.instance() or QApplication([])
    view = ControllerView(
        ENTRY, ControllerServices.for_entry(ENTRY, SERVER_DIR), status_poll_ms=0,
        job_runner=run_inline,
    )
    view.refresh_status()
    view.refresh_verdict()
    view.refresh_channel()
    SHOTS.mkdir(parents=True, exist_ok=True)
    view.resize(940, 520)
    view.grab().save(str(SHOTS / f"{label}.png"))
    say(f"{label}: verdict = {view.verdict_label.text()!r}")
    say(f"{label}: ENABLE button enabled = {view.enable_channel_button.isEnabled()}")
    say(f"{label}: wrote {SHOTS / label}.png")


def main() -> None:
    which = sys.argv[1]
    if which == "stop":
        from yulon.ui.controller_view import ControllerServices  # noqa: PLC0415

        say("stopping the world so both shots are of the state the press requires")
        ControllerServices.for_entry(ENTRY, SERVER_DIR).controller.stop()
        for _ in range(60):
            if docker.container_state(SPEC.world).status != "running":
                break
            time.sleep(5)
        say(f"world: {docker.container_state(SPEC.world).status}")
        return
    if which == "before":
        text = VIEW.read_text(encoding="utf-8")
        assert FIXED in text
        VIEW.write_text(text.replace(FIXED, BEFORE, 1), encoding="utf-8")
        say("put the interlock back the way it was")
        shot("7-button-dead-before-the-fix")
        return
    if which == "after":
        text = VIEW.read_text(encoding="utf-8")
        assert BEFORE in text
        VIEW.write_text(text.replace(BEFORE, FIXED, 1), encoding="utf-8")
        say("restored the fix")
        shot("8-button-live-after-the-fix")
        return
    if which == "start":
        from yulon.ui.controller_view import ControllerServices  # noqa: PLC0415

        ControllerServices.for_entry(ENTRY, SERVER_DIR).controller.start()
        for _ in range(120):
            if docker.container_state(SPEC.world).status == "running":
                break
            time.sleep(5)
        say(f"world: {docker.container_state(SPEC.world).status}")
        return
    raise SystemExit(f"unknown stage {which}")


if __name__ == "__main__":
    main()
