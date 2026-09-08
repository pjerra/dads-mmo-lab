"""8.1a's crash-loop clause, re-pressed under the three-strike rule (2026-09-08).

The clause changed on 2026-09-08: `dashboard.LOOP_RESTART_STRIKES` went from 1 to
3, and the definition of done now reads "within two polls of its **third** new
restart". Two directions have to be shown, and only one of them is the loop:

  `loop`   -- stage_d2's own recipe (the database taken away under a running
              world, six polls at ten seconds), recording at which poll the third
              NEW restart arrived and at which poll the verdict said
              `restart_loop`.
  `single` -- the direction the rule exists FOR: one restart followed by a
              healthy run must read `up`. Under the old threshold that same
              reading was `restart loop -- 1 restarts`, which is the false alarm
              photographed in 8.2b's own evidence.

WHERE THIS RAN. Not on 8.1a's own box. `yulon-ubuntu`'s virtual disk lives on the
Hyper-V host's D:, and that drive left the SATA bus at 12:40:41 on 2026-09-08
(host `disk` event 51, then a storm of `Ntfs` 140 "A device which does not
exist was specified"); the guest's ext4 remounted read-only twelve seconds
later. So this press is on `m910q`'s TBC install, which is 8.1b's tree, and it
proves the SHARED code and the two directions -- it does not discharge 8.1a's
own WotLK/Linux line, and nothing here may be copied onto it.

GROUND FIRST. Every stage prints the state it found BEFORE it acts, including
`_strikes` on the watcher, because a step whose assertion is already true at the
start proves nothing: `up` was already the answer before the single restart, so
the thing being asserted is `up` **while the watcher holds exactly one strike**,
which is false at the start of the stage and true at the end of it.

Usage:  python gate81a3s.py ground|single|loop
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

CHECKOUT = Path.home() / "gate81a3s"
sys.path.insert(0, str(CHECKOUT / "pylauncher"))

from PySide6.QtWidgets import QApplication  # noqa: E402

from yulon import dashboard, docker  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices, ControllerView  # noqa: E402

SERVER_DIR = Path.home() / "tbc-7.4c"
ENTRY = load_catalog().get("wow-tbc")
SPEC = ENTRY.container_spec()
SHOTS = Path.home() / "gate81a3s-out"
SHOTS.mkdir(parents=True, exist_ok=True)


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def raw(container: str) -> dict[str, object]:
    """Docker's own words about one container, so the transcript is not a summary."""
    proc = subprocess.run(
        ["docker", "inspect", "-f",
         "{{.State.Status}}|{{.State.Running}}|{{.RestartCount}}|{{.State.StartedAt}}|{{.State.ExitCode}}|{{.State.Pid}}",
         container],
        capture_output=True, text=True, timeout=60,
    )
    out = proc.stdout.strip()
    if not out:
        return {"error": proc.stderr.strip()}
    status, running, restarts, started, exit_code, pid = out.split("|")
    return {
        "status": status, "running": running, "restarts": int(restarts),
        "started_at": started, "exit_code": int(exit_code), "pid": int(pid),
    }


def alive(container: str) -> str:
    """Logged at every capture: a container that has DIED photographs like a refusal."""
    return str(raw(container).get("running", "?"))


# --------------------------------------------------------------- the tab

def open_tab() -> tuple[QApplication, ControllerView, dashboard.Dashboard]:
    """The real Server tab, with its polling OFF so every tick is one this file asked for.

    `status_poll_ms=0` is the view's own "do not poll" setting, not a stub: the
    timer never starts and the constructor's first refresh never runs either, so
    the watcher's strike count is exactly the number of ticks below it.
    """
    app = QApplication.instance() or QApplication([])
    services = ControllerServices.for_entry(ENTRY, SERVER_DIR)
    view = ControllerView(ENTRY, services, status_poll_ms=0)
    view.resize(1100, 780)
    tabs = view._tabs
    idx = [i for i in range(tabs.count()) if tabs.tabText(i) == "Server"][0]
    tabs.setCurrentIndex(idx)
    watcher = services.dashboard.__self__  # type: ignore[union-attr]
    return app, view, watcher


