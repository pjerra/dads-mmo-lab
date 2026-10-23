"""Headless `--help` for the ssh box (Phase 1 task 1).

The box has no display. `--help` must answer before Qt loads (spike S1: with no
verb the app opened the GUI, which aborts headless with no platform plugin).
"""

from __future__ import annotations

import sys

import pytest

import main


def _qt_module_names() -> set[str]:
    """The Qt modules loaded right now (`PySide6*`, `shiboken*`), if any."""
    return {
        name
        for name in sys.modules
        if name == "PySide6" or name.startswith(("PySide6.", "shiboken"))
    }


@pytest.mark.parametrize("flag", ["--help", "-h"])
def test_help_prints_the_verbs_and_leaves_before_qt(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str], flag: str
) -> None:
    """`--help` answers with the verbs and exit 0, without loading Qt."""
    before = _qt_module_names()
    monkeypatch.setattr(sys, "argv", ["yulon", flag])
    assert main.main() == 0
    out = capsys.readouterr().out
    assert "--version" in out
    assert "--server" in out and "status" in out
    assert "--provision" in out
    assert _qt_module_names() == before
    widgets = sys.modules.get("PySide6.QtWidgets")
    assert widgets is None or widgets.QApplication.instance() is None

