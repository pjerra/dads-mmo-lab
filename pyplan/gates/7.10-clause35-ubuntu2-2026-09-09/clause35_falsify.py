"""T3: the corrected 7.10 networking clause, driven until it FAILS and then PASSES.

The defect this exists for. `widget_driver.py`'s networking clause read
`acore_auth.realmlist.address` out of the database and asserted that string
appeared in the plan text the Networking tab shows. But `network_plan('lan')`
PROPOSES this box's LAN address (`172.30.48.189`) and the row HOLDS whatever was
last applied. On 2026-09-04, 09-05 and 09-08 the row happened to hold the LAN
address, so for three runs the clause compared the plan to itself and could not
have failed. On 2026-09-09 the row held `100.99.204.5` — put there so the
owner's client reaches this VM through the Hyper-V host — and the clause failed
while saying nothing whatever about the widget.

A clause that cannot fail is worth nothing, so a clause corrected on paper is
worth nothing either until it has been WATCHED failing. This driver watches both
halves fail, on the live server, through the real widgets:

  PART A — the PROPOSAL half, now `plan.lan_ip in plan_text`.
      A1  the real plan against the text the widget rendered for it   -> expect OK
      A2  the SAME predicate against a plan object whose `lan_ip` differs from
          the text on screen — a tab showing one address while its plan holds
          another                                                    -> expect FAIL
      A3  that same mutant handed to the widget's own `_plan_ready` slot, so the
          widget re-renders it and object and text agree again       -> expect OK
      The mutation is in the INPUT — a `dataclasses.replace` of a frozen
      `NetworkPlan` — never in the clause. The clause is one expression, written
      once at the top, and all three evaluations call it.

  PART B — the APPLY half: the row the server advertises becomes the plan's
      address, and only after Apply is pressed.
      B1  the row is read (the GROUND) and deliberately set to a third address,
          192.168.77.77, which is neither the plan's nor the one this box
          advertises, and read back by `docker exec mysql`
      B2  the ground guard: the clause REFUSES to run if the row already equals
          the plan, because a clause true before its action is this whole
          round's defect
      B3  the apply clause evaluated with plan and row disagreeing -> expect FAIL
      B4  the real `Apply` QPushButton pressed with `QTest.mouseClick`
      B5  the same clause re-evaluated on the row read again        -> expect OK
      B6  the row put back to 100.99.204.5 on both address columns, mask
          255.255.255.0, and read back

Every reading of the row is taken by `docker exec ac-database mysql`, a route
that is not the widget. The password is passed in `MYSQL_PWD` so it is on no
command line this log records.

Apply really does run: it adds two `ufw allow` rules (it does NOT run `ufw
enable` — that refusal is the plan's own, see the warning it renders) and runs
the realmlist UPDATE. `run-t3.sh` copies `/etc/ufw/user.rules` aside before this
driver and puts it back afterwards, ownership and mode restated, exactly as
`7.10-rerun-ubuntu2-2026-09-09/run-710-rerun.sh` does around `sweep_driver4.py`.
"""

from __future__ import annotations

import dataclasses
import os
import subprocess
import sys
import time
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
sys.path.insert(0, "/home/pk/lane710b/checkout/pylauncher")

from PySide6.QtCore import Qt  # noqa: E402
from PySide6.QtTest import QTest  # noqa: E402
from PySide6.QtWidgets import QApplication  # noqa: E402

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices, ControllerView  # noqa: E402

SERVER_DIR = Path("/home/pk/wowserver")
SHOTS = Path(os.environ.get("T3_SHOTS", "/home/pk/lane710b/out-t3/shots"))

# A third address: not the plan's LAN address, not the one this box advertises.
# 192.168.77.77 is routable-shaped and belongs to no interface here, so nothing
# can reach it by accident during the seconds the row holds it.
DISAGREEING = "192.168.77.77"
# What the row must hold when this driver is done — the address the 8.7a lane
# wrote so the owner's client reaches this VM through the Hyper-V host.
RESTORE_TO = "100.99.204.5"
RESTORE_MASK = "255.255.255.0"

