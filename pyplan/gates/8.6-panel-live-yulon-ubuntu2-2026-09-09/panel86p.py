"""8.6 panel-live -- the REAL `PartyPanel` on the VM's own desktop, against the live world.

Runs ON `yulon-ubuntu2`, against `~/wowserver`. The widget is
`yulon.ui.widgets.party_panel.PartyPanel`, unmodified, handed the same
`party.InstallParty` object `ControllerServices.my_party` holds and the same
`ThreadedJobRunner` the view hands it -- the two lines of
`controller_view._build_my_party_group` that matter (a `QGroupBox` titled
"My Party" with the panel inside it, and `jobs=self._run`).

The seam is assembled by `gate86b.build()` from the 8.6 folder next door, so
this file reimplements no command string and no connection: it is the earlier
gate's own object with a surface on it. The DIFFERENCE from the running app,
stated because it is the one thing this press does not show: the app builds
that seam inside `_for_wotlk` from a registered install in `state.json`, and
this driver builds it from the same inputs by hand. Everything above the seam
-- the widget, its slots, its threading -- is the shipped code.

    ground <master>   read the seam BEFORE any press and print it
    show <master>     open the window on this desktop and keep it open

`show` is a WAITER: it opens the window, writes the screen coordinates of every
control to `~/panel86p/coords.txt` and a transcript of what the panel SAYS to
`~/panel86p/panel.log` once a second, and then does nothing else. The presses
come from outside, as real clicks and real keystrokes through `xdotool` on this
desktop -- a `QPushButton.click()` from inside the process would be this file
pressing itself.
"""

from __future__ import annotations

import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path.home() / "dads-mmo-lab" / "pylauncher"))
sys.path.insert(
    0,
    str(
        Path.home()
        / "dads-mmo-lab"
        / "pyplan"
        / "gates"
        / "8.6-wotlk-yulon-ubuntu2-2026-09-09"
    ),
)

import gate86b  # noqa: E402  (the seam, built exactly as the earlier gate built it)

OUT = Path.home() / "panel86p"


def liveness() -> str:
    return gate86b.liveness()


def ground(master: str) -> int:
    seam, _accounts, sql = gate86b.build()
    print(liveness())
    facts = seam.facts()
    print(f"facts: {facts}")
    state = seam.state(master)
    print(f"state.ready={state.ready} blocker={state.blocker!r} problem={state.problem!r}")
    print(f"state.members={state.members}")
    for check in state.checks:
        print(f"    {'ok ' if check.met else 'NO '}{check.name}")
    print(f"{master} online guid: {seam.online_guid(master)}")
    chars = gate86b.ENTRY.schema_map()["characters"]
    print(
        "group_member rows on the whole server: "
        + sql.query("characters", f"SELECT COUNT(*) FROM {chars}.group_member;").strip()
    )
    return 0


def show(master: str) -> int:
    from PySide6.QtCore import QTimer
    from PySide6.QtWidgets import QApplication, QGroupBox, QMainWindow, QVBoxLayout, QWidget

    from yulon.ui.widgets.job import threaded_job_runner
    from yulon.ui.widgets.party_panel import PartyPanel

    OUT.mkdir(exist_ok=True)
    seam, _accounts, _sql = gate86b.build()

    app = QApplication(sys.argv[:1])
    window = QMainWindow()
    window.setWindowTitle("Yulon - My Party (8.6 live panel press)")
    holder = QWidget(window)
    outer = QVBoxLayout(holder)
    # `_build_my_party_group`'s own two lines: the group box, and the panel in it
    # wired to this window's job runner.
    group = QGroupBox("My Party", holder)
    inside = QVBoxLayout(group)
    panel = PartyPanel(seam, jobs=threaded_job_runner(window), parent=group)
    inside.addWidget(panel)
    outer.addWidget(group)
    window.setCentralWidget(holder)
    window.setGeometry(0, 0, 1024, 730)
    window.show()

    coords = OUT / "coords.txt"
    log = OUT / "panel.log"

    def dump() -> None:
        lines = []
        for name, widget in (
            ("character", panel.character),
            ("refresh_button", panel.refresh_button),
            ("klass", panel.klass),
            ("add_button", panel.add_button),
            ("member_list", panel.member_list),
            ("dismiss_button", panel.dismiss_button),
            ("check_list", panel.check_list),
        ):
            top = widget.mapToGlobal(widget.rect().topLeft())
            centre = widget.mapToGlobal(widget.rect().center())
            lines.append(
                f"{name} centre={centre.x()},{centre.y()} "
                f"top={top.x()},{top.y()} size={widget.width()}x{widget.height()}"
            )
        rows = [panel.member_list.item(i).text() for i in range(panel.member_list.count())]
        for i in range(panel.member_list.count()):
            item = panel.member_list.item(i)
            rect = panel.member_list.visualItemRect(item)
            pt = panel.member_list.viewport().mapToGlobal(rect.center())
            lines.append(f"row{i} centre={pt.x()},{pt.y()} text={item.text()}")
        coords.write_text("\n".join(lines) + "\n")
        with log.open("a") as fh:
            fh.write(
                f"[{time.strftime('%H:%M:%S')}] summary={panel.summary.text()!r} "
                f"report={panel.report.text()!r} rows={rows} "
                f"dismiss={panel.dismiss_button.text()!r} "
                f"checks={[panel.check_list.item(i).text() for i in range(panel.check_list.count())]}\n"
            )

    timer = QTimer(window)
    timer.timeout.connect(dump)
    timer.start(1000)
    with log.open("a") as fh:
        fh.write(f"[{time.strftime('%H:%M:%S')}] window up for master {master}\n")
    return app.exec()


def main() -> int:
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    if args[0] == "ground":
        return ground(args[1])
    if args[0] == "show":
        return show(args[1])
    print(f"unknown: {args[0]}")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
