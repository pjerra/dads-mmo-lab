"""8.7b addendum: can a SQL mod be installed the way owner answer 7 requires?

Owner answer 7 (2026-09-06, quoted in `pyplan/write-ledger.md`): *no Phase 8
feature writes `characters` or `world` while the world server is up*, and the
ledger's row for `apply.py::_run_sql::run_statement` says 8.7 is where the
applier's guard lands.

`all-stackables` writes the world schema. So the compliant sequence is: press
Stop on the Server tab, install, press Start. This asks the machine whether
that sequence exists on this tree, instead of assuming it does.
"""
from __future__ import annotations
import os, subprocess, sys, time
from pathlib import Path
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
sys.path.insert(0, "/home/pk/gate87b/pylauncher")
from PySide6.QtWidgets import QApplication  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerView, ControllerServices  # noqa: E402

SHOTS = Path("/home/pk/gate87b/evidence")
SERVER = Path("/home/pk/tbc-7.4c")
PW = (SERVER / ".db_password").read_text().strip()


def census() -> str:
    r = subprocess.run(["docker", "ps", "-a", "--filter", "name=tbc-",
                        "--format", "{{.Names}} {{.State}}"], capture_output=True, text=True)
    return " | ".join(sorted(r.stdout.split("\n")) ).strip(" |")


def q(sql: str) -> str:
    r = subprocess.run(["docker", "exec", "tbc-db", "mariadb", "-uroot", f"-p{PW}",
                        "-N", "-B", "mangos", "-e", sql], capture_output=True, text=True)
    return (r.stdout.strip() or ("ERR: " + r.stderr.strip()[:200]))


def pump_busy(app, view, seconds=900.0):
    end = time.time() + seconds
    while view._busy and time.time() < end:
        app.processEvents(); time.sleep(0.1)
    for _ in range(20):
        app.processEvents(); time.sleep(0.02)


def pump_mod(app, view, seconds=300.0):
    end = time.time() + seconds
    while view._module_pending is not None and time.time() < end:
        app.processEvents(); time.sleep(0.05)
    for _ in range(20):
        app.processEvents(); time.sleep(0.02)


def main() -> int:
    app = QApplication.instance() or QApplication([])
    entry = load_catalog().get("wow-tbc")
    view = ControllerView(entry, ControllerServices.for_entry(entry, SERVER))
    view.resize(1100, 780)
    tabs = view._tabs
    t_mod = [i for i in range(tabs.count()) if tabs.tabText(i) == "Modules"][0]

    print("census, everything up:", census())
    print("ground stackable>1 :", q("SELECT COUNT(*) FROM item_template WHERE stackable > 1"))
    print("ground stackable=200:", q("SELECT COUNT(*) FROM item_template WHERE stackable = 200"))

    print("\n--- pressing Stop on the Server tab, as owner answer 7 requires ---")
    tabs.setCurrentIndex(0)
    view.stop_server(); pump_busy(app, view)
    print("status label:", view.status_label.text())
    print("census after Stop:", census())
    print("is the DATABASE still reachable?", q("SELECT 1"))

    print("\n--- now pressing Install selected on all-stackables, world down ---")
    tabs.setCurrentIndex(t_mod)
    row = [i for i in range(view.module_list.count())
           if view.module_list.item(i).data(256) == "all-stackables"][0]
    view.module_list.setCurrentRow(row)
    app.processEvents()
    view.install_module_button.click()
    pump_mod(app, view)
    print("what the user reads in the report box:")
    print(view.module_report.toPlainText())
    view.grab().save(str(SHOTS / "14-sql-mod-with-the-world-stopped.png"))

    print("\n--- putting it back: Start, then read the database ---")
    tabs.setCurrentIndex(0)
    view.start_server(); pump_busy(app, view)
    print("census after Start:", census())
    time.sleep(5)
    print("stackable>1  :", q("SELECT COUNT(*) FROM item_template WHERE stackable > 1"))
    print("stackable=200:", q("SELECT COUNT(*) FROM item_template WHERE stackable = 200"))
    print("backup table :", q("SELECT COUNT(*) FROM information_schema.tables "
                              "WHERE table_schema='mangos' AND table_name='yulon_stackable_backup'"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
