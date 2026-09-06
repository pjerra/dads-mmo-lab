"""The 8.1a live gate, run on yulon-ubuntu against the finished 7.2 install.

Every stage prints what it asked and what it got, so the transcript is the
evidence rather than a summary of it. Read-only stages run first; the two that
disturb the server (a forced kill, a stop) run last and say so.

Usage:  python gate81a.py <stage>   where stage is a|b|c|d|e|f
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate81a" / "pylauncher"))

from yulon import dashboard, dbreads, docker, logsnap, platform  # noqa: E402
from yulon.apply import DockerSql  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "wowserver"
ENTRY = load_catalog().get("wow-wotlk")
SPEC = ENTRY.container_spec()


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def hand_query(sql_text: str) -> str:
    """The same question, asked with docker exec mysql, by hand."""
    argv = [
        "docker", "exec", "-i", SPEC.db,
        "mysql", "-uroot", "-ppassword", "--batch", "--skip-column-names", "-e", sql_text,
    ]
    proc = subprocess.run(argv, capture_output=True, text=True, timeout=120)
    return proc.stdout.strip() or proc.stderr.strip()


def services() -> ControllerServices:
    return ControllerServices.for_entry(ENTRY, SERVER_DIR)


def stage_a() -> None:
    """The counts the app reports, and the same counts run by hand, in the same minute."""
    svc = services()
    print(f"[{stamp()}] container state: {docker.container_state(SPEC.world)}")
    assert svc.dashboard is not None
    verdict = svc.dashboard()
    print(f"[{stamp()}] app verdict     : {verdict}")
    print(f"[{stamp()}] app line        : {dashboard.line(verdict)}")

    answer = dbreads.resolve_marker(ENTRY, SERVER_DIR)
    print(f"[{stamp()}] marker          : {answer}")
    assert answer.marker is not None, answer.problem
    clause = dbreads.bot_clause(ENTRY, answer.marker)
    print(f"[{stamp()}] bot clause      : {clause}")

    players = hand_query(
        f"SELECT COUNT(*) FROM acore_characters.characters WHERE online = 1 AND NOT ({clause});"
    )
    bots = hand_query(
        f"SELECT COUNT(*) FROM acore_characters.characters WHERE online = 1 AND ({clause});"
    )
    print(f"[{stamp()}] by hand         : players={players} bots={bots}")
    print(f"[{stamp()}] app             : players={verdict.players} bots={verdict.bots}")


def stage_b() -> None:
    """The bot marker's three outcomes, against a COPY of the live configuration."""
    live = SERVER_DIR / ENTRY.observability.bots.prefix_conf_file
    if not live.exists():
        # Measured on this box: an ordinary install has no `playerbots.conf`,
        # only the `.dist` the module ships, which the server does not load.
        # So the nearest thing to "a copy of the live configuration" is a copy
        # of that file under the name the server looks for, and the transcript
        # says which was used rather than quietly substituting one.
        source = live.with_name(live.name + ".dist")
        print(f"[{stamp()}] no {live.name} on this install; copying {source.name} instead")
        live = source
    print(f"[{stamp()}] source conf: {live} ({live.stat().st_size} bytes)")
    key = ENTRY.observability.bots.prefix_conf_key
    for name, value in (("blank", ""), ("matches-nothing", "NOSUCHBOTPREFIX")):
        copy_root = Path("/tmp") / f"gate81a-{name}"
        shutil.rmtree(copy_root, ignore_errors=True)
        target = copy_root / ENTRY.observability.bots.prefix_conf_file
        target.parent.mkdir(parents=True)
        text = live.read_text(encoding="utf-8", errors="replace")
        lines = [ln for ln in text.splitlines() if not ln.startswith(key)]
        lines.append(f"{key} = {value}")
        target.write_text("\n".join(lines) + "\n", encoding="utf-8")
        answer = dbreads.resolve_marker(ENTRY, copy_root)
        print(f"[{stamp()}] {name:16} -> marker={answer.marker} problem={answer.problem!r}")
        if answer.marker is not None:
            sql = DockerSql(SPEC.db, "password", schemas=ENTRY.schema_map())
            counts = dbreads.population(sql, ENTRY, answer.marker)
            print(f"[{stamp()}] {name:16} -> {counts}")


