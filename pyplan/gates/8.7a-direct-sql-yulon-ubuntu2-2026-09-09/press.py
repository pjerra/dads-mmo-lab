"""8.7a's direct-SQL guard, pressed against a live world (yulon-ubuntu2, 2026-09-09).

`apply.py::_refuse_direct_sql_into_a_running_world` landed on 2026-09-08 with
320 lines of unit tests and no live press at all: no shipped caller wires its
`world_running` seam, so the guard has never faced a running worldserver. This
driver is that press, on `manifests/wow-wotlk/modules/mod-arac.json`.

That manifest is NOT the only shipped one with a direct world-SQL step, which a
glob over all four games' manifests said and the ticket's prose did not:
`applied_by` DEFAULTS to `"direct"` (`manifest.py:136`), so 43 steps across 18
manifests target a `WORLD_HELD_DBS` schema directly. `mod-arac` is the only
`module`-type one, and the only one whose SQL is a file inside a cloned module.

Everything goes through the app's own objects. `ControllerServices.for_entry()`
builds the Modules tab's applier, `Controller.stop()`/`.start()` are the Server
tab's two buttons, and the row counts are read through the SAME `DockerSql` the
applier writes with -- so a count that could not be read and a write that could
not happen fail the same way.

THE ONE THING THIS ADDS to what the app does today is the seam itself:
`Applier._world_running`, which `apply.py:1920` says no shipped caller passes.
It is wired here to the install's real Docker state (`world_running()` below),
never to a constant -- a lambda returning True would prove this file, not the
app.

Every step reads its GROUND first and prints it, because a step whose assertion
is already true before its action proves nothing. For this press that danger is
concrete: two of `arac.sql`'s six tables are UPDATEd rather than inserted into,
so their row COUNT cannot move at all, and if the ground already satisfied
those predicates the success clause would photograph as a pass having written
nothing. Both predicates are counted before and after (`skills_still_to_change`,
`quests_still_to_change`), beside `COUNT(*)` and `CHECKSUM TABLE` for all six.

Steps are selected by argv, each with its own capture, so a long one is never
left half-way by an ssh timeout.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

REPO = Path(os.environ.get("GATE_REPO", "/home/pk/dads-mmo-lab"))
sys.path.insert(0, str(REPO / "pylauncher"))

SERVER = Path(os.environ.get("GATE_SERVER", "/home/pk/wowserver"))
OUT = Path(os.environ.get("GATE_OUT", "/home/pk/arac-gate-out"))
OUT.mkdir(parents=True, exist_ok=True)

MODULE_ID = "mod-arac"
WORLD = "ac-worldserver"
DB = "ac-database"
AUTH = "ac-authserver"

# The six tables `data/sql/db-world/arac.sql` WRITES, read out of the file
# itself rather than trusted from prose (`tables` step prints the derivation).
# `quest_template_addon` is joined for a predicate and never written, so it is
# not here -- but its rows are what decide how many `quest_template` rows move.
TABLES = (
    "playercreateinfo",
    "playercreateinfo_action",
    "playercreateinfo_skills",
    "playercreateinfo_spell_custom",
    "player_totem_model",
    "quest_template",
)

# The two UPDATE predicates, as the SQL spells them. A row still matching one of
# these is a row the file has NOT yet changed, so ground > 0 and after == 0 is
# the only shape in which those two tables prove anything.
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
    return subprocess.run(argv, capture_output=True, text=True, timeout=timeout)


def state_of(container: str) -> str:
    """Alive, its pid and its restart count -- printed at every capture.

    A server that has DIED photographs exactly like a refusal, so liveness is a
    printed fact beside each reading rather than an assumption.
    """
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


def services():
    from yulon.catalog.catalog import load_catalog
    from yulon.ui.controller_view import ControllerServices

    entry = load_catalog().get("wow-wotlk")
    return entry, ControllerServices.for_entry(entry, SERVER)


def world_running() -> bool | None:
    """Is THIS install's worldserver up? The seam the guard asks, wired for real.

    The tree's one existing `world_running` wiring is the Modules/My Party group's
    (`controller_view.py`: `docker.container_state(spec.world).settled`), and this
    is that reading with the one difference this guard's three-valued contract
    needs: `container_state()` answers an empty `ContainerState` when Docker
    would not say, and `.settled` turns that into `False` -- which through THIS
    seam is fail-OPEN, "not running", the one answer that lets the SQL through.
    So an unreadable inspect is `None` here ("could not ask"), which the guard
    refuses on. Recorded in the README as a note for whoever wires this for real.
    """
    from yulon import docker

    state = docker.container_state(WORLD)
    if not state.status:
        return None
    return state.settled


def applier_with_the_seam():
    """The Modules tab's own applier, with `world_running` attached.

    `ControllerServices.for_entry()` builds it exactly as the tab does --
    `wotlk_modules.applier(server_dir, sql=sql, client_dir=client_dir)`, with
    `client_dir=None` and no `DbcCopier`, so this manifest's MPQ and DBC steps
    report as skipped and nothing outside the database and `modules/` is touched.
    The seam is then assigned, because no shipped caller passes it yet
    (`apply.py:1920`) and building a second `Applier` here would press an object
    the app does not have.
    """
    entry, svc = services()
    applier = svc.applier
    assert applier is not None, "the WotLK entry has no applier, so there is nothing to press"
    applier._world_running = world_running
    return entry, svc, applier


def manifest_of(svc):
    assert svc.store is not None
    return next(m for m in svc.store.load_all("module") if m.id == MODULE_ID)


def sql_of(applier):
    runner = applier.sql
    assert runner is not None, "the applier has no SQL runner, so the guard would not even fire"
    return runner


def counts(applier) -> dict[str, str]:
    """Every reading the clause rests on, through the applier's own SQL runner.

    Row counts for all six tables `arac.sql` writes, `CHECKSUM TABLE` beside
    each of them (an UPDATE moves no count but always moves the checksum), and
    the two UPDATE predicates counted as "rows this file has not changed yet".
    """
    runner = sql_of(applier)

    def one(statement: str) -> str:
        """One reading, or why there is none.

        An unreadable count is RECORDED rather than raised, because on this box
        it is a measurement in its own right: the app's Stop takes the database
        down with the servers, so after following the guard's own instruction
        there is nothing left to read the rows from -- and a driver that died
        here would have skipped the press that proves it.
        """
        from yulon.apply import ApplyError

        try:
            return runner.query("world", statement).strip().splitlines()[0].strip()
        except ApplyError as exc:
            return f"UNREADABLE ({exc})"

    reading: dict[str, str] = {}
    for table in TABLES:
        reading[f"rows {table}"] = one(f"SELECT COUNT(*) FROM {table}")
    for table in TABLES:
        line = one(f"CHECKSUM TABLE {table}")
        reading[f"checksum {table}"] = line.split("\t")[-1]
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
        "any mod_arac conf under env/dist/etc/modules": sorted(
            str(p.relative_to(SERVER))
            for p in (SERVER / "env/dist/etc/modules").glob("*arac*")
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
    """The report's fields, and -- for the screenshots -- the tab's own wording.

    `_format_report()` is what the Modules tab puts in its report box on a
    success, so saving its output is the only way a screenshot of a successful
    press shows the app's sentence rather than this driver's transcription of it.
    """
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


# --------------------------------------------------------------- 0: the tables


def step_tables() -> None:
    """Which tables `arac.sql` writes, derived from the file, not from prose."""
    import re

    path = Path(os.environ.get("GATE_ARAC_SQL", "/tmp/arac-read/data/sql/db-world/arac.sql"))
    say(f"=== the SQL this press is about: {path} ===")
    text = path.read_text(encoding="utf-8", errors="replace")
    say(f"lines: {len(text.splitlines())}")
    pattern = re.compile(
        r"\b(INSERT\s+IGNORE\s+INTO|INSERT\s+INTO|REPLACE\s+INTO|DELETE\s+FROM|UPDATE|"
        r"TRUNCATE\s+TABLE|TRUNCATE|ALTER\s+TABLE|CREATE\s+TABLE|DROP\s+TABLE)\s+`?(\w+)`?",
        re.I,
    )
    found: dict[str, set[str]] = {}
    for verb, table in pattern.findall(text):
        found.setdefault(table, set()).add(" ".join(verb.upper().split()))
    for table in sorted(found):
        say(f"  {table:34} {sorted(found[table])}")
    say(f"this file's TABLES constant: {list(TABLES)}")
    say(f"agrees with the file: {sorted(found) == sorted(TABLES)}")


# ----------------------------------------------------------------- 1: ground


def step_ground() -> None:
    say("=== GROUND, read before anything is installed or written ===")
    stamp(f"server_dir={SERVER}  out={OUT}")
    say(f"containers up: {boxes()}")
    say(f"world:    {world_state()}")
    say(f"database: {state_of(DB)}")
    say(f"auth:     {state_of(AUTH)}")
    print_module_state("ground")
    entry, svc, applier = applier_with_the_seam()
    manifest = manifest_of(svc)
    say(f"manifest: {manifest.id} {manifest.name!r}")
    say(f"  sql steps: {[(s.db, s.path, s.applied_by, s.when) for s in manifest.sql]}")
    say(f"  build.rebuild={manifest.build.rebuild}")
    say(f"  server_dbc={[s.src for s in manifest.server_dbc]}  client={[s.src for s in manifest.client]}")
    say(f"applier: sql={type(applier.sql).__name__} client_dir={applier.client_dir} dbc={applier.dbc}")
    say(f"the seam, asked now: world_running() = {world_running()!r}")
    reading = counts(applier)
    print_counts("GROUND", reading)
    save_counts("counts-ground.json", reading)
    say(f"world at the end of the ground reading: {world_state()}")


def step_dump() -> None:
    """A restorable copy of the six tables, so the owner's box goes back exactly.

    `remove()` deletes the clone and keeps the rows ("DB rows are kept", its own
    docstring), so the applier CANNOT undo this SQL -- this dump is how step 4's
    "back to the ground" is made true rather than reported as a deviation.
    """
    from yulon.apply import mysql_client, mysql_env
    from yulon.runner import child_env, creationflags

    _entry, _svc, applier = applier_with_the_seam()
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
    say("=== a restorable copy of the six tables, before anything writes ===")
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
    say(f"wrote {target}  bytes={target.stat().st_size}  stderr={proc.stderr.strip()[:400]!r}")
    say(f"CREATE TABLE lines in it: {proc.stdout.count('CREATE TABLE')}")
    say(f"world: {world_state()}")


# --------------------------------------------- 2: the refusal, with world UP


def step_refuse() -> None:
    from yulon.apply import ApplyError

    say("=== PRESS 1: install mod-arac through the app's applier, WORLD UP ===")
    entry, svc, applier = applier_with_the_seam()
    manifest = manifest_of(svc)
    say(f"ground: containers up = {boxes()}")
    say(f"ground: world = {world_state()}")
    say(f"ground: the seam says world_running() = {world_running()!r}")
    print_module_state("before press 1")
    before = counts(applier)
    print_counts("before press 1", before)
    ground = load_counts("counts-ground.json")
    drift = diff_counts(ground, before)
    say(f"drift from the ground reading: {drift or 'none'}")
    if world_running() is not True:
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
    say(f"world: {world_state()}")


# ------------------------------------------------ 3: stop, then press again


def step_stop() -> None:
    say("=== STOP, through the app's own control (Controller.stop) ===")
    _entry, svc, _applier = applier_with_the_seam()
    say(f"ground: containers up = {boxes()}")
    say(f"ground: status() = {svc.controller.status()}")
    say(f"ground: the seam says world_running() = {world_running()!r}")
    stamp("--- svc.controller.stop() ---")
    started = time.time()
    stopped = svc.controller.stop()
    stamp(f"stop() returned {stopped!r} in {time.time() - started:.1f}s")
    say(f"after: containers up = {boxes()}")
    say(f"after: world = {world_state()}")
    say(f"after: database = {state_of(DB)}")
    say(f"after: status() = {svc.controller.status()}")
    say(f"after: the seam says world_running() = {world_running()!r}")


def step_press_stopped() -> None:
    from yulon.apply import ApplyError

    say("=== PRESS 2: the same step, WORLD STOPPED ===")
    _entry, svc, applier = applier_with_the_seam()
    manifest = manifest_of(svc)
    say(f"ground: containers up = {boxes()}")
    say(f"ground: world = {world_state()}")
    say(f"ground: database = {state_of(DB)}")
    say(f"ground: the seam says world_running() = {world_running()!r}")
    if world_running() is not False:
        say("GROUND FAILS: the world is not stopped, so this step would repeat press 1. Stopping.")
        raise SystemExit(1)
    print_module_state("before press 2")
    before = counts(applier)
    print_counts("before press 2", before)
    stamp("--- the press ---")
    outcome = "FAILED"
    try:
        result = applier.install(manifest)
        report_lines(result, save_as=os.environ.get("GATE_REPORT_FILE", ""))
        outcome = "APPLIED"
    except ApplyError as exc:
        say(f"ApplyError: {exc}")
        (OUT / "press2-failure-sentence.txt").write_text(str(exc) + "\n", encoding="utf-8")
    stamp(f"OUTCOME: {outcome}")
    print_module_state("after press 2")
    after = counts(applier)
    print_counts("after press 2", after)
    save_counts("counts-after-success.json", after)
    changed = diff_counts(before, after)
    say("COUNTS CHANGED BY THIS PRESS:")
    for line in changed or ["  nothing"]:
        say(f"  {line}")
    say(f"world: {world_state()}   database: {state_of(DB)}")


def step_db_up() -> None:
    """`docker.start_database()` -- the app's own "the database alone" primitive.

    Only run if press 2 could not reach the database. Its own comment says why it
    exists: *"Started rather than demanded, because Stop takes the database down
    with everything else -- a user who followed the repair refusals would
    otherwise have no way back to a state that action accepts."*
    """
    from yulon import docker

    _entry, svc, _applier = applier_with_the_seam()
    say("=== the database alone, through docker.start_database() ===")
    say(f"ground: containers up = {boxes()}")
    say(f"ground: database = {state_of(DB)}")
    started = time.time()
    docker.start_database(svc.controller.spec, SERVER, because="the press could not be made")
    stamp(f"start_database() returned in {time.time() - started:.1f}s")
    say(f"after: containers up = {boxes()}")
    say(f"after: database = {state_of(DB)}")
    say(f"after: world = {world_state()}")
    say(f"after: the seam says world_running() = {world_running()!r}")


# ------------------------------------------------------------- 4: start again


def step_start() -> None:
    say("=== START, through the app's own control (Controller.start) ===")
    entry, svc, _applier = applier_with_the_seam()
    say(f"ground: containers up = {boxes()}")
    say(f"ground: status() = {svc.controller.status()}")
    stamp("--- svc.controller.start() ---")
    started = time.time()
    svc.controller.start()
    stamp(f"start() returned in {time.time() - started:.1f}s")
    say(f"after: containers up = {boxes()}")
    say(f"after: world = {world_state()}")
    # The realm's OWN address, not a loopback. `azerothcore_ready()` matches
    # `<host>:<port>` in the AUTH log, and this realm advertises 100.99.204.5 --
    # so the run of this step at 08:08:24Z, which was handed `127.0.0.1`, could
    # never have matched whatever the server did, and its quiet budget never
    # expired because a 500-bot world prints constantly. `10-start.log` is that
    # run, with no readiness line; `step_ready()` asked the question properly.
    host = os.environ.get("GATE_REALM_HOST", "100.99.204.5")
    ready = svc.controller.wait_ready(host, 8085, timeout=float(os.environ.get("GATE_READY", 900)))
    stamp(f"wait_ready({host}, 8085) = {ready!r}")
    say(f"after: world = {world_state()}")
    say(f"after: status() = {svc.controller.status()}")
    say(f"after: the seam says world_running() = {world_running()!r}")


def step_ready() -> None:
    """The readiness wait alone, pointed at the realm's OWN address.

    Split out from `start` because the first `start` press was handed
    `127.0.0.1` and the auth server never prints that: `azerothcore_ready()`
    matches `<realm_host>:<realm_port>` in the AUTH log, and this realm is
    `100.99.204.5:8085`, so the wait could not have succeeded whatever the
    server did. Re-pressing Start to fix that would have run `compose up -d`
    over a world that is already up and serving the owner's bots, so the wait
    is asked on its own instead.
    """
    from yulon import docker

    _entry, svc, _applier = applier_with_the_seam()
    host = os.environ.get("GATE_REALM_HOST", "100.99.204.5")
    say("=== the app's own readiness wait, against the realm this box serves ===")
    say(f"ground: containers up = {boxes()}")
    say(f"ground: world = {world_state()}")
    say(f"ground: auth  = {state_of(AUTH)}")
    ready_spec = docker.azerothcore_ready(host, 8085, timeout=float(os.environ.get("GATE_READY", 600)))
    stamp(f"--- wait_ready_for(spec, azerothcore_ready({host!r}, 8085)) ---")
    started = time.time()
    ready = docker.wait_ready_for(svc.controller.spec, ready_spec)
    stamp(f"ready = {ready!r} after {time.time() - started:.1f}s")
    say(f"after: world = {world_state()}")
    say(f"after: auth  = {state_of(AUTH)}")
    say(f"after: status() = {svc.controller.status()}")


# ----------------------------------------------------------------- 5: remove


def step_remove() -> None:
    from yulon.apply import ApplyError

    say("=== REMOVE mod-arac through the app's applier ===")
    _entry, svc, applier = applier_with_the_seam()
    manifest = manifest_of(svc)
    say(f"ground: containers up = {boxes()}")
    say(f"ground: world = {world_state()}")
    say(f"ground: the seam says world_running() = {world_running()!r}")
    print_module_state("before remove")
    before = counts(applier)
    print_counts("before remove", before)
    stamp("--- applier.remove(manifest) ---")
    try:
        result = applier.remove(manifest)
        report_lines(result, save_as=os.environ.get("GATE_REPORT_FILE", ""))
    except ApplyError as exc:
        say(f"ApplyError: {exc}")
    print_module_state("after remove")
    after = counts(applier)
    print_counts("after remove", after)
    save_counts("counts-after-remove.json", after)
    ground = load_counts("counts-ground.json")
    back = diff_counts(ground, after)
    say("STILL DIFFERENT FROM THE GROUND AFTER THE REMOVE:")
    for line in back or ["  nothing"]:
        say(f"  {line}")
    say(f"world: {world_state()}")


def step_restore() -> None:
    """Put the six tables back to the ground, from the dump taken before press 1.

    `remove()` keeps the rows by design, so this is the only way the box goes
    back to what the owner left. Restored with the world DOWN, for the same
    reason the guard exists: a running worldserver holds these tables.
    """
    from yulon.apply import mysql_client, mysql_env
    from yulon.runner import child_env, creationflags

    _entry, _svc, applier = applier_with_the_seam()
    runner = sql_of(applier)
    say("=== RESTORE the six tables from the pre-press dump ===")
    say(f"ground: world = {world_state()}  (must NOT be running)")
    if world_running() is not False:
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
    ground = load_counts("counts-ground.json")
    left = diff_counts(ground, after)
    say("DIFFERENT FROM THE GROUND AFTER THE RESTORE:")
    for line in left or ["  nothing -- every count and every checksum is the ground reading"]:
        say(f"  {line}")


# -------------------------------------------------------- 6: put the box back


def step_realm() -> None:
    """The realm row the box is left with, and the authserver restart it needs."""
    _entry, _svc, applier = applier_with_the_seam()
    runner = sql_of(applier)
    address = os.environ.get("GATE_REALM_ADDRESS", "100.99.204.5")
    say("=== the realm row, and the authserver restart that makes it live ===")
    say("before:")
    say(runner.query("auth", "SELECT id, name, address, localAddress, localSubnetMask, port FROM realmlist").strip())
    runner.run_statement(
        "auth",
        f"UPDATE realmlist SET address = '{address}', localAddress = '{address}', "
        f"localSubnetMask = '255.255.255.0' WHERE id = 1",
    )
    say("after:")
    say(runner.query("auth", "SELECT id, name, address, localAddress, localSubnetMask, port FROM realmlist").strip())
    proc = sh(["docker", "restart", AUTH], timeout=300)
    say(f"docker restart {AUTH}: rc={proc.returncode} {proc.stdout.strip()!r} {proc.stderr.strip()!r}")
    say(f"auth: {state_of(AUTH)}")
    say(f"world: {world_state()}")
    say(f"containers up: {boxes()}")


def step_owner() -> None:
    """The owner's things, listed and read-only, so the report can say untouched."""
    _entry, _svc, applier = applier_with_the_seam()
    runner = sql_of(applier)
    say("=== the owner's things on this box (read only) ===")
    for path in ("/home/pk/LootPet.lua",):
        proc = sh(["stat", "-c", "%n %s bytes mtime=%y", path], timeout=60)
        say(f"  {(proc.stdout or proc.stderr).strip()}")
    say("  account PERZI (id, username, last_login, joindate, and the LENGTH of its verifier):")
    say(
        "    "
        + runner.query(
            "auth",
            "SELECT id, username, last_login, joindate, LENGTH(verifier), LENGTH(salt) "
            "FROM account WHERE username = 'PERZI'",
        ).strip()
    )
    say("  characters on that account:")
    say(
        "    "
        + runner.query(
            "characters",
            "SELECT c.guid, c.name, c.level, c.race, c.class, c.online, c.logout_time "
            "FROM characters c JOIN acore_auth.account a ON a.id = c.account "
            "WHERE a.username = 'PERZI'",
        ).strip().replace("\n", "\n    ")
    )
    say(f"world: {world_state()}   containers up: {boxes()}")


