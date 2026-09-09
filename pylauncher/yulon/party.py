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

import re
import shutil
import subprocess
import time
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path

from yulon import dbreads, platform, play, resources, runner
from yulon.actions import Outcome
from yulon.catalog.catalog import CatalogEntry
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
            # Every one of OURS is there — not "these files and no others".
            # `mod-ale` installs its own example `LootPet.lua` into the same
            # directory (measured on `yulon-ubuntu2` 2026-09-09, on the only
            # install where the engine has ever run), and set equality made a
            # complete bridge report as a partial one naming nothing missing.
            set(BRIDGE_SCRIPTS) <= set(facts.deployed),
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


# -- reading the facts off one install -------------------------------------


@dataclass(frozen=True)
class ConfRead:
    """`mod_ale.conf` as it is on disk, or the absence of it.

    `enabled` and `script_path` are `None` when the file is not there at all:
    a file nobody wrote has said nothing about the engine, and reporting that
    as "switched off" sends a person to edit a key that does not exist. The
    `conf_present` precondition above them is what names the real fix.
    """

    present: bool
    enabled: bool | None
    script_path: str | None


def read_conf(path: Path) -> ConfRead:
    """Read both of ALE's keys out of one conf file, column 0 only.

    Column 0 is `conf.patch()`'s rule and it is here for the reason that rule
    exists: the shipped `mod_ale.conf.dist` carries a commented `ALE.Enabled =
    true` beside a compiled default of `false` (`ALEConfig.cpp:20`), so a
    pattern that matches indented or commented lines reads the file's own
    prose as its settings and reports an engine that is off as on.
    """
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ConfRead(False, None, None)
    enabled = _conf_value(text, "ALE.Enabled")
    script = _conf_value(text, "ALE.ScriptPath")
    return ConfRead(True, None if enabled is None else enabled.strip() == "1", script)


def _conf_value(text: str, key: str) -> str | None:
    """The LAST active setting of `key`, unquoted, or `None`.

    The last rather than the first: a conf file read top to bottom by the
    server takes the last assignment, and an applier that appends a corrected
    key leaves the old one above it.
    """
    found: str | None = None
    for line in text.splitlines():
        head, sep, tail = line.partition("=")
        if sep and head.strip() == key and head[:1] not in ("#", " ", "\t"):
            found = tail.strip().strip('"')
    return found


def read_facts(
    server_dir: Path,
    *,
    world_running: bool,
    engine: BinaryRead,
    probe: Probe,
) -> Facts:
    """Every fact the sentences are decided from, read once off one install.

    The two answers this cannot get from the disk are passed in, because both
    cost a subprocess and the caller is what knows whether it may spend one:
    `engine` comes from `read_engine_in_binary` and `probe` from asking the
    SERVER `dml_bridge_ping`. Asking the server is not decoration — the deploy
    reporting success is precisely what was believed on 2026-08-20, and the
    scripts were on disk that day too.
    """
    installed = server_dir.is_dir()
    conf = read_conf(server_dir / ALE_CONF)
    try:
        deployed = tuple(sorted(p.name for p in dest_dir(server_dir).glob("*.lua")))
    except OSError:
        deployed = ()
    return Facts(
        server_installed=installed,
        world_running=world_running,
        engine_cloned=(server_dir / "modules" / "mod-ale").is_dir(),
        engine_in_binary=engine.engine,
        conf_present=conf.present,
        engine_enabled=conf.enabled,
        script_path=conf.script_path,
        deployed=deployed,
        bridge_answered=probe.arrived,
    )


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


# -- the party itself ------------------------------------------------------
#
# Everything above proves the bridge is there. This is what the bridge is FOR:
# a bot of a chosen class in the party frame, geared and specced, and gone
# again on request.
#
# The command strings are `rust-main`'s, cited at their lines, because the Lua
# on the other side of the wire is the same Lua and a second spelling of
# `dml_addclass` would be a second protocol. The class LIST is not inherited:
# `dk` is a tenth class this tree's `addclass` accepts (`mod-playerbots
# PlayerbotMgr.cpp:1089`, read on `yulon-ubuntu` 2026-09-08, gated on the
# master reaching the heroic start level at `:1156`), and the bash launcher's
# `_valid_bot_class` deliberately excluded it. That was its tree's fact.


class BadRequest(ValueError):
    """A class or a name that must never reach the channel.

    Raised rather than returned, and raised BEFORE anything is sent: these
    strings are interpolated into a command line the world server parses, so
    the check is a boundary and not a courtesy.
    """


BOT_CLASS_IDS = {
    "warrior": 1,
    "paladin": 2,
    "hunter": 3,
    "rogue": 4,
    "priest": 5,
    "shaman": 7,
    "mage": 8,
    "warlock": 9,
    "druid": 11,
    "dk": 6,
}
"""Every class this tree's `addclass` takes, and the id the module gives it.

Both halves come from the same forty lines: `mod-playerbots
PlayerbotMgr.cpp:1093-1134`, read on `yulon-ubuntu2` on 2026-09-09. The ids are
not decoration — the premade specs are keyed by class NUMBER
(`AiPlayerbot.PremadeSpecName.<class>.<specno>`), so a spec picker has to turn
the word a person chose into the module's own number, and there is exactly one
place that mapping may live. See the note above for `dk`, which is 6 and which
the bash launcher's list deliberately excluded."""

BOT_CLASSES = tuple(BOT_CLASS_IDS)
"""The classes THIS tree's `addclass` accepts, derived rather than written twice.

A second tuple beside the mapping is a tuple that will one day offer a class the
specs are not keyed by, which is a picker offering a class whose spec list is
silently empty."""

