"""The 8.1d live gate, on m910q against the Tortoise install at ~/tortoise-server.

Same shape as 8.1c's runner and deliberately not the same file: every fact it
touches is this fork's own — `tw_char`/`tw_logon`, a bot marker with no
registry, and this core's own ready and halt wording. The box's own section
forbids inheriting anything from a sibling.

Usage:  python gate81d.py <stage>   where stage is a|b|c|d|e|f|watch
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate81d" / "pylauncher"))

from yulon import dashboard, dbreads, docker, logsnap  # noqa: E402
from yulon.apply import DockerSql  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices  # noqa: E402

SERVER_DIR = Path.home() / "tortoise-server"
ENTRY = load_catalog().get("wow-tortoise")
SPEC = ENTRY.container_spec()
PASSWORD = (
    (SERVER_DIR / (ENTRY.install.password.file or "")).read_text(encoding="utf-8").strip()
    if ENTRY.install.password.file
    else ""
)


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def sql_seam() -> DockerSql:
    return DockerSql(
        SPEC.db,
        PASSWORD,
        schemas=ENTRY.schema_map(),
        client=ENTRY.install.native.db.client if ENTRY.install.native else None,
    )


def hand_query(sql_text: str) -> str:
    """The same question, by hand, with THIS tree's client binary and password."""
    client = ENTRY.install.native.db.client if ENTRY.install.native else "mysql"
    argv = [
        "docker", "exec", "-i", SPEC.db,
        client or "mysql", "-uroot", f"-p{PASSWORD}",
        "--batch", "--skip-column-names", "-e", sql_text,
    ]
    proc = subprocess.run(argv, capture_output=True, text=True, timeout=120)
    return proc.stdout.strip() or proc.stderr.strip()


def services() -> ControllerServices:
    return ControllerServices.for_entry(ENTRY, SERVER_DIR)


def stage_a() -> None:
    """The counts, against the same questions asked by hand."""
    svc = services()
    say(f"container state: {docker.container_state(SPEC.world)}")
    assert svc.dashboard is not None
    verdict = svc.dashboard()
    say(f"app verdict     : {verdict}")
    say(f"app line        : {dashboard.line(verdict)}")

    answer = dbreads.resolve_marker(ENTRY, SERVER_DIR)
    say(f"marker          : {answer}")
    assert answer.marker is not None, answer.problem
    clause = dbreads.bot_clause(ENTRY, answer.marker)
    say(f"bot clause      : {clause}")

    chars = f"{ENTRY.databases.characters}.{ENTRY.observability.characters.table}"
    online = f"SELECT COUNT(*) FROM {chars} WHERE online = 1 AND"

    def by_hand() -> tuple[int, int]:
        return (
            int(hand_query(f"{online} NOT ({clause});")),
            int(hand_query(f"{online} ({clause});")),
        )

    # THREE readings, not two. This server's 500 bots log in over several
    # minutes, so a hand count taken before the app's and one taken after it
    # are two different truths -- 8.1c saw the same drift, 505 to 503, and 8.1b
    # only got equality because its population had settled. What can honestly
    # be asserted while the number is moving is that the app's answer lies
    # between the two hand answers.
    before = by_hand()
    verdict = svc.dashboard()
    after = by_hand()
    say(f"by hand, before : players={before[0]} bots={before[1]}")
    say(f"app             : players={verdict.players} bots={verdict.bots}")
    say(f"by hand, after  : players={after[0]} bots={after[1]}")
    for what, mine, lo, hi in (
        ("players", verdict.players, before[0], after[0]),
        ("bots", verdict.bots, before[1], after[1]),
    ):
        assert min(lo, hi) <= mine <= max(lo, hi), f"{what}: {mine} is not between {lo} and {hi}"
    if before == after:
        say("counts agree, and the population did not move between the readings")
    else:
        say("counts agree: the app's answer lies between two hand readings taken around it")


def stage_b() -> None:
    """The marker, from this install's own conf, and the two ways it can fail."""
    live = SERVER_DIR / ENTRY.observability.bots.prefix_conf_file
    say(f"live conf: {live} (exists={live.exists()})")
    key = ENTRY.observability.bots.prefix_conf_key
    if live.exists():
        for line in live.read_text(encoding="utf-8", errors="replace").splitlines():
            if line.startswith(key):
                say(f"the live line   : {line}")
    say(f"resolved        : {dbreads.resolve_marker(ENTRY, SERVER_DIR)}")

    base = (
        live.read_text(encoding="utf-8", errors="replace").splitlines() if live.exists() else []
    )
    for name, value in (("blank", ""), ("matches-nothing", "NOSUCHBOTPREFIX")):
        root = Path("/tmp") / f"gate81d-{name}"
        shutil.rmtree(root, ignore_errors=True)
        target = root / ENTRY.observability.bots.prefix_conf_file
        target.parent.mkdir(parents=True)
        kept = [ln for ln in base if not ln.startswith(key)]
        target.write_text("\n".join(kept) + f"\n{key} = {value}\n", encoding="utf-8")
        answer = dbreads.resolve_marker(ENTRY, root)
        say(f"{name:16} -> marker={answer.marker} problem={answer.problem!r}")
        if answer.marker is not None:
            say(f"{name:16} -> {dbreads.population(sql_seam(), ENTRY, answer.marker)}")


