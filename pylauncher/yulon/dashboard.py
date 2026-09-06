"""One verdict about one install, composed from state the app already has (8.1a).

The bug this closes, verbatim from `pyplan/bug-checklist.md:499`:

    HIGH — A TBC server whose mangosd is stuck in a restart loop is reported as
    a fully successful install, end to end through the GUI.

A crash-looping worldserver is in `docker ps` between its restarts, so every
check that asks "is a container running" answers yes, and the install reports
success while nobody can play. Two facts separate the two states and both come
from the single `docker inspect` this module already pays for: the restart
count, and how long the CURRENT run has lasted.

**Nothing here runs a command on the world thread**, on this timer or any other
(the decision page's "no periodic world-thread command, on any timer, for any
reason"). A tick is one `docker inspect` and, when the server is up, one SQL
read. That is also why `tick()` asks the database nothing when the world is
down: its database is down too, so the query would spend the tick timing out to
say what the container state already said.
"""

from __future__ import annotations

import re
from collections.abc import Callable
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Literal

from yulon import dbreads, docker
from yulon.catalog.catalog import CatalogEntry
from yulon.log import get_logger

logger = get_logger(__name__)

State = Literal["up", "stopped", "restart_loop", "unknown"]

SETTLED_AFTER = timedelta(minutes=10)
"""How long a run must have lasted before a restarted server is called steady again.

A **policy**, not a measurement, and it is written down here rather than left
implicit. The rule it replaces — "clear the loop as soon as one tick sees the
count unchanged" — reads a crash cycle as health for most of every cycle,
because a worldserver takes minutes to load its maps and a loop is therefore
quiet between crashes. Ten minutes is past any first-boot load measured on these
four trees and short enough that a server which really has settled stops being
called unstable within one session.
"""

_DOCKER_FRACTION = re.compile(r"\.(\d{1,9})")


@dataclass(frozen=True)
class Verdict:
    """What one tick found, with every field a caller might otherwise guess at.

    `players` and `bots` are `None` — never zero — whenever they are not known,
    for the reason `dbreads.Population` gives: a number nobody could read must
    not look like a number that was read and came back empty.
    """

    state: State
    restarts: int = 0
    started_at: str = ""
    uptime: timedelta | None = None
    players: int | None = None
    bots: int | None = None
    problem: str = ""
    warning: str = ""

    @property
    def stable(self) -> bool:
        """Whether a later step may aim a command at this server.

        The interlock itself belongs to 8.2a, where the first command button
        exists. This box publishes the state it will key off, so that when the
        interlock arrives it is reading a value that has already been proved
        against a real crash loop rather than one written the same day.

        `unknown` is False on purpose: a server nobody could ask about is not a
        server anyone should fire a command at.
        """
        return self.state == "up"


def line(verdict: Verdict) -> str:
    """The one sentence the Server tab shows above its three up/down words.

    Pure, so what it says can be asserted without a widget: this is the whole
    user-visible half of 8.1a, and a phrase reachable only through a GUI test is
    a phrase nobody reads twice.

    It never fills a gap with a number. A count that could not be read is given
    as the reason it could not, because "0 players" and "I could not ask" look
    identical on a tab and mean opposite things.
    """
    if verdict.state == "stopped":
        return "stopped"
    if verdict.state == "unknown":
        return "could not be asked — docker did not answer about this container"
    parts: list[str] = []
    if verdict.state == "restart_loop":
        head = f"restart loop — {verdict.restarts} restarts"
        parts.append(head + (f", this run {_uptime(verdict.uptime)}" if verdict.uptime else ""))
    else:
        counts = (
            f"{verdict.players} players, {verdict.bots} bots"
            if verdict.players is not None and verdict.bots is not None
            else verdict.problem
        )
        parts.append(f"up — {counts}" if counts else "up")
        if verdict.uptime is not None:
            parts[-1] += f", {_uptime(verdict.uptime)}"
    if verdict.warning:
        parts.append(verdict.warning)
    if verdict.problem and verdict.players is not None:
        parts.append(verdict.problem)
    return " · ".join(parts)


def _uptime(uptime: timedelta | None) -> str:
    """`up 2h 14m`, and `up 42s` for a run that has not seen a minute yet."""
    if uptime is None:
        return ""
    seconds = int(uptime.total_seconds())
    if seconds < 60:
        return f"up {seconds}s"
    hours, rest = divmod(seconds, 3600)
    minutes = rest // 60
    return f"up {hours}h {minutes}m" if hours else f"up {minutes}m"


class Dashboard:
    """Watches one install. Stateful, because a restart count only means something twice.

    A single count says nothing — every server that has ever been restarted has
    a non-zero one. What says something is the count CHANGING between two ticks,
    so the previous value is kept here rather than asked for again.
    """

    def __init__(
        self,
        spec: docker.ContainerSpec,
        entry: CatalogEntry,
        server_dir: Path,
        *,
        sql: dbreads.SqlReader,
        wsl_distro: str | None = None,
        state_of: Callable[[str], docker.ContainerState] | None = None,
        now: Callable[[], datetime] | None = None,
    ) -> None:
        self.spec = spec
        self.entry = entry
        self.server_dir = server_dir
        self.sql = sql
        self._state_of = state_of or (
            lambda container: docker.container_state(container, wsl_distro=wsl_distro)
        )
        self._now = now or (lambda: datetime.now(UTC))
        self._last_restarts: int | None = None
        self._looping = False

    def tick(self) -> Verdict:
        """Ask once, and answer with everything that was learned."""
        state = self._state_of(self.spec.world)
        uptime = self._uptime(state.started_at)
        grew = self._last_restarts is not None and state.restart_count > self._last_restarts
        self._last_restarts = state.restart_count
        if grew:
            self._looping = True
        elif self._looping and uptime is not None and uptime >= SETTLED_AFTER:
            self._looping = False

        if state.status == "":
            return Verdict("unknown", state.restart_count, state.started_at, uptime)
        if state.status == "restarting" or (self._looping and state.status == "running"):
            return Verdict("restart_loop", state.restart_count, state.started_at, uptime)
        if state.status != "running":
            return Verdict("stopped", state.restart_count, state.started_at, uptime)
        return self._with_population(state, uptime)

    def _with_population(self, state: docker.ContainerState, uptime: timedelta | None) -> Verdict:
        """The two counts, or the reason there are none. Never a wrong number."""
        answer = dbreads.resolve_marker(self.entry, self.server_dir)
        if answer.marker is None:
            return Verdict(
                "up", state.restart_count, state.started_at, uptime, problem=answer.problem
            )
        counts = dbreads.population(self.sql, self.entry, answer.marker)
        return Verdict(
            "up",
            state.restart_count,
            state.started_at,
            uptime,
            players=counts.players,
            bots=counts.bots,
            problem=counts.problem,
            warning=counts.warning,
        )

    def _uptime(self, started_at: str) -> timedelta | None:
        """How long the current run has lasted, or `None` if that cannot be read.

        Docker prints nine fractional digits and `fromisoformat` accepts three
        or six, so the fraction is trimmed rather than the whole timestamp
        thrown away. A timestamp that still will not parse leaves the uptime
        absent — an absent duration is honest, a wrong one is not.
        """
        if not started_at:
            return None
        text = _DOCKER_FRACTION.sub(lambda m: "." + m.group(1)[:6], started_at.strip())
        try:
            started = datetime.fromisoformat(text)
        except ValueError:
            logger.debug(f"could not read the container's start time {started_at!r}")
            return None
        if started.tzinfo is None:
            started = started.replace(tzinfo=UTC)
        return self._now() - started
