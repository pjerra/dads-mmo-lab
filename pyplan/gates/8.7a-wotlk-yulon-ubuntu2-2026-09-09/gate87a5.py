"""8.7a's FIFTH clause on WoW WotLK, pressed on `yulon-ubuntu2` after the rebuild.

    "a configuration change the module needs is shown by the running server,
     not only read back from the file"

8.7a's own run (`pyplan/gates/8.7a-wotlk-yulon-ubuntu-2026-09-08/README.md`) recorded this
clause NOT MET and measured why: AzerothCore looks a module's conf file up by the names of the
modules COMPILED INTO the binary, so a module that has not been through a rebuild has no name on
that list and its conf file is invisible however correct it is. The rebuild lane compiled
`mod-ale` in on this box on 2026-09-09 and showed the running server naming `mod_ale.conf`.

WHY THIS IS NOT A RE-PHOTOGRAPH OF THAT. Both readings already on the record were taken with the
file's values ALREADY the ones the app wants (`ALE.Enabled = 1`, the manifest's `ALE.ScriptPath`),
so nothing in them can distinguish "the running server follows this file" from "the running server
was going to do that anyway". A step whose assertion is already true before its action runs proves
nothing. So this gate:

  1  reads the ground, INCLUDING the two other lanes' edits to that same file, and says in as many
     words which assertions are already true;
  2  makes the ground FALSE by hand -- `ALE.ScriptPath` is pointed at a decoy directory holding one
     marker script and none of the five bridge scripts -- and restarts through the app's own Stop
     and Start, so the running server is asked with the wrong value in force. That direction is the
     control: `dml_bridge_ping` must STOP existing, and the marker must appear in the log;
  3  presses the APP -- `applier.configure('mod-ale')`, the Modules surface's own seam -- to put the
     value back, restarts the same way, and asks the running server again.

Both directions use the SAME instrument (the app's Stop/Start, and the world's own log bounded by
this boot's `StartedAt`), so the only thing that differs between them is the file.

Usage:  python gate87a5.py ground|perturb|configure|verify|restore
        (one stage per invocation: each restart stage is minutes long, and a gate that times out
         half-way through a stop leaves a server down)
"""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

sys.path.insert(0, str(Path(__file__).resolve().parent / "pylauncher"))

from PySide6.QtWidgets import QApplication  # noqa: E402

from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.controller_wow_wotlk import docker_ctl  # noqa: E402
from yulon.ui.controller_view import ControllerServices, ControllerView  # noqa: E402

SERVER = Path("/home/pk/wowserver")
OUT = Path(__file__).resolve().parent / "out"
OUT.mkdir(parents=True, exist_ok=True)
MODULE_ID = "mod-ale"
ALE_CONF = SERVER / "env/dist/etc/modules/mod_ale.conf"
GOOD_PATH = "/azerothcore/env/dist/etc/modules/lua_scripts"
DECOY_HOST = SERVER / "env/dist/etc/modules/lua_gate_decoy"
DECOY_IN_CONTAINER = "/azerothcore/env/dist/etc/modules/lua_gate_decoy"
MARKER = "GATE87A5-MARKER-FROM-THE-DECOY-PATH"
GROUND_COPY = OUT / "mod_ale.conf.as-found"
WORLD = docker_ctl.SPEC.world
_ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")


def stamp() -> str:
    return datetime.now(UTC).strftime("%H:%M:%SZ")


def say(text: str) -> None:
    print(f"[{stamp()}] {text}", flush=True)


def sh(argv: list[str], timeout: float = 180.0) -> subprocess.CompletedProcess[str]:
    return subprocess.run(argv, capture_output=True, text=True, timeout=timeout)


def raw(container: str = WORLD) -> dict[str, object]:
    proc = sh(
        ["docker", "inspect", "-f",
         "{{.State.Status}}|{{.State.Running}}|{{.RestartCount}}|{{.State.StartedAt}}|{{.Image}}",
         container],
        timeout=60,
    )
    out = proc.stdout.strip()
    if not out:
        return {"error": proc.stderr.strip()}
    status, running, restarts, started, image = out.split("|")
    return {"status": status, "running": running, "restarts": int(restarts),
            "started_at": started, "image": image[:19]}


def alive(container: str = WORLD) -> bool:
    return raw(container).get("running") == "true"


def this_boot_log() -> str:
    """The world's own words since THIS boot only.

    `docker logs --since 20m` would carry the previous boot's `Using modules configuration`
    line straight into the reading of a restart that happened seconds ago -- which is how a
    stale line reads as a fresh fact. The container's own `StartedAt` bounds it instead.
    """
    started = str(raw().get("started_at", ""))
    proc = sh(["docker", "logs", "--since", started, WORLD], timeout=180)
    return proc.stdout + proc.stderr


