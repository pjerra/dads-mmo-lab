"""8.1a's crash-loop clause under the three-strike rule, on its OWN tree at last.

The clause: "a crash-looping world reads as a restart loop within two polls of its THIRD new
restart rather than reading as up". `dashboard.LOOP_RESTART_STRIKES` went 1 -> 3 on 2026-09-08 and
8.1a's line says in as many words that its crash-loop stage is owed a re-press under the new rule.

That re-press ran on 2026-09-08 -- but on `m910q`'s TBC tree, because `yulon-ubuntu`'s disk left the
Hyper-V host's SATA bus that lunchtime. Its README says so and says the WotLK press is still owed:
a per-tree fact is measured per tree, never inherited from a sibling emulator. This is that press,
on `yulon-ubuntu2`, against the AzerothCore WotLK install at /home/pk/wowserver.

TWO STAGES, because the recipe alone cannot show the rule:

  `loop`    -- stage_d2's own recipe, verbatim: the database taken away under a running world, six
               polls at ten seconds. This is what 8.1a's line names, so it is run as written, and
               what it can and cannot observe on THIS tree is recorded rather than assumed.
  `strikes` -- the threshold crossed one strike at a time, the database left UP and the world's own
               process killed three times. On TBC the recipe's cadence could not see the third
               strike arrive (RestartCount went 2 -> 9 inside one ten-second poll, and the container
               read `restarting`, which decides the verdict on its own). Whether WotLK does the same
               is a per-tree fact and is measured here.

GROUND FIRST, everywhere: `up` with zero strikes is already the answer before any of this, so what
has to become true is `up` while the watcher holds exactly ONE strike, and `restart loop` only on
the third. Every capture logs whether the world container was alive at that instant, because a
server that has died photographs exactly like a refusal.

Usage:  python gate81a3s_wotlk.py ground|strikes|loop|restore
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

sys.path.insert(0, str(Path(__file__).resolve().parent / "pylauncher"))

from PySide6.QtWidgets import QApplication  # noqa: E402

from yulon import dashboard, docker  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.ui.controller_view import ControllerServices, ControllerView  # noqa: E402

SERVER_DIR = Path("/home/pk/wowserver")
ENTRY = load_catalog().get("wow-wotlk")
SPEC = ENTRY.container_spec()
OUT = Path(__file__).resolve().parent / "out"
OUT.mkdir(parents=True, exist_ok=True)


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def raw(container: str) -> dict[str, object]:
    proc = subprocess.run(
        ["docker", "inspect", "-f",
         "{{.State.Status}}|{{.State.Running}}|{{.RestartCount}}|{{.State.StartedAt}}|"
         "{{.State.ExitCode}}|{{.State.Pid}}|{{.HostConfig.RestartPolicy.Name}}",
         container],
        capture_output=True, text=True, timeout=60,
    )
    out = proc.stdout.strip()
    if not out:
        return {"error": proc.stderr.strip()}
    status, running, restarts, started, code, pid, policy = out.split("|")
    return {"status": status, "running": running, "restarts": int(restarts),
            "started_at": started, "exit_code": int(code), "pid": int(pid), "policy": policy}


def alive(container: str) -> str:
    return str(raw(container).get("running", "?"))


def open_tab() -> tuple[QApplication, ControllerView, dashboard.Dashboard]:
    """The real Server tab with its polling OFF, so every tick is one this file asked for."""
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
    view.grab().save(str(OUT / name))
    say(f"captured {name}: {why}")
    say(f"   world container alive at this capture: {alive(SPEC.world)}")
    say(f"   the line on the tab   : {view.verdict_label.text()!r}")
    say(f"   the words under it    : {view.status_label.text()!r}")
    # The button 8.2b photographed greyed on a healthy server, which is what the
    # three-strike rule was changed for.
    button = getattr(view, "enable_channel_button", None)
    if button is not None:
        say(f"   'Turn on the command channel': visible={button.isVisible()} "
            f"enabled={button.isEnabled()}")


# ---------------------------------------------------------------- ground

def stage_ground() -> None:
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


# ---------------------------------------------------------------- strikes

def stage_strikes() -> None:
    """One, two, three -- the database left UP, so only the strike count decides the word."""
    app, view, watcher = open_tab()
    before = raw(SPEC.world)
    say(f"GROUND, world   : {json.dumps(before)}")
    say(f"GROUND, database: {json.dumps(raw(SPEC.db))}")
    assert before["running"] == "true", "this stage needs a running world"
    assert before["policy"] == "unless-stopped", (
        f"the world's restart policy is {before['policy']!r}; a kill will not be restarted "
        "and this stage cannot produce a strike at all")
    tab_tick(app, view)
    say(f"GROUND, watcher : _last_restarts={watcher._last_restarts} _strikes={watcher._strikes} "
        f"_looping={watcher._looping}")
    say(f"GROUND, the tab says: {view.verdict_label.text()!r}")
    say("GROUND is why this is not a no-op: zero strikes, and the tab already says `up`. "
        "The word has to CHANGE, and only on the third kill.")

    readings: list[dict[str, object]] = []
    settled: dict[int, str] = {}
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
                # A kill sent before docker has reset its own restart backoff lands in
                # `restarting`, which decides the verdict by itself and would hide the
                # arithmetic this stage exists for.
                if probe * 4 >= 20:
                    break
        if kill == 1:
            shoot(app, view, "1-tab-strike-one-of-three.png",
                  "one new restart on a watcher that was already looking: the tab says up")
        if kill == 3:
            shoot(app, view, "2-tab-strike-three-of-three.png",
                  "the third new restart: the same tab, the same running world, now a loop")

    say("")
    for level in (1, 2, 3):
        say(f"at {level} strike(s), world running, the tab said: {settled.get(level)!r}")
    say(f"the first `restart loop` on a RUNNING world came at: {first_loop_at}")
    (OUT / "strikes-readings.json").write_text(json.dumps(readings, indent=1), encoding="utf-8")


# ---------------------------------------------------------------- loop

def stage_loop() -> None:
    """stage_d2's own recipe, verbatim: the database taken away, six polls at ten seconds."""
    app, view, watcher = open_tab()
    before = raw(SPEC.world)
    db_before = raw(SPEC.db)
    say(f"GROUND, world   : {json.dumps(before)}")
    say(f"GROUND, database: {json.dumps(db_before)}")
    tab_tick(app, view)
    say(f"GROUND, watcher : _last_restarts={watcher._last_restarts} _strikes={watcher._strikes} "
        f"_looping={watcher._looping}")
    say(f"GROUND, the tab says: {view.verdict_label.text()!r}")
    say("GROUND is why this is not a no-op: the tab is NOT saying `restart loop` here, and the "
        "watcher holds zero strikes.")
    shoot(app, view, "3-tab-before-the-crash-loop.png",
          "before the database is taken away: the tab is not saying restart loop")

    say(f"stopping the database ({SPEC.db}) so the world has nothing to connect to")
    subprocess.run(["docker", "stop", "-t", "60", SPEC.db], capture_output=True, timeout=300)
    say(f"database now: {json.dumps(raw(SPEC.db))}")
    world = raw(SPEC.world)
    if world.get("running") == "true":
        say(f"killing the world's main process so the crash cycle starts now: "
            f"sudo kill -9 {world['pid']}")
        subprocess.run(["sudo", "kill", "-9", str(world["pid"])], capture_output=True)
    subprocess.run(["docker", "start", SPEC.world], capture_output=True, timeout=180)

    third_new_restart_at = None
    first_loop_verdict_at = None
    readings: list[dict[str, object]] = []
    shot_three = False
    for poll in range(1, 31):        # six is the recipe; the rest is the extension
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
    say(f"third NEW restart at poll           : {third_new_restart_at}")
    say(f"first `restart loop` verdict at poll: {first_loop_verdict_at}")
    if not shot_three:
        shoot(app, view, "4-tab-three-strikes-reads-restart-loop.png",
              f"end of the run; the watcher holds {watcher._strikes} strikes")
    (OUT / "loop-readings.json").write_text(json.dumps(readings, indent=1), encoding="utf-8")

    say("putting the box back: the world stopped, the database started")
    subprocess.run(["docker", "stop", "-t", "60", SPEC.world], capture_output=True, timeout=300)
    subprocess.run(["docker", "start", SPEC.db], capture_output=True, timeout=180)
    say(f"world   : {json.dumps(raw(SPEC.world))}")
    say(f"database: {json.dumps(raw(SPEC.db))}")


# ---------------------------------------------------------------- restore

def stage_restore() -> None:
    """The stack back up through the app's own Start, and the tab read again."""
    app, view, watcher = open_tab()
    say(f"before: db={json.dumps(raw(SPEC.db))}")
    say(f"before: world={json.dumps(raw(SPEC.world))}")
    started = time.time()
    view.services.controller.start()
    say(f"the app's own Start returned in {time.time() - started:.1f}s")
    for _ in range(90):
        logs = subprocess.run(["docker", "logs", "--tail", "400", SPEC.world],
                              capture_output=True, text=True, timeout=120).stdout
        if "ready..." in logs:
            break
        time.sleep(10)
    say(f"the world reached its ready marker: {'ready...' in logs}")
    tab_tick(app, view)
    say(f"the tab now says: {view.verdict_label.text()!r} / {view.status_label.text()!r}")
    for name in (SPEC.db, SPEC.auth, SPEC.world):
        say(f"{name}: {json.dumps(raw(name))}")
    shoot(app, view, "5-tab-left-as-found.png", "the box left as it was found: the stack up")


if __name__ == "__main__":
    {"ground": stage_ground, "strikes": stage_strikes, "loop": stage_loop,
     "restore": stage_restore}[sys.argv[1]]()
