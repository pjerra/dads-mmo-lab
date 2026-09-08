"""My Party's server-side bridge: deploy it, then ask whether it arrived (8.6).

## The route, and why it needs a bridge at all

`.playerbots bot addclass <class>` builds the party. It is declared
`SEC_PLAYER, Console::No` (`mod-playerbots PlayerbotCommandScript.cpp:36`, read
on `yulon-ubuntu` 2026-09-08 at `b949b50b`), and AzerothCore filters its command
table by `IsInvokerVisible(handler)` before it walks it. So a console or SOAP
caller cannot see the subcommand, never mind run it — asked over SOAP that day,
`playerbots bot addclass mage` came back with a USAGE list naming only the four
`Console::Yes` siblings. It also resolves its master from a **live session**, so
there is no console spelling of it waiting to be found.

The route that remains is a Lua script inside the world process: ALE's
`PLAYER_EVENT_ON_COMMAND` hook hands a script every command string, including
the ones the command table refused, and `Player:RunCommand` runs the playerbot
command **as that player**, in the session it needs. That is the project's own
bridge, and the scripts under `pylauncher/lua/party/` are it.

## Why this module is mostly about proving the bridge is there

Deploying the scripts is a directory copy and it always works. Whether the
server ever READS them has four independent ways to be false, and on
2026-08-20 one of them was: the deploy reported success and every bridge command
answered *"Command does not exist"*. That failure is the reason this box exists,
so the shape of this module is: gather facts, say which precondition is unmet in
its own words, and prove arrival **by the script answering** rather than by the
copy succeeding.

The four ways, each with its own sentence in `preconditions()`:

1. **The Lua engine is not compiled into the image.** A conf file cannot see
   this and neither can a module marker — `modules/mod-ale` can be cloned and
   the running binary still have no Lua in it, because the module is C++ and
   arrives at a rebuild. Read from the binary (`read_engine_in_binary`).
2. **`ALE.Enabled` is off.** Its compiled default is `false`
   (`ALE src/LuaEngine/ALEConfig.cpp:20`) while the shipped conf's own comment
   says `true` — the code wins, so a conf that never sets the key runs no
   scripts at all.
3. **`ALE.ScriptPath` points elsewhere.** It ships as `"lua_scripts"`, relative,
   resolved against the worldserver's cwd where nothing is. This is the
   2026-08-20 bug by name (`rust-main:crates/dml-wow/src/bridge.rs:189-195`).
4. **The world has not restarted since the copy.** ALE reads the directory when
   the engine starts, which is why `deploy()` reports `changed`.

## What this module deliberately does not do

It does not write `mod_ale.conf`. The Rust launcher's `ensure_ale_conf` did, and
it was right for a launcher whose installer created that file; here the file
belongs to `mod-ale.json`, and two writers with different ideas of a working
conf is exactly the drift `style-guide` §4 forbids. The manifest was corrected
instead — it named `ALE.EnableLuaEngine`, a key that appears in neither the
module's `conf/mod_ale.conf.dist` nor its `ALEConfig.cpp`.
"""

from __future__ import annotations

import shutil
import subprocess
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path

from yulon import platform, resources, runner
from yulon.channel import Answer
from yulon.log import get_logger

logger = get_logger(__name__)

BRIDGE_SCRIPTS = (
    "dml_addclass.lua",
    "dml_bridge_ping.lua",
    "dml_login.lua",
    "dml_uninvite.lua",
    "dml_whisper.lua",
)
"""Every script the bridge is made of. All of them, or the bridge is partial."""

PROBE_COMMAND = "dml_bridge_ping"
PROBE_TOKEN = "DML-BRIDGE-READY"
"""The word only `dml_bridge_ping.lua` says. See its header for why not "ok"."""

MOD_ALE_REV = "319f43edd58ffa6ed72873ddd68820ab86e4a99b"
"""`azerothcore/mod-ale` at HEAD of 2026-09-07, the revision whose
`conf/mod_ale.conf.dist` and `src/LuaEngine/ALEConfig.cpp` this module's facts
were read from on 2026-09-08. It is a pin rather than a branch because the
module moved from `c3de7942` (HEAD 2026-09-06, `phase8-delta.md:18`) to this in
two days, and the manifest names the source a two-hour rebuild compiles."""

ALE_SCRIPT_PATH = "/azerothcore/env/dist/etc/modules/lua_scripts"
"""Where ALE must be told to look: the container-visible path of the same
directory `dest_dir()` writes on the host. A constant of the compose layout."""

ALE_CONF = "env/dist/etc/modules/mod_ale.conf"

WORLDSERVER_BINARY = "/azerothcore/env/dist/bin/worldserver"