# ------------------------------------------------------------------ the shots


def step_table() -> None:
    """The readings side by side -- the clause of this whole gate, in one file.

    Built from the saved JSON rather than retyped, so the README and the
    photograph on the VM's desktop cannot disagree with what the database said.
    """
    names = [
        ("counts-ground.json", "ground"),
        ("counts-after-refusal.json", "after refusal"),
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
    say("")
    say(f"world: {world_state()}")
    say(f"containers up: {boxes()}")


def step_shot() -> None:
    """One reading, rendered by the app's own Modules tab, offscreen.

    `GATE_SHOT_KIND=failed` renders it the way `_module_failed` does (which is
    what a user sees when the guard refuses); `report` renders a saved
    `ApplyReport` through `_format_report`; `text` puts a log on the tab.
    """
    from PySide6.QtWidgets import QApplication

    from yulon.ui.controller_view import ControllerView

    app = QApplication.instance() or QApplication([])
    entry, svc, _applier = applier_with_the_seam()
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
    "tables": step_tables,
    "ground": step_ground,
    "dump": step_dump,
    "refuse": step_refuse,
    "stop": step_stop,
    "press-stopped": step_press_stopped,
    "db-up": step_db_up,
    "start": step_start,
    "ready": step_ready,
    "remove": step_remove,
    "restore": step_restore,
    "realm": step_realm,
    "owner": step_owner,
    "table": step_table,
    "shot": step_shot,
}


def main() -> int:
    from yulon.log import configure, use_utf8_streams

    use_utf8_streams()
    configure()
    if len(sys.argv) < 2 or sys.argv[1] not in STEPS:
        say(f"usage: press.py <{'|'.join(STEPS)}>")
        return 2
    STEPS[sys.argv[1]]()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