PLAYERBOTS_CONF = "env/dist/etc/modules/playerbots.conf"
"""The module conf the premade spec names are read out of, `.dist` included.

The prior art reads the deployed file and falls back to the shipped `.dist`
(`rust-main:cli/src/50-party.sh:137-145`), and so does the server: on
`yulon-ubuntu2` on 2026-09-09 there is a `playerbots.conf.dist` and no
`playerbots.conf`, so a reader that looked only for the deployed name would
offer no specs at all on the one install where My Party has ever worked."""

SPEC_NAME_KEY = "AiPlayerbot.PremadeSpecName."

WORLD_CONF = "env/dist/etc/worldserver.conf"
MAX_LEVEL_KEY = "MaxPlayerLevel"
"""Where this install's own level cap is, and the key that holds it.

`MaxPlayerLevel = 80` on `yulon-ubuntu2` (`worldserver.conf:2128`, 2026-09-09).
Eighty is nowhere in this app: it is one conf value on one install of one tree,
and a constant would silently mis-bound the fork that ships 60 or 255."""

SPEC_SHAPE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9 ._-]*$")
"""What may go in the tail of `dml_whisper <master> <bot> talents spec <name>`.

The prior art's charset (`50-party.sh:208-215`) and deliberately WIDER than the
shipped names' lowercase-and-spaces: `playerbots.conf` is hand-editable, the
picker offers whatever it says, and refusing `Arms PvE` here would refuse a name
the module accepts. What it must never admit is a quote, a backslash, a newline
or anything else the world server's parser would read as a second command."""

MAX_NAME = 12
"""A WoW character name's own limit. `characters.name` is `varchar(12)`."""

POLL_TRIES = 12
POLL_SLEEP = 0.5
"""The poll window a bot has to appear in: 12 reads half a second apart, six
seconds. The prior art's numbers (`rust-main:crates/dml-wow/src/party.rs:321`
and `:327`, both env-overridable there) rather than a guess of our own; what
this tree actually took is recorded in 8.6's gate folder beside the press."""


MASTER = "character's"
BOT = "bot's"
"""What a rejected name is called in its own refusal, so "Invalid bot name" is
not said about the master. The prior art keeps two error builders for the same
reason (`rust-main:.../party.rs:138,143`)."""


def valid_name(name: str) -> bool:
    """A character name, and nothing that is also a command separator."""
    return 1 <= len(name) <= MAX_NAME and name.isalpha()


def _check_name(name: str, what: str) -> str:
    if not valid_name(name):
        raise BadRequest(
            f"{name!r} is not a {what} name: a character's name is 1 to {MAX_NAME} letters, and "
            "this one goes into a command the world server parses."
        )
    return name


def add_command(player: str, klass: str, *, gender: str = "") -> str:
    """`dml_addclass <player> <class> [gender]`
    (`rust-main:crates/dml-wow/src/party.rs:163`).

    NOT `.playerbots bot addclass`. That one is `SEC_PLAYER, Console::No` and a
    console or SOAP caller cannot even see it — sending it over this channel is
    the 2026-08-20 failure spelled out by hand. The bridge exists to run it
    inside the master's own session; see this module's header.
    """
    _check_name(player, MASTER)
    if klass.strip().lower() not in BOT_CLASSES:
        raise BadRequest(
            f"{klass!r} is not a class this server's addclass accepts. It has: "
            f"{', '.join(BOT_CLASSES)}."
        )
    if gender not in ("", "male", "female"):
        raise BadRequest(f"{gender!r} is not a gender: it is male, female, or left out.")
    tail = f" {gender}" if gender else ""
    return f"dml_addclass {player} {klass.strip().lower()}{tail}"


def uninvite_command(bot: str) -> str:
    """`dml_uninvite <bot>` (`rust-main:.../party.rs:184`)."""
    return f"dml_uninvite {_check_name(bot, BOT)}"


def logout_command(player: str, bot: str) -> str:
    """The logout whisper that follows an uninvite (`.../party.rs:192`).

    Best-effort at every call site in the prior art and here: an uninvited bot
    has already left the party, and a whisper that does not land leaves it
    standing in the world rather than leaving it in the group.
    """
    return f"{_whisper(player, bot)} logout"


def autogear_command(player: str, bot: str) -> str:
    """`.../party.rs:249`. "Geared" is this whisper, not `addclass`."""
    return f"{_whisper(player, bot)} autogear"


def talents_command(player: str, bot: str) -> str:
    """`.../party.rs:246`. "Specced" is this whisper, not `addclass`."""
    return f"{_whisper(player, bot)} talents autopick"


def valid_spec(spec: str) -> bool:
    """A premade spec name that cannot break out of the whisper it goes into."""
    return bool(SPEC_SHAPE.match(spec))


def spec_command(player: str, bot: str, spec: str) -> str:
    """`dml_whisper <master> <bot> talents spec <name>`
    (`rust-main:crates/dml-wow/src/party.rs:253`, `cli/src/90-main.sh:3976`).

    The whisper `talents autopick` REPLACES rather than joins. Measured in the
    module (`ChangeTalentsAction.cpp:57-59` and `:146`, read on `yulon-ubuntu2`
    2026-09-09): `autopick` runs `InitTalentsTree(true)` and a chosen spec runs
    `InitTalentsBySpecNo`, so sending both leaves the bot with whichever went
    last. `add_bot` sends one or the other and never the pair.
    """
    if not valid_spec(spec):
        raise BadRequest(
            f"{spec!r} is not a premade spec name: it goes into the tail of a command the world "
            "server parses, so it may hold only letters, digits, spaces and . _ -"
        )
    return f"{_whisper(player, bot)} talents spec {spec}"