PASSES = 0
FAILS = 0
UNEXPECTED: list[str] = []


def say(text: str = "") -> None:
    print(text, flush=True)


def check(label: str, ok: bool, expect: bool, detail: str = "") -> None:
    """Record a clause AND whether it did what this driver came to watch it do.

    `expect` is not a way of passing a failure. A FAIL that was expected is still
    a FAIL and is printed as one; what `expect` adds is that a run in which the
    clause did NOT fail where it was supposed to is itself an error, listed under
    UNEXPECTED. Otherwise "I proved it can fail" would be a claim rather than a
    reading.
    """
    global PASSES, FAILS
    if ok:
        PASSES += 1
        say(f"[OK]   {label}" + (f" -- {detail}" if detail else ""))
    else:
        FAILS += 1
        say(f"[FAIL] {label}" + (f" -- {detail}" if detail else ""))
    if ok is not expect:
        UNEXPECTED.append(f"{label}: expected {'OK' if expect else 'FAIL'}, got "
                          f"{'OK' if ok else 'FAIL'}")
        say(f"       !! UNEXPECTED: this evaluation was supposed to "
            f"{'PASS' if expect else 'FAIL'}")


def mysql(statement: str) -> str:
    """Read/write the DB by a route that is NOT the widget under test."""
    done = subprocess.run(
        ["docker", "exec", "-e", "MYSQL_PWD", "ac-database",
         "mysql", "-uroot", "-N", "-B", "-e", statement],
        capture_output=True, text=True, env={**os.environ, "MYSQL_PWD": "password"},
    )
    if done.returncode != 0:
        say(f"       (mysql rc={done.returncode}: {done.stderr.strip()})")
    return done.stdout.strip()


def realm_row() -> tuple[str, str, str]:
    row = mysql("SELECT address, localAddress, localSubnetMask "
                "FROM acore_auth.realmlist WHERE id=1;")
    parts = row.split("\t")
    return tuple(parts) if len(parts) == 3 else (row, "?", "?")  # type: ignore[return-value]


def pump(predicate, timeout_s: float = 90.0, label: str = "") -> bool:
    app = QApplication.instance()
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        app.processEvents()
        if predicate():
            app.processEvents()
            return True
        time.sleep(0.02)
    say(f"       (timed out after {timeout_s}s waiting for {label})")
    return False


def shot(widget, name: str) -> None:
    SHOTS.mkdir(parents=True, exist_ok=True)
    QApplication.instance().processEvents()
    path = SHOTS / f"{name}.png"
    widget.grab().save(str(path))
    up = subprocess.run(["docker", "ps", "--format", "{{.Names}}"],
                        capture_output=True, text=True).stdout.split()
    alive = [n for n in ("ac-database", "ac-authserver", "ac-worldserver") if n in up]
    say(f"       [shot] {time.strftime('%FT%T%z')}  {path.name}  "
        f"containers alive at capture: {alive if alive else 'NONE'}")


def show_tab(view, title: str) -> bool:
    tabs = view._tabs  # noqa: SLF001 -- a gate driving the surface a user sees
    for index in range(tabs.count()):
        if tabs.tabText(index) == title:
            tabs.setCurrentIndex(index)
            QApplication.instance().processEvents()
            return True
    return False


def press_show_plan(view) -> str:
    QTest.mouseClick(view.plan_button, Qt.MouseButton.LeftButton)
    pump(lambda: view.network_text.toPlainText().strip() != ""
         and "working out the plan" not in view.network_text.toPlainText(), 120, "network plan")
    return view.network_text.toPlainText()


# --- THE TWO CLAUSES, written once ------------------------------------------
# Every evaluation below calls one of these. A driver that re-typed the
# comparison at each site could quietly evaluate a different predicate at the
# one it wanted to fail.

