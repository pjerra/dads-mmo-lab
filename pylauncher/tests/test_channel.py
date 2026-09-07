"""Tests for `yulon.channel` — one command, one typed answer (8.2a).

The seam every feature above it calls. It holds no command text and no UI; what
it owns is the three-outcome answer, the per-install lock, and turning a
transport failure into a reason a person can act on.
"""

from __future__ import annotations

import threading
import time

from yulon import channel, docker, soap


def _state(status: str = "running", restarts: int = 0) -> docker.ContainerState:
    return docker.ContainerState(status, "2026-09-07T10:00:00.000000000Z", restarts)


class _Wire:
    """A stand-in for `soap.execute`, recording what it was asked."""

    def __init__(self, *replies: soap.Reply) -> None:
        self.replies = list(replies)
        self.asked: list[str] = []

    def __call__(self, endpoint: soap.Endpoint, command: str, *, timeout: float = 0) -> soap.Reply:
        self.asked.append(command)
        return self.replies[min(len(self.asked) - 1, len(self.replies) - 1)]


def _channel(wire: _Wire, state: docker.ContainerState | None = None) -> channel.SoapChannel:
    return channel.SoapChannel(
        endpoint=soap.Endpoint(host="127.0.0.1", port=7878, account="YULON_AB", password="pw"),
        state_of=lambda: state if state is not None else _state(),
        send=wire,
    )


# -- the three outcomes -----------------------------------------------------


def test_a_command_the_server_ran_is_a_yes_carrying_what_it_printed() -> None:
    answer = _channel(_Wire(soap.Reply("answered", "Players online: 3."))).send("server info")

    assert answer.outcome == "yes"
    assert answer.text == "Players online: 3."
    assert answer.indeterminate is False


def test_a_command_the_server_refused_is_a_no_and_not_a_could_not_ask() -> None:
    """The server was reached and answered; the command is what failed."""
    answer = _channel(_Wire(soap.Reply("refused", "Account cannot be created."))).send("x")

    assert answer.outcome == "no"
    assert answer.text == "Account cannot be created."


def test_a_timeout_is_could_not_ask_and_is_marked_indeterminate() -> None:
    """It may have run. That is the whole difficulty and it is carried, not hidden."""
    answer = _channel(_Wire(soap.Reply("timeout", "no answer within 20s"))).send("account create x")

    assert answer.outcome == "unknown"
    assert answer.indeterminate is True


def test_a_timeout_is_never_retried_because_the_command_may_have_run() -> None:
    """Proved with a wire that answers differently the second time.

    A retry that succeeded would report "done" for a command that ran twice —
    two accounts, two mails, two of whatever it was. The seam asks once.
    """
    wire = _Wire(soap.Reply("timeout", "no answer"), soap.Reply("answered", "Account created."))

    answer = _channel(wire).send("account create bob pw")

    assert wire.asked == ["account create bob pw"], "it asked a second time"
    assert answer.outcome == "unknown"


# -- why, when it could not ask ---------------------------------------------


def test_an_unreachable_channel_says_the_world_is_not_running_when_it_is_not() -> None:
    """The reason comes from container state, so the user is told what to do."""
    wire = _Wire(soap.Reply("unreachable", "connection refused"))

    answer = _channel(wire, _state(status="exited")).send("server info")

    assert answer.outcome == "unknown"
    assert "not running" in answer.reason


def test_an_unreachable_channel_says_restart_loop_when_that_is_what_it_is() -> None:
    wire = _Wire(soap.Reply("unreachable", "connection refused"))

    answer = _channel(wire, _state(status="restarting", restarts=9)).send("server info")

    assert "restart" in answer.reason


def test_a_silent_port_on_a_running_world_names_the_misconfiguration() -> None:
    """The spike's finding, turned into a sentence.

    A listener on the container's own loopback leaves Docker accepting and
    relaying nothing, so a timeout against a world that IS running and settled
    is not a slow command — it is a bind nobody can reach.
    """
    wire = _Wire(soap.Reply("timeout", "no answer within 20s"))

    answer = _channel(wire, _state(status="running")).send("server info")

    assert answer.outcome == "unknown"
    assert "listening" in answer.reason or "bound" in answer.reason


def test_bad_credentials_say_so_rather_than_blaming_the_server() -> None:
    answer = _channel(_Wire(soap.Reply("unauthorised", "no"))).send("server info")

    assert answer.outcome == "unknown"
    assert "password" in answer.reason or "account" in answer.reason


def test_an_account_below_administrator_gets_its_own_sentence() -> None:
    """Different repair from a wrong password, so a different sentence."""
    answer = _channel(_Wire(soap.Reply("forbidden", "no"))).send("server info")

    assert "level" in answer.reason


# -- the lock ---------------------------------------------------------------


def test_two_commands_on_one_install_do_not_overlap() -> None:
    """The listener runs on the single world thread.

    The prior art serialises for exactly this reason, and even the bash CLI took
    a lock file (`launcher/src-tauri/src/lib.rs:117-123`).
    """
    overlaps: list[bool] = []
    inside = threading.Event()

    class _Slow(_Wire):
        def __call__(self, endpoint, command, *, timeout=0):  # noqa: ANN001, ANN204
            overlaps.append(inside.is_set())
            inside.set()
            time.sleep(0.05)
            inside.clear()
            return soap.Reply("answered", "ok")

    one = _channel(_Slow())
    threads = [threading.Thread(target=lambda: one.send("server info")) for _ in range(4)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()

    assert overlaps == [False, False, False, False], "two commands were in flight at once"


def test_two_installs_do_not_wait_on_each_other() -> None:
    """The lock is per install, not global: two servers are two world threads."""
    first, second = _channel(_Wire(soap.Reply("answered", "a"))), _channel(
        _Wire(soap.Reply("answered", "b"))
    )

    assert first.lock is not second.lock


# -- what "the server said no to this credential" means (review, 2026-09-07) --


def test_only_a_rejected_credential_is_marked_denied() -> None:
    """The one outcome that justifies resetting an account's password.

    Every transport failure and every refusal arrives as `unknown`, so a caller
    reading the outcome alone cannot tell "your password is wrong" from "the
    server is not there" -- and 8.2a's repair, offered on the strength of that
    distinction, rotates a GM account's password. An adversarial review found
    it: opening the tab against a stopped server offered to reset a working
    credential.
    """
    for outcome, denied in (
        ("unauthorised", True),
        ("forbidden", False),
        ("unreachable", False),
        ("timeout", False),
        ("unreadable", False),
        ("answered", False),
        ("refused", False),
    ):
        answer = _channel(_Wire(soap.Reply(outcome, "text", 0))).send("server info")
        assert answer.denied is denied, f"{outcome} should be denied={denied}"


def test_a_level_too_low_is_not_denied_because_a_new_password_would_not_fix_it() -> None:
    """`forbidden` is a real refusal of this credential, and repair is wrong for it.

    The account exists and its password is right; what is wrong is its GM
    level. Rotating the password would burn a rotation and change nothing.
    """
    answer = _channel(_Wire(soap.Reply("forbidden", "", 403))).send("server info")

    assert answer.denied is False
    assert "GM level" in answer.reason
