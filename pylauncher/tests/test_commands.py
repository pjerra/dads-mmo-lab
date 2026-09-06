"""Tests for `yulon.commands` — the text a command becomes (8.2a).

Pure. This module sends nothing and knows no container; it turns a template and
some arguments into one line, and refuses arguments the server would refuse.

The validators are the ones the prior art already derived from the bash oracle
(`rust-main:crates/dml-wow/src/soap_cmds.rs:24-40`, from
`cli/src/90-main.sh:116-138`), and the leading-dot rule is this project's answer
to an asymmetry that prior art shipped: `gm_level_cmd` emits `.character level`
WITH a dot and `gm_at_login_cmd` emits `character …` without, both pinned by
their own tests, and no comment explains why (`hypeer.md:1369-1374`).
"""

from __future__ import annotations

import pytest

from yulon import commands

# -- the leading dot --------------------------------------------------------


def test_a_template_carrying_a_leading_dot_is_the_same_command_as_one_without() -> None:
    """The console parser strips `.` and `!` before it looks anything up.

    `azerothcore.md:161-176`: `if (str[0] == '.' || str[0] == '!')` on the
    console path — while the IN-GAME handler requires one. So the dot is a
    matter of taste on this route, and the prior art shipped both spellings.
    Yu'lon's templates carry none and this is the one place that decides.
    """
    assert commands.line(".server info") == commands.line("server info") == "server info"
    assert commands.line("!server info") == "server info"


def test_a_dot_inside_a_command_is_left_alone() -> None:
    """Only the FIRST character is a prefix; `.` occurs inside real arguments."""
    assert commands.line("server set motd Hello. Welcome.") == "server set motd Hello. Welcome."


def test_the_line_is_stripped_of_surrounding_space_but_not_of_its_own() -> None:
    assert commands.line("  server info  ") == "server info"
    assert commands.line("server set motd  two  spaces") == "server set motd  two  spaces"


def test_an_empty_command_is_refused_here_rather_than_by_the_server() -> None:
    """The server answers an empty command with a sender fault (`ACSoap.cpp:109-110`).

    Refusing it here costs a round trip less and gives a better sentence.
    """
    with pytest.raises(commands.CommandError):
        commands.line("   ")


def test_a_command_carrying_a_newline_is_refused() -> None:
    """One line is one command; a second line would be a second command."""
    with pytest.raises(commands.CommandError):
        commands.line("server info\naccount delete someone")


# -- the validators ---------------------------------------------------------


@pytest.mark.parametrize("name", ["bob", "Bob_2", "a" * 20, "YULON_AB12CD34"])
def test_account_names_the_server_accepts(name: str) -> None:
    assert commands.valid_account_name(name)


@pytest.mark.parametrize("name", ["ab", "a" * 21, "bob smith", "bob;drop", "bób", ""])
def test_account_names_the_server_would_refuse(name: str) -> None:
    assert not commands.valid_account_name(name)


@pytest.mark.parametrize("password", ["abcd", "a" * 16, "p@ss_w0rd!", "a-b+c=d#e%f"])
def test_passwords_the_server_accepts(password: str) -> None:
    assert commands.valid_account_password(password)


@pytest.mark.parametrize("password", ["abc", "a" * 17, "pass word", "pass'word", ""])
def test_passwords_the_server_would_refuse(password: str) -> None:
    assert not commands.valid_account_password(password)


def test_the_password_this_app_mints_is_one_the_server_accepts() -> None:
    """The two modules agree, asserted rather than assumed.

    `channel_setup` picks the length and the alphabet from the same server rule
    these validators encode; if either drifts, every fresh install fails at the
    one moment the user has nothing to retype.
    """
    from yulon import channel_setup

    for _ in range(20):
        assert commands.valid_account_password(channel_setup.generate_password())
    assert commands.valid_account_name(channel_setup.account_name("ab12cd34"))


@pytest.mark.parametrize("name", ["Bob", "a", "a" * 12])
def test_character_names_the_server_accepts(name: str) -> None:
    assert commands.valid_character_name(name)


@pytest.mark.parametrize("name", ["a" * 13, "Bob Smith", "", "Bob;"])
def test_character_names_the_server_would_refuse(name: str) -> None:
    assert not commands.valid_character_name(name)


# -- building a command -----------------------------------------------------


def test_the_account_create_command_is_built_from_validated_parts() -> None:
    assert (
        commands.account_create("YULON_AB12CD34", "p@ssw0rd12345678")
        == "account create YULON_AB12CD34 p@ssw0rd12345678"
    )


def test_the_gm_level_command_carries_the_realm_argument_this_core_needs() -> None:
    """`-1` is "every realm", and `account_access` is realm-agnostic on the SOAP path.

    `azerothcore.md` records the level coming from `account_access.gmlevel` with
    no `RealmID` filter; the prior art's builder passes `-1` for the same reason
    (`soap_cmds.rs:95`).
    """
    assert commands.account_set_gm_level("YULON_AB12CD34", 3) == (
        "account set gmlevel YULON_AB12CD34 3 -1"
    )


def test_a_command_refuses_an_argument_the_server_would_refuse() -> None:
    """Refused here, so nothing reaches the wire that cannot succeed."""
    with pytest.raises(commands.CommandError):
        commands.account_create("no", "p@ssw0rd12345678")
    with pytest.raises(commands.CommandError):
        commands.account_create("YULON_AB12CD34", "abc")  # below the server's four-character floor
    with pytest.raises(commands.CommandError):
        commands.account_create("YULON_AB12CD34", "a" * 17)  # above its sixteen-character ceiling


def test_an_argument_cannot_smuggle_a_second_command_in() -> None:
    """The charsets are allow-lists, so a space or a semicolon never lands in a line."""
    with pytest.raises(commands.CommandError):
        commands.account_create("YULON_AB12CD34 x; account delete bob", "p@ssw0rd12345678")
