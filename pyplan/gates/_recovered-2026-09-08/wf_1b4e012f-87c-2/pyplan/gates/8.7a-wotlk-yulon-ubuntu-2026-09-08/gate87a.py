"""The 8.7a live gate: modules on a real AzerothCore WotLK server.

Usage:  python gate87a.py <stage>

    ground   read the install and the databases; write nothing
    behind   press "Check for updates" and compare with `git rev-list` by hand
    install  press "Install selected" on mod-transmog, and read what it CLAIMS
    refuse   press "Apply module SQL" with the world UP, and count the rows
    stop     press Stop on the Server tab
    apply    press "Apply module SQL" with the world DOWN
    start    press Start, and ask the running worldserver what it loaded

Every press goes through a real `ControllerView` built by
`ControllerServices.for_entry()` against `/home/pk/wowserver`. Nothing below the
view is faked: the applier clones from GitHub, the importer is a real container,
and the row counts come from `mysql` inside `ac-database` rather than from the
app.
"""

from __future__ import annotations

import os
import subprocess
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

TREE = Path("/home/pk/gate87a-live/pylauncher")
SERVER_DIR = Path("/home/pk/wowserver")
SHOTS = Path("/home/pk/gate87a-shots")
MODULE = "mod-transmog"

sys.path.insert(0, str(TREE))

from PySide6.QtGui import QTextCursor  # noqa: E402
from PySide6.QtWidgets import QApplication  # noqa: E402

from yulon import docker  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices, ControllerView  # noqa: E402

ENTRY = load_catalog().get("wow-wotlk")


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def sh(argv: list[str]) -> str:
    return subprocess.run(argv, capture_output=True, text=True).stdout.strip()


def containers() -> list[str]:
    return sorted(sh(["docker", "ps", "-a", "--format", "{{.Names}}"]).split())


def alive() -> list[str]:
    return sorted(sh(["docker", "ps", "--format", "{{.Names}}"]).split())


def query(statement: str) -> str:
    """One SQL question, asked BY HAND inside the database container.

    Not through the app: the clause is that no rows were written, and an app
    that answered its own question about its own write would be the only
    witness to it.
    """
    return sh(
        [
            "docker",
            "exec",
            "ac-database",
            "sh",
            "-c",
            f'mysql -uroot -p"$MYSQL_ROOT_PASSWORD" -N -B -e "{statement}"',
        ]
    )


COUNTS = {
    "acore_world.updates": "SELECT COUNT(*) FROM acore_world.updates",
    "acore_characters.updates": "SELECT COUNT(*) FROM acore_characters.updates",
    "acore_world.module_string": "SELECT COUNT(*) FROM acore_world.module_string",
    "acore_world.creature_template#190010": (
        "SELECT COUNT(*) FROM acore_world.creature_template WHERE entry = 190010"
    ),
    "acore_characters tables": (
        "SELECT COUNT(*) FROM information_schema.tables "
        "WHERE table_schema = 'acore_characters'"
    ),
    "acore_world tables": (
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'acore_world'"
    ),
}


BASELINE = {
    "acore_world.updates": "2968",
    "acore_characters.updates": "29",
    "acore_world.module_string": "8",
    "acore_world.creature_template#190010": "0",
    "acore_characters tables": "111",
    "acore_world tables": "315",
}
"""What every one of `COUNTS` answered with the stack UP, on 2026-09-08.

Read twice, by hand, inside `ac-database`: at 07:43:34Z (`ground`) and again at
07:46:29Z and 07:46:30Z either side of the refused press (`refuse`). Identical
all three times. It is written down because the `apply` stage runs with the
database STOPPED and cannot ask — Stop stops all three — so the figure it
compares against has to be one that was taken while there was something to ask.
"""


def counts() -> dict[str, str]:
    return {name: query(sql) for name, sql in COUNTS.items()}


def show_counts(label: str, values: dict[str, str]) -> None:
    say(f"{label}:")
    for name, value in values.items():
        print(f"    {name:38} = {value}", flush=True)