def read_spec_names(text: str) -> dict[int, tuple[str, ...]]:
    """Every premade spec this conf defines, per class id, in the module's order.

    Two measured rules, both from `mod-playerbots` on `yulon-ubuntu2` 2026-09-09,
    and neither of them obvious from the file alone:

    * **The conf IS the accepted list.** `PlayerbotAIConfig.cpp:487-493` loads
      `AiPlayerbot.PremadeSpecName.<class>.<specno>` and `SpecPick` compares the
      whispered text to those values with `==`
      (`ChangeTalentsAction.cpp:144`). A list kept by hand in this app could only
      ever drift from it, and the drift is invisible: a name the module does not
      have is answered `Spec <x> not found` in the GAME window
      (`:157`, through `TellMasterNoFacing`), so nothing comes back over the
      channel and a wrong spec looks exactly like a right one.
    * **A gap ends the class.** `SpecPick` walks specno upwards from 0 and
      **breaks** at the first empty name (`:138-142`), so a conf that defines
      8.0, 8.1 and 8.3 has an unreachable 8.3 whatever it says. Only the
      contiguous run from 0 is returned — offering the rest would be offering
      names this server cannot be made to take.

    Column 0 only, which is `read_conf`'s rule for `read_conf`'s reason: the
    shipped file carries commented keys and a pattern that matched them would
    read the file's prose as its settings.

    **A value is taken verbatim to the end of the line, `#` included**, and
    whether the core's own reader would drop a trailing comment is NOT measured
    on this tree — so `… = arcane pve # the good one` is read here as the whole
    string, which is very probably not what the module has. What that costs is
    bounded, and it is bounded the safe way round: `#` is outside `SPEC_SHAPE`,
    so `InstallParty.specs` filters such a name out and the picker never offers
    it. The result of the unmeasured case is a spec missing from the list, not a
    spec offered that the server would refuse in the game window where nothing
    can hear it. Measuring `sConfigMgr`'s comment handling is what would let this
    read the value the module actually holds.
    """
    seen: dict[int, dict[int, str]] = {}
    for line in text.splitlines():
        head, sep, tail = line.partition("=")
        if not sep or head[:1] in ("#", " ", "\t"):
            continue
        key = head.strip()
        if not key.startswith(SPEC_NAME_KEY):
            continue
        parts = key[len(SPEC_NAME_KEY) :].split(".")
        if len(parts) != 2 or not all(part.isdigit() for part in parts):
            continue
        value = tail.strip().strip('"')
        if value:
            seen.setdefault(int(parts[0]), {})[int(parts[1])] = value
    out: dict[int, tuple[str, ...]] = {}
    for klass, numbered in seen.items():
        run: list[str] = []
        number = 0
        while number in numbered:
            run.append(numbered[number])
            number += 1
        out[klass] = tuple(run)
    return out


def spec_names(server_dir: Path) -> dict[int, tuple[str, ...]]:
    """`read_spec_names` over this install's own `playerbots.conf`."""
    return read_spec_names(_conf_text(server_dir / PLAYERBOTS_CONF))


def max_player_level(server_dir: Path) -> int | None:
    """This install's own `MaxPlayerLevel`, or `None` for "nobody could read it".

    `None` rather than the compiled default, and that is a deliberate refusal
    rather than an omission: AzerothCore's built-in cap has not been measured on
    this tree, and answering 80 for a conf that never sets the key would be this
    app inventing a fact about somebody's fork — the same mistake `ALE.Enabled`'s
    shipped comment makes about ITS compiled default. A cap nobody could read
    means no level is offered, and the sentence says which key to set.
    """
    text = _conf_text(server_dir / WORLD_CONF)
    value = _conf_value(text, MAX_LEVEL_KEY)
    return int(value) if value is not None and value.isdigit() else None


def _conf_text(path: Path) -> str:
    """A conf file's text, falling back to the shipped `.dist` beside it."""
    for candidate in (path, Path(f"{path}.dist")):
        try:
            return candidate.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
    return ""


def _whisper(player: str, bot: str) -> str:
    """`dml_whisper <master> <bot>`, with BOTH names checked.

    One place, because the three whispers differ only in their tail and a name
    check spelled three times is a name check that will one day be spelled
    twice.
    """
    return f"dml_whisper {_check_name(player, MASTER)} {_check_name(bot, BOT)}"


@dataclass(frozen=True)
class Member:
    """One bot in the master's party, as the group table has it."""

    name: str
    guid: int
    klass: int
    level: int


def group_rows_sql(entry: CatalogEntry, marker: dbreads.Marker, *, master_guid: int) -> str:
    """The bot members of the group `master_guid` is in.

    Two things this query is careful about, and both are the reason it is not
    `SELECT * FROM group_member`:

    * **`group_member.guid` is the GROUP, `memberGuid` is the member** on this
      tree (`describe group_member`, `yulon-ubuntu2` 2026-09-08). The sub-select
      turns the master's guid into his group's id; without it this reads every
      group on the server.
    * **The marker's clause decides which rows are bots.** The master has a
      `group_member` row of his own, and a read that returned him would report a
      party of one before any bot joined. `dbreads.bot_clause` is the same
      two-armed clause the Bots tab counts with — registry OR account prefix —
      so a party of bots cannot read back as empty the way `preset-save`'s
      registry-only version did (`rust-main:.../party.rs:277`).
    """
    schemas = entry.schema_map()
    chars = schemas["characters"]
    clause = dbreads.bot_clause(entry, marker)
    return (
        "SELECT c.name, c.guid, c.class, c.level "
        f"FROM {chars}.group_member gm "
        f"JOIN {chars}.characters c ON c.guid = gm.memberGuid "
        f"WHERE gm.guid = (SELECT guid FROM {chars}.group_member "
        f"WHERE memberGuid = {int(master_guid)} LIMIT 1) "
        f"AND ({clause}) "
        "ORDER BY c.name"
    )


