"""The 8.6 live gate on `yulon-ubuntu`: is My Party's route there at all?

The 2026-08-20 experiment, repeated properly. That run deployed the bridge,
read a success envelope, and every bridge command answered "Command does not
exist"; nothing recorded WHY, so the route has never been recorded working and
this box's first question is whether it works at all.

Usage:  python gate86.py <stage>

    ground   everything before anything is written: the binary, the conf, the
             deploy directory, and the server's own answer to the bridge command
    deploy   deploy the bridge scripts THROUGH THE APP'S OWN CODE
    after    ask the server again, read the log, and print the preconditions
    report   the whole thing again, as one page, for the screenshot

`ground` and `after` ask the same four questions in the same words on purpose:
a gate step whose assertion is already true before its action proves nothing,
and this gate's action (the deploy) is exactly the kind that can change nothing.
Printing both readings side by side is what makes that visible instead of
assumed.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path.home() / "gate86" / "pylauncher"))

from yulon import channel, docker, party, soap  # noqa: E402
from yulon.catalog.catalog import load_catalog  # noqa: E402
from yulon.controller_wow_wotlk import console  # noqa: E402

SERVER_DIR = Path.home() / "wowserver"
ENTRY = load_catalog().get("wow-wotlk")
SPEC = ENTRY.container_spec()
CRED = Path.home() / ".local/share/yulon/credentials/wow-wotlk-243c46e3.json"
OUT = Path.home() / "gate86-out"

_NAMES_THE_BRIDGE = re.compile(r"dml_|\bALE\b|ALE\.[A-Z]|LuaEngine|lua_scripts|\bEluna\b")
"""Word-precise AND case-sensitive, and both were bought by a wrong answer.

