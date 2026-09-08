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
    assert (
        commands.teleport_to("Guglu", "Stormwind", verb="teleport name")
        == "teleport name Guglu Stormwind"
    )


def test_a_teleport_location_is_one_token_of_the_servers_own_alphabet() -> None:
    """The names come from `game_tele`, which on this install holds 1989 of them
    — `7thLegionFront`, `AbandonedArmory`, `AB`. A space or a quote in this
    argument is a second argument to the server, not a longer name."""
    for bad in ("Storm wind", 'Storm"wind', "Stormwind;", "", "x" * 65):
        with pytest.raises(commands.CommandError):
            commands.teleport_to("Guglu", bad, verb="teleport name")


def test_a_teleport_refuses_a_name_the_server_would_not_accept() -> None:
    with pytest.raises(commands.CommandError):
        commands.teleport_to("Gu glu", "Stormwind", verb="teleport name")


# -- level ------------------------------------------------------------------


def test_a_level_change_names_the_character_and_the_level() -> None:
    assert (
        commands.set_character_level("Guglu", 80, verb="character level")
        == "character level Guglu 80"
    )


def test_a_level_outside_what_the_server_takes_is_refused_here() -> None:
    """1 to 255 is the command's own range; the server's configured maximum is
    its own business and it says so itself. What this refuses is the shape."""
    for bad in (0, -1, 256, 1000):
        with pytest.raises(commands.CommandError):
            commands.set_character_level("Guglu", bad, verb="character level")


def test_a_level_cannot_be_built_without_naming_the_verb_this_tree_has() -> None:
    """8.4d, and the same argument `cap=` won in 8.4c.

    Three of these trees spell it `character level`; the tortoise fork has no
    console route to an arbitrary level AT ALL -- `.levelup` is the only command
    that writes one and its own table row sets `AllowConsole` false
    (`Chat.cpp:923`, read from this fork's source 2026-09-07). Its catalog block
    therefore carries `set_level_command: null`, and a default here would let
    that null turn back into a sibling's string on the way to the wire.
    """
    with pytest.raises(TypeError):
        commands.set_character_level("Guglu", 80)


def test_a_level_verb_nobody_measured_is_refused_rather_than_sent() -> None:
    """An empty verb means the tree has not got one, and sending
    `" Guglu 80"` would be a command whose first word is a character name."""
    for bad in ("", "   ", "character level; account delete x", "character"):
        with pytest.raises(commands.CommandError):
            commands.set_character_level("Guglu", 80, verb=bad)


# -- rename -----------------------------------------------------------------


def test_a_rename_marks_the_character_for_the_next_login() -> None:
    """`character rename $name` alone. The optional `reserveName` and `$newName`
    arguments are deliberately not sent: one reserves the old name server-wide
    and the other renames without asking, and neither is what a button called
    "Rename at next login" promises."""
    assert commands.rename_at_login("Guglu", verb="character rename") == "character rename Guglu"


def test_the_rename_verb_comes_from_the_tree_rather_than_this_file() -> None:
    """8.4d. The tortoise fork's rename is TOP-LEVEL and there is no
    `character rename` on it at all.

    Read from this fork's own source, 2026-09-07:
    `{ "rename", SEC_MODERATOR, true, &ChatHandler::HandleCharacterRenameCommand, ... }`
    at `src/game/Chat/Chat.cpp:850`, while its `characterCommandTable`
    (deleted/erase/getname/diffitems/reputation/hasitem/fillflys/clean/itemlog/
    mail/inactivity) has no rename row. So the string this app has always sent
    would arrive there as an unknown SUBcommand and answer with a list of the
    subcommands it does have -- a refusal that reads like the app being broken.
    """
    assert commands.rename_at_login("Guglu", verb="character rename") == "character rename Guglu"
    assert commands.rename_at_login("Ddsasd", verb="rename") == "rename Ddsasd"


def test_a_rename_cannot_be_built_without_naming_the_verb_this_tree_has() -> None:
    with pytest.raises(TypeError):
        commands.rename_at_login("Guglu")


def test_a_rename_verb_nobody_measured_is_refused_rather_than_sent() -> None:
    for bad in ("", "   ", "rename; account delete x", "character"):
        with pytest.raises(commands.CommandError):
            commands.rename_at_login("Guglu", verb=bad)


# -- mail -------------------------------------------------------------------


def test_mail_with_items_quotes_the_subject_and_the_body() -> None:
    """The server parses the quotes: `"#subject" "#text"` in its own help."""
    line = commands.mail_items("Guglu", subject="A gift", body="For you", items=((6948, 1),), cap=1)

    assert line == 'send items Guglu "A gift" "For you" 6948:1'


def test_mail_carries_every_item_as_id_and_count() -> None:
    line = commands.mail_items(
        "Guglu", subject="Set", body="Wear it", items=((6948, 1), (2589, 20)), cap=12
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
        cap=1,
    )

    assert line == 'send items Guglu "He said hello" "first second  third" 6948:1'
    assert "\n" not in line and "\r" not in line


def test_a_mail_with_no_items_is_refused_rather_than_sent_empty() -> None:
    with pytest.raises(commands.CommandError):
        commands.mail_items("Guglu", subject="s", body="b", items=(), cap=1)


