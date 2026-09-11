"""8.7a's direct-SQL guard, pressed through the app's OWN wiring (yulon-ubuntu2, 2026-09-09).

T2's press (`pyplan/gates/8.7a-direct-sql-yulon-ubuntu2-2026-09-09/`) proved the
guard against a real running worldserver and then said what it could not prove:

    "That any user is protected. The seam is not wired in the shipped app.
     `press.py` attaches `Applier._world_running` itself."

and that the refusal's own instruction -- *Press Stop, then install again* --
ended in `container 3c922e8e... is not running`, because the app's Stop takes
the database down with the world.

This driver is the same press with that one line REMOVED. Nothing is attached to
the applier: `ControllerServices.for_entry()` builds it, and if the seam is not
there the guard returns at its first line and this gate photographs a live world
being written to. `assert_the_seams_are_the_apps_own()` reads the two private
fields to PROVE they are populated and then never writes them.

Every step reads its GROUND first and prints it, because a step whose assertion
is already true before its action proves nothing. The row counts and
`CHECKSUM TABLE` for all six tables `arac.sql` writes are the clause; the two
UPDATE-only predicates are counted as "rows this file has not changed yet",
because two of the six cannot move by row count at all.

Steps are selected by argv, each with its own capture. Logging is at DEBUG on
stdout so `_docker()`'s own argv lines and `start_database()`'s own INFO line
appear in the transcript rather than being described by this file.
"""

from __future__ import annotations

import json
import logging
import os
import subprocess
import sys
import threading
import time
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

REPO = Path(os.environ.get("GATE_REPO", "/home/pk/t7"))
sys.path.insert(0, str(REPO / "pylauncher"))

SERVER = Path(os.environ.get("GATE_SERVER", "/home/pk/wowserver"))
OUT = Path(os.environ.get("GATE_OUT", "/home/pk/t7-gate-out"))
OUT.mkdir(parents=True, exist_ok=True)

MODULE_ID = "mod-arac"
WORLD = "ac-worldserver"
DB = "ac-database"
AUTH = "ac-authserver"

TABLES = (
    "playercreateinfo",
    "playercreateinfo_action",
    "playercreateinfo_skills",
    "playercreateinfo_spell_custom",
    "player_totem_model",
    "quest_template",
)

SKILLS_LEFT = (
    "SELECT COUNT(*) FROM playercreateinfo_skills "
    "WHERE skill IN (45, 46, 160, 173, 226) AND classMask <> 0 AND raceMask <> 0"
)
QUESTS_LEFT = (
    "SELECT COUNT(*) FROM quest_template t JOIN quest_template_addon a ON a.id = t.id "
    "WHERE a.allowableclasses <> 0 AND t.AllowableRaces <> 0 AND t.AllowableRaces <> 1791"
)
QUESTS_AT_1791 = "SELECT COUNT(*) FROM quest_template WHERE AllowableRaces = 1791"
TOTEMS_ARAC_RACES = "SELECT COUNT(*) FROM player_totem_model WHERE RaceID IN (1,4,5,7,10)"


def say(text: str = "") -> None:
    print(text, flush=True)


def stamp(text: str) -> None:
    say(f"[{time.strftime('%H:%M:%SZ', time.gmtime())}] {text}")


def sh(argv: list[str], timeout: float = 180.0) -> subprocess.CompletedProcess[str]:
    return subprocess.run(argv, capture_output=True, text=True, errors="replace", timeout=timeout)


def state_of(container: str) -> str:
    proc = sh(
        [
            "docker",
            "inspect",
            "-f",
            "{{.State.Status}} pid={{.State.Pid}} restarts={{.RestartCount}} "
            "started={{.State.StartedAt}}",
            container,
        ],
        timeout=60,
    )
    return (proc.stdout or proc.stderr).strip()


def world_state() -> str:
    return state_of(WORLD)


def boxes() -> str:
    proc = sh(["docker", "ps", "--format", "{{.Names}} {{.Status}}"], timeout=60)
    return ", ".join(sorted(line.strip() for line in proc.stdout.splitlines() if line.strip()))


