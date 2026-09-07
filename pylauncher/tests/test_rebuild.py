"""The rebuild control: the action `_format_report` has always named and never offered.

The Modules tab prints "worldserver REBUILD required before this takes effect"
after every module install whose manifest sets `build.rebuild` — 20 of the 41
shipped manifests, all of them `module`. Until this file there was no button
anywhere in `yulon/ui/` that started one, so those twenty entries could not work
at all on a server that was already built, and the sentence named an action the
app did not have.

Re-pressing Install was not it, and that is very probably what the user who
reported this meant by "i used the this thing to rebuild the server but nothing
changes when i log back in" (2026-09-07): `stage_build()` skips the compile when
the state file records a build AND the daemon holds every image, and the image
tag is derived from the FOLDER, so adding a module never changes it. Measured on
yulon-ubuntu through the app's own predicates: `state.has("build")` True,
`built_images()` True, `build_would_be_skipped()` True.

So the rebuild is not "run the install again". It is the build stage with the
skip rule switched off, the containers recreated from the image it produces, and
the readiness wait the install ends with — and the tests below are about the
force, the refusals, the cancel, and the fact that the progress a user watches is
the installer's own, not a second reporting scheme that can drift from it.

What is unit-tested only is stated in the report and in
`test_the_recreate_argv_does_not_rely_on_compose_noticing_a_new_image`: no
rebuild was run for this change.
"""

from __future__ import annotations

import threading
from pathlib import Path

import pytest
import yaml

from tests.support_native import ENTRY, Recorder, engine, install
from yulon import docker
from yulon.apply import CLONE_DIRS, Applier
from yulon.catalog import composegen, native
from yulon.catalog.installer import InstallerError, InstallOptions

PYPLAN = Path(__file__).resolve().parents[2] / "pyplan"
RENDERED = Path(__file__).resolve().parent / "data" / "wotlk-rendered"


def a_finished_install(rec: Recorder, tmp_path: Path) -> Path:
    """A server dir in the state a real install leaves it in, made by the real install.

    Not a hand-built fixture: the whole question this feature turns on is what
    a FINISHED install looks like to the build stage's skip rule, and a fixture
    that laid its own state file and its own compose files would be free to lay
    the state that makes the test pass. `install()` runs all nine stages
    through the machine double, so the state file, the three compose files and
    their markers are the engine's own output.
    """
    server_dir = tmp_path / "wow"
    install(rec, server_dir)
    rec.calls.clear()
    return server_dir


# -- the force ---------------------------------------------------------------


def test_a_rebuild_compiles_a_server_whose_images_are_all_already_there(
    tmp_path: Path,
) -> None:
    """THE test for requirement 2. If the force is lost, this is what goes red.

    `images=True` plus a recorded `build` is exactly the state
    `build_would_be_skipped()` answers True for, and it is the state every
    already-installed server is in. A rebuild that honoured the skip would print
    "The server is already built; skipping the compile." and reproduce the
    defect it exists to fix, so both halves are asserted: the compile RAN, and
    the skip sentence was not said.
    """
    rec = Recorder(images=True)
    server_dir = a_finished_install(rec, tmp_path)
    said = list(engine(rec).rebuild(InstallOptions(server_dir=server_dir)))
    assert "build" in rec.calls, f"the rebuild skipped the compile: {rec.calls}"
    assert not any("skipping the compile" in line for line in said), said


@pytest.mark.parametrize("images", [True, False, None], ids=("present", "gone", "unknown"))
@pytest.mark.parametrize("recorded", [True, False], ids=("recorded", "fresh"))
def test_a_forced_context_is_never_a_skip_in_any_of_the_six_states(
    tmp_path: Path, recorded: bool, images: bool | None
) -> None:
    """The predicate and the stage, driven together over every state, forced.

    `test_build_would_be_skipped_answers_exactly_what_stage_build_then_does`
    (test_spine.py) does this for the unforced context and is the reason that
    predicate can be trusted by `CmangosInstaller._patch_sources()`. The force
    adds a seventh input to the same rule, and a force honoured by the stage but
    not by the predicate would make that refusal fire on a press that is about
    to compile — so the two are driven together here as well.
    """
    rec = Recorder(images=images)
    server_dir = tmp_path / "srv"
    server_dir.mkdir()
    installer = engine(rec)
    ctx = native.StageContext(
        server_dir=server_dir,
        client_dir=None,
        state=native.InstallState(
            ENTRY.id, "abc", "azerothcore", completed=("build",) if recorded else ()
        ),
        cancel=None,
        secrets=native.Secrets(db_password="pw"),
        force_build=True,
    )
    assert installer.build_would_be_skipped(ctx) is False
    said = list(installer.stage_build(ctx))
    assert "build" in rec.calls
    assert not any("skipping the compile" in line for line in said), said