def by_hand_behind(clone: str) -> str:
    """`git fetch origin HEAD && git rev-list --count HEAD..FETCH_HEAD`, run by a person.

    The literal pair of commands the checklist's "the same query run by hand"
    names. Run in the clone with the host's own git, not through any seam of
    the app's.
    """
    path = SERVER_DIR / "modules" / clone
    subprocess.run(["git", "fetch", "origin", "HEAD"], cwd=path, capture_output=True, text=True)
    return sh(["git", "-C", str(path), "rev-list", "--count", "HEAD..FETCH_HEAD"])


def modules_on_disk() -> list[str]:
    return sorted(p.name for p in (SERVER_DIR / "modules").iterdir() if p.is_dir())


def build() -> tuple[QApplication, ControllerView]:
    app = QApplication.instance() or QApplication([])
    services = ControllerServices.for_entry(ENTRY, SERVER_DIR)
    view = ControllerView(ENTRY, services, status_poll_ms=0)
    view.resize(1180, 860)
    return app, view


def tab_index(view: ControllerView, title: str) -> int:
    for i in range(view._tabs.count()):
        if view._tabs.tabText(i) == title:
            return i
    raise AssertionError(f"no {title} tab")


def shoot(view: ControllerView, name: str, tab: str = "Modules") -> None:
    """Render the real widgets to a PNG, and say whether the app is still there.

    A dead window photographs like a refusal, so the caller's transcript has to
    carry proof of life beside the file name.
    """
    SHOTS.mkdir(parents=True, exist_ok=True)
    view._tabs.setCurrentIndex(tab_index(view, tab))
    view.show()
    QApplication.processEvents()
    target = SHOTS / name
    view.grab().save(str(target))
    say(f"shot {target} ({target.stat().st_size} bytes), view alive: {view.isVisible()}")


APPLIED_MARKER = 'Applying update "'
"""What an importer line really looks like, and why the obvious grep is wrong.

`>> Applying update "trasm_world_NPC.sql" '2798F80'...` is the importer's; the
tab's own closing sentence (`MODULE_SQL_FINISHED`) QUOTES that marker to explain
it — "Any '>> Applying update <file>.sql' line above is a file that was applied
just now" — so a check for `>> Applying update` matches the app explaining
itself and is true of a run that applied nothing. It cost this driver one FAIL
verdict at 07:49:44Z on a stage that had actually passed. The double quote is
the importer's and the app's sentence has no file name in it.
"""


def focus(view: ControllerView, needle: str) -> bool:
    """Scroll the report to the line a screenshot is being taken FOR.

    A `QPlainTextEdit` shows its first screenful, and the first screenful of an
    importer run is compose's container chatter and MySQL's banner — identical
    between a run that applied five files and one that applied none. Two
    screenshots taken this way came out byte-for-byte the same size, which is
    what raised it. The picture has to be of the evidence.
    """
    box = view.module_report
    box.moveCursor(QTextCursor.MoveOperation.Start)
    found = box.find(needle)
    box.ensureCursorVisible()
    QApplication.processEvents()
    say(f"scrolled the report to {needle!r}: {found}")
    return found


def pump(app: QApplication, done, limit: float, what: str) -> float:
    started = time.monotonic()
    while time.monotonic() - started < limit and not done():
        app.processEvents()
        time.sleep(0.05)
    took = time.monotonic() - started
    say(f"{what}: {'finished' if done() else 'TIMED OUT'} after {took:.1f}s")
    return took


def quiet(view: ControllerView):
    """Waits for the busy lock — every control on this tab takes it EXCEPT one.

    `_module_action()` (Install/Remove) does not call `_set_busy()` at all, so
    this predicate is true the instant the click returns and the driver would
    photograph a clone that is still being made. Found the hard way on the
    first press of the install stage, 09:44 local: "install mod-transmog:
    finished after 0.0s", an empty report, and a `QThread: Destroyed while
    thread is still running` abort at exit. Install waits on `finished()`.
    """
    return lambda: not view._busy