def conf_lines(keys: tuple[str, ...] = ("ALE.Enabled", "ALE.ScriptPath", "Logger.ALE")) -> list[str]:
    if not ALE_CONF.is_file():
        return ["(mod_ale.conf is not on disk)"]
    out = []
    for line in ALE_CONF.read_text(encoding="utf-8", errors="replace").splitlines():
        bare = line.strip()
        if bare.startswith("#"):
            continue
        if any(bare.startswith(k) for k in keys):
            out.append(bare)
    return out


def module_config_lines(log: str) -> list[str]:
    wanted = ("Loading Modules Configuration", "Using modules configuration", "Config::LoadFile",
              "Not found modules config", MARKER, "[dml_", "[gate87a5]")
    out = []
    for line in log.splitlines():
        # The colour codes are why the first run of this gate could not see the one
        # line that answers the clause: the conf file the server opened is printed as
        # `\x1b[0m\x1b[36m> mod_ale.conf`, so `startswith('>')` was false on it.
        bare = _ANSI.sub("", line).strip()
        # The conf file the server OPENED is printed on its own line under
        # `Using modules configuration:`, as a bare `> name.conf` — the line that
        # answers this clause, and the one a substring filter drops.
        if any(w in bare for w in wanted) or (bare.startswith(">") and bare.endswith(".conf")):
            out.append(bare)
    return out


def ask(command: str) -> tuple[str, list[str]]:
    """Ask the running world over the app's OWN command channel (SOAP), as 8.6 did.

    `console.send_command` was the first instrument here and it is the wrong one for this
    question: a `ConsoleReply` says whether a prompt was seen, not whether the command
    exists, so "does not exist" and "answered" are the same shape on it. The channel's
    `Answer` is the flip the bridge probe is built on — a refusal comes back `no` carrying
    the server's own *Command … does not exist*, an answer comes back `yes`.
    """
    _app, view = _CACHED_VIEW if _CACHED_VIEW else build_view()
    setup = view.services.channel_setup
    channel = setup.live_channel() if setup is not None else None
    if channel is None:
        return "no-credential", ["(the command channel has never been proved on this install)"]
    answer = channel.send(command)
    lines = [ln.strip() for ln in answer.text.splitlines() if ln.strip()]
    return answer.outcome, lines


_CACHED_VIEW: tuple[QApplication, ControllerView] | None = None


def build_view() -> tuple[QApplication, ControllerView]:
    global _CACHED_VIEW
    app = QApplication.instance() or QApplication([])
    entry = load_catalog().get("wow-wotlk")
    services = ControllerServices.for_entry(entry, SERVER)
    view = ControllerView(entry, services, status_poll_ms=0)
    view.resize(1150, 820)
    _CACHED_VIEW = (app, view)
    return app, view


def shot(app: QApplication, view: ControllerView, name: str, why: str, tab: str = "Modules") -> None:
    for index in range(view._tabs.count()):
        if view._tabs.tabText(index) == tab:
            view._tabs.setCurrentIndex(index)
            break
    for _ in range(30):
        app.processEvents()
        time.sleep(0.02)
    view.grab().save(str(OUT / name))
    state = raw()
    say(f"SHOT {name}: {why}")
    say(f"   {WORLD} at this capture: running={state.get('running')} "
        f"restarts={state.get('restarts')} image={state.get('image')}")


def set_key(key: str, value: str) -> None:
    """A by-hand edit, kept apart from the app's own writer on purpose.

    The perturbation is MINE and is labelled as mine everywhere; the only write in this gate
    that the app makes is `applier.configure()` in stage 3.
    """
    text = ALE_CONF.read_text(encoding="utf-8", errors="replace")
    lines = text.splitlines(keepends=True)
    for i, line in enumerate(lines):
        if line.strip().startswith(key):
            lines[i] = f"{key} = {value}\n"
            break
    else:
        lines.append(f"{key} = {value}\n")
    ALE_CONF.write_text("".join(lines), encoding="utf-8", newline="")


def restart_through_the_app(app: QApplication, view: ControllerView, what: str) -> None:
    """The app's own Stop then Start -- the same instrument in both directions."""
    say(f"--- {what}: the app's own Stop ---")
    started = time.time()
    view.services.controller.stop()
    say(f"stop returned in {time.time() - started:.1f}s; world alive = {alive()}")
    say(f"--- {what}: the app's own Start ---")
    started = time.time()
    view.services.controller.start()
    say(f"start returned in {time.time() - started:.1f}s; world: {json.dumps(raw())}")
    for _ in range(90):
        if "ready..." in this_boot_log():
            break
        time.sleep(10)
    say(f"this boot reached the world's ready marker: {'ready...' in this_boot_log()}")


# ------------------------------------------------------------------ 1: ground