ALE_MARKER = "ALE.ScriptPath"
"""A string the Lua engine puts in the binary and nothing else does."""

CONTROL_MARKER = "AiPlayerbot.Enabled"
"""A string the binary MUST have, so a zero for `ALE_MARKER` can be told apart
from a reading that never happened. Both were measured on `yulon-ubuntu`
2026-09-08: `ALE.ScriptPath` 0, `AiPlayerbot.Enabled` 1."""


class NothingToDeploy(RuntimeError):
    """The deploy source held no scripts. Never reported as a successful no-op."""


@dataclass(frozen=True)
class Deployment:
    """What one deploy did. `changed` is what owes the user a world restart."""

    changed: bool
    names: tuple[str, ...]


def lua_root() -> Path:
    """The bundled deploy source: one subdirectory per bridge family."""
    return resources.lua_dir()


def dest_dir(server_dir: Path) -> Path:
    """The host side of the directory the container mounts at `ALE_SCRIPT_PATH`."""
    return server_dir / "env" / "dist" / "etc" / "modules" / "lua_scripts"


def deploy(root: Path, dest: Path) -> Deployment:
    """Copy every family's `*.lua` under `root` into the flat `dest`.

    Flat because ALE scans one directory. Content-compared rather than
    timestamped, so a redeploy of identical bytes reports `changed=False` and
    does not ask for a world restart nobody needs.

    **Finding nothing raises.** The Rust launcher shipped the other behaviour
    and documented the cost in its own source: with the source root wrong it
    emitted a success envelope for a no-op, so "Enable My Party" appeared to
    work and My Party simply did not function, with nothing pointing at the
    cause (`rust-main:crates/dml-wow/src/bridge.rs:56-70`). Both ways of finding
    nothing raise — an unreadable root, and a root holding no `.lua` at all.
    """
    dest.mkdir(parents=True, exist_ok=True)
    try:
        families = sorted(p for p in root.iterdir() if p.is_dir())
    except OSError as exc:
        raise NothingToDeploy(_nothing(root)) from exc
    copied: list[str] = []
    changed = False
    for family in families:
        for src in sorted(family.glob("*.lua")):
            target = dest / src.name
            wanted = src.read_bytes()
            if not target.is_file() or target.read_bytes() != wanted:
                shutil.copy2(src, target)
                changed = True
            copied.append(src.name)
    if not copied:
        raise NothingToDeploy(_nothing(root))
    logger.info(f"deployed {len(copied)} bridge scripts to {dest} (changed={changed})")
    # Sorted by NAME, not by the family walk that produced them: the flat
    # directory is what the engine sees, and a report ordered by a source
    # layout the destination does not have would move a script the day a
    # family is renamed.
    return Deployment(changed, tuple(sorted(copied)))


def _nothing(root: Path) -> str:
    return (
        f"no bridge scripts were found under {root}. Nothing was deployed, so the "
        "bridge is not there — this is reported rather than passed over, because a "
        "deploy that copies nothing and says it succeeded is how My Party last failed."
    )


# -- reading the binary ----------------------------------------------------


@dataclass(frozen=True)
class BinaryRead:
    """Whether the Lua engine is in the running server's own executable.

    `engine` is `None` for "could not be read", which is a third answer and not
    a quiet `False`: a wrong path, a stripped binary or a container that would
    not take an `exec` all look exactly like an absent engine to a bare grep.
    """

    engine: bool | None
    sentence: str


def read_engine_in_binary(
    container: str,
    *,
    binary: str = WORLDSERVER_BINARY,
    wsl_distro: str | None = None,
    run: Callable[..., subprocess.CompletedProcess[str]] = runner.run,
) -> BinaryRead:
    """Ask the image itself, because no conf file knows the answer.

    Owner instruction, 2026-09-08: *"Read the binary, not a conf file."* The two
    disagree in the case that matters — `mod-ale` cloned into `modules/` and the
    running worldserver still built without it, which is a rebuild away and not
    a config away.

    Two strings are counted in one pass. `ALE_MARKER` is the question;
    `CONTROL_MARKER` is a string the binary must have, and a zero there means
    the reading is worthless rather than negative.
    """
    prefix = platform.docker_prefix(wsl_distro)
    if prefix is None:
        return BinaryRead(None, "the Lua engine could not be read: there is no docker command here")
    script = (
        f'printf "ALE %s\\n" "$(grep -c -a -F "{ALE_MARKER}" "{binary}" || true)"; '
        f'printf "CONTROL %s\\n" "$(grep -c -a -F "{CONTROL_MARKER}" "{binary}" || true)"'
    )
    argv = [*prefix, "exec", container, "sh", "-c", script]
    try:
        proc = run(argv, timeout=60.0)
    except OSError as exc:
        logger.warning(f"could not read {binary} in {container}: {exc}")
        return BinaryRead(None, _unreadable("docker could not be started"))
    if proc.returncode != 0:
        detail = (proc.stderr or "").strip().splitlines()
        why = detail[-1] if detail else f"exit {proc.returncode}"
        return BinaryRead(None, _unreadable(why))
    counts = _counts(proc.stdout)
    if counts.get("CONTROL", 0) < 1:
        return BinaryRead(
            None,
            _unreadable(
                f"the reading found no {CONTROL_MARKER} either, and every build of this "
                "server has that — so what was read was not this server's executable"
            ),
        )
    if counts.get("ALE", 0) < 1:
        return BinaryRead(
            False,
            "the Lua engine is not built into this server's executable. It arrives at the "
            "next rebuild; installing the module alone does not put it there.",
        )
    return BinaryRead(True, "the Lua engine is built into this server's executable.")