Unanchored `ALE` matched `botActiveAloneSmartScale` twenty-five times. Adding
word boundaries fixed that and `re.IGNORECASE` then matched `Loading Motd
locale...` on the `ALE\\.` arm. The engine spells itself `ALE` in capitals
everywhere it appears, so case is the discriminator and folding it away was
the mistake."""


def stamp() -> str:
    return datetime.now(UTC).strftime("%Y-%m-%d %H:%M:%SZ")


def say(text: str) -> None:
    subprocess.run([str(Path.home() / "claude-say"), text], check=False)


def make_channel() -> channel.SoapChannel:
    cred = json.loads(CRED.read_text())
    endpoint = soap.Endpoint(
        host=cred.get("host", "127.0.0.1"),
        port=int(cred.get("port", 7878)),
        account=cred["account"],
        password=cred["password"],
        namespace=ENTRY.soap_namespace if hasattr(ENTRY, "soap_namespace") else "urn:AC",
    )
    return channel.SoapChannel(
        endpoint=endpoint,
        state_of=lambda: docker.container_state(SPEC.world),
    )


# -- the four questions ----------------------------------------------------


def q_binary() -> party.BinaryRead:
    return party.read_engine_in_binary(SPEC.world)


def q_cloned() -> bool:
    return (SERVER_DIR / "modules" / "mod-ale" / "CMakeLists.txt").is_file()


def q_conf() -> tuple[bool, bool | None, str | None]:
    """(the conf exists, ALE.Enabled, ALE.ScriptPath) read from the file itself."""
    conf = SERVER_DIR / party.ALE_CONF
    if not conf.is_file():
        return False, None, None
    enabled: bool | None = None
    path: str | None = None
    for line in conf.read_text(errors="replace").splitlines():
        bare = line.strip()
        if bare.startswith("ALE.Enabled"):
            value = bare.partition("=")[2].strip().strip('"').lower()
            enabled = value in ("1", "true", "yes")
        elif bare.startswith("ALE.ScriptPath"):
            path = bare.partition("=")[2].strip().strip('"')
    return True, enabled, path


def q_deployed() -> tuple[str, ...]:
    dest = party.dest_dir(SERVER_DIR)
    if not dest.is_dir():
        return ()
    return tuple(sorted(p.name for p in dest.glob("*.lua")))


def q_probe() -> tuple[party.Probe, channel.Answer]:
    answer = make_channel().send(party.PROBE_COMMAND)
    return party.read_probe(answer), answer


def facts() -> party.Facts:
    binary = q_binary()
    conf_present, enabled, path = q_conf()
    probe, _ = q_probe()
    state = docker.container_state(SPEC.world)
    return party.Facts(
        server_installed=(SERVER_DIR / "acore.json").is_file(),
        world_running=state.status == "running",
        engine_cloned=q_cloned(),
        engine_in_binary=binary.engine,
        conf_present=conf_present,
        engine_enabled=enabled,
        script_path=path,
        deployed=q_deployed(),
        bridge_answered=probe.arrived,
    )


def read(label: str) -> str:
    """One full reading, as text. The same words before and after the deploy."""
    binary = q_binary()
    conf_present, enabled, path = q_conf()
    probe, answer = q_probe()
    deployed = q_deployed()
    lines = [
        f"=== {label}  {stamp()} ===",
        "",
        "1. IS THE LUA ENGINE IN THE IMAGE?  (read from the binary, not a conf)",
        f"   modules/mod-ale cloned : {q_cloned()}",
        f"   in the worldserver     : {binary.engine!r}",
        f"   {binary.sentence}",
        "",
        "2. WHAT DOES THE CONF SAY?",
        f"   {party.ALE_CONF} exists : {conf_present}",
        f"   ALE.Enabled             : {enabled!r}",
        f"   ALE.ScriptPath          : {path!r}",
        "",
        "3. ARE THE BRIDGE SCRIPTS DEPLOYED?",
        f"   {party.dest_dir(SERVER_DIR)}",
        f"   {list(deployed)}",
        "",
        f"4. DOES THE SERVER KNOW `{party.PROBE_COMMAND}`?   <- the box's first question",
        f"   channel outcome : {answer.outcome}",
        f"   server said     : {answer.text.strip()!r}",
        f"   arrived         : {probe.arrived!r}",
        f"   {probe.sentence}",
        "",
        "PRECONDITIONS, in the order a person would fix them:",
    ]
    for check in party.preconditions(facts()):
        mark = "ok  " if check.met else "NOT "
        lines.append(f"   [{mark}] {check.name}")
        if not check.met:
            lines.append(f"           {check.sentence}")
    lines += ["", f"READY: {party.ready(facts())}"]
    return "\n".join(lines)


# -- stages ----------------------------------------------------------------


def stage_ground() -> None:
    say("8.6 GROUND: reading the server before anything is deployed")
    text = read("GROUND -- before anything is written")
    OUT.mkdir(exist_ok=True)
    (OUT / "1-ground.txt").write_text(text)
    print(text)


def stage_deploy() -> None:
    say("8.6: deploying the bridge scripts through the app's own party.deploy()")
    root = party.lua_root()
    dest = party.dest_dir(SERVER_DIR)
    print(f"source : {root}")
    print(f"dest   : {dest}")
    done = party.deploy(root, dest)
    text = "\n".join(
        [
            f"=== DEPLOY  {stamp()} ===",
            f"source     : {root}",
            f"dest       : {dest}",
            f"changed    : {done.changed}",
            f"names      : {list(done.names)}",
            "",
            "second press, to show the copy is idempotent:",
            f"changed    : {party.deploy(root, dest).changed}",
        ]
    )
    OUT.mkdir(exist_ok=True)
    (OUT / "2-deploy.txt").write_text(text)
    print(text)


def stage_after() -> None:
    say("8.6: asking the server the same four questions again, after the deploy")
    text = read("AFTER THE DEPLOY -- the same four questions")
    log = subprocess.run(
        ["docker", "logs", "--tail", "4000", SPEC.world],
        capture_output=True,
        text=True,
        timeout=120,
    )
    # Word-precise. The first version matched a bare "ALE" and returned 25
    # lines about AiPlayerbot.botActiveAloneSmartSCALE -- a filter that answers
    # loudly about nothing is worse than one that answers nothing, because it
    # looks like evidence.
    lines = (log.stdout + log.stderr).splitlines()
    hits = [line for line in lines if _NAMES_THE_BRIDGE.search(line)]
    # A control, on the same reasoning as `party.CONTROL_MARKER`: an empty hit
    # list means "the engine is never mentioned" only if the log was read at
    # all. `playerbots` is in every start of this server.
    control = sum(1 for line in lines if "playerbots" in line.lower())
    text += (
        f"\n\nTHE WORLDSERVER'S OWN LOG ({len(lines)} lines read; "
        f"{control} of them name playerbots, so the log WAS read):\n"
        "every line naming the bridge, the Lua engine or the script directory --\n"
    )
    text += "\n".join(f"   {line}" for line in hits[-25:]) or "   (there is no such line)"
    OUT.mkdir(exist_ok=True)
    (OUT / "3-after.txt").write_text(text)
    print(text)


def stage_console() -> None:
    """The same question through the ATTACH CONSOLE, for the log line the box asks for.

    SOAP writes no log line. `ACSoap` logs the command it received at
    `LOG_DEBUG("network.soap", ...)`, which this install does not have on, and
    the answer goes back down the socket -- so the fault text captured by
    `stage_ground` is the server's own words but not the server's own LOG.

    The console is the same `ChatHandler` with `IsConsole()` true, reached the
    way the Console tab reaches it, and everything it prints lands in `docker
    logs`. So the bridge command typed there produces exactly the artefact the
    box names, in the worldserver's own file, with a timestamp.
    """
    say("8.6: typing the bridge command at the worldserver's own console, for its own log line")
    before = subprocess.run(
        ["docker", "logs", "--tail", "5", SPEC.world], capture_output=True, text=True, timeout=60
    )
    marker = f"# dml-gate-86 {stamp()}"
    reply = console.send_command(party.PROBE_COMMAND, container=SPEC.world, window=12.0)
    after = subprocess.run(
        ["docker", "logs", "--tail", "40", SPEC.world], capture_output=True, text=True, timeout=60
    )
    tail = (after.stdout + after.stderr).splitlines()
    hits = [line for line in tail if party.PROBE_COMMAND in line] or [
        "   (the console echo did not reach the log)"
    ]
    text = "\n".join(
        [
            f"=== THE CONSOLE, AND THE WORLDSERVER'S OWN LOG  {stamp()} ===",
            f"marker      : {marker}",
            "",
            f"typed       : {party.PROBE_COMMAND}",
            f"prompted    : {reply.prompted}",
            f"the console answered:",
            *[f"   {line}" for line in reply.lines],
            "",
            "THE SAME EXCHANGE IN `docker logs` -- the worldserver's own file:",
            *[f"   {line}" for line in hits],
            "",
            "the five log lines before it, to show the log was moving:",
            *[f"   {line}" for line in (before.stdout + before.stderr).splitlines()[-5:]],
        ]
    )
    OUT.mkdir(exist_ok=True)
    (OUT / "4-console.txt").write_text(text)
    print(text)


STAGES = {
    "ground": stage_ground,
    "deploy": stage_deploy,
    "after": stage_after,
    "console": stage_console,
}


def main() -> int:
    if len(sys.argv) != 2 or sys.argv[1] not in STAGES:
        print(__doc__)
        return 2
    STAGES[sys.argv[1]]()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