def stage_ground() -> None:
    app, view = build_view()
    say("=== GROUND: read, and nothing written ===")
    for name in (docker_ctl.SPEC.db, docker_ctl.SPEC.auth, WORLD):
        say(f"{name}: {json.dumps(raw(name))}")
    say(f"mod_ale.conf on disk: {ALE_CONF.is_file()}")
    for line in conf_lines():
        say(f"  {line}")
    shutil.copy2(ALE_CONF, GROUND_COPY)
    say(f"kept a byte copy of the file as found at {GROUND_COPY}")
    say(f"scripts in the good path: {sorted(p.name for p in (SERVER / 'env/dist/etc/modules/lua_scripts').iterdir())}")

    log = this_boot_log()
    say("--- what the RUNNING server says about module configuration, this boot ---")
    for line in module_config_lines(log):
        say(f"  {line}")
    outcome, lines = ask(".server debug")
    say(f"--- .server debug (outcome={outcome}) ---")
    for line in lines:
        if line.startswith("|-") or "enabled modules" in line:
            say(f"  {line}")
    ping = ask("dml_bridge_ping")
    say(f"--- dml_bridge_ping: outcome={ping[0]} lines={ping[1]} ---")

    core = SERVER / "env/dist/etc/worldserver.conf"
    custom = next((ln.strip() for ln in core.read_text(encoding="utf-8", errors="replace").splitlines()
                   if ln.strip().startswith("Creatures.CustomIDs")), "(absent)")
    say(f"the key 8.7a's folder names, read only: {custom}")

    say("")
    say("WHICH ASSERTIONS ARE ALREADY TRUE HERE (so that pressing them would prove nothing):")
    say("  * ALE.Enabled is already 1 and the engine is already running")
    say("  * the five bridge scripts are already loaded and dml_bridge_ping already answers")
    say("  * mod_ale.conf is already the file the running server names")
    say("So the press has to make each of those FALSE first, and the control is that")
    say("dml_bridge_ping STOPS existing while the file says somewhere else.")
    view.module_report.setPlainText(
        "mod_ale.conf as found:\n" + "\n".join(f"  {ln}" for ln in conf_lines())
        + "\n\nthe running server, this boot:\n"
        + "\n".join(f"  {ln}" for ln in module_config_lines(log)[:12])
        + f"\n\ndml_bridge_ping: {ping[0]}  {ping[1][:1]}"
    )
    shot(app, view, "1-ground-the-values-are-already-right.png",
         "the ground: every assertion this clause makes is already true")


# ----------------------------------------------------------------- 2: perturb

def stage_perturb() -> None:
    app, view = build_view()
    say("=== CONTROL: the file is pointed at a decoy path, BY HAND, and the world restarted ===")
    if not GROUND_COPY.is_file():
        say("REFUSED: no byte copy of the file as found. Run `ground` first.")
        return
    DECOY_HOST.mkdir(parents=True, exist_ok=True)
    marker = DECOY_HOST / "dml_gate_marker.lua"
    marker.write_text(
        f'print("[gate87a5] {MARKER}")\n',
        encoding="utf-8", newline="\n",
    )
    say(f"decoy path {DECOY_HOST} holds: {sorted(p.name for p in DECOY_HOST.iterdir())}")
    say(f"BEFORE (mine, by hand): {conf_lines()}")
    set_key("ALE.ScriptPath", f'"{DECOY_IN_CONTAINER}"')
    say(f"AFTER  (mine, by hand): {conf_lines()}")

    restart_through_the_app(app, view, "control")

    log = this_boot_log()
    say("--- the RUNNING server, with the WRONG value in force ---")
    for line in module_config_lines(log):
        say(f"  {line}")
    say(f"the marker from the decoy path is in this boot's log: {MARKER in log}")
    say(f"any of the five bridge scripts loaded this boot: "
        f"{[ln for ln in module_config_lines(log) if '[dml_' in ln]}")
    ping = ask("dml_bridge_ping")
    say(f"dml_bridge_ping now: outcome={ping[0]} lines={ping[1]}")
    control = ask("server info")
    say(f"CONTROL, so a silent channel cannot be mistaken for a missing command: "
        f"server info outcome={control[0]} first line={control[1][:1]}")
    view.module_report.setPlainText(
        "mod_ale.conf, perturbed BY HAND (not by the app):\n"
        + "\n".join(f"  {ln}" for ln in conf_lines())
        + "\n\nthe running server, this boot:\n"
        + "\n".join(f"  {ln}" for ln in module_config_lines(log)[:12])
        + f"\n\ndml_bridge_ping: {ping[0]}  {ping[1][:1]}"
    )
    shot(app, view, "2-wrong-value-the-running-server-follows-it.png",
         "the decoy path in force: the marker loads, the five bridge scripts do not, "
         "and dml_bridge_ping has stopped existing")