def _unreadable(why: str) -> str:
    return f"the Lua engine could not be read out of this server's executable: {why}"


def _counts(stdout: str) -> dict[str, int]:
    out: dict[str, int] = {}
    for line in stdout.splitlines():
        name, _, value = line.strip().partition(" ")
        try:
            out[name] = int(value.strip())
        except ValueError:
            continue
    return out


# -- the preconditions -----------------------------------------------------


@dataclass(frozen=True)
class Facts:
    """Everything the sentences are decided from, read once and passed in.

    A dataclass rather than nine arguments so the ordering logic can be tested
    without a server, and so a fact nobody could read arrives as `None` instead
    of being flattened into a `False` that reads as an accusation.
    """

    server_installed: bool
    world_running: bool
    engine_cloned: bool
    engine_in_binary: bool | None
    conf_present: bool
    engine_enabled: bool | None
    script_path: str | None
    deployed: tuple[str, ...]
    bridge_answered: bool | None


@dataclass(frozen=True)
class Precondition:
    """One thing that must hold, and the sentence said when it does not."""

    name: str
    met: bool
    sentence: str


def preconditions(facts: Facts) -> tuple[Precondition, ...]:
    """Every precondition in the order a person would have to fix them.

    The order is the point. With the server absent everything below it is
    unreadable too, and naming the deployed scripts there would send somebody to
    a folder that cannot exist. `blocker()` reads the first unmet one.
    """
    return (
        Precondition(
            "server_installed",
            facts.server_installed,
            "this game is not installed yet, so there is no server folder to put the bridge in.",
        ),
        Precondition(
            "world_running",
            facts.world_running,
            "the world server is not running, so nothing can be asked of it and no bot can "
            "join a party.",
        ),
        Precondition(
            "engine_cloned",
            facts.engine_cloned,
            "the Lua engine (ALE) is not installed. Install it on the Modules page — My Party "
            "runs through it and there is no other route to it on this server.",
        ),
        Precondition(
            "engine_in_binary",
            facts.engine_in_binary is True,
            _binary_sentence(facts.engine_in_binary),
        ),
        Precondition(
            "conf_present",
            facts.conf_present,
            f"the Lua engine has no mod_ale.conf in {ALE_CONF}, so it is running on its "
            "compiled defaults — and the compiled default for ALE.Enabled is off.",
        ),
        Precondition(
            "engine_enabled",
            facts.engine_enabled is True,
            _enabled_sentence(facts.engine_enabled),
        ),
        Precondition(
            "script_path",
            facts.script_path == ALE_SCRIPT_PATH,
            _script_path_sentence(facts.script_path),
        ),
        Precondition(
            "deployed",
            tuple(sorted(facts.deployed)) == tuple(sorted(BRIDGE_SCRIPTS)),
            _deployed_sentence(facts.deployed),
        ),
        Precondition(
            "bridge_answered",
            facts.bridge_answered is True,
            _answered_sentence(facts.bridge_answered),
        ),
    )


def _binary_sentence(engine: bool | None) -> str:
    if engine is None:
        return (
            "whether the Lua engine is built into this server could not be read from its own "
            "executable, so My Party is not offered — the answer is unknown, not no."
        )
    # NOT "the Lua engine is installed but not built in". Every precondition
    # below the first unmet one is still shown, and that wording states as fact
    # something the precondition above it may have just denied -- printed on
    # `yulon-ubuntu` on 2026-09-08 over an install where mod-ale was not cloned
    # at all. A sentence describes its own fact and never somebody else's.
    return (
        "the Lua engine is not built into the server that is running. It is C++, so it arrives "
        "at the next rebuild of this game's image and not at the next restart."
    )


