"""Tests for `yulon.passwordcheck` — is this row the password we asked for? (8.3b)

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

from yulon import passwordcheck

# measured, m910q, 2026-09-07 — account SRPPROBE, password "kn0wn-p@ss77"
SALT = "E040A443299D8590D08C3353B3F9960548A9BB82B873659FC7533CA2031B485A"
VERIFIER = "27BADE411219414667B39335D71F258E191312B78C065F630954135E8870111B"


def _srp(account: str, password: str, salt: str = SALT, stored: str = VERIFIER) -> bool:
    return passwordcheck.matches("mangos_srp6", account, password, (salt, stored))


def test_the_recipe_reproduces_a_verifier_taken_off_the_live_server() -> None:
    assert _srp("SRPPROBE", "kn0wn-p@ss77") is True


def test_a_different_password_does_not_reproduce_it() -> None:
    """Checked on the live row too: the wrong password matched no byte order."""
    assert _srp("SRPPROBE", "WRONG-p@ss77") is False


def test_the_account_name_is_part_of_it_so_another_accounts_row_cannot_pass() -> None:
    assert _srp("SOMEONEELSE", "kn0wn-p@ss77") is False


def test_the_name_and_the_password_are_folded_the_way_the_server_folds_them() -> None:
    """Both are upper-cased before hashing, which is why a lower-case name works."""
    assert _srp("srpprobe", "kn0wn-p@ss77") is True
    assert _srp("SRPPROBE", "KN0WN-P@SS77") is True


def test_a_scheme_nobody_has_measured_answers_nothing_rather_than_a_guess() -> None:
    """The whole value of this module is that it was measured on the machine.

    A scheme that has not been is one whose password changes this app cannot
    confirm — which is a smaller answer, not a wrong one.
    """
    pair = (SALT, VERIFIER)
    assert passwordcheck.matches("azerothcore", "SRPPROBE", "kn0wn-p@ss77", pair) is False
    assert passwordcheck.matches("", "SRPPROBE", "kn0wn-p@ss77", pair) is False


def test_a_salt_that_is_not_hex_answers_nothing_rather_than_raising() -> None:
    """A caller reads this off a database and cannot promise what comes back."""
    assert _srp("SRPPROBE", "kn0wn-p@ss77", salt="not hex") is False
    assert _srp("SRPPROBE", "kn0wn-p@ss77", salt="") is False


def test_a_row_with_the_wrong_number_of_columns_in_it_is_not_an_answer() -> None:
    """One scheme has two columns and the other has one, so the count is a fact
    about the scheme rather than about whatever came back from the database."""
    assert passwordcheck.matches("mangos_srp6", "SRPPROBE", "kn0wn-p@ss77", (VERIFIER,)) is False
    assert passwordcheck.matches("mangos_sha", "SHAPROBE", "x", (SALT, VERIFIER)) is False


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
    assert _srp("SRPPROBE", "kn0wn-p@ss77", stored=VERIFIER)
    assert _srp("SRPPROBE", "kn0wn-p@ss77", stored=VERIFIER.lower())
    assert _srp("SRPPROBE", "kn0wn-p@ss77", stored="0000" + VERIFIER)
    assert _srp("SRPPROBE", "kn0wn-p@ss77", stored=f" {VERIFIER} ")


def test_the_wrong_password_and_the_unmeasured_scheme_both_answer_false() -> None:
    """Never `None` here: a caller asking "may I say this worked?" gets a no."""
    assert _srp("SRPPROBE", "WRONG-p@ss77") is False
    assert (
        passwordcheck.matches("azerothcore", "SRPPROBE", "kn0wn-p@ss77", (SALT, VERIFIER)) is False
    )
    assert _srp("SRPPROBE", "kn0wn-p@ss77", stored="") is False


# -- the other tree, and the other shape ------------------------------------

# measured, m910q, 2026-09-07 -- account SHAPROBE on the Tortoise fork. The
# first is what the app wrote for the password it was given; the second is what
# the SERVER wrote into the same column after `account set password`.
SHA_KNOWN = "B83C39DFF7DCC47570C17ADF657989306814F0E6"
SHA_AFTER_THE_SERVER_CHANGED_IT = "229128ADF72BFF11D4708CE859E58DAEC5CAF1E8"


def test_the_tortoise_fork_keeps_one_unsalted_hash_and_this_is_it() -> None:
    """A different shape, not a different byte order: one column, no salt.

    Both rows below are real, and the second matters more than the first: it is
    what the server itself wrote when IT changed the password, which is the only
    thing a check on this column can honestly rest on. `v` and `s` went to `0`
    in that same write, so this column is the one that decides.
    """
    assert passwordcheck.matches("mangos_sha", "SHAPROBE", "kn0wn-p@ss77", (SHA_KNOWN,))
    assert passwordcheck.matches(
        "mangos_sha", "SHAPROBE", "tru7h-p@ss99", (SHA_AFTER_THE_SERVER_CHANGED_IT,)
    )


def test_the_tortoise_hash_is_not_reproduced_by_the_wrong_password_or_name() -> None:
    assert passwordcheck.matches("mangos_sha", "SHAPROBE", "WRONG-p@ss77", (SHA_KNOWN,)) is False
    assert passwordcheck.matches("mangos_sha", "SOMEONE", "kn0wn-p@ss77", (SHA_KNOWN,)) is False


def test_the_two_trees_do_not_answer_for_each_other() -> None:
    """The scheme comes from the catalog, and a wrong one must not pass.

    Not hypothetical: `wow-tortoise`'s account row carries `v` and `s` columns
    TOO, so a check that reached for the SRP6 pair on this tree would find
    something to read and would answer no forever.
    """
    assert passwordcheck.matches("mangos_srp6", "SHAPROBE", "kn0wn-p@ss77", (SHA_KNOWN,)) is False
    assert passwordcheck.matches("mangos_sha", "SRPPROBE", "kn0wn-p@ss77", (VERIFIER,)) is False
