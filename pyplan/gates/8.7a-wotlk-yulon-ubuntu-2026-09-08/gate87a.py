"""8.7a live gate on yulon-ubuntu: modules, WoW WotLK.

Run from ~/gate87a-lane3 with an exported `pylauncher/` beside it. Every step
names the clause it proves and reads its GROUND first -- a step whose assertion
is already true before its action runs proves nothing.

Steps are selected by argv so the long ones (stop/apply/start) can be run
separately and the box is never left half-way by a timeout.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

sys.path.insert(0, str(Path(__file__).resolve().parent / "pylauncher"))

from PySide6.QtWidgets import QApplication  # noqa: E402

from yulon import apply as apply_module  # noqa: E402
from yulon import docker  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.controller_wow_wotlk import console as wotlk_console  # noqa: E402
from yulon.controller_wow_wotlk import docker_ctl  # noqa: E402
from yulon.controller_wow_wotlk import modules as wotlk_modules  # noqa: E402
from yulon.ui.controller_view import ControllerServices, ControllerView  # noqa: E402

SERVER = Path("/home/pk/wowserver")
OUT = Path(__file__).resolve().parent / "shots"
OUT.mkdir(exist_ok=True)
MODULE_ID = os.environ.get("GATE_MODULE", "mod-npc-beastmaster")
NPC_ENTRY = int(os.environ.get("GATE_NPC", "601026"))
DBS = {"world": "acore_world", "characters": "acore_characters", "auth": "acore_auth"}


def say(text: str) -> None:
    print(text, flush=True)


def sh(argv: list[str], timeout: float = 120.0) -> subprocess.CompletedProcess[str]:
    return subprocess.run(argv, capture_output=True, text=True, timeout=timeout)


def alive(container: str = "ac-worldserver") -> bool:
    proc = sh(["docker", "inspect", "-f", "{{.State.Running}}", container], timeout=30)
    return proc.stdout.strip() == "true"


def sql(statement: str) -> str:
    proc = sh(
        ["docker", "exec", "ac-database", "mysql", "-uroot", "-ppassword", "-N", "-e", statement],
        timeout=60,
    )
    return proc.stdout.strip()


def ledger() -> dict[str, int]:
    """One row count per database's `updates` table -- what a write would move."""
    out: dict[str, int] = {}
    for label, schema in DBS.items():
        out[label] = int(sql(f"SELECT COUNT(*) FROM {schema}.updates;") or "-1")
    return out


def containers() -> list[str]:
    proc = sh(["docker", "ps", "-a", "--format", "{{.Names}}"], timeout=30)
    return sorted(n for n in proc.stdout.split() if n)


def shot(view: ControllerView, name: str, tab: str = "Modules") -> None:
    """Capture the MODULES tab, and record whether the world was alive at that instant.

    A screenshot of a client that has DIED photographs exactly like a refusal,
    so the liveness is logged beside every capture rather than assumed.

    The tab is selected here rather than once at startup, and that is not
    decoration: the first run of this gate captured eight images that were all
    byte-identical pictures of the SERVER tab, because `ControllerView` opens on
    it and nothing in the gate ever moved. Eight files, one of them apparently
    showing a refusal, none of them showing anything at all.
    """
    for index in range(view._tabs.count()):
        if view._tabs.tabText(index) == tab:
            view._tabs.setCurrentIndex(index)
            break
    else:  # pragma: no cover - a tab that vanished is worth a loud artifact
        say(f"!! no {tab} tab to capture")
    QApplication.processEvents()
    path = OUT / name
    view.grab().save(str(path))
    say(f"SHOT {name}  ac-worldserver alive at capture: {alive()}")


def build_view() -> ControllerView:
    entry = load_catalog().get("wow-wotlk")
    services = ControllerServices.for_entry(entry, SERVER)
    view = ControllerView(entry, services, status_poll_ms=0)
    view.resize(1100, 800)
    return view


