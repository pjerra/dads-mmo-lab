"""Tests for `yulon.dashboard` — the one line above the three up/down words (8.1a).

The bug this exists to close, verbatim from `pyplan/bug-checklist.md:499`:

    HIGH — A TBC server whose mangosd is stuck in a restart loop is reported as
    a fully successful install, end to end through the GUI.

A crash-looping worldserver appears in `docker ps` between restarts, so every
check that asks "is it running" says yes. What separates the two is the restart
count and whether the current run has lasted.
"""

from __future__ import annotations

from datetime import datetime, timedelta
from pathlib import Path

from yulon import dashboard, docker
from yulon.catalog import catalog as catalog_module

WOTLK = catalog_module.load_catalog().get("wow-wotlk")
SPEC = WOTLK.container_spec()
NOW = datetime.fromisoformat("2026-09-06T18:00:00+00:00")


class _FakeSql:
    def __init__(self, answer: str = "3\t497\t500\t512\n") -> None:
        self.statements: list[str] = []
        self.answer = answer

    def query(self, db: str, statement: str) -> str:
        self.statements.append(statement)
        return self.answer


def _install(tmp_path: Path) -> Path:
    conf = tmp_path / WOTLK.observability.bots.prefix_conf_file
    conf.parent.mkdir(parents=True, exist_ok=True)
    conf.write_text("AiPlayerbot.RandomBotAccountPrefix = rndbot\n", encoding="utf-8")
    return tmp_path


def _watch(
    tmp_path: Path, states: list[docker.ContainerState], sql: _FakeSql | None = None
) -> dashboard.Dashboard:
    """A dashboard whose container states are handed to it, one per tick."""
    remaining = list(states)
    return dashboard.Dashboard(
        SPEC,
        WOTLK,
        _install(tmp_path),
        sql=sql if sql is not None else _FakeSql(),
        state_of=lambda _container: remaining.pop(0),
        now=lambda: NOW,
    )


def _running(started: str = "2026-09-06T12:00:00.123456789Z", restarts: int = 0):
    return docker.ContainerState("running", started, restarts)


def test_a_server_that_is_up_reports_its_population(tmp_path: Path) -> None:
    sql = _FakeSql("3\t497\t500\t512\n")

    verdict = _watch(tmp_path, [_running()], sql).tick()

    assert verdict.state == "up"
    assert (verdict.players, verdict.bots) == (3, 497)
    assert verdict.stable is True


def test_a_stopped_server_is_not_asked_how_many_players_it_has(tmp_path: Path) -> None:
    """Its database is down too; the query would time out once per tick to say nothing."""
    sql = _FakeSql()

    verdict = _watch(tmp_path, [docker.ContainerState("exited", "", 0)], sql).tick()

    assert verdict.state == "stopped"
    assert sql.statements == []
    assert verdict.players is None


def test_a_restart_count_that_grew_between_ticks_is_a_restart_loop(tmp_path: Path) -> None:
    """The count is the only thing that separates a loop from a server that is up."""
    watch = _watch(tmp_path, [_running(restarts=2), _running(restarts=3)])

    first = watch.tick()
    second = watch.tick()

    assert first.state == "up"
    assert second.state == "restart_loop"
    assert second.restarts == 3
    assert second.stable is False


def test_a_container_docker_calls_restarting_is_a_loop_on_the_very_first_tick(
    tmp_path: Path,
) -> None:
    """No history needed: `restarting` is the daemon saying so itself."""
    verdict = _watch(tmp_path, [docker.ContainerState("restarting", "", 7)]).tick()

    assert verdict.state == "restart_loop"
    assert verdict.stable is False


def test_a_young_run_after_a_restart_still_reads_as_a_loop(tmp_path: Path) -> None:
    """A worldserver takes minutes to load, so a loop is quiet between its crashes.

    Clearing the verdict as soon as one tick sees a steady count would call a
    server with a three-minute crash cycle healthy for most of every cycle —
    which is the shape of the bug this closes.
    """
    just_started = (NOW - timedelta(seconds=30)).strftime("%Y-%m-%dT%H:%M:%S.000000000Z")
    watch = _watch(
        tmp_path,
        [
            _running(restarts=4),
            _running(started=just_started, restarts=5),
            _running(just_started, 5),
        ],
    )

    watch.tick()
    watch.tick()
    third = watch.tick()

    assert third.state == "restart_loop"


def test_a_server_that_restarted_hours_ago_and_has_been_up_since_is_not_a_loop(
    tmp_path: Path,
) -> None:
    """Every server has restarted at some point; that alone is not instability."""
    long_ago = "2026-09-06T12:00:00.000000000Z"
    watch = _watch(tmp_path, [_running(long_ago, 4), _running(long_ago, 4)])

    watch.tick()
    second = watch.tick()

    assert second.state == "up"
    assert second.restarts == 4