def finished(view: ControllerView):
    """Waits for the Modules tab's own pending marker, which every module action clears."""
    return lambda: view._module_pending is None and not view._busy


def report(view: ControllerView) -> str:
    text = view.module_report.toPlainText()
    print("--- what the Modules tab shows ---", flush=True)
    print(text, flush=True)
    print("--- end ---", flush=True)
    return text


def select(view: ControllerView, item_id: str) -> None:
    for i in range(view.module_list.count()):
        if view.module_list.item(i).data(256) == item_id:
            view.module_list.setCurrentRow(i)
            say(f"selected: {view.module_list.item(i).text()}")
            return
    raise AssertionError(f"{item_id} is not in the Modules list")


# ---------------------------------------------------------------- the stages


def stage_ground() -> int:
    say("containers alive: " + ", ".join(alive()))
    say("modules on disk: " + ", ".join(modules_on_disk()))
    say(f"docker.allowed_modules() = {docker.allowed_modules(SERVER_DIR)!r}")
    spec = ENTRY.container_spec()
    say(f"import service: {spec.import_service!r}")
    say(f"importer_sees_modules() = {docker.importer_sees_modules(spec.import_service, SERVER_DIR)!r}")
    show_counts("row counts, asked of ac-database by hand", counts())
    for clone in modules_on_disk():
        say(f"by hand: {clone} is {by_hand_behind(clone)} commits behind FETCH_HEAD")
    say("worldserver's own words about module configuration, this run:")
    log = sh(["docker", "logs", "ac-worldserver"])
    for line in log.splitlines():
        if "Modules Configuration" in line or "module strings" in line or "modules/" in line:
            print("    " + line, flush=True)
    return 0


def stage_behind() -> int:
    app, view = build()
    on_disk = modules_on_disk()
    say(f"modules on disk before the press: {on_disk}")
    view.updates_button.click()
    pump(app, quiet(view), 300, "Check for updates")
    text = report(view)
    shoot(view, "1-behind.png")

    ok = "rev-list --count HEAD..FETCH_HEAD" in text
    for clone in on_disk:
        hand = by_hand_behind(clone)
        if hand == "0":
            expected = f"{clone} — up to date"
        else:
            unit = "commit" if hand == "1" else "commits"
            expected = f"{clone} — {hand} {unit} behind"
        found = expected in text
        say(f"by hand {clone}={hand}; tab says {expected!r}: {found}")
        ok = ok and found
    say(f"VERDICT behind: {'PASS' if ok else 'FAIL'}")
    return 0 if ok else 1


def stage_behindpin() -> int:
    """The same clause with a NON-ZERO answer, on a clone deliberately moved back.

    Both modules on this install were at their remote's tip, so the first press
    could only ever produce "up to date" — a figure that agrees with the
    hand-run query by agreeing with zero. The design asked for a module
    "deliberately pinned one commit behind" for exactly this reason. The clone
    is deepened (it is a depth-1 clone and has no parent to move to otherwise),
    moved back two commits, asked, and put back.
    """
    clone = SERVER_DIR / "modules" / MODULE
    tip = sh(["git", "-C", str(clone), "rev-parse", "HEAD"])
    say(f"{MODULE} tip before: {tip}")
    say(sh(["git", "-C", str(clone), "fetch", "--deepen", "5", "origin", "HEAD"]) or "deepened")
    say("history depth now: " + sh(["git", "-C", str(clone), "rev-list", "--count", "HEAD"]))
    subprocess.run(["git", "-C", str(clone), "reset", "--hard", "HEAD~2"], capture_output=True)
    moved = sh(["git", "-C", str(clone), "rev-parse", "HEAD"])
    say(f"{MODULE} moved back to: {moved}")

    app, view = build()
    view.updates_button.click()
    pump(app, quiet(view), 300, "Check for updates (pinned back)")
    text = report(view)
    shoot(view, "1b-behind-pinned.png")

    hand = by_hand_behind(MODULE)
    say(f"by hand {MODULE} = {hand} commits behind")
    ok = f"{MODULE} — {hand} commits behind" in text and hand == "2"

    subprocess.run(["git", "-C", str(clone), "reset", "--hard", tip], capture_output=True)
    back = sh(["git", "-C", str(clone), "rev-parse", "HEAD"])
    say(f"{MODULE} put back to: {back} (matches the tip: {back == tip})")
    files = sorted(p.name for p in clone.glob("data/sql/db-world/*.sql"))
    say(f"world SQL files after the restore: {files}")
    ok = ok and back == tip and len(files) == 3
    say(f"VERDICT behindpin: {'PASS' if ok else 'FAIL'}")
    return 0 if ok else 1


