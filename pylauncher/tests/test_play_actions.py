"""Tests for `InstallPlay` — the object the Characters tab presses (8.4a).

The seam where a read and a write meet, which is where this phase's defects have
lived. Every action canonicalises the name first, because the server is as
case-sensitive as its own column, and every action reports all three outcomes.
"""

from __future__ import annotations

import pytest

from yulon import play
from yulon.catalog.catalog import load_catalog

WOTLK = load_catalog().get("wow-wotlk")


class _Reader:
    """A SQL seam that answers by matching on the statement."""

    def __init__(self, **answers: str) -> None:
        self.answers = answers
        self.asked: list[str] = []

    def query(self, db: str, statement: str) -> str:
        self.asked.append(statement)
        for key, answer in self.answers.items():
            if key in statement:
                return answer
        return ""

    def run_statement(self, db: str, statement: str) -> None:  # pragma: no cover
        raise AssertionError("a read must not write")


class _Channel:
    """A channel that records what it was sent and answers from a script."""

    def __init__(self, *outcomes: str, text: str = "done") -> None:
        self.outcomes = list(outcomes)
        self.text = text
        self.sent: list[str] = []

    def send(self, command: str) -> object:
        self.sent.append(command)
        outcome = self.outcomes.pop(0) if self.outcomes else "yes"
        return type(
            "Answer",
            (),
            {
                "outcome": outcome,
                "text": self.text if outcome == "yes" else "the server said no",
                "reason": "" if outcome != "unknown" else "the server could not be reached",
                "indeterminate": False,
                "denied": False,
            },
        )()


def _install(tmp_path, sql=None, channel=None) -> play.InstallPlay:
    return play.InstallPlay(
        WOTLK,
        tmp_path,
        sql=sql if sql is not None else _Reader(**{"SELECT name FROM": "Guglu\n"}),
        channel_for_saved=lambda: channel if channel is not None else _Channel(),
    )


# -- the name is the server's, not the typist's -----------------------------


def test_every_action_sends_the_stored_spelling_of_the_name(tmp_path) -> None:
    """Typed `guglu`, stored `Guglu`, and the command carries `Guglu`.

    The prior art's own bug, and it does not fail loudly: the server answers
    "not online" for a character who is standing in Stormwind.
    """
    channel = _Channel()
    install = _install(tmp_path, channel=channel)

    install.teleport("guglu", "Stormwind")
    install.set_level("GUGLU", 80)
    install.rename("gUgLu")
    install.revive("guglu")

    assert channel.sent == [
        "teleport name Guglu Stormwind",
        "character level Guglu 80",
        "character rename Guglu",
        "revive Guglu",
    ]


def test_a_character_nobody_has_is_refused_before_the_server_is_asked(tmp_path) -> None:
    """And the refusal names what was typed, because that is what the person
    can see and correct."""
    channel = _Channel()
    install = _install(tmp_path, sql=_Reader(), channel=channel)

    outcome = install.teleport("nobodyhere", "Stormwind")

    assert outcome.done is False
    assert "nobodyhere" in outcome.problem
    assert channel.sent == [], "a name that does not exist reached the server"


# -- the three outcomes -----------------------------------------------------


def test_a_command_the_server_refused_is_reported_as_a_refusal(tmp_path) -> None:
    install = _install(tmp_path, channel=_Channel("no"))

    outcome = install.revive("guglu")

    assert outcome.done is False
    assert outcome.indeterminate is False


def test_a_channel_that_could_not_ask_says_so_rather_than_that_it_failed(tmp_path) -> None:
    install = _install(tmp_path, channel=_Channel("unknown"))

    outcome = install.set_level("guglu", 10)

    assert outcome.done is False
    assert "could not be reached" in outcome.problem


# -- mail -------------------------------------------------------------------


def test_mailed_gold_is_turned_into_copper_once(tmp_path) -> None:
    """The button says gold because that is what a person has; the server counts
    copper. One multiplication, in one place."""
    channel = _Channel()
    install = _install(tmp_path, channel=channel)

    install.mail_gold("guglu", gold=5, subject="Wages", body="Well earned")

    assert channel.sent == ['send money Guglu "Wages" "Well earned" 50000']


def test_a_gear_set_larger_than_one_mail_is_sent_as_the_mails_it_needs(tmp_path) -> None:
    """Nineteen pieces and a cap of twelve is two mails, and the second carries
    the remaining seven. The definition of done says the button promises a
    number before the press, so the number comes from the same function that
    does the sending.
    """
    nineteen = "\n".join(str(6000 + n) for n in range(19))
    sql = _Reader(**{"SELECT name FROM": "Guglu\n", "character_inventory": nineteen})
    channel = _Channel()
    install = _install(tmp_path, sql=sql, channel=channel)

    outcome = install.send_gear_set("guglu", to="guglu", subject="Set", body="Wear it")

    assert outcome.done is True
    assert len(channel.sent) == 2, channel.sent
    assert channel.sent[0].count(":1") == 12
    assert channel.sent[1].count(":1") == 7


def test_the_number_of_mails_a_set_needs_can_be_asked_before_pressing(tmp_path) -> None:
    nineteen = "\n".join(str(6000 + n) for n in range(19))
    sql = _Reader(**{"SELECT name FROM": "Guglu\n", "character_inventory": nineteen})
    install = _install(tmp_path, sql=sql)

    assert install.gear_set_size("guglu") == (19, 2)


