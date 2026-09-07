"""The account list the Accounts tab shows (8.3a).

A read, and what it leaves OUT is the whole point. A WotLK install with the
owner's settings has 500 bot accounts and one account this app made for itself;
a list that shows all 502 rows is a list nobody can use, and one of those rows
is a credential the user must never be handed a Set-password button for.
"""

from __future__ import annotations

import pytest

from yulon import dbreads, useraccounts
from yulon.catalog.catalog import load_catalog

WOTLK = load_catalog().get("wow-wotlk")
MARKER = dbreads.Marker(prefix="rndbot", source="default")


class _Reader:
    """A SQL seam that answers one statement and records what it was asked."""

    def __init__(self, answer: str = "") -> None:
        self.answer = answer
        self.asked: list[tuple[str, str]] = []

    def query(self, db: str, statement: str) -> str:
        self.asked.append((db, statement))
        return self.answer

    def run_statement(self, db: str, statement: str) -> None:  # pragma: no cover
        raise AssertionError("the list must not write")


def test_the_list_is_the_rows_the_server_has() -> None:
    sql = _Reader("1\tALICE\t0\n7\tBOB\t3\n")

    listing = useraccounts.accounts(sql, WOTLK, MARKER, app_account="YULON_AB12CD34")

    assert [(a.id, a.username, a.gm_level) for a in listing.accounts] == [
        (1, "ALICE", 0),
        (7, "BOB", 3),
    ]
    assert listing.problem == ""


def test_the_bots_are_left_out_by_this_installs_own_marker() -> None:
    """500 of them, and the marker is the live value 8.1a resolves."""
    sql = _Reader("")

    useraccounts.accounts(sql, WOTLK, MARKER, app_account="YULON_AB12CD34")

    statement = sql.asked[0][1]
    assert "RNDBOT%" in statement.upper()
    assert "NOT (" in statement


def test_the_apps_own_account_is_left_out_because_it_is_not_the_users_to_change() -> None:
    """It is in the list's WHERE, not filtered afterwards.

    Filtering after the read would leave the row on the wire and in any log of
    the statement, and would put the burden on every future caller to remember.
    """
    sql = _Reader("")

    useraccounts.accounts(sql, WOTLK, MARKER, app_account="YULON_AB12CD34")

    # By the prefix every account this app makes carries, not by this install's
    # own name: two installs can share an auth database, and the neighbour's
    # channel account must not be listed either.
    assert "LEFT(a.username, 6) <> 'YULON_'" in sql.asked[0][1]


def test_a_row_the_read_cannot_parse_is_a_problem_rather_than_a_silent_gap() -> None:
    sql = _Reader("1\tALICE\t0\nnonsense\n")

    listing = useraccounts.accounts(sql, WOTLK, MARKER, app_account="YULON_AB")

    assert listing.accounts == []
    assert "nonsense" in listing.problem


def test_a_database_that_cannot_be_read_says_so_rather_than_showing_no_accounts() -> None:
    """An empty list and an unreadable database look identical on screen.

    One of them means "this server has no accounts" and the other means "do not
    trust what you are looking at", and the tab has to be able to tell a person
    which.
    """

    class _Broken(_Reader):
        def query(self, db: str, statement: str) -> str:
            raise RuntimeError("the database container is not running")

    listing = useraccounts.accounts(_Broken(), WOTLK, MARKER, app_account="YULON_AB")

    assert listing.accounts == []
    assert "not running" in listing.problem


def test_the_gm_level_comes_from_where_this_tree_keeps_it() -> None:
    """A per-tree fact, in the catalog, measured on this install.

    AzerothCore keeps the level in `account_access` keyed `id`; the CMaNGOS
    trees keep it on the account row. Reading the wrong one shows every account
    as level 0, which is a lie that looks like an answer.
    """
    sql = _Reader("")

    useraccounts.accounts(sql, WOTLK, MARKER, app_account="YULON_AB")

    statement = sql.asked[0][1]
    assert "account_access" in statement
    assert "gmlevel" in statement


def test_a_game_whose_level_store_has_not_been_measured_refuses_to_guess() -> None:
    entry = load_catalog().get("wow-tbc")
    if entry.accounts.level is not None:  # pragma: no cover - it has been measured since
        pytest.skip("wow-tbc's level store has been measured")

    listing = useraccounts.accounts(_Reader(), entry, MARKER, app_account="YULON_AB")

    assert listing.accounts == []
    assert entry.name in listing.problem