def report(view: ControllerView) -> str:
    return view.module_report.toPlainText()


# --------------------------------------------------------------------------- 0


def step_ground(view: ControllerView) -> None:
    say("=== GROUND, read before anything is done ===")
    say(f"containers: {containers()}")
    say(f"import service: {docker_ctl.SPEC.import_service!r}")
    say(f"allowed_modules({SERVER}) = {docker.allowed_modules(SERVER)!r}")
    say(f"importer_sees_modules() = {docker.importer_sees_modules('ac-db-import', SERVER)}")
    say(f"ledger: {ledger()}")
    say(f"module clone on disk: {(SERVER / 'modules' / MODULE_ID).exists()}")
    say(f"creature {NPC_ENTRY} in creature_template: "
        f"{sql(f'SELECT COUNT(*) FROM acore_world.creature_template WHERE entry={NPC_ENTRY};')}")
    conf = (SERVER / "env/dist/etc/worldserver.conf").read_text(encoding="utf-8", errors="replace")
    line = next((ln for ln in conf.splitlines() if ln.startswith("Creatures.CustomIDs")), "(absent)")
    say(f"worldserver.conf Creatures.CustomIDs: {line}")
    say(f"  contains {NPC_ENTRY}: {str(NPC_ENTRY) in line}")
    say(f"services.module_updates wired: {view.services.module_updates is not None}")
    say(f"services.module_sql wired:     {view.services.module_sql is not None}")
    say(f"button '{view.module_updates_button.text()}' enabled="
        f"{view.module_updates_button.isEnabled()}")
    say(f"button '{view.module_sql_button.text()}' enabled={view.module_sql_button.isEnabled()}")


# --------------------------------------------------------------------------- 1


def step_behind(view: ControllerView, shot_name: str = "1-commits-behind.png") -> None:
    say("=== CLAUSE 1: the commits-behind figure equals the same query by hand ===")
    started = time.time()
    view.check_module_updates()
    # The press is asynchronous: `_run()` hands the fetch to a worker thread, so
    # a capture taken on the next line photographs the "asking…" placeholder and
    # proves nothing. Wait for the button to come back, which is what the user
    # waits for too.
    for _ in range(600):
        QApplication.processEvents()
        if view.module_updates_button.isEnabled() and "Asking" not in report(view):
            break
        time.sleep(0.5)
    text = report(view)
    say(f"--- what the tab shows ({time.time() - started:.1f}s) ---")
    say(text)
    say("--- end ---")
    shot(view, shot_name)

    rows = wotlk_modules.module_updates(SERVER)
    ok = True
    for row in rows:
        if not row.is_checkout:
            say(f"BY HAND {row.key}: not a checkout, nothing to compare")
            continue
        by_hand = sh(
            ["git", "-C", str(row.path), "rev-list", "--count", "HEAD..FETCH_HEAD"], timeout=60
        )
        say(f"BY HAND {row.key}: rev-list --count HEAD..FETCH_HEAD = "
            f"{by_hand.stdout.strip()!r} (app said {row.behind})")
        if by_hand.stdout.strip() != str(row.behind):
            ok = False
        head = sh(["git", "-C", str(row.path), "rev-parse", "--short", "HEAD"], timeout=30)
        fetched = sh(["git", "-C", str(row.path), "rev-parse", "--short", "FETCH_HEAD"], timeout=30)
        say(f"        HEAD={head.stdout.strip()} FETCH_HEAD={fetched.stdout.strip()}")
    say(f"CLAUSE 1: {'PASS' if ok and rows else 'FAIL'}")


# --------------------------------------------------------------------------- 2