def tab_tick(app: QApplication, view: ControllerView, seconds: float = 60.0) -> None:
    """One refresh of the real tab, pumped until its worker has answered."""
    view.refresh_status()
    view.refresh_verdict()
    end = time.time() + seconds
    while (view._verdict_pending or view._status_pending) and time.time() < end:
        app.processEvents()
        time.sleep(0.02)
    for _ in range(25):
        app.processEvents()
        time.sleep(0.02)


def shoot(app: QApplication, view: ControllerView, name: str, why: str) -> None:
    for _ in range(20):
        app.processEvents()
        time.sleep(0.02)
    path = SHOTS / name
    view.grab().save(str(path))
    say(f"captured {path.name}: {why}")
    say(f"   world container alive at this capture: {alive(SPEC.world)}")
    say(f"   the line on the tab            : {view.verdict_label.text()!r}")
    say(f"   the three words under it       : {view.status_label.text()!r}")


# --------------------------------------------------------------- stages

def stage_ground() -> None:
    """What was here before anything was touched. Read, printed, and nothing else."""
    say(f"LOOP_RESTART_STRIKES as shipped in this checkout: {dashboard.LOOP_RESTART_STRIKES}")
    say(f"SETTLED_AFTER: {dashboard.SETTLED_AFTER}")
    for name in (SPEC.db, SPEC.auth, SPEC.world):
        say(f"{name}: {json.dumps(raw(name))}")
    app, view, watcher = open_tab()
    say(f"watcher before its first tick: _last_restarts={watcher._last_restarts} "
        f"_strikes={watcher._strikes} _looping={watcher._looping}")
    tab_tick(app, view)
    say(f"watcher after one tick       : _last_restarts={watcher._last_restarts} "
        f"_strikes={watcher._strikes} _looping={watcher._looping}")
    shoot(app, view, "0-tab-ground-zero-strikes.png",
          "the tab as found, before anything was done to the server")


