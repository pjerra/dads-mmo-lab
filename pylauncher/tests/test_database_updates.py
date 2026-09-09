"""The button that applies an install plan's re-runnable phases to a server that exists.

T11 made the route: a phase declared `rerun_on_marked` is applied by `_import`
to an install the probe already reads as finished, before the world starts, with
no marker written. Its reviewer then found the route had no way in for a GUI
user — the catalog tile greys to "Installed" once the app knows the folder, and
`rebuild_stages()` excludes `import` on purpose — so for an established Tortoise
install the sentence that opened T11 was still true: no button in the app
applies those three files, and the only caller was the CLI harness.

This file is about the second route. `update_stages()` is the database started
alone and the import stage, and nothing else — no `up`, no compile, no compose
regeneration — and `update_databases()` refuses before any of it while the world
server is up or unreadable, because the route writes DDL into the character
database and a running worldserver holds that database in memory and saves it
back over whatever it finds (owner answer 7, checklist 8.7a).

The second precondition is the one round 1 did not have, and it is the larger
half. `import` is the family's own stage and the flagged-phase route is ONE arm
of a five-branch table: `absent` runs the whole plan and writes a completion
marker, `partial` drops every schema the plan names first. A press that consented
to a named list of files must reach neither, so `ctx.updates_only` is read before
`stage_import()` is called at all and the ordinary import is unreachable on this
route rather than guarded on it.

What is unit-tested only: no press was run on a box for this change. The live
half is `pyplan/gates/tortoise-updates-button-m910q-2026-09-09/`, and until it
exists the m910q evidence for this route is T11's, through the CLI harness.
"""

from __future__ import annotations

import threading
from pathlib import Path

import pytest

from tests.support_native import Recorder
from tests.test_families_cmangos import ENTRY as CM_ENTRY
from tests.test_families_cmangos import (
    EVERY_PRESS,
    IMPORTED_OLDER_PLAN,
    MARKED_ONLY,
    POPULATED_AND_COMPLETE,
    engine_with_sql,
    entry_with_sql,
    ready_to_import,
    rerun_plan,
)
from yulon import docker
from yulon.catalog import native
from yulon.catalog.catalog import SqlPhase, load_catalog
from yulon.catalog.families import sqlplan
from yulon.catalog.families.cmangos import CmangosInstaller
from yulon.catalog.installer import InstallerError, InstallOptions, installer_for

TORTOISE = load_catalog().get("wow-tortoise")
WOTLK = load_catalog().get("wow-wotlk")


@pytest.fixture(autouse=True)
def gated(monkeypatch: pytest.MonkeyPatch) -> None:
    """`CmangosInstaller._gate` is the double `engine_with_sql(plan, rec)` attached.

    `test_families_cmangos.py`'s own `gated` is autouse THERE and reaches
    nothing here, so an engine driven from this file would take the real
    `MarkerGate` to a database that does not exist — and answer `absent`, which
    on the INSTALL route is the arm that imports the whole plan. On this route it
    is a refusal (`_only_the_rerunnable_phases`), and the fixture is still what
    makes these tests about the family rather than about a missing daemon. Copied
    rather than shared: `test_rebuild.py` keeps the same four lines under the
    name `cmangos_gate` and says so.
    """

    def gate(self: CmangosInstaller, ctx: native.StageContext) -> native.ImportGate:
        attached = getattr(self, "_test_gate", None)
        assert attached is not None, "build engines with engine_with_sql(plan, rec) in this file"
        return attached

    monkeypatch.setattr(CmangosInstaller, "_gate", gate, raising=True)


def a_world_that_is(answer: bool | None) -> object:
    """The `world_running` seam, answering the same thing however often it is asked."""

    def world_running(container: str) -> bool | None:
        return answer

    return world_running


def worlds(*answers: bool | None) -> tuple[object, list[str]]:
    """A `world_running` seam whose answers change between readings, and its record.

    A list and not a counter because the second reading is the point: the
    database start between the two waits for the container to report healthy,
    and a Server-tab Start inside that window puts a live world behind the very
    tables the next statement writes.
    """
    asked: list[str] = []
    remaining = list(answers)

    def world_running(container: str) -> bool | None:
        asked.append(container)
        return remaining.pop(0) if len(remaining) > 1 else remaining[0]

    return world_running, asked


