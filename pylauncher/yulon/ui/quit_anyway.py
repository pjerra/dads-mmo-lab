"""The way out of a refused quit: "Quit anyway" (T690).

A quit that is refused while work runs must not be a trap. A flag that a bug
elsewhere left set would make Yu'lon impossible to close, and a job that has hung
would hold the window for ever. So every refusal of a quit or a close offers this
second button, and says plainly what it costs.

Quit anyway ends Yu'lon at once through the forced-exit path (`main`'s
`_stop_for_forced_quit()` and `_leave_with_jobs_still_running()`): it does not wait
for the jobs it is cutting off, because waiting up to `STOP_GRACE_SECONDS + 30` seconds
per tab with the window already gone is what the measured defect looked like. What the
work was writing is left half-done; the sentence says so before the click.
"""

from __future__ import annotations

from PySide6.QtWidgets import QAbstractButton, QMessageBox, QWidget

from yulon.ui.message_box import FittedMessageBox

TITLE = "Yu'lon is still working"

COST = (
    "Quit anyway ends Yu'lon at once and cuts that work off, so whatever it was writing is "
    "left half-done. Only do this if it looks stuck."
)


def build_box(
    reason: str, parent: QWidget | None = None
) -> tuple[QMessageBox, dict[str, QAbstractButton]]:
    """The refusal with its two answers: "wait" (the default and the escape) and "quit"."""
    box = FittedMessageBox(
        QMessageBox.Icon.Warning, TITLE, "", QMessageBox.StandardButton.NoButton, parent
    )
    box.setText(f"{reason}\n\n{COST}")
    wait = box.addButton("Keep waiting", QMessageBox.ButtonRole.RejectRole)
    quit_ = box.addButton("Quit anyway", QMessageBox.ButtonRole.DestructiveRole)
    box.setDefaultButton(wait)
    box.setEscapeButton(wait)
    return box, {"wait": wait, "quit": quit_}


def _run(box: QMessageBox, buttons: dict[str, QAbstractButton]) -> QAbstractButton | None:
    """Show the box and answer the button pressed. A seam for the tests."""
    box.exec()
    return box.clickedButton()


def ask(parent: QWidget | None, reason: str) -> bool:
    """True when the player chose "Quit anyway"; anything else keeps the window."""
    box, buttons = build_box(reason, parent)
    try:
        return _run(box, buttons) is buttons["quit"]
    finally:
        box.deleteLater()
