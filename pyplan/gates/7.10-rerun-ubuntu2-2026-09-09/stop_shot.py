"""Photograph the panel five seconds after Stop, on a source that cannot print in the meantime.

The clause is "the follow stopped after the click". Its live source is `docker logs -f
ac-worldserver`, and tonight's ground press proved that clause is not discriminating on this
box: the pre-fix code passed it too, because a line arrived 26.93 s after the click and the
driver waits 60. Source C -- a real child that prints twice and then sleeps 3600 -- cannot do
that, so C is what gets photographed, under both trees, with nothing else different but
PYTHONPATH.

Each frame records, beside it, whether the CHILD PROCESS was still alive at the moment of the
grab (`pgrep -f 'sleep 3600'`). A panel that has gone quiet because its child died of
something else photographs exactly like a panel whose Stop worked, and the sidecar is what
tells those apart -- the same reason `widget_driver.py` writes a container line beside every
frame it takes.

Writes only its own PNG. Starts and stops one `sleep`.
"""

from __future__ import annotations

import os
import subprocess
import sys
import time
from collections.abc import Iterator
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from PySide6.QtCore import Qt  # noqa: E402
from PySide6.QtTest import QTest  # noqa: E402
from PySide6.QtWidgets import QApplication, QPushButton  # noqa: E402

from yulon import runner  # noqa: E402

TAG = sys.argv[1]
SHOTS = Path("/home/pk/lane710b/out/shots")
MARKER = f"quiet-child-{TAG}-{os.getpid()}"


def quiet_child_source() -> Iterator[str]:
    # `exec -a {MARKER} sleep 3600`, and the -a is the whole point. The first version of this
    # file wrote a plain `sleep 3600`, and bash's last-command optimisation EXEC'd into it:
    # the process that remained had the command line `sleep 3600` with the marker nowhere in
    # it, so `pgrep -f` found nothing and the liveness sidecar printed "0" beside every frame
    # -- including the frames taken while the child was demonstrably alive and streaming.
    # A dead probe reads exactly like a true reading, which is why `children_alive()` now
    # refuses instead of returning 0.
    yield from runner.stream(
        ["bash", "-lc",
         f"echo '{MARKER} line 1'; echo '{MARKER} line 2'; exec -a {MARKER} sleep 3600"]
    )


def children_alive() -> int:
    done = subprocess.run(["pgrep", "-fc", MARKER], capture_output=True, text=True)
    return int(done.stdout.strip() or 0)


def main() -> int:
    import yulon

    print("yulon package under test:", Path(yulon.__file__).resolve(), flush=True)
    print("runner has end_streams_started_on:",
          hasattr(runner, "end_streams_started_on"), flush=True)
    SHOTS.mkdir(parents=True, exist_ok=True)
    app = QApplication([])
    from yulon.ui.widgets.log_panel import LogPanel

    panel = LogPanel()
    panel.resize(900, 400)
    panel.show()
    stop = next(b for b in panel.findChildren(QPushButton) if b.text() == "Stop")

    panel.run(quiet_child_source, title="a quiet child: two lines, then sleep 3600")
    deadline = time.time() + 30
    while time.time() < deadline and len(panel.text().splitlines()) < 2:
        app.processEvents()
        time.sleep(0.02)
    time.sleep(2)
    app.processEvents()
    # THE SIDECAR IS CHECKED BEFORE IT IS TRUSTED. At this point the child is streaming and
    # has not been asked to stop, so a count of zero here can only mean the probe cannot see
    # its own child -- and a probe that cannot see is worse than no probe, because its "0"
    # after the click would read as proof.
    if children_alive() < 1:
        print(f"REFUSING: pgrep cannot see the child ({MARKER!r}) while it is still "
              f"streaming, so its liveness figures would be meaningless", flush=True)
        return 2

    name = f"stop-quiet-child-{TAG}-before-click"
    panel.grab().save(str(SHOTS / f"{name}.png"))
    line = (f"{time.strftime('%FT%T%z')}  {name}.png  running={panel.running} "
            f"child processes alive: {children_alive()}")
    print(f"[shot] {line}", flush=True)
    (SHOTS / "shots.txt").open("a", encoding="utf-8").write(line + "\n")

    print(f"CLICK at {time.strftime('%FT%T%z')}", flush=True)
    QTest.mouseClick(stop, Qt.MouseButton.LeftButton)
    deadline = time.time() + 5
    while time.time() < deadline:
        app.processEvents()
        time.sleep(0.05)

    name = f"stop-quiet-child-{TAG}-5s-after-click"
    panel.grab().save(str(SHOTS / f"{name}.png"))
    line = (f"{time.strftime('%FT%T%z')}  {name}.png  running={panel.running} "
            f"cancelled={panel.cancelled} status={panel.status_text()!r} "
            f"child processes alive: {children_alive()}")
    print(f"[shot] {line}", flush=True)
    (SHOTS / "shots.txt").open("a", encoding="utf-8").write(line + "\n")

    print(f"VERDICT {TAG}: running={panel.running} five seconds after the click", flush=True)
    subprocess.run(["pkill", "-f", MARKER], check=False)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
