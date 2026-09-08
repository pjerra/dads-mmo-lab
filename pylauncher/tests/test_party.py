"""My Party's server-side bridge: the deploy, the preconditions, the proof (8.6).

Every fact asserted here about the WotLK tree was **measured on `yulon-ubuntu`
on 2026-09-08** and is recorded in `pyplan/gates/8.6-wotlk-yulon-ubuntu-2026-09-08/`.
None of it is inherited from a sibling family: the invalid-command sentence, the
console filter on `.playerbots bot`, the module's own conf key and its compiled
default are per-tree facts, and three of the four differ from what the prior art
assumed.

The one thing this file cannot test is the bridge answering, because the Lua
engine is not compiled into the image on that box. That is the box's first
question and its answer is *no, not today* — so what is tested here is the shape
of every sentence said on the way to that answer, and the classifier that will
recognise the answer when a rebuild puts the engine in.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from yulon import party, resources
from yulon.channel import Answer

# -- where the scripts live ------------------------------------------------


def test_the_bridge_scripts_ship_with_the_app() -> None:
    """The scripts are data the app deploys, so they must be in the bundle root."""
    root = resources.lua_dir()
    assert root == resources.bundle_root() / "lua"
    assert (root / "party").is_dir()
    names = {p.name for p in (root / "party").glob("*.lua")}
    assert names == set(party.BRIDGE_SCRIPTS), names


def test_the_pyinstaller_spec_carries_the_lua_tree() -> None:
    """A script that ships from a checkout and not from a build is a bridge that
    works for developers only. The spec lists `manifests/` and
    `catalog/installers/` by hand, so a third tree has to be added by hand too."""
    spec = (Path(__file__).resolve().parents[1] / "build" / "pylauncher.spec").read_text()
    assert '"lua"' in spec, spec


def test_every_bridge_script_registers_the_command_hook_and_is_console_only() -> None:
    """Two properties no test above the file can see, and both are load-bearing.

    `RegisterPlayerEvent(42, ...)` is PLAYER_EVENT_ON_COMMAND — the hook ALE
    pushes `(player, text, handler)` into (`ALE PlayerHooks.cpp:42-62`). And
    `player ~= nil` is the console/SOAP guard: `OnCommand` sets the player to
    `nullptr` only for `handler.IsConsole()`, so a script without that line is
    one a logged-in player can fire by typing it in chat.
    """
    for name in party.BRIDGE_SCRIPTS:
        text = (resources.lua_dir() / "party" / name).read_text()
        assert "RegisterPlayerEvent(42" in text, name
        assert "if player ~= nil then return end" in text, name


def test_the_dest_is_the_directory_the_container_mounts() -> None:
    assert party.dest_dir(Path("/srv/wow")) == Path("/srv/wow/env/dist/etc/modules/lua_scripts")


# -- the deploy ------------------------------------------------------------


def _root_with(tmp_path: Path, **families: dict[str, str]) -> Path:
    root = tmp_path / "lua"
    for family, files in families.items():
        (root / family).mkdir(parents=True)
        for name, body in files.items():
            (root / family / name).write_text(body)
    return root


def test_deploy_flattens_every_family_into_one_directory(tmp_path: Path) -> None:
    """ALE scans ONE directory (`ALE.ScriptPath`), so the family folders that
    organise the source do not survive the copy."""
    root = _root_with(tmp_path, party={"a.lua": "A"}, gm={"b.lua": "B"})
    dest = tmp_path / "dest"
    done = party.deploy(root, dest)
    assert done.changed is True
    assert done.names == ("a.lua", "b.lua")
    assert (dest / "a.lua").read_text() == "A"
    assert (dest / "b.lua").read_text() == "B"


def test_a_second_deploy_of_the_same_bytes_changes_nothing(tmp_path: Path) -> None:
    """`changed` is what tells the caller a world restart is owed, so a deploy
    that always says yes asks for a restart the user does not need."""
    root = _root_with(tmp_path, party={"a.lua": "A"})
    dest = tmp_path / "dest"
    assert party.deploy(root, dest).changed is True
    assert party.deploy(root, dest).changed is False


def test_a_changed_script_is_copied_over_the_old_one(tmp_path: Path) -> None:
    root = _root_with(tmp_path, party={"a.lua": "A"})
    dest = tmp_path / "dest"
    party.deploy(root, dest)
    (root / "party" / "a.lua").write_text("A2")
    assert party.deploy(root, dest).changed is True
    assert (dest / "a.lua").read_text() == "A2"


def test_deploying_nothing_raises_instead_of_reporting_success(tmp_path: Path) -> None:
    """The Rust launcher shipped this bug and named it in its own source: with
    the source root wrong it emitted `done{changed:false}` — a success envelope
    for a no-op — so "Enable My Party" appeared to work and My Party simply did
    not function (`rust-main:crates/dml-wow/src/bridge.rs:60-70`).

    Both ways of finding nothing must fail, not one.
    """
    with pytest.raises(party.NothingToDeploy):
        party.deploy(tmp_path / "absent", tmp_path / "dest")
    empty = tmp_path / "empty"
    empty.mkdir()
    with pytest.raises(party.NothingToDeploy):
        party.deploy(empty, tmp_path / "dest")
    no_lua = _root_with(tmp_path, party={"readme.txt": "x"})
    with pytest.raises(party.NothingToDeploy):
        party.deploy(no_lua, tmp_path / "dest")


def test_the_shipped_scripts_deploy(tmp_path: Path) -> None:
    """The bundled tree is a real deploy source, not only a directory of files."""
    done = party.deploy(resources.lua_dir(), tmp_path / "dest")
    assert done.names == tuple(sorted(party.BRIDGE_SCRIPTS))


# -- reading the binary ----------------------------------------------------


class _Run:
    """A stand-in for one `docker exec`, recording what it was asked."""

    def __init__(self, stdout: str, returncode: int = 0) -> None:
        self.stdout = stdout
        self.returncode = returncode
        self.argv: list[str] = []

    def __call__(self, command: list[str], **kw: object) -> object:
        self.argv = command
        return type("P", (), {"stdout": self.stdout, "stderr": "", "returncode": self.returncode})()


def test_the_engine_is_read_out_of_the_binary_not_out_of_a_conf_file() -> None:
    """Owner instruction, 2026-09-08: "Read the binary, not a conf file."

    A conf file is written by the manifest and says nothing about what was
    compiled — on `yulon-ubuntu` `mod_ale.conf` was absent AND the binary had no
    ALE in it, and only the second of those needs a rebuild.
    """
    run = _Run("ALE 0\nCONTROL 1\n")
    read = party.read_engine_in_binary("ac-worldserver", run=run)
    assert read.engine is False
    assert party.ALE_MARKER in " ".join(run.argv)
    assert party.CONTROL_MARKER in " ".join(run.argv)


def test_an_absent_control_string_reads_as_could_not_be_read_not_as_absent() -> None:
    """A grep that finds nothing looks identical to a grep that did not run.

    This is the `guards-that-prove-declarations` lesson in one assertion: the
    probe carries a string the binary MUST have, and a zero there means the
    reading is worthless — a wrong path, a stripped binary, a busybox grep — not
    that the engine is missing. Without it a mistyped path would have reported
    "the Lua engine is not built in" about a server that had it.
    """
    read = party.read_engine_in_binary("ac-worldserver", run=_Run("ALE 0\nCONTROL 0\n"))
    assert read.engine is None
    assert "could not" in read.sentence.lower()


def test_a_binary_carrying_the_engine_reads_as_present() -> None:
    read = party.read_engine_in_binary("ac-worldserver", run=_Run("ALE 4\nCONTROL 1\n"))
    assert read.engine is True


def test_a_failed_exec_is_could_not_be_read() -> None:
    read = party.read_engine_in_binary("ac-worldserver", run=_Run("", returncode=1))
    assert read.engine is None


# -- the preconditions -----------------------------------------------------


def _facts(**over: object) -> party.Facts:
    """Everything met, so each test can break exactly one thing."""
    base = dict(
        server_installed=True,
        world_running=True,
        engine_cloned=True,
        engine_in_binary=True,
        conf_present=True,
        engine_enabled=True,
        script_path=party.ALE_SCRIPT_PATH,
        deployed=tuple(sorted(party.BRIDGE_SCRIPTS)),
        bridge_answered=True,
    )
    base.update(over)
    return party.Facts(**base)  # type: ignore[arg-type]


def test_everything_met_is_ready_and_names_no_blocker() -> None:
    assert party.ready(_facts()) is True
    assert party.blocker(_facts()) is None


@pytest.mark.parametrize(
    ("broken", "must_say"),
    [
        ({"server_installed": False}, "not installed"),
        ({"world_running": False}, "not running"),
        ({"engine_cloned": False}, "Modules"),
        ({"engine_in_binary": False}, "rebuild"),
        ({"engine_in_binary": None}, "could not"),
        ({"conf_present": False}, "mod_ale.conf"),
        ({"engine_enabled": False}, "ALE.Enabled"),
        ({"script_path": "lua_scripts"}, "relative"),
        ({"deployed": ()}, "bridge scripts"),
        ({"bridge_answered": False}, "does not answer"),
        ({"bridge_answered": None}, "could not"),
    ],
)
def test_each_unmet_precondition_draws_its_own_sentence(
    broken: dict[str, object], must_say: str
) -> None:
    """The box's words: "with each precondition drawing its own sentence when it
    is not met". One shared "My Party is not available" is what this forbids."""
    facts = _facts(**broken)
    assert party.ready(facts) is False
    said = party.blocker(facts)
    assert said is not None
    assert must_say in said, said