def proposal_clause(plan, plan_text: str) -> bool:
    """The text a user reads names the LAN address the widget itself computed."""
    return plan is not None and bool(plan.lan_ip) and plan.lan_ip in plan_text


def apply_clause(row_address: str, plan) -> bool:
    """The realm row the server advertises holds the plan's LAN address."""
    return plan is not None and bool(plan.lan_ip) and row_address == plan.lan_ip


def main() -> int:
    app = QApplication([])
    entry = load_catalog().get("wow-wotlk")
    services = ControllerServices.for_entry(entry, SERVER_DIR)
    view = ControllerView(entry, services, status_poll_ms=0)
    view.resize(900, 700)
    view.show()
    app.processEvents()
    show_tab(view, "Networking")

    say("=" * 78)
    say("PART A -- the PROPOSAL half: the plan text names the address the widget computed")
    say("=" * 78)
    plan_text = press_show_plan(view)
    plan = view._plan  # noqa: SLF001 -- the plan object the widget itself is showing
    say("       plan, as the widget renders it:")
    for line in plan_text.splitlines():
        say(f"         {line}")
    say(f"       plan.lan_ip, the widget's own computed address: {plan.lan_ip!r}")
    address, local_address, mask = realm_row()
    say(f"       the realm row, read by docker exec mysql: address={address!r} "
        f"localAddress={local_address!r} mask={mask!r}")
    say(f"       plan and row {'AGREE' if address == plan.lan_ip else 'DISAGREE'} "
        f"right now -- the old clause's answer was decided by this line; "
        f"the corrected one's is not")
    shot(view, "A-networking-tab-real-plan")

    check("A1  the plan the widget shows names the LAN address the widget itself computed",
          proposal_clause(plan, plan_text), True,
          f"plan.lan_ip={plan.lan_ip!r} appears in the text on screen")

    # The mutation is of the INPUT, not of the clause: a frozen NetworkPlan with
    # one field replaced. This models a real defect -- a Networking tab left
    # showing the plan for one address while the object behind it holds another.
    mutant = dataclasses.replace(plan, lan_ip=DISAGREEING)
    check("A2  the same clause, against a plan whose lan_ip is NOT what the tab shows",
          proposal_clause(mutant, plan_text), False,
          f"mutant.lan_ip={mutant.lan_ip!r} does not appear in the text the widget "
          f"rendered for {plan.lan_ip!r} -- the clause SAYS SO, which is what the "
          f"row-comparison could not do")

    # Hand the mutant to the widget's own slot so the widget really re-renders it.
    view._plan_ready(mutant)  # noqa: SLF001 -- the real slot _run() would call
    app.processEvents()
    mutant_text = view.network_text.toPlainText()
    say("       the tab, re-rendered by the widget's own _plan_ready for the mutant:")
    for line in mutant_text.splitlines()[:3]:
        say(f"         {line}")
    shot(view, "A-networking-tab-mutant-plan")
    check("A3  and passes again once the widget renders the plan it was given",
          proposal_clause(mutant, mutant_text), True,
          f"the clause follows the WIDGET ({mutant.lan_ip!r} now on screen), not the row "
          f"({address!r}, unchanged throughout A)")

    # Put the real plan back on screen through the real button before Part B.
    plan_text = press_show_plan(view)
    plan = view._plan  # noqa: SLF001
    say(f"       Show plan pressed again; the tab holds the real plan for {plan.lan_ip!r}")

    say("")
    say("=" * 78)
    say("PART B -- the APPLY half: the row becomes the plan's address, and only after Apply")
    say("=" * 78)
    ground_address, ground_local, ground_mask = realm_row()
    say(f"       B1  the GROUND, read before anything: address={ground_address!r} "
        f"localAddress={ground_local!r} mask={ground_mask!r}")
    say(f"       B1  setting the row to {DISAGREEING} so plan and row disagree deliberately")
    mysql(f"UPDATE acore_auth.realmlist SET address='{DISAGREEING}', "
          f"localAddress='{DISAGREEING}' WHERE id=1;")
    before_address, before_local, before_mask = realm_row()
    say(f"       B1  read back: address={before_address!r} localAddress={before_local!r} "
        f"mask={before_mask!r}")

    check("B2  the ground disagrees with the plan, so the clause below has something to do",
          before_address != plan.lan_ip, True,
          f"row {before_address!r} != plan.lan_ip {plan.lan_ip!r}; had they been equal this "
          f"driver would have REFUSED, because a clause true before its action proves nothing")
    if before_address == plan.lan_ip:
        say("       REFUSING to press Apply: the row already holds the plan's address.")
        return 2

    check("B3  the row the server advertises holds the plan's LAN address",
          apply_clause(before_address, plan), False,
          f"row={before_address!r} plan.lan_ip={plan.lan_ip!r} -- BEFORE Apply, and the "
          f"clause fails, which is the whole point of correcting it")
    shot(view, "B-networking-tab-before-apply")

    say(f"       B4  pressing the real Apply QPushButton (enabled={view.apply_button.isEnabled()})")
    lines_before = len(view.network_text.toPlainText().splitlines())
    QTest.mouseClick(view.apply_button, Qt.MouseButton.LeftButton)
    pump(lambda: len(view.network_text.toPlainText().splitlines()) > lines_before
         and view.apply_button.isEnabled(), 180, "the apply report")
    report_text = view.network_text.toPlainText()
    say("       B4  what the tab appended after Apply:")
    for line in report_text.splitlines()[lines_before:]:
        say(f"         {line}")
    shot(view, "B-networking-tab-after-apply")

    after_address, after_local, after_mask = realm_row()
    say(f"       B5  the row, read again by docker exec mysql: address={after_address!r} "
        f"localAddress={after_local!r} mask={after_mask!r}")
    check("B5  the row the server advertises holds the plan's LAN address",
          apply_clause(after_address, plan), True,
          f"row {before_address!r} -> {after_address!r}, plan.lan_ip {plan.lan_ip!r} -- "
          f"the SAME clause, the same expression, and the only thing that changed is "
          f"that the button was pressed")

    say(f"       B6  putting the row back to {RESTORE_TO} on both address columns, "
        f"mask {RESTORE_MASK}")
    mysql(f"UPDATE acore_auth.realmlist SET address='{RESTORE_TO}', "
          f"localAddress='{RESTORE_TO}', localSubnetMask='{RESTORE_MASK}' WHERE id=1;")
    final_address, final_local, final_mask = realm_row()
    say(f"       B6  read back: address={final_address!r} localAddress={final_local!r} "
        f"mask={final_mask!r}")
    check("B6  the row is back to the address this box advertises, on both columns",
          final_address == RESTORE_TO and final_local == RESTORE_TO
          and final_mask == RESTORE_MASK, True,
          f"{final_address}/{final_local}/{final_mask} "
          f"(ac-authserver is restarted by run-t3.sh so it re-reads this)")

    view.shutdown()
    app.processEvents()

    say("")
    say("=" * 78)
    say(f"RESULT: {PASSES} OK, {FAILS} FAIL "
        f"-- of which 2 FAILs were the point (A2 and B3)")
    if UNEXPECTED:
        say("EVALUATIONS THAT DID NOT DO WHAT THIS DRIVER CAME TO WATCH:")
        for item in UNEXPECTED:
            say(f"  - {item}")
    else:
        say("every evaluation went the way the docstring says it must: "
            "A1 OK, A2 FAIL, A3 OK, B2 OK, B3 FAIL, B5 OK, B6 OK")
    say("=" * 78)
    return 1 if UNEXPECTED else 0


if __name__ == "__main__":
    raise SystemExit(main())