def test_a_gear_set_from_a_character_wearing_nothing_is_refused(tmp_path) -> None:
    """Rather than sending an empty mail, which the server would refuse anyway
    with a sentence about item ids that says nothing about gear."""
    sql = _Reader(**{"SELECT name FROM": "Guglu\n"})
    channel = _Channel()
    install = _install(tmp_path, sql=sql, channel=channel)

    outcome = install.send_gear_set("guglu", to="guglu", subject="s", body="b")

    assert outcome.done is False
    assert "nothing" in outcome.problem.lower() or "no items" in outcome.problem.lower()
    assert channel.sent == []


def test_a_set_that_fails_half_way_says_which_mails_went(tmp_path) -> None:
    """Two mails, the second refused. Reporting a plain failure would have a
    person send the whole set again and the recipient get the first twelve
    twice."""
    nineteen = "\n".join(str(6000 + n) for n in range(19))
    sql = _Reader(**{"SELECT name FROM": "Guglu\n", "character_inventory": nineteen})
    channel = _Channel("yes", "no")
    install = _install(tmp_path, sql=sql, channel=channel)

    outcome = install.send_gear_set("guglu", to="guglu", subject="s", body="b")

    assert outcome.done is False
    assert "1" in outcome.problem and "2" in outcome.problem, outcome.problem


# -- what the tab may draw --------------------------------------------------


def test_a_tree_with_no_measured_play_block_offers_nothing(tmp_path) -> None:
    """The tab draws what the tree has, and a button drawn on a tree nobody has
    asked would send a command nobody has checked.

    The unmeasured tree here was Vanilla until 8.4c measured it; the guard moved
    to Tortoise rather than going, because it is the guard the whole
    `NotMeasured` class exists for and there is still exactly one tree holding
    it. When 8.4d measures Tortoise there will be no tree left to stand here,
    and the honest thing then is a synthesised entry rather than a deletion.
    """
    tortoise = load_catalog().get("wow-tortoise")

    assert play.InstallPlay.for_entry_is_possible(tortoise) is False
    assert play.InstallPlay.for_entry_is_possible(WOTLK) is True


def test_the_mail_cap_offered_is_the_one_this_tree_carries(tmp_path) -> None:
    install = _install(tmp_path)

    assert install.mail_item_cap == 12


def test_an_unmeasured_tree_has_no_cap_to_offer_rather_than_a_plausible_one(
    tmp_path,
) -> None:
    """The fallback answered 1, which stopped being a refusal on 2026-09-07.

    It was written when no tree carried a cap of one, so the number could only
    mean "nobody measured this". Vanilla's is 1 (`Mail.h:49`), so from 8.4c an
    unmeasured tree and a measured one answer the same thing at this property —
    and a gear button on the unmeasured one would promise a mail per piece with
    nothing behind the promise. It raises instead, in the same voice
    `play.equipped` already refuses in.
    """
    tortoise = load_catalog().get("wow-tortoise")
    install = play.InstallPlay(
        tortoise, tmp_path, sql=_Reader(), channel_for_saved=lambda: _Channel()
    )

    with pytest.raises(play.NotMeasured) as refused:
        install.mail_item_cap  # noqa: B018

    assert "WoW Tortoise" in str(refused.value)


def test_the_cap_is_this_trees_own_and_not_its_siblings() -> None:
    """Same core, same command, different number — asserted in one breath.

    Read on m910q, 2026-09-07, from the two installs' own checkouts: Vanilla's
    `src/mangos-classic/src/game/Mails/Mail.h:49` is `#define MAX_MAIL_ITEMS 1`
    and TBC's `src/mangos-tbc/src/game/Mails/Mail.h:49`, the same line of the
    same header, is `12`. The server enforces it at
    `src/game/Chat/Level3.cpp:6393` and, being a `return false`, the refusal
    reaches SOAP as a closed connection rather than as a sentence (8.3b).

    Both halves are in ONE test on purpose. 8.4c's whole hazard is pasting the
    sibling's block over, which would leave a Vanilla cap that reads 12 and a
    gear button promising two mails for a server that will send neither.
    """
    catalog = load_catalog()

    assert catalog.get("wow-vanilla").play.mail_item_cap == 1
    assert catalog.get("wow-tbc").play.mail_item_cap == 12


@pytest.mark.parametrize("level", [0, 256])
def test_a_level_this_command_does_not_take_is_refused_with_its_reason(
    tmp_path, level: int
) -> None:
    install = _install(tmp_path, channel=_Channel())

    outcome = install.set_level("guglu", level)

    assert outcome.done is False
    assert str(level) in outcome.problem


def test_a_half_sent_set_says_what_arrived_and_does_not_advise_the_impossible(
    tmp_path,
) -> None:
    """8.4a's adversarial review: the sentence offered an operation nobody has.

    "send only what is missing" is not a thing this app can do — there is one
    button and it sends everything worn — so a person following that advice
    presses it again and the recipient gets the first twelve items twice.

    What the sentence can honestly do is say which mails arrived, that pressing
    again sends the whole set from the start, and that the ones already
    delivered will arrive a second time if they do.
    """
    nineteen = "\n".join(str(6000 + n) for n in range(19))
    sql = _Reader(**{"SELECT name FROM": "Guglu\n", "character_inventory": nineteen})
    install = _install(tmp_path, sql=sql, channel=_Channel("yes", "no"))

    outcome = install.send_gear_set("guglu", to="guglu", subject="s", body="b")

    assert outcome.done is False
    said = outcome.problem.lower()
    assert "1 of 2" in said or "1 of the 2" in said, outcome.problem
    assert "again" in said, "it does not say what pressing again would do"
    assert "missing" not in said, "it still advises an operation that does not exist"
