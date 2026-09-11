"""The 8.7c live gate: modules on WoW Vanilla (CMaNGOS mangos-classic), m910q.

Usage:  ~/gate81b-venv/bin/python gate87c.py <stage>

    ground      what is true before anything is pressed: the conf, its checksum,
                the containers, and the Motd the RUNNING server reports now
    install     the motd manifest through the Modules tab's own button
    unrestarted the running server still reports the OLD value
    restart     stop, start, ask again -- the definition of done
    remove      the Remove button puts the shipped line back, byte for byte
    sqlmod      the other shape a CMaNGOS module has: inline SQL against `mangos`

**The clause being proved** (checklist 2503, "as 8.7b"): *at least one manifest
installs and its key is reported by the running server after the restart it asks
for.* The manifest that can prove it is `motd`, because it is the only item in
the set whose effect the server states back on its own console. On THIS tree the
command table row is `src/game/Chat/Chat.cpp:797` -- `{ "motd", SEC_PLAYER,
true, &ChatHandler::HandleServerMotdCommand }` -- and the third field is
`allowConsole`, so it answers from the mangosd console with no client and no
logged-in character; the answer is printed at `src/game/Chat/Level0.cpp:280-282`
from `sWorld.GetMotd()`. Both were read in `~/vanilla-75b/src/mangos-classic` on
2026-09-08. They are NOT the TBC citations: that box cites `Chat.cpp:816` in a
different checkout, and the line numbers do not transfer.

`Chat.cpp:785` binds a SECOND `motd` -- `HandleServerSetMotdCommand`, under
`server set` -- which changes the running value without writing the conf. This
gate never uses it: what is being proved is that a value the app wrote into a
FILE survives a restart, and `server set motd` would produce the same console
line while proving nothing.

**Ground before action, everywhere.** `stage_ground` records the Motd the server
is running with before anything is installed, and `stage_install` refuses to run
if that value is already the one it would write. A step whose assertion is
already true before its action proves nothing.

**Every capture says whether the server was alive at that moment.** A screenshot
of a dead server photographs exactly like a refusal, so `alive()` prints
`docker ps` for the three containers beside every grab.

Read-only about the world except where it says otherwise: the only thing this
writes on the box is `~/vanilla-75b/etc/mangosd.conf` (through the app), and
`stage_remove` puts it back and checks the checksum against
`etc/mangosd.conf.before-8.7c`.
"""

from __future__ import annotations

import hashlib
import os
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
SHOTS = HERE
SERVER = Path.home() / "vanilla-75b"
CONF = SERVER / "etc" / "mangosd.conf"
BEFORE = SERVER / "etc" / "mangosd.conf.before-8.7c"
GAME = "wow-vanilla"
WORLD = "vanilla-mangosd"
MOTD_TEXT = "Welcome!"
"""What the Modules tab actually writes, and it is the MANIFEST'S OWN DEFAULT.

Not a string this gate chose, and the first run of `stage_install` found out why:
`ControllerView._module_values()` opens the prompt dialog only when some prompt
has NO default, so `motd` -- whose prompt defaults to "Welcome!" -- is installed
without asking, and the `_prompt_asker` a gate replaces is never called. The
value the user gets from that button is therefore this one. See the README's
findings: the mod whose entire point is a chosen sentence is the one the tab
never asks about. It still proves the clause -- it differs from the line the
server is running, which `stage_ground` records before anything is pressed.
"""
SHIPPED_MOTD = "Welcome to the Continued Massive Network Game Object Server."


def say(message: str) -> None:
    print(f"[{time.strftime('%H:%M:%SZ', time.gmtime())}] {message}", flush=True)


def sh(*args: str) -> str:
    return subprocess.run(args, capture_output=True, text=True, check=False).stdout.strip()


def alive(where: str) -> None:
    """Which of this install's containers are up, at the moment of a capture."""
    rows = sh("docker", "ps", "--format", "{{.Names}} {{.Status}}")
    mine = [r for r in rows.splitlines() if r.startswith("vanilla-")]
    say(f"alive at {where}: {mine if mine else 'NOTHING -- read every capture here as suspect'}")


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