def files_on_disk(server_dir: Path, *names: str) -> None:
    """Lay the files a phase's glob will match, with a line naming each."""
    for name in names:
        path = server_dir / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(f"-- {name}\nSELECT 1;\n", encoding="utf-8")


def file_plan() -> object:
    """`rerun_plan()`'s pair, with the flagged phase reading files instead of statements.

    The confirmation's subject is the file list, and a statements-only plan has
    none. The unflagged phase stays a statement phase so the assertions about
    what did NOT move read the same as they do everywhere else in this file.
    """
    plan = rerun_plan()
    return plan.model_copy(
        update={
            "phases": (
                plan.phases[0],
                SqlPhase(
                    name="character updates",
                    into=CM_ENTRY.databases.characters,
                    files=("src/updates/*.sql",),
                    sort="name",
                    rerun_on_marked=True,
                ),
            )
        }
    )


# -- the tuple ---------------------------------------------------------------


def test_the_updates_tuple_is_the_database_and_the_import_and_nothing_else() -> None:
    """Two stages, named, in this order — and the four a press must never re-enter.

    `up` is the one that matters most and is why the list is spelled out rather
    than counted: this press is for a server somebody is playing on and has
    stopped, and starting the world at the end of it would put the world back
    up under a user who stopped it to let this run. The other three are the
    install's own — a compile, a compose regeneration and a Dockerfile
    re-render have nothing to do with three SQL files.

    Catches the tuple built from `stages()` with the finished stages skipped
    (which is what `rebuild_stages()`'s docstring says a rebuild deliberately
    is not, for the same reasons), and any stage appended to it.
    """
    engine = installer_for(TORTOISE)
    names = [stage.name for stage in engine.update_stages()]
    assert names == ["start-db", "import"]
    for forbidden in ("up", "build", "generate-compose", "write-dockerfile"):
        assert forbidden not in names, names


def test_the_import_stage_of_an_updates_press_is_never_written_into_the_state_file() -> None:
    """`recorded=False`, and it is the family's own stage with the record taken off.

    The install's `import` stage is recorded, and rightly: it imported. This
    press reaches the same body and the body does NOT import — on a marked
    install `_import` runs the probe's table, applies the flagged phases and
    returns — so a record written here would claim a stage that did not happen,
    and an install whose state file never recorded `import` (one made by the
    shell scripts, or one killed mid-install) would have its next resume skip
    the import on the strength of it.

    Catches `replace(..., recorded=False)` dropped from `update_stages()`.
    """
    engine = installer_for(TORTOISE)
    by_name = {stage.name: stage for stage in engine.update_stages()}
    assert by_name["import"].recorded is False
    assert by_name["start-db"].recorded is False
    assert engine.stage_named("import").recorded is True, "the install's own is still recorded"


def test_the_press_runs_only_the_phases_the_plan_declares_rerunnable(tmp_path: Path) -> None:
    """The whole point, at the engine: the flagged phase moved and nothing else did.

    The pair is what makes it readable — a plan carrying only the flagged phase
    could not tell "the flag was honoured" from "the whole plan ran again" —
    and it is T11's own fixture, so this asserts the button reaches that route
    rather than a second copy of it.

    Catches `update_stages()` returning the install's whole tuple, and the
    press reaching `_import`'s ordinary import path.
    """
    server_dir = tmp_path / "srv"
    server_dir.mkdir()
    rec = ready_to_import(IMPORTED_OLDER_PLAN)
    engine = engine_with_sql(rerun_plan(), rec, world_running=a_world_that_is(False))
    said = list(engine.update_databases(InstallOptions(server_dir=server_dir)))
    assert EVERY_PRESS in rec.sql_calls, rec.sql_calls
    assert MARKED_ONLY not in rec.sql_calls, rec.sql_calls
    assert any("The import marker is unchanged." in line for line in said), said