def world_log(lines: int = 3) -> None:
    """The world's OWN last words at this capture, not this driver's summary."""
    proc = sh(["docker", "logs", "--tail", str(lines), WORLD], timeout=90)
    text = (proc.stdout + proc.stderr).strip().splitlines()
    say(f"  {WORLD} last {lines} log line(s):")
    for line in text[-lines:] or ["(none)"]:
        say(f"    | {line.strip()}")


def capture(label: str) -> None:
    say(f"--- state at {label} ---")
    say(f"  containers up: {boxes()}")
    say(f"  world:    {world_state()}")
    say(f"  database: {state_of(DB)}")
    say(f"  auth:     {state_of(AUTH)}")
    world_log()


def services():
    from yulon.catalog.catalog import load_catalog
    from yulon.ui.controller_view import ControllerServices

    entry = load_catalog().get("wow-wotlk")
    return entry, ControllerServices.for_entry(entry, SERVER)


def the_apps_own_applier():
    """The Modules tab's applier, exactly as `for_entry()` built it. Nothing attached.

    The two private fields are READ to prove they arrived and are never written.
    T2 had to write `_world_running` here; that assignment is the one line this
    gate exists to delete.
    """
    entry, svc = services()
    applier = svc.applier
    assert applier is not None, "the WotLK entry has no applier, so there is nothing to press"
    say("--- the seams on the applier the app built (read, never assigned) ---")
    say(f"  yulon package under test: {__import__('yulon').__file__}")
    say(f"  applier._world_running   = {applier._world_running!r}")
    say(f"  applier._start_database  = {applier._start_database!r}")
    assert applier._world_running is not None, (
        "the shipped applier carries no world_running seam -- this is T2's finding, unfixed"
    )
    assert applier._start_database is not None, (
        "the shipped applier carries no start_database seam -- Stop-then-install cannot work"
    )
    say(f"  applier._world_running() answers {applier._world_running()!r} right now")
    return entry, svc, applier


def manifest_of(svc):
    assert svc.store is not None
    return next(m for m in svc.store.load_all("module") if m.id == MODULE_ID)


def sql_of(applier):
    runner = applier.sql
    assert runner is not None, "the applier has no SQL runner, so the guard would not even fire"
    return runner


def counts(applier) -> dict[str, str]:
    runner = sql_of(applier)

    def one(statement: str) -> str:
        from yulon.apply import ApplyError

        try:
            return runner.query("world", statement).strip().splitlines()[0].strip()
        except ApplyError as exc:
            return f"UNREADABLE ({exc})"

    reading: dict[str, str] = {}
    for table in TABLES:
        reading[f"rows {table}"] = one(f"SELECT COUNT(*) FROM {table}")
    for table in TABLES:
        reading[f"checksum {table}"] = one(f"CHECKSUM TABLE {table}").split("\t")[-1]
    reading["skills_still_to_change"] = one(SKILLS_LEFT)
    reading["quests_still_to_change"] = one(QUESTS_LEFT)
    reading["quests_at_1791"] = one(QUESTS_AT_1791)
    reading["totem_rows_for_arac_races"] = one(TOTEMS_ARAC_RACES)
    return reading


def print_counts(label: str, reading: dict[str, str]) -> None:
    say(f"--- counts: {label} ---")
    for key, value in reading.items():
        say(f"  {key:42} {value}")


def save_counts(name: str, reading: dict[str, str]) -> None:
    (OUT / name).write_text(json.dumps(reading, indent=2) + "\n", encoding="utf-8")
    say(f"saved {OUT / name}")


def load_counts(name: str) -> dict[str, str]:
    return json.loads((OUT / name).read_text(encoding="utf-8"))


def diff_counts(before: dict[str, str], after: dict[str, str]) -> list[str]:
    return [
        f"{key}: {before[key]} -> {after[key]}"
        for key in before
        if str(before[key]) != str(after.get(key))
    ]


