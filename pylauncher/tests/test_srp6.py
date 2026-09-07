"""Tests for `yulon.srp6` — is this row the password we asked for? (8.3b)

A password change on the CMaNGOS trees reports a failure when it works, so the
row has to answer instead. "The row changed" is NOT that answer: any other
writer changes the row too, and a command that was refused would then be
reported as a success while the account holds somebody else's password. The
answer is the verifier itself — recomputed from the row's own salt and the
password that was asked for, and compared.

The vector below is not from a specification. It was taken off the live Vanilla
server on m910q, 2026-09-07, from an account created through this app with a
known password, and every candidate byte order was tried until one reproduced
the stored verifier exactly. Exactly one did.
"""

from __future__ import annotations

from yulon import srp6

# measured, m910q, 2026-09-07 — account SRPPROBE, password "kn0wn-p@ss77"
SALT = "E040A443299D8590D08C3353B3F9960548A9BB82B873659FC7533CA2031B485A"
VERIFIER = "27BADE411219414667B39335D71F258E191312B78C065F630954135E8870111B"


def test_the_recipe_reproduces_a_verifier_taken_off_the_live_server() -> None:
    assert srp6.verifier("mangos_srp6", "SRPPROBE", "kn0wn-p@ss77", SALT) == VERIFIER


def test_a_different_password_does_not_reproduce_it() -> None:
    """Checked on the live row too: the wrong password matched no byte order."""
    assert srp6.verifier("mangos_srp6", "SRPPROBE", "WRONG-p@ss77", SALT) != VERIFIER


def test_the_account_name_is_part_of_it_so_another_accounts_row_cannot_pass() -> None:
    assert srp6.verifier("mangos_srp6", "SOMEONEELSE", "kn0wn-p@ss77", SALT) != VERIFIER


def test_the_name_and_the_password_are_folded_the_way_the_server_folds_them() -> None:
    """Both are upper-cased before hashing, which is why a lower-case name works."""
    assert srp6.verifier("mangos_srp6", "srpprobe", "kn0wn-p@ss77", SALT) == VERIFIER
    assert srp6.verifier("mangos_srp6", "SRPPROBE", "KN0WN-P@SS77", SALT) == VERIFIER


def test_a_scheme_nobody_has_measured_answers_nothing_rather_than_a_guess() -> None:
    """The whole value of this module is that it was measured on the machine.

    A scheme that has not been is one whose password changes this app cannot
    confirm — which is a smaller answer, not a wrong one.
    """
    assert srp6.verifier("mangos_sha", "SRPPROBE", "kn0wn-p@ss77", SALT) is None
    assert srp6.verifier("", "SRPPROBE", "kn0wn-p@ss77", SALT) is None


def test_a_salt_that_is_not_hex_answers_nothing_rather_than_raising() -> None:
    """A caller reads this off a database and cannot promise what comes back."""
    assert srp6.verifier("mangos_srp6", "SRPPROBE", "kn0wn-p@ss77", "not hex") is None
    assert srp6.verifier("mangos_srp6", "SRPPROBE", "kn0wn-p@ss77", "") is None


# -- comparing it with what the database actually holds ----------------------


def test_a_stored_verifier_is_matched_however_the_server_padded_it() -> None:
    """The stored text is a big number's hex, and nobody promised its width.

    This vector's verifier happens to start with `27`, so its 64 characters say
    nothing about what the server writes for a verifier whose leading byte is
    small. A comparison that insisted on the exact text would answer "not
    confirmed" for a password change that had worked -- and this app's whole
    answer here rests on that comparison. So leading zeros and case are
    normalised on both sides, and the test says so rather than the code.
    """
    assert srp6.matches("mangos_srp6", "SRPPROBE", "kn0wn-p@ss77", SALT, VERIFIER)
    assert srp6.matches("mangos_srp6", "SRPPROBE", "kn0wn-p@ss77", SALT, VERIFIER.lower())
    assert srp6.matches("mangos_srp6", "SRPPROBE", "kn0wn-p@ss77", SALT, "0000" + VERIFIER)
    assert srp6.matches("mangos_srp6", "SRPPROBE", "kn0wn-p@ss77", SALT, f" {VERIFIER} ")


def test_the_wrong_password_and_the_unmeasured_scheme_both_answer_false() -> None:
    """Never `None` here: a caller asking "may I say this worked?" gets a no."""
    assert srp6.matches("mangos_srp6", "SRPPROBE", "WRONG-p@ss77", SALT, VERIFIER) is False
    assert srp6.matches("azerothcore", "SRPPROBE", "kn0wn-p@ss77", SALT, VERIFIER) is False
    assert srp6.matches("mangos_srp6", "SRPPROBE", "kn0wn-p@ss77", SALT, "") is False