def test_a_mail_may_not_carry_more_items_than_this_tree_allows() -> None:
    """The number is the caller's, and the same list is legal or not by it.

    Twelve is WotLK's and TBC's; one is Vanilla's, measured by 8.4c on that
    install's own `Mail.h`. Both are asserted here against one list of items,
    because a check that reads a number from anywhere but its argument would
    pass the first two lines and fail the third.
    """
    twelve = tuple((6948, 1) for _ in range(12))
    assert commands.mail_items("Guglu", subject="s", body="b", items=twelve, cap=12)

    with pytest.raises(commands.CommandError):
        commands.mail_items("Guglu", subject="s", body="b", items=twelve + ((2589, 1),), cap=12)
    with pytest.raises(commands.CommandError):
        commands.mail_items("Guglu", subject="s", body="b", items=twelve, cap=1)


def test_a_mail_cannot_be_built_without_naming_this_trees_cap() -> None:
    """8.4c. `cap` had a default of twelve, which is one tree's fact sitting in
    the module all four trees read.

    On the Vanilla install the cap is ONE (`Mail.h:49`, read on m910q
    2026-09-07), so the default did not merely widen a check — it built a line
    that server refuses, and the refusal arrives as a closed connection with no
    sentence in it (8.3b). Nothing shipped broken, because both callers in
    `yulon.play` happened to pass `cap=`; a caller that forgets is now a
    `TypeError` at the call rather than a hang-up at the server.
    """
    with pytest.raises(TypeError):
        commands.mail_items("Guglu", subject="s", body="b", items=((6948, 1),))


def test_the_refusal_counts_a_single_attachment_in_english() -> None:
    """Before 8.4c the sentence read "at most 1 items in one mail".

    No tree could reach it: a cap of one arrived with the Vanilla install, and
    this refusal is on the way to a button somebody is about to press again.
    """
    with pytest.raises(commands.CommandError) as refused:
        commands.mail_items("Guglu", subject="s", body="b", items=((6948, 1), (2589, 1)), cap=1)

    assert "at most 1 item in one mail" in str(refused.value), str(refused.value)


def test_an_item_count_of_zero_or_less_is_not_a_gift() -> None:
    for bad in (0, -3):
        with pytest.raises(commands.CommandError):
            commands.mail_items("Guglu", subject="s", body="b", items=((6948, bad),), cap=1)


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
        lambda name: commands.teleport_to(name, "Stormwind", verb="teleport name"),
        lambda name: commands.set_character_level(name, 10, verb="character level"),
        lambda name: commands.rename_at_login(name, verb="character rename"),
        lambda name: commands.mail_items(name, subject="s", body="b", items=((1, 1),), cap=1),
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


# -- 8.4a's review: what a person may type into a mail ------------------------


def test_mail_text_keeps_only_what_the_servers_parser_can_carry() -> None:
    """The review's finding, and the argument for it is the tokenizer.

    The subject and body are user text placed inside quotes the server parses.
    Dropping quotes and turning line breaks into spaces was not enough: a
    backslash may escape the closing quote in the core's own tokenizer, and a
    tab or a control character can end an argument or be refused outright.

    So this keeps what has been seen to work and drops the rest, rather than
    forbidding a list of characters somebody thought of. Printable text and
    spaces go through; everything else becomes a space, and the field is capped.
    """
    line = commands.mail_items(
        "Guglu",
        subject='a "quote" and a \\ and a \ttab',
        body="line\nbreak\x00null\x1bescape",
        items=((6948, 1),),
        cap=1,
    )

    assert '"a quote and a  and a  tab"' in line, line
    assert "\\" not in line
    assert "\t" not in line and "\x00" not in line and "\x1b" not in line


def test_mail_text_is_capped_rather_than_sent_at_any_length() -> None:
    """An unbounded argument is a command of unbounded length, and nobody has
    measured what this core does with one."""
    line = commands.mail_items(
        "Guglu", subject="s" * 500, body="b" * 5000, items=((6948, 1),), cap=1
    )

    subject = line.split('"')[1]
    body = line.split('"')[3]
    assert len(subject) <= commands.MAIL_SUBJECT_CAP
    assert len(body) <= commands.MAIL_BODY_CAP


def test_mail_text_that_is_only_unusable_characters_still_leaves_a_subject() -> None:
    """A subject that sanitised down to nothing would send `""`, and the server
    would read the body as the subject."""
    line = commands.mail_items("Guglu", subject="\x00\x01", body="\x00", items=((6948, 1),), cap=1)

    subject = line.split('"')[1]
    assert subject.strip() != "" or subject == " ", line


# -- 8.4b: the same verb is not the same word on the next tree ----------------


def test_the_teleport_verb_comes_from_the_tree_rather_than_this_file() -> None:
    """Measured on both servers, 2026-09-07.

    AzerothCore: `.teleport name [#playername] #location`.
    CMaNGOS TBC: `teleport` is not a command at all -- *"There is no such
    command"* -- and the verb is `.tele name [#playername] #location`.

    Both say "Character can be offline" in their own help, which is what makes
    this a button that works on a list. A constant here would have drawn a
    working button on one tree and a silent refusal on the other.
    """
    assert (
        commands.teleport_to("Guglu", "Stormwind", verb="teleport name")
        == "teleport name Guglu Stormwind"
    )
    assert (
        commands.teleport_to("Ddsasd", "Orgrimmar", verb="tele name")
        == "tele name Ddsasd Orgrimmar"
    )


def test_a_teleport_verb_nobody_measured_is_refused_rather_than_sent() -> None:
    """The verb is a fact from the catalog, and an empty one means the tree has
    not been measured — sending `" Guglu Stormwind"` would be a command whose
    first word is a character name."""
    for bad in ("", "   ", "tele name; account delete x"):
        with pytest.raises(commands.CommandError):
            commands.teleport_to("Guglu", "Stormwind", verb=bad)