def module_on_disk() -> dict[str, object]:
    clone = SERVER / "modules" / MODULE_ID
    return {
        "modules/mod-arac is a dir": clone.is_dir(),
        "files in it": sorted(p.name for p in clone.iterdir()) if clone.is_dir() else [],
        "marker .arac_sql_applied": (clone / ".arac_sql_applied").exists(),
        "mod_arac conf under env/dist/etc/modules": sorted(
            str(p.relative_to(SERVER)) for p in (SERVER / "env/dist/etc/modules").glob("*arac*")
        )
        if (SERVER / "env/dist/etc/modules").is_dir()
        else "(no modules conf dir)",
        "modules/ on disk": sorted(p.name for p in (SERVER / "modules").glob("*"))
        if (SERVER / "modules").is_dir()
        else "(no modules dir)",
    }


def print_module_state(label: str) -> None:
    say(f"--- {MODULE_ID} on disk: {label} ---")
    for key, value in module_on_disk().items():
        say(f"  {key:44} {value!r}")


def report_lines(result, save_as: str = "") -> None:
    if save_as:
        from yulon.ui.controller_view import _format_report

        (OUT / save_as).write_text(_format_report(result), encoding="utf-8")
        say(f"saved the tab's own rendering to {OUT / save_as}")
    say(f"ApplyReport action={result.action} item={result.item_id}")
    say("done:")
    for line in result.done:
        say(f"  {line}")
    say("skipped:")
    for line in result.skipped:
        say(f"  {line}")
    say("pending_sql:")
    for pending in result.pending_sql:
        say(f"  db={pending.db} path={pending.path} files={pending.files}")
    say(
        f"rebuild_required={result.rebuild_required}  "
        f"restart_recommended={result.restart_recommended}"
    )


# ----------------------------------------------------------------- 0: the seams


def step_seams() -> None:
    """What the shipped wiring binds, and what each binding answers right now.

    The one thing T2 could not show. `world_running` is read three ways -- the
    real container, a container that does not exist, and a daemon that cannot be
    reached -- because the guard's `None` branch had never been seen live and
    `container_state()` produces it for both of the last two.
    """
    from yulon import docker

    _entry, _svc, _applier = the_apps_own_applier()
    say("=== the three-valued seam, read for real ===")
    for name in (WORLD, "no-such-container-8f2a"):
        state = docker.container_state(name)
        say(f"  container_state({name!r}).status = {state.status!r}")
        say(f"  docker.world_running({name!r})    = {docker.world_running(name)!r}")
    say("  and a daemon that cannot be reached (DOCKER_HOST at a dead port):")
    keep = os.environ.get("DOCKER_HOST")
    os.environ["DOCKER_HOST"] = "tcp://127.0.0.1:1"
    try:
        state = docker.container_state(WORLD)
        say(f"    container_state({WORLD!r}).status = {state.status!r}")
        say(f"    container_state(...).settled       = {state.settled!r}   <- FAIL-OPEN if used")
        say(f"    docker.world_running({WORLD!r})   = {docker.world_running(WORLD)!r}")
    finally:
        if keep is None:
            os.environ.pop("DOCKER_HOST", None)
        else:
            os.environ["DOCKER_HOST"] = keep
    say(f"  DOCKER_HOST restored to {os.environ.get('DOCKER_HOST')!r}")
    capture("the end of the seam reading")


# ----------------------------------------------------------------- 1: the ground


def step_tables() -> None:
    """Which tables `arac.sql` writes, derived from the file, not from prose."""
    _entry, svc, applier = the_apps_own_applier()
    manifest = manifest_of(svc)
    clone = applier.clone_dir(manifest)
    say(f"manifest {manifest.id}: sql steps = {[(s.path, s.db, s.applied_by) for s in manifest.sql]}")
    say(f"clone dir the file would live in: {clone}")
    path = clone / "data/sql/db-world/arac.sql"
    say(f"{path} exists now: {path.is_file()}")
    if path.is_file():
        text = path.read_text(encoding="utf-8", errors="replace")
        for keyword in ("INSERT", "UPDATE", "DELETE", "REPLACE"):
            for line in text.splitlines():
                if line.strip().upper().startswith(keyword):
                    say(f"  {keyword}: {line.strip()[:120]}")
    say(f"the six tables this gate reads: {TABLES}")