def stage_install() -> int:
    app, view = build()
    before = counts()
    show_counts("before the install", before)
    select(view, MODULE)
    view.install_module_button.click()
    pump(app, finished(view), 900, f"install {MODULE}")
    text = report(view)
    shoot(view, "2-installed.png")
    after = counts()
    show_counts("after the install", after)

    clone = SERVER_DIR / "modules" / MODULE
    say(f"clone present: {clone.is_dir()}, .git: {(clone / '.git').is_dir()}")
    sql_files = sorted(p.name for p in clone.glob("data/sql/db-world/*.sql"))
    say(f"world SQL files the clone brought: {sql_files}")
    conf = SERVER_DIR / "env/dist/etc/modules/transmog.conf"
    say(f"conf written: {conf.is_file()}")
    say(f"modules on disk now: {modules_on_disk()}")
    say(f"docker.allowed_modules() now = {docker.allowed_modules(SERVER_DIR)!r}")

    # 8.7a's third clause, which is what this stage is for: SQL marked for the
    # import one-shot must be reported as NOT applied, with the files it really
    # found — never ticked as done. A tick is `✓`; the deferral is `! NOT
    # applied`, and until 2026-09-07 the deferral WAS a tick.
    ticked_sql = [line for line in text.splitlines() if line.strip().startswith("✓") and "sql" in line]
    say(f"lines that TICK a sql step: {ticked_sql}")
    ok = (
        clone.is_dir()
        and conf.is_file()
        and before == after
        and "! NOT applied:" in text
        and not ticked_sql
    )
    say(f"VERDICT install: {'PASS' if ok else 'FAIL'} (databases unchanged: {before == after})")
    return 0 if ok else 1


def stage_refuse() -> int:
    app, view = build()
    say("containers alive: " + ", ".join(alive()))
    before, before_containers = counts(), containers()
    show_counts("before the refused press", before)
    failures: list[str] = []
    view.action_failed.connect(failures.append)
    view.module_sql_button.click()
    pump(app, quiet(view), 300, "Apply module SQL (world up)")
    text = report(view)
    shoot(view, "3-refused-world-up.png")
    after, after_containers = counts(), containers()
    show_counts("after the refused press", after)
    say(f"containers created by the press: {sorted(set(after_containers) - set(before_containers))}")

    ok = (
        bool(failures)
        and "running" in text
        and "Press Stop first" in text
        and "FAILED" in text
        and before == after
        and after_containers == before_containers
    )
    say(f"rows unchanged: {before == after}")
    say(f"VERDICT refuse: {'PASS' if ok else 'FAIL'}")
    return 0 if ok else 1


def stage_stop() -> int:
    app, view = build()
    pump(app, lambda: not view._status_pending, 120, "first status read")
    say(f"stop button enabled: {view.stop_button.isEnabled()}")
    view.stop_button.click()
    pump(app, quiet(view), 900, "Stop")
    say("containers alive now: " + ", ".join(alive()))
    say(f"status label: {view.status_label.text()}")
    shoot(view, "4-stopped.png", tab="Server")
    ok = "ac-worldserver" not in alive() and "ac-authserver" not in alive()
    say(f"VERDICT stop: {'PASS' if ok else 'FAIL'}")
    return 0 if ok else 1