def test_the_press_starts_the_database_and_never_the_world(tmp_path: Path) -> None:
    """The one container this press may start, and the one it may not.

    `stage_start_db` is the primitive T7 wired for the applier's stopped-world
    path (`docker.start_database`), reused rather than re-derived; `up` is
    absent from the tuple, so nothing here can bring the world back.

    Catches `start-db` dropped from the tuple (the probe would then answer
    `unreadable` and the press would refuse instead of applying), and `up`
    added to it.
    """
    server_dir = tmp_path / "srv"
    server_dir.mkdir()
    rec = ready_to_import(IMPORTED_OLDER_PLAN)
    rec.db_started = False
    engine = engine_with_sql(rerun_plan(), rec, world_running=a_world_that_is(False))
    list(engine.update_databases(InstallOptions(server_dir=server_dir)))
    assert rec.db_started is True
    assert not any(call.startswith("start ") for call in rec.calls), rec.calls


# -- the refusals ------------------------------------------------------------


def test_a_press_while_the_world_is_up_applies_no_sql(tmp_path: Path) -> None:
    """8.7a's rule, on this route: the world holds these tables and writes them back.

    T11's reviewer (note 3) read the route as it then stood — reached only
    through `engine.run()`, where `stage_start_db` returns as soon as the
    database is up and nothing asks about the world — and recorded that it
    writes DDL into `tw_char` under a running world. This is that, refused.

    Catches the guard deleted, and the whole press starting before it is asked.
    """
    server_dir = tmp_path / "srv"
    server_dir.mkdir()
    rec = ready_to_import(IMPORTED_OLDER_PLAN)
    engine = engine_with_sql(rerun_plan(), rec, world_running=a_world_that_is(True))
    with pytest.raises(InstallerError) as raised:
        list(engine.update_databases(InstallOptions(server_dir=server_dir)))
    assert "world server is running" in str(raised.value)
    assert "Press Stop" in str(raised.value)
    assert rec.sql_calls == [], rec.sql_calls
    assert rec.calls == [], "not one seam was touched: the refusal is the first thing that runs"


def test_a_press_that_cannot_tell_whether_the_world_is_up_applies_no_sql(tmp_path: Path) -> None:
    """`None` is not "no". The three-valued seam, refused on its middle answer.

    `docker.container_state()` answers an empty state for a missing container
    and for a daemon that will not reply, and `.settled` turns both into
    `False` — which through this guard would be fail-OPEN. `docker.world_running`
    answers `None` there instead, and this is the branch that costs.

    **The remedy has to name Docker**, and that is not decoration: the most
    likely reason nobody can answer is that the daemon is not answering, and
    "Stop the server, then press again" is then an instruction the user cannot
    follow — the shape T7's ticket is titled after. Asked of the sentence,
    because a refusal a user cannot act on is the defect, not the branch.

    Catches the seam read through `container_state(...).settled`, the
    `None` branch folded into the `False` one, and the dead-daemon clause
    dropped from the remedy.
    """
    server_dir = tmp_path / "srv"
    server_dir.mkdir()
    rec = ready_to_import(IMPORTED_OLDER_PLAN)
    engine = engine_with_sql(rerun_plan(), rec, world_running=a_world_that_is(None))
    with pytest.raises(InstallerError) as raised:
        list(engine.update_databases(InstallOptions(server_dir=server_dir)))
    assert "could not tell whether" in str(raised.value)
    assert "check that Docker is running" in str(raised.value)
    assert rec.sql_calls == [], rec.sql_calls


