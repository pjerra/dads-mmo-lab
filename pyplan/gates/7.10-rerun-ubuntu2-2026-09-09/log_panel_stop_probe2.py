"""Does the LogPanel's Stop button end a follow that is blocked between lines? A, B and C.

The 2026-09-08 run measured that it did not: after `QTest.mouseClick` on the panel's real
Stop, `panel.cancelled` went True and `panel.running` was STILL True 120 s later, and the
process aborted with `QThread: Destroyed while thread '' is still running` (exit 134).
`pyplan/gates/7.10-rerun-ubuntu2-2026-09-08/log-panel-stop-probe.txt` is that measurement.
`d56f91b7` is the fix. This probe is the re-measurement, and it is built so that a PASS
cannot be an accident of how chatty the box happened to be.

Three sources through the same panel and the same button:

  A. the real one -- `ControllerServices.logs_source()`, `docker logs -f ac-worldserver`.
     This is the production path and the one the widget driver clicks. Its verdict is only
     as discriminating as the world's silence, WHICH IS WHY THIS PROBE PRINTS THE WORLD'S
     LINE RATE FIRST: a chatty world would stop even on the broken code, and a reader must
     be able to tell those two passes apart.
  B. a synthetic generator, one line every 50 ms. The control. It stopped in 0.02 s on the
     broken code too, and it has no child process, so `end_streams_started_on()` cannot
     reach it -- it stops because the worker's flag is read between lines. It is here to
     show the harness itself is not what changed.
  C. `runner.stream(["bash","-lc","echo ...; echo ...; sleep 3600"])` -- a REAL child that
     prints twice and then goes silent forever. DETERMINISTICALLY quiet: no measurement of
     the box can make C chatty, so C is the source on which the broken code must hang and
     the fixed code must not. C is the discriminating clause; A is the production one.

Which `yulon` package is imported is decided by PYTHONPATH and printed on the first line, so
the same file measures the pre-fix tree (`~/lane710b/oldpylauncher`) and the post-fix one
(`~/lane710b/checkout/pylauncher`) with nothing else different.

Read-only: it follows a log and runs a `sleep`. It starts, stops and writes nothing.
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
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402
from yulon.ui.widgets.log_panel import LogPanel  # noqa: E402

SERVER_DIR = Path("/home/pk/wowserver")
POLL_SECONDS = float(os.environ.get("PROBE_POLL_SECONDS", "90"))


def button_named(widget, text: str) -> QPushButton | None:
    for button in widget.findChildren(QPushButton):
        if button.text() == text:
            return button
    return None


def world_line_rate(seconds: float = 20.0) -> None:
    """How many lines does the world print, unprompted, in `seconds`? Printed, not assumed.

    THE GROUND FOR CLAUSE A. On 2026-09-08 the same click passed on 2026-09-05's box and
    failed here, and the whole difference was that one worldserver printed inside the
    60-second window and the other did not. So the rate is read before the presses and put
    at the top of the log: a reader can then see whether A's verdict was discriminating on
    this box on this night, or whether only C's was.
    """
    print(f"\n--- how quiet is the world? counting NEW lines for {seconds:.0f}s "
          f"({time.strftime('%FT%T%z')}) ---", flush=True)
    proc = subprocess.Popen(
        ["docker", "logs", "-f", "--since", "0s", "ac-worldserver"],
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
    )
    time.sleep(seconds)
    proc.terminate()
    try:
        out, _ = proc.communicate(timeout=10)
    except subprocess.TimeoutExpired:
        proc.kill()
        out, _ = proc.communicate()
    count = len([ln for ln in out.splitlines() if ln.strip()])
    print(f"new worldserver lines in {seconds:.0f}s: {count}  "
          f"({count / seconds:.2f} lines/s)", flush=True)
    if count:
        for line in out.splitlines()[:3]:
            print(f"    {line}", flush=True)
    print("a world printing at this rate would break a blocked read on its own within "
          f"{'seconds' if count else 'NEVER -- it is silent'}", flush=True)


def slow_source() -> Iterator[str]:
    """One line every 50 ms, forever. No child process at all."""
    index = 0
    while True:
        time.sleep(0.05)
        index += 1
        yield f"synthetic line {index}"


def quiet_child_source() -> Iterator[str]:
    """A real child that prints twice and then is silent for an hour."""
    yield from runner.stream(
        ["bash", "-lc", "echo 'quiet child line 1'; echo 'quiet child line 2'; sleep 3600"]
    )


def drive(app: QApplication, label: str, source) -> bool:
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
    # Read AFTER the lines have arrived and BEFORE the click, so the reader can see that the
    # thing being stopped was actually running and actually blocked.
    settle = 3.0
    time.sleep(settle)
    app.processEvents()
    lines_settled = len(panel.text().splitlines())
    print(f"lines after a further {settle:.0f}s of NOT clicking: {lines_settled} "
          f"(+{lines_settled - lines_before}) -- "
          f"{'BLOCKED between lines' if lines_settled == lines_before else 'still arriving'}",
          flush=True)

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

    waited = panel.wait(10000)
    print(f"  panel.wait(10000) -> {waited}", flush=True)
    print(f"  status the instant the thread joined, BEFORE pumping: "
          f"{panel.status_text()!r}", flush=True)
    # `finished` is a QUEUED signal: joining the worker thread does not deliver it. The
    # 05:00 press read the header here and got the job title on A and cancelled on B
    # and C -- a race in this probe, not a difference in the product, and it is settled
    # by pumping once and reading again rather than by leaving two spellings in the log.
    QApplication.instance().processEvents()
    print(f"  after wait: running={panel.running} status={panel.status_text()!r}", flush=True)
    print(f"  VERDICT {label.split(' --')[0]}: "
          f"{'STOPPED' if stopped_after is not None else 'HUNG'}", flush=True)
    return stopped_after is not None


def main() -> int:
    import yulon

    print("yulon package under test:", Path(yulon.__file__).resolve(), flush=True)
    print("runner has end_streams_started_on:",
          hasattr(runner, "end_streams_started_on"), flush=True)
    print(f"POLL_SECONDS = {POLL_SECONDS}", flush=True)
    app = QApplication([])
    entry = load_catalog().get("wow-wotlk")
    services = ControllerServices.for_entry(entry, SERVER_DIR)

    which = sys.argv[1] if len(sys.argv) > 1 else "both"
    verdicts: dict[str, bool] = {}
    if which in ("both", "rate"):
        world_line_rate()
    if which in ("both", "slow"):
        verdicts["B synthetic"] = drive(
            app, "B -- a synthetic generator, one line every 50 ms", slow_source)
    if which in ("both", "quiet"):
        verdicts["C quiet child"] = drive(
            app, "C -- a real child that prints twice then sleeps 3600", quiet_child_source)
    if which in ("both", "live"):
        verdicts["A live worldserver"] = drive(
            app, "A -- the real worldserver follow (docker logs -f)", services.logs_source)

    print("\n" + "=" * 78, flush=True)
    for name, stopped in verdicts.items():
        print(f"{name:22s} {'STOPPED' if stopped else 'HUNG'}", flush=True)
    print("=" * 78, flush=True)
    return 0 if all(verdicts.values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