def test_no_two_preconditions_share_a_sentence() -> None:
    """A distinct sentence per precondition is the whole clause; two the same is
    a user told to fix the wrong thing."""
    sentences = [p.sentence for p in party.preconditions(_facts(server_installed=False))]
    assert len(sentences) == len(set(sentences))


def test_the_blocker_is_the_first_thing_to_fix_not_the_last() -> None:
    """With the server absent, everything below it is unreadable too. Naming the
    deployed scripts there would send a user to a folder that cannot exist."""
    facts = _facts(server_installed=False, engine_cloned=False, deployed=())
    assert "not installed" in (party.blocker(facts) or "")


def test_no_sentence_asserts_a_fact_another_precondition_has_already_denied() -> None:
    """FOUND BY RUNNING IT, `yulon-ubuntu` 2026-09-08 09:53Z, on the real install.

    Every precondition below the first unmet one is still printed, and two of
    them stated as fact something the reading had just contradicted:

        engine_in_binary  "the Lua engine is installed but not built into the
                           server that is running"   -- it was NOT installed
        bridge_answered   "The scripts are in place, so the world server has
                           not read them"            -- they were NOT in place

    Neither is the blocker, so neither would ever have reached a user through
    `blocker()`. That is exactly why it survived: a sentence nobody reads on the
    happy path is still a sentence, and the gate page prints all nine. A
    precondition may describe its OWN fact and may not assert somebody else's.
    """
    facts = _facts(
        engine_cloned=False,
        engine_in_binary=False,
        conf_present=False,
        engine_enabled=None,
        script_path=None,
        deployed=(),
        bridge_answered=False,
    )
    said = {p.name: p.sentence for p in party.preconditions(facts)}
    assert "is installed" not in said["engine_in_binary"], said["engine_in_binary"]
    assert "scripts are in place" not in said["bridge_answered"], said["bridge_answered"]


