"""T205: Restore works with the server stopped, by starting the database alone for it.

Reported from a Steam Deck (WoW WotLK, 2026-10-03): the Maintenance tab's
restore refused while the world and login servers ran ("Stop the server and try
again"), and refused again once Stop had taken everything down, because Stop
takes the database with it ("…is not running, so there is nothing to restore
into"). No press in the app reached the state the restore accepted; the player
got there with `docker compose stop` in a terminal. Backup had the same dead end
and T76 gave it `DatabaseAlone`; these tests hold Restore to the same handling.

The doubles are built to behave like the real thing, because a fake that
succeeded either way could not tell a press that started the database from one
that did not:

* `_Stack` is the install's containers. Its census is what the REAL
  `plan_restore()` / `restore()` read, and `bring_up()` changes it, so a
  re-plan after the start sees the database up exactly as `docker ps` would.
* `_Mysql` refuses every call while the database container is down -- what
  `docker exec` into a stopped container does -- so a restore that skipped the
  start fails rather than "succeeding" into nothing.
* The planner and the restore are `controller_wow_wotlk.maintenance`'s own,
  bound the way `_for_wotlk()` binds them; only the census and the client are
  doubles.
"""

from __future__ import annotations

import subprocess
from collections.abc import Iterator
from dataclasses import replace
from pathlib import Path
from typing import IO

import pytest

from tests.test_controller_view import _Ps, _services
from tests.test_maintenance import good_dump
from yulon import docker, runner
from yulon.catalog.catalog import load_catalog
from yulon.controller_wow_wotlk import maintenance
from yulon.controller_wow_wotlk.maintenance import MaintenanceError, RestorePlan
from yulon.ui import controller_view as controller_view_module
from yulon.ui.controller_view import ControllerServices, ControllerView, DatabaseAlone
from yulon.ui.widgets.job import run_inline

CATALOG = load_catalog()
WOTLK = CATALOG.get("wow-wotlk")
TORTOISE = CATALOG.get("wow-tortoise")
SPEC = WOTLK.container_spec()
EVERYTHING = {SPEC.db, SPEC.auth, SPEC.world}

PLANNED_START = (
    "The database will be started on its own for this restore and stopped again afterwards; "
    "the game servers stay stopped."
)


class _Stack:
    """This install's containers: what is running, and the two T76/T205 halves that change it."""

    def __init__(self, *running: str, refuses: str = "") -> None:
        self.running = set(running)
        self.refuses = refuses
        self.starts = 0
        self.stops = 0
        self.because: list[str] = []

    def census(self) -> list[str]:
        return sorted(self.running)

    def bring_up(self, because: str) -> bool:
        """`docker.start_database()`'s contract: True only where it had to start it."""
        self.because.append(because)
        if SPEC.db in self.running:
            return False
        self.starts += 1
        if self.refuses:
            raise docker.DockerCommandError(f"{self.refuses}, so {because}")
        self.running.add(SPEC.db)
        return True

    def take_down(self) -> None:
        self.stops += 1
        self.running.discard(SPEC.db)

    def alone(self) -> DatabaseAlone:
        return DatabaseAlone(bring_up=self.bring_up, take_down=self.take_down)


class _Mysql:
    """The database container's client: refuses while it is down, as `docker exec` does."""

    def __init__(self, stack: _Stack, *, fails_load: str = "") -> None:
        self.stack = stack
        self.fails_load = fails_load
        self.loaded: list[bytes] = []
        self.up_while_loading: list[bool] = []

    def _up(self) -> None:
        if SPEC.db not in self.stack.running:
            raise MaintenanceError(f"Error response from daemon: {SPEC.db} is not running")

    def databases(self) -> tuple[str, ...]:
        self._up()
        return ("acore_auth", "acore_characters", "acore_world")

    def dump_into(self, database: str, sink: IO[bytes]) -> None:
        self._up()
        sink.write(good_dump(database))

    def load_from(self, source: IO[bytes]) -> None:
        self._up()
        self.up_while_loading.append(SPEC.db in self.stack.running)
        if self.fails_load:
            raise MaintenanceError(self.fails_load)
        self.loaded.append(source.read())


