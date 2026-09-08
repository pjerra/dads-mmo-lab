"""8.7b, the definition of done, photographed: the server's own answer in the app.

`.server motd` typed into the app's Console tab and sent with its Send button,
once with the conf already changed and the server NOT yet restarted, and once
after the restart the Modules report asked for. The second shot is the box.
"""
from __future__ import annotations
import os, subprocess, sys, time
from pathlib import Path
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
sys.path.insert(0, "/home/pk/gate87b/pylauncher")
from PySide6.QtWidgets import QApplication  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.controller_wow_tbc import modules as m  # noqa: E402
from yulon.ui.controller_view import ControllerView, ControllerServices  # noqa: E402

SHOTS = Path("/home/pk/gate87b/evidence")
SERVER = Path("/home/pk/tbc-7.4c")
VALUE = "Yulon 8.7b gate"


def alive() -> str:
    r = subprocess.run(["docker", "inspect", "-f",
                        "running={{.State.Running}} started={{.State.StartedAt}}", "tbc-mangosd"],
                       capture_output=True, text=True)
    return r.stdout.strip()


def started_at() -> str:
    return subprocess.run(["docker", "inspect", "-f", "{{.State.StartedAt}}", "tbc-mangosd"],
                          capture_output=True, text=True).stdout.strip()


def wait_ready(since: str, budget: int = 900) -> str:
    t0 = time.time()
    while time.time() - t0 < budget:
        out = subprocess.run(["docker", "logs", "--since", since, "tbc-mangosd"],
                             capture_output=True, text=True)
        if "Avg Diff:" in (out.stdout + out.stderr):
            return "ready after %.0f s" % (time.time() - t0)
        time.sleep(2)
    return "TIMEOUT"


def pump_console(app, view, seconds=120.0):
    end = time.time() + seconds
    while view._console_pending and time.time() < end:
        app.processEvents(); time.sleep(0.05)
    for _ in range(30):
        app.processEvents(); time.sleep(0.02)


def pump_busy(app, view, seconds=900.0):
    end = time.time() + seconds
    while view._busy and time.time() < end:
        app.processEvents(); time.sleep(0.1)
    for _ in range(20):
        app.processEvents(); time.sleep(0.02)


def ask(app, view, tab_console):
    view._tabs.setCurrentIndex(tab_console)
    view.command_edit.setText("server motd")
    app.processEvents()
    view.send_button.click()
    pump_console(app, view)
    panel = view.console_log
    for attr in ("toPlainText", "text"):
        if hasattr(panel, attr):
            try:
                return getattr(panel, attr)()
            except Exception:
                pass
    for child in panel.findChildren(object):
        if hasattr(child, "toPlainText"):
            return child.toPlainText()
    return "(could not read the panel text; the screenshot is the record)"


def main() -> int:
    app = QApplication.instance() or QApplication([])
    entry = load_catalog().get("wow-tbc")
    services = ControllerServices.for_entry(entry, SERVER)
    view = ControllerView(entry, services)
    view.resize(1100, 780)
    tabs = view._tabs
    t_console = [i for i in range(tabs.count()) if tabs.tabText(i) == "Console"][0]

    conf = SERVER / "etc/mangosd.conf"

    def motd_line():
        return [l.rstrip() for l in conf.read_text(errors="replace").splitlines()
                if l.startswith("Motd")]

    print("mangosd:", alive())
    print("ground Motd line:", motd_line())

    print("\n--- installing motd =", repr(VALUE), "through services.applier ---")
    r = services.applier.install(m.store().load("mod", "motd"), {"motd": VALUE})
    print("   done:", r.done, " restart_recommended:", r.restart_recommended)
    print("   Motd line now:", motd_line())

    print("\n--- Console tab, BEFORE the restart (mangosd", alive(), ") ---")
    print(ask(app, view, t_console))
    view.grab().save(str(SHOTS / "11-console-before-restart-still-the-old-motd.png"))

    print("\n--- Stop then Start on the Server tab ---")
    tabs.setCurrentIndex(0)
    view.stop_server(); pump_busy(app, view)
    print("   after Stop :", alive())
    view.start_server(); pump_busy(app, view)
    since = started_at()
    print("   after Start:", alive())
    print("  ", wait_ready(since))

    print("\n--- Console tab, AFTER the restart (mangosd", alive(), ") ---")
    print(ask(app, view, t_console))
    view.grab().save(str(SHOTS / "12-console-after-restart-the-servers-own-answer.png"))

    print("\n--- putting the box back: remove, then restart ---")
    rr = services.applier.remove(m.store().load("mod", "motd"))
    print("   done:", rr.done, " Motd line now:", motd_line())
    tabs.setCurrentIndex(0)
    view.stop_server(); pump_busy(app, view)
    view.start_server(); pump_busy(app, view)
    since = started_at()
    print("  ", wait_ready(since))
    print("   final:", alive())
    print("\n--- Console tab, after the remove ---")
    print(ask(app, view, t_console))
    view.grab().save(str(SHOTS / "13-console-after-remove-back-to-the-shipped-default.png"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
