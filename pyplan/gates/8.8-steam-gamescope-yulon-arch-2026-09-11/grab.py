"""Grab an X window with Qt (this box has no screenshot tool).

gamescope headless draws nothing to :0, so the VM console screenshot cannot show
the gamepad UI. Its XWayland server is a real X server, and Qt`s own
QScreen.grabWindow(<wid>) reads a window off it. gamescope redirects its clients
so the ROOT window is empty; the window id must be given.

argv: <out.png> [window-id, decimal or 0x..]; 0 (the default) is the root.
"""
import os
import sys

from PySide6.QtGui import QGuiApplication

app = QGuiApplication(sys.argv[:1])
wid = int(sys.argv[2], 0) if len(sys.argv) > 2 else 0
img = QGuiApplication.primaryScreen().grabWindow(wid)
img.save(sys.argv[1])
print("DISPLAY", os.environ.get("DISPLAY"), "wid", wid, "->", sys.argv[1], img.width(), "x", img.height())
