"""The rebuild control, pressed live on yulon-ubuntu2 (2026-09-09).

The first live press of the rollback feature that landed on 2026-09-08
(`native.rebuild()` / `_keep_rollback` / `_restore_rollback`, 7808c0d6 +
235d8b08), and the same press that answers questions 1-3 of the 8.6 folder:
does the Lua engine arrive in the binary, does `mod_ale.conf` arrive with
`ALE.Enabled = 1` and an absolute script path, and does `dml_bridge_ping`
answer `DML-BRIDGE-READY`.

Every step reads its GROUND first and prints it, because a step whose
assertion is already true before its action runs proves nothing. The three
readings this gate turns on are all of that shape:

* the engine in the binary -- False before, and it must be a NEW binary after;
* the `-rollback` tags -- absent before the press, present during the compile,
  gone after it settles;
* `dml_bridge_ping` -- "Command 'dml_bridge_ping' does not exist" before.

Steps are selected by argv so the long one (the compile: 35-72 minutes on this
class of box) runs on its own under nohup and no step is ever left half-way by
a timeout.
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
OUT = Path(os.environ.get("GATE_OUT", "/home/pk/rebuild-gate-out"))
OUT.mkdir(parents=True, exist_ok=True)
MODULE_ID = "mod-ale"
WORLD = "ac-worldserver"
PING = "dml_bridge_ping"


def say(text: str = "") -> None:
    print(text, flush=True)


def stamp(text: str) -> None:
    say(f"[{time.strftime('%H:%M:%SZ', time.gmtime())}] {text}")


def sh(argv: list[str], timeout: float = 120.0) -> subprocess.CompletedProcess[str]:
    return subprocess.run(argv, capture_output=True, text=True, timeout=timeout)


def world_state() -> str:
    """Alive, its pid and its restart count -- read at every capture.

    A server that has DIED photographs exactly like a refusal, so the liveness
    is a printed fact beside each reading rather than an assumption.
    """
    proc = sh(
        [
            "docker",
            "inspect",
            "-f",
            "{{.State.Status}} pid={{.State.Pid}} restarts={{.RestartCount}} "
            "started={{.State.StartedAt}} image={{.Image}}",
            WORLD,
        ],
        timeout=30,
    )
    return (proc.stdout or proc.stderr).strip()


def images() -> list[str]:
    proc = sh(
        ["docker", "images", "--format", "{{.Repository}}:{{.Tag}} {{.ID}} {{.CreatedSince}}"],
        timeout=30,
    )
    return sorted(line for line in proc.stdout.splitlines() if line.strip())


def rollback_tags() -> list[str]:
    return [line for line in images() if "-rollback" in line.split()[0]]


def bridge_scripts() -> list[str]:
    from yulon import party

    dest = party.dest_dir(SERVER)
    if not dest.is_dir():
        return []
    return sorted(p.name for p in dest.glob("*.lua"))


def ale_conf_lines() -> list[str]:
    conf = SERVER / "env/dist/etc/modules/mod_ale.conf"
    if not conf.is_file():
        return ["(mod_ale.conf does not exist)"]
    text = conf.read_text(encoding="utf-8", errors="replace")
    return [ln for ln in text.splitlines() if ln.strip().startswith("ALE.")]


def services():
    from yulon.catalog.catalog import load_catalog
    from yulon.ui.controller_view import ControllerServices

    entry = load_catalog().get("wow-wotlk")
    return entry, ControllerServices.for_entry(entry, SERVER)


def ask_server(command: str) -> str:
    """Ask the running world one command over the app's own command channel."""
    _entry, svc = services()
    channel = svc.channel_setup.live_channel() if svc.channel_setup is not None else None
    if channel is None:
        return "(no saved credential: the command channel has never been proved on this install)"
    answer = channel.send(command)
    return (
        f"outcome={answer.outcome!r} denied={answer.denied} "
        f"indeterminate={answer.indeterminate} reason={answer.reason!r}\n"
        f"text: {answer.text!r}"
    )


