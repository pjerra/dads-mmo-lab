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

from yulon import dbreads, party, resources
from yulon.catalog.catalog import load_catalog
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


# -- adding a bot, dismissing it (8.6 part 2) ------------------------------
#
# Every command string below is the shape `rust-main` fires, cited at its line,
# and every SQL shape is that tree's too. What is NOT inherited is the class
# list: `dk` is a class THIS tree's `addclass` accepts (`PlayerbotMgr.cpp:1089`,
# read on `yulon-ubuntu` 2026-09-08), and the bash launcher's `_valid_bot_class`
# excluded it. A sibling's list would have refused a class this server supports.

WOTLK = load_catalog().get("wow-wotlk")
RNDBOT = dbreads.Marker(prefix="rndbot", source="default")


def test_the_add_command_is_the_bridge_command_and_not_the_playerbot_one() -> None:
    """`.playerbots bot addclass` is what the SERVER runs; the app can only ask
    the bridge (`rust-main:crates/dml-wow/src/party.rs:163`). Sending the
    playerbot spelling over the channel is the 2026-08-20 mistake by hand."""
    assert party.add_command("Pakka", "mage") == "dml_addclass Pakka mage"
    assert party.add_command("Pakka", "mage", gender="female") == "dml_addclass Pakka mage female"


def test_deathknight_is_a_class_this_tree_accepts() -> None:
    assert party.add_command("Pakka", "dk") == "dml_addclass Pakka dk"


@pytest.mark.parametrize("bad", ["sorcerer", "", "mage; .server shutdown 1", "MAGE!"])
def test_a_class_this_tree_does_not_have_is_refused_before_anything_is_sent(bad: str) -> None:
    with pytest.raises(party.BadRequest):
        party.add_command("Pakka", bad)


@pytest.mark.parametrize("bad", ["", "Pak ka", "Pakka;", "a" * 13])
def test_a_name_that_is_not_a_character_name_never_reaches_the_channel(bad: str) -> None:
    with pytest.raises(party.BadRequest):
        party.add_command(bad, "mage")


def test_the_dismiss_commands_are_the_uninvite_and_the_logout_whisper() -> None:
    """Two commands, and the second is best-effort — `rust-main`'s kick fires
    `dml_uninvite` then `dml_whisper <master> <bot> logout`
    (`crates/dml-wow/src/party.rs:184,192`)."""
    assert party.uninvite_command("Bottom") == "dml_uninvite Bottom"
    assert party.logout_command("Pakka", "Bottom") == "dml_whisper Pakka Bottom logout"


def test_the_finishing_whispers_are_gear_and_talents() -> None:
    """ "geared and specced" is not what `addclass` alone does: the bash and Rust
    launchers both whisper the bot afterwards
    (`rust-main:crates/dml-wow/src/party.rs:246,249`)."""
    assert party.autogear_command("Pakka", "Bottom") == "dml_whisper Pakka Bottom autogear"
    assert party.talents_command("Pakka", "Bottom") == "dml_whisper Pakka Bottom talents autopick"


def test_the_group_read_is_anchored_on_the_masters_own_group() -> None:
    """`WHERE gm.guid = (SELECT guid FROM group_member WHERE memberGuid=<master>)`
    — the master's GROUP id, not his guid (`group_member.guid` is the group on
    this tree; `describe group_member`, `yulon-ubuntu2` 2026-09-08). Without the
    sub-select this reads every group on the server."""
    sql = party.group_rows_sql(WOTLK, RNDBOT, master_guid=1001)
    assert "group_member" in sql
    assert "memberGuid=1001" in sql.replace(" ", "")
    assert "RNDBOT%" in sql.upper()
    assert "ORDER BY" in sql.upper()


def test_the_group_read_only_returns_rows_the_bot_marker_recognises() -> None:
    """The box's words: "one row appears in the group table on an account the
    bot marker recognises". The master is in his own `group_member` row too, and
    a read that returned him would report a party of one before any bot joined."""
    sql = party.group_rows_sql(WOTLK, RNDBOT, master_guid=1001)
    assert dbreads.bot_clause(WOTLK, RNDBOT) in sql


