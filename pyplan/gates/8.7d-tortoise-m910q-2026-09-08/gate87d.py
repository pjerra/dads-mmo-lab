"""The 8.7d live gate: modules on WoW Tortoise, and the auto-update guard.

    Usage:  ~/gate81b-venv/bin/python gate87d.py <stage>

    ground      what was true before anything ran: the stack, the store, the
                binding, the updater's own configuration and arming, and the
                value `.perf intervalreport` reports right now
    install     press Install on `perf-report` through the tab, then ask the
                RUNNING server again WITHOUT restarting -- the old value is what
                makes the restart load-bearing rather than decorative
    restart     restart through the app's own controller, wait for this fork's
                ready line, and ask again: the definition of done
    armed       checklist 2504. Arm this fork's auto-updater with one file, show
                the guard refuse the same press that just succeeded, disarm it,
                show the same press succeed again
    remove      Remove puts the shipped values back; restart; the server reports
                600 again and the conf is byte-identical to `ground`'s
    teardown    leave the box the way the next lane needs it

Every stage prints a UTC clock on every line and says whether the worldserver
process is alive at the moment of each capture -- a screenshot of a client or a
server that has DIED photographs exactly like a refusal.

READ-WRITE, and deliberately so: this box's whole point is a module install
against a live server. What is written, and what puts it back:

* `~/tortoise-server/etc/mangosd.conf` -- `Perf.Enable` and
  `Perf.ReportInterval`. `ground` copies the file to `mangosd.conf.before-87d`
  and records its sha256; `remove` restores the shipped values through the
  manifest's own `when: "remove"` patches and the sha is compared back.
* one file inside the RUNNING container at
  `/opt/tortoise/sql/database_updates/world/`, for the `armed` stage, whose
  whole content is `SELECT 1;`. It exists only between two lines of one stage
  and is deleted before anything is restarted. Nothing ever restarts the world
  while it is there -- that is the entire point of the guard being tested.

Nothing here rebuilds anything. The stack runs the PRE-REBUILD image
(`yulon.local/cmangos-tortoise-server:native-58c6fd1c`) and the source checkout
stays where it is.
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "pylauncher"))

from yulon import docker  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.controller_wow_tortoise import autoupdate  # noqa: E402
from yulon.controller_wow_tortoise import modules as tortoise_modules  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "tortoise-server"
SHOTS = Path.home() / "gate87d-shots"
ENTRY = load_catalog().get("wow-tortoise")
SPEC = ENTRY.container_spec()
CONF = SERVER_DIR / "etc" / "mangosd.conf"
BEFORE = SERVER_DIR / "etc" / "mangosd.conf.before-87d"
READY = "World server is up and running"
ARMED_FILE = "zzzz_yulon_87d_probe.sql"


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def alive() -> str:
    """Whether the worldserver process is alive, said out loud beside every capture."""
    state = docker.container_state(SPEC.world)
    return f"{SPEC.world}: status={state.status} restarts={state.restart_count}"


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def services() -> ControllerServices:
    return ControllerServices.for_entry(ENTRY, SERVER_DIR)


def arming() -> autoupdate.Arming:
    return autoupdate.read_arming(
        SERVER_DIR,
        world_container=SPEC.world,
        schemas=ENTRY.schema_map(),
        sql=services().sql,
    )


def console(command: str) -> tuple[bool, tuple[str, ...]]:
    from yulon.controller_wow_tortoise import console as tortoise_console  # noqa: PLC0415

    reply = tortoise_console.send(command)
    return reply.prompted, tuple(reply.lines)


def perf_interval() -> str:
    """What the RUNNING server says its performance report interval is.

    `.perf intervalreport` with no argument prints
    `Performance report interval is <n>` from
    `sWorld.getConfig(CONFIG_UINT32_PERFORMANCE_REPORT_INTERVAL)`
    (`Commands.cpp:19146-19154`); its command row carries `allowConsole = true`
    (`Chat.cpp:837`), so the mangosd console answers with no client attached.
    """
    prompted, lines = console("perf intervalreport")
    say(f"  .perf intervalreport -> prompted={prompted} lines={lines}")
    for line in lines:
        if "Performance report interval is" in line:
            return line.strip()
    return "(no answer inside the window)"


def view_for(made: ControllerServices, tab: str):
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    app = QApplication.instance() or QApplication([])
    view = ControllerView(ENTRY, made, status_poll_ms=0, job_runner=run_inline)
    view.resize(1000, 700)
    view._tabs.setCurrentIndex(
        next(i for i in range(view._tabs.count()) if view._tabs.tabText(i) == tab)
    )
    view._gate_app = app
    SHOTS.mkdir(parents=True, exist_ok=True)
    return view


def select(view, item_id: str) -> None:
    """Select a manifest in the tab's list the way a user's click does."""
    for row in range(view.module_list.count()):
        item = view.module_list.item(row)
        if str(item.data(256)) == item_id:
            view.module_list.setCurrentRow(row)
            return
    raise AssertionError(f"{item_id} is not in the Modules list")


def shot(view, name: str) -> None:
    view.grab().save(str(SHOTS / name))
    say(f"  screenshot {name} taken; {alive()}")


def press(view, action: str) -> str:
    """Press Install/Remove on the tab and return what the report box then says."""
    view._module_action(action)
    return view.module_report.toPlainText()


def sh(argv: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(argv, capture_output=True, text=True, check=False)


# ------------------------------------------------------------------- stages


def stage_ground() -> None:
    say("GROUND -- nothing below has run yet")
    say(alive())
    say(f"compose ps:\n{sh(['docker', 'compose', 'ps']).stdout.strip()}")

    if not BEFORE.exists():
        shutil.copy2(CONF, BEFORE)
    say(f"etc/mangosd.conf sha256 = {sha(CONF)}; copy kept at {BEFORE.name}")
    for key in ("Perf.Enable", "Perf.ReportInterval"):
        line = next(
            (li for li in CONF.read_text().splitlines() if li.strip().startswith(key)), "(absent)"
        )
        say(f"  conf now says: {line}")

    made = services()
    say(f"store.game = {made.store.game if made.store else None}")
    say(f"applier    = {type(made.applier).__module__}.{type(made.applier).__name__}")
    assert isinstance(made.applier, autoupdate.GuardedApplier), made.applier
    ids = [m.id for m in made.store.load_all("mod")]
    say(f"manifests  = {ids}")
    say(f"sql        = client={made.sql.client} container={made.sql.db_container} "
        f"schemas={dict(made.sql.schemas)}")
    assert made.sql.schemas["world"] == "tw_world"

    settings = autoupdate.read_settings(SERVER_DIR)
    say(f"updater settings: enabled={settings.enabled} declared={settings.declared} "
        f"path={settings.path!r} folders={dict(settings.folders)}")

    arm = arming()
    say(f"updater arming (through the app): {arm.summary()}  armed={arm.armed}")

    # The same subtraction by hand, so the app's answer has something to be
    # equal TO. `sha1sum` inside the container, `SELECT` through mariadb -- no
    # yulon code on either side of this reading.
    for folder, db in (("world", "tw_world"), ("character", "tw_char"), ("auth", "tw_logon")):
        listed = sh(
            ["docker", "exec", SPEC.world, "sh", "-c",
             f'ls -1 /opt/tortoise/sql/database_updates/{folder}/*.sql 2>/dev/null | wc -l']
        ).stdout.strip()
        rows = sh(
            ["docker", "exec", SPEC.db, "sh", "-c",
             f'mariadb -uroot -p"$(cat /run/secrets/x 2>/dev/null || echo)" -N -B '
             f'-e "SELECT COUNT(*) FROM {db}.migrations" 2>/dev/null || echo ?']
        ).stdout.strip()
        say(f"  by hand: {folder}/*.sql = {listed} file(s); {db}.migrations rows = {rows}")

    say(f"the running server, before anything: {perf_interval()}")

    view = view_for(made, "Modules")
    view.reload_modules()
    select(view, "perf-report")
    say(f"the tab lists {view.module_list.count()} item(s); selected = "
        f"{view.module_list.currentItem().text()!r}")
    shot(view, "1-modules-listed.png")
    say("GROUND RECORDED")


def stage_install() -> None:
    say("INSTALL -- the press a user makes, on the object the tab holds")
    say(alive())
    before_sha = sha(CONF)
    say(f"conf sha before = {before_sha}")
    say(f"the running server, before the press: {perf_interval()}")

    made = services()
    view = view_for(made, "Modules")
    view.reload_modules()
    select(view, "perf-report")
    report = press(view, "install")
    say("the report box says:")
    for line in report.splitlines():
        say(f"    {line}")
    assert "FAILED" not in report, report
    assert "auto-update guard" in report, "the guard has to say it looked"
    assert "restart" in report.lower()
    shot(view, "2-installed.png")

    say(f"conf sha after  = {sha(CONF)}")
    diff = sh(["diff", str(BEFORE), str(CONF)])
    say(f"diff against the ground copy:\n{diff.stdout.strip() or '(identical -- WRONG)'}")
    assert diff.returncode == 1, "the install changed nothing"

    say("and now the whole point of the restart:")
    said = perf_interval()
    say(f"the RUNNING server still reports: {said}")
    assert "600" in said, (
        "the running server already reports the new value, so the restart this item asks for "
        "is decorative -- record that, it is a finding about the manifest"
    )
    say("INSTALL PASSED: the file changed, the running server has not")


def _wait_ready(since: float, timeout: float = 900.0) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        out = sh(["docker", "logs", "--since", str(int(since)), SPEC.world]).stdout
        if READY in out:
            say(f"  ready line seen after {int(time.time() - since)}s")
            return
        time.sleep(5)
    raise AssertionError(f"{SPEC.world} did not print {READY!r} within {timeout}s")


def stage_restart() -> None:
    say("RESTART -- through the app's own controller, which is what the tab presses")
    made = services()
    say(f"before: {alive()}")
    started = time.time()
    say("controller.stop() ...")
    say(f"  stop returned {made.controller.stop()}")
    say(f"after stop: {alive()}")
    say("controller.start() ...")
    made.controller.start()
    say(f"after start: {alive()}")
    _wait_ready(started)
    say(f"ready: {alive()}")

    said = perf_interval()
    say(f"THE DEFINITION OF DONE: the running server now reports: {said}")
    assert "120" in said, said

    view = view_for(made, "Console")
    view.command_edit.setText("perf intervalreport")
    view.send_console_command()
    say(f"the Console tab shows:\n{view.console_log.text().strip()[-400:]}")
    shot(view, "3-reported-after-restart.png")
    say("RESTART PASSED: the key this install wrote is reported by the running server")


def _arm(present: bool) -> None:
    if present:
        sh(["docker", "exec", SPEC.world, "sh", "-c",
            f"printf 'SELECT 1;\\n' > /opt/tortoise/sql/database_updates/world/{ARMED_FILE}"])
    else:
        sh(["docker", "exec", SPEC.world, "rm", "-f",
            f"/opt/tortoise/sql/database_updates/world/{ARMED_FILE}"])
    listed = sh(["docker", "exec", SPEC.world, "sh", "-c",
                 f"ls /opt/tortoise/sql/database_updates/world/{ARMED_FILE} 2>&1"]).stdout.strip()
    say(f"  the probe file is now: {listed}")


def stage_armed() -> None:
    """Checklist 2504, with its ground read first.

    A refusal that would have fired anyway proves nothing, so the SAME press on
    the SAME item is made three times: permitted, refused, permitted. The middle
    one is the only thing that changes between them.
    """
    say("ARMED -- checklist 2504: nothing may run this fork's auto-update path")
    say(alive())
    made = services()
    view = view_for(made, "Modules")
    view.reload_modules()
    select(view, "perf-report")

    say("GROUND: the updater as it stands")
    ground = arming()
    say(f"  {ground.summary()}  armed={ground.armed}")
    assert ground.armed is False, "already armed; this stage's ground is not what it claims"
    settled = sha(CONF)
    say(f"  conf sha = {settled}")
    ok = press(view, "install")
    say(f"  the same press, permitted: {ok.splitlines()[0] if ok else ok!r}")
    assert "FAILED" not in ok, ok

    say("ACTION: put ONE file the ledger has no hash for into the updater's world folder")
    _arm(True)
    armed = arming()
    say(f"  the app now reads: {armed.summary()}  armed={armed.armed}")
    assert armed.armed is True
    assert ARMED_FILE in armed.names(), armed.names()

    say("ASSERTION: the identical press is now refused")
    refused = press(view, "install")
    for line in refused.splitlines():
        say(f"    {line}")
    assert "FAILED" in refused, refused
    assert ARMED_FILE in refused, "the refusal must name what would run"
    assert "Database.AutoUpdate.Enabled" in refused, "and the switch that stops it"
    assert sha(CONF) == settled, "a refused install wrote to the conf anyway"
    say(f"  conf sha unchanged = {sha(CONF)}")
    say(f"  {alive()}  <-- the world is UP and stays up; nothing restarted it")
    shot(view, "4-refused-while-armed.png")

    say("DISARM: the probe file is removed BEFORE anything can restart into it")
    _arm(False)
    back = arming()
    say(f"  the app reads: {back.summary()}  armed={back.armed}")
    assert back.armed is False
    again = press(view, "install")
    say(f"  the same press, permitted again: {again.splitlines()[0] if again else again!r}")
    assert "FAILED" not in again, again
    shot(view, "5-permitted-again.png")
    say("ARMED PASSED: permitted, refused, permitted -- one file was the only difference")


def stage_remove() -> None:
    say("REMOVE -- the manifest's own `when: remove` patches, then a restart")
    made = services()
    view = view_for(made, "Modules")
    view.reload_modules()
    select(view, "perf-report")
    report = press(view, "remove")
    for line in report.splitlines():
        say(f"    {line}")
    assert "FAILED" not in report, report
    shot(view, "6-removed.png")

    say(f"conf sha after remove = {sha(CONF)}")
    say(f"ground copy sha       = {sha(BEFORE)}")
    diff = sh(["diff", str(BEFORE), str(CONF)])
    say(f"diff:\n{diff.stdout.strip() or '(byte-identical)'}")

    started = time.time()
    made.controller.stop()
    made.controller.start()
    _wait_ready(started)
    say(f"after the restart: {alive()}")
    said = perf_interval()
    say(f"the running server reports: {said}")
    assert "600" in said, said
    say("REMOVE PASSED: the shipped value is back and the server says so")


def stage_teardown() -> None:
    say("TEARDOWN")
    say(alive())
    say(f"conf sha = {sha(CONF)} (ground was {sha(BEFORE)})")
    listed = sh(["docker", "exec", SPEC.world, "sh", "-c",
                 f"ls /opt/tortoise/sql/database_updates/world/{ARMED_FILE} 2>&1"]).stdout.strip()
    say(f"the probe file: {listed}")
    say(f"compose ps:\n{sh(['docker', 'compose', 'ps']).stdout.strip()}")
    arm = arming()
    say(f"updater arming: {arm.summary()}  armed={arm.armed}")
    say(json.dumps({"conf_sha": sha(CONF), "ground_sha": sha(BEFORE)}, indent=1))


STAGES = {
    "ground": stage_ground,
    "install": stage_install,
    "restart": stage_restart,
    "armed": stage_armed,
    "remove": stage_remove,
    "teardown": stage_teardown,
}

if __name__ == "__main__":
    os.chdir(SERVER_DIR)
    name = sys.argv[1] if len(sys.argv) > 1 else ""
    if name not in STAGES:
        raise SystemExit(f"usage: gate87d.py <{'|'.join(STAGES)}>")
    STAGES[name]()