def engine_read() -> str:
    from yulon import party

    read = party.read_engine_in_binary(WORLD)
    return f"engine={read.engine!r}  {read.sentence}"


# --------------------------------------------------------------------- ground


def step_ground() -> None:
    from yulon import docker

    say("=== GROUND, read before anything is installed or compiled ===")
    stamp(f"server_dir={SERVER}")
    say(f"world: {world_state()}")
    say("docker images:")
    for line in images():
        say(f"  {line}")
    say(f"-rollback tags present NOW (must be none): {rollback_tags()}")
    say(f"allowed_modules(): {docker.allowed_modules(SERVER)!r}")
    say(f"modules/ on disk: {sorted(p.name for p in (SERVER / 'modules').glob('*')) if (SERVER / 'modules').is_dir() else '(no modules dir)'}")
    say(f"modules/{MODULE_ID} cloned: {(SERVER / 'modules' / MODULE_ID).is_dir()}")
    say(f"mod_ale.conf ALE.* lines: {ale_conf_lines()}")
    say(f"bridge scripts on disk: {bridge_scripts()}")
    say("the Lua engine, read out of the RUNNING server's own executable:")
    say(f"  {engine_read()}")
    say(f"the server asked {PING!r}:")
    say(f"  {ask_server(PING)}")
    say(f"world at the end of the ground reading: {world_state()}")


# ---------------------------------------------------------------- install ALE


def step_install() -> None:
    """Install ALE through the applier -- the seam the Modules tab presses."""
    say("=== INSTALL: AzerothCore Lua Engine (ALE), through the app's own applier ===")
    say(f"ground: modules/{MODULE_ID} cloned = {(SERVER / 'modules' / MODULE_ID).is_dir()}")
    say(f"ground: mod_ale.conf ALE.* lines = {ale_conf_lines()}")
    say(f"ground: world = {world_state()}")
    if (SERVER / "modules" / MODULE_ID).is_dir():
        say("GROUND FAILS: the module is already on disk; this step would prove nothing")
        return
    _entry, svc = services()
    manifest = next(m for m in svc.store.load_all("module") if m.id == MODULE_ID)
    say(f"manifest: {manifest.id} {manifest.name!r} rebuild_flag={manifest.build}")
    started = time.time()
    result = svc.applier.install(manifest)
    say(f"--- ApplyReport after {time.time() - started:.1f}s ---")
    say("done:")
    for entry_line in result.done:
        say(f"  {entry_line}")
    say("skipped:")
    for entry_line in result.skipped:
        say(f"  {entry_line}")
    say("pending_sql:")
    for pending in result.pending_sql:
        say(f"  db={pending.db} path={pending.path} files={list(pending.files)}")
    say(f"rebuild_required={result.rebuild_required}  restart_recommended={result.restart_recommended}")
    say(f"AFTER: modules/{MODULE_ID} cloned = {(SERVER / 'modules' / MODULE_ID).is_dir()}")
    head = sh(["git", "-C", str(SERVER / "modules" / MODULE_ID), "rev-parse", "HEAD"], timeout=30)
    say(f"AFTER: {MODULE_ID} HEAD = {head.stdout.strip()!r} (manifest pins {manifest.source})")
    say(f"AFTER: mod_ale.conf ALE.* lines = {ale_conf_lines()}")
    say(f"AFTER: the engine in the RUNNING binary is still: {engine_read()}")
    say(f"world: {world_state()}")


def step_report_shot() -> None:
    """The same install report, rendered by the app's own Modules tab."""
    from PySide6.QtWidgets import QApplication
    from yulon.ui.controller_view import ControllerView, _format_report

    app = QApplication.instance() or QApplication([])
    entry, svc = services()
    view = ControllerView(entry, svc, status_poll_ms=0)
    view.resize(1100, 800)
    for index in range(view._tabs.count()):
        if view._tabs.tabText(index) == "Modules":
            view._tabs.setCurrentIndex(index)
            break
    source = OUT / os.environ.get("GATE_SHOT_FILE", "4-install-ale.log")
    view.module_report.setPlainText(source.read_text(encoding="utf-8", errors="replace"))
    app.processEvents()
    path = OUT / os.environ.get("GATE_SHOT_NAME", "modules-tab-ale-install.png")
    view.grab().save(str(path))
    say(f"SHOT {path}  {WORLD} at capture: {world_state()}")