def read_members(raw: str) -> tuple[Member, ...] | str:
    """The group read's rows, or the sentence saying why they are not rows.

    A row that does not parse is REPORTED and not skipped: a party silently one
    bot short reads exactly like a party that never filled, and telling those
    two apart is the only thing this whole module is for.
    """
    out: list[Member] = []
    for line in raw.splitlines():
        if not line.strip():
            continue
        fields = line.split("\t")
        if len(fields) != 4 or not all(f.strip().lstrip("-").isdigit() for f in fields[1:]):
            return f"a party row came back as {line.strip()!r}, which is not a group member"
        out.append(
            Member(
                name=fields[0].strip(),
                guid=int(fields[1]),
                klass=int(fields[2]),
                level=int(fields[3]),
            )
        )
    return tuple(out)


@dataclass(frozen=True)
class Addition:
    """What one press of "add a bot" did, in four states rather than two.

    `added` and `joined` are separate because the server can accept the command
    and the bot can still not be in the party six seconds later: that is a
    third outcome, and reporting it as success is what "My Party works" used to
    mean. `blocker` is set only when nothing was sent at all.
    """

    added: bool
    joined: bool
    bot: str | None
    geared: bool
    specced: bool
    sentence: str
    blocker: str = ""
    spec: str = ""
    """The premade spec asked for, or "" for "let the server pick".

    `specced` is what happened to it, and it means the same thing either way: the
    WHISPER was accepted. It is never "the talents are now this", because the
    module answers a spec only in the game window and nothing this app can read
    sees that answer."""
    level: int | None = None
    level_before: int | None = None
    level_after: int | None = None
    """The level asked for, and `characters.level` on either side of the press.

    Both readings come from the group query this press already makes, so the
    "after" is the database and not the level command's own `yes`. `before` is
    what makes a chosen level provable: a bot the server already made at 60,
    asked for 60, has proved nothing, and `_level_note` says so rather than
    reporting a change that did not happen."""


LevelSetter = Callable[[str, int], Outcome]
"""How a level is set: `play.InstallPlay.set_level`, and never a second path.

A bot is a character and 8.4a already owns that command — including the refusal,
in the entry's own measured words, on the trees whose console has no route to a
level. A seam here rather than an import at the call site for the same reason
`engine` is one: the tests for these sentences must not be tests of a console."""


def add_bot(
    *,
    facts: Facts,
    player: str,
    klass: str,
    send: Callable[[str], Answer],
    members: Callable[[], tuple[Member, ...]],
    gender: str = "",
    spec: str = "",
    specs: tuple[str, ...] = (),
    level: int | None = None,
    max_level: int | None = None,
    set_level: LevelSetter | None = None,
    tries: int = POLL_TRIES,
    pause: float = POLL_SLEEP,
    sleep: Callable[[float], None] = time.sleep,
) -> Addition:
    """Add one bot of `klass` to `player`'s party, and say what actually happened.

    **The ground is read before the press.** `members()` is called once before
    the command goes out and the new bot is the guid that was not in that
    reading — not "there is a bot in the party", which is already true on a
    server where one is. A poll whose assertion holds before its action is a
    poll that proves nothing, and 500 of the characters on this install are
    bots.

    **Nothing is sent while a precondition is unmet.** `blocker()` names the
    first one in its own words and this returns without touching the channel.
    That is the whole difference from 2026-08-20, when a control pressed
    happily into a bridge that was not there and reported success.

    **A spec and a level are checked before the join, not after it.** Both
    refusals are `blocker`s — nothing at all was sent — because the alternative
    is a bot standing in the party that the press then refuses to finish. The
    order once the bot IS there is level, then spec, then gear, and each step is
    a measurement rather than a preference: `SpecPick` applies the premade build
    through `PlayerbotFactory factory(bot, bot->GetLevel())`
    (`ChangeTalentsAction.cpp:146-149`), so a spec chosen before the level is a
    level-1 build on a level-60 bot; and `autogear` follows the spec because gear
    must match the new talents (`90-main.sh:3975-3977`, whose own comment says
    so).
    """
    stop = blocker(facts)
    if stop is not None:
        return Addition(False, False, None, False, False, stop, blocker=stop)
    command = add_command(player, klass, gender=gender)
    if spec:
        refusal = _spec_refusal(spec, klass, specs)
        if refusal:
            return Addition(False, False, None, False, False, refusal, blocker=refusal, spec=spec)
    if level is not None:
        if set_level is None:
            raise BadRequest(
                "a level was asked for with no way to set one. A level goes through the "
                "Characters tab's own seam and this call was handed none."
            )
        refusal = _level_refusal(level, max_level)
        if refusal:
            return Addition(False, False, None, False, False, refusal, blocker=refusal, level=level)
    before = {member.guid for member in members()}
    answer = send(command)
    if answer.outcome != "yes":
        said = (answer.text or answer.reason).strip()
        return Addition(
            False,
            False,
            None,
            False,
            False,
            f"the server did not run {command!r}: {said}",
        )
    joined: Member | None = None
    for attempt in range(tries):
        if attempt:
            sleep(pause)
        joined = next((m for m in members() if m.guid not in before), None)
        if joined is not None:
            break
    if joined is None:
        window = tries * pause
        return Addition(
            True,
            False,
            None,
            False,
            False,
            f"the server accepted the command and no bot joined the party within {window:g} "
            "seconds. It may still arrive; nothing here says it will.",
        )
    level_before = joined.level
    level_after: int | None = level_before
    level_problem = ""
    if level is not None and set_level is not None:
        outcome = set_level(joined.name, level)
        if not outcome.done:
            level_problem = (outcome.problem or outcome.text).strip()
        after = next((row for row in members() if row.guid == joined.guid), None)
        level_after = None if after is None else after.level
    if spec:
        # One or the other, never both: see `spec_command`'s docstring for the
        # measurement. The gear follows, because it must match the new talents.
        specced = send(spec_command(player, joined.name, spec)).outcome == "yes"
        geared = send(autogear_command(player, joined.name)).outcome == "yes"
    else:
        # Byte-identical to the pair that was proved live on 2026-09-09. The
        # prior art keeps its own no-spec branch unchanged for the same reason
        # (`90-main.sh:3973`): a reorder nobody measured is a change to a
        # working path.
        geared = send(autogear_command(player, joined.name)).outcome == "yes"
        specced = send(talents_command(player, joined.name)).outcome == "yes"
    return Addition(
        True,
        True,
        joined.name,
        geared,
        specced,
        f"{joined.name} joined the party"
        + _finish(geared, specced, spec)
        + _level_note(level, level_before, level_after, level_problem),
        spec=spec,
        level=level,
        level_before=level_before,
        level_after=level_after,
    )


