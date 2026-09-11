"""Press one of the Modules tab's two buttons, through the app's own handler.

`sys.argv[1]` is `adopt` or `updates`; every output file is prefixed with it, so
the two presses have separate frames, separate panels and separate SQL traces.

Everything the app ships is real here: `ControllerServices.for_entry()` builds
the wiring, `ControllerView` builds the tab, and the button's own handler
(`adopt_as_imported()` / `apply_database_updates()`) is what runs. Two things a
headless box makes necessary, both narrow and both stated in the README:

* `QMessageBox.question` is replaced by a function that builds the SAME dialog
  from the same arguments, shows it, photographs it and answers Yes. A real
  `exec()` blocks forever with nobody to click it.
* Frames are `QWidget.grab()` of the real widgets, not a desktop screenshot.

The adopt button's enabling reading is NOT set by hand: `refresh_status()` is
called and the event loop pumped until the tab has taken it through
`_ask_about_the_import()`, which is the app's own once-per-database-start rule.

Every subprocess this press starts is recorded with its argv and its stdin, with
the install's database password redacted, so the trace can be read for exactly
which statements reached the database and for the absence of everything else.
"""

from __future__ import annotations

import subprocess
import sys
import time
from datetime import datetime
from pathlib import Path

WHICH = sys.argv[1]
assert WHICH in ("adopt", "updates")

SERVER = Path("/home/pk/tortoise-server")
OUT = Path("/home/pk/t19-live/out")
OUT.mkdir(parents=True, exist_ok=True)

PASSWORD = (SERVER / ".db_password").read_text().strip()
TRACE = OUT / f"sql-trace-{WHICH}.txt"
_trace_lines: list[str] = []


def _hide(text: str) -> str:
    return text.replace(PASSWORD, "<REDACTED-DB-PASSWORD>") if PASSWORD else text


def _record(what: str, argv: object, stdin: object = None) -> None:
    _trace_lines.append(f"--- {what}: {_hide(str(argv))}")
    if stdin:
        for line in _hide(str(stdin)).splitlines():
            _trace_lines.append(f"    | {line}")


_real_run = subprocess.run
_real_popen = subprocess.Popen


def traced_run(*args: object, **kwargs: object):  # type: ignore[no-untyped-def]
    _record("run", args[0] if args else kwargs.get("args"), kwargs.get("input"))
    return _real_run(*args, **kwargs)  # type: ignore[arg-type]


class TracedPopen(_real_popen):  # type: ignore[misc,valid-type]
    def __init__(self, *args: object, **kwargs: object) -> None:
        _record("popen", args[0] if args else kwargs.get("args"))
        super().__init__(*args, **kwargs)  # type: ignore[arg-type]

    def communicate(self, input: object = None, timeout: object = None):  # type: ignore[no-untyped-def,override]
        if input:
            _record("popen-stdin", "(stdin of the call above)", input)
        return super().communicate(input, timeout)  # type: ignore[arg-type]


subprocess.run = traced_run  # type: ignore[assignment]
subprocess.Popen = TracedPopen  # type: ignore[misc,assignment]

from PySide6.QtCore import QEventLoop, QTimer  # noqa: E402
from PySide6.QtWidgets import QApplication, QMessageBox  # noqa: E402

from yulon import docker as docker_module  # noqa: E402

# `exec_stdin()` writes the SQL to the child's stdin through `_pump`, not through
# `communicate()`, so this is where the STATEMENTS are readable. Wrapping the
# module global reaches it: `exec_stdin` looks `_pump` up at call time. Without
# this the trace shows argv only, and "one INSERT and nothing else" would be a
# claim about a pipe nobody read.
_real_pump = docker_module._pump


def traced_pump(source, sink, container):  # type: ignore[no-untyped-def]
    data = source.read()
    _record("stdin", f"(into {container})", data.decode("utf-8", "replace"))
    import io

    return _real_pump(io.BytesIO(data), sink, container)


docker_module._pump = traced_pump  # type: ignore[assignment]

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices, ControllerView  # noqa: E402


def stamp() -> str:
    return f"{datetime.now().astimezone():%Y-%m-%d %H:%M:%S %Z}"


print(f"=== press {WHICH} ===  {stamp()}")

entry = load_catalog().get("wow-tortoise")
app = QApplication(sys.argv)

