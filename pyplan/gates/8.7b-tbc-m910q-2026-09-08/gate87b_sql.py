"""8.7b step 9: the SQL mod, installed by the same Modules-tab button press."""
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


def q(sql: str) -> str:
    r = subprocess.run(["docker", "exec", "tbc-db", "mariadb", "-uroot", f"-p{PW}",
                        "-N", "-B", "mangos", "-e", sql], capture_output=True, text=True)
    return r.stdout.strip()


def pump(app, view, seconds=300.0):
    end = time.time() + seconds
    while view._module_pending is not None and time.time() < end:
        app.processEvents(); time.sleep(0.05)
    for _ in range(20):
        app.processEvents(); time.sleep(0.02)


def press(view, app, item_id, button):
    row = [i for i in range(view.module_list.count())
           if view.module_list.item(i).data(256) == item_id][0]
    view.module_list.setCurrentRow(row)
    app.processEvents()
    button.click()
    pump(app, view)
    return view.module_report.toPlainText()


def main() -> int:
    app = QApplication.instance() or QApplication([])
    entry = load_catalog().get("wow-tbc")
    view = ControllerView(entry, ControllerServices.for_entry(entry, SERVER))
    view.resize(1100, 780)
    view._tabs.setCurrentIndex([i for i in range(view._tabs.count())
                                if view._tabs.tabText(i) == "Modules"][0])
    app.processEvents()

    print("mangosd running at press time:", q("SELECT 1") and subprocess.run(
        ["docker", "inspect", "-f", "{{.State.Running}}", "tbc-mangosd"],
        capture_output=True, text=True).stdout.strip())

    print("\n--- pressing 'Install selected' on all-stackables ---")
    print(press(view, app, "all-stackables", view.install_module_button))
    view.grab().save(str(SHOTS / "8-modules-tab-sql-installed.png"))

    print("stackable > 1 :", q("SELECT COUNT(*) FROM item_template WHERE stackable > 1"))
    print("stackable = 200:", q("SELECT COUNT(*) FROM item_template WHERE stackable = 200"))
    print("backup rows    :", q("SELECT COUNT(*) FROM yulon_stackable_backup"))

    print("\n--- pressing 'Remove selected' on all-stackables ---")
    print(press(view, app, "all-stackables", view.remove_module_button))
    view.grab().save(str(SHOTS / "9-modules-tab-sql-removed.png"))

    print("stackable > 1 :", q("SELECT COUNT(*) FROM item_template WHERE stackable > 1"))
    print("stackable = 200:", q("SELECT COUNT(*) FROM item_template WHERE stackable = 200"))
    print("backup table gone:", q(
        "SELECT COUNT(*) FROM information_schema.tables "
        "WHERE table_schema='mangos' AND table_name='yulon_stackable_backup'"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