def stage_single() -> None:
    """ONE restart, then a healthy run. It must read `up`, and it must hold one strike.

    The kill is sent from the HOST to the container's main process, because a
    signal sent to PID 1 from inside its own namespace is dropped by the kernel
    unless that process installed a handler -- and `docker kill` is worse than
    useless here: it sets Docker's own explicitly-stopped flag, so
    `restart: unless-stopped` never fires (measured on yulon-ubuntu, 2026-09-06,
    and it is why stage_d2 exists at all).
    """
    app, view, watcher = open_tab()

    before = raw(SPEC.world)
    say(f"GROUND, world  : {json.dumps(before)}")
    assert before["running"] == "true", "this stage needs a running world; start the stack first"
    tab_tick(app, view)
    say(f"GROUND, watcher: _last_restarts={watcher._last_restarts} _strikes={watcher._strikes} "
        f"_looping={watcher._looping}")
    say(f"GROUND, the tab says: {view.verdict_label.text()!r}")
    say("GROUND is the reason this stage is not a no-op: `up` is ALREADY the answer here, "
        "with ZERO strikes. What has to become true is `up` while the watcher holds ONE.")
    shoot(app, view, "1-tab-before-the-single-restart.png",
          "before the kill: the world is up and the watcher holds zero strikes")

    readings: list[dict[str, object]] = []
    pid = before["pid"]
    say(f"killing the world's main process on the host: sudo kill -9 {pid}")
    killed = subprocess.run(["sudo", "kill", "-9", str(pid)], capture_output=True, text=True)
    say(f"kill returned {killed.returncode} {killed.stderr.strip()!r}")

    seen_one_strike_at = None
    for poll in range(1, 61):
        time.sleep(5)
        state = raw(SPEC.world)
        tab_tick(app, view)   # the TAB's own tick is the reading; nothing else ticks
        line = view.verdict_label.text()
        readings.append({"poll": poll, "at": stamp(), **state, "strikes": watcher._strikes,
                         "line": line})
        say(f"poll {poll:>2}: docker {state.get('status')!r}/{state.get('restarts')} "
            f"exit={state.get('exit_code')} | strikes={watcher._strikes} | tab {line!r}")
        if (state.get("restarts") == before["restarts"] + 1
                and state.get("status") == "running"
                and watcher._strikes == 1
                and line.startswith("up")):
            seen_one_strike_at = poll
            break

    say("")
    if seen_one_strike_at is None:
        say("NOT REACHED: the world never came back to a healthy `up` with exactly one strike.")
    else:
        say(f"REACHED at poll {seen_one_strike_at}: docker's RestartCount went "
            f"{before['restarts']} -> {before['restarts'] + 1}, the watcher holds exactly ONE "
            f"strike, and the tab says {view.verdict_label.text()!r} -- not a loop.")
        shoot(app, view, "2-tab-one-strike-reads-up.png",
              "ONE restart, a healthy run, one strike on the watcher: the tab says up")

    # Four seconds of `running` is not yet "a healthy run", and the clause says
    # a healthy run. So the watch stays on the SAME watcher -- a fresh one would
    # baseline at the post-restart count and hold no strike at all, which is a
    # different claim -- while the world finishes its boot and its bots come
    # back. The strike must still be there, and the word must still be `up`.
    say("")
    say("--- the run continues, on the same watcher, until it is a run and not a moment ---")
    for settle in range(1, 25):
        time.sleep(15)
        state = raw(SPEC.world)
        tab_tick(app, view)
        line = view.verdict_label.text()
        readings.append({"poll": f"settle{settle}", "at": stamp(), **state,
                         "strikes": watcher._strikes, "line": line})
        say(f"settle {settle:>2}: docker {state.get('status')!r}/{state.get('restarts')} "
            f"| strikes={watcher._strikes} looping={watcher._looping} | tab {line!r}")
        if "bots" in line and ", 0 bots" not in line and settle >= 2:
            break
    shoot(app, view, "2b-tab-one-strike-after-a-real-run.png",
          f"the same watcher, still ONE strike, after the restarted world finished its boot")

    # The same live readings, replayed through a watcher whose threshold is the
    # OLD one. Not a second live press -- a replay, and it is labelled as one --
    # but every state in it was read off this container a moment ago.
    say("")
    say("--- REPLAY of the readings above through the OLD threshold (1), same states ---")
    replay = [docker.ContainerState(
        status=str(r["status"]), restart_count=int(r["restarts"]), started_at=str(r["started_at"]))
        for r in readings]
    baseline = docker.ContainerState(
        status=str(before["status"]), restart_count=int(before["restarts"]),
        started_at=str(before["started_at"]))
    for threshold in (3, 1):
        saved = dashboard.LOOP_RESTART_STRIKES
        dashboard.LOOP_RESTART_STRIKES = threshold
        try:
            feed = iter([baseline, *replay])
            w = dashboard.Dashboard(SPEC, ENTRY, SERVER_DIR, sql=watcher.sql,
                                    state_of=lambda _c: next(feed))
            verdicts = []
            for _ in range(len(replay) + 1):
                try:
                    verdicts.append(w.tick().state)
                except StopIteration:
                    break
            say(f"threshold {threshold}: {verdicts}")
        finally:
            dashboard.LOOP_RESTART_STRIKES = saved

    (SHOTS / "single-readings.json").write_text(json.dumps(readings, indent=1), encoding="utf-8")