def test_the_forced_compile_says_why_it_is_compiling_something_already_built(
    tmp_path: Path,
) -> None:
    """A build that looks unnecessary must say it was asked for, not just start.

    Without this the panel shows an hour of compiler output with nothing above
    it explaining why a finished server is being compiled, which is the shape
    of a bug rather than of a deliberate action.
    """
    rec = Recorder(images=True)
    server_dir = a_finished_install(rec, tmp_path)
    said = list(engine(rec).rebuild(InstallOptions(server_dir=server_dir)))
    assert any("asked for" in line and "already built" in line for line in said), said


# -- modules cloned since the last build --------------------------------------


def test_a_module_cloned_after_the_build_is_inside_what_the_build_overlay_compiles() -> None:
    """Requirement 3, asserted against the real overlay rather than assumed.

    A module is cloned to `<server_dir>/modules/<id>` (`apply.CLONE_DIRS`), and
    AzerothCore compiles modules INTO the worldserver binary. So "the rebuild
    picks up a module cloned since the last build" is true only if the build
    context the overlay declares CONTAINS that folder. The overlay in
    `tests/data/wotlk-rendered/` is the file the engine renders, compared field
    by field against a real install's `docker compose config` by
    `test_compose_fixture.py`, so it is the right thing to read this off.

    Every service is checked, not the worldserver alone: a context of `.` on
    three targets and something else on the fourth would still leave one image
    built from a tree without the module in it.
    """
    overlay = yaml.safe_load((RENDERED / composegen.BUILD_FILE).read_text(encoding="utf-8"))
    services = overlay["services"]
    assert services, "the build overlay declares no services at all"
    for name, service in services.items():
        context = service["build"]["context"]
        assert context == ".", f"{name} builds from {context!r}, not from the server dir"
    # The other half of the sentence: what `.` resolves to is the directory the
    # clone lands in. `build_staged()` runs compose with `cwd=server_dir`.
    assert CLONE_DIRS["module"] == "modules"
    assert Applier(Path("/srv"), git=None).clone_dir(  # type: ignore[arg-type]
        _a_module_manifest()
    ) == Path("/srv/modules/mod-ah-bot")


def _a_module_manifest() -> object:
    from yulon.controller_wow_wotlk import modules as wotlk_modules

    return wotlk_modules.store().load("module", "mod-ah-bot")


def test_the_rebuild_builds_with_all_three_compose_files(tmp_path: Path) -> None:
    """The `-f` set, off the call the engine makes, not off `build_staged`'s docstring.

    A bare `docker compose build` in a generated install's directory builds
    NOTHING and exits 0 — the `build:` blocks live in the overlay compose never
    auto-loads — and naming any `-f` disables auto-loading, so all three have to
    be listed. A rebuild that lost the overlay would print a successful build
    and change nothing, which is the user's complaint word for word.
    """
    rec = Recorder(images=True)
    seen: list[object] = []

    def build(server_dir: Path, files: object, **kwargs: object) -> docker.AttachedRun:
        rec.calls.append("build")
        seen.append(tuple(files))  # type: ignore[arg-type]
        return docker.AttachedRun(0, ("built",))

    server_dir = a_finished_install(rec, tmp_path)
    list(engine(rec, build=build).rebuild(InstallOptions(server_dir=server_dir)))
    assert seen == [composegen.COMPOSE_FILES]


# -- refusals ----------------------------------------------------------------


