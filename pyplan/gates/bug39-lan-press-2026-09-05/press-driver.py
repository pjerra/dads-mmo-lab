"""bug-checklist §39: press the app's own LAN step, through its real widgets, on a remote box.

Every round of §39 from 7 on was listings, probes and stand-ins, by the standing rule
that bars applying a firewall change on a box reached over ssh. This driver applies it,
on `yulon-ubuntu`, behind two armed failsafe timers and a proven console fallback.

What makes the press load-bearing rather than decorative:

  * `status_poll_ms=0`. `ControllerView` re-reads status on a 5 s QTimer; with it off
    the only thing that can fill a label is a click.
  * `Show plan` and `Apply` are pressed with `QTest.mouseClick`, which delivers a real
    press/release to the real QPushButton. Nothing calls `networking.apply()` directly.
  * `Apply` is asserted DISABLED before a plan exists (the 7.1 lockout's button) and its
    enabling is asserted to be the plan's arrival, not the click's.
  * Every consequence is re-read by a route that is not the widget: `sudo ufw status
    numbered`, `sudo ss -lntp`, `docker exec ac-database mysql`.
"""

from __future__ import annotations

import os
import subprocess
import sys
import time
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
sys.path.insert(0, "/home/pk/p7/checkout/pylauncher")

from PySide6.QtCore import Qt, QTimer  # noqa: E402
from PySide6.QtTest import QTest  # noqa: E402
from PySide6.QtWidgets import QApplication, QMessageBox  # noqa: E402

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices, ControllerView  # noqa: E402

SERVER_DIR = Path("/home/pk/wowserver")

PASSES = 0
FAILS = 0
MODALS: list[tuple[str, str]] = []


def say(text: str = "") -> None:
    print(text, flush=True)


def check(label: str, ok: bool, detail: str = "") -> None:
    global PASSES, FAILS
    if ok:
        PASSES += 1
        say(f"[OK]   {label}" + (f" -- {detail}" if detail else ""))
    else:
        FAILS += 1
        say(f"[FAIL] {label}" + (f" -- {detail}" if detail else ""))


def shell(argv: list[str], label: str = "") -> str:
    """Read the machine by a route that is NOT the widget under test."""
    done = subprocess.run(argv, capture_output=True, text=True)
    say(f"$ {' '.join(argv)}")
    say(f"  exit {done.returncode}")
    for line in (done.stdout or "").rstrip().splitlines():
        say(f"  {line}")
    for line in (done.stderr or "").rstrip().splitlines():
        if "Using a password on the command line" in line:
            continue
        say(f"  [stderr] {line}")
    say()
    return (done.stdout or "").strip()


def ufw_status() -> str:
    return shell(["sudo", "-n", "ufw", "status", "numbered"])


def ufw_rules() -> str:
    """`ufw status` on an INACTIVE firewall prints one line and no rules at all.

    So the rule list has to be read where ufw keeps it. This is the only readback
    that can show whether `ufw allow 3724/tcp` arrived on a box whose ufw is off.
    """
    done = subprocess.run(["sudo", "-n", "cat", "/etc/ufw/user.rules"],
                          capture_output=True, text=True)
    body = done.stdout
    keep = [ln for ln in body.splitlines() if "3724" in ln or "8085" in ln or "22" == ln.strip()]
    say("$ sudo -n cat /etc/ufw/user.rules   (the game-port lines)")
    say(f"  exit {done.returncode}, {len(body)} bytes, {len(body.splitlines())} lines")
    for line in keep:
        say(f"  {line}")
    if not keep:
        say("  (no line mentions 3724 or 8085)")
    say()
    return "\n".join(keep)


def realm_row() -> str:
    return shell(
        ["docker", "exec", "ac-database", "mysql", "-uroot", "-ppassword", "-N", "-B", "-e",
         "SELECT id,name,address,localAddress,localSubnetMask,port FROM acore_auth.realmlist;"]
    )


def sshd_listener() -> str:
    done = subprocess.run(["sudo", "-n", "ss", "-lntp"], capture_output=True, text=True)
    lines = [ln for ln in done.stdout.splitlines() if ":22 " in ln or ln.rstrip().endswith(":22")]
    say("$ sudo -n ss -lntp | grep :22")
    for line in lines:
        say(f"  {line}")
    say()
    return "\n".join(lines)