def step_install(view: ControllerView) -> None:
    say("=== CLAUSE 2: SQL for the import one-shot is REPORTED PENDING, never claimed done ===")
    if (SERVER / "modules" / MODULE_ID).exists():
        say(f"GROUND FAILS: {MODULE_ID} is already installed; this step would prove nothing")
        return
    manifest = next(m for m in view.services.store.load_all("module") if m.id == MODULE_ID)
    applier = view.services.applier
    assert applier is not None
    result = applier.install(manifest)
    say("--- ApplyReport.done ---")
    for entry in result.done:
        say(f"  {entry}")
    say("--- ApplyReport.pending_sql ---")
    for pending in result.pending_sql:
        say(f"  db={pending.db} path={pending.path} files={list(pending.files)}")
    say("--- ApplyReport.skipped ---")
    for entry in result.skipped:
        say(f"  {entry}")
    say(f"rebuild_required={result.rebuild_required} restart={result.restart_recommended}")
    claimed = [entry for entry in result.done if "sql" in entry]
    say(f"steps in `done` mentioning sql (must be none): {claimed}")
    view.module_report.setPlainText(_format(result))
    shot(view, "2-pending-not-done.png")
    say(f"allowed_modules() now: {docker.allowed_modules(SERVER)!r}")
    conf = (SERVER / "env/dist/etc/worldserver.conf").read_text(encoding="utf-8", errors="replace")
    line = next((ln for ln in conf.splitlines() if ln.startswith("Creatures.CustomIDs")), "(absent)")
    say(f"worldserver.conf Creatures.CustomIDs after install: {line}")
    say(f"CLAUSE 2: {'PASS' if not claimed and result.pending_sql else 'FAIL'}")


def _format(result: object) -> str:
    from yulon.ui.controller_view import _format_report

    return _format_report(result)  # type: ignore[arg-type]


# --------------------------------------------------------------------------- 3


def step_refused(view: ControllerView) -> None:
    say("=== CLAUSE 3: refused while the world is running, and NO ROWS WRITTEN ===")
    say(f"ground: ac-worldserver alive = {alive()}, ac-authserver alive = {alive('ac-authserver')}")
    before_rows = ledger()
    before_containers = containers()
    before_npc = sql(
        f"SELECT COUNT(*) FROM acore_world.creature_template WHERE entry={NPC_ENTRY};"
    )
    say(f"ledger BEFORE: {before_rows}")
    say(f"creature {NPC_ENTRY} BEFORE: {before_npc}")
    started = time.time()
    view.apply_module_sql()
    # The refusal arrives on the GUI thread AFTER the worker raises, so a
    # capture on the next line photographs the "Running…" placeholder and reads
    # as a press that is still in flight. Wait for the tab to settle.
    for _ in range(600):
        QApplication.processEvents()
        if not view._module_sql_running:
            break
        time.sleep(0.5)
    QApplication.processEvents()
    text = report(view)
    say("--- what the tab shows ---")
    say(text)
    say("--- end ---")
    shot(view, "3-refused-world-up.png")
    after_rows = ledger()
    after_npc = sql(f"SELECT COUNT(*) FROM acore_world.creature_template WHERE entry={NPC_ENTRY};")
    written = {k: after_rows[k] - before_rows[k] for k in after_rows}
    say(f"ledger AFTER:  {after_rows}")
    say(f"rows written by the refused press: {written}")
    say(f"creature {NPC_ENTRY} AFTER: {after_npc}")
    say(f"containers created by the press: "
        f"{sorted(set(containers()) - set(before_containers))}")
    say(f"button enabled again: {view.module_sql_button.isEnabled()}  "
        f"took {time.time() - started:.1f}s")
    ok = (
        "running" in text
        and "Press Stop first" in text
        and all(v == 0 for v in written.values())
        and after_npc == before_npc
        and not (set(containers()) - set(before_containers))
    )
    say(f"CLAUSE 3: {'PASS' if ok else 'FAIL'}")


# --------------------------------------------------------------------------- 4


BEFORE_STOP: dict[str, int] = {}