def step_ground() -> None:
    say("=== THE GROUND, read before anything is written ===")
    _entry, svc, applier = the_apps_own_applier()
    capture("the ground")
    print_module_state("the ground")
    say(f"controller.status() = {svc.controller.status()}")
    reading = counts(applier)
    print_counts("GROUND", reading)
    save_counts("counts-ground.json", reading)


def step_dump() -> None:
    """A restorable copy of the six tables, taken fresh before anything writes."""
    from yulon.apply import mysql_client, mysql_env
    from yulon.runner import child_env, creationflags

    _entry, _svc, applier = the_apps_own_applier()
    runner = sql_of(applier)
    tool = mysql_client(DB, "mysqldump", client=runner.client)
    argv = [
        "docker",
        "exec",
        "-i",
        "-e",
        "MYSQL_PWD",  # the value comes from OUR env, never from argv
        DB,
        tool,
        "-uroot",
        "--single-transaction",
        "--skip-lock-tables",
        "--no-tablespaces",
        "acore_world",
        *TABLES,
    ]
    say("=== a FRESH restorable copy of the six tables, before anything writes ===")
    say(f"argv (password is in the environment, not here): {argv}")
    target = OUT / "world-six-tables-before.sql"
    proc = subprocess.run(
        argv,
        capture_output=True,
        text=True,
        errors="replace",
        env=child_env(mysql_env(runner.root_password, runner.wsl_distro)),
        creationflags=creationflags(),
        timeout=900,
    )
    if proc.returncode != 0:
        say(f"FAILED rc={proc.returncode}: {proc.stderr.strip()[:2000]}")
        raise SystemExit(1)
    target.write_text(proc.stdout, encoding="utf-8")
    say(f"wrote {target}  bytes={target.stat().st_size}")
    say(f"CREATE TABLE lines in it: {proc.stdout.count('CREATE TABLE')}")
    capture("the end of the dump")


# ------------------------------------------------ 2: the refusal, world UP


def step_refuse() -> None:
    from yulon.apply import ApplyError

    say("=== PRESS 1: install mod-arac through the app's own applier, WORLD UP ===")
    _entry, svc, applier = the_apps_own_applier()
    manifest = manifest_of(svc)
    capture("the ground of press 1")
    print_module_state("before press 1")
    before = counts(applier)
    print_counts("before press 1", before)
    drift = diff_counts(load_counts("counts-ground.json"), before)
    say(f"drift from the ground reading: {drift or 'none'}")
    if applier._world_running() is not True:
        say("GROUND FAILS: the world is not up, so a refusal would prove nothing. Stopping.")
        raise SystemExit(1)
    stamp("--- the press ---")
    outcome = "NO REFUSAL: install() RETURNED"
    try:
        result = applier.install(manifest)
        report_lines(result)
    except ApplyError as exc:
        outcome = "REFUSED"
        say(f"ApplyError: {exc}")
        (OUT / "refusal-sentence.txt").write_text(str(exc) + "\n", encoding="utf-8")
        say(f"names the step: {'sql data/sql/db-world/arac.sql → world' in str(exc)}")
        say(f"says no rows were written: {'no rows were written' in str(exc)}")
    stamp(f"OUTCOME: {outcome}")
    print_module_state("after press 1")
    after = counts(applier)
    print_counts("after press 1", after)
    save_counts("counts-after-refusal.json", after)
    changed = diff_counts(before, after)
    say(f"COUNTS CHANGED BY THE REFUSED PRESS: {changed or 'nothing -- identical, every reading'}")
    capture("the end of press 1")


# --------------------------------------------------- 3: Stop, through the app