def stage_b2() -> None:
    """The warning outcome, which the prefix alone cannot produce on this install.

    Stage B set the prefix to something no account matches and the count still
    came back 500 — because the registry arm found them. That is the two-arm
    clause working as designed, and it means the "matched nothing" state cannot
    be reached here by editing the prefix.

    So this fabricates the shape the OTHER three trees really have — a marker
    with no registry at all — and points it at this live database. The clause,
    the query and the rows are real; the entry's registry is the one thing made
    up, and it is made up to equal what CMaNGOS installs actually carry.
    """
    entry = ENTRY.model_copy(
        update={
            "observability": ENTRY.observability.model_copy(
                update={"bots": ENTRY.observability.bots.model_copy(update={"registry": None})}
            )
        },
        deep=True,
    )
    sql = DockerSql(SPEC.db, "password", schemas=entry.schema_map())
    for prefix in ("rndbot", "NOSUCHBOTPREFIX"):
        marker = dbreads.Marker(prefix, "conf")
        print(f"[{stamp()}] prefix-only clause: {dbreads.bot_clause(entry, marker)}")
        counts = dbreads.population(sql, entry, marker)
        print(f"[{stamp()}] {prefix:16} -> players={counts.players} bots={counts.bots} "
              f"characters={counts.characters}")
        print(f"[{stamp()}] {prefix:16} -> warning={counts.warning!r}")


def stage_c() -> None:
    """The same verdict, read off the tab, offscreen."""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    app = QApplication.instance() or QApplication([])
    # Polling ON, which is what a real tab has: the point is that the first read
    # happens at construction rather than a poll interval later
    # (`bug-checklist.md:552`). Nothing below calls refresh by hand.
    view = ControllerView(ENTRY, services(), status_poll_ms=5000, job_runner=run_inline)
    print(f"[{stamp()}] at construction, with nothing else called:")
    print(f"[{stamp()}]   status label  : {view.status_label.text()!r}")
    print(f"[{stamp()}]   verdict label : {view.verdict_label.text()!r}")
    print(f"[{stamp()}]   verdict shown : {view.verdict_label.isVisibleTo(view)}")
    print(f"[{stamp()}]   start enabled : {view.start_button.isEnabled()}")
    print(f"[{stamp()}]   stop enabled  : {view.stop_button.isEnabled()}")

    off = ControllerView(ENTRY, services(), status_poll_ms=0, job_runner=run_inline)
    print(f"[{stamp()}] with polling off: status={off.status_label.text()!r} "
          f"verdict={off.verdict_label.text()!r}")
    del app


def stage_d() -> None:
    """A forced kill, twice, read through two polls. THIS RESTARTS THE WORLD SERVER."""
    svc = services()
    watch = dashboard.Dashboard(SPEC, ENTRY, SERVER_DIR, sql=DockerSql(SPEC.db, "password", schemas=ENTRY.schema_map()))
    print(f"[{stamp()}] poll 1 (before) : {dashboard.line(watch.tick())}")
    for round_ in (1, 2):
        subprocess.run(["docker", "kill", SPEC.world], capture_output=True, text=True, timeout=60)
        print(f"[{stamp()}] killed the worldserver, round {round_}")
        time.sleep(6)
        verdict = watch.tick()
        print(f"[{stamp()}] poll {round_ + 1}         : state={verdict.state} restarts={verdict.restarts} stable={verdict.stable}")
        print(f"[{stamp()}]                 : {dashboard.line(verdict)}")
    del svc