# --------------------------------------------------------------- the app

def entry():
    from yulon.catalog.catalog import load_catalog

    return load_catalog().get(GAME)


def services():
    from yulon.ui.controller_view import ControllerServices

    return ControllerServices.for_entry(entry(), SERVER)


def view_for(made):
    """The real `ControllerView`, with its jobs run inline so a grab is not a race."""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication

    from yulon.ui.controller_view import ControllerView
    from yulon.ui.widgets.job import run_inline

    app = QApplication.instance() or QApplication([])
    view = ControllerView(entry(), made, status_poll_ms=0, job_runner=run_inline)
    view.resize(1000, 700)
    view._gate_app = app
    return view


def open_tab(view, name: str) -> None:
    view._tabs.setCurrentIndex(
        next(i for i in range(view._tabs.count()) if view._tabs.tabText(i) == name)
    )


def console(command: str) -> list[str]:
    from yulon.controller_wow_vanilla import console as vanilla_console

    reply = vanilla_console.send_command(command)
    return list(reply.lines)


def motd_now() -> str:
    """What the RUNNING server says its Motd is, through the shipped console seam."""
    for line in console(".server motd"):
        if "Message of the day" in line:
            return line.split(":", 1)[1].strip()
    raise SystemExit("the console did not answer .server motd; is mangosd up?")


def wait_ready(since: str, budget: int = 1800) -> None:
    """Poll THIS run's log for `Avg Diff:` -- the entry's own ready marker.

    `--since` is the point of the argument. `docker logs` without it replays the
    whole history, so a restart would be declared ready by a line the PREVIOUS
    run printed, which is a probe that answers the same before and after.
    """
    deadline = time.time() + budget
    while time.time() < deadline:
        out = sh("docker", "logs", "--since", since, WORLD)
        if "Avg Diff:" in out:
            say(f"ready: `Avg Diff:` seen in the log written since {since}")
            return
        time.sleep(5)
    raise SystemExit(f"mangosd did not reach `Avg Diff:` within {budget}s")


def manifest(mod_id: str):
    from yulon.controller_wow_vanilla import modules as vanilla_modules

    return vanilla_modules.store().load("mod", mod_id)


def select(view, mod_id: str) -> None:
    for row in range(view.module_list.count()):
        if view.module_list.item(row).data(256) == mod_id:
            view.module_list.setCurrentRow(row)
            return
    raise SystemExit(f"{mod_id} is not in the Modules list; the store is the wrong game's")


# ------------------------------------------------------------------ stages


def stage_ground() -> None:
    """What is true before anything is pressed."""
    alive("ground")
    say(f"conf sha256          {digest(CONF)}")
    say(f"conf.before-8.7c sha {digest(BEFORE)}")
    assert digest(CONF) == digest(BEFORE), "the conf already differs from the copy taken aside"

    made = services()
    say(f"store game           {made.store.game}")
    say(f"applier server_dir   {made.applier.server_dir}")
    say(f"applier sql schemas  {dict(made.applier.sql.schemas)}")
    say(f"applier sql client   {made.applier.sql.client} in {made.applier.sql.db_container}")
    ids = [m.id for m in made.store.load_all("mod")]
    say(f"mods this tab offers {ids}")
    assert made.store.game == GAME
    assert dict(made.applier.sql.schemas)["world"] == "mangos"

    running = motd_now()
    say(f"the RUNNING server reports Motd: {running!r}")
    assert running != MOTD_TEXT, (
        "the value this gate would install is ALREADY the running one; the install step "
        "would prove nothing. Restore etc/mangosd.conf.before-8.7c and restart first."
    )
    say(f"GROUND RECORDED: Motd is {running!r}, and the gate will write {MOTD_TEXT!r}")

    view = view_for(made)
    open_tab(view, "Modules")
    view.grab().save(str(SHOTS / "1-modules-tab.png"))
    alive("1-modules-tab.png")
    say(f"the tab lists {view.module_list.count()} item(s); Install enabled: "
        f"{view.install_module_button.isEnabled()}")