def step_stop() -> None:
    say("=== STOP, through the app's own control (Controller.stop) ===")
    _entry, svc, applier = the_apps_own_applier()
    capture("before the stop")
    say(f"before: controller.status() = {svc.controller.status()}")
    stamp("--- svc.controller.stop() ---")
    started = time.time()
    stopped = svc.controller.stop()
    stamp(f"stop() returned {stopped!r} in {time.time() - started:.1f}s")
    capture("after the stop")
    say(f"after: controller.status() = {svc.controller.status()}")
    say(f"after: the applier's own seam answers {applier._world_running()!r}")
    say("NOTE: `compose stop` KEEPS the containers -- they are `exited`, not removed.")


# ------------------------------- 4: the race -- Start pressed during the wait


def step_race() -> None:
    """The second reading, firing: the Server tab's Start, pressed mid-install.

    The window is real and this is its measurement. `start_database()` waits for
    the database to report healthy (up to `_DB_HEALTHY_TIMEOUT_SECONDS = 180.0`;
    6.6 s measured on this box by T2), and the Server tab's Start is a button the
    same user can press inside it. A world started there is holding these tables
    when the writes land.

    The racing press is the app's OWN Start (`Controller.start()`), on a thread,
    fired after a short delay so it lands inside the health wait rather than
    before the install begins.
    """
    from yulon.apply import ApplyError

    say("=== PRESS 2: install with the world DOWN, and Start pressed during the wait ===")
    _entry, svc, applier = the_apps_own_applier()
    manifest = manifest_of(svc)
    capture("the ground of press 2")
    before = counts(applier)
    print_counts("before press 2", before)
    if applier._world_running() is not False:
        say("GROUND FAILS: the world must be DOWN for this press. Stopping.")
        raise SystemExit(1)
    say(f"ground: the seam answers {applier._world_running()!r} (False -- the guard permits)")

    delay = float(os.environ.get("GATE_RACE_DELAY", "2.0"))
    started_at: list[str] = []

    def press_start() -> None:
        time.sleep(delay)
        started_at.append(time.strftime("%H:%M:%SZ", time.gmtime()))
        say(f"[{started_at[-1]}] >>> RACING PRESS: svc.controller.start() (the Server tab's Start)")
        try:
            answer = svc.controller.start()
            say(f">>> controller.start() returned {answer!r}")
        except Exception as exc:  # noqa: BLE001 - recorded, never allowed to kill the press
            say(f">>> controller.start() raised {type(exc).__name__}: {exc}")

    racer = threading.Thread(target=press_start, name="the-server-tabs-start", daemon=True)
    stamp(f"--- the press (Start will be pressed {delay:.1f}s in) ---")
    racer.start()
    outcome = "NO REFUSAL: install() RETURNED"
    try:
        result = applier.install(manifest)
        report_lines(result)
    except ApplyError as exc:
        outcome = "REFUSED"
        say(f"ApplyError: {exc}")
        (OUT / "race-refusal-sentence.txt").write_text(str(exc) + "\n", encoding="utf-8")
    stamp(f"OUTCOME: {outcome}")
    racer.join(timeout=600)
    say(f"the racing Start was pressed at {started_at or '(never)'}")
    capture("the end of press 2")
    after = counts(applier)
    print_counts("after press 2", after)
    save_counts("counts-after-race.json", after)
    changed = diff_counts(before, after)
    say(f"COUNTS CHANGED BY THE RACED PRESS: {changed or 'nothing -- identical, every reading'}")


# ------------------------------- 5: the success, world stopped, database alone