def new_ssh_banner() -> str:
    """A NEW inbound TCP connection to sshd, to 172.30.55.119:22, and its banner.

    The 7.1 lockout left the running session alive (conntrack keeps an established
    flow) and killed the next connection — "Connection timed out during banner
    exchange". So the question is not whether this session still answers; it is
    whether a fresh connection to the LAN address gets sshd's banner. No key and
    no login involved: the banner is the handshake the lockout ate.
    """
    import socket

    say("$ connect() to 172.30.55.119:22 and read sshd's banner (a NEW flow, not this one)")
    try:
        with socket.create_connection(("172.30.55.119", 22), timeout=10) as sock:
            sock.settimeout(10)
            banner = sock.recv(256).decode("ascii", "replace").strip()
    except OSError as exc:
        say(f"  FAILED: {exc!r}")
        say()
        return f"FAILED {exc!r}"
    say(f"  banner: {banner!r}")
    say()
    return banner


def pump(predicate, timeout_s: float = 120.0, label: str = "") -> bool:
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


def install_modal_watcher(app) -> QTimer:
    timer = QTimer()
    timer.setInterval(150)

    def tick() -> None:
        modal = app.activeModalWidget()
        if modal is None:
            return
        title = modal.windowTitle()
        text = modal.text() if isinstance(modal, QMessageBox) else "<not a QMessageBox>"
        MODALS.append((title, text))
        say(f"       [modal] title={title!r}")
        for line in text.splitlines():
            say(f"       [modal] {line}")
        if isinstance(modal, QMessageBox):
            btn = modal.defaultButton() or (modal.buttons()[0] if modal.buttons() else None)
            if btn is not None:
                say(f"       [modal] clicking {btn.text()!r}")
                QTest.mouseClick(btn, Qt.MouseButton.LeftButton)
                return
        modal.close()

    timer.timeout.connect(tick)
    timer.start()
    return timer


def stamp(label: str) -> None:
    say(f"--- {label}: "
        f"{time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())} UTC / "
        f"{time.strftime('%Y-%m-%dT%H:%M:%S%z')} local")


