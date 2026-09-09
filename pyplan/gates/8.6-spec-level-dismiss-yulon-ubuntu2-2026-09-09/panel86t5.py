"""T5 live half -- the REAL `PartyPanel`, with T5's three new controls, on the VM's desktop.

Runs ON `yulon-ubuntu2` against `~/wowserver`. The widget is
`yulon.ui.widgets.party_panel.PartyPanel`, unmodified, handed the same
`party.InstallParty` object `ControllerServices.my_party` holds and the same
`ThreadedJobRunner` the view hands it. The seam is assembled by `gate86b.build()`
from the 8.6 folder next door, so this file reimplements no command string and no
connection: it is the earlier gate's own object with T5's surface on it.

The DIFFERENCE from the running app, stated because it is the one thing this
press does not show: the app builds that seam inside `_for_wotlk` from a
registered install in `state.json`, and this driver builds it from the same
inputs by hand. Everything above the seam -- the widget, its slots, its
threading, its three new controls -- is the shipped code.

    ground <master>            read the seam BEFORE any press and print it
    specs <class>              this install's premade specs for one class
    account <name> <password>  make THIS lane's own account (never PERZI's)
    sql <statement>            one read against acore_characters
    send <command>             one command over the same channel the panel uses
    show <master>              open the window on this desktop and keep it open

`show` is a WAITER: it opens the window, writes the screen coordinates of every
control to `~/panel86t5/coords.txt` and a transcript of what the panel SAYS to
`~/panel86t5/panel.log` once a second, and then does nothing else. The presses
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
    str(Path.home() / "dads-mmo-lab" / "pyplan" / "gates" / "8.6-wotlk-yulon-ubuntu2-2026-09-09"),
)

import gate86b  # noqa: E402  (the seam, built exactly as the earlier gate built it)

from yulon import party  # noqa: E402

OUT = Path.home() / "panel86t5"


def ground(master: str) -> int:
    seam, _accounts, sql = gate86b.build()
    print(gate86b.liveness())
    state = seam.state(master)
    print(f"state.ready={state.ready} blocker={state.blocker!r} problem={state.problem!r}")
    print(f"state.members={state.members}")
    print(f"{master} online guid: {seam.online_guid(master)}")
    print(f"seam.max_level() = {seam.max_level()}")
    for klass in party.BOT_CLASSES:
        print(f"seam.specs({klass!r}) = {seam.specs(klass)}")
    chars = gate86b.ENTRY.schema_map()["characters"]
    print(
        "group_member rows on the whole server: "
        + sql.query("characters", f"SELECT COUNT(*) FROM {chars}.group_member;").strip()
    )
    return 0


def specs(klass: str) -> int:
    seam, _accounts, _sql = gate86b.build()
    for name in seam.specs(klass):
        print(name)
    return 0


def account(name: str, password: str) -> int:
    """This lane's own account, through the app's own account creator.

    `wotlk_accounts.create_account` is the same callable `InstallChannel` is
    handed as its `create=` (`gate86b.build`), so this is the app's own route
    to an account and not a hand-written SQL insert. The password is a
    throwaway and is masked everywhere it would otherwise be written down.
    """
    _seam, _accounts, sql = gate86b.build()
    print(gate86b.liveness())
    from yulon.controller_wow_wotlk import accounts as wotlk_accounts

    outcome = wotlk_accounts.create_account(
        sql, name, password, gm_level=0, scheme=gate86b.ENTRY.accounts.scheme or "azerothcore"
    )
    # `AccountResult` deliberately holds no password, which is why it is what
    # gets printed: a failure raises `AccountError` rather than returning one.
    print(f"create -> username={outcome.username!r} created={outcome.created}")
    return 0


def read_sql(statement: str) -> int:
    _seam, _accounts, sql = gate86b.build()
    print(sql.query("characters", statement))
    return 0


def send(command: str) -> int:
    """One command over the same channel the panel's seam uses.

    Used for two things this gate has to do from outside the panel: whisper an
    INVALID spec (the picker cannot offer one, and the module's refusal is the
    measurement the ticket asks for), and add a third bot between the two
    presses of a confirmation so the seam's refusal can be photographed.
    """
    seam, _accounts, _sql = gate86b.build()
    channel = seam._channel_for_saved()  # noqa: SLF001 - the gate is inside the seam's own house
    if channel is None:
        print("no channel")
        return 1
    answer = channel.send(command)
    print(f"outcome={answer.outcome} reason={answer.reason!r}")
    print(f"text={answer.text!r}")
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
    window.setWindowTitle("Yulon - My Party (T5 live: spec, level, dismiss all)")
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
    # The VM's XWayland screen is 1024x768 (measured with `xdotool
    # getdisplaygeometry` on 2026-09-09), and T5's panel is three controls
    # taller than the one the 8.6 folder photographed.
    window.setGeometry(0, 0, 1020, 752)
    window.show()

    coords = OUT / "coords.txt"
    log = OUT / "panel.log"

    def dump() -> None:
        lines = []
        for name, widget in (
            ("character", panel.character),
            ("refresh_button", panel.refresh_button),
            ("klass", panel.klass),
            ("spec", panel.spec),
            ("level", panel.level),
            ("add_button", panel.add_button),
            ("member_list", panel.member_list),
            ("dismiss_button", panel.dismiss_button),
            ("dismiss_all_button", panel.dismiss_all_button),
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
        offered = [panel.spec.itemText(i) for i in range(panel.spec.count())]
        coords.write_text("\n".join(lines) + "\n")
        with log.open("a") as fh:
            fh.write(
                f"[{time.strftime('%H:%M:%S')}] summary={panel.summary.text()!r} "
                f"report={panel.report.text()!r} rows={rows} "
                f"class={panel.klass.currentText()!r} "
                f"spec={panel.spec.currentText()!r} spec_offered={offered} "
                f"level={panel.level.value()} level_text={panel.level.text()!r} "
                f"level_max={panel.level.maximum()} "
                f"dismiss={panel.dismiss_button.text()!r} "
                f"dismiss_all={panel.dismiss_all_button.text()!r}\n"
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
    verb = args[0]
    if verb == "ground":
        return ground(args[1])
    if verb == "specs":
        return specs(args[1])
    if verb == "account":
        return account(args[1], args[2])
    if verb == "sql":
        return read_sql(args[1])
    if verb == "send":
        return send(" ".join(args[1:]))
    if verb == "show":
        return show(args[1])
    print(f"unknown: {verb}")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