def step_press_stopped() -> None:
    """Stop, then install again -- the sentence the refusal ends with, followed.

    The whole sequence in one capture: the seam answers `False`, the database is
    down with the world, `start_database()` runs `compose up -d --no-deps` and
    waits for health, the SQL runs, and the report says the database was started
    and the world was left stopped. The world container is `exited` at every
    capture in this step, printed rather than asserted in prose.
    """
    from yulon.apply import ApplyError

    say("=== PRESS 3: the same step again, WORLD STOPPED, DATABASE DOWN ===")
    _entry, svc, applier = the_apps_own_applier()
    manifest = manifest_of(svc)
    capture("the ground of press 3")
    say(f"ground: controller.status() = {svc.controller.status()}")
    print_module_state("before press 3")
    if applier._world_running() is not False:
        say("GROUND FAILS: the world must be DOWN for this press. Stopping.")
        raise SystemExit(1)
    say(f"ground: the seam answers {applier._world_running()!r}")
    say(f"ground: the database is {state_of(DB)}")
    before = counts(applier)
    print_counts("before press 3 (read through the applier's own runner)", before)
    stamp("--- the press ---")
    started = time.time()
    outcome = "REFUSED"
    try:
        result = applier.install(manifest)
        outcome = "APPLIED"
        stamp(f"install() returned after {time.time() - started:.1f}s")
        report_lines(result, save_as="success-report.txt")
        sentence = "started the database alone; the world server was left stopped"
        say(f"the report carries {sentence!r}: {sentence in result.done}")
    except ApplyError as exc:
        say(f"ApplyError: {exc}")
        (OUT / "press3-failure-sentence.txt").write_text(str(exc) + "\n", encoding="utf-8")
    stamp(f"OUTCOME: {outcome}")
    capture("the end of press 3 -- the world must still be exited")
    print_module_state("after press 3")
    after = counts(applier)
    print_counts("after press 3", after)
    save_counts("counts-after-success.json", after)
    say("WHAT MOVED:")
    for line in diff_counts(before, after) or ["  nothing"]:
        say(f"  {line}")


# ---------------------------------------------------------------- 6: the remove


def step_remove() -> None:
    from yulon.apply import ApplyError

    say("=== REMOVE mod-arac through the app's applier ===")
    _entry, svc, applier = the_apps_own_applier()
    manifest = manifest_of(svc)
    capture("the ground of the remove")
    print_module_state("before remove")
    before = counts(applier)
    print_counts("before remove", before)
    stamp("--- applier.remove(manifest) ---")
    try:
        result = applier.remove(manifest)
        report_lines(result, save_as="remove-report.txt")
    except ApplyError as exc:
        say(f"ApplyError: {exc}")
    print_module_state("after remove")
    after = counts(applier)
    print_counts("after remove", after)
    save_counts("counts-after-remove.json", after)
    say("STILL DIFFERENT FROM THE GROUND AFTER THE REMOVE:")
    for line in diff_counts(load_counts("counts-ground.json"), after) or ["  nothing"]:
        say(f"  {line}")
    capture("the end of the remove")


def step_restore() -> None:
    """Put the six tables back to the ground, from the dump taken before press 1."""
    from yulon.apply import mysql_client, mysql_env
    from yulon.runner import child_env, creationflags

    _entry, _svc, applier = the_apps_own_applier()
    runner = sql_of(applier)
    say("=== RESTORE the six tables from the pre-press dump ===")
    capture("before the restore -- the world must NOT be running")
    if applier._world_running() is not False:
        say("REFUSING: the world is up, and this write is exactly what the guard forbids.")
        raise SystemExit(1)
    before = counts(applier)
    print_counts("before restore", before)
    source = OUT / "world-six-tables-before.sql"
    tool = mysql_client(DB, "mysql", client=runner.client)
    argv = ["docker", "exec", "-i", "-e", "MYSQL_PWD", DB, tool, "-uroot", "acore_world"]
    say(f"argv: {argv}   from {source} ({source.stat().st_size} bytes)")
    with source.open("rb") as handle:
        proc = subprocess.run(
            argv,
            stdin=handle,
            capture_output=True,
            text=True,
            errors="replace",
            env=child_env(mysql_env(runner.root_password, runner.wsl_distro)),
            creationflags=creationflags(),
            timeout=1800,
        )
    say(f"rc={proc.returncode} stderr={proc.stderr.strip()[:800]!r}")
    after = counts(applier)
    print_counts("after restore", after)
    save_counts("counts-after-restore.json", after)
    say("DIFFERENT FROM THE GROUND AFTER THE RESTORE:")
    for line in diff_counts(load_counts("counts-ground.json"), after) or [
        "  nothing -- every count and every checksum is the ground reading"
    ]:
        say(f"  {line}")
    capture("the end of the restore")