class _Chan:
    """A channel that records what it was asked and answers from a script."""

    def __init__(self, answers: dict[str, Answer] | None = None) -> None:
        self.sent: list[str] = []
        self.answers = answers or {}

    def send(self, command: str) -> Answer:
        self.sent.append(command)
        return self.answers.get(command.split()[0], Answer("yes", "ok"))


def test_a_party_that_is_not_ready_sends_nothing_and_says_which_step_failed() -> None:
    """The box's words: "with the bridge absent the group says which precondition
    failed and never reads as ready". Never-ready is the whole point: the
    2026-08-20 failure was a control that pressed happily into a dead bridge."""
    chan = _Chan()
    result = party.add_bot(
        facts=_facts(bridge_answered=False),
        player="Pakka",
        klass="mage",
        send=chan.send,
        members=lambda: (),
    )
    assert chan.sent == []
    assert result.added is False
    assert result.blocker == party.blocker(_facts(bridge_answered=False))
    assert "does not answer" in result.sentence


def test_the_ground_is_read_before_the_press_and_the_new_row_is_the_difference() -> None:
    """A poll that asserts "there is a bot in the party" proves nothing on a
    server that already had one. The new member is the guid that was not there
    before (`rust-main:crates/dml-wow/src/party.rs:315`)."""
    already = party.Member(name="Oldbot", guid=500, klass=8, level=10)
    fresh = party.Member(name="Newbot", guid=777, klass=8, level=1)
    reads = iter([(already,), (already,), (already, fresh)])
    chan = _Chan()
    result = party.add_bot(
        facts=_facts(),
        player="Pakka",
        klass="mage",
        send=chan.send,
        members=lambda: next(reads),
        sleep=lambda _s: None,
    )
    assert result.bot == "Newbot"
    assert result.joined is True
    assert chan.sent[0] == "dml_addclass Pakka mage"
    assert "dml_whisper Pakka Newbot autogear" in chan.sent
    assert "dml_whisper Pakka Newbot talents autopick" in chan.sent
    assert result.geared is True
    assert result.specced is True


def test_a_bot_that_never_joins_is_reported_as_added_but_not_joined() -> None:
    """Not as a success and not as a failure: the command was accepted and the
    row never appeared, which is a third thing and the one a person can act on."""
    chan = _Chan()
    result = party.add_bot(
        facts=_facts(),
        player="Pakka",
        klass="mage",
        send=chan.send,
        members=lambda: (),
        tries=3,
        sleep=lambda _s: None,
    )
    assert result.added is True
    assert result.joined is False
    assert result.bot is None
    assert chan.sent == ["dml_addclass Pakka mage"]
    assert "no bot joined" in result.sentence


def test_a_refused_add_never_polls_and_quotes_the_server() -> None:
    chan = _Chan({"dml_addclass": Answer("no", "Command 'dml_addclass' does not exist")})
    polls = 0

    def members() -> tuple[party.Member, ...]:
        nonlocal polls
        polls += 1
        return ()

    result = party.add_bot(
        facts=_facts(),
        player="Pakka",
        klass="mage",
        send=chan.send,
        members=members,
        sleep=lambda _s: None,
    )
    assert result.added is False
    assert "does not exist" in result.sentence
    assert polls <= 1


def test_dismissing_uninvites_then_whispers_logout_and_confirms_the_row_is_gone() -> None:
    reads = iter([(party.Member("Newbot", 777, 8, 1),), ()])
    chan = _Chan()
    result = party.dismiss(
        player="Pakka",
        bot="Newbot",
        send=chan.send,
        members=lambda: next(reads),
        sleep=lambda _s: None,
    )
    assert chan.sent[0] == "dml_uninvite Newbot"
    assert "dml_whisper Pakka Newbot logout" in chan.sent
    assert result.removed is True


def test_a_logout_whisper_that_fails_does_not_fail_the_dismiss() -> None:
    """Best-effort at every call site in the prior art, whose own comment says
    the failure "never aborts the caller" (`party.rs:189-192`)."""
    chan = _Chan({"dml_whisper": Answer("no", "player not found")})
    reads = iter([(party.Member("Newbot", 777, 8, 1),), ()])
    result = party.dismiss(
        player="Pakka",
        bot="Newbot",
        send=chan.send,
        members=lambda: next(reads),
        sleep=lambda _s: None,
    )
    assert result.removed is True