def stage_apply() -> int:
    app, view = build()
    pump(app, lambda: not view._status_pending, 120, "first status read")
    say("containers alive: " + (", ".join(alive()) or "(none)"))
    say(f"status label: {view.status_label.text()}")
    shoot(view, "4b-server-stopped.png", tab="Server")
    # The database is DOWN — Stop stops all three — so there is nothing to ask
    # here. The baseline is the one the `ground` and `refuse` stages both read
    # with the stack up, minutes earlier and identical in both; the only thing
    # between them and this press was the refused press, which is the stage
    # that asserted it changed nothing.
    before = BASELINE
    show_counts("baseline (read with the stack up, ground + refuse)", before)
    view.module_sql_button.click()
    pump(app, quiet(view), 1800, "Apply module SQL (world down)")
    text = report(view)
    focus(view, APPLIED_MARKER)
    shoot(view, "5-applied.png")
    after = counts()
    show_counts("after the apply", after)
    for name in COUNTS:
        if before[name] != after[name]:
            say(f"MOVED  {name}: {before[name]} -> {after[name]}")

    ok = (
        APPLIED_MARKER in text
        and "Loading modules:" in text
        and after["acore_world.updates"] != before["acore_world.updates"]
    )
    say(f"VERDICT apply: {'PASS' if ok else 'FAIL'}")
    return 0 if ok else 1


def stage_again() -> int:
    """The second press, for the clause the first one's output proves and cannot show twice.

    Two things at once:

    * **the configuration change the module needs, shown by the running
      server.** There is no file to read it back from — `allowed_modules()`
      computes the list from the modules folder on disk and it travels in argv
      as `-e AC_UPDATES_ALLOWED_MODULES=…` — so the only witness is the
      importer itself, which says `Configuration field Updates.AllowedModules
      was overridden with environment variable` and then names the list it is
      using. That line is what makes the difference between a run that applies
      a module's SQL and the `Loading modules: all` run that applies nothing.
    * **the ledger holds.** A module already recorded in `updates` correctly
      gets nothing, so this press must move no row at all.
    """
    app, view = build()
    say("containers alive: " + (", ".join(alive()) or "(none)"))
    before = counts()
    show_counts("before the second press", before)
    view.module_sql_button.click()
    pump(app, quiet(view), 1800, "Apply module SQL (second press)")
    text = report(view)
    (SHOTS / "second-press-report.txt").write_text(text, encoding="utf-8")
    focus(view, "Updates.AllowedModules")
    shoot(view, "6-config-from-the-running-importer.png")
    after = counts()
    show_counts("after the second press", after)

    override = [line for line in text.splitlines() if "Updates.AllowedModules" in line]
    loading = [line for line in text.splitlines() if "Loading modules" in line]
    say(f"the importer's own words about its configuration: {override}")
    say(f"and the list it used: {loading}")
    ok = bool(override) and bool(loading) and before == after and APPLIED_MARKER not in text
    say(f"rows unchanged by the second press: {before == after}")
    say(f"VERDICT again: {'PASS' if ok else 'FAIL'}")
    return 0 if ok else 1


SECOND = "mod-solocraft"


def stage_second() -> int:
    """The whole apply clause again on a SECOND module, with the world still down.

    Two reasons, and neither is thoroughness for its own sake:

    * the first apply's picture showed the wrong screenful (see `focus()`), and
      that run cannot be repeated — its files are ledgered in `updates` now, so
      pressing again correctly applies nothing. A module whose SQL is still
      pending is the only way to photograph an apply happening;
    * one module could be a coincidence of that repository's layout.
      `mod-solocraft` keeps a single file where `mod-transmog` keeps three plus
      an `updates/` folder.

    The install half runs with the server DOWN, which is a state the earlier
    install was not pressed in.
    """
    app, view = build()
    say("containers alive: " + (", ".join(alive()) or "(none)"))
    before = counts()
    show_counts("before installing " + SECOND, before)
    select(view, SECOND)
    view.install_module_button.click()
    pump(app, finished(view), 900, f"install {SECOND}")
    installed = report(view)
    shoot(view, "7a-second-installed.png")
    say(f"docker.allowed_modules() now = {docker.allowed_modules(SERVER_DIR)!r}")
    middle = counts()
    say(f"the install alone changed nothing: {before == middle}")

    view.module_sql_button.click()
    pump(app, quiet(view), 1800, f"Apply module SQL for {SECOND}")
    applied = report(view)
    (SHOTS / "second-module-report.txt").write_text(applied, encoding="utf-8")
    focus(view, APPLIED_MARKER)
    shoot(view, "7b-second-applied.png")
    after = counts()
    show_counts("after the apply", after)
    for name in COUNTS:
        if middle[name] != after[name]:
            say(f"MOVED  {name}: {middle[name]} -> {after[name]}")

    ok = (
        "! NOT applied:" in installed
        and before == middle
        and APPLIED_MARKER in applied
        and after != middle
    )
    say(f"VERDICT second: {'PASS' if ok else 'FAIL'}")
    return 0 if ok else 1