# -- the two actions, through the server's own commands ---------------------


class _Channel:
    """A channel that records the command text and answers from a script."""

    def __init__(
        self,
        *outcomes: str,
        reason: str = "",
        indeterminate: bool = False,
    ) -> None:
        self.outcomes = list(outcomes) or ["yes"]
        self.sent: list[str] = []
        self.reason = reason
        self.indeterminate = indeterminate

    def send(self, command: str) -> object:
        self.sent.append(command)
        outcome = self.outcomes.pop(0) if self.outcomes else "yes"
        default_reason = "" if outcome != "unknown" else "the server could not be reached"
        return type(
            "Answer",
            (),
            {
                "outcome": outcome,
                "text": "done" if outcome == "yes" else "the server said no",
                "reason": self.reason or default_reason,
                "indeterminate": self.indeterminate,
                "denied": False,
            },
        )()


def test_a_password_change_is_the_servers_own_command_and_not_a_row_written_here() -> None:
    """Owner answer 7: the server performs its own writes.

    A password written as a row is a row this app has to get exactly right on
    every core, forever. The server already knows how; it is asked.
    """
    channel = _Channel()

    outcome = useraccounts.set_password(
        channel, account="ALICE", password="n3w-p@ss", app_account="YULON_AB"
    )

    assert outcome.done is True
    assert channel.sent == ["account set password ALICE n3w-p@ss n3w-p@ss"]


def test_a_level_change_is_the_servers_own_command_across_every_realm() -> None:
    channel = _Channel()

    outcome = useraccounts.set_gm_level(
        channel, account="ALICE", level=2, app_account="YULON_AB", realms=True
    )

    assert outcome.done is True
    assert channel.sent == ["account set gmlevel ALICE 2 -1"]


def test_neither_action_may_be_taken_on_the_apps_own_account() -> None:
    """The clause the box names, enforced where the command is built.

    Not in the UI, where it would be one forgotten `if` away from being gone:
    changing this account's password breaks the channel every other Phase 8
    feature rides on, and changing its level below 3 breaks it just as
    thoroughly.
    """
    for act in (
        lambda c: useraccounts.set_password(
            c, account="yulon_ab", password="whatever1", app_account="YULON_AB"
        ),
        lambda c: useraccounts.set_gm_level(
            c, account="YULON_AB", level=0, app_account="YULON_AB", realms=True
        ),
    ):
        channel = _Channel()

        outcome = act(channel)

        assert outcome.done is False
        assert "its own" in outcome.problem
        assert channel.sent == [], "it sent the command before refusing"


def test_an_argument_the_server_would_refuse_is_refused_here_instead() -> None:
    channel = _Channel()

    outcome = useraccounts.set_password(
        channel, account="ALICE", password="no", app_account="YULON_AB"
    )

    assert outcome.done is False
    assert channel.sent == []


def test_a_server_that_says_no_is_reported_in_the_servers_own_words() -> None:
    outcome = useraccounts.set_password(
        _Channel("no"), account="ALICE", password="n3w-p@ss", app_account="YULON_AB"
    )

    assert outcome.done is False
    assert "the server said no" in outcome.problem


def test_a_channel_that_could_not_ask_says_that_rather_than_that_it_failed() -> None:
    """Three outcomes, not two: "could not ask" is its own answer.

    Reporting an unreachable server as a failed change would have the user
    believe the password is unchanged when nobody knows whether it is.
    """
    outcome = useraccounts.set_gm_level(
        _Channel("unknown"), account="ALICE", level=1, app_account="YULON_AB", realms=True
    )

    assert outcome.done is False
    assert "could not be reached" in outcome.problem


# -- the object the tab holds ------------------------------------------------


def _install(tmp_path, *, sql, channel=None):
    return useraccounts.InstallAccounts(
        WOTLK,
        tmp_path,
        sql=sql,
        channel_for_saved=lambda: channel,
        app_account="YULON_AB12CD34",
    )


