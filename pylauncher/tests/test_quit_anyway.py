"""The "Quit anyway" way out of a refused quit (T690).

A refusal with no way past it is a trap: a flag stuck by a bug elsewhere would make
Yu'lon impossible to close. The box names what is running, says plainly what quitting
anyway leaves half-done, and keeps "Keep waiting" as the default.
"""

from __future__ import annotations

from typing import Any

from yulon.ui import quit_anyway

REASON = "A restore is writing into this server's databases on its Maintenance tab."


def test_the_box_names_the_work_and_the_cost_and_waiting_is_the_default(qapp: Any) -> None:
    box, buttons = quit_anyway.build_box(REASON)
    try:
        assert set(buttons) == {"wait", "quit"}
        assert buttons["wait"].text() == "Keep waiting"
        assert buttons["quit"].text() == "Quit anyway"
        assert box.defaultButton() is buttons["wait"]
        assert box.escapeButton() is buttons["wait"]
        text = box.text()
        assert REASON in text
        assert "half-done" in text and "ends Yu'lon at once" in text
    finally:
        box.deleteLater()


def test_ask_answers_true_only_for_quit_anyway(qapp: Any, monkeypatch: Any) -> None:
    for pressed, expected in (("quit", True), ("wait", False)):
        monkeypatch.setattr(
            quit_anyway, "_run", lambda _box, buttons, which=pressed: buttons[which]
        )
        assert quit_anyway.ask(None, REASON) is expected