def stage_start() -> int:
    app, view = build()
    pump(app, lambda: not view._status_pending, 120, "first status read")
    say(f"start button enabled: {view.start_button.isEnabled()}")
    view.start_button.click()
    pump(app, quiet(view), 900, "Start")
    say("containers alive now: " + ", ".join(alive()))
    # The worldserver takes a while to get past its own configuration; wait for
    # the line rather than for a clock.
    started = time.monotonic()
    while time.monotonic() - started < 600:
        log = sh(["docker", "logs", "--since", "10m", "ac-worldserver"])
        if "module strings" in log:
            break
        time.sleep(5)
    say("what the RUNNING worldserver says it loaded, this start:")
    for line in log.splitlines():
        if (
            "Modules Configuration" in line
            or "module strings" in line
            or "modules/" in line
            or "World initialized" in line
        ):
            print("    " + line, flush=True)
    shoot(view, "6-started.png", tab="Server")
    say(f"status label: {view.status_label.text()}")
    return 0


def stage_words() -> int:
    """What the RUNNING worldserver says about this start, and where a picture stops.

    Two different questions, and they get different answers on this install:

    * the module STRINGS the apply wrote — the server counts them itself at
      startup and says so, and the number has to equal what the database holds;
    * the module CONF the install wrote — asked here rather than assumed,
      because "shown by the running server, not only read back from the file"
      is the clause and a file this app wrote is not evidence about a server.

    The block is pulled out of `docker logs` at the LAST `Loading Modules
    Configuration`, because the container carries every run it has ever had and
    the first one is days old — reading from the top answers about a server
    that stopped hours ago.
    """
    log = sh(["docker", "logs", "ac-worldserver"]).splitlines()
    starts = [i for i, line in enumerate(log) if "Loading Modules Configuration" in line]
    say(f"this container has logged {len(starts)} start(s); reading the last one")
    block = log[starts[-1] :]
    keep = [
        line
        for line in block
        if "Module" in line or "module" in line or "World initialized" in line
    ]
    (SHOTS / "logs").mkdir(parents=True, exist_ok=True)
    (SHOTS / "logs" / "worldserver-this-start.log").write_text("\n".join(keep), encoding="utf-8")
    for line in keep[:12]:
        print("    " + line, flush=True)
    say("what the etc/modules folder holds now:")
    for entry in sorted((SERVER_DIR / "env/dist/etc/modules").iterdir()):
        print(f"    {entry.name}", flush=True)
    say(f"acore_world.module_string by hand = {query(COUNTS['acore_world.module_string'])}")

    app, view = build()
    view._tabs.setCurrentIndex(tab_index(view, "Console"))
    view.follow_button.click()
    pump(app, lambda: len(view.console_log.text()) > 400, 120, "worldserver log stream")
    shoot(view, "8-console-live.png", tab="Console")
    for panel in view.log_panels():
        panel.stop()
    return 0