def stage_install() -> None:
    """The manifest installs, through the tab's own button, and asks for a restart."""
    alive("install")
    running = motd_now()
    assert running != MOTD_TEXT, f"already {running!r}: this step would prove nothing"
    say(f"ground for this step: the server is running with {running!r}")

    made = services()
    view = view_for(made)
    open_tab(view, "Modules")
    select(view, "motd")
    say(f"selected: {view.module_list.currentItem().text()}")
    view.install_module_button.click()
    report = view.module_report.toPlainText()
    say("the tab reports:")
    for line in report.splitlines():
        say(f"  | {line}")
    view.grab().save(str(SHOTS / "2-installed.png"))
    alive("2-installed.png")

    assert "FAILED" not in report, report
    # The app's OWN words for the restart it asks for, and they are not the word
    # "restart": `_format_report` renders `restart_recommended` as "Press Stop
    # and then Start on the Server tab to apply this." This assertion looked for
    # "restart" on the first run and failed on a report that had already asked
    # correctly -- a gate-driver defect, recorded rather than quietly fixed.
    assert "Stop and then Start" in report, f"the report did not ask for a restart: {report!r}"
    text = CONF.read_text(encoding="utf-8")
    assert f'Motd = "{MOTD_TEXT}"' in text, "the key did not land in the file"
    say(f"conf sha256 after install {digest(CONF)}")

    diff = subprocess.run(
        ["diff", str(BEFORE), str(CONF)], capture_output=True, text=True, check=False
    ).stdout
    say("diff against the copy taken before the gate:")
    for line in diff.splitlines():
        say(f"  | {line}")
    changed = [line for line in diff.splitlines() if line.startswith(("<", ">"))]
    assert len(changed) == 2, f"expected ONE changed line, got {changed}"
    say("INSTALL PASSED: one line changed, and the report asked for the restart")


def stage_unrestarted() -> None:
    """The running server still reports the OLD value: the restart is load-bearing."""
    alive("unrestarted")
    running = motd_now()
    say(f"without a restart, the server still reports: {running!r}")
    assert f'Motd = "{MOTD_TEXT}"' in CONF.read_text(encoding="utf-8"), "the file was not written"
    if running == MOTD_TEXT:
        say("FINDING: mangosd re-read the conf without a restart; build.restart over-claims")
        raise SystemExit("recorded as a finding, not a pass")
    say(f"UNRESTARTED PASSED: file says {MOTD_TEXT!r}, server still says {running!r}")


def stage_restart() -> None:
    """Stop and start through the app's own controller, then ask again."""
    alive("before the restart")
    before = motd_now()
    assert before != MOTD_TEXT, f"already {before!r} before the restart: nothing to prove"
    made = services()
    say("stopping through the app's controller (VanillaController.stop)")
    made.controller.stop()
    mark = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() - 2))
    say(f"starting again; readiness will be read from the log written since {mark}")
    made.controller.start()
    wait_ready(mark)
    alive("after the restart")

    after = motd_now()
    say(f"after the restart the server reports: {after!r}")
    assert after == MOTD_TEXT, f"expected {MOTD_TEXT!r}, got {after!r}"

    view = view_for(services())
    open_tab(view, "Console")
    view.command_edit.setText("server motd")
    view.send_button.click()
    view.grab().save(str(SHOTS / "3-server-reports-the-new-motd.png"))
    alive("3-server-reports-the-new-motd.png")
    say("the app's own Console tab shows:")
    for line in view.console_log.text().splitlines()[-4:]:
        say(f"  | {line}")
    say(f"DEFINITION OF DONE PASSED: was {before!r}, is {after!r}, after the restart it asked for")