@pytest.fixture(autouse=True)
def _no_docker(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(runner, "run", _Ps())


def _real_maintenance(
    tmp_path: Path, stack: _Stack, mysql: _Mysql, *, seam: bool = True
) -> ControllerServices:
    """`_services()`, with the real planner and restore bound as `_for_wotlk()` binds them."""

    def plan_restore(path: Path, *, can_start_database: bool = False) -> RestorePlan:
        return maintenance.plan_restore(
            path, tmp_path, running=stack.census, can_start_database=can_start_database
        )

    return replace(
        _services(_Ps(), tmp_path, []),
        plan_restore=plan_restore,
        restore=lambda plan: maintenance.restore(
            plan, mysql, confirm=plan.token, running=stack.census
        ),
        interrupted_restore=lambda: maintenance.interrupted_restore(tmp_path),
        database_alone=stack.alone() if seam else None,
    )


def _view(services: ControllerServices, entry: object = WOTLK) -> ControllerView:
    return ControllerView(
        entry, services, status_poll_ms=0, job_runner=run_inline  # type: ignore[arg-type]
    )


def _select_backup(view: ControllerView, tmp_path: Path) -> Path:
    """A real mysqldump in the backups dir, listed and selected as a player would."""
    path = maintenance.backups_dir(tmp_path) / "20261003_120000_acore_characters.sql"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(good_dump("acore_characters"))
    view.refresh_backups()
    for row in range(view.backup_list.count()):
        if path.name in view.backup_list.item(row).text():
            view.backup_list.setCurrentRow(row)
    return path


def _failures(view: ControllerView) -> list[str]:
    seen: list[str] = []
    view.action_failed.connect(seen.append)
    return seen


# -- the reported case ---------------------------------------------------------------------


def test_a_stopped_server_restores_by_starting_the_database_alone_and_stopping_it_again(
    qapp: object, tmp_path: Path
) -> None:
    """The Steam Deck report, through the Maintenance tab: Stop, then Restore, works.

    Asserted in the order the fix claims. Planning starts nothing; the plan says
    the database will be started; the load ran with the database up and the
    game servers down; and the press left the server stopped as it found it.
    """
    stack = _Stack()
    mysql = _Mysql(stack)
    view = _view(_real_maintenance(tmp_path, stack, mysql))
    path = _select_backup(view, tmp_path)

    view.show_restore_plan()
    assert stack.starts == 0, "showing the plan started the database"
    assert view.restore_button.isEnabled(), view.maintenance_report.toPlainText()
    assert PLANNED_START in view.maintenance_report.toPlainText()

    view.run_restore()

    assert mysql.loaded == [path.read_bytes()], view.maintenance_report.toPlainText()
    assert mysql.up_while_loading == [True]
    assert stack.starts == 1 and stack.stops == 1
    assert stack.running == set(), "the press did not leave the server as it found it"
    assert stack.because == ["the restore was not started"]
    assert "Restored acore_characters" in view.maintenance_report.toPlainText()


def test_a_database_already_up_is_neither_started_nor_stopped_by_a_restore(
    qapp: object, tmp_path: Path
) -> None:
    """Only a database this press started may be stopped by it."""
    stack = _Stack(SPEC.db)
    mysql = _Mysql(stack)
    view = _view(_real_maintenance(tmp_path, stack, mysql))
    _select_backup(view, tmp_path)

    view.show_restore_plan()
    assert view.restore_button.isEnabled()
    assert PLANNED_START not in view.maintenance_report.toPlainText()
    view.run_restore()

    assert len(mysql.loaded) == 1
    assert stack.starts == 0 and stack.stops == 0
    assert stack.running == {SPEC.db}, "a database the user had up was taken down"


def test_a_running_world_still_refuses_and_starts_nothing(qapp: object, tmp_path: Path) -> None:
    """The game servers' rule is unchanged, and "Stop the server and try again" now works.

    Only the world-and-login rule is broken here: the database is up, so the
    database rule could not refuse whatever T205 did to it.
    """
    stack = _Stack(*EVERYTHING)
    mysql = _Mysql(stack)
    view = _view(_real_maintenance(tmp_path, stack, mysql))
    _select_backup(view, tmp_path)

    view.show_restore_plan()
    text = view.maintenance_report.toPlainText()
    assert not view.restore_button.isEnabled()
    assert "Stop the server and try again." in text
    assert "nothing to restore into" not in text
    view.run_restore()
    assert mysql.loaded == [] and stack.starts == 0 and stack.stops == 0

    # And the press the refusal names now leads to a restore that goes ahead.
    stack.running.clear()
    view.show_restore_plan()
    assert view.restore_button.isEnabled(), view.maintenance_report.toPlainText()
    view.run_restore()
    assert len(mysql.loaded) == 1 and stack.running == set()


def test_a_server_started_after_the_plan_refuses_the_restore_and_is_left_running(
    qapp: object, tmp_path: Path
) -> None:
    """The plan is a census, not a permission slip: the press re-plans with the database up.

    Planned with everything stopped; the player then pressed Start. The press
    finds the database already up (so starts and stops nothing) and the re-plan
    refuses for the running world, exactly as it did before T205.
    """
    stack = _Stack()
    mysql = _Mysql(stack)
    view = _view(_real_maintenance(tmp_path, stack, mysql))
    _select_backup(view, tmp_path)
    failures = _failures(view)

    view.show_restore_plan()
    assert view.restore_button.isEnabled()
    stack.running |= EVERYTHING
    view.run_restore()

    assert mysql.loaded == []
    assert failures and "Stop the server and try again." in failures[-1], failures
    assert stack.starts == 0 and stack.stops == 0
    assert stack.running == EVERYTHING, "the player's running server was touched"


# -- every way out of the press puts the database back ---------------------------------------


def test_a_database_that_cannot_start_says_nothing_was_restored(
    qapp: object, tmp_path: Path
) -> None:
    """Bring-up failed: say so plainly, restore nothing, and leave nothing running.

    `take_down()` is not called: a `bring_up()` that raises answered nothing,
    and the real one puts back anything it started itself (see the factory
    test below). The double started nothing, so the stack is as it was.
    """
    stack = _Stack(refuses=f"{SPEC.db} did not report healthy within 180s")
    mysql = _Mysql(stack)
    view = _view(_real_maintenance(tmp_path, stack, mysql))
    _select_backup(view, tmp_path)
    failures = _failures(view)

    view.show_restore_plan()
    view.run_restore()

    assert mysql.loaded == []
    assert failures, "the failed start was not reported"
    assert failures[-1].startswith("Nothing was restored"), failures[-1]
    assert "did not report healthy" in failures[-1], "docker's own reason was dropped"
    assert stack.running == set() and stack.stops == 0
    assert maintenance.interrupted_restore(tmp_path) is None, "a marker for a restore never begun"


def test_a_failed_restore_still_stops_the_database_it_started(qapp: object, tmp_path: Path) -> None:
    """A load that fails part-way still puts the database back down (`finally`)."""
    stack = _Stack()
    mysql = _Mysql(stack, fails_load="ERROR 2013 (HY000): Lost connection")
    view = _view(_real_maintenance(tmp_path, stack, mysql))
    _select_backup(view, tmp_path)
    failures = _failures(view)

    view.show_restore_plan()
    view.run_restore()

    assert failures and "failed part-way" in failures[-1], failures
    assert mysql.up_while_loading == [True]
    assert stack.starts == 1 and stack.stops == 1
    assert stack.running == set()


def test_a_restore_refused_at_the_press_still_stops_the_database_it_started(
    qapp: object, tmp_path: Path
) -> None:
    """The file changed between the plan and the press: refused, and the database put back.

    The plan was made with the database down and the press re-planned with it
    up; the token still caught the replaced file, so the protection is not
    weakened by the database being started in between.
    """
    stack = _Stack()
    mysql = _Mysql(stack)
    view = _view(_real_maintenance(tmp_path, stack, mysql))
    path = _select_backup(view, tmp_path)
    failures = _failures(view)

    view.show_restore_plan()
    path.write_bytes(good_dump("acore_characters", "acore_world"))
    view.run_restore()

    assert failures and "changed in between" in failures[-1], failures
    assert mysql.loaded == []
    assert stack.starts == 1 and stack.stops == 1
    assert stack.running == set()


def test_a_tab_without_the_database_seam_refuses_a_stopped_server_as_before(
    qapp: object, tmp_path: Path
) -> None:
    """`database_alone=None` keeps today's behaviour: nothing can start it, so it refuses."""
    stack = _Stack()
    mysql = _Mysql(stack)
    view = _view(_real_maintenance(tmp_path, stack, mysql, seam=False))
    _select_backup(view, tmp_path)

    view.show_restore_plan()
    assert not view.restore_button.isEnabled()
    assert "nothing to restore into" in view.maintenance_report.toPlainText()


# -- T144's path, on the one game that has the bot request -----------------------------------


class _BotRequest:
    """T144's seam with no pending request: records that the press reached it."""

    def __init__(self) -> None:
        self.calls: list[str] = []

    def before_restore(self) -> None:
        self.calls.append("before-restore")

    def restore_warning(self) -> None:
        return None

    def take_module_moved(self) -> None:
        return None


class _NoBots:
    def page(self, *, after: object = None, name_like: str = "") -> object:
        return None


def _tortoise(tmp_path: Path, stack: _Stack, mysql: _Mysql) -> tuple[ControllerView, _BotRequest]:
    request = _BotRequest()
    services = replace(
        _real_maintenance(tmp_path, stack, mysql), bots=_NoBots(), bot_pool_rebuild=request
    )
    return _view(services, TORTOISE), request


def test_the_bot_request_path_also_starts_and_stops_the_database(
    qapp: object, tmp_path: Path
) -> None:
    """`_restore_with_the_bot_request()` re-plans before it touches the conf; it needs the db up."""
    stack = _Stack()
    mysql = _Mysql(stack)
    view, request = _tortoise(tmp_path, stack, mysql)
    _select_backup(view, tmp_path)

    view.show_restore_plan()
    assert view.restore_button.isEnabled(), view.maintenance_report.toPlainText()
    view.run_restore()

    assert request.calls == ["before-restore"]
    assert len(mysql.loaded) == 1, view.maintenance_report.toPlainText()
    assert stack.starts == 1 and stack.stops == 1 and stack.running == set()


def test_the_bot_request_path_refuses_a_server_started_after_the_plan(
    qapp: object, tmp_path: Path
) -> None:
    """Its own re-plan refuses before the conf is touched, and the server is left running."""
    stack = _Stack()
    mysql = _Mysql(stack)
    view, request = _tortoise(tmp_path, stack, mysql)
    _select_backup(view, tmp_path)
    failures = _failures(view)

    view.show_restore_plan()
    stack.running |= EVERYTHING
    view.run_restore()

    assert failures and "Stop the server and try again." in failures[-1], failures
    assert request.calls == [], "the conf was touched for a restore that refused"
    assert mysql.loaded == [] and stack.stops == 0 and stack.running == EVERYTHING


# -- the wiring every game really gets -------------------------------------------------------


def test_every_game_plans_a_stopped_server_as_one_its_restore_starts_the_database_for(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Through the callable each game factory builds: the keyword reaches the shared planner.

    Nothing is running in this fake. Without the keyword every game still says
    its own database is not running (`test_each_game_s_restore_plan_censuses_
    its_own_containers`); with it, none does, and every plan records the start.
    """
    monkeypatch.setattr(
        docker.runner,
        "run",
        lambda cmd, cwd=None, timeout=None: subprocess.CompletedProcess(cmd, 0, "", ""),
    )
    for game in controller_view_module._FACTORIES:
        entry = CATALOG.get(game)
        services = ControllerServices.for_entry(entry, tmp_path / game)
        assert services.database_alone is not None, game
        plan = services.plan_restore(tmp_path / "missing.sql", can_start_database=True)
        refusals = " ".join(plan.refusals)
        assert "is not running" not in refusals, (game, refusals)
        assert plan.starts_database is True, game


class _Docker:
    """`docker.status` / `start_database` / `stop_containers`, for the factory's own halves."""

    def __init__(self, running: list[str] | None, *, start_fails: bool = False) -> None:
        self.running = running
        self.start_fails = start_fails
        self.started: list[str] = []
        self.stopped: list[list[str]] = []

    def status(self, wsl_distro: str | None = None) -> list[str]:
        if self.running is None:
            raise docker.DockerCommandError("Cannot connect to the Docker daemon")
        return list(self.running)

    def start_database(
        self,
        spec: docker.ContainerSpec,
        server_dir: Path,
        *,
        because: str = "",
        wsl_distro: str | None = None,
    ) -> bool:
        self.started.append(because)
        assert self.running is not None
        self.running.append(spec.db)  # `compose up -d` ran; the health wait is what failed
        if self.start_fails:
            raise docker.DockerCommandError(
                f"{spec.db} did not report healthy within 180s, so {because}."
            )
        return True

    def stop_containers(self, names: list[str], *, wsl_distro: str | None = None) -> None:
        self.stopped.append(list(names))
        if self.running is not None:
            for name in names:
                if name in self.running:
                    self.running.remove(name)


def _factory(monkeypatch: pytest.MonkeyPatch, fake: _Docker) -> Iterator[DatabaseAlone]:
    for name in ("status", "start_database", "stop_containers"):
        monkeypatch.setattr(controller_view_module.docker, name, getattr(fake, name))
    for game in controller_view_module._FACTORIES:
        spec = CATALOG.get(game).container_spec()
        yield controller_view_module._database_alone(spec, Path("srv"), wsl_distro=None)


def test_a_start_that_fails_after_compose_started_the_container_stops_it_again(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """`start_database()` raises on a health timeout with the container UP; it must not stay up.

    The database was down before the press, so whatever is running under its
    name afterwards was started by it -- and a restore that "leaves nothing
    running" when it could not start the database has to put that back too.
    """
    fake = _Docker([], start_fails=True)
    for alone in _factory(monkeypatch, fake):
        fake.running, fake.stopped = [], []
        with pytest.raises(docker.DockerCommandError, match="so nothing was run"):
            alone.bring_up("nothing was run")
        assert fake.running == [], "a database whose start failed was left running"
        assert len(fake.stopped) == 1


def test_a_start_that_never_reached_docker_stops_nothing(monkeypatch: pytest.MonkeyPatch) -> None:
    """Docker would not say what runs: nothing was started, so nothing may be stopped."""
    fake = _Docker(None)
    for alone in _factory(monkeypatch, fake):
        with pytest.raises(docker.DockerCommandError):
            alone.bring_up("nothing was run")
    assert fake.started == [] and fake.stopped == []


def test_a_database_already_up_is_not_started_by_the_factory(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Up already: `bring_up()` answers False and touches nothing, so nothing is stopped later."""
    for game in controller_view_module._FACTORIES:
        db = CATALOG.get(game).container_spec().db
        fake = _Docker([db])
        monkeypatch.setattr(controller_view_module.docker, "status", fake.status)
        monkeypatch.setattr(controller_view_module.docker, "start_database", fake.start_database)
        monkeypatch.setattr(controller_view_module.docker, "stop_containers", fake.stop_containers)
        alone = controller_view_module._database_alone(
            CATALOG.get(game).container_spec(), Path("srv"), wsl_distro=None
        )
        assert alone.bring_up("nothing was run") is False, game
        assert fake.started == [] and fake.stopped == [], game