# -------------------------------------------------------- 7: put the box back


def step_start() -> None:
    say("=== START, through the app's own control (Controller.start) ===")
    _entry, svc, applier = the_apps_own_applier()
    capture("before the start")
    stamp("--- svc.controller.start() ---")
    started = time.time()
    answer = svc.controller.start()
    stamp(f"start() returned {answer!r} in {time.time() - started:.1f}s")
    capture("after the start")
    say(f"after: controller.status() = {svc.controller.status()}")
    say(f"after: the applier's own seam answers {applier._world_running()!r}")


def step_ready() -> None:
    from yulon import docker
    from yulon.docker import azerothcore_ready

    _entry, svc, _applier = the_apps_own_applier()
    host = os.environ.get("GATE_REALM_ADDRESS", "100.99.204.5")
    port = int(os.environ.get("GATE_REALM_PORT", "8085"))
    say(f"=== readiness, asked against the realm's own address {host}:{port} ===")
    started = time.time()
    ready = docker.wait_ready_for(svc.controller.spec, azerothcore_ready(host, port))
    stamp(f"ready = {ready!r} after {time.time() - started:.1f}s")
    capture("after the readiness wait")


def step_final() -> None:
    """The box as it is handed back: realm row, the owner's things, the conf line."""
    _entry, svc, applier = the_apps_own_applier()
    runner = sql_of(applier)
    say("=== the realm row (read; only written if a column is wrong) ===")
    columns = "SELECT id, name, address, localAddress, localSubnetMask, port FROM realmlist"
    say(runner.query("auth", columns).strip())
    address = os.environ.get("GATE_REALM_ADDRESS", "100.99.204.5")
    row = runner.query("auth", columns).strip().splitlines()
    fields = row[-1].split("\t") if row else []
    wrong = not (
        len(fields) >= 5
        and fields[2] == address
        and fields[3] == address
        and fields[4] == "255.255.255.0"
    )
    say(f"needs a write: {wrong}")
    if wrong and os.environ.get("GATE_FIX_REALM") == "1":
        runner.run_statement(
            "auth",
            f"UPDATE realmlist SET address = '{address}', localAddress = '{address}', "
            f"localSubnetMask = '255.255.255.0' WHERE id = 1",
        )
        say(runner.query("auth", columns).strip())
        proc = sh(["docker", "restart", AUTH], timeout=300)
        say(f"docker restart {AUTH}: rc={proc.returncode}")
    say("=== the accounts on this box (ids and names only) ===")
    say(
        runner.query(
            "auth",
            "SELECT COUNT(*) AS accounts_total, MIN(id) AS lowest, MAX(id) AS highest FROM account",
        ).strip()
    )
    say(
        runner.query(
            "auth",
            "SELECT id, username, last_login FROM account "
            "WHERE id > 100 ORDER BY id",
        ).strip()
    )
    say("=== orphaned account_access rows (must be none) ===")
    say(
        runner.query(
            "auth",
            "SELECT COUNT(*) FROM account_access a "
            "LEFT JOIN account c ON c.id = a.id WHERE c.id IS NULL",
        ).strip()
    )
    say("=== the owner's things (read only) ===")
    for path in ("/home/pk/LootPet.lua", "/home/pk/LootPet2.lua"):
        proc = sh(["stat", "-c", "%n %s bytes mtime=%y", path], timeout=60)
        say(f"  {(proc.stdout or proc.stderr).strip()}")
    say("  account PERZI (no password read, written or reset -- lengths only):")
    say(
        runner.query(
            "auth",
            "SELECT id, username, last_login, joindate, LENGTH(salt), LENGTH(verifier) "
            "FROM account WHERE username = 'PERZI'",
        ).strip()
    )
    say("  character Pakka:")
    say(
        runner.query(
            "characters",
            "SELECT guid, name, level, race, class, online FROM characters WHERE name = 'Pakka'",
        ).strip()
    )
    conf = SERVER / "env/dist/etc/worldserver.conf"
    proc = sh(["stat", "-c", "%n mtime=%y", str(conf)], timeout=60)
    say(f"=== {conf} ===")
    say(f"  {(proc.stdout or proc.stderr).strip()}")
    proc = sh(["grep", "-n", "Logger.ALE", str(conf)], timeout=60)
    say(f"  Logger.ALE: {(proc.stdout or proc.stderr).strip()!r}")
    print_module_state("the box as handed back")
    capture("the box as handed back")