def stage_remove() -> None:
    """Remove puts the shipped line back and the file is byte-identical again."""
    alive("remove")
    made = services()
    view = view_for(made)
    open_tab(view, "Modules")
    select(view, "motd")
    view.remove_module_button.click()
    report = view.module_report.toPlainText()
    say("the tab reports:")
    for line in report.splitlines():
        say(f"  | {line}")
    view.grab().save(str(SHOTS / "4-removed.png"))
    alive("4-removed.png")

    assert "FAILED" not in report, report
    text = CONF.read_text(encoding="utf-8")
    assert f'Motd = "{SHIPPED_MOTD}"' in text
    assert MOTD_TEXT not in text
    say(f"conf sha256 after remove {digest(CONF)}")
    say(f"conf.before-8.7c sha256  {digest(BEFORE)}")
    assert digest(CONF) == digest(BEFORE), "remove did not put the file back byte for byte"
    say("REMOVE PASSED: byte-identical to the file this gate started from")


def stage_sqlmod() -> None:
    """The other shape: inline SQL against `mangos`, with its own undo.

    Proved by a query and NOT by the console -- said here rather than implied,
    because the server states nothing back about item stack sizes. What the
    running server does say is that it still reaches `Avg Diff:` afterwards,
    which is its only word on this one and is worth having.
    """
    alive("sqlmod")
    from yulon.controller_wow_vanilla import modules as vanilla_modules

    made = services()
    reader = made.applier.sql

    def count(where: str) -> int:
        password = (SERVER / ".db_password").read_text(encoding="utf-8").strip()
        out = sh(
            "docker", "exec", "vanilla-db", "mariadb", "-uroot", f"-p{password}",
            "-N", "-B", "mangos", "-e",
            f"SELECT COUNT(*) FROM item_template WHERE {where}",
        )
        return int(out.splitlines()[-1])

    stackable = count("stackable > 1")
    already = count("stackable = 200")
    say(f"before: {stackable} stackable rows, {already} of them already at 200")
    assert stackable > 0, "nothing is stackable; this install's world DB is not loaded"
    assert already != stackable, "every stackable row is ALREADY 200; this proves nothing"

    view = view_for(made)
    open_tab(view, "Modules")
    select(view, "all-stackables")
    view.install_module_button.click()
    report = view.module_report.toPlainText()
    for line in report.splitlines():
        say(f"  | {line}")
    view.grab().save(str(SHOTS / "5-sql-mod-installed.png"))
    alive("5-sql-mod-installed.png")
    assert "FAILED" not in report, report

    after = count("stackable = 200")
    say(f"after install: {after} rows at 200 (was {already}); the schema reached was "
        f"{dict(reader.schemas)['world']!r}")
    assert after == stackable, f"{after} rows at 200, expected {stackable}"

    view2 = view_for(services())
    open_tab(view2, "Modules")
    select(view2, "all-stackables")
    view2.remove_module_button.click()
    say("remove reports:")
    for line in view2.module_report.toPlainText().splitlines():
        say(f"  | {line}")
    view2.grab().save(str(SHOTS / "6-sql-mod-removed.png"))
    alive("6-sql-mod-removed.png")
    back = count("stackable = 200")
    say(f"after remove: {back} rows at 200 (started at {already})")
    assert back == already, f"{back} rows still at 200, expected {already}"
    gone = sh(
        "docker", "exec", "vanilla-db", "mariadb", "-uroot",
        f"-p{(SERVER / '.db_password').read_text(encoding='utf-8').strip()}",
        "-N", "-B", "mangos", "-e", "SHOW TABLES LIKE 'yulon_stackable_backup'",
    )
    say(f"the backup table after remove: {gone!r}")
    assert gone == "", "the undo table is still there"
    say("SQL MOD PASSED: installed, counted, removed, and its own backup table dropped")
    _ = vanilla_modules


STAGES = {
    "ground": stage_ground,
    "install": stage_install,
    "unrestarted": stage_unrestarted,
    "restart": stage_restart,
    "remove": stage_remove,
    "sqlmod": stage_sqlmod,
}

if __name__ == "__main__":
    if len(sys.argv) != 2 or sys.argv[1] not in STAGES:
        raise SystemExit(f"usage: python gate87c.py <{'|'.join(STAGES)}>")
    STAGES[sys.argv[1]]()
