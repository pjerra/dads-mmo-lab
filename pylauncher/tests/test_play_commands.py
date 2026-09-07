"""Tests for the Play commands — the eight verbs of 8.4a.

Every shape here was read off a live AzerothCore server on 2026-09-07 by asking
it, not off a wiki:

    .teleport name [#playername] #location
    .character level [$playername] [#level]
    .character rename [$name] [reserveName] [$newName]
    .send items #playername "#subject" "#text" itemid1[:count1] ... itemidN[:countN]
    .send money #playername "#subject" "#text" #money
    .revive

and one where the server's own help is WRONG, which is why these are behaviour
readings rather than help readings: `.revive` documents no argument at all, and
`revive NOSUCHCHARACTER` answered *"Character 'Nosuchcharacter' does not
exist."* — it takes a name, and a feature built from the help would have drawn
no revive button on the one tree that has one.
"""

from __future__ import annotations

import pytest

from yulon import commands

# -- teleport ---------------------------------------------------------------


def test_a_teleport_names_the_character_and_the_place() -> None:
    """`teleport name` and not `teleport`: the second moves whoever is selected,
    and over a command channel nobody is selected."""
    assert commands.teleport_to("Guglu", "Stormwind") == "teleport name Guglu Stormwind"


def test_a_teleport_location_is_one_token_of_the_servers_own_alphabet() -> None:
    """The names come from `game_tele`, which on this install holds 1989 of them
    — `7thLegionFront`, `AbandonedArmory`, `AB`. A space or a quote in this
    argument is a second argument to the server, not a longer name."""
    for bad in ("Storm wind", 'Storm"wind', "Stormwind;", "", "x" * 65):
        with pytest.raises(commands.CommandError):
            commands.teleport_to("Guglu", bad)


def test_a_teleport_refuses_a_name_the_server_would_not_accept() -> None:
    with pytest.raises(commands.CommandError):
        commands.teleport_to("Gu glu", "Stormwind")


# -- level ------------------------------------------------------------------


def test_a_level_change_names_the_character_and_the_level() -> None:
    assert commands.set_character_level("Guglu", 80) == "character level Guglu 80"


def test_a_level_outside_what_the_server_takes_is_refused_here() -> None:
    """1 to 255 is the command's own range; the server's configured maximum is
    its own business and it says so itself. What this refuses is the shape."""
    for bad in (0, -1, 256, 1000):
        with pytest.raises(commands.CommandError):
            commands.set_character_level("Guglu", bad)


# -- rename -----------------------------------------------------------------


def test_a_rename_marks_the_character_for_the_next_login() -> None:
    """`character rename $name` alone. The optional `reserveName` and `$newName`
    arguments are deliberately not sent: one reserves the old name server-wide
    and the other renames without asking, and neither is what a button called
    "Rename at next login" promises."""
    assert commands.rename_at_login("Guglu") == "character rename Guglu"


# -- mail -------------------------------------------------------------------


def test_mail_with_items_quotes_the_subject_and_the_body() -> None:
    """The server parses the quotes: `"#subject" "#text"` in its own help."""
    line = commands.mail_items("Guglu", subject="A gift", body="For you", items=((6948, 1),))

    assert line == 'send items Guglu "A gift" "For you" 6948:1'


def test_mail_carries_every_item_as_id_and_count() -> None:
    line = commands.mail_items(
        "Guglu", subject="Set", body="Wear it", items=((6948, 1), (2589, 20))
    )

    assert line == 'send items Guglu "Set" "Wear it" 6948:1 2589:20'


def test_a_quote_or_a_newline_in_the_text_cannot_end_the_argument() -> None:
    """A quote would close the subject early and hand the rest to the parser as
    items; a newline would end the command and start another one. Quotes are
    dropped and line breaks become spaces — replaced rather than deleted, so
    words do not glue together."""
    line = commands.mail_items(
        "Guglu",
        subject='He said "hello"',
        body="first\nsecond\r\nthird",
        items=((6948, 1),),
    )

    assert line == 'send items Guglu "He said hello" "first second  third" 6948:1'
    assert "\n" not in line and "\r" not in line


def test_a_mail_with_no_items_is_refused_rather_than_sent_empty() -> None:
    with pytest.raises(commands.CommandError):
        commands.mail_items("Guglu", subject="s", body="b", items=())


def test_a_mail_may_not_carry_more_items_than_this_tree_allows() -> None:
    """Twelve on this tree, which is what its own mail code caps an attachment
    list at. The number is passed in rather than written here, because the
    trees do not agree and 8.4c's tree allows exactly one."""
    twelve = tuple((6948, 1) for _ in range(12))
    assert commands.mail_items("Guglu", subject="s", body="b", items=twelve, cap=12)

    with pytest.raises(commands.CommandError):
        commands.mail_items("Guglu", subject="s", body="b", items=twelve + ((2589, 1),), cap=12)
    with pytest.raises(commands.CommandError):
        commands.mail_items("Guglu", subject="s", body="b", items=twelve, cap=1)


def test_an_item_count_of_zero_or_less_is_not_a_gift() -> None:
    for bad in (0, -3):
        with pytest.raises(commands.CommandError):
            commands.mail_items("Guglu", subject="s", body="b", items=((6948, bad),))


def test_mailed_money_is_sent_in_copper_because_that_is_what_the_server_counts() -> None:
    """`#money` is copper. A button that says gold multiplies by 10000 before it
    gets here, and this refuses to guess which unit it was handed."""
    line = commands.mail_money("Guglu", subject="Wages", body="Well earned", copper=50_000)

    assert line == 'send money Guglu "Wages" "Well earned" 50000'


def test_mailed_money_has_to_be_a_positive_amount_within_this_games_cap() -> None:
    for bad in (0, -1, 2_147_483_648):
        with pytest.raises(commands.CommandError):
            commands.mail_money("Guglu", subject="s", body="b", copper=bad)


# -- revive -----------------------------------------------------------------


def test_a_revive_names_the_character_even_though_the_help_does_not() -> None:
    """Measured, 2026-09-07: `.revive` documents no argument, and
    `revive NOSUCHCHARACTER` answered "Character 'Nosuchcharacter' does not
    exist." The behaviour is the fact; the help is what the core happens to
    print."""
    assert commands.revive("Guglu") == "revive Guglu"


@pytest.mark.parametrize(
    "builder",
    [
        lambda name: commands.teleport_to(name, "Stormwind"),
        lambda name: commands.set_character_level(name, 10),
        commands.rename_at_login,
        lambda name: commands.mail_items(name, subject="s", body="b", items=((1, 1),)),
        lambda name: commands.mail_money(name, subject="s", body="b", copper=1),
        commands.revive,
    ],
)
def test_every_verb_refuses_a_character_name_the_server_would_not_accept(builder) -> None:
    """One list of names, every verb, because a check that is on six code paths
    is a check that is missing from one of them.

    The last two are the ones that matter: a space makes the rest of the name a
    second argument, and a semicolon or a quote is how one command becomes two.
    """
    for bad in ("", "x" * 13, "Gu glu", "Guglu;revive Other", 'Gu"glu', "Guglu\nrevive Other"):
        with pytest.raises(commands.CommandError):
            builder(bad)