# ------------------------------------------------------------------ the shots


def step_table() -> None:
    names = [
        ("counts-ground.json", "ground"),
        ("counts-after-refusal.json", "after refusal"),
        ("counts-after-race.json", "after the race"),
        ("counts-after-success.json", "after success"),
        ("counts-after-remove.json", "after remove"),
        ("counts-after-restore.json", "after restore"),
    ]
    have = [(load_counts(name), label) for name, label in names if (OUT / name).is_file()]
    width = max(len(key) for key in have[0][0])
    head = "| " + "reading".ljust(width) + " | "
    head += " | ".join(f"{label:>13}" for _, label in have) + " |"
    rule = "| " + "-" * width + " | " + " | ".join("-" * 13 for _ in have) + " |"
    lines = [head, rule]
    for key in have[0][0]:
        row = "| " + key.ljust(width) + " | "
        row += " | ".join(f"{str(reading.get(key, '?')):>13}" for reading, _ in have) + " |"
        lines.append(row)
    text = "\n".join(lines)
    (OUT / "counts-table.txt").write_text(text + "\n", encoding="utf-8")
    say(text)
    capture("the counts table")


def step_shot() -> None:
    """One reading, rendered by the app's own Modules tab, offscreen."""
    from PySide6.QtWidgets import QApplication

    from yulon.ui.controller_view import ControllerView

    app = QApplication.instance() or QApplication([])
    entry, svc, _applier = the_apps_own_applier()
    view = ControllerView(entry, svc, status_poll_ms=0)
    view.resize(1100, 800)
    for index in range(view._tabs.count()):
        if view._tabs.tabText(index) == "Modules":
            view._tabs.setCurrentIndex(index)
            break
    kind = os.environ.get("GATE_SHOT_KIND", "text")
    text = (OUT / os.environ["GATE_SHOT_SOURCE"]).read_text(encoding="utf-8", errors="replace")
    if kind == "failed":
        view._module_pending = f"install {MODULE_ID}"
        view._module_failed(text.strip())
    else:
        view.module_report.setPlainText(text)
    app.processEvents()
    path = OUT / os.environ["GATE_SHOT_NAME"]
    view.grab().save(str(path))
    say(f"SHOT {path}  kind={kind}  {WORLD} at capture: {world_state()}")


STEPS = {
    "seams": step_seams,
    "tables": step_tables,
    "ground": step_ground,
    "dump": step_dump,
    "refuse": step_refuse,
    "stop": step_stop,
    "race": step_race,
    "press-stopped": step_press_stopped,
    "remove": step_remove,
    "restore": step_restore,
    "start": step_start,
    "ready": step_ready,
    "final": step_final,
    "table": step_table,
    "shot": step_shot,
}


def main() -> int:
    from yulon.log import use_utf8_streams

    use_utf8_streams()
    # DEBUG on the app's OWN logger, with no handler added here: `yulon.log`
    # already installs one, and a second copy of every line makes a transcript
    # that reads like two runs. The level is what matters -- `_docker()` puts its
    # argv at DEBUG, which is how the compose command in the capture below is the
    # app's own and not this file's description of it.
    logging.getLogger("yulon").setLevel(logging.DEBUG)
    if len(sys.argv) < 2 or sys.argv[1] not in STEPS:
        say(f"usage: t7_press.py <{'|'.join(STEPS)}>")
        return 2
    STEPS[sys.argv[1]]()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