def test_a_partial_deploy_is_not_a_deploy() -> None:
    """One script arriving and another not is exactly how a half-copied bridge
    answers `dml_addclass` and not `dml_uninvite`."""
    some = tuple(sorted(party.BRIDGE_SCRIPTS))[:2]
    assert party.ready(_facts(deployed=some)) is False
    assert "bridge scripts" in (party.blocker(_facts(deployed=some)) or "")


# -- the proof -------------------------------------------------------------


def test_the_probe_command_is_the_one_the_shipped_script_registers() -> None:
    """A probe asking for a command no script answers is a probe that always
    says no. This binds the two together in the only place that can."""
    text = (resources.lua_dir() / "party" / "dml_bridge_ping.lua").read_text()
    assert f'"^{party.PROBE_COMMAND}' in text or f"^{party.PROBE_COMMAND}" in text
    assert party.PROBE_TOKEN in text


def test_the_measured_refusal_reads_as_a_bridge_that_did_not_arrive() -> None:
    """MEASURED on `yulon-ubuntu`, 2026-09-08 09:38Z, over the app's own SOAP
    channel, with nothing deployed:

        HTTP 500, faultstring: Command 'dml_bridge_ping' does not exist

    This is the exact wire text; the 2026-08-20 note reported the same shape.
    """
    proof = party.read_probe(Answer("no", "Command 'dml_bridge_ping' does not exist\r\n"))
    assert proof.arrived is False
    assert "does not know" in proof.sentence