def stage_d2() -> None:
    """A REAL restart loop, because `docker kill` cannot make one.

    Measured here first, 2026-09-06: `docker kill` sets Docker's own
    "explicitly stopped" flag, so a container with `restart: unless-stopped`
    (which these three carry, `docker-compose.yml:67`, `:212`, `:241`) stays
    exited with RestartCount 0. The checklist's recipe — "a forced kill of the
    world twice in a row" — therefore cannot produce the state it asks for, and
    the dashboard correctly answered `stopped` both times.

    What DOES loop is the world failing on its own, which this box demonstrated
    on its own at boot: RestartCount 6 while the database was still starting.
    So that is what this reproduces deliberately — the database is stopped, the
    world is started, and it crashes and is restarted until the database comes
    back.
    """
    watch = dashboard.Dashboard(
        SPEC, ENTRY, SERVER_DIR, sql=DockerSql(SPEC.db, "password", schemas=ENTRY.schema_map())
    )
    print(f"[{stamp()}] poll 0 (before)  : {dashboard.line(watch.tick())}")
    subprocess.run(["docker", "stop", "-t", "60", SPEC.db], capture_output=True, timeout=120)
    print(f"[{stamp()}] stopped the database, so the world has nothing to connect to")
    subprocess.run(["docker", "start", SPEC.world], capture_output=True, timeout=60)
    print(f"[{stamp()}] started the worldserver against no database")
    for poll in range(1, 7):
        time.sleep(10)
        verdict = watch.tick()
        state = docker.container_state(SPEC.world)
        print(
            f"[{stamp()}] poll {poll}          : state={verdict.state} restarts={verdict.restarts} "
            f"stable={verdict.stable} (docker says {state.status!r}/{state.restart_count})"
        )
        print(f"[{stamp()}]                  : {dashboard.line(verdict)}")
    subprocess.run(["docker", "stop", "-t", "60", SPEC.world], capture_output=True, timeout=120)
    subprocess.run(["docker", "start", SPEC.db], capture_output=True, timeout=60)
    print(f"[{stamp()}] world stopped and the database started again")


def stage_e() -> None:
    """A stop, and the log file it leaves. THIS STOPS THE SERVER."""
    svc = services()
    console_tail = docker.log_tail(SPEC.world, 5)
    print(f"[{stamp()}] the last 5 lines the Console tab was showing:")
    for line in console_tail.splitlines():
        print(f"    {line}")
    stopped = svc.controller.stop()
    print(f"[{stamp()}] stop() -> {stopped}")
    snapshot = svc.log_snapshot.last
    print(f"[{stamp()}] snapshot        : {snapshot}")
    assert snapshot is not None and snapshot.path is not None, snapshot
    saved = snapshot.path.read_text(encoding="utf-8", errors="replace")
    print(f"[{stamp()}] saved file      : {snapshot.path} ({len(saved)} bytes, {len(saved.splitlines())} lines)")
    print(f"[{stamp()}] its last 5 lines:")
    for line in saved.splitlines()[-5:]:
        print(f"    {line}")
    print(f"[{stamp()}] the console tail is inside the saved file: {console_tail.strip() in saved}")
    print(f"[{stamp()}] files in the logs dir: {sorted(p.name for p in (platform.config_dir() / 'logs').glob('*.log'))}")


def stage_f() -> None:
    """A snapshot that cannot write, in front of a stop that must happen anyway."""
    from yulon.controller import Controller  # noqa: PLC0415

    blocked = Path("/tmp/gate81a-unwritable")
    blocked.write_text("I am a file, not a directory\n", encoding="utf-8")
    recorder = logsnap.Recorder(SPEC, SERVER_DIR, game=ENTRY.id, logs_dir=blocked)
    controller = Controller(SPEC, SERVER_DIR, pre_stop=recorder)
    print(f"[{stamp()}] before          : {docker.container_state(SPEC.world).status!r}")
    stopped = controller.stop()
    print(f"[{stamp()}] snapshot        : {recorder.last}")
    print(f"[{stamp()}] stop() -> {stopped}")
    print(f"[{stamp()}] after           : {docker.container_state(SPEC.world).status!r}")


if __name__ == "__main__":
    {"a": stage_a, "b": stage_b, "b2": stage_b2, "d2": stage_d2, "c": stage_c, "d": stage_d, "e": stage_e, "f": stage_f}[
        sys.argv[1]
    ]()