def test_a_daemon_that_cannot_be_asked_says_unknown_rather_than_stopped(tmp_path: Path) -> None:
    """ "Stopped" is a claim. An empty answer is the absence of one."""
    verdict = _watch(tmp_path, [docker.ContainerState()]).tick()

    assert verdict.state == "unknown"
    assert verdict.stable is False
    assert verdict.players is None


def test_the_uptime_survives_dockers_nanosecond_timestamps(tmp_path: Path) -> None:
    """`fromisoformat` takes 3 or 6 fractional digits; docker prints 9."""
    verdict = _watch(tmp_path, [_running("2026-09-06T17:00:00.000000000Z")]).tick()

    assert verdict.uptime == timedelta(hours=1)


def test_a_timestamp_that_cannot_be_parsed_leaves_the_uptime_absent_not_wrong(
    tmp_path: Path,
) -> None:
    verdict = _watch(tmp_path, [_running("some day in the future")]).tick()

    assert verdict.uptime is None
    assert verdict.state == "up"


def test_a_marker_that_cannot_be_resolved_refuses_the_counts_without_hiding_the_server(
    tmp_path: Path,
) -> None:
    """The tab still says the server is up; it just does not invent two numbers."""
    server_dir = _install(tmp_path)
    (server_dir / WOTLK.observability.bots.prefix_conf_file).write_text(
        "AiPlayerbot.RandomBotAccountPrefix =\n", encoding="utf-8"
    )
    sql = _FakeSql()
    watch = dashboard.Dashboard(
        SPEC,
        WOTLK,
        server_dir,
        sql=sql,
        state_of=lambda _c: _running(),
        now=lambda: NOW,
    )

    verdict = watch.tick()

    assert verdict.state == "up"
    assert verdict.players is None
    assert verdict.problem != ""
    assert sql.statements == []


def test_the_counts_warning_reaches_the_verdict(tmp_path: Path) -> None:
    """A marker matching nothing is a question for the user, not a silent zero."""
    verdict = _watch(tmp_path, [_running()], _FakeSql("2\t0\t0\t812\n")).tick()

    assert verdict.bots == 0
    assert verdict.warning != ""


def test_a_game_with_no_measured_block_yet_says_so_and_still_reports_the_container(
    tmp_path: Path,
) -> None:
    """8.1c and 8.1d each add their tree's block; until then there are no counts."""
    unmeasured = catalog_module.load_catalog().get("wow-vanilla")
    watch = dashboard.Dashboard(
        unmeasured.container_spec(),
        unmeasured,
        tmp_path,
        sql=_FakeSql(),
        state_of=lambda _c: _running(),
        now=lambda: NOW,
    )

    verdict = watch.tick()

    assert verdict.state == "up"
    assert verdict.players is None
    assert "wow-vanilla" in verdict.problem


# -- the sentence the tab shows --------------------------------------------
#
# Pure, and tested here rather than through the widget: what this says is the
# whole user-visible half of 8.1a, and a phrase that only a GUI test can reach
# is a phrase nobody reads twice.


def test_the_line_leads_with_the_two_counts_because_that_is_what_was_asked_for() -> None:
    verdict = dashboard.Verdict("up", players=3, bots=497, uptime=timedelta(hours=2, minutes=14))

    assert dashboard.line(verdict) == "up — 3 players, 497 bots, up 2h 14m"


def test_the_line_says_restart_loop_in_those_words_with_the_count() -> None:
    """The word a person searches for when their server "keeps going down"."""
    verdict = dashboard.Verdict("restart_loop", restarts=4, uptime=timedelta(seconds=30))

    assert "restart loop" in dashboard.line(verdict)
    assert "4 restarts" in dashboard.line(verdict)


def test_a_refused_count_is_given_as_its_reason_and_never_as_two_zeroes() -> None:
    verdict = dashboard.Verdict("up", problem="AiPlayerbot.RandomBotAccountPrefix is blank")

    line = dashboard.line(verdict)

    assert "blank" in line
    assert "0 players" not in line


def test_the_warning_rides_along_with_the_counts_rather_than_replacing_them() -> None:
    verdict = dashboard.Verdict("up", players=2, bots=0, warning="no account matched 'rndbot'")

    line = dashboard.line(verdict)

    assert "2 players, 0 bots" in line
    assert "rndbot" in line


def test_an_unknown_state_says_docker_could_not_be_asked_rather_than_pretending() -> None:
    assert "could not" in dashboard.line(dashboard.Verdict("unknown"))


def test_a_stopped_server_says_stopped_and_nothing_else() -> None:
    assert dashboard.line(dashboard.Verdict("stopped")) == "stopped"


def test_an_uptime_under_a_minute_still_reads_as_a_duration() -> None:
    verdict = dashboard.Verdict("up", players=0, bots=0, uptime=timedelta(seconds=42))

    assert "up 42s" in dashboard.line(verdict)
