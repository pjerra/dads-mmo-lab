"""The Server tab's state at a glance (T699): the status pill, one row per service, Get playing.

Pure, so every word and shape can be asserted without a widget. The widgets in
`widgets/server_cards.py` draw what these answer; the colours are `theme.STATUS_TONES`.

Every state has its own colour, its own SHAPE and its own WORD, so it reads the same
for a player who cannot tell green from amber (the owner's mockup, 2026-10-11):
ONLINE is a filled dot, STARTING a half-filled one, PARTLY UP a triangle, OFFLINE a
hollow ring. The other words the realm badge has (stopping, restarting, crash loop,
update failed, unknown) keep a shape of their own too.

The input is the realm badge's status word (`DadcraftRealmBadge.status`), which is
already the one reading the tray, the header and the launcher follow: the pill is that
reading drawn bigger, never a second one.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal

from yulon.controller import InstallStatus

Shape = Literal["dot", "half", "triangle", "ring", "question", "cross", "diamond"]
Tone = Literal["up", "between", "partly", "down", "unknown", "bad"]


@dataclass(frozen=True)
class PillLook:
    """What the pill says for one realm state: its word, its shape and its tone."""

    key: str
    word: str
    shape: Shape
    tone: Tone


ONLINE = PillLook("online", "ONLINE", "dot", "up")
STARTING = PillLook("starting", "STARTING", "half", "between")
STOPPING = PillLook("stopping", "STOPPING", "half", "between")
RESTARTING = PillLook("restarting", "RESTARTING", "half", "between")
PARTLY_UP = PillLook("partly", "PARTLY UP", "triangle", "partly")
CRASH_LOOP = PillLook("loop", "CRASH LOOP", "diamond", "bad")
UPDATE_FAILED = PillLook("failed", "UPDATE FAILED", "cross", "bad")
UNKNOWN = PillLook("unknown", "STATUS UNKNOWN", "question", "unknown")
OFFLINE = PillLook("offline", "OFFLINE", "ring", "down")

_BY_STATUS: dict[str, PillLook] = {
    "running": ONLINE,
    "online": ONLINE,
    "ready": ONLINE,
    "up": ONLINE,
    "starting": STARTING,
    "importing": STARTING,
    "working": STARTING,
    "building": STARTING,
    "stopping": STOPPING,
    "restarting": RESTARTING,
    "partial": PARTLY_UP,
    "loop": CRASH_LOOP,
    "failed": UPDATE_FAILED,
    "unknown": UNKNOWN,
}


def pill_for(status: str) -> PillLook:
    """The pill for a realm badge status word; anything else (stopped included) is OFFLINE."""
    return _BY_STATUS.get(status.lower(), OFFLINE)


SIDEBAR_WORDS = {
    "online": "Online",
    "starting": "Starting",
    "stopping": "Stopping",
    "restarting": "Restarting",
    "partly": "Partly up",
    "loop": "Crash loop",
    "failed": "Update failed",
    "unknown": "Status unknown",
    "offline": "Offline",
}
"""The pill's word as the sidebar's one line says it."""


def sidebar_line(status: str, players: int | None = None) -> str:
    """The sidebar's one line for a server: "Online · 3 players", "Starting", "Offline"."""
    look = pill_for(status)
    word = SIDEBAR_WORDS[look.key]
    if look is ONLINE and players is not None:
        return f"{word} · {players} {'player' if players == 1 else 'players'}"
    return word


@dataclass(frozen=True)
class ServiceLine:
    """One service's row on the Services card."""

    name: str
    word: str
    shape: Shape
    tone: Tone
    detail: str = ""


SERVICE_NAMES = ("Database", "Login server", "World server")
"""The three services, in the order a server comes up; the same words `_status_words()` uses."""