def stage_loop() -> None:
    """stage_d2's own recipe, with the three-strike arithmetic written down per poll."""
    app, view, watcher = open_tab()

    before = raw(SPEC.world)
    db_before = raw(SPEC.db)
    say(f"GROUND, world   : {json.dumps(before)}")
    say(f"GROUND, database: {json.dumps(db_before)}")
    tab_tick(app, view)
    say(f"GROUND, watcher : _last_restarts={watcher._last_restarts} _strikes={watcher._strikes} "
        f"_looping={watcher._looping}")
    say(f"GROUND, the tab says: {view.verdict_label.text()!r}")
    say("GROUND is the reason this stage is not a no-op: the tab is NOT saying `restart loop` "
        "here, and the watcher holds zero strikes.")
    shoot(app, view, "3-tab-before-the-crash-loop.png",
          "before the database is taken away: the tab is not saying restart loop")

    say(f"stopping the database ({SPEC.db}), so the world has nothing to connect to")
    subprocess.run(["docker", "stop", "-t", "60", SPEC.db], capture_output=True, timeout=180)
    say(f"database now: {json.dumps(raw(SPEC.db))}")
    world = raw(SPEC.world)
    if world.get("running") == "true":
        say(f"killing the world's main process so the crash cycle starts now: "
            f"sudo kill -9 {world['pid']}")
        subprocess.run(["sudo", "kill", "-9", str(world["pid"])], capture_output=True)
    subprocess.run(["docker", "start", SPEC.world], capture_output=True, timeout=120)

    third_new_restart_at = None
    first_loop_verdict_at = None
    readings: list[dict[str, object]] = []
    shot_three = False
    for poll in range(1, 31):          # six is the recipe; the rest is the extension
        time.sleep(10)
        state = raw(SPEC.world)
        tab_tick(app, view)
        line = view.verdict_label.text()
        new = (int(state["restarts"]) - int(before["restarts"])
               if isinstance(state.get("restarts"), int) else None)
        readings.append({"poll": poll, "at": stamp(), **state, "new_since_ground": new,
                         "strikes": watcher._strikes, "looping": watcher._looping, "line": line})
        say(f"poll {poll:>2}: docker {state.get('status')!r}/{state.get('restarts')} "
            f"(new {new}) exit={state.get('exit_code')} | strikes={watcher._strikes} "
            f"looping={watcher._looping} | tab {line!r}")
        if third_new_restart_at is None and isinstance(new, int) and new >= 3:
            third_new_restart_at = poll
            say(f"   ^ the THIRD new restart is on the board at poll {poll}")
        if first_loop_verdict_at is None and line.startswith("restart loop"):
            first_loop_verdict_at = poll
            say(f"   ^ the tab first said `restart loop` at poll {poll}")
        if not shot_three and watcher._strikes >= 3:
            shoot(app, view, "4-tab-three-strikes-reads-restart-loop.png",
                  f"the watcher holds {watcher._strikes} strikes")
            shot_three = True
        if poll >= 6 and third_new_restart_at is not None and first_loop_verdict_at is not None \
                and shot_three:
            break

    say("")
    say(f"third NEW restart at poll : {third_new_restart_at}")
    say(f"first `restart loop` verdict at poll: {first_loop_verdict_at}")
    if not shot_three:
        shoot(app, view, "4-tab-three-strikes-reads-restart-loop.png",
              f"end of the run; the watcher holds {watcher._strikes} strikes")
    (SHOTS / "loop-readings.json").write_text(json.dumps(readings, indent=1), encoding="utf-8")

    say("putting the box back: the world stopped, the database started")
    subprocess.run(["docker", "stop", "-t", "60", SPEC.world], capture_output=True, timeout=180)
    subprocess.run(["docker", "start", SPEC.db], capture_output=True, timeout=120)
    say(f"world   : {json.dumps(raw(SPEC.world))}")
    say(f"database: {json.dumps(raw(SPEC.db))}")