def test_a_world_started_while_the_database_came_up_is_refused_before_the_first_statement(
    tmp_path: Path,
) -> None:
    """The window between the two readings, closed the way T7 closed its own.

    `start_database()` waits for the container to report healthy — up to
    `docker._DB_HEALTHY_TIMEOUT_SECONDS` — and the Server tab's Start is a
    button the same user can press meanwhile. The first reading is that old by
    the time the first statement would be sent, so the world is read again
    after the database is up and before the import stage runs, in the same
    words: it is the same fact and the same remedy.

    Catches the second reading deleted (the seam is then asked once and the
    statements go into a live world's tables).
    """
    server_dir = tmp_path / "srv"
    server_dir.mkdir()
    rec = ready_to_import(IMPORTED_OLDER_PLAN)
    rec.db_started = False
    seam, asked = worlds(False, True)
    engine = engine_with_sql(rerun_plan(), rec, world_running=seam)
    with pytest.raises(InstallerError) as raised:
        list(engine.update_databases(InstallOptions(server_dir=server_dir)))
    assert "world server is running" in str(raised.value)
    assert rec.db_started is True, "the database really was started between the two readings"
    assert rec.sql_calls == [], rec.sql_calls
    assert asked == [CM_ENTRY.container_spec().world] * 2, asked


def test_a_seam_that_raises_is_read_as_could_not_tell(tmp_path: Path) -> None:
    """Fails closed on anything short of a clear "no", the seam's own failure included.

    The same discipline `apply.Applier`'s guard takes: *could not ask* is not
    *not running*, and a seam that raised would otherwise take the press's
    refusal out with it as a traceback in a dialog.

    Catches the `try`/`except` around the reading removed.
    """
    server_dir = tmp_path / "srv"
    server_dir.mkdir()

    def angry(container: str) -> bool | None:
        raise RuntimeError("the daemon is not there")

    rec = ready_to_import(IMPORTED_OLDER_PLAN)
    engine = engine_with_sql(rerun_plan(), rec, world_running=angry)
    with pytest.raises(InstallerError) as raised:
        list(engine.update_databases(InstallOptions(server_dir=server_dir)))
    assert "could not tell whether" in str(raised.value)
    assert "the daemon is not there" in str(raised.value)
    assert rec.sql_calls == [], rec.sql_calls


def test_a_clone_that_never_had_the_directory_refuses_naming_the_pattern(tmp_path: Path) -> None:
    """T11's reviewer, note 4: `expand()` refuses a `fail` phase whose glob matched nothing.

    A clone made before the fork's `sql/character_updates/` existed — or one
    whose fork moved it — has no file for this phase, and `on_error: fail` is
    set on it precisely so that a silent skip is impossible. The sentence is
    `sqlplan._matches()`'s own and is not wrapped in a second one: it already
    names the pattern, the folder it looked in, and that the sources may not
    have cloned completely.

    Catches the confirmation swallowing the refusal and offering an empty file
    list instead.
    """
    server_dir = tmp_path / "srv"
    server_dir.mkdir()
    engine = engine_with_sql(file_plan(), Recorder())
    with pytest.raises(InstallerError) as raised:
        engine.update_confirmation(InstallOptions(server_dir=server_dir))
    assert "src/updates/*.sql" in str(raised.value)
    assert "may not have cloned completely" in str(raised.value)


# -- the import state this press requires ------------------------------------


NOT_FINISHED = {
    "absent": docker.ImportState("absent", "no schema has tables"),
    "partial": docker.ImportState("partial", "tw_char has tables, tw_world has none"),
    "unreadable": docker.ImportState("unreadable", "the databases would not answer"),
    "populated but incomplete": docker.ImportState(
        "populated", "tw_logon holds rows, tw_world is empty", complete=False
    ),
}
"""Every probe answer that is NOT a finished import, by the name a reader uses.

Enumerated rather than sampled, because the danger is per ARM and the arms differ:
`absent` runs the whole plan and writes a completion marker, `partial` drops every
schema the plan names first (`gate.reset()`), and the two refusals are refusals
about an INSTALL and say install-shaped things. A press that consented to a named
list of files must reach none of them.
"""