def _spec_refusal(spec: str, klass: str, specs: tuple[str, ...]) -> str:
    """Why this spec was not sent, or "" for one this server has.

    Refused HERE rather than by the server, because the server does not refuse
    it anywhere this app can hear: `Spec <x> not found` goes to the master's
    chat window (`ChangeTalentsAction.cpp:157`) and the channel sees a command
    that ran. An unchecked spec is a bot that is silently not specced under a
    sentence saying it is.
    """
    if not valid_spec(spec):
        return (
            f"{spec!r} is not a premade spec name — it may hold only letters, digits, spaces "
            "and . _ - because it goes into a command the world server parses. Nothing was sent."
        )
    if not specs:
        return (
            f"this install's {PLAYERBOTS_CONF} lists no premade specs for {klass}, so there is "
            "no spec to ask for and nothing was sent. The module answers an unknown spec only "
            "in the game window, so a spec sent on a guess would look exactly like one that "
            "worked."
        )
    if spec not in specs:
        return (
            f"{spec!r} is not one of the premade specs this server has for {klass}: "
            f"{', '.join(specs)}. Nothing was sent."
        )
    return ""


def _level_refusal(level: int, max_level: int | None) -> str:
    """Why this level was not sent, or "" for one inside this server's own range."""
    if max_level is None:
        return (
            f"this server's own top level could not be read: {MAX_LEVEL_KEY} is not set in "
            f"{WORLD_CONF}, so there is nothing to bound a level by and nothing was sent."
        )
    if not 1 <= level <= max_level:
        return (
            f"level {level} is outside 1 to {max_level}, which is this server's own "
            f"{MAX_LEVEL_KEY}. Nothing was sent."
        )
    return ""


def _finish(geared: bool, specced: bool, spec: str = "") -> str:
    if spec:
        caveat = (
            " The module answers a spec only in the game window, so that is the whisper being "
            "accepted and not the talents changing."
        )
        if specced and geared:
            return f", geared, and asked for the {spec} spec.{caveat}"
        if specced:
            return (
                f", and asked for the {spec} spec. The autogear whisper was refused, so it is "
                f"not geared.{caveat}"
            )
        if geared:
            return (
                f", geared. The {spec} spec whisper was refused, so its talents are whatever "
                "the server picked."
            )
        return f". Neither the {spec} spec whisper nor the autogear whisper was accepted."
    if geared and specced:
        return ", geared and specced."
    if geared:
        return ", geared. The talents whisper was refused, so it is not specced."
    if specced:
        return ", specced. The autogear whisper was refused, so it is not geared."
    return ". Neither the autogear nor the talents whisper was accepted."


def _level_note(level: int | None, before: int | None, after: int | None, problem: str) -> str:
    """What the chosen level did, read out of `characters.level` on both sides.

    The "already" arm is the gate rule in the app's own voice: a step whose
    assertion was true before its action has proved nothing, and a bot the server
    happened to make at the level that was asked for is exactly that step.
    """
    if level is None:
        return ""
    if problem:
        return f" The level was not set: {problem}"
    if before == level:
        return (
            f" characters.level already read {level} before the press, so nothing about the "
            "level changed and this says nothing about whether it would have."
        )
    if after is None:
        return (
            " The level was sent and this bot is no longer in the group table, so no level "
            "could be read back."
        )
    if after != level:
        return (
            f" The level command was accepted and characters.level still reads {after}, "
            f"not {level}."
        )
    return f" characters.level read {before} before the press and {after} after."


@dataclass(frozen=True)
class Dismissal:
    """What one press of "dismiss" did. `removed` is read back, not assumed."""

    removed: bool
    logged_out: bool
    sentence: str
    bot: str = ""
    """Which bot this is about, so a list of them needs no parallel list of names.

    Defaulted rather than positional so the field could be added without moving
    the three that were already there."""


