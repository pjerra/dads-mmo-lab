"""Tests for `yulon.channel_setup` — the app's own SOAP account (8.2a).

Every rule here was taken from `rust-main:crates/dml-wow/src/soap_autosetup.rs`,
which has been through this once already, and each carries the reason that
module records. Design C's write ledger flagged the same file as the hazard
(`soap_autosetup.rs:116-120`) and named the defence: the account's name is a key
derived per install, so a second run finds the first row instead of writing
another.
"""

from __future__ import annotations

import string

import pytest

from yulon import channel_setup as setup

# -- the password -----------------------------------------------------------


def test_the_password_is_sixteen_characters_because_the_server_refuses_longer() -> None:
    """A ceiling, not a preference.

    AzerothCore validates `{4,16}` before it writes anything, so a "stronger"
    32-character password is refused on every fresh install — at the one moment
    the user has nothing to retype.
    """
    assert len(setup.generate_password()) == setup.PASSWORD_LENGTH
    assert setup.PASSWORD_LENGTH == 16


def test_the_password_uses_only_characters_that_account_creation_accepts() -> None:
    allowed = set(setup.PASSWORD_ALPHABET)
    assert allowed <= set(string.ascii_letters + string.digits + "_@#%+=!-")
    for _ in range(20):
        assert set(setup.generate_password()) <= allowed


def test_the_password_discards_biased_bytes_rather_than_folding_them() -> None:
    """`byte % 70` is the obvious way and it is skewed.

    256 is not a multiple of 70 (256 = 3x70 + 46), so plain modulo hands the
    first 46 symbols a fourth chance the last 24 never get. The bias is
    invisible in any output a human would look at, which is why it is tested
    rather than eyeballed: a byte at or above 210 must be thrown away, so a
    stream of 210s produces nothing at all until it changes.
    """
    n = len(setup.PASSWORD_ALPHABET)
    limit = (256 // n) * n
    assert limit == 210

    stream = iter([limit, limit + 1, 255, 0, 1, 2] + [3] * 100)

    def fill(size: int) -> bytes:
        return bytes(next(stream) for _ in range(size))

    password = setup.generate_password(fill=fill, size=1)

    # The three rejected bytes contributed nothing; the first kept byte is 0.
    assert password[0] == setup.PASSWORD_ALPHABET[0]
    assert password[1] == setup.PASSWORD_ALPHABET[1]
    assert password[2] == setup.PASSWORD_ALPHABET[2]


def test_two_passwords_are_not_the_same() -> None:
    assert setup.generate_password() != setup.generate_password()


# -- the account name -------------------------------------------------------


def test_the_account_name_is_derived_from_the_install_so_a_second_run_finds_it() -> None:
    """The defence against writing one account row per attempt.

    Two installs of one game on one machine get different names; the same
    install always gets the same one, so "create" can find the row it made last
    time instead of adding another.
    """
    first = setup.account_name("ab12cd34")
    assert first == setup.account_name("ab12cd34")
    assert first != setup.account_name("99999999")
    assert first.upper() == first, "account names are compared upper-cased on this core"


# -- the state machine ------------------------------------------------------


def test_a_verify_that_fails_after_a_create_never_creates_a_second_account() -> None:
    """The trap this is a state machine to avoid, in the prior art's own words.

    "A create that succeeded followed by a verify that failed must not leave the
    latch open: the next poll would create a SECOND account, and the one after
    that a third — one row per tick into the user's auth database, forever."
    """
    state = setup.Idle()

    after_create = state.created("YULON_AB12CD34", "hunter2hunter22")
    assert isinstance(after_create, setup.Pending)

    after_failure = after_create.verify_failed()

    assert isinstance(after_failure, setup.Pending), "a failed verify must not go back to Idle"
    assert after_failure.account == "YULON_AB12CD34"
    assert after_failure.password == "hunter2hunter22", "the credential is carried, not re-minted"
    assert after_failure.tries == 1


def test_it_gives_up_after_three_tries_rather_than_spinning() -> None:
    """Three tells a bad verifier apart from a world that went away, without a loop."""
    state: setup.State = setup.Idle().created("YULON_AB12CD34", "pw")
    for _ in range(setup.MAX_VERIFY_TRIES):
        assert isinstance(state, setup.Pending)
        state = state.verify_failed()

    assert isinstance(state, setup.GaveUp)
    assert "three" in state.reason or "3" in state.reason


def test_a_verified_account_is_the_only_thing_that_reaches_done() -> None:
    state = setup.Idle().created("YULON_AB12CD34", "pw").verified()

    assert isinstance(state, setup.Verified)
    assert state.account == "YULON_AB12CD34"


def test_nothing_is_persisted_until_the_round_trip_has_answered() -> None:
    """The architecture's rule for this module, asserted on the type rather than the prose.

    Only `Verified` carries what a credential file would be written from; the
    states before it deliberately have no such method, so "persist before the
    round trip answered" is not something a caller can express.
    """
    pending = setup.Idle().created("YULON_AB12CD34", "pw")

    assert hasattr(setup.Verified("YULON_AB12CD34", "pw"), "credentials")
    assert not hasattr(pending, "credentials")
    assert not hasattr(setup.Idle(), "credentials")


def test_the_credential_a_verified_state_yields_keeps_the_password_out_of_its_repr() -> None:
    verified = setup.Idle().created("YULON_AB12CD34", "s3cret-p4ssw0rd").verified()

    assert "s3cret-p4ssw0rd" not in repr(verified)
    assert "s3cret-p4ssw0rd" not in repr(verified.credentials(host="127.0.0.1", port=7878))


def test_a_pending_state_cannot_be_told_it_was_created_again() -> None:
    """The latch, from the other side: `created()` exists only on `Idle`."""
    pending = setup.Idle().created("YULON_AB12CD34", "pw")

    with pytest.raises(AttributeError):
        pending.created("YULON_AB12CD34", "another")  # type: ignore[attr-defined]