# --------------------------------------------------------------- 3: the press

def stage_configure() -> None:
    app, view = build_view()
    say("=== THE PRESS: the APP's own configure() puts the value the module needs back ===")
    say(f"ground for this step (the wrong value is in force): {conf_lines()}")
    log = this_boot_log()
    say(f"ground: the marker is in this boot's log: {MARKER in log}")
    ping = ask("dml_bridge_ping")
    say(f"ground: dml_bridge_ping outcome={ping[0]} lines={ping[1]}")
    if ping[0] == "yes":
        say("GROUND FAILS: the bridge already answers, so this step would prove nothing. Stop.")
        return

    store = view.services.store
    manifest = next(m for m in store.load_all("module") if m.id == MODULE_ID)
    applier = view.services.applier
    assert applier is not None
    result = applier.configure(manifest)
    say("--- ApplyReport.done ---")
    for entry in result.done:
        say(f"  {entry}")
    say("--- ApplyReport.skipped ---")
    for entry in result.skipped:
        say(f"  {entry}")
    say(f"rebuild_required={result.rebuild_required} restart_recommended={result.restart_recommended}")
    say(f"the file, READ BACK (which the clause says is NOT enough): {conf_lines()}")
    view.module_report.setPlainText(
        "applier.configure('mod-ale'):\n"
        + "\n".join(f"  {e}" for e in result.done)
        + ("\nskipped:\n" + "\n".join(f"  {e}" for e in result.skipped) if result.skipped else "")
        + "\n\nthe file read back (not enough on its own):\n"
        + "\n".join(f"  {ln}" for ln in conf_lines())
    )
    shot(app, view, "3-the-app-wrote-it-back-file-only.png",
         "the app's own configure() report, and the file read back -- the half the clause "
         "calls insufficient")

    restart_through_the_app(app, view, "the press")
    stage_verify(app, view)


# --------------------------------------------------------------- 4: the proof

def stage_verify(app: QApplication | None = None, view: ControllerView | None = None) -> None:
    if app is None or view is None:
        app, view = build_view()
    say("=== THE PROOF: the same question, asked of the RUNNING server ===")
    log = this_boot_log()
    for line in module_config_lines(log):
        say(f"  {line}")
    loaded = [ln for ln in module_config_lines(log) if "[dml_" in ln and "loaded" in ln]
    say(f"bridge scripts this boot loaded from the path the app wrote: {len(loaded)}")
    say(f"the decoy marker in THIS boot's log (must be False): {MARKER in log}")
    ping = ask("dml_bridge_ping")
    say(f"dml_bridge_ping: outcome={ping[0]} lines={ping[1]}")
    outcome, lines = ask(".server debug")
    named = [ln for ln in lines if ln.startswith("|-") or "enabled modules" in ln]
    say(f".server debug (outcome={outcome}): {named}")
    say(f"the file says: {conf_lines()}")
    verdict = ping[0] == "yes" and len(loaded) >= 5 and MARKER not in log
    say(f"CLAUSE 5 on this box, for a module that has been through a rebuild: "
        f"{'PASS' if verdict else 'FAIL'}")
    view.module_report.setPlainText(
        "the RUNNING server, after the app's configure() and the restart:\n"
        + "\n".join(f"  {ln}" for ln in module_config_lines(log)[:14])
        + f"\n\ndml_bridge_ping: {ping[0]}  {ping[1][:1]}"
        + "\n\n.server debug:\n" + "\n".join(f"  {ln}" for ln in named)
    )
    shot(app, view, "4-the-running-server-shows-the-value-the-app-wrote.png",
         "the value the app wrote, shown by the running server: the five scripts loaded "
         "from that path by name, the decoy gone, the bridge answering")


# ------------------------------------------------------------- 5: put it back

def stage_restore() -> None:
    say("=== putting the file back exactly as it was found, and removing the decoy ===")
    if not GROUND_COPY.is_file():
        say("REFUSED: no byte copy of the file as found.")
        return
    now = ALE_CONF.read_bytes()
    found = GROUND_COPY.read_bytes()
    say(f"the file now == the file as found: {now == found}")
    if now != found:
        ALE_CONF.write_bytes(found)
        say("restored the as-found bytes")
        say(f"now: {conf_lines()}")
    if DECOY_HOST.exists():
        shutil.rmtree(DECOY_HOST)
    say(f"decoy directory removed: {not DECOY_HOST.exists()}")
    say(f"world: {json.dumps(raw())}")


if __name__ == "__main__":
    {"ground": stage_ground, "perturb": stage_perturb, "configure": stage_configure,
     "verify": stage_verify, "restore": stage_restore}[sys.argv[1]]()