def test_a_result_carrying_the_scripts_own_word_is_the_arrival() -> None:
    """The proof the box asks for: the SCRIPT answering, not the deploy
    reporting success. `handler:SendSysMessage` (`ALE ChatHandlerMethods.h:29`)
    writes into the SOAP print buffer, and a command whose hook returned false
    never sets the error flag — so the reply flips from a fault to a result
    carrying the script's own token (`ACSoap.cpp:133-140`)."""
    proof = party.read_probe(Answer("yes", f"{party.PROBE_TOKEN} 5 scripts\r\n"))
    assert proof.arrived is True
    assert party.PROBE_TOKEN in proof.text


def test_a_success_without_the_token_is_not_an_arrival() -> None:
    """A deploy reporting success is what fooled 2026-08-20. So is any other
    command on the server that happens to answer to this name."""
    proof = party.read_probe(Answer("yes", "ok\r\n"))
    assert proof.arrived is False
    assert "own word" in proof.sentence


def test_a_channel_that_could_not_ask_is_neither_arrived_nor_absent() -> None:
    """Three outcomes, as everywhere else in this phase: yes, no, could not ask.
    Reporting a stopped server as "the bridge is missing" would send the user to
    reinstall a module over a server that is simply down."""
    proof = party.read_probe(Answer("unknown", "", reason="this server is not running"))
    assert proof.arrived is None
    assert "could not" in proof.sentence.lower()


# -- the manifest ----------------------------------------------------------


def _ale_manifest() -> dict:
    path = resources.manifests_dir() / "wow-wotlk" / "modules" / "mod-ale.json"
    return json.loads(path.read_text())


def test_the_manifest_names_the_conf_key_the_module_actually_has() -> None:
    """MEASURED from the module's own source, 2026-09-08:

        conf/mod_ale.conf.dist:63          ALE.Enabled = true
        src/LuaEngine/ALEConfig.cpp:20     "ALE.Enabled", default "false"

    The manifest said `ALE.EnableLuaEngine`, which appears in neither. A key
    that does not exist is not written and not read, so the engine would have
    taken its COMPILED default — and that default is false while the shipped
    conf's own comment says true.
    """
    keys = [k["key"] for k in _ale_manifest()["conf"][0]["keys"]]
    assert "ALE.Enabled" in keys
    assert "ALE.EnableLuaEngine" not in keys


def test_the_manifest_writes_the_engine_on_rather_than_naming_the_key() -> None:
    """A key with no default is surfaced, not set. The compiled default is
    false, so surfacing it leaves the engine off."""
    enabled = next(k for k in _ale_manifest()["conf"][0]["keys"] if k["key"] == "ALE.Enabled")
    assert enabled.get("default") == "1"


def test_the_lua_engine_is_pinned() -> None:
    """The box asks for a revision. `azerothcore/mod-ale` moved from `c3de7942`
    (HEAD 2026-09-06, `phase8-delta.md:18`) to `319f43ed` (HEAD 2026-09-07) in
    two days, so an unpinned manifest hands a two-hour rebuild whatever that
    morning's HEAD is."""
    rev = _ale_manifest()["source"].get("rev")
    assert rev == party.MOD_ALE_REV
    assert len(rev) == 40


def test_the_script_path_the_manifest_writes_is_the_absolute_container_path() -> None:
    """The silent-bridge bug (`rust-main:bridge.rs:189-195`, live on Ubuntu
    2026-08-20): ALE ships `ALE.ScriptPath = "lua_scripts"`, relative, resolved
    against the worldserver's cwd where nothing is — so every bridge command
    answers "does not exist" while the deploy reports success."""
    manifest = _ale_manifest()
    path = next(k for k in manifest["conf"][0]["keys"] if k["key"] == "ALE.ScriptPath")
    assert party.ALE_SCRIPT_PATH in path["default"]
    assert path["default"].startswith('"/')
