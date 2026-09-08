"""The Modules tab at 13fd052d^ -- the ground frame, before the two buttons existed."""
from __future__ import annotations
import os, sys, time
from pathlib import Path
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
from PySide6.QtWidgets import QApplication

OUT = Path(sys.argv[1]); OUT.mkdir(parents=True, exist_ok=True)
app = QApplication([])
from yulon.catalog.catalog import load_catalog
from yulon.ui.controller_view import ControllerServices, ControllerView

wotlk = load_catalog().get("wow-wotlk")
server_dir = Path(sys.argv[2]); server_dir.mkdir(parents=True, exist_ok=True)
services = ControllerServices.for_entry(wotlk, server_dir)
view = ControllerView(wotlk, services, status_poll_ms=0)
view.resize(1100, 760)
tabs = view._tabs
idx = [i for i in range(tabs.count()) if tabs.tabText(i) == "Modules"][0]
tabs.setCurrentIndex(idx)
view.show(); app.processEvents()
pix = view.grab(); pix.save(str(OUT / "1-modules-tab-before.png"))
present = [n for n in ("module_link_button", "module_folder_button") if hasattr(view, n)]
missing = [n for n in ("module_from_link", "module_from_folder", "module_install_custom",
                       "module_forget") if not hasattr(services, n)]
stamp = time.strftime("%Y-%m-%dT%H:%M:%S%z")
with (OUT / "alive.txt").open("a", encoding="utf-8") as fh:
    fh.write(f"{stamp} pid={os.getpid()} alive 1-modules-tab-before.png "
             f"{pix.width()}x{pix.height()} parent commit 13fd052d^\n")
(OUT / "1-modules-tab-before.txt").write_text(
    f"buttons present on the view at 13fd052d^: {present}\n"
    f"ControllerServices fields ABSENT at 13fd052d^: {missing}\n", encoding="utf-8")
print("before:", pix.width(), "x", pix.height(), "buttons:", present, "absent fields:", missing)