# -------------------------------------------------------------------- rebuild


def step_rebuild() -> None:
    """THE PRESS. Every line the engine yields, stamped, to this log."""
    from yulon.install_wiring import rebuild_for_app

    entry, _svc = services()
    say("=== REBUILD, through rebuild_for_app(entry, server_dir)(cancel) ===")
    say(f"ground: world = {world_state()}")
    say(f"ground: -rollback tags = {rollback_tags()}")
    say("ground: docker images:")
    for line in images():
        say(f"  {line}")
    say(f"ground: the engine in the running binary = {engine_read()}")
    say("--- the press ---")
    started = time.time()
    rebuild = rebuild_for_app(entry, SERVER)
    try:
        for line in rebuild(None):
            stamp(line)
    except Exception as exc:  # noqa: BLE001 - the refusal IS the result here
        stamp(f"REBUILD RAISED: {type(exc).__name__}: {exc}")
        say(f"after the failure: world = {world_state()}")
        say(f"after the failure: -rollback tags = {rollback_tags()}")
        say("after the failure: docker images:")
        for line in images():
            say(f"  {line}")
        say("REBUILD OUTCOME: FAILED (the message above is what the user reads)")
        raise SystemExit(1) from exc
    stamp(f"REBUILD RETURNED CLEANLY in {time.time() - started:.1f}s")
    say(f"after: world = {world_state()}")
    say(f"after: -rollback tags (must be none) = {rollback_tags()}")
    say("after: docker images:")
    for line in images():
        say(f"  {line}")


# ---------------------------------------------------------------------- after


def step_after() -> None:
    say("=== AFTER: what the rebuild changed ===")
    say(f"world: {world_state()}")
    say(f"-rollback tags (must be none once it settled): {rollback_tags()}")
    say("docker images:")
    for line in images():
        say(f"  {line}")
    say("the Lua engine, read out of the NEW binary:")
    say(f"  {engine_read()}")
    say(f"mod_ale.conf ALE.* lines: {ale_conf_lines()}")
    proc = sh(
        ["docker", "exec", WORLD, "sh", "-c", "ls -l /azerothcore/env/dist/etc/modules/lua_scripts"],
        timeout=60,
    )
    say(f"the container's view of the script directory:\n{proc.stdout or proc.stderr}")


def step_deploy() -> None:
    from yulon import party

    say("=== DEPLOY the five bridge scripts ===")
    dest = party.dest_dir(SERVER)
    say(f"ground: scripts already there = {bridge_scripts()}")
    result = party.deploy(party.lua_root(), dest)
    say(f"deploy: changed={result.changed} names={list(result.names)}")
    say(f"after: scripts on disk = {bridge_scripts()}")
    say(f"world: {world_state()}")


def step_ping() -> None:
    say("=== THE ARRIVAL PROOF: dml_bridge_ping over the command channel ===")
    say(f"ground: world = {world_state()}")
    say(f"ground: bridge scripts on disk = {bridge_scripts()}")
    say(f"ground: the engine in the binary = {engine_read()}")
    for command in (PING, ".reload ale", PING):
        say(f"--- {command!r} ---")
        say(ask_server(command))
    say(f"world at the end: {world_state()}")