def test_a_rebuild_refuses_a_folder_this_app_has_no_record_of_building(tmp_path: Path) -> None:
    """An install this app did not build. Requirement 4's first half."""
    rec = Recorder()
    server_dir = tmp_path / "somebody-elses"
    server_dir.mkdir()
    (server_dir / composegen.BASE_FILE).write_text("services: {}\n", encoding="utf-8")
    with pytest.raises(InstallerError) as raised:
        list(engine(rec).rebuild(InstallOptions(server_dir=server_dir)))
    message = str(raised.value)
    assert native.STATE_FILE in message
    assert str(server_dir) in message
    assert "Nothing was started" in message
    assert "build" not in rec.calls


def test_a_rebuild_refuses_an_adopted_install_whose_compose_file_is_not_ours(
    tmp_path: Path,
) -> None:
    """The DML-built install adopted through "Use existing…", said honestly.

    `composegen.write_plan()` never overwrites a compose file this engine did
    not write — it raises rather than orphan somebody's character volumes — so
    the app cannot put a build overlay into such a folder, and without an
    overlay `docker compose build` builds nothing. The refusal has to say that,
    including the half that is about what the button CANNOT do here, because a
    user whose server is one of those is exactly the user this feature is for.

    The state file is present and complete: this is not the "no record" case
    above, it is the one where the record is there and the compose files are
    somebody else's.
    """
    rec = Recorder()
    server_dir = a_finished_install(rec, tmp_path)
    (server_dir / composegen.BUILD_FILE).write_text(
        "# hand-written by the DML launcher\nservices: {}\n", encoding="utf-8"
    )
    with pytest.raises(InstallerError) as raised:
        list(engine(rec).rebuild(InstallOptions(server_dir=server_dir)))
    message = str(raised.value)
    assert composegen.BUILD_FILE in message
    assert "did not write" in message
    assert "Nothing was started" in message
    assert "build" not in rec.calls


def test_a_rebuild_refuses_when_the_build_overlay_is_missing_altogether(tmp_path: Path) -> None:
    """A deleted overlay is the same dead end as a foreign one and gets the same refusal.

    `composegen.is_ours()` answers True for a file that is not there — the
    right answer for "may I write this?" and the wrong one for "can I build
    with this?". A rebuild that read only the marker would sail past a missing
    overlay into a `docker compose build` that exits 0 having built nothing.
    """
    rec = Recorder()
    server_dir = a_finished_install(rec, tmp_path)
    (server_dir / composegen.BUILD_FILE).unlink()
    with pytest.raises(InstallerError) as raised:
        list(engine(rec).rebuild(InstallOptions(server_dir=server_dir)))
    assert composegen.BUILD_FILE in str(raised.value)
    assert "build" not in rec.calls


# -- the cancel --------------------------------------------------------------


def test_a_cancel_set_before_the_first_stage_compiles_nothing(tmp_path: Path) -> None:
    """Stop is a real stop, and the engine's own cancel is what carries it.

    The confirmation dialog is the user's first chance to say no
    (`test_declining_the_rebuild_confirmation_starts_nothing`); this is the
    second, and it is the one that has to hold once the panel is streaming.
    """
    rec = Recorder(images=True)
    server_dir = a_finished_install(rec, tmp_path)
    cancel = threading.Event()
    cancel.set()
    with pytest.raises(InstallerError):
        list(engine(rec).rebuild(InstallOptions(server_dir=server_dir), cancel=cancel))
    assert "build" not in rec.calls


# -- progress ----------------------------------------------------------------


def test_the_rebuild_reports_through_the_installers_own_staged_reporting(
    tmp_path: Path,
) -> None:
    """Requirement 5. The same `Step n of m` / `--- <name>` lines, from the same loop.

    Not a similar-looking pair of lines: `^--- <stage>` is what gate scripts,
    log captures and the interrupted-import watchers match on, and a second
    reporting scheme would be a second place for that format to drift. The
    cancel note is asserted too, because the spine — not the stage body — is
    what says it, so its presence is evidence the shared loop ran.
    """
    rec = Recorder(images=True)
    server_dir = a_finished_install(rec, tmp_path)
    said = list(engine(rec).rebuild(InstallOptions(server_dir=server_dir)))
    stages = [line for line in said if line.startswith("--- ")]
    assert stages == ["--- build", "--- recreate", "--- ready"], said
    steps = [line for line in said if line.startswith("Step ")]
    assert steps[0].startswith(f"Step 1 of {len(stages)}"), steps
    assert len(steps) == len(stages)
    assert native.BUILD_CANCEL_NOTE in said