services = ControllerServices.for_entry(entry, SERVER)
print("services.updates is None:", services.updates is None)
print("services.adopt is None:", services.adopt is None)
view = ControllerView(entry, services, status_poll_ms=0)
view.resize(1180, 900)
view.show()

for index in range(view._tabs.count()):
    if view._tabs.tabText(index) == "Modules":
        view._tabs.setCurrentIndex(index)
        break
app.processEvents()

# The tab takes the adopt button's enabling reading once per time the database
# comes up, off `_apply_status()`. `status_poll_ms=0` means nothing puts that
# question by itself, so this asks for a status refresh -- the app's own slot,
# the one the five-second poll calls -- and pumps the loop until the answer has
# landed. Nothing here sets `_adopt_state`; the tab sets it from its own probe.
print(f"adopt reading requested at {stamp()}")
view.refresh_status()
deadline = time.monotonic() + 180
while time.monotonic() < deadline and view._adopt_state is None:
    app.processEvents()
    time.sleep(0.05)
print("view._adopt_state:", view._adopt_state)
print(f"adopt reading in hand at {stamp()}")

print("adopt button label:", view.adopt_button.text())
print("adopt button enabled:", view.adopt_button.isEnabled())
print("updates button label:", view.updates_button.text())
print("updates button enabled:", view.updates_button.isEnabled())
print("rebuild button enabled:", view.rebuild_button.isEnabled())
view.grab().save(str(OUT / f"frame-{WHICH}-1-modules-tab.png"))
view.adopt_button.grab().save(str(OUT / f"frame-{WHICH}-1b-adopt-button.png"))
view.updates_button.grab().save(str(OUT / f"frame-{WHICH}-1c-updates-button.png"))

failures: list[str] = []
view.action_failed.connect(failures.append)

_real_question = QMessageBox.question
shown: list[str] = []


def photographed_question(parent, title, text, buttons=None, default=None):  # type: ignore[no-untyped-def]
    """The app's own dialog, built from the app's own arguments, then answered Yes."""
    shown.append(f"TITLE: {title}\n\n{text}")
    box = QMessageBox(parent)
    box.setWindowTitle(title)
    box.setText(text)
    if buttons is not None:
        box.setStandardButtons(buttons)
    if default is not None:
        box.setDefaultButton(default)
    box.show()
    app.processEvents()
    box.grab().save(str(OUT / f"frame-{WHICH}-2-confirmation.png"))
    print("default button is No:", box.defaultButton() is box.button(QMessageBox.StandardButton.No))
    box.close()
    app.processEvents()
    return QMessageBox.StandardButton.Yes


QMessageBox.question = staticmethod(photographed_question)  # type: ignore[assignment]

handler = view.adopt_as_imported if WHICH == "adopt" else view.apply_database_updates
print(f"pressing the {WHICH} button through its own handler at {stamp()}")
started = handler()
print("handler returned:", started)

loop = QEventLoop()
done: list[tuple[bool, str]] = []


def finished(ok: bool, message: str) -> None:
    done.append((ok, message))
    loop.quit()


view.rebuild_log.run_finished.connect(finished)
if started:
    QTimer.singleShot(1_800_000, loop.quit)
    loop.exec()
app.processEvents()

view.grab().save(str(OUT / f"frame-{WHICH}-3-report.png"))
view.rebuild_log.grab().save(str(OUT / f"frame-{WHICH}-3b-report-panel.png"))
view.grab().save(str(OUT / f"frame-{WHICH}-4-after.png"))

(OUT / f"confirmation-{WHICH}.txt").write_text("\n\n".join(shown), encoding="utf-8")
(OUT / f"panel-{WHICH}.txt").write_text(view.rebuild_log._text.toPlainText(), encoding="utf-8")
(OUT / f"action-failed-{WHICH}.txt").write_text("\n".join(failures), encoding="utf-8")
print("run_finished:", done)
print("action_failed:", failures)
print("adopt button enabled after the press:", view.adopt_button.isEnabled())
print("updates button enabled after the press:", view.updates_button.isEnabled())

TRACE.write_text("\n".join(_trace_lines) + "\n", encoding="utf-8")
print("trace lines:", len(_trace_lines))
print(f"=== done ===  {stamp()}")