def step_stop(view: ControllerView) -> None:
    say("=== stopping this install through the app's own Stop ===")
    # Read the ledger HERE, with the database still up. The app's Stop takes
    # ac-database down with the servers, so a count taken after it answers -1
    # for every schema -- which is what the first run of this gate recorded as
    # its "before", and -1 subtracted from a real count reads as a plausible
    # number rather than as a failed read.
    BEFORE_STOP.update(ledger())
    say(f"ledger with the database still UP: {BEFORE_STOP}")
    started = time.time()
    view.services.controller.stop()
    say(f"stopped in {time.time() - started:.1f}s; ac-worldserver alive = {alive()}")
    view.refresh_status()
    QApplication.processEvents()
    shot(view, "4-stopped.png")


# --------------------------------------------------------------------------- 5


def step_apply(view: ControllerView) -> None:
    say("=== CLAUSE 4: it applies once the server is stopped ===")
    say(f"ground: ac-worldserver alive = {alive()}, ac-authserver alive = {alive('ac-authserver')}")
    before = ledger()
    if any(v < 0 for v in before.values()) and BEFORE_STOP:
        say(f"ledger unreadable with the database down ({before}); using the count taken "
            f"before the stop, with nothing run in between")
        before = dict(BEFORE_STOP)
    before_npc = sql(f"SELECT COUNT(*) FROM acore_world.creature_template WHERE entry={NPC_ENTRY};")
    say(f"ledger BEFORE: {before}")
    say(f"creature {NPC_ENTRY} BEFORE: {before_npc}")
    started = time.time()
    view.apply_module_sql()
    for _ in range(2000):
        QApplication.processEvents()
        if not view._module_sql_running:
            break
        time.sleep(0.5)
    text = report(view)
    say(f"--- what the tab shows ({time.time() - started:.1f}s) ---")
    say(text)
    say("--- end ---")
    shot(view, "5-applied-world-down.png")
    after = ledger()
    after_npc = sql(f"SELECT COUNT(*) FROM acore_world.creature_template WHERE entry={NPC_ENTRY};")
    say(f"ledger AFTER:  {after}")
    say("rows written: " + repr({k: after[k] - before[k] for k in after}))
    say(f"creature {NPC_ENTRY} AFTER: {after_npc}")
    say(f"new updates rows: {sql('SELECT name FROM acore_world.updates ORDER BY timestamp DESC LIMIT 6;')}")
    ok = ">> Applying update" in text and after["world"] > before["world"]
    say(f"CLAUSE 4: {'PASS' if ok else 'FAIL'}")


# --------------------------------------------------------------------------- 6


def step_start(view: ControllerView) -> None:
    say("=== starting the install again (the restart the module asks for) ===")
    started = time.time()
    view.services.controller.start()
    say(f"start returned in {time.time() - started:.1f}s")
    for _ in range(120):
        logs = sh(["docker", "logs", "--tail", "400", "ac-worldserver"], timeout=60).stdout
        if "ready..." in logs:
            break
        time.sleep(10)
    say(f"world reached its ready marker: {'ready...' in logs}")
    view.refresh_status()
    QApplication.processEvents()
    shot(view, "6-started.png", tab="Server")


def step_serverstatus(view: ControllerView) -> None:
    """Re-capture the Server tab with its status line SETTLED.

    `refresh_status()` is asynchronous like every other press here, so the first
    capture after a start photographs `status: unknown` -- the label before the
    read comes back -- on a server that is up. Same class of artifact as the
    eight Server-tab pictures the first run produced: a file that reads as a
    fact and is a placeholder.
    """
    say("=== the Server tab, with the status read settled ===")
    view.refresh_status()
    for _ in range(120):
        QApplication.processEvents()
        if "unknown" not in view.status_label.text():
            break
        time.sleep(0.5)
    say(f"status label: {view.status_label.text()!r}")
    shot(view, "6-started.png", tab="Server")