def test_the_listing_uses_this_installs_own_marker(tmp_path) -> None:
    """Read from the install, not from the catalog's default.

    The owner's servers run 500 bots under whatever prefix that install's conf
    says; a listing built on the shipped default would show every one of them
    the moment somebody changed it.
    """
    conf = tmp_path / WOTLK.observability.bots.prefix_conf_file
    conf.parent.mkdir(parents=True, exist_ok=True)
    conf.write_text("AiPlayerbot.RandomBotAccountPrefix = HOUSEBOT\n", encoding="utf-8")
    sql = _Reader("")

    _install(tmp_path, sql=sql).listing()

    assert "HOUSEBOT%" in sql.asked[0][1].upper()


def test_an_action_with_no_channel_says_where_to_turn_it_on(tmp_path) -> None:
    """Rather than a failure that reads like the server's.

    These changes are made by the server, so with no channel there is nothing
    to ask — and the user needs the one sentence that gets them unstuck, not a
    connection error.
    """
    outcome = _install(tmp_path, sql=_Reader()).set_password("ALICE", "n3w-p@ss")

    assert outcome.done is False
    assert "Server tab" in outcome.problem


def test_the_install_object_refuses_its_own_account_too(tmp_path) -> None:
    channel = _Channel()

    outcome = _install(tmp_path, sql=_Reader(), channel=channel).set_gm_level("yulon_ab12cd34", 0)

    assert outcome.done is False
    assert channel.sent == []


# -- what the second adversarial review found (2026-09-07) -------------------


def test_every_account_this_app_owns_is_protected_and_not_only_this_installs() -> None:
    """Two installs can share one auth database, and then one breaks the other.

    `YULON_AAAAAAAA` and `YULON_BBBBBBBB` are two installs' channel accounts. A
    guard that knows only its own name lists the neighbour's and offers to
    change its password, which ends that install's command channel while saying
    nothing about it. The prefix is the app's, not the install's, and it is what
    the guard reads.
    """
    channel = _Channel()

    outcome = useraccounts.set_password(
        channel, account="YULON_BBBBBBBB", password="n3w-p@ss", app_account="YULON_AAAAAAAA"
    )

    assert outcome.done is False
    assert channel.sent == []


def test_an_ordinary_account_that_merely_starts_with_the_letters_is_not_protected() -> None:
    """`YULONGATE` is a person's account on the gate box, and it is theirs to change."""
    channel = _Channel()

    outcome = useraccounts.set_password(
        channel, account="YULONGATE", password="n3w-p@ss", app_account="YULON_AAAAAAAA"
    )

    assert outcome.done is True
    assert channel.sent == ["account set password YULONGATE n3w-p@ss n3w-p@ss"]


def test_the_list_leaves_out_every_account_this_app_owns() -> None:
    sql = _Reader("")

    useraccounts.accounts(sql, WOTLK, MARKER, app_account="YULON_AB12CD34")

    statement = sql.asked[0][1]
    assert "LEFT(a.username, 6) <> 'YULON_'" in statement


def test_a_change_the_server_did_not_answer_in_time_is_not_reported_as_a_failure() -> None:
    """A timeout means the command MAY have run.

    The SOAP listener queues onto the world thread and blocks until the command
    finishes, so a client giving up says nothing about whether the server did.
    Told "that did not work", a person retypes the old password and is locked
    out of an account whose password has already changed.
    """

    class _Slow:
        sent: list[str] = []

        def send(self, command: str) -> object:
            self.sent.append(command)
            return type(
                "Answer",
                (),
                {
                    "outcome": "unknown",
                    "text": "",
                    "reason": "the server did not answer within 20s",
                    "indeterminate": True,
                    "denied": False,
                },
            )()

    outcome = useraccounts.set_password(
        _Slow(), account="ALICE", password="n3w-p@ss", app_account="YULON_AB"
    )

    assert outcome.done is False
    assert outcome.indeterminate is True
    assert "may already" in outcome.problem.lower()


# -- 8.3b: the reply is a hint; the row is the answer ------------------------


class _Credentials:
    """A reader whose account row changes the first time it is asked twice."""

    def __init__(self, *, changes: bool = True) -> None:
        self.changes = changes
        self.reads: list[str] = []
        self._n = 0

    def query(self, db: str, statement: str) -> str:
        self.reads.append(statement)
        self._n += 1
        if not self.changes:
            return "AAAA\tBBBB\n"
        return "AAAA\tBBBB\n" if self._n == 1 else "CCCC\tDDDD\n"


