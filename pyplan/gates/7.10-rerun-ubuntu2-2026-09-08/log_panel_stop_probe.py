"""Why did the LogPanel's Stop button leave the follow running? Measured, twice, two sources.

`widget_driver.py` reported its one FAIL here: after `QTest.mouseClick` on the panel's Stop,
`panel.cancelled` was True (so `stop()` really ran) and `panel.running` was still True 60 s
later, and the process then aborted at interpreter shutdown with
`QThread: Destroyed while thread '' is still running` (exit 134).

This probe asks the same question with a stopwatch on it, and it asks it of TWO sources so the
answer cannot be "the box":

  A. the real one -- `ControllerServices.logs_source()`, i.e. `docker logs -f ac-worldserver`
     on a world with 500 playerbots printing;
  B. a synthetic generator that yields one line every 50 ms and nothing else.

If B stops at once and A does not, the difference is the torrent, not the button. If both
hang, the button is broken at this tip. Either way the numbers are here rather than the
adjective.

After the click it polls every 0.2 s and prints, from the GUI thread:

  * `panel.running` (`QThread.isRunning()`, a direct C++ query),
  * `panel.cancelled` (`_stop_requested`, set by `stop()`),
  * the worker's own `_stop` flag,
  * how many lines have been appended so far,
  * how long each `processEvents()` call itself took -- because a GUI thread buried in a
    backlog of queued `line` signals is a different failure from a worker that never breaks,
    and the driver's `pump()` cannot tell them apart.

Read-only: it follows a log and prints. It starts, stops and writes nothing.
"""

from __future__ import annotations

import os
import sys
import time
from collections.abc import Iterator
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

from PySide6.QtCore import Qt  # noqa: E402
from PySide6.QtTest import QTest  # noqa: E402
from PySide6.QtWidgets import QApplication, QPushButton  # noqa: E402

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402
from yulon.ui.widgets.log_panel import LogPanel  # noqa: E402

SERVER_DIR = Path("/home/pk/wowserver")
POLL_SECONDS = 120.0


def button_named(widget, text: str) -> QPushButton | None:
    for button in widget.findChildren(QPushButton):
        if button.text() == text:
            return button
    return None


def slow_source() -> Iterator[str]:
    """One line every 50 ms, forever. Nothing to do with docker."""
    index = 0
    while True:
        time.sleep(0.05)
        index += 1
        yield f"synthetic line {index}"


def drive(app: QApplication, label: str, source) -> None:
    print(f"\n{'=' * 78}\n{label}\n{'=' * 78}", flush=True)
    panel = LogPanel()
    panel.resize(900, 600)
    panel.show()
    stop_button = button_named(panel, "Stop")
    print(f"Stop button enabled while idle: {stop_button.isEnabled()}", flush=True)

    panel.run(source, title=label)
    deadline = time.time() + 30
    while time.time() < deadline and not (panel.running and stop_button.isEnabled()):
        app.processEvents()
        time.sleep(0.02)
    print(f"after run(): running={panel.running} stop_enabled={stop_button.isEnabled()}",
          flush=True)

    deadline = time.time() + 30
    while time.time() < deadline and len(panel.text().splitlines()) < 3:
        app.processEvents()
        time.sleep(0.02)
    lines_before = len(panel.text().splitlines())
    print(f"lines in the panel before the click: {lines_before}", flush=True)

    worker = panel._worker  # noqa: SLF001 -- the probe is about this object's own flag
    clicked_at = time.time()
    print(f"CLICK at {time.strftime('%FT%T%z')}", flush=True)
    QTest.mouseClick(stop_button, Qt.MouseButton.LeftButton)

    last_report = 0.0
    stopped_after: float | None = None
    while time.time() - clicked_at < POLL_SECONDS:
        pump_started = time.monotonic()
        app.processEvents()
        pump_took = time.monotonic() - pump_started
        elapsed = time.time() - clicked_at
        if not panel.running and stopped_after is None:
            stopped_after = elapsed
            print(f"  [{elapsed:6.2f}s] RUNNING WENT FALSE  (processEvents took "
                  f"{pump_took:.3f}s)", flush=True)
            break
        if elapsed - last_report >= 1.0:
            last_report = elapsed
            print(f"  [{elapsed:6.2f}s] running={panel.running} cancelled={panel.cancelled} "
                  f"worker._stop={getattr(worker, '_stop', '?')} "
                  f"lines={len(panel.text().splitlines())} "
                  f"processEvents took {pump_took:.3f}s", flush=True)
        time.sleep(0.02)

    if stopped_after is None:
        print(f"  NEVER STOPPED within {POLL_SECONDS:.0f}s: running={panel.running} "
              f"cancelled={panel.cancelled} worker._stop={getattr(worker, '_stop', '?')} "
              f"lines={len(panel.text().splitlines())}", flush=True)
    else:
        print(f"  stopped {stopped_after:.2f}s after the click", flush=True)

    print(f"  panel.wait(10000) -> {panel.wait(10000)}", flush=True)
    print(f"  after wait: running={panel.running} status={panel.status_text()!r}", flush=True)


def main() -> int:
    app = QApplication([])
    entry = load_catalog().get("wow-wotlk")
    services = ControllerServices.for_entry(entry, SERVER_DIR)

    which = sys.argv[1] if len(sys.argv) > 1 else "both"
    if which in ("both", "slow"):
        drive(app, "B -- a synthetic generator, one line every 50 ms", slow_source)
    if which in ("both", "live"):
        drive(app, "A -- the real worldserver follow (docker logs -f)", services.logs_source)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