def test_the_rebuild_ends_by_saying_what_it_did_and_what_it_did_not(tmp_path: Path) -> None:
    """The closing line is the answer to "nothing changes when I log back in".

    It has to name the server as running on the new build, and it must not
    imply the database was touched: this press compiles and recreates, and the
    module SQL an AzerothCore module carries is applied by the emulator's own
    updater for modules in `AC_MODULES_LIST`, which nothing here runs.
    """
    rec = Recorder(images=True)
    server_dir = a_finished_install(rec, tmp_path)
    said = list(engine(rec).rebuild(InstallOptions(server_dir=server_dir)))
    assert said[-1] == f"{ENTRY.name} was rebuilt and is running in {server_dir}"
    assert native.REBUILD_CLOSING_NOTE in said
    # The claim it must NOT make. "Applied", "installed" or any past-tense verb
    # about the database would be this feature's own bug repeated one layer up:
    # telling a user something took effect when nobody checked.
    assert "ran no SQL" in native.REBUILD_CLOSING_NOTE
    for promise in ("SQL was applied", "fully installed", "module is now installed"):
        assert promise not in native.REBUILD_CLOSING_NOTE
    # And nothing anywhere in the run went near a database.
    assert "sql" not in rec.calls and "query" not in rec.calls, rec.calls


def test_the_recreate_argv_does_not_rely_on_compose_noticing_a_new_image() -> None:
    """UNIT-TESTED ONLY, and the reason the flag is there rather than assumed.

    `start_staged()`'s docstring records what was measured against Docker
    29.1.3: it "recreates a service whose configuration changed". Nothing in
    this repository records what compose does when only the IMAGE behind an
    unchanged tag has changed — and the image tag is derived from the folder,
    so a rebuild never changes it. A rebuild that left the old container
    running would produce a perfect log and no change at all, which is the
    defect this whole feature is about, so the recreate is asked for
    explicitly instead of hoped for.

    No rebuild was run to write this. What is asserted is the argv.
    """
    argv = docker.recreate_argv(ENTRY.container_spec())
    assert argv[:4] == ["compose", "up", "-d", "--force-recreate"]
    assert "--no-deps" in argv
    spec = ENTRY.container_spec()
    assert spec.import_service is not None
    assert spec.import_service not in argv, "a recreate must never re-run the one-shot import"


# -- the cost the confirmation states -----------------------------------------


def test_every_build_time_the_confirmation_quotes_is_still_recorded_in_pyplan() -> None:
    """Requirement 6: the numbers are this project's own measurements, not invented.

    Each one is pinned to the page it was measured on, so a confirmation that
    grew a friendlier number would go red here rather than in front of a user
    who then plans their evening around it. If a citation ever stops matching,
    the fix is to re-read the gate and change the sentence — not to relax the
    pin.
    """
    from yulon.catalog.installer import rebuild_confirmation

    text = rebuild_confirmation(ENTRY, Path("/srv"))
    checklist = (PYPLAN / "checklist.md").read_text(encoding="utf-8")
    rounds = (PYPLAN / "hunt-rounds.md").read_text(encoding="utf-8")
    for quoted, source, page in (
        ("35-72 minutes", rounds.replace("–", "-"), "hunt-rounds.md"),
        ("68 minutes", checklist, "checklist.md"),
        ("15 minutes", checklist, "checklist.md"),
    ):
        assert quoted in text, f"the confirmation no longer quotes {quoted}"
        assert quoted in source, f"{quoted} is no longer recorded in pyplan/{page}"


def test_the_confirmation_says_what_it_costs_before_it_says_yes(tmp_path: Path) -> None:
    """Everything a person needs to answer the question, in the question.

    The three facts a "Rebuild?" box that omitted them would be lying by
    omission about: it takes as long as an install's compile, the server goes
    down and comes back, and saying no changes nothing.
    """
    from yulon.catalog.installer import rebuild_confirmation

    text = rebuild_confirmation(ENTRY, tmp_path)
    assert str(tmp_path) in text, "the question does not say which install it is about"
    lowered = text.lower()
    assert "minutes" in lowered, "the question does not say how long"
    assert "stopped" in lowered, "the question does not say the server goes down"
    assert "say no and nothing happens" in lowered, "the question does not say what no costs"