def service_lines(
    status: InstallStatus | None,
    world: str,
    ports: tuple[int | None, int, int],
) -> tuple[ServiceLine, ServiceLine, ServiceLine]:
    """One line per service from the last status poll and the realm badge's word for the world.

    `status` None is a poll that could not be read: every row says Unknown, never Stopped.
    `world` is the badge's own word (`_badge_word()`): "starting" while the world is up in
    `docker ps` and has not printed its ready marker, "loop" and "failed" from the verdict.
    A stopped service beside a running one is drawn as PARTLY UP's triangle: it is the one
    to look at. With nothing running, every row is OFFLINE's ring.
    """
    db_port, auth_port, world_port = ports
    details = (
        f"port {db_port}" if db_port else "",
        f"port {auth_port}",
        f"port {world_port}",
    )
    if status is None:
        return tuple(  # type: ignore[return-value]
            ServiceLine(name, "Unknown", "question", "unknown", detail)
            for name, detail in zip(SERVICE_NAMES, details, strict=True)
        )
    some = status.any_running

    def stopped(name: str, detail: str) -> ServiceLine:
        if some:
            return ServiceLine(name, "Stopped", "triangle", "partly", detail)
        return ServiceLine(name, "Stopped", "ring", "down", detail)

    def running(name: str, detail: str) -> ServiceLine:
        return ServiceLine(name, "Running", "dot", "up", detail)

    db = (
        running(SERVICE_NAMES[0], details[0])
        if status.db
        else stopped(SERVICE_NAMES[0], details[0])
    )
    auth = (
        running(SERVICE_NAMES[1], details[1])
        if status.auth
        else stopped(SERVICE_NAMES[1], details[1])
    )
    name, detail = SERVICE_NAMES[2], details[2]
    look = pill_for(world)
    if not status.world:
        world_line = stopped(name, detail)
    elif look is CRASH_LOOP:
        world_line = ServiceLine(name, "Crash loop", "diamond", "bad", detail)
    elif look is UPDATE_FAILED:
        world_line = ServiceLine(name, "Update failed", "cross", "bad", detail)
    elif look is STARTING:
        world_line = ServiceLine(
            name, "Loading…", "half", "between", f"{detail} · loading the world"
        )
    elif look is ONLINE:
        world_line = ServiceLine(name, "Ready", "dot", "up", f"{detail} · world loaded")
    else:
        world_line = running(name, detail)
    return db, auth, world_line


def services_summary(status: InstallStatus | None) -> str:
    """The Services card's corner: "3 of 3 running", or "not known" when the poll was not read."""
    if status is None:
        return "not known"
    up = sum((status.db, status.auth, status.world))
    return f"{up} of 3 running"


def partly_up_sentence(status: InstallStatus | None) -> str:
    """For PARTLY UP: which services are not running, and the press that brings them up."""
    if status is None or not status.any_running or status.all_running:
        return ""
    down = [
        name
        for name, up in zip(SERVICE_NAMES, (status.db, status.auth, status.world), strict=True)
        if not up
    ]
    names = " and the ".join(n.lower() for n in down)
    verb = "is" if len(down) == 1 else "are"
    them = "it" if len(down) == 1 else "them"
    return f"The {names} {verb} not running. Press Start to bring {them} up."


@dataclass(frozen=True)
class Step:
    """One step of the Get playing card. `done` None: not known yet (the accounts unread)."""

    key: Literal["server", "client", "account", "play"]
    text: str
    done: bool | None


def get_playing(server_up: bool, has_client: bool, accounts: int | None) -> tuple[Step, ...]:
    """The four steps, in order: Server running, Ready-to-play client, A game account, Press PLAY.

    `accounts` is how many game accounts a person could log in with, or None while that
    has not been read (the server is down, or the read failed): the step is then neither
    ticked nor offered as the thing to do next.
    """
    return (
        Step("server", "Server running", server_up),
        Step("client", "Ready-to-play client", has_client),
        Step("account", "A game account", None if accounts is None else accounts > 0),
        Step("play", "Press PLAY", False),
    )


def steps_done(steps: tuple[Step, ...]) -> str:
    """The card's corner: "2 of 4 done", or "Ready to play" once only PLAY is left."""
    done = sum(1 for step in steps if step.done)
    if all(step.done for step in steps if step.key != "play"):
        return "Ready to play"
    return f"{done} of {len(steps)} done"


MODE_WORDS = {"lan": "LAN", "internet": "Internet", "loopback": "This computer only"}


def mode_now(chosen: str | None, announced: str | None) -> str:
    """The network mode in force, in words, from the Networking tab's last Apply and the realm row.

    `chosen` is `networking.read_network_intent()`'s mode, None when nobody has pressed
    Apply. The install's own realm step then advertised this computer's LAN address,
    unless it found none, which leaves the row on this computer: a realm row read back
    as a loopback address says so.
    """
    if chosen in MODE_WORDS:
        return MODE_WORDS[chosen]
    if announced is not None and (announced.startswith("127.") or announced == "localhost"):
        return "This computer only (set at install)"
    return "LAN (set at install)"