def stage_lookup() -> int:
    """Ask the RUNNING world about a row the apply wrote, through the app's console box.

    The nearest thing to the checklist's "visible effect" that does not need a
    rebuild. `mod-transmog`'s world SQL inserts creature 190010, whose NAME on
    the row is `Warpweaver` and whose subname is `Transmogrifier`; the
    worldserver was started AFTER the apply, so if the row reached the database
    the running server holds it in memory and its own `lookup creature` will
    name it. The NPC's behaviour still needs the compile — the tab says so —
    but its existence does not.

    The first press of this stage asked for `Transmogrifier`, which is the
    manifest's `npcs[].name` for this entry, and the running server answered
    `No creatures found!` in 3.7 s. That is the SUBNAME on the row and AC's
    `lookup creature` matches the name column. The channel was working
    throughout; the refusal belonged to the question.
    """
    app, view = build()
    view._tabs.setCurrentIndex(tab_index(view, "Console"))
    view.command_edit.setText("lookup creature Warpweaver")
    view.send_button.click()
    # More than one line: the first is the panel's echo of the command, which
    # is there the instant the button is released and says nothing about a
    # reply.
    pump(
        app,
        lambda: len(view.console_log.text().splitlines()) > 1,
        180,
        "console command reply",
    )
    text = view.console_log.text()
    print("--- what the Console tab shows ---", flush=True)
    print(text[-1500:], flush=True)
    print("--- end ---", flush=True)
    shoot(view, "9-lookup-190010.png", tab="Console")
    say(f"190010 in the database by hand = {query(COUNTS['acore_world.creature_template#190010'])}")
    ok = "190010" in text
    say(f"VERDICT lookup: {'PASS' if ok else 'FAIL (the channel may not be set up on this box)'}")
    return 0 if ok else 1


def stage_cleanup() -> int:
    """Put the modules folder back with the app's own Remove, and say what stays behind.

    Not tidiness: `allowed_modules()` is read off this folder, and so is the
    next BUILD. Two modules that were cloned for a gate would otherwise be
    compiled into the worldserver by whoever runs the next rebuild, which
    nobody asked for and which takes hours. The clones and their confs go; the
    rows they wrote do NOT — a module's SQL is ledgered in `updates` by hash and
    there is no honest way to un-apply it here. The checkpoint
    `8.7a-before-module-sql-2026-09-08` is what undoes those.
    """
    app, view = build()
    for item in (MODULE, SECOND):
        select(view, item)
        view.remove_module_button.click()
        pump(app, finished(view), 900, f"remove {item}")
        report(view)
    shoot(view, "11-removed.png")
    say(f"modules on disk now: {modules_on_disk()}")
    say(f"docker.allowed_modules() now = {docker.allowed_modules(SERVER_DIR)!r}")
    say("what the etc/modules folder holds now:")
    for entry in sorted((SERVER_DIR / "env/dist/etc/modules").iterdir()):
        print(f"    {entry.name}", flush=True)
    show_counts("rows the removal deliberately leaves behind", counts())
    say("containers alive: " + (", ".join(alive()) or "(none)"))
    ok = modules_on_disk() == ["mod-playerbots"]
    say(f"VERDICT cleanup: {'PASS' if ok else 'FAIL'}")
    return 0 if ok else 1


STAGES = {
    "ground": stage_ground,
    "behind": stage_behind,
    "behindpin": stage_behindpin,
    "install": stage_install,
    "refuse": stage_refuse,
    "stop": stage_stop,
    "apply": stage_apply,
    "again": stage_again,
    "second": stage_second,
    "start": stage_start,
    "words": stage_words,
    "lookup": stage_lookup,
    "cleanup": stage_cleanup,
}


if __name__ == "__main__":
    if len(sys.argv) != 2 or sys.argv[1] not in STAGES:
        print(__doc__)
        raise SystemExit(2)
    code = STAGES[sys.argv[1]]()
    # The app's own close path, which waits the job runner's threads out. A
    # driver that just exits gets `QThread: Destroyed while thread is still
    # running` and aborts the process with a core dump — which is the driver
    # crashing, not the app, and it must not be left in a transcript looking
    # like the app's verdict.
    app = QApplication.instance()
    for widget in QApplication.topLevelWidgets():
        widget.close()
    if app is not None:
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            app.processEvents()
            time.sleep(0.05)
    raise SystemExit(code)