def stage_strikes() -> None:
    """The threshold crossed one strike at a time, which `loop` cannot show.

    MEASURED FIRST, and it is why this stage exists. `loop` -- stage_d2's own
    recipe -- took the database away and the world was dead in under a second:
    at the first ten-second poll docker's `RestartCount` had already gone from 2
    to 9. SEVEN new restarts inside one poll interval. At that cadence the third
    strike can never be seen ARRIVING on this tree, so the recipe answers the
    definition of done ("within two polls of its third new restart": zero polls
    later, the same poll) without ever exercising the arithmetic the new rule is
    made of.

    So the database is left UP here and the world is killed three times, each
    kill after the previous run has lasted long enough for docker to reset its
    own backoff. The world then comes back HEALTHY every time, `.State.Status`
    reads `running` at the readings that matter, and the strike count is the
    only thing deciding the word. That is the path the false alarm of 8.2b came
    down, and the only path on which one, two and three differ.
    """
    app, view, watcher = open_tab()

    before = raw(SPEC.world)
    say(f"GROUND, world   : {json.dumps(before)}")
    say(f"GROUND, database: {json.dumps(raw(SPEC.db))}")
    assert before["running"] == "true", "this stage needs a running world"
    tab_tick(app, view)
    say(f"GROUND, watcher : _last_restarts={watcher._last_restarts} _strikes={watcher._strikes} "
        f"_looping={watcher._looping}")
    say(f"GROUND, the tab says: {view.verdict_label.text()!r}")
    say("GROUND is the reason this stage is not a no-op: zero strikes, and the tab is saying "
        "`up`. The word has to CHANGE, and only on the third kill.")

    readings: list[dict[str, object]] = []
    settled: dict[int, str] = {}          # strike level -> the line while `running`
    first_loop_at: str | None = None
    for kill in (1, 2, 3):
        state = raw(SPEC.world)
        say("")
        say(f"--- kill {kill}: sudo kill -9 {state['pid']} (the database stays UP) ---")
        subprocess.run(["sudo", "kill", "-9", str(state["pid"])], capture_output=True)
        for probe in range(1, 46):
            time.sleep(4)
            state = raw(SPEC.world)
            tab_tick(app, view)
            line = view.verdict_label.text()
            strikes = watcher._strikes
            readings.append({"kill": kill, "probe": probe, "at": stamp(), **state,
                             "strikes": strikes, "looping": watcher._looping, "line": line})
            say(f"kill {kill} probe {probe:>2}: docker {state.get('status')!r}/"
                f"{state.get('restarts')} exit={state.get('exit_code')} | strikes={strikes} "
                f"looping={watcher._looping} | tab {line!r}")
            if line.startswith("restart loop") and state.get("status") == "running" \
                    and first_loop_at is None:
                first_loop_at = (f"kill {kill} probe {probe}, with the world RUNNING and "
                                 f"{strikes} strikes")
            if state.get("status") == "running" and strikes == kill:
                settled[kill] = line
                # Docker resets its own restart backoff only after the container
                # has stayed up; a kill sent too soon lands in the backoff and
                # the next reading is `restarting`, which decides the word by
                # itself and would hide the strike arithmetic this stage is for.
                if probe * 4 >= 20:
                    break
        if kill == 1:
            shoot(app, view, "5-tab-strike-one-of-three.png",
                  "one new restart on a watcher that was already looking: the tab says up")
        if kill == 3:
            shoot(app, view, "6-tab-strike-three-of-three.png",
                  "the third new restart: the same tab, the same running world, now a loop")

    say("")
    for level in (1, 2, 3):
        say(f"at {level} strike(s), world running, the tab said: {settled.get(level)!r}")
    say(f"the first `restart loop` on a RUNNING world came at: {first_loop_at}")
    (SHOTS / "strikes-readings.json").write_text(json.dumps(readings, indent=1), encoding="utf-8")


if __name__ == "__main__":
    {"ground": stage_ground, "single": stage_single, "loop": stage_loop,
     "strikes": stage_strikes}[sys.argv[1]]()