def step_running(view: ControllerView) -> None:
    say("=== CLAUSE 5: the running server shows it, not the file ===")
    say(f"ac-worldserver alive: {alive()}")
    reply = wotlk_console.send_command(".server debug")
    say("--- .server debug ---")
    for line in reply.lines:
        say(f"  {line}")
    lookup = wotlk_console.send_command(f".lookup creature White Fang")
    say("--- .lookup creature White Fang ---")
    for line in lookup.lines:
        say(f"  {line}")
    view.module_report.setPlainText(
        ".server debug\n" + "\n".join(reply.lines)
        + "\n\n.lookup creature White Fang\n" + "\n".join(lookup.lines)
    )
    shot(view, "7-running-server-states-it.png")

    say("--- the conf key, in the running server's own load ---")
    logs = sh(["docker", "logs", "--since", "30m", "ac-worldserver"], timeout=120).stdout
    for needle in ("Loading Creature Custom IDs Config", "Loading Modules Configuration"):
        for line in logs.splitlines():
            if needle in line:
                say(f"  {line.strip()}")
    complaints = [ln.strip() for ln in logs.splitlines() if "has assigned gossip menu" in ln]
    say(f"gossip complaints in this boot: {len(complaints)}")
    for line in complaints[:5]:
        say(f"  {line}")
    row = sql(
        "SELECT entry, gossip_menu_id, npcflag, flags_extra FROM acore_world.creature_template "
        f"WHERE entry={NPC_ENTRY};"
    )
    say(f"creature_template row for {NPC_ENTRY} (entry gossip_menu_id npcflag flags_extra): {row}")
    say(f"complaint naming {NPC_ENTRY}: "
        f"{[c for c in complaints if str(NPC_ENTRY) in c] or 'none'}")


def step_rewind(view: ControllerView) -> None:
    """Put the fresh clone three commits back, so the figure is not 0 == 0.

    A gate step whose assertion is already true proves nothing, and "up to date"
    is what a clone made ten minutes ago always is. `reset --hard HEAD~3` is the
    same move the app's own update path makes in the other direction, on a clone
    this app created; the ground -- which commit it was on -- is printed both
    sides so the figure can be checked against the history by hand.
    """
    clone = SERVER / "modules" / MODULE_ID
    say("=== CLAUSE 1 again, against a checkout that really IS behind ===")
    was = sh(["git", "-C", str(clone), "rev-parse", "HEAD"], timeout=30).stdout.strip()
    say(f"HEAD before the rewind: {was}")
    shallow = sh(["git", "-C", str(clone), "rev-parse", "--is-shallow-repository"], timeout=30)
    say(f"is-shallow-repository: {shallow.stdout.strip()!r}  "
        "(the app clones modules shallow, so HEAD~3 does not exist until it is deepened)")
    deepen = sh(["git", "-C", str(clone), "fetch", "--deepen", "10", "origin"], timeout=180)
    say(f"fetch --deepen 10 -> rc={deepen.returncode} {deepen.stderr.strip()[:200]}")
    reset = sh(["git", "-C", str(clone), "reset", "--hard", "HEAD~3"], timeout=60)
    say(f"reset --hard HEAD~3 -> rc={reset.returncode} {reset.stdout.strip()}{reset.stderr.strip()[:200]}")
    now = sh(["git", "-C", str(clone), "rev-parse", "HEAD"], timeout=30).stdout.strip()
    say(f"HEAD after  the rewind: {now}")
    say(f"the three commits skipped: "
        f"{sh(['git', '-C', str(clone), 'log', '--oneline', f'{now}..{was}'], timeout=30).stdout}")
    step_behind(view, shot_name="1b-commits-behind-nonzero.png")