class _Sql:
    """The reader seam, answering each statement from a script in order."""

    def __init__(self, *answers: str) -> None:
        self.answers = list(answers)
        self.asked: list[str] = []

    def query(self, db: str, statement: str) -> str:
        self.asked.append(statement)
        return self.answers.pop(0) if self.answers else ""


def _ready_install(tmp_path: Path) -> Path:
    """A server folder with every disk-side precondition already met."""
    server = tmp_path / "wowserver"
    (server / "modules" / "mod-ale").mkdir(parents=True)
    (server / "env" / "dist" / "etc" / "modules").mkdir(parents=True)
    (server / party.ALE_CONF).write_text(
        f'ALE.Enabled = 1\nALE.ScriptPath = "{party.ALE_SCRIPT_PATH}"\n'
    )
    dest = party.dest_dir(server)
    dest.mkdir(parents=True)
    for name in party.BRIDGE_SCRIPTS:
        (dest / name).write_text("-- x\n")
    return server


def _install(
    server: Path, sql: _Sql, chan: object | None, running: bool = True
) -> party.InstallParty:
    return party.InstallParty(
        WOTLK,
        server,
        sql=sql,
        channel_for_saved=lambda: chan,
        container="ac-worldserver",
        world_running=lambda: running,
        engine=lambda: party.BinaryRead(True, "in"),
    )


def test_the_seam_refuses_a_master_who_is_not_logged_in(tmp_path: Path) -> None:
    """The bridge resolves its master with `GetPlayerByName` and prints
    "player not found/offline" when there is none (`dml_addclass.lua`), and
    `.playerbots bot addclass` needs a live session in the first place. A press
    that goes out anyway gets a silent no-op and a bot that never arrives."""
    chan = _Chan({"dml_bridge_ping": Answer("yes", "DML-BRIDGE-READY dml_bridge_ping")})
    sql = _Sql("")  # the online lookup finds nobody
    result = _install(_ready_install(tmp_path), sql, chan).add("Pakka", "mage")
    assert result.added is False
    assert "logged in" in result.sentence
    assert chan.sent == ["dml_bridge_ping"]


def test_the_seam_never_asks_the_server_while_the_world_is_down(tmp_path: Path) -> None:
    """`world_running` is passed in rather than guessed at, and a stopped world
    is the second precondition — so no channel is opened and no SQL is run."""
    sql = _Sql()
    chan = _Chan()
    state = _install(_ready_install(tmp_path), sql, chan, running=False).state("Pakka")
    assert state.ready is False
    assert "not running" in state.blocker
    assert sql.asked == []
    assert chan.sent == []


def test_the_seam_reads_the_ground_and_shows_the_party(tmp_path: Path) -> None:
    """The group read is anchored on the master's ONLINE guid, and both reads
    happen: the lookup, then the group."""
    chan = _Chan({"dml_bridge_ping": Answer("yes", "DML-BRIDGE-READY dml_bridge_ping")})
    sql = _Sql("1001\n", "Bottom\t777\t8\t1\n")
    state = _install(_ready_install(tmp_path), sql, chan).state("Pakka")
    assert state.ready is True
    assert state.blocker == ""
    assert state.members == (party.Member("Bottom", 777, 8, 1),)
    assert "online = 1" in sql.asked[0]
    assert "memberGuid=1001" in sql.asked[1].replace(" ", "")


def test_a_bridge_that_does_not_answer_leaves_the_group_unready(tmp_path: Path) -> None:
    """The live half of "never reads as ready": the disk says everything is in
    place and the SERVER says it has never heard of the bridge."""
    chan = _Chan({"dml_bridge_ping": Answer("no", "Command 'dml_bridge_ping' does not exist")})
    sql = _Sql()
    state = _install(_ready_install(tmp_path), sql, chan).state("Pakka")
    assert state.ready is False
    assert "does not answer" in state.blocker
    assert sql.asked == []


def test_the_group_rows_come_back_as_members_and_a_bad_row_is_said_so() -> None:
    """Four fields per row, and a row that is not four fields is reported
    rather than skipped: a party silently one bot short is the same lie as a
    party that never filled."""
    assert party.read_members("Bottom\t777\t8\t1\n") == (party.Member("Bottom", 777, 8, 1),)
    assert isinstance(party.read_members("Bottom\t777\n"), str)