def step_channel() -> None:
    """Set the command channel up, because this fresh install has none.

    `channel_setup.enable()` refuses while the world is running -- writing the
    SOAP listener under a live world risks the world on a failed bind -- so the
    sequence is the one a user performs: Stop, enable, Start, then one real
    round trip. Nothing here is inferred from the conf file: the credential is
    written only after a command came back.
    """
    from yulon.catalog import composegen

    say("=== COMMAND CHANNEL: this install has never had one ===")
    entry, svc = services()
    channel = svc.channel_setup
    install_id = composegen.install_id(SERVER)
    say(f"install_id: {install_id}")
    say(f"ground: setup_state = {channel.setup_state()!r}")
    say(f"ground: live_channel() = {channel.live_channel() is not None}")
    say(f"ground: world = {world_state()}")
    override = SERVER / "docker-compose.override.yml"
    before = override.read_text(encoding="utf-8", errors="replace") if override.is_file() else ""
    say(f"ground: SOAP named in the override = {'SOAP' in before.upper()}")

    stamp("stopping the world through the app's own Stop")
    svc.controller.stop()
    say(f"stopped: world = {world_state()}")
    result = channel.enable(world_running=False)
    say(f"enable() -> path={result.path} changed={result.changed}")
    after = override.read_text(encoding="utf-8", errors="replace")
    say(f"after: SOAP named in the override = {'SOAP' in after.upper()}")
    for line in after.splitlines():
        if "SOAP" in line.upper():
            say(f"  {line.strip()}")
    stamp("starting the world again")
    svc.controller.start()
    say(f"started: world = {world_state()}")
    stamp("settling the channel against the live server")
    state = channel.settle()
    say(f"settle() -> {state!r}")
    say(f"live_channel(): {channel.live_channel() is not None}")
    say(f"credential file: {channel_setup_path(entry.id, install_id)}")


def wait_ready(marker: str = "ready...", timeout_s: float = 900.0) -> bool:
    """Wait for the world's own ready line in its own log. Prints what it saw."""
    started = time.time()
    while time.time() - started < timeout_s:
        proc = sh(["docker", "logs", "--tail", "400", WORLD], timeout=60)
        blob = proc.stdout + proc.stderr
        if marker in blob:
            stamp(f"the world printed {marker!r} after {time.time() - started:.0f}s")
            return True
        time.sleep(10)
    stamp(f"the world never printed {marker!r} within {timeout_s:.0f}s")
    return False


def step_channel_prove() -> None:
    """Prove the channel against a world that has finished loading.

    Separate from `channel` because the first press caught the world six
    seconds after Start and got `Pending` -- correctly. The account exists from
    then on, and `create_account` never rewrites the credentials of a row that
    exists, so a second process starting from `Idle` gets a 401. That is
    `Refused`, and `repair()` is the way out that `ensure()`'s own comment
    describes; this walks it rather than hand-writing SQL.
    """
    say("=== COMMAND CHANNEL: proving it against a loaded world ===")
    _entry, svc = services()
    channel = svc.channel_setup
    say(f"ground: setup_state = {channel.setup_state()!r}")
    say(f"ground: live_channel() = {channel.live_channel() is not None}")
    say(f"ground: world = {world_state()}")
    if not wait_ready():
        say("the world never reported ready; nothing was proved")
        return
    for attempt in range(1, 4):
        state = channel.settle()
        say(f"settle() #{attempt} -> {state!r}")
        if type(state).__name__ == "Verified":
            break
        if type(state).__name__ == "Refused":
            repaired = channel.repair()
            say(f"repair() -> {repaired!r}")
            if type(repaired).__name__ == "Verified":
                break
        time.sleep(20)
    say(f"live_channel(): {channel.live_channel() is not None}")
    say(f"asked {PING!r}: {ask_server(PING)}")
    say("asked 'server info': " + ask_server("server info"))
    say(f"world: {world_state()}")


def channel_setup_path(game: str, install_id: str) -> str:
    from yulon import channel_setup

    path = channel_setup.credential_path(game, install_id)
    return f"{path} exists={path.is_file()}"


STEPS = {
    "ground": step_ground,
    "install": step_install,
    "shot": step_report_shot,
    "rebuild": step_rebuild,
    "after": step_after,
    "deploy": step_deploy,
    "ping": step_ping,
    "channel": step_channel,
    "channel-prove": step_channel_prove,
}


def main() -> int:
    from yulon.log import configure, use_utf8_streams

    use_utf8_streams()
    configure()
    if len(sys.argv) < 2 or sys.argv[1] not in STEPS:
        say(f"usage: rebuild_gate.py <{'|'.join(STEPS)}>")
        return 2
    STEPS[sys.argv[1]]()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
