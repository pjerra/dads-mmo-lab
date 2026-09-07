"""One command in, one typed answer out (Phase 8.2a).

The seam every feature above this calls. It holds no command text — that is
`commands.py` — and no UI. What it owns is three things:

**The three-outcome answer.** `yes` is a command the server ran and reported as
succeeding. `no` is one it ran and refused. `unknown` is everything else, and it
is not a failure of the command: it is the absence of an answer about it. The
spike that measured a published port against a listener on the container's own
loopback found exactly these three on the wire — refused, silent, answering —
so the type is the shape the transport already has.

**A write is never retried.** A timeout carries `indeterminate=True` and stops
there. The command may have run: the listener queues onto the world thread and
blocks until it finishes (`ACSoap.cpp:120-130`), so a client giving up says
nothing about whether the server did. A retry that then succeeded would report
"done" for a command that ran twice — two accounts, two mails, two of whatever
it was.

**One lock per install.** The SOAP listener runs on the single world thread, so
two commands in flight are two things queued behind each other at best. The
prior art serialises for the same reason and says so
(`launcher/src-tauri/src/lib.rs:117-123`), noting that even the bash CLI took a
`~/.dml/soap.lock` file lock. Per install rather than global, because this app
can hold two servers open and they are two world threads.

And when it could not ask, it says **why** — read from container state at that
moment rather than guessed, so the sentence tells the user what to do next.
"""

from __future__ import annotations

import threading
from collections.abc import Callable
from dataclasses import dataclass
from typing import Literal, Protocol

from yulon import docker, soap
from yulon.log import get_logger

logger = get_logger(__name__)

Outcome = Literal["yes", "no", "unknown"]


@dataclass(frozen=True)
class Answer:
    """What one command came back as."""

    outcome: Outcome
    text: str = ""
    reason: str = ""
    """Why there is no answer, when there is none. Empty for `yes` and `no`."""
    indeterminate: bool = False
    """True when the command may have run. Only a timeout sets it."""
    denied: bool = False
    """True only when the SERVER said it does not accept this credential.

    Every transport failure and every rejection arrives as `unknown`, so a
    caller reading the outcome alone cannot tell "your password is wrong" from
    "the server is not there". 8.2a's repair rotates a GM account's password on
    the strength of that distinction, and an adversarial review found what
    happens without it: opening the tab against a stopped server read as a bad
    credential and offered to reset a working one.

    `forbidden` is deliberately NOT denied. The account exists and its password
    is right; what is wrong is its GM level, and a new password would burn a
    rotation and change nothing.
    """

    @property
    def known(self) -> bool:
        return self.outcome in ("yes", "no")


class Channel(Protocol):
    """What a feature module is handed. Deliberately one method."""

    def send(self, command: str) -> Answer: ...


class SoapChannel:
    """The SOAP transport: the only one WotLK, TBC and Vanilla need.

    Tortoise has no SOAP at all and reaches its world another way; that
    transport arrives with its own box rather than being written blind here.
    """

    def __init__(
        self,
        *,
        endpoint: soap.Endpoint,
        state_of: Callable[[], docker.ContainerState],
        send: Callable[..., soap.Reply] = soap.execute,
        timeout: float = soap.DEFAULT_TIMEOUT,
    ) -> None:
        self.endpoint = endpoint
        self._state_of = state_of
        self._send = send
        self._timeout = timeout
        self.lock = threading.Lock()
        """This install's lock. One per channel, so two servers do not queue."""

    def send(self, command: str) -> Answer:
        """Ask once, hold the lock only for the asking, and never retry."""
        with self.lock:
            reply = self._send(self.endpoint, command, timeout=self._timeout)
        return self._answer(reply)

    def _answer(self, reply: soap.Reply) -> Answer:
        if reply.outcome == "answered":
            return Answer("yes", reply.text)
        if reply.outcome == "refused":
            return Answer("no", reply.text)
        if reply.outcome == "unauthorised":
            return Answer(
                "unknown",
                reply.text,
                denied=True,
                reason=(
                    "the server did not accept this install's account and password. Its "
                    "credentials may have been changed on the server, or the file this app "
                    "keeps them in may be stale."
                ),
            )
        if reply.outcome == "forbidden":
            return Answer(
                "unknown",
                reply.text,
                reason=(
                    "this install's account exists but its GM level is below administrator, "
                    "which is the level the command channel needs."
                ),
            )
        return Answer(
            "unknown",
            reply.text,
            reason=self._why(reply.outcome),
            indeterminate=reply.outcome == "timeout",
        )

    def _why(self, outcome: str) -> str:
        """Turn a transport failure into the reason the container state gives.

        Asked at the moment it failed rather than remembered, because the whole
        value of the sentence is that it describes now.
        """
        state = self._state_of()
        if state.status == "":
            return "docker could not be asked about this server, so nothing is known about it"
        if state.status == "restarting":
            return (
                f"this server is in a restart loop ({state.restart_count} restarts), so nothing "
                "is listening for long enough to answer"
            )
        if state.status != "running":
            return "this server is not running, so there is nothing to answer"
        if outcome == "timeout":
            # The spike's finding: a running, settled world that accepts and
            # says nothing is a bind nobody can reach, not a slow command.
            return (
                "the server is running but the command channel did not answer. Either it is "
                "still loading, or the listener is bound inside the container where the "
                "published port cannot reach it."
            )
        return (
            "the server is running but nothing is listening on the command channel — it may not "
            "be turned on for this install yet"
        )