@pytest.mark.parametrize("named", sorted(NOT_FINISHED))
def test_a_press_against_databases_that_are_not_a_finished_import_applies_nothing(
    tmp_path: Path, named: str
) -> None:
    """The precondition the round-1 tuple did not have, over all four answers it refuses.

    `import` is the family's own stage, and the flagged-phase route is ONE arm of
    a five-branch table. On `absent` that table imports the whole plan — hours,
    `CREATE USER … IDENTIFIED BY` with a password this tuple has no `db-password`
    stage to persist, and a completion marker — and on `partial` it first drops
    every schema the plan names. Both are reachable from a controller tab, which
    opens for any remembered folder and for "Use existing…", including a Tortoise
    install that failed at or after `import` (cold review of T14, round 1).

    Asserted on the three things that must not have happened: no SQL, no marker,
    and no drop.

    Catches `ctx.updates_only` never read (the press then falls into
    `stage_import()`'s table: `absent` and `partial` go green with SQL in the
    recorder, and the two refusals arrive in install-shaped words), and the
    precondition widened to `state != "unreadable"` or to any single arm.
    """
    server_dir = tmp_path / "srv"
    server_dir.mkdir()
    rec = ready_to_import(NOT_FINISHED[named])
    engine = engine_with_sql(rerun_plan(), rec, world_running=a_world_that_is(False))
    with pytest.raises(InstallerError) as raised:
        list(engine.update_databases(InstallOptions(server_dir=server_dir)))
    assert "do not read as a finished import" in str(raised.value)
    assert named.split()[0] in str(raised.value), "the refusal does not say what it read"
    assert "nothing was cleared" in str(raised.value)
    assert rec.sql_calls == [], rec.sql_calls
    assert not [s for s in rec.sql_scripts if sqlplan.MARKER_TABLE in s], rec.sql_scripts
    assert "reset" not in rec.calls, rec.calls


@pytest.mark.parametrize("finished", [IMPORTED_OLDER_PLAN, POPULATED_AND_COMPLETE])
def test_a_press_against_databases_that_read_as_finished_applies_the_flagged_phase(
    tmp_path: Path, finished: docker.ImportState
) -> None:
    """Both answers the route accepts, so the precondition is not merely "the marker".

    An install made by the shell scripts carries no marker row and reads
    `populated` with every schema complete — which is the install T11's route
    exists for, and a precondition narrowed to `imported` would lock it out of
    the button while leaving it exposed to the restart loop.

    Catches the predicate narrowed to `state == "imported"`.
    """
    server_dir = tmp_path / "srv"
    server_dir.mkdir()
    rec = ready_to_import(finished)
    engine = engine_with_sql(rerun_plan(), rec, world_running=a_world_that_is(False))
    said = list(engine.update_databases(InstallOptions(server_dir=server_dir)))
    assert EVERY_PRESS in rec.sql_calls, rec.sql_calls
    assert any(f"read as {finished.state}" in line for line in said), said


def test_the_press_asks_the_databases_exactly_once(tmp_path: Path) -> None:
    """One probe, and the reason it may not be two.

    A precondition checked after `stage_import()` had already branched would be a
    second question, and between two probes the answer can differ — so the arm
    that drops every schema the plan names would still be reachable on the first
    answer while the check passed on the second. `_Remembering` exists for this
    exact hazard on the install route; here the route probes once itself and
    hands the same answer to both the precondition and the re-run.

    Catches `_only_the_rerunnable_phases()` calling `stage_import()` (which
    probes again), and a precondition added in `_guard_then` on top of this one.
    """
    server_dir = tmp_path / "srv"
    server_dir.mkdir()
    rec = ready_to_import(IMPORTED_OLDER_PLAN)
    engine = engine_with_sql(rerun_plan(), rec, world_running=a_world_that_is(False))
    list(engine.update_databases(InstallOptions(server_dir=server_dir)))
    assert rec.calls.count("probe") == 1, rec.calls


def test_a_game_whose_plan_has_no_rerunnable_phase_refuses_the_press_itself(
    tmp_path: Path,
) -> None:
    """The half no probe can see, refused by the spine before the tuple is built.

    The button is not offered for such a game, but `update_databases()` is a
    method and the greying is in the view — and a family that never learned to
    read `ctx.updates_only` would take this press straight into
    `stage_import()`'s table. AzerothCore is exactly that family: it imports
    through a compose one-shot and carries no phase list at all. So the refusal
    is here, where the plan is readable, rather than trusted to a family.

    Catches the refusal deleted, and `update_phases()` consulted only by the view.
    """
    engine = installer_for(WOTLK)
    with pytest.raises(InstallerError) as raised:
        list(engine.update_databases(InstallOptions(server_dir=tmp_path / "wotlk")))
    assert "no phase meant to be re-applied" in str(raised.value)
    assert "Nothing was started." in str(raised.value)