def stage_c() -> None:
    """The first poll is not five seconds late."""
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication  # noqa: PLC0415

    from yulon.ui.controller_view import ControllerView  # noqa: PLC0415
    from yulon.ui.widgets.job import run_inline  # noqa: PLC0415

    app = QApplication.instance() or QApplication([])
    view = ControllerView(ENTRY, services(), status_poll_ms=5000, job_runner=run_inline)
    say("at construction, with nothing else called:")
    say(f"  status label  : {view.status_label.text()!r}")
    say(f"  verdict label : {view.verdict_label.text()!r}")
    say(f"  verdict shown : {view.verdict_label.isVisibleTo(view)}")
    shots = Path.home() / "gate81d-shots"
    shots.mkdir(parents=True, exist_ok=True)
    view.resize(940, 560)
    view.grab().save(str(shots / "1-server-tab.png"))
    say(f"wrote {shots / '1-server-tab.png'}")
    del app


def stage_d() -> None:
    """A real crash loop on this core: the database taken away under the world."""
    watch = dashboard.Dashboard(SPEC, ENTRY, SERVER_DIR, sql=sql_seam())
    say(f"poll 0 (before)  : {dashboard.line(watch.tick())}")
    subprocess.run(["docker", "stop", "-t", "60", SPEC.db], capture_output=True, timeout=120)
    say("stopped the database")
    subprocess.run(["docker", "start", SPEC.world], capture_output=True, timeout=60)
    say("started the worldserver against no database")
    for poll in range(1, 8):
        time.sleep(10)
        verdict = watch.tick()
        state = docker.container_state(SPEC.world)
        say(
            f"poll {poll}          : state={verdict.state} restarts={verdict.restarts} "
            f"stable={verdict.stable} (docker says {state.status!r}/{state.restart_count})"
        )
    subprocess.run(["docker", "stop", "-t", "60", SPEC.world], capture_output=True, timeout=120)
    subprocess.run(["docker", "start", SPEC.db], capture_output=True, timeout=60)
    say("world stopped and the database started again")


def stage_e() -> None:
    """The stop, its snapshot, and THIS core's own ready and halt lines in it."""
    svc = services()
    console_tail = docker.log_tail(SPEC.world, 5)
    say("the last 5 lines the Console tab was showing:")
    for line in console_tail.splitlines():
        print(f"    {line}")
    stopped = svc.controller.stop()
    say(f"stop() -> {stopped}")
    snapshot = svc.log_snapshot.last
    say(f"snapshot        : {snapshot}")
    assert snapshot is not None and snapshot.path is not None, snapshot
    saved = snapshot.path.read_text(encoding="utf-8", errors="replace")
    say(f"saved file      : {snapshot.path} ({len(saved)} bytes, {len(saved.splitlines())} lines)")
    say(f"console tail inside it: {console_tail.strip() in saved}")
    markers = ENTRY.install.native.ready if ENTRY.install.native else None
    say(f"this core's ready pattern: {markers.world if markers else None!r}")
    for label, text in (("halt", "Halting process"), ("halt2", "Terminating")):
        hits = [ln for ln in saved.splitlines() if text in ln]
        say(f"{label:5} {text!r}: {len(hits)} line(s)")
        for ln in hits[-2:]:
            print(f"    {ln}")


def stage_f() -> None:
    """A snapshot that fails is reported, and the stop still happens."""
    from yulon.controller import Controller  # noqa: PLC0415

    blocked = Path("/tmp/gate81d-unwritable")
    blocked.write_text("I am a file, not a directory\n", encoding="utf-8")
    recorder = logsnap.Recorder(SPEC, SERVER_DIR, game=ENTRY.id, logs_dir=blocked)
    controller = Controller(SPEC, SERVER_DIR, pre_stop=recorder)
    say(f"before          : {docker.container_state(SPEC.world).status!r}")
    stopped = controller.stop()
    say(f"snapshot        : {recorder.last}")
    say(f"stop() -> {stopped}")
    say(f"after           : {docker.container_state(SPEC.world).status!r}")


def stage_watch() -> None:
    watch = dashboard.Dashboard(SPEC, ENTRY, SERVER_DIR, sql=sql_seam())
    last = None
    say("watching; every line below is a CHANGE in what the tab would show")
    for _ in range(900):
        line = dashboard.line(watch.tick())
        if line != last:
            say(line)
            last = line
        time.sleep(3)


if __name__ == "__main__":
    {
        "a": stage_a,
        "b": stage_b,
        "c": stage_c,
        "d": stage_d,
        "e": stage_e,
        "f": stage_f,
        "watch": stage_watch,
    }[sys.argv[1]]()