def dismiss(
    *,
    player: str,
    bot: str,
    send: Callable[[str], Answer],
    members: Callable[[], tuple[Member, ...]],
    tries: int = POLL_TRIES,
    pause: float = POLL_SLEEP,
    sleep: Callable[[float], None] = time.sleep,
) -> Dismissal:
    """Uninvite `bot`, whisper it to log out, and read the group table back.

    The whisper is best-effort, as it is at every call site in the prior art
    whose own comment says its failure "never aborts the caller"
    (`rust-main:.../party.rs:189-192`): the uninvite is what empties the row,
    and a bot that stays logged in is standing in the world, not in the group.

    `removed` is the group table read AFTER, never the uninvite's own `yes`.
    """
    answer = send(uninvite_command(bot))
    if answer.outcome != "yes":
        said = (answer.text or answer.reason).strip()
        return Dismissal(False, False, f"the server did not uninvite {bot}: {said}", bot=bot)
    logged_out = send(logout_command(player, bot)).outcome == "yes"
    for attempt in range(tries):
        if attempt:
            sleep(pause)
        if all(member.name != bot for member in members()):
            return Dismissal(
                True,
                logged_out,
                f"{bot} left the party" + ("." if logged_out else ", and is still logged in."),
                bot=bot,
            )
    window = tries * pause
    return Dismissal(
        False,
        logged_out,
        f"the server accepted the uninvite and {bot} is still in the group table "
        f"{window:g} seconds later.",
        bot=bot,
    )


@dataclass(frozen=True)
class MassDismissal:
    """What one press of "dismiss all" did, per bot rather than as a count.

    `dismissals` is every attempt in the order it was made, each carrying its own
    `removed` and its own sentence, because a count is a report nobody can check
    against the party frame in front of them — and because one bot that refuses
    must not hide the ones that went. `blocker` means the channel was never
    touched, which is this module's word for it everywhere else.
    """

    attempted: int
    dismissals: tuple[Dismissal, ...]
    sentence: str
    blocker: str = ""


def dismiss_all(
    *,
    player: str,
    bots: tuple[str, ...],
    send: Callable[[str], Answer],
    members: Callable[[], tuple[Member, ...]],
    tries: int = POLL_TRIES,
    pause: float = POLL_SLEEP,
    sleep: Callable[[float], None] = time.sleep,
) -> MassDismissal:
    """`dismiss` per bot, and every one of them attempted.

    The prior art's own review finding, in both directions: one unreachable bot
    must not strand the rest of the party (`90-main.sh:4076`), and an attempt is
    not a dismissal (`:4079-4083` — SOAP down with the database up used to report
    "dismissed: N" while every bot was still in the group). Here the second half
    is free, because `dismiss` reads the group table back rather than trusting
    the uninvite's `yes`.

    It costs what that readback costs: a bot whose row does not clear polls for
    the full window before it is reported as still there.
    """
    if not bots:
        stop = (
            f"{player}'s party has no bots in it, so there is nothing to dismiss and nothing "
            "was sent."
        )
        return MassDismissal(0, (), stop, blocker=stop)
    done = tuple(
        dismiss(
            player=player,
            bot=bot,
            send=send,
            members=members,
            tries=tries,
            pause=pause,
            sleep=sleep,
        )
        for bot in bots
    )
    return MassDismissal(len(done), done, _mass_sentence(done))


def _mass_sentence(done: tuple[Dismissal, ...]) -> str:
    gone = [one.bot for one in done if one.removed]
    stayed = [one for one in done if not one.removed]
    if not stayed:
        return f"{bots_word(len(done))} left the party: {', '.join(gone)}."
    head = (
        f"{len(gone)} of {bots_word(len(done))} left the party: {', '.join(gone)}. "
        if gone
        else f"None of {bots_word(len(done))} left the party. "
    )
    # Every refusal in its own words. Summarising them ("2 failed") is how one
    # bot that is simply logged out reads the same as a channel that is down.
    return head + "Still here — " + "; ".join(f"{one.bot}: {one.sentence}" for one in stayed)


def bots_word(count: int) -> str:
    """ "1 bot" or "N bots", in one place.

    Public because the panel says the same thing on its armed button, and the
    live gate found "1 bots" in this feature's own summary line once already
    (`test_one_bot_is_a_bot_and_not_one_bots`)."""
    return "1 bot" if count == 1 else f"{count} bots"


# -- the tab's seam --------------------------------------------------------


SqlReader = dbreads.SqlReader
"""The database read seam, and it is `dbreads`' own rather than a second one.

A Protocol re-declared here with `db: str` looks identical and is not: the real
reader's `db` is a `Literal` of the five schema aliases, and a Protocol widening
it to `str` is not satisfied by the object every caller actually has -- mypy
said so on all three platforms, and the wrong fix was to widen the real one.
The read half is all that is wanted (`run_statement` is not reachable through
it), which is exactly what `dbreads.SqlReader` already is."""


@dataclass(frozen=True)
class PartyState:
    """What the My Party group shows: the rows, or which precondition stopped it.

    `ready` is never a default. `checks` carries ALL nine rather than only the
    blocking one, because a person fixing this wants to see how far down the
    list they have got — but `blocker` is the FIRST unmet one, because that is
    the only one worth acting on next.
    """

    ready: bool
    blocker: str
    checks: tuple[Precondition, ...]
    members: tuple[Member, ...] = ()
    problem: str = ""