# -- the confirmation --------------------------------------------------------


def test_the_confirmation_names_every_file_the_press_will_stream(tmp_path: Path) -> None:
    """The list is expanded from the folder, not read off the catalog's glob.

    A dialog that said `src/updates/*.sql` while the press streamed two files
    would be a promise about a pattern. `expand()` is what produces both lists,
    so the dialog and the run cannot disagree about which files exist or about
    the order they go in.

    Catches the file list built by globbing the catalog patterns instead, and
    the unflagged phase's own files leaking into the list.
    """
    server_dir = tmp_path / "srv"
    server_dir.mkdir()
    files_on_disk(server_dir, "src/updates/0002_second.sql", "src/updates/0001_first.sql")
    engine = engine_with_sql(file_plan(), Recorder())
    said = engine.update_confirmation(InstallOptions(server_dir=server_dir))
    assert "src/updates/0001_first.sql" in said
    assert "src/updates/0002_second.sql" in said
    assert said.index("0001_first") < said.index("0002_second"), said
    assert "character updates" in said
    assert "world base" not in said, "the phases the marker rule still covers are not offered"


def test_the_confirmation_says_the_marker_is_unchanged_and_the_world_must_be_stopped(
    tmp_path: Path,
) -> None:
    """The two facts a user cannot see and would be right to fear, said before they agree.

    The marker clause is what separates this from a re-import: the databases
    keep the completion marker they have, no new one is written, and no
    `verify` rule is re-asked — because a marker row says the whole PLAN
    finished and this press is two phases of it.

    The stopped-world clause is stated here as well as in the refusal because a
    user meeting the refusal has already paid for the dialog.

    Catches either clause dropped from the confirmation.
    """
    server_dir = tmp_path / "srv"
    server_dir.mkdir()
    files_on_disk(server_dir, "src/updates/0001_first.sql")
    said = engine_with_sql(file_plan(), Recorder()).update_confirmation(
        InstallOptions(server_dir=server_dir)
    )
    assert "marker" in said
    assert "Stop" in said
    assert str(server_dir) in said


def test_the_confirmation_says_a_file_can_be_refused_and_the_press_stops_there(
    tmp_path: Path,
) -> None:
    """T11's reviewer, note 2: `MODIFY money INT(10) UNSIGNED` can fail on real data.

    A `guild_bank_money.money` that is negative cannot narrow to unsigned under
    MySQL's strict mode, and `on_error: fail` means the press stops on that file
    with the client's own last line. Stated, not fixed: this ticket puts a
    button on the route T11 built and does not change what the route applies.

    Catches the clause dropped, and an `on_error` claim that says a failing file
    is skipped.
    """
    server_dir = tmp_path / "srv"
    server_dir.mkdir()
    files_on_disk(server_dir, "src/updates/0001_first.sql")
    said = engine_with_sql(file_plan(), Recorder()).update_confirmation(
        InstallOptions(server_dir=server_dir)
    )
    assert "stops" in said
    assert "guild bank" in said.lower()


# -- who gets the button -----------------------------------------------------


def test_the_entries_that_offer_this_press_are_exactly_the_ones_with_a_flagged_phase() -> None:
    """The enabling rule, read off the catalog and bound to `SqlPhase.rerun_on_marked`.

    Two derivations of one fact — the button's own (`update_phases()`) and the
    route's (`rerunnable_phases()`, which `_rerun_on_marked` filters with) —
    over every shipped entry, so a button offered for a game whose plan has no
    such phase, or withheld from one that does, is red here rather than
    discovered on a server.

    Catches the reader hard-coding `wow-tortoise`, and a phase's flag moved to
    another phase or another entry.
    """
    offered = {
        entry.id: tuple(phase.name for phase in native.update_phases(entry))
        for entry in load_catalog().games
        if native.update_phases(entry)
    }
    assert offered == {"wow-tortoise": ("character updates",)}, offered
    assert native.update_phases(WOTLK) == (), "AzerothCore imports through a compose one-shot"


