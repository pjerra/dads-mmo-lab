"""The 8.1c live gate, run on m910q against the Vanilla install at ~/vanilla-75b.

Same shape as 8.1a's runner and deliberately not the same file: every fact it
touches is this tree's own — `characters.characters`, `realmd.account`, a bot
marker with no registry, and a generated database password that lives in a file
under the server dir rather than being a constant.

Usage:  python gate81c.py <stage>   where stage is a|b|c|d2|e|f|watch
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate81c" / "pylauncher"))

from yulon import dashboard, dbreads, docker, logsnap, platform  # noqa: E402
from yulon.apply import DockerSql  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "vanilla-75b"
ENTRY = load_catalog().get("wow-vanilla")
SPEC = ENTRY.container_spec()
PASSWORD = (SERVER_DIR / (ENTRY.install.password.file or "")).read_text(encoding="utf-8").strip()


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def sql_seam() -> DockerSql:
    return DockerSql(
        SPEC.db,
        PASSWORD,
        schemas=ENTRY.schema_map(),
        client=ENTRY.install.native.db.client if ENTRY.install.native else None,
    )


def hand_query(sql_text: str) -> str:
    """The same question, by hand, with THIS tree's client binary and password."""
    client = "mariadb"
    argv = [
        "docker", "exec", "-i", SPEC.db,
        client, "-uroot", f"-p{PASSWORD}", "--batch", "--skip-column-names", "-e", sql_text,
    ]
    proc = subprocess.run(argv, capture_output=True, text=True, timeout=120)
    return proc.stdout.strip() or proc.stderr.strip()


def services() -> ControllerServices:
    return ControllerServices.for_entry(ENTRY, SERVER_DIR)


def stage_a() -> None:
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

    chars = f"{ENTRY.databases.characters}.{ENTRY.observability.characters.table}"
    players = hand_query(f"SELECT COUNT(*) FROM {chars} WHERE online = 1 AND NOT ({clause});")
    bots = hand_query(f"SELECT COUNT(*) FROM {chars} WHERE online = 1 AND ({clause});")
    print(f"[{stamp()}] by hand         : players={players} bots={bots}")
    print(f"[{stamp()}] app             : players={verdict.players} bots={verdict.bots}")


def stage_b() -> None:
    """This tree HAS the conf file, so the live value is read from it."""
    live = SERVER_DIR / ENTRY.observability.bots.prefix_conf_file
    print(f"[{stamp()}] live conf: {live} (exists={live.exists()})")
    for line in live.read_text(encoding="utf-8", errors="replace").splitlines():
        if line.startswith(ENTRY.observability.bots.prefix_conf_key):
            print(f"[{stamp()}] the live line   : {line}")
    print(f"[{stamp()}] resolved        : {dbreads.resolve_marker(ENTRY, SERVER_DIR)}")

    key = ENTRY.observability.bots.prefix_conf_key
    for name, value in (("blank", ""), ("matches-nothing", "NOSUCHBOTPREFIX")):
        root = Path("/tmp") / f"gate81c-{name}"
        shutil.rmtree(root, ignore_errors=True)
        target = root / ENTRY.observability.bots.prefix_conf_file
        target.parent.mkdir(parents=True)
        kept = [
            ln
            for ln in live.read_text(encoding="utf-8", errors="replace").splitlines()
            if not ln.startswith(key)
        ]
        target.write_text("\n".join(kept) + f"\n{key} = {value}\n", encoding="utf-8")
        answer = dbreads.resolve_marker(ENTRY, root)
        print(f"[{stamp()}] {name:16} -> marker={answer.marker} problem={answer.problem!r}")
        if answer.marker is not None:
            print(f"[{stamp()}] {name:16} -> {dbreads.population(sql_seam(), ENTRY, answer.marker)}")


def stage_c() -> None:
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    app = QApplication.instance() or QApplication([])
    view = ControllerView(ENTRY, services(), status_poll_ms=5000, job_runner=run_inline)
    print(f"[{stamp()}] at construction, with nothing else called:")
    print(f"[{stamp()}]   status label  : {view.status_label.text()!r}")
    print(f"[{stamp()}]   verdict label : {view.verdict_label.text()!r}")
    print(f"[{stamp()}]   verdict shown : {view.verdict_label.isVisibleTo(view)}")
    del app


def stage_d2() -> None:
    """A real crash loop on this core: the database taken away under the world."""
    watch = dashboard.Dashboard(SPEC, ENTRY, SERVER_DIR, sql=sql_seam())
    print(f"[{stamp()}] poll 0 (before)  : {dashboard.line(watch.tick())}")
    subprocess.run(["docker", "stop", "-t", "60", SPEC.db], capture_output=True, timeout=120)
    print(f"[{stamp()}] stopped the database")
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
    subprocess.run(["docker", "stop", "-t", "60", SPEC.world], capture_output=True, timeout=120)
    subprocess.run(["docker", "start", SPEC.db], capture_output=True, timeout=60)
    print(f"[{stamp()}] world stopped and the database started again")


def stage_e() -> None:
    """The stop, its snapshot, and THIS core's own ready and halt lines in it."""
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
    print(f"[{stamp()}] console tail inside it: {console_tail.strip() in saved}")
    # This core's own wording, recorded rather than inherited from WotLK's.
    markers = ENTRY.install.native.ready if ENTRY.install.native else None
    for label, text in (("ready", markers.world if markers else ""), ("halt", "Halting process")):
        hits = [ln for ln in saved.splitlines() if text and text in ln]
        print(f"[{stamp()}] {label:5} marker {text!r}: {len(hits)} line(s)")
        for ln in hits[-2:]:
            print(f"    {ln}")


def stage_f() -> None:
    from yulon.controller import Controller  # noqa: PLC0415

    blocked = Path("/tmp/gate81c-unwritable")
    blocked.write_text("I am a file, not a directory\n", encoding="utf-8")
    recorder = logsnap.Recorder(SPEC, SERVER_DIR, game=ENTRY.id, logs_dir=blocked)
    controller = Controller(SPEC, SERVER_DIR, pre_stop=recorder)
    print(f"[{stamp()}] before          : {docker.container_state(SPEC.world).status!r}")
    stopped = controller.stop()
    print(f"[{stamp()}] snapshot        : {recorder.last}")
    print(f"[{stamp()}] stop() -> {stopped}")
    print(f"[{stamp()}] after           : {docker.container_state(SPEC.world).status!r}")


def stage_watch() -> None:
    watch = dashboard.Dashboard(SPEC, ENTRY, SERVER_DIR, sql=sql_seam())
    last = None
    print(f"[{stamp()}] watching; every line below is a CHANGE in what the tab would show")
    for _ in range(900):
        line = dashboard.line(watch.tick())
        if line != last:
            print(f"[{stamp()}] {line}", flush=True)
            last = line
        time.sleep(3)


if __name__ == "__main__":
    {
        "a": stage_a, "b": stage_b, "c": stage_c, "d2": stage_d2,
        "e": stage_e, "f": stage_f, "watch": stage_watch,
    }[sys.argv[1]]()