class InstallParty:
    """One install's My Party, as the tab presses it.

    Reads go to the database and every write goes to the SERVER over the
    command channel, which is owner answer 7 and also the only route there is:
    `.playerbots bot addclass` resolves its master from a live session, so
    there is no SQL spelling of adding a bot to a party.

    **Nothing is pressed until the server has said the bridge is there.** The
    facts are re-read on every press rather than cached at start-up: the world
    can be restarted, the module reinstalled and `mod_ale.conf` edited while
    the tab is open, and a control that presses on a five-minute-old reading is
    the 2026-08-20 failure with a delay in front of it.
    """

    def __init__(
        self,
        entry: CatalogEntry,
        server_dir: Path,
        *,
        sql: SqlReader,
        channel_for_saved: Callable[[], object | None],
        container: str,
        world_running: Callable[[], bool],
        wsl_distro: str | None = None,
        engine: Callable[[], BinaryRead] | None = None,
        level_setter: LevelSetter | None = None,
    ) -> None:
        self.entry = entry
        self.server_dir = server_dir
        self._sql = sql
        self._channel_for_saved = channel_for_saved
        self._container = container
        self._world_running = world_running
        self._wsl_distro = wsl_distro
        # A seam for the same reason `world_running` is one: reading it costs a
        # `docker exec`, and the tests for every sentence this object says must
        # not be tests that need a container. The default is the real reader.
        self._engine = engine or (lambda: read_engine_in_binary(container, wsl_distro=wsl_distro))
        # 8.6/T5. The level goes through the Characters tab's own seam, built
        # here over the same four things it would be built over in the factory —
        # a bot is a character, and a second `character level` would be a second
        # place to fix the day a fork renames it. It is also the half that
        # already refuses the trees with no such command, in the entry's own
        # measured words (`play._no_set_level`), which is a sentence this module
        # would otherwise have to write about somebody else's server.
        self.level_setter: LevelSetter = (
            level_setter
            or play.InstallPlay(
                entry, server_dir, sql=sql, channel_for_saved=channel_for_saved
            ).set_level
        )

    @staticmethod
    def for_entry_is_possible(entry: CatalogEntry) -> bool:
        """Whether this tree has measured what My Party would need.

        Two things, and neither is optional: a bot marker (the group read has
        to know which rows are bots) and the `mod-ale` route itself, which is
        an AzerothCore module. A CMaNGOS tree gets no My Party control rather
        than a control that sends AzerothCore's commands at it.
        """
        return entry.observability is not None and entry.id == "wow-wotlk"

    # -- reads ---------------------------------------------------------------

    def facts(self) -> Facts:
        """The nine facts, read fresh, and the two subprocesses they cost.

        Ordered so the expensive halves are skipped once something above them
        has already decided the answer: a stopped world is asked nothing, and a
        server folder that is not there is not grepped for a Lua engine.
        """
        if not self.server_dir.is_dir() or not self._world_running():
            return read_facts(
                self.server_dir,
                world_running=self._world_running(),
                engine=BinaryRead(None, _unreadable("the world server is not running")),
                probe=Probe(None, "the server was not asked: it is not running", ""),
            )
        return read_facts(
            self.server_dir,
            world_running=True,
            engine=self._engine(),
            probe=self._probe(),
        )

    def _probe(self) -> Probe:
        channel = self._channel_for_saved()
        if channel is None:
            return Probe(
                None,
                "the bridge could not be asked about: this install has no command channel set "
                "up yet, and My Party has no other way to reach the server.",
                "",
            )
        try:
            answer = channel.send(PROBE_COMMAND)  # type: ignore[attr-defined]
        except Exception as exc:  # noqa: BLE001 - one answer for every seam failure
            logger.warning(f"could not ask {self.entry.id} about the bridge: {exc}")
            return Probe(None, f"the bridge could not be asked about: {exc}", "")
        return read_probe(answer)

    def state(self, master: str) -> PartyState:
        """The group, or the first precondition that is not met.

        The rows are read only when everything above them is met. A group table
        drawn under an unmet precondition would be an empty list beside a
        sentence saying why — and an empty list is what a working party with no
        bots in it looks like.
        """
        facts = self.facts()
        checks = preconditions(facts)
        stop = blocker(facts)
        if stop is not None:
            return PartyState(False, stop, checks)
        rows = self.members(master)
        if isinstance(rows, str):
            return PartyState(True, "", checks, problem=rows)
        return PartyState(True, "", checks, members=rows)

    def online_guid(self, name: str) -> int | None:
        """This character's guid while it is logged in, or `None`.

        `online = 1` is part of the question and not a detail: the bridge
        resolves its master with `GetPlayerByName` and prints
        "player not found/offline" for a character who is not there, which
        arrives back over the channel as a command that succeeded and a bot
        that never comes.
        """
        if not valid_name(name):
            return None
        chars = self.entry.schema_map()["characters"]
        raw = self._sql.query(
            "characters",
            f"SELECT guid FROM {chars}.characters WHERE name = '{name}' AND online = 1 LIMIT 1;",
        )
        text = raw.strip()
        return int(text) if text.isdigit() else None

    def members(self, master: str) -> tuple[Member, ...] | str:
        """The bots in `master`'s party, or the sentence saying why not."""
        answer = dbreads.resolve_marker(self.entry, self.server_dir)
        if answer.marker is None:
            return answer.problem or "this install's bot marker is unreadable"
        guid = self.online_guid(master)
        if guid is None:
            return _not_online(master)
        try:
            raw = self._sql.query(
                "characters", group_rows_sql(self.entry, answer.marker, master_guid=guid) + ";"
            )
        except Exception as exc:  # noqa: BLE001 - one answer for every seam failure
            logger.warning(f"could not read {master}'s party: {exc}")
            return f"could not read this character's party: {exc}"
        return read_members(raw)

    # -- writes --------------------------------------------------------------

    def specs(self, klass: str) -> tuple[str, ...]:
        """The premade specs this install lists for `klass`, in the module's order.

        Filtered by `valid_spec`, which the conf's own values are NOT guaranteed
        to pass: the file is hand-editable and raw-writable from the Modules
        editor. The prior art offered every conf name verbatim and then had a
        validator refuse some of them (`50-party.sh:208-215`'s own note); a
        picker that cannot offer a name this app would refuse to send is the
        version of that fix with nothing left to disagree about.
        """
        number = BOT_CLASS_IDS.get(klass.strip().lower())
        if number is None:
            return ()
        listed = spec_names(self.server_dir).get(number, ())
        return tuple(name for name in listed if valid_spec(name))

    def max_level(self) -> int | None:
        """This install's own `MaxPlayerLevel`, or `None` for "could not be read"."""
        return max_player_level(self.server_dir)

    def add(
        self,
        master: str,
        klass: str,
        *,
        gender: str = "",
        spec: str = "",
        level: int | None = None,
    ) -> Addition:
        """Add one bot of `klass` to `master`'s party, over the channel."""
        facts = self.facts()
        stop = blocker(facts)
        if stop is not None:
            return Addition(False, False, None, False, False, stop, blocker=stop)
        if self.online_guid(master) is None:
            return Addition(False, False, None, False, False, _not_online(master))
        send = self._send_or_none()
        if send is None:
            return Addition(False, False, None, False, False, _no_channel())
        return add_bot(
            facts=facts,
            player=master,
            klass=klass,
            gender=gender,
            spec=spec,
            specs=self.specs(klass),
            level=level,
            max_level=self.max_level(),
            set_level=self.level_setter,
            send=send,
            members=lambda: _rows_only(self.members(master)),
        )

    def remove_all(self, master: str, confirmed: tuple[int, ...]) -> MassDismissal:
        """Send `confirmed` away — and nothing else, whatever the party holds now.

        `confirmed` is the guids a person agreed to, and it is required rather
        than optional because it is the whole point of the method. Round 2's
        must-fix: a caller that checks its own screen and then asks for "the
        party" has guarded the wrong side of the door — the two presses of a
        confirmation are seconds apart, a bot can join in between, and the
        screen has nothing to say about it. So this re-reads the group table and
        refuses unless the fresh set is EXACTLY the confirmed one, in both
        directions: a party that gained a bot is not the party that was agreed
        to, and neither is one that lost one. A subset that "obviously" still
        works is how a confirmation quietly becomes a suggestion.

        Guids and not names, for the reason the group read selects them: a name
        is what a display shows, and `group_member` keys on the guid.

        The list still comes from the table rather than from the caller — the
        bot manager logs bots in and out on a timer, and acting on a list read
        minutes ago is 8.4d's finding again. What the caller supplies is what it
        is allowed to act on, not what is there.
        """
        send = self._send_or_none()
        if send is None:
            return MassDismissal(0, (), _no_channel(), blocker=_no_channel())
        rows = self.members(master)
        if isinstance(rows, str):
            return MassDismissal(0, (), rows, blocker=rows)
        if {row.guid for row in rows} != set(confirmed):
            moved = _not_the_confirmed_party(master, rows, confirmed)
            return MassDismissal(0, (), moved, blocker=moved)
        return dismiss_all(
            player=master,
            bots=tuple(row.name for row in rows),
            send=send,
            members=lambda: _rows_only(self.members(master)),
        )

    def remove(self, master: str, bot: str) -> Dismissal:
        """Uninvite `bot` from `master`'s party and read the group table back."""
        send = self._send_or_none()
        if send is None:
            return Dismissal(False, False, _no_channel())
        return dismiss(
            player=master,
            bot=bot,
            send=send,
            members=lambda: _rows_only(self.members(master)),
        )

    def _send_or_none(self) -> Callable[[str], Answer] | None:
        channel = self._channel_for_saved()
        if channel is None:
            return None
        return channel.send  # type: ignore[attr-defined,no-any-return]