def test_a_script_the_module_brought_is_not_a_missing_bridge_script(tmp_path: Path) -> None:
    """MEASURED on `yulon-ubuntu2` 2026-09-09, by the rebuild lane: `mod-ale`
    installs its own example `LootPet.lua` into the very directory the bridge
    is deployed to. The deployed precondition asked for set EQUALITY, so on the
    only install where the engine has ever worked My Party would have reported
    "some of the bridge scripts are missing" and named none of them — while all
    five were there. What is required is that ours are all present."""
    facts = _facts(deployed=(*party.BRIDGE_SCRIPTS, "LootPet.lua"))
    assert party.ready(facts) is True
    assert party.blocker(facts) is None


def test_the_conf_is_read_for_both_keys_or_for_neither(tmp_path: Path) -> None:
    """`ALE.Enabled` and `ALE.ScriptPath` are read from column 0 only, the same
    rule `conf.patch()` writes by: the shipped `mod_ale.conf.dist` is full of
    commented prose, and a looser pattern reads its own comment saying `true`
    as the setting — which is the exact lie the compiled default `false` makes
    expensive (`ALEConfig.cpp:20`)."""
    conf = tmp_path / "mod_ale.conf"
    conf.write_text(
        "# ALE.Enabled = 1\n"
        '#   ALE.ScriptPath = "/somewhere/else"\n'
        "ALE.Enabled = 1\n"
        'ALE.ScriptPath = "/azerothcore/env/dist/etc/modules/lua_scripts"\n'
    )
    read = party.read_conf(conf)
    assert read.present is True
    assert read.enabled is True
    assert read.script_path == party.ALE_SCRIPT_PATH


def test_a_conf_that_is_not_there_reads_as_absent_and_not_as_off(tmp_path: Path) -> None:
    """`enabled=None` and not `False`: a file nobody wrote has said nothing
    about the engine, and "switched off" sends a person to edit a key that is
    not there. The precondition above it is what names the real fix."""
    read = party.read_conf(tmp_path / "nothing.conf")
    assert read.present is False
    assert read.enabled is None
    assert read.script_path is None


def test_the_facts_are_gathered_from_the_disk_the_binary_and_the_wire(tmp_path: Path) -> None:
    """`read_facts` is what the tab's seam calls. It asks each source once and
    it asks the SERVER whether the bridge is there — the deploy's own success is
    not evidence, which is the whole finding of 2026-08-20."""
    server = tmp_path / "wowserver"
    (server / "modules" / "mod-ale").mkdir(parents=True)
    (server / "env" / "dist" / "etc" / "modules").mkdir(parents=True)
    (server / "env" / "dist" / "etc" / "modules" / "mod_ale.conf").write_text(
        f'ALE.Enabled = 1\nALE.ScriptPath = "{party.ALE_SCRIPT_PATH}"\n'
    )
    dest = party.dest_dir(server)
    dest.mkdir(parents=True)
    for name in party.BRIDGE_SCRIPTS:
        (dest / name).write_text("-- x\n")
    facts = party.read_facts(
        server,
        world_running=True,
        engine=party.BinaryRead(True, "in"),
        probe=party.Probe(True, "answered", "DML-BRIDGE-READY"),
    )
    assert party.ready(facts) is True
    assert facts.deployed == tuple(sorted(party.BRIDGE_SCRIPTS))
    assert facts.engine_cloned is True


def test_the_facts_carry_a_server_that_was_never_installed(tmp_path: Path) -> None:
    facts = party.read_facts(
        tmp_path / "nowhere",
        world_running=False,
        engine=party.BinaryRead(None, "unread"),
        probe=party.Probe(None, "unasked", ""),
    )
    assert facts.server_installed is False
    assert party.blocker(facts) is not None
    assert "not installed" in party.blocker(facts)  # type: ignore[operator]


def test_a_row_that_is_still_there_is_not_reported_as_dismissed() -> None:
    chan = _Chan()
    result = party.dismiss(
        player="Pakka",
        bot="Newbot",
        send=chan.send,
        members=lambda: (party.Member("Newbot", 777, 8, 1),),
        tries=2,
        sleep=lambda _s: None,
    )
    assert result.removed is False
    assert "still" in result.sentence