def main() -> int:
    say("=" * 78)
    say("bug-checklist §39 -- the LAN step, pressed on yulon-ubuntu through real widgets")
    say("=" * 78)
    stamp("start")
    say(f"SSH_CONNECTION in this process: {os.environ.get('SSH_CONNECTION', '<unset>')!r}")
    say(f"euid {os.geteuid()}   python {sys.version.split()[0]}")
    say()

    say("=== BEFORE: the machine, read by routes that are not the widget")
    before_ufw = ufw_status()
    before_rules = ufw_rules()
    before_realm = realm_row()
    before_ssh = sshd_listener()
    before_banner = new_ssh_banner()
    shell(["systemctl", "list-units", "b39-*", "--all", "--no-pager"])

    app = QApplication([])
    install_modal_watcher(app)
    entry = load_catalog().get("wow-wotlk")
    services = ControllerServices.for_entry(entry, SERVER_DIR)
    say(f"catalog entry: {entry.id} / {entry.name}")
    say(f"services: {type(services).__name__} from ControllerServices.for_entry "
        "-- the object main.py builds for a real install")
    say(f"services.network_plan  -> {services.network_plan!r}")
    say(f"services.network_apply -> {services.network_apply!r}")
    say()

    view = ControllerView(entry, services, status_poll_ms=0)  # timer OFF: the click is the cause
    view.resize(900, 700)
    view.show()
    app.processEvents()

    say("=== THE NETWORKING TAB")
    check("Apply is DISABLED before any plan exists -- the button that took 7.1's ssh away",
          not view.apply_button.isEnabled())
    check("the tab's buttons read as a user reads them",
          view.plan_button.text() == "Show plan" and view.apply_button.text() == "Apply",
          f"{view.plan_button.text()!r} / {view.apply_button.text()!r}")
    check("LAN is the default mode and the LAN radio is the one checked",
          view.lan_radio.isChecked() and view.network_mode() == "lan")

    # A real press on the radio, even though it is already checked: the mode the
    # plan is computed for has to be one a click can set, not only a default.
    QTest.mouseClick(view.lan_radio, Qt.MouseButton.LeftButton)
    app.processEvents()
    check("after a real click on the LAN radio the mode is still lan",
          view.lan_radio.isChecked() and not view.internet_radio.isChecked()
          and view.network_mode() == "lan")

    say("\n-- Show plan (QTest.mouseClick on the real QPushButton)")
    stamp("Show plan clicked")
    QTest.mouseClick(view.plan_button, Qt.MouseButton.LeftButton)
    placeholder = view.network_text.toPlainText()
    say(f"       the instant after the click the tab shows: {placeholder!r}")
    check("the click itself put the placeholder up (so the wait below is for real work)",
          "working out the plan" in placeholder, placeholder)
    got = pump(lambda: view.network_text.toPlainText().strip() != ""
               and "working out the plan" not in view.network_text.toPlainText(),
               180, "the plan to replace its placeholder")
    check("the placeholder was replaced by a plan", got)
    plan_text = view.network_text.toPlainText()
    say("       PLAN, as the widget renders it for the user:")
    for line in plan_text.splitlines():
        say(f"         {line}")
    say()
    check("Apply went live once a plan existed", view.apply_button.isEnabled())
    check("the plan names this box's LAN address", "172.30.55.119" in plan_text)
    check("the plan names both game ports", "3724" in plan_text and "8085" in plan_text)
    check("the plan does NOT contain a ufw enable",
          "enable" not in plan_text.lower().split("Warnings:")[0],
          "checked the command block, not the warning prose")

    say("\n-- Apply (QTest.mouseClick on the real QPushButton)")
    stamp("Apply clicked")
    before_click = view.network_text.toPlainText()
    QTest.mouseClick(view.apply_button, Qt.MouseButton.LeftButton)
    check("Apply disabled itself for the duration of the run", not view.apply_button.isEnabled())
    got = pump(lambda: view.network_text.toPlainText() != before_click
               and "Applied:" in view.network_text.toPlainText(), 240, "the apply report")
    check("a report came back and the widget rendered it", got)
    stamp("Apply returned")
    full_text = view.network_text.toPlainText()
    report_text = full_text[len(before_click):]
    say("       REPORT, as the widget renders it for the user:")
    for line in report_text.splitlines():
        say(f"         {line}")
    say()
    check("Apply is clickable again", view.apply_button.isEnabled())
    say(f"       modal dialogs seen during the whole run: {MODALS!r}")
    say()

    say("=== AFTER: the same routes, plus the ssh question the lockout answered 'no' to")
    after_ufw = ufw_status()
    after_rules = ufw_rules()
    after_realm = realm_row()
    after_ssh = sshd_listener()
    after_banner = new_ssh_banner()

    check("ufw is still INACTIVE after the press (the enable was withheld)",
          "inactive" in after_ufw.lower(), after_ufw.splitlines()[0] if after_ufw else "<empty>")
    check("the game ports are now in ufw's rule list",
          "3724" in after_rules and "8085" in after_rules, after_rules.replace("\n", " | "))
    check("they were NOT there before the press",
          "3724" not in before_rules and "8085" not in before_rules,
          before_rules.replace("\n", " | ") or "<no game-port line>")
    check("the sshd listener is unchanged", after_ssh == before_ssh)
    check("a NEW connection to 172.30.55.119:22 still gets sshd's banner after the press",
          after_banner.startswith("SSH-"), after_banner)
    check("the realm row was left advertising this box's LAN address",
          "172.30.55.119" in after_realm, after_realm)
    say()
    say(f"ufw before: {before_ufw!r}")
    say(f"ufw after : {after_ufw!r}")
    say(f"banner before: {before_banner!r}")
    say(f"banner after : {after_banner!r}")
    say(f"realm before: {before_realm!r}")
    say(f"realm after : {after_realm!r}")
    say("This driver's own output reached the laptop over the ssh session that was open "
        "before the press, which is the session the 7.1 lockout also left alive; the "
        "banner probe above is the half that lockout killed.")
    say()
    stamp("end")
    say(f"PASSES {PASSES}   FAILS {FAILS}")
    return 0 if FAILS == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