def _rows_only(answer: tuple[Member, ...] | str) -> tuple[Member, ...]:
    """A read that failed is an EMPTY party for polling purposes, deliberately.

    The poll is looking for a guid that was not there before; a read that could
    not be done adds nothing to either side of that comparison, and treating it
    as "the bot is not here yet" makes the press time out and say so rather
    than announce a bot it never saw.
    """
    return () if isinstance(answer, str) else answer


def _not_online(name: str) -> str:
    return (
        f"{name} is not logged in. A bot is added to a live session — the server resolves the "
        "master by name in the world, so log the character in and press again."
    )


def _not_the_confirmed_party(
    master: str, rows: tuple[Member, ...], confirmed: tuple[int, ...]
) -> str:
    """Why nothing was dismissed, naming the party as it is NOW.

    As it is now rather than as the difference between the two: a person who has
    just been refused wants to know what to confirm next, and "one more bot than
    you agreed to" is a sentence they cannot check against the party frame in
    front of them.
    """
    holds = ", ".join(row.name for row in rows) if rows else "no bots at all"
    return (
        f"{master}'s party is not the one that was confirmed: {bots_word(len(confirmed))} were "
        f"agreed to and the group table now holds {holds}. Nothing was sent — show the party "
        "again and confirm what is there now."
    )


def _no_channel() -> str:
    return (
        "this install has no command channel set up yet, and My Party has no other way to reach "
        "the server. Set it up on the Server tab."
    )
