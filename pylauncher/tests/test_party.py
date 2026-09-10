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

from yulon import dbreads, party, play, resources
from yulon.actions import Outcome
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


def test_the_uninvite_grammar_and_refusal_sentence_are_the_shipped_scripts_own() -> None:
    """Round 1's must-fix 3: the wire grammar `uninvite_command` builds for and
    the refusal sentence `dismiss()` parses each existed twice — once in this
    file, once in `dml_uninvite.lua` — with nothing binding them to the ONE
    script that actually speaks the protocol. Same shape as the probe-command
    pin above."""
    text = (resources.lua_dir() / "party" / "dml_uninvite.lua").read_text()
    assert party.UNINVITE_COMMAND_PATTERN in text
    assert party.UNINVITE_REFUSAL_FORMAT in text


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
    (`crates/dml-wow/src/party.rs:184,192`).

    T13: `uninvite_command` now carries the master's own name too, wider than
    `rust-main:.../party.rs:184`'s `dml_uninvite <bot>` — the Lua checks it at
    the moment it acts (see `dml_uninvite.lua`), which is the only place that
    can still see the group when the whisper lands."""
    assert party.uninvite_command("Pakka", "Bottom") == "dml_uninvite Pakka Bottom"
    assert party.logout_command("Pakka", "Bottom") == "dml_whisper Pakka Bottom logout"


@pytest.mark.parametrize("bad", ["", "Pak ka", "Pakka;", "a" * 13])
def test_uninvite_refuses_a_bad_master_name_before_anything_is_sent(bad: str) -> None:
    with pytest.raises(party.BadRequest):
        party.uninvite_command(bad, "Bottom")


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
    assert chan.sent[0] == "dml_uninvite Pakka Newbot"
    assert "dml_whisper Pakka Newbot logout" in chan.sent
    assert result.removed is True


def test_a_bot_that_moved_to_another_party_is_recognised_and_nothing_more_is_sent() -> None:
    """T13 (T5's round-3 Codex review): the confirm-to-whisper window. `remove_all`
    re-reads the group table and refuses unless the fresh set is exactly what was
    confirmed, but between that read and this whisper landing the bot manager can
    still move a bot into a DIFFERENT master's party on its own timer — and
    `dml_uninvite.lua` now checks the master at the moment it acts rather than
    trusting the name it was sent, answering in words this parses rather than
    treating as a plain "yes" (the bridge's hook never sets the core's error flag
    either way, so `answer.outcome` alone cannot tell the two apart).

    The mutation this catches: a parse that treats that reply as an ordinary
    "yes" goes on to whisper `logout` and poll the group table — sending a
    command for a party this master was never in, and (with the bot durably
    parked in someone else's party) eventually reporting the same `removed`
    result for the wrong reason. `chan.sent` and `logged_out` are what tell
    them apart: the right code sends nothing more once the bridge has already
    said no.
    """
    chan = _Chan(
        {
            "dml_uninvite": Answer(
                "yes", "Newbot is not in Pakka's party now (Newbot is grouped with someone else)"
            )
        }
    )
    result = party.dismiss(
        player="Pakka",
        bot="Newbot",
        send=chan.send,
        members=lambda: (party.Member("Newbot", 777, 8, 1),),
        sleep=lambda _s: None,
    )
    assert result.removed is False
    assert result.logged_out is False
    assert "Newbot" in result.sentence
    assert chan.sent == ["dml_uninvite Pakka Newbot"], (
        "a refused uninvite must not whisper logout or poll the table of a party "
        "this master was never confirmed against"
    )


def test_the_refusal_sentence_relays_the_server_and_invents_nothing() -> None:
    """Round 1's must-fix 1: the sentence is bounded to what the bridge
    actually said, not a Python-invented mechanism (the earlier "the group
    table moved it into a different party" wording asserted something no
    reply ever reported)."""
    chan = _Chan(
        {
            "dml_uninvite": Answer(
                "yes", "Newbot is not in Pakka's party now (Newbot is grouped with someone else)"
            )
        }
    )
    result = party.dismiss(
        player="Pakka",
        bot="Newbot",
        send=chan.send,
        members=lambda: (),
        sleep=lambda _s: None,
    )
    assert result.sentence == (
        "Newbot was not removed: the server says Newbot is not in Pakka's party now "
        "(Newbot is grouped with someone else)"
    )


@pytest.mark.parametrize(
    "reason",
    [
        "Pakka not found or offline",
        "Newbot not found or offline",
        "Newbot is not grouped with anyone",
        "Newbot is grouped with someone else",
    ],
)
def test_every_non_removing_branch_is_recognised_and_sends_no_logout_whisper(reason: str) -> None:
    """Round 1's must-fix 2: `dml_uninvite.lua` answers ALL FOUR non-removing
    branches (player not found, bot not found, bot ungrouped, bot grouped with
    someone else) with the one shared marker plus that branch's own reason —
    "the one marker with the reason" of the lead's two offered options — and
    `dismiss()` must recognise every one of them the same way the single
    "moved to another party" case above is recognised: no logout whisper, no
    poll of a party this master was never confirmed against."""
    chan = _Chan({"dml_uninvite": Answer("yes", f"Newbot is not in Pakka's party now ({reason})")})
    result = party.dismiss(
        player="Pakka",
        bot="Newbot",
        send=chan.send,
        members=lambda: (party.Member("Newbot", 777, 8, 1),),
        sleep=lambda _s: None,
    )
    assert result.removed is False
    assert result.logged_out is False
    assert chan.sent == ["dml_uninvite Pakka Newbot"], (
        "a refused uninvite must not whisper logout or poll the table of a party "
        "this master was never confirmed against"
    )


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
    server: Path,
    sql: _Sql,
    chan: object | None,
    running: bool = True,
    level_setter: party.LevelSetter | None = None,
) -> party.InstallParty:
    return party.InstallParty(
        WOTLK,
        server,
        sql=sql,
        channel_for_saved=lambda: chan,
        container="ac-worldserver",
        world_running=lambda: running,
        engine=lambda: party.BinaryRead(True, "in"),
        level_setter=level_setter,
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


def test_a_group_read_that_keeps_failing_is_reported_as_unknown_not_removed() -> None:
    """T21 (T13 round 2 review, note 5). `_rows_only` folds a FAILED read into
    `()`, which is right for `add_bot`'s poll — watching for a guid to APPEAR,
    where a read that could not be done adds nothing either way — and wrong
    for this one, which is watching for a guid to VANISH: `()` reads as
    "gone". Round 2 of T13 removed the one route that made a failed read
    look like silence (the Lua's empty SOAP reply); what remains is a genuine
    uninvite whose follow-up read errors, and it must not become "removed" on
    a read that never happened."""
    chan = _Chan()
    result = party.dismiss(
        player="Pakka",
        bot="Newbot",
        send=chan.send,
        members=lambda: "could not read this character's party: connection refused",
        tries=2,
        sleep=lambda _s: None,
    )
    assert result.removed is False
    assert result.logged_out is True
    assert "Newbot" in result.sentence
    assert "could not read this character's party: connection refused" in result.sentence


def test_a_group_read_that_fails_once_does_not_fail_the_dismiss() -> None:
    """A transient failure between two polls is neither "still there" nor
    "already gone" — it is nothing yet, and the poll tries again rather than
    giving up on the first bad read."""
    reads = iter(["could not read this character's party: timeout", ()])
    result = party.dismiss(
        player="Pakka",
        bot="Newbot",
        send=_Chan().send,
        members=lambda: next(reads),
        sleep=lambda _s: None,
    )
    assert result.removed is True


# -- 8.6, T5: the chosen spec ----------------------------------------------
#
# Every fact in this section was measured on `yulon-ubuntu2` on 2026-09-09 by
# READING this tree's own `mod-playerbots` checkout and its shipped
# `playerbots.conf.dist`. None of it is the bash launcher's list: that list is a
# static mirror of a 2026-07-19 reading of a DIFFERENT tree, and two of its
# claims are false here (it has no `prot pve` for paladin, and it drops class 6
# entirely -- this conf has seven death-knight specs and this tree's `addclass`
# takes `dk`).

SPEC_CONF = """
# a comment
AiPlayerbot.PremadeSpecName.1.0 = arms pve
AiPlayerbot.PremadeSpecName.1.1 = fury pve
AiPlayerbot.PremadeSpecName.8.0 = arcane pve
AiPlayerbot.PremadeSpecName.8.1 = fire pve
AiPlayerbot.PremadeSpecLink.8.1.80 = 0-53-18
#AiPlayerbot.PremadeSpecName.8.2 = frost pve
AiPlayerbot.PremadeSpecName.6.0 = blood pve
"""
"""A conf shaped like the real one, with the three things that decide the answer:
a commented key (which is not a setting), a `PremadeSpecLink` sibling (which is
not a name), and a class the prior art's list dropped."""


def test_the_premade_specs_are_read_from_this_installs_own_conf() -> None:
    """The accepted names are the conf's, per class, and never a list in here.

    Measured (`mod-playerbots/src/PlayerbotAIConfig.cpp:487-493`, read on
    `yulon-ubuntu2` 2026-09-09): the module loads
    `AiPlayerbot.PremadeSpecName.<class>.<specno>` and `SpecPick` compares the
    whispered text to those values with `==`
    (`src/Ai/Base/Actions/ChangeTalentsAction.cpp:144`). So the conf IS the
    accepted list, and a hand-kept list here could only ever drift from it.
    """
    names = party.read_spec_names(SPEC_CONF)
    assert names[1] == ("arms pve", "fury pve")
    assert names[8] == ("arcane pve", "fire pve")


def test_a_death_knight_spec_is_read_here_although_the_prior_arts_list_drops_them() -> None:
    """Class 6 is a per-tree fact, and this tree has it both ways: `BOT_CLASSES`
    takes `dk` and the conf lists its specs. `_party_spec_names` skips class 6
    (`rust-main:cli/src/50-party.sh:157`), which here would offer a death knight
    no spec at all."""
    assert party.read_spec_names(SPEC_CONF)[6] == ("blood pve",)


def test_a_spec_number_the_conf_skips_hides_every_spec_after_it() -> None:
    """MEASURED, and not a guess about tidiness: `SpecPick` walks specno upwards
    and **breaks** at the first empty name
    (`ChangeTalentsAction.cpp:138-142`, `yulon-ubuntu2` 2026-09-09). So a gap at
    8.2 makes 8.3 unreachable no matter what the conf says about it -- offering
    it would be offering a name the module answers "not found" to, in the game
    window where nothing this app can read will see it."""
    conf = SPEC_CONF + "AiPlayerbot.PremadeSpecName.8.3 = frostfire pve\n"
    assert party.read_spec_names(conf)[8] == ("arcane pve", "fire pve")


def test_a_class_the_conf_says_nothing_about_has_no_specs_rather_than_a_default() -> None:
    assert party.read_spec_names(SPEC_CONF).get(11, ()) == ()


def test_the_class_ids_are_the_modules_own_and_the_class_list_is_derived_from_them() -> None:
    """One table, so the picker's class and the conf's class cannot disagree.

    `PlayerbotMgr.cpp:1093-1134` (`yulon-ubuntu2` 2026-09-09) is where `addclass`
    turns the word into the id: shaman is 7, druid is 11, dk is 6, and there is
    no 10. `BOT_CLASSES` is derived from this mapping rather than written beside
    it, because a second list is a list that will one day be missing a class the
    specs are keyed by."""
    assert party.BOT_CLASS_IDS["shaman"] == 7
    assert party.BOT_CLASS_IDS["druid"] == 11
    assert party.BOT_CLASS_IDS["dk"] == 6
    assert tuple(party.BOT_CLASS_IDS) == party.BOT_CLASSES


def test_the_spec_whisper_is_the_prior_arts_command() -> None:
    """`dml_whisper <master> <bot> talents spec <name>`
    (`rust-main:crates/dml-wow/src/party.rs:253`, `cli/src/90-main.sh:3976`).
    The same Lua is on the other side of the wire, so a second spelling would be
    a second protocol."""
    assert (
        party.spec_command("Pakka", "Bottom", "frost pve")
        == "dml_whisper Pakka Bottom talents spec frost pve"
    )


@pytest.mark.parametrize(
    "bad",
    ["", " arms pve", "arms pve; .server shutdown 1", 'arms "pve"', "arms\npve", "arms\\pve"],
)
def test_a_spec_that_could_break_out_of_the_whisper_never_reaches_the_channel(bad: str) -> None:
    """The tail of a command the world server parses, so the check is a boundary.

    The charset is the prior art's (`50-party.sh:208-215`) and it is WIDER than
    the shipped names' lowercase-and-spaces on purpose: `playerbots.conf` is
    hand-editable and the picker offers whatever it says, so refusing
    `Arms PvE` here would refuse a name the module accepts. What it must never
    admit is a quote, a backslash, a newline or a shell separator."""
    with pytest.raises(party.BadRequest):
        party.spec_command("Pakka", "Bottom", bad)


def test_a_spec_this_server_does_not_list_is_refused_before_anything_is_sent() -> None:
    """The whole reason the app validates at all: a name the module does not have
    is answered `Spec <x> not found` IN THE GAME WINDOW
    (`ChangeTalentsAction.cpp:157`), by `TellMasterNoFacing`. Nothing comes back
    over the channel, so a wrong spec sent is a bot that is silently not specced
    and an app that reported success."""
    chan = _Chan()
    result = party.add_bot(
        facts=_facts(),
        player="Pakka",
        klass="mage",
        spec="warglaive pvp",
        specs=("arcane pve", "fire pve"),
        send=chan.send,
        members=lambda: (),
    )
    assert chan.sent == []
    assert result.added is False
    assert result.blocker == result.sentence
    assert "warglaive pvp" in result.sentence
    assert "arcane pve" in result.sentence


def test_a_spec_asked_for_where_this_install_lists_none_says_so_rather_than_an_empty_list() -> None:
    """A conf that was never deployed lists nothing, and "choose one of: " with
    nothing after it is a refusal that tells a person to do the impossible."""
    chan = _Chan()
    result = party.add_bot(
        facts=_facts(),
        player="Pakka",
        klass="mage",
        spec="fire pve",
        specs=(),
        send=chan.send,
        members=lambda: (),
    )
    assert chan.sent == []
    assert "playerbots.conf" in result.sentence


def test_the_chosen_spec_is_whispered_after_the_join_and_the_gear_follows_it() -> None:
    """Three orderings, and each is a measured reason rather than a preference.

    * The spec is whispered AFTER the join, because it is addressed to the bot by
      name and there is no name until the group table has one.
    * `autogear` follows the spec, because gear must match the new talents
      (`90-main.sh:3975-3977`, whose own comment says so).
    * `talents autopick` is NOT sent, because it is the thing the chosen spec
      replaces -- `InitTalentsTree(true)` after `InitTalentsBySpecNo` would pick
      the talents again and the chosen spec would be gone
      (`ChangeTalentsAction.cpp:57-59` and `:146`).
    """
    fresh = party.Member(name="Newbot", guid=777, klass=8, level=1)
    reads = iter([(), (fresh,), (fresh,)])
    chan = _Chan()
    result = party.add_bot(
        facts=_facts(),
        player="Pakka",
        klass="mage",
        spec="fire pve",
        specs=("arcane pve", "fire pve"),
        send=chan.send,
        members=lambda: next(reads),
        sleep=lambda _s: None,
    )
    assert result.spec == "fire pve"
    assert result.specced is True
    assert "dml_whisper Pakka Newbot talents autopick" not in chan.sent
    assert chan.sent.index("dml_whisper Pakka Newbot talents spec fire pve") < chan.sent.index(
        "dml_whisper Pakka Newbot autogear"
    )
    assert "fire pve" in result.sentence


def test_the_sentence_never_claims_the_talents_took() -> None:
    """The module answers a spec only in the game window, so `spec_sent` is the
    whisper being accepted and nothing more. A sentence saying the bot IS fire
    pve would be this app asserting something it cannot read -- the readback is
    the bot's talent table, and that is the live half's job."""
    fresh = party.Member(name="Newbot", guid=777, klass=8, level=1)
    reads = iter([(), (fresh,), (fresh,)])
    result = party.add_bot(
        facts=_facts(),
        player="Pakka",
        klass="mage",
        spec="fire pve",
        specs=("fire pve",),
        send=_Chan().send,
        members=lambda: next(reads),
        sleep=lambda _s: None,
    )
    assert "in the game window" in result.sentence


def test_no_spec_leaves_the_finishing_pair_exactly_as_it_was() -> None:
    """The prior art's rule for its own spec branch: "No --spec keeps this branch
    byte-identical to before" (`90-main.sh:3973`). This tree's gear-then-autopick
    pair was proved live on 2026-09-09 and a reorder nobody measured is a change
    to a working path."""
    fresh = party.Member(name="Newbot", guid=777, klass=8, level=1)
    reads = iter([(), (fresh,)])
    chan = _Chan()
    party.add_bot(
        facts=_facts(),
        player="Pakka",
        klass="mage",
        send=chan.send,
        members=lambda: next(reads),
        sleep=lambda _s: None,
    )
    assert chan.sent == [
        "dml_addclass Pakka mage",
        "dml_whisper Pakka Newbot autogear",
        "dml_whisper Pakka Newbot talents autopick",
    ]


# -- 8.6, T5: the chosen level ----------------------------------------------
#
# The level is 8.4a's seam and a bot is a character, so this sends
# `play.InstallPlay.set_level` and never a second level path. What is measured
# per install is the BOUND: `MaxPlayerLevel = 80` in
# `env/dist/etc/worldserver.conf` on `yulon-ubuntu2`, 2026-09-09. Eighty is not
# written into this app anywhere -- it is a conf value on one install of one
# tree, and the fork that ships 255 or 60 would be silently mis-bounded by a
# constant.


class _Level:
    """A stand-in for the Characters tab's `set_level`, recorded into one log.

    One shared log with the channel, because the ORDER between them is the thing
    two of these tests are about and two separate lists cannot show it.
    """

    def __init__(self, log: list[str], outcome: Outcome | None = None) -> None:
        self.log = log
        self.outcome = outcome or Outcome(True, "level changed")
        self.asked: list[tuple[str, int]] = []

    def __call__(self, character: str, level: int) -> Outcome:
        self.asked.append((character, level))
        self.log.append(f"set_level {character} {level}")
        return self.outcome


class _LoggedChan(_Chan):
    """`_Chan` writing into a shared log as well as its own list."""

    def __init__(self, log: list[str], answers: dict[str, Answer] | None = None) -> None:
        super().__init__(answers)
        self.log = log

    def send(self, command: str) -> Answer:
        self.log.append(command)
        return super().send(command)


def test_the_maximum_level_is_read_from_this_servers_own_worldserver_conf(tmp_path: Path) -> None:
    """Measured on `yulon-ubuntu2` 2026-09-09: `MaxPlayerLevel = 80`, at
    `env/dist/etc/worldserver.conf:2128`. Read rather than assumed, and read with
    the same column-0 rule `read_conf` uses -- the shipped `.dist` carries
    commented keys, and a pattern that matched them would read the file's prose
    as its settings."""
    server = tmp_path / "wowserver"
    (server / "env" / "dist" / "etc").mkdir(parents=True)
    (server / party.WORLD_CONF).write_text(
        "#MaxPlayerLevel = 255\nStartPlayerLevel = 1\nMaxPlayerLevel = 80\n"
    )
    assert party.max_player_level(server) == 80


def test_a_maximum_level_that_could_not_be_read_is_none_and_not_a_guess(tmp_path: Path) -> None:
    """A conf that is not there has said nothing about the level cap. `None` is a
    third answer here for the same reason it is one in `ConfRead`: a default of
    80 would be this app inventing a fact about somebody's fork."""
    assert party.max_player_level(tmp_path / "nowhere") is None


def test_a_level_above_this_servers_own_maximum_is_refused_before_anything_is_sent() -> None:
    """The bound is the server's, so the refusal names the server's number."""
    chan = _Chan()
    result = party.add_bot(
        facts=_facts(),
        player="Pakka",
        klass="mage",
        level=90,
        max_level=80,
        set_level=_Level([]),
        send=chan.send,
        members=lambda: (),
    )
    assert chan.sent == []
    assert result.added is False
    assert result.blocker == result.sentence
    assert "80" in result.sentence


def test_a_level_asked_for_where_the_maximum_could_not_be_read_is_refused() -> None:
    """Not bounded by a guess and not sent unbounded: the app says it could not
    read the cap, which is the thing a person can fix."""
    chan = _Chan()
    result = party.add_bot(
        facts=_facts(),
        player="Pakka",
        klass="mage",
        level=60,
        max_level=None,
        set_level=_Level([]),
        send=chan.send,
        members=lambda: (),
    )
    assert chan.sent == []
    assert "MaxPlayerLevel" in result.sentence


def test_a_level_below_one_is_refused_before_anything_is_sent() -> None:
    chan = _Chan()
    result = party.add_bot(
        facts=_facts(),
        player="Pakka",
        klass="mage",
        level=0,
        max_level=80,
        set_level=_Level([]),
        send=chan.send,
        members=lambda: (),
    )
    assert chan.sent == []
    assert result.blocker == result.sentence


def test_the_level_is_set_on_the_bot_and_read_back_out_of_the_group_table() -> None:
    """`characters.level` after the press, which is what the design asks for
    (`pyplan/phase8-designs/b-users-surface.md:422`) -- not the number the button
    sent, and not the level command's own `yes`. The group read already carries
    it (`Member.level` IS `characters.level`), so the readback costs one more
    read of a query this press already makes."""
    log: list[str] = []
    joined_low = party.Member("Newbot", 777, 8, 1)
    joined_high = party.Member("Newbot", 777, 8, 60)
    reads = iter([(), (joined_low,), (joined_high,)])
    level = _Level(log)
    result = party.add_bot(
        facts=_facts(),
        player="Pakka",
        klass="mage",
        level=60,
        max_level=80,
        set_level=level,
        send=_LoggedChan(log).send,
        members=lambda: next(reads),
        sleep=lambda _s: None,
    )
    assert level.asked == [("Newbot", 60)]
    assert (result.level, result.level_before, result.level_after) == (60, 1, 60)
    assert "characters.level read 1 before the press and 60 after" in result.sentence


def test_a_bot_already_at_the_chosen_level_is_told_so_rather_than_counted_as_a_change() -> None:
    """The gate rule in the app's own voice: a step whose assertion was already
    true before its action has proved nothing. A bot the server made at 60, asked
    for 60, reads back 60 -- and reporting that as "now at level 60" is the
    sentence that would let the live gate certify a level it never changed."""
    log: list[str] = []
    already = party.Member("Newbot", 777, 8, 60)
    reads = iter([(), (already,), (already,)])
    result = party.add_bot(
        facts=_facts(),
        player="Pakka",
        klass="mage",
        level=60,
        max_level=80,
        set_level=_Level(log),
        send=_LoggedChan(log).send,
        members=lambda: next(reads),
        sleep=lambda _s: None,
    )
    assert (result.level_before, result.level_after) == (60, 60)
    assert "already" in result.sentence


def test_a_level_the_group_table_does_not_show_afterwards_is_not_reported_as_set() -> None:
    """The command accepted and `characters.level` unchanged is a third state, and
    it is the one a console that answered `yes` to nothing looks like."""
    log: list[str] = []
    low = party.Member("Newbot", 777, 8, 1)
    reads = iter([(), (low,), (low,)])
    result = party.add_bot(
        facts=_facts(),
        player="Pakka",
        klass="mage",
        level=60,
        max_level=80,
        set_level=_Level(log),
        send=_LoggedChan(log).send,
        members=lambda: next(reads),
        sleep=lambda _s: None,
    )
    assert (result.level_before, result.level_after) == (1, 1)
    assert "still reads 1, not 60" in result.sentence
    # Live on `yulon-ubuntu2` 2026-09-09 this arm was the answer to two presses,
    # and both are in the gate folder: `panel-8-…png` (42 asked, row 1) and
    # `panel-9-…png` with `panel-transcript.log:61-130` (55 asked, row 1 for the
    # sixty-eight seconds the bot was in the party, 13:17:35 to 13:18:43).
    # What the folder does NOT hold is a forced save leaving it unwritten, so the
    # sentence claims no mechanism -- it says what was read, and where that
    # leaves the reader.
    assert "stay unwritten for as long as this panel watched it" in result.sentence
    assert "not set until the row says otherwise" in result.sentence


def test_a_refused_level_does_not_stop_the_spec_or_the_gear() -> None:
    """One refusal must not hide the others, and it must not cancel them either:
    the level is the Characters tab's console command and the spec is a whisper
    through the bridge, so a tree with no `character level` still gets its spec."""
    log: list[str] = []
    joined = party.Member("Newbot", 777, 8, 1)
    reads = iter([(), (joined,), (joined,)])
    result = party.add_bot(
        facts=_facts(),
        player="Pakka",
        klass="mage",
        spec="fire pve",
        specs=("fire pve",),
        level=60,
        max_level=80,
        set_level=_Level(log, Outcome(False, problem="There is no such subcommand")),
        send=_LoggedChan(log).send,
        members=lambda: next(reads),
        sleep=lambda _s: None,
    )
    assert "There is no such subcommand" in result.sentence
    assert "dml_whisper Pakka Newbot talents spec fire pve" in log
    assert "dml_whisper Pakka Newbot autogear" in log
    assert result.specced is True


def test_the_level_is_set_before_the_spec_is_whispered() -> None:
    """MEASURED, and the reason the order is not a preference: `SpecPick` applies
    the premade build through `PlayerbotFactory factory(bot, bot->GetLevel())`
    (`ChangeTalentsAction.cpp:146-149`, `yulon-ubuntu2` 2026-09-09). A spec
    applied at level 1 and then levelled to 60 is a bot with a level-1 build."""
    log: list[str] = []
    joined = party.Member("Newbot", 777, 8, 1)
    reads = iter([(), (joined,), (joined,)])
    party.add_bot(
        facts=_facts(),
        player="Pakka",
        klass="mage",
        spec="fire pve",
        specs=("fire pve",),
        level=60,
        max_level=80,
        set_level=_Level(log),
        send=_LoggedChan(log).send,
        members=lambda: next(reads),
        sleep=lambda _s: None,
    )
    assert log.index("set_level Newbot 60") < log.index(
        "dml_whisper Pakka Newbot talents spec fire pve"
    )


def test_a_level_with_no_route_to_set_it_raises_rather_than_reporting_a_send() -> None:
    """A caller asking for a level with no setter is a mistake in this app and not
    a refusal to show a user, so it is raised the way a bad class name is."""
    with pytest.raises(party.BadRequest):
        party.add_bot(
            facts=_facts(),
            player="Pakka",
            klass="mage",
            level=60,
            max_level=80,
            send=_Chan().send,
            members=lambda: (),
        )


# -- 8.6, T5: dismiss all ---------------------------------------------------


def test_dismiss_all_sends_every_bot_away_and_names_each_one() -> None:
    """Each dismissal reported by name. A count alone ("3 bots dismissed") is a
    sentence nobody can check against the party frame in front of them."""
    chan = _Chan()
    result = party.dismiss_all(
        player="Pakka",
        bots=("Anmi", "Jilsur"),
        send=chan.send,
        members=lambda: (),
        sleep=lambda _s: None,
    )
    assert result.attempted == 2
    assert [d.bot for d in result.dismissals] == ["Anmi", "Jilsur"]
    assert all(d.removed for d in result.dismissals)
    assert "Anmi" in result.sentence
    assert "Jilsur" in result.sentence
    assert chan.sent == [
        "dml_uninvite Pakka Anmi",
        "dml_whisper Pakka Anmi logout",
        "dml_uninvite Pakka Jilsur",
        "dml_whisper Pakka Jilsur logout",
    ]


def test_one_bot_that_will_not_leave_does_not_hide_the_others() -> None:
    """The prior art's own review finding, in the other direction: "one
    unreachable bot must not strand the rest of the party"
    (`90-main.sh:4076`). And the bot that stayed is named with the reason it
    stayed, rather than subtracted from a count."""

    class _Half(_Chan):
        def send(self, command: str) -> Answer:
            self.sent.append(command)
            if command == "dml_uninvite Pakka Anmi":
                return Answer("no", "Command 'dml_uninvite' does not exist")
            return Answer("yes", "ok")

    chan = _Half()
    result = party.dismiss_all(
        player="Pakka",
        bots=("Anmi", "Jilsur"),
        send=chan.send,
        # Neither row is in this reading, so the only thing that decides Anmi is
        # the refusal to its uninvite.
        members=lambda: (),
        sleep=lambda _s: None,
    )
    assert [d.removed for d in result.dismissals] == [False, True]
    assert "Anmi" in result.sentence
    assert "does not exist" in result.sentence
    assert "Jilsur" in result.sentence
    assert "dml_uninvite Pakka Jilsur" in chan.sent, "the refusal must not stop the next bot"


def test_dismiss_all_with_no_bots_sends_nothing_and_says_so() -> None:
    """Nothing to dismiss is a refusal with a `blocker`, which is this module's
    word for "the channel was never touched"."""
    chan = _Chan()
    result = party.dismiss_all(
        player="Pakka", bots=(), send=chan.send, members=lambda: (), sleep=lambda _s: None
    )
    assert chan.sent == []
    assert result.attempted == 0
    assert result.blocker == result.sentence
    assert result.dismissals == ()


def test_the_seam_reads_the_party_before_dismissing_all_of_it(tmp_path: Path) -> None:
    """The list comes from the group table at the moment of the press, not from
    whatever the panel last drew: the bot manager logs bots in and out on a timer
    and a list read minutes ago is 8.4d's finding all over again.

    The caller supplies what it is ALLOWED to dismiss, and the table supplies who
    that is — the guid confirmed is matched against the fresh read, and the name
    that goes into `dml_uninvite` comes from the row, never from the caller."""
    chan = _Chan({"dml_bridge_ping": Answer("yes", "DML-BRIDGE-READY dml_bridge_ping")})
    sql = _Sql("1001\n", "Anmi\t949\t8\t1\n", "1001\n", "")
    result = _install(_ready_install(tmp_path), sql, chan).remove_all("Pakka", (949,))
    assert result.attempted == 1
    assert [d.bot for d in result.dismissals] == ["Anmi"]
    assert "dml_uninvite Pakka Anmi" in chan.sent


def test_the_seam_dismisses_nothing_where_the_party_could_not_be_read(tmp_path: Path) -> None:
    """A read that failed is not an empty party: reporting "no bots to dismiss"
    for a group table nobody could read is the same lie `_rows_only` exists to
    keep out of the poll."""
    chan = _Chan({"dml_bridge_ping": Answer("yes", "DML-BRIDGE-READY dml_bridge_ping")})
    sql = _Sql("")  # the online lookup finds nobody
    result = _install(_ready_install(tmp_path), sql, chan).remove_all("Pakka", (949,))
    assert result.attempted == 0
    assert "logged in" in result.sentence
    assert chan.sent == []


class _AlwaysFailsAfterOnline:
    """`online_guid` always resolves; every group-table read raises.

    An outage that starts the moment the press goes out: `dml_uninvite` and
    the logout whisper still land (this is not a channel that is down), but
    the poll's own read errors on every attempt."""

    def __init__(self, guid: str = "1001\n") -> None:
        self._guid = guid

    def query(self, db: str, statement: str) -> str:
        if "group_member" in statement:
            raise RuntimeError("connection refused")
        return self._guid


class _SucceedsOnceThenFails:
    """The group table reads once (the seam's own pre-dismiss confirmation)
    and raises on every read after -- an outage starting after the batch is
    already confirmed and the presses are already going out."""

    def __init__(self, first_group_row: str, guid: str = "1001\n") -> None:
        self._guid = guid
        self._first_group_row = first_group_row
        self._group_reads = 0

    def query(self, db: str, statement: str) -> str:
        if "group_member" in statement:
            self._group_reads += 1
            if self._group_reads == 1:
                return self._first_group_row
            raise RuntimeError("connection refused")
        return self._guid


def test_the_seam_does_not_report_removed_when_every_poll_read_fails(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """T21, round 2's must-fix 1 (Codex adversarial review, round 1): `dismiss`'s
    own unit tests call it directly with an already-unfolded `members`, so a
    regression that put `_rows_only` back between `InstallParty.remove` and the
    poll would sail through them unnoticed. This goes through the seam instead.

    `InstallParty.remove` exposes no `tries`/`pause`/`sleep` of its own, so the
    poll's real wait is silenced the same way `dismiss`'s own tests silence it
    -- by reaching past the seam to the one place that wait is injectable,
    `dismiss.__kwdefaults__["sleep"]` -- rather than actually waiting out
    `POLL_TRIES * POLL_SLEEP` (6 seconds) for a database that is never coming
    back in this test.
    """
    monkeypatch.setitem(party.dismiss.__kwdefaults__, "sleep", lambda _s: None)
    chan = _Chan()
    result = _install(_ready_install(tmp_path), _AlwaysFailsAfterOnline(), chan).remove(
        "Pakka", "Newbot"
    )
    assert result.removed is False
    assert result.unreadable is True
    assert "connection refused" in result.sentence


def test_the_batch_seam_does_not_report_removed_when_every_poll_read_fails(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The same finding on the batch route (Codex adversarial review, round 1,
    must-fix 1): round 1 of this ticket fixed `InstallParty.remove` and left
    `InstallParty.remove_all` handing `dismiss_all` the `_rows_only`-folded
    read, so a database outage mid-batch still reported every bot in it
    `removed=True`. `remove_all`'s own confirmation read succeeds once (that is
    what lets the batch start); the poll behind it never gets another good
    read."""
    monkeypatch.setitem(party.dismiss_all.__kwdefaults__, "sleep", lambda _s: None)
    chan = _Chan({"dml_bridge_ping": Answer("yes", "DML-BRIDGE-READY dml_bridge_ping")})
    sql = _SucceedsOnceThenFails("Anmi\t949\t8\t1\n")
    result = _install(_ready_install(tmp_path), sql, chan).remove_all("Pakka", (949,))
    assert result.attempted == 1
    assert not any(d.removed for d in result.dismissals)
    assert all(d.unreadable for d in result.dismissals)
    assert "connection refused" in result.sentence


# -- 8.6, T5: three buckets for a mass dismissal (T21, round 2) -------------


def test_a_mass_dismissal_where_every_bot_is_unreadable_says_so_not_stayed() -> None:
    """T21, round 2's must-fix 2 (Codex adversarial review, round 1): "Still
    here" asserts a row somebody actually saw. A read that never happened
    cannot say that, so it gets its own words rather than being folded into
    the bucket for bots the poll actually watched stay."""
    result = party.dismiss_all(
        player="Pakka",
        bots=("Anmi", "Jilsur"),
        send=_Chan().send,
        members=lambda: "could not read this character's party: connection refused",
        tries=1,
        sleep=lambda _s: None,
    )
    assert not any(d.removed for d in result.dismissals)
    assert "None of 2 bots were confirmed to have left the party" in result.sentence
    assert "None of 2 bots left the party" not in result.sentence, (
        "two polls that never read the table cannot say nobody left; all two may have"
    )
    assert "Still here" not in result.sentence
    assert "Could not be confirmed" in result.sentence
    assert "connection refused" in result.sentence


def test_a_mixed_mass_dismissal_names_each_bucket_in_its_own_words() -> None:
    """One bot that went, one the bridge refused outright (a row somebody
    read), and one whose table could not be read at all: three different
    findings, and the sentence must not collapse the last two together."""

    class _Mixed(_Chan):
        def send(self, command: str) -> Answer:
            self.sent.append(command)
            if command == "dml_uninvite Pakka Jilsur":
                return Answer(
                    "yes",
                    "Jilsur is not in Pakka's party now (Jilsur is grouped with someone else)",
                )
            return Answer("yes", "ok")

    reads = iter([(), "could not read this character's party: timeout"])
    result = party.dismiss_all(
        player="Pakka",
        bots=("Anmi", "Jilsur", "Newbot"),
        send=_Mixed().send,
        members=lambda: next(reads),
        tries=1,
        sleep=lambda _s: None,
    )
    assert [d.removed for d in result.dismissals] == [True, False, False]
    sentence = result.sentence
    assert "1 of 3 bots confirmed left the party: Anmi." in sentence, (
        "one read never happened, so the count is of what was confirmed"
    )
    still_here = sentence.split("Still here — ")[1].split(" Could not be confirmed")[0]
    unreadable = sentence.split("Could not be confirmed — ")[1]
    assert "Jilsur" in still_here and "Newbot" not in still_here
    assert "Newbot" in unreadable and "Jilsur" not in unreadable


# -- 8.6, T5: what the seam offers the panel --------------------------------


def test_the_seam_offers_the_specs_this_install_lists_for_the_chosen_class(tmp_path: Path) -> None:
    """Class name in, this install's names out. The mapping is the module's own
    (`BOT_CLASS_IDS`) so the picker cannot ask for a class the conf is not keyed
    by."""
    server = _ready_install(tmp_path)
    (server / party.PLAYERBOTS_CONF).write_text(SPEC_CONF)
    seam = _install(server, _Sql(), None)
    assert seam.specs("mage") == ("arcane pve", "fire pve")
    assert seam.specs("dk") == ("blood pve",)
    assert seam.specs("druid") == ()


def test_a_shipped_dist_conf_is_not_a_deployed_one_and_offers_no_specs(tmp_path: Path) -> None:
    """The inverse of what this file asserted until the live gate ran it.

    It used to read the `.dist` when no conf was deployed, on the prior art's
    example and on a claim written here that nobody had checked -- that the
    server falls back to it too. **Measured on `yulon-ubuntu2`, 2026-09-09**: that
    box has a `.dist` and no `playerbots.conf`, and the module answered
    `talents spec list` with `Total 0 specs found`. Every one of the 63 names
    this app had offered came back `Spec <name> not found`, in the game window,
    where nothing here can hear it.

    So an install with only a `.dist` has NO premade specs, the picker offers
    only the server's own pick, and asking for one is refused by the app in its
    own words."""
    server = _ready_install(tmp_path)
    (server / (party.PLAYERBOTS_CONF + ".dist")).write_text(SPEC_CONF)
    assert _install(server, _Sql(), None).specs("mage") == ()


def test_a_deployed_conf_beside_a_dist_is_the_one_that_is_read(tmp_path: Path) -> None:
    """The control: the deployed file wins, and its contents are what is offered.

    Written with the two files DISAGREEING, because agreeing files cannot tell a
    reader that prefers the wrong one from a reader that prefers the right one.
    """
    server = _ready_install(tmp_path)
    (server / (party.PLAYERBOTS_CONF + ".dist")).write_text(SPEC_CONF)
    (server / party.PLAYERBOTS_CONF).write_text("AiPlayerbot.PremadeSpecName.8.0 = deployed only\n")
    assert _install(server, _Sql(), None).specs("mage") == ("deployed only",)


def test_the_seam_reads_this_installs_maximum_level(tmp_path: Path) -> None:
    server = _ready_install(tmp_path)
    (server / party.WORLD_CONF).write_text("MaxPlayerLevel = 60\n")
    assert _install(server, _Sql(), None).max_level() == 60


def test_the_seam_sets_a_level_through_the_characters_tab_and_not_a_second_path(
    tmp_path: Path,
) -> None:
    """8.4a's seam, reused. Two level paths would be two spellings of
    `character level` and two places to fix the day a fork renames it -- and the
    Characters tab's is the one that already refuses the trees with no such
    command, in the entry's own measured words."""
    seam = _install(_ready_install(tmp_path), _Sql(), None)
    assert seam.level_setter.__self__.__class__ is play.InstallPlay  # type: ignore[attr-defined]
    assert seam.level_setter.__name__ == "set_level"  # type: ignore[attr-defined]


def test_the_seam_hands_the_specs_the_bound_and_the_level_setter_to_the_press(
    tmp_path: Path,
) -> None:
    """Round 1's third finding: the three hand-offs had no test of their own.

    Every spec and level test above calls `add_bot` directly and supplies
    `specs=`, `max_level=` and `set_level=` itself, so the three lines in
    `InstallParty.add` that fill them in were covered by nothing -- blanking any
    of them (`specs=()`, `max_level=None`, `set_level=None`) left every test
    green while the panel's own press refused every spec, refused every level,
    or raised. This is the one press that goes the whole way: two conf files on
    disk, the group table read three times -- the ground, the poll, the level
    readback -- and the level setter injected so the console is not.
    """
    server = _ready_install(tmp_path)
    (server / party.PLAYERBOTS_CONF).write_text(SPEC_CONF)
    (server / party.WORLD_CONF).write_text("MaxPlayerLevel = 80\n")
    log: list[str] = []
    level = _Level(log)
    chan = _LoggedChan(log, {"dml_bridge_ping": Answer("yes", "DML-BRIDGE-READY dml_bridge_ping")})
    sql = _Sql(
        "1001\n",  # online_guid for the press
        "1001\n",  # the ground reading: online_guid…
        "",  # …and an empty group
        "1001\n",  # the poll: online_guid…
        "Newbot\t777\t8\t1\n",  # …and the bot, at the level the server made it
        "1001\n",  # the level readback: online_guid…
        "Newbot\t777\t8\t60\n",  # …and characters.level after the press
    )

    result = _install(server, sql, chan, level_setter=level).add(
        "Pakka", "mage", spec="fire pve", level=60
    )

    assert result.blocker == "", result.sentence
    assert level.asked == [("Newbot", 60)], "the level went through the seam it was handed"
    assert "dml_whisper Pakka Newbot talents spec fire pve" in chan.sent
    assert "dml_whisper Pakka Newbot talents autopick" not in chan.sent
    assert (result.spec, result.level_before, result.level_after) == ("fire pve", 1, 60)


def test_the_seam_refuses_a_party_that_is_not_the_one_that_was_confirmed(tmp_path: Path) -> None:
    """Round 2's must-fix, at the end that can actually enforce it.

    A panel comparing its own rows guards the wrong side of the door: between
    the two presses the group table can gain a bot nobody agreed to send away,
    and the panel would see nothing change. So the confirmed identities travel
    with the press and the SEAM re-reads: the fresh set must match exactly, or
    nothing is sent and the sentence says the party is not the one confirmed.
    Guids rather than names because a guid is what the group table has -- two
    bots can be renamed, and `characters.name` is not what `group_member` keys.
    """
    chan = _Chan({"dml_bridge_ping": Answer("yes", "DML-BRIDGE-READY dml_bridge_ping")})
    sql = _Sql("1001\n", "Anmi\t949\t8\t1\nKobo\t950\t8\t1\n")

    result = _install(_ready_install(tmp_path), sql, chan).remove_all("Pakka", (949,))

    assert result.attempted == 0
    assert result.dismissals == ()
    assert result.blocker == result.sentence
    assert "confirmed" in result.sentence
    assert chan.sent == [], "not one uninvite may go out"


def test_the_seam_refuses_a_party_that_lost_a_bot_since_it_was_confirmed(tmp_path: Path) -> None:
    """Both directions, because a set that shrank is not the set agreed to either
    -- and a subset that "obviously" still works is how a confirmation quietly
    becomes a suggestion."""
    chan = _Chan({"dml_bridge_ping": Answer("yes", "DML-BRIDGE-READY dml_bridge_ping")})
    sql = _Sql("1001\n", "Anmi\t949\t8\t1\n")

    result = _install(_ready_install(tmp_path), sql, chan).remove_all("Pakka", (949, 950))

    assert result.attempted == 0
    assert chan.sent == []