def test_a_password_change_this_core_reports_as_failed_is_confirmed_by_the_row() -> None:
    """Measured on m910q, 2026-09-07, and it is by design in the core.

    CMaNGOS's `HandleAccountSetPasswordCommand` sends its success message and
    then `SetSentErrorMessage(true); return false;` -- deliberately, "to avoid
    normal report for hide passwords" (`Level3.cpp:1178-1183`). SOAP turns a
    handler that returned false into a fault, so a SUCCESSFUL change comes back
    as a failure; on the live TBC server the salt and verifier both moved while
    the app was told the channel was unreachable.

    Telling a person their password did not change when it did is the worst of
    the available wrongs: they retype the old one, for an account that no longer
    has it. 8.3a's own review raised this hazard for timeouts; here it is
    guaranteed rather than occasional.

    So the row decides. The reply is a hint.
    """
    reader = _Credentials(changes=True)

    outcome = useraccounts.set_password(
        _Channel("unknown"),
        account="ALICE",
        password="n3w-p@ss34",
        app_account="YULON_AB",
        credentials=lambda name: reader.query("auth", f"SELECT s, v ... {name}"),
    )

    assert outcome.done is True, outcome.problem
    assert "changed" in outcome.text.lower(), outcome.text


def test_a_password_change_that_really_did_nothing_still_reports_the_problem() -> None:
    """The other half. An unchanged row plus an unhappy reply is a failure."""
    reader = _Credentials(changes=False)

    outcome = useraccounts.set_password(
        _Channel("unknown"),
        account="ALICE",
        password="n3w-p@ss34",
        app_account="YULON_AB",
        credentials=lambda name: reader.query("auth", f"SELECT s, v ... {name}"),
    )

    assert outcome.done is False
    assert outcome.problem, "a failure with no sentence is not an answer"


def test_a_clean_yes_is_believed_without_a_second_read() -> None:
    """A core that answers properly is believed, and asked nothing further.

    The BEFORE read cannot be conditional -- it has to be taken before the
    command is sent, when nothing yet knows whether the reply will be usable --
    but the second one is only needed where the first answer was not an answer.
    One SELECT on the happy path, two on the path that needs them.
    """
    reader = _Credentials(changes=True)

    outcome = useraccounts.set_password(
        _Channel("yes"),
        account="ALICE",
        password="n3w-p@ss34",
        app_account="YULON_AB",
        credentials=lambda name: reader.query("auth", f"SELECT s, v ... {name}"),
    )

    assert outcome.done is True
    assert len(reader.reads) == 1, "a clean answer needed no second opinion"


def test_a_password_change_with_no_reader_falls_back_to_the_reply() -> None:
    """`credentials` is optional, and without it the reply is all there is.

    Kept optional so a caller that has no database seam -- a test, a tree whose
    box has not measured its columns -- still gets the old behaviour rather than
    an exception.
    """
    outcome = useraccounts.set_password(
        _Channel("yes"),
        account="ALICE",
        password="n3w-p@ss34",
        app_account="YULON_AB",
    )

    assert outcome.done is True


def test_an_indeterminate_answer_does_not_invent_the_reason_it_is_indeterminate() -> None:
    """Two different machines end here, and only one of them is a timeout.

    A timeout is this app giving up while the server works on. A CMaNGOS
    refusal is the opposite: the server hung up on us, immediately (8.3b,
    measured at the wire). Telling a person "the server keeps working on a
    command after this app stops waiting" for the second one describes
    something that did not happen, and the reason the channel supplies already
    says what did.
    """
    outcome = useraccounts.set_password(
        _Channel(
            "unknown",
            reason=(
                "the server took the command and closed the connection without answering. "
                "On this server that is also what a refused command looks like, so the "
                "command may have run."
            ),
            indeterminate=True,
        ),
        account="ALICE",
        password="n3w-p@ss34",
        app_account="YULON_AB",
    )

    assert outcome.indeterminate is True
    assert "closed the connection" in outcome.problem
    assert "stops waiting" not in outcome.problem, outcome.problem
    assert "check" in outcome.problem.lower(), "it still tells them what to do"