def test_a_flag_added_to_another_entry_is_offered_without_the_reader_being_touched() -> None:
    """The rule is data, and this is the proof it is not a name in an `if`.

    `test_the_entries_that_offer_this_press_are_exactly_the_ones_with_a_flagged_phase`
    pins today's catalog; this pins that the pin is about the catalog. The TBC
    entry ships no flagged phase at all, and one added to its plan reaches the
    button with no code change.

    Catches the reader written as `entry.id == "wow-tortoise"`, which passes
    the test above and fails this one.
    """
    assert native.update_phases(CM_ENTRY) == ()
    widened = entry_with_sql(rerun_plan())
    assert [phase.name for phase in native.update_phases(widened)] == ["character updates"]


def test_the_route_and_the_button_filter_phases_with_one_function() -> None:
    """One filter, two callers: the press's and the button's answers cannot drift.

    `_rerun_on_marked()` decides what runs and `update_phases()` decides whether
    the control is offered at all. Written twice they could disagree in the
    direction that matters — a button offered for a phase the route then skips,
    which reports success and applies nothing.

    Catches `rerunnable_phases()` inlined again in either caller.
    """
    plan = rerun_plan()
    assert [phase.name for phase in native.rerunnable_phases(plan)] == ["character updates"]
    assert native.rerunnable_phases(plan) == native.update_phases(entry_with_sql(plan))


def test_a_tuple_with_no_import_stage_to_guard_refuses_before_anything_runs(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """The other half of `stage_named()`'s refusal: a tuple whose stage is under another name.

    `stage_named()` covers a family with no `import` at all. This covers the
    shape that is worse because it is quiet — a tuple this method stopped
    putting `import` into, or one carrying it under another name — because what
    that produces is not a missing stage but a second reading of the world that
    silently never happens. T8 built the same guard for the rebuild's rollback
    wrappers after finding they had bound positionally.

    Catches the `if not guarded:` block deleted.
    """
    server_dir = tmp_path / "srv"
    server_dir.mkdir()
    rec = ready_to_import(IMPORTED_OLDER_PLAN)
    engine = engine_with_sql(rerun_plan(), rec, world_running=a_world_that_is(False))
    monkeypatch.setattr(type(engine), "update_stages", lambda self: (self.stage_named("start-db"),))
    with pytest.raises(InstallerError) as raised:
        list(engine.update_databases(InstallOptions(server_dir=server_dir)))
    assert "no `import` stage to guard" in str(raised.value)
    assert "Nothing was started." in str(raised.value)
    assert rec.calls == [], rec.calls


def test_a_press_can_be_stopped_before_it_reaches_the_first_statement(tmp_path: Path) -> None:
    """The cancel every other staged run honours, honoured here too.

    **A guard, and honestly labelled as one**: deleting the explicit
    `_check_cancel()` from `update_databases()` leaves this green, because
    `_staged()` asks the same question before each stage. What the explicit one
    buys is not visible from here — it refuses before `_update_context()` reads
    the state file and resolves the install's database password — and `rebuild()`
    carries it in the same position for the same reason. Measured 2026-09-09;
    the mutation is equivalent, not uncaught.

    What this does catch is the cancel never reaching the stages at all: a press
    built with `cancel=None` on the context.
    """
    server_dir = tmp_path / "srv"
    server_dir.mkdir()
    rec = ready_to_import(IMPORTED_OLDER_PLAN)
    stop = threading.Event()
    stop.set()
    engine = engine_with_sql(rerun_plan(), rec, world_running=a_world_that_is(False))
    with pytest.raises(InstallerError):
        list(engine.update_databases(InstallOptions(server_dir=server_dir), cancel=stop))
    assert rec.sql_calls == [], rec.sql_calls