def step_config(view: ControllerView) -> None:
    """CLAUSE 5, and the two things that stop it being met on a box with no rebuild.

    What is proved here is the MECHANISM -- this tree's running server does state
    worldserver.conf values back, so "shown by the running server" is a question
    with an answer on wow-wotlk -- and then, precisely, which two facts stop the
    module's own configuration from reaching it.
    """
    say("=== CLAUSE 5: what the running server does and does not show about config ===")
    say(f"ac-worldserver alive: {alive()}")
    conf_path = SERVER / "env/dist/etc/worldserver.conf"
    conf = conf_path.read_text(encoding="utf-8", errors="replace")

    def value(key: str) -> str:
        for line in conf.splitlines():
            if line.strip().startswith(key):
                return line.strip()
        return f"{key}: (absent -- the compiled default is in force)"

    say("--- worldserver.conf, on disk ---")
    for key in (
        "vmap.enableLOS",
        "vmap.enableHeight",
        "vmap.enableIndoorCheck",
        "MoveMaps.Enable",
        "DBC.Locale",
        "Creatures.CustomIDs",
    ):
        say(f"  {value(key)}")

    reply = wotlk_console.send_command(".server debug")
    stated = [
        line
        for line in reply.lines
        if any(w in line for w in ("VMAPs status", "MMAPs status", "DBC locale", "enabled modules"))
        or line.strip().startswith("|-")
    ]
    say("--- the same values, as the RUNNING server states them (.server debug) ---")
    for line in stated:
        say(f"  {line}")

    say("--- the module's own conf, and why the running server cannot show it ---")
    mod_conf = SERVER / "env/dist/etc/modules/mod_npc_beastmaster.conf"
    say(f"  {mod_conf} on disk: {mod_conf.is_file()}")
    say(f"  compiled modules the server lists: "
        f"{[ln.strip() for ln in reply.lines if ln.strip().startswith('|- mod')]}")
    logs = sh(["docker", "logs", "--since", "20m", "ac-worldserver"], timeout=120).stdout
    for line in logs.splitlines():
        if "Modules Configuration" in line or "modules config" in line or ".conf'" in line:
            say(f"  {line.strip()}")

    view.module_report.setPlainText(
        "worldserver.conf on disk:\n"
        + "\n".join(f"  {value(k)}" for k in ("vmap.enableLOS", "MoveMaps.Enable", "DBC.Locale",
                                              "Creatures.CustomIDs"))
        + "\n\nthe RUNNING server, asked (.server debug):\n"
        + "\n".join(f"  {ln}" for ln in stated)
    )
    shot(view, "8-config-and-the-running-server.png")



def step_remove(view: ControllerView) -> None:
    """Take a module back off, through the tab's own Remove.

    Used here to put back a module chosen for the gate that turned out to ship
    no db-world SQL at all -- `mod-junk-to-gold`'s glob matched nothing, which
    the report says plainly rather than guessing, and a module with nothing
    pending cannot prove the clause about applying what is pending.
    """
    say(f"=== removing {MODULE_ID} through the applier ===")
    manifest = next(m for m in view.services.store.load_all("module") if m.id == MODULE_ID)
    applier = view.services.applier
    assert applier is not None
    result = applier.remove(manifest)
    for entry in result.done:
        say(f"  done: {entry}")
    for entry in result.skipped:
        say(f"  skipped: {entry}")
    say(f"clone on disk after remove: {(SERVER / 'modules' / MODULE_ID).exists()}")
    say(f"allowed_modules() now: {docker.allowed_modules(SERVER)!r}")


STEPS = {
    "remove": step_remove,
    "serverstatus": step_serverstatus,
    "ground": step_ground,
    "config": step_config,
    "behind": step_behind,
    "rewind": step_rewind,
    "install": step_install,
    "refused": step_refused,
    "stop": step_stop,
    "apply": step_apply,
    "start": step_start,
    "running": step_running,
}


def main() -> int:
    app = QApplication([])
    view = build_view()
    view.show()
    QApplication.processEvents()
    for name in sys.argv[1:]:
        STEPS[name](view)
    say("done")
    del app
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