def _enabled_sentence(enabled: bool | None) -> str:
    if enabled is None:
        return (
            "the value of ALE.Enabled could not be read from mod_ale.conf, so whether the Lua "
            "engine will run any script is unknown."
        )
    return (
        "the Lua engine is switched off: ALE.Enabled is not set to 1 in mod_ale.conf. Its "
        "compiled default is off, whatever the comment beside it in the shipped file says."
    )


def _script_path_sentence(path: str | None) -> str:
    if path is None:
        return "ALE.ScriptPath could not be read from mod_ale.conf."
    if not path.startswith("/"):
        return (
            f"the Lua engine is looking for scripts in {path!r}, a relative path it resolves "
            "against the server's own working directory, where the bridge is not. It must be "
            f"{ALE_SCRIPT_PATH}."
        )
    return (
        f"the Lua engine is looking for scripts in {path!r}, which is not where the bridge was "
        f"deployed. It must be {ALE_SCRIPT_PATH}."
    )


def _deployed_sentence(deployed: tuple[str, ...]) -> str:
    missing = sorted(set(BRIDGE_SCRIPTS) - set(deployed))
    if len(missing) == len(BRIDGE_SCRIPTS):
        return "the bridge scripts are not in the server folder yet."
    return (
        "some of the bridge scripts are missing from the server folder "
        f"({', '.join(missing)}), so part of My Party would answer and part would not."
    )


def _answered_sentence(answered: bool | None) -> str:
    if answered is None:
        return (
            "the server could not be asked whether the bridge is there, so My Party is not "
            "offered — nothing here says it is missing."
        )
    # Same correction as `_binary_sentence`, same morning: this used to open
    # with "The scripts are in place", which the precondition directly above it
    # had just said they were not.
    return (
        "the bridge does not answer: this server does not know the bridge's own commands. If "
        "everything above this line is met, the world server has not read the scripts yet — "
        "restart it, and if it still does not answer, the Lua engine is not reading that "
        "directory."
    )


def blocker(facts: Facts) -> str | None:
    """The first unmet precondition's sentence, or `None` when all are met."""
    for check in preconditions(facts):
        if not check.met:
            return check.sentence
    return None


def ready(facts: Facts) -> bool:
    """True only when every precondition is met. Never a default."""
    return all(check.met for check in preconditions(facts))


# -- the proof -------------------------------------------------------------


@dataclass(frozen=True)
class Probe:
    """What the server said when asked whether the bridge is there.

    `arrived` is three-valued for the reason every answer in this phase is: a
    server that could not be reached has said nothing about its bridge, and
    reporting that as "the bridge is missing" sends somebody to reinstall a
    module over a server that is simply down.
    """

    arrived: bool | None
    sentence: str
    text: str


def read_probe(answer: Answer) -> Probe:
    """Classify one reply to `PROBE_COMMAND`.

    The discriminator was measured on `yulon-ubuntu` on 2026-09-08 and is a
    clean flip rather than a text match on a hopeful substring. With no bridge,
    `_ParseCommands` falls through to `SendErrorMessage(LANG_CMD_INVALID)`
    (`AC Chat.cpp:245`), which sets the error flag, and `ACSoap` turns that into
    a `<faultstring>`: *Command 'dml_bridge_ping' does not exist*. With the
    bridge, the hook returns false, `TryExecuteCommand` reports the command
    handled, no error flag is set, and the same buffer comes back as a
    `<result>` (`ACSoap.cpp:133-140`) — carrying whatever
    `handler:SendSysMessage` put in it.

    So a `yes` is necessary and not sufficient: it must carry the script's own
    word. A deploy reporting success is what fooled 2026-08-20, and so would
    any other command on the server answering to this name.
    """
    if answer.outcome == "unknown":
        why = answer.reason or "the server did not answer"
        return Probe(None, f"the bridge could not be asked about: {why}", answer.text)
    if answer.outcome == "no":
        if "does not exist" in answer.text:
            return Probe(
                False,
                f"the server does not know {PROBE_COMMAND}, so the bridge has not arrived. "
                "The scripts may be on disk; nothing has read them.",
                answer.text,
            )
        return Probe(
            False,
            f"the server refused {PROBE_COMMAND} in its own words: {answer.text.strip()}",
            answer.text,
        )
    if PROBE_TOKEN not in answer.text:
        return Probe(
            False,
            f"the server ran {PROBE_COMMAND} and did not answer with the bridge's own word "
            f"({PROBE_TOKEN}), so what answered was not this bridge.",
            answer.text,
        )
    return Probe(True, "the bridge answered, in its own words.", answer.text)
