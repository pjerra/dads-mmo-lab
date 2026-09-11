"""Steam Resume probe: one top-level Qt window, OpenGL-backed or raster.

Launched from a Steam shortcut it tells whether Steam's overlay can hook an
OpenGL-backed Qt window, and so whether Big Picture's Resume can raise it.
"""
import sys
from PySide6.QtWidgets import QApplication, QMainWindow, QLabel
from PySide6.QtOpenGLWidgets import QOpenGLWidget

app = QApplication(sys.argv)
w = QMainWindow()
mode = sys.argv[1] if len(sys.argv) > 1 else "gl"
w.setWindowTitle("yulon-glprobe-" + mode)
w.setCentralWidget(QOpenGLWidget() if mode == "gl" else QLabel("raster probe"))
w.resize(600, 400)
w.show()
sys.exit(app.exec())
