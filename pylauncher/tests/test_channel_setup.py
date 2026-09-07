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
from pathlib import Path

import pytest

from yulon import channel_setup as setup

FIXED = "2026-09-07 01:23 UTC"

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
    assert "s3cret-p4ssw0rd" not in repr(verified.credentials(host="127.0.0.1", port=7878, namespace="urn:AC"))


def test_a_pending_state_cannot_be_told_it_was_created_again() -> None:
    """The latch, from the other side: `created()` exists only on `Idle`."""
    pending = setup.Idle().created("YULON_AB12CD34", "pw")

    with pytest.raises(AttributeError):
        pending.created("YULON_AB12CD34", "another")  # type: ignore[attr-defined]


# -- the time it was proved, and repairing a credential that stopped working --


def test_a_verified_state_carries_the_time_the_round_trip_answered() -> None:
    """ "Verified" without a time cannot be told from "verified in March".

    The tab's line is the only place a user learns the channel works, and a
    channel that was proved once and has been broken since reads identically
    to one proved a minute ago unless the moment is carried.
    """
    verified = setup.Idle().created("YULON_AB12CD34", "pw").verified(now=lambda: FIXED)

    assert verified.at == FIXED


def test_the_time_is_written_beside_the_credential_and_read_back() -> None:
    """It survives the app closing, because that is when the question is asked."""
    import tempfile

    with tempfile.TemporaryDirectory() as raw:
        config = Path(raw)
        verified = setup.Idle().created("YULON_AB12CD34", "pw").verified(now=lambda: FIXED)
        setup.save_credential(
            verified,
            game="wow-wotlk",
            install_id="ab12cd34",
            host="127.0.0.1",
            port=7878,
            config_dir=config,
        )

        assert setup.verified_at("wow-wotlk", "ab12cd34", config_dir=config) == FIXED


def test_a_missing_or_unreadable_credential_has_no_time_rather_than_a_wrong_one() -> None:
    import tempfile

    with tempfile.TemporaryDirectory() as raw:
        assert setup.verified_at("wow-wotlk", "nothing", config_dir=Path(raw)) is None


def test_a_credential_the_server_rejects_reads_as_refused_not_as_verified() -> None:
    """The state the repair path starts from.

    A saved credential is written only after a round trip answered, so finding
    one is normally proof. It stops being proof the moment the account is
    changed underneath us -- somebody resets it, or the auth database is
    restored from before it existed -- and the honest answer then is refused,
    not verified.
    """
    state = setup.Verified("YULON_AB12CD34", "pw", at=FIXED)

    refused = setup.refused(state, reason="the server did not accept the saved password")

    assert isinstance(refused, setup.Refused)
    assert refused.account == "YULON_AB12CD34"
    assert "did not accept" in refused.reason


def test_repair_resets_the_password_of_the_account_it_already_has() -> None:
    """And never, ever creates a second one.

    `create` is handed in as a seam that raises: a repair that quietly minted
    `YULON_..._2` would look identical from the outside -- the channel would
    work -- while leaving a GM-level-3 account behind in the user database
    every time a credential went stale.
    """
    import tempfile

    reset: list[tuple[str, str]] = []

    def never_create(*_args: object) -> object:
        raise AssertionError("repair created an account")

    def do_reset(account: str, password: str) -> None:
        reset.append((account, password))

    with tempfile.TemporaryDirectory() as raw:
        config = Path(raw)
        state = setup.Refused("YULON_AB12CD34", "stale", reason="rejected")

        after = setup.repair(
            state=state,
            create=never_create,
            reset=do_reset,
            # Yes only to a password that is NOT the one the server refused, so a
            # repair that proves the old credential fails here instead of
            # passing on a channel that would never have answered.
            channel_for=lambda pw: _Answering("yes" if pw != "stale" else "no"),
            game="wow-wotlk",
            install_id="ab12cd34",
            host="127.0.0.1",
            port=7878,
            config_dir=config,
            now=lambda: FIXED,
        )

        assert isinstance(after, setup.Verified)
        assert after.at == FIXED
        assert len(reset) == 1
        assert reset[0][0] == "YULON_AB12CD34"
        assert reset[0][1] != "stale", "it reused the password the server just refused"
        assert setup.load_credential("wow-wotlk", "ab12cd34", config_dir=config) is not None


def test_a_repair_whose_round_trip_still_fails_saves_nothing() -> None:
    """The rule the whole module is built on, on the one path that could break it.

    The password was already reset in the database by the time the round trip
    is tried, so there is a real temptation to write it down anyway. A
    credential that has not answered is exactly what this app refuses to keep.
    """
    import tempfile

    with tempfile.TemporaryDirectory() as raw:
        config = Path(raw)
        state = setup.Refused("YULON_AB12CD34", "stale", reason="rejected")

        after = setup.repair(
            state=state,
            create=lambda *a: None,
            reset=lambda *a: None,
            channel_for=lambda _pw: _Answering("no"),
            game="wow-wotlk",
            install_id="ab12cd34",
            host="127.0.0.1",
            port=7878,
            config_dir=config,
        )

        assert not isinstance(after, setup.Verified)
        assert setup.load_credential("wow-wotlk", "ab12cd34", config_dir=config) is None


class _Answering:
    """A channel that always answers the same way."""

    def __init__(self, outcome: str) -> None:
        self.outcome = outcome

    def send(self, _command: object) -> object:
        return type("Answer", (), {"outcome": self.outcome, "text": "", "indeterminate": False})()


class _Denies:
    """A channel whose server rejects the credential -- 401, not silence."""

    def send(self, _command: object) -> object:
        return type(
            "Answer",
            (),
            {"outcome": "no", "text": "unauthorised", "indeterminate": False, "denied": True},
        )()


def test_a_credential_the_server_rejects_becomes_repairable_not_another_try(
    tmp_path: Path,
) -> None:
    """Found by 8.2c's gate on m910q, and it is a dead end, not a slow path.

    TBC's world takes minutes to load its bots, so the first three round trips
    after a Start can all miss it and the setup gives up with nothing saved --
    which is the designed behaviour and fine. What is not fine is the run after
    it: with no credential on disk the setup starts from `Idle`, generates a
    NEW password, and calls `create`, which by design keeps the password of an
    account that already exists. Every round trip from then on is a 401, and a
    401 counted as "did not answer yet" gives up again. Forever, on every run,
    with no way out but hand-written SQL.

    `Answer.denied` already tells the two apart -- it was added in 8.2a for the
    check-on-open path -- and this is the other place that has to read it. A
    rejection means the account is KNOWN, which is precisely the state the
    repair path exists for.

    WotLK hid this: its world answers inside three tries.
    """
    state = setup.Pending("YULON_AB12CD34", "generated-just-now", tries=0)

    after = setup.ensure(
        account="YULON_AB12CD34",
        password="generated-just-now",
        create=lambda *_args: None,
        channel=_Denies(),
        game="wow-tbc",
        install_id="ab12cd34",
        host="127.0.0.1",
        port=7878,
        config_dir=tmp_path,
        state=state,
    )

    assert isinstance(after, setup.Refused), after
    assert after.account == "YULON_AB12CD34"
    assert after.password == "generated-just-now", "the repair needs the password it tried"
    assert "reject" in after.reason.lower() or "not accept" in after.reason.lower(), after.reason


def test_silence_during_setup_is_still_just_another_try(tmp_path: Path) -> None:
    """The other half of the distinction, and the reason it cannot be collapsed.

    A world still loading has not rejected anything. Turning that into a refusal
    would offer the user a password reset for a server that was never asked.
    """
    state = setup.Pending("YULON_AB12CD34", "generated-just-now", tries=0)

    after = setup.ensure(
        account="YULON_AB12CD34",
        password="generated-just-now",
        create=lambda *_args: None,
        channel=_Answering("no"),
        game="wow-tbc",
        install_id="ab12cd34",
        host="127.0.0.1",
        port=7878,
        config_dir=tmp_path,
        state=state,
    )

    assert isinstance(after, setup.Pending) and after.tries == 1


def test_ensure_leaves_a_refused_credential_for_the_repair_path(tmp_path: Path) -> None:
    """A refused state must never fall into the create-or-verify machinery.

    `Refused` carries an account that exists, so the branch that treats an
    un-verified state as "try again" would call the wrong methods on it. mypy
    found this the moment `Refused` joined the union; what it costs at runtime
    is an AttributeError inside a background job.
    """

    def never_create(*_args: object) -> object:
        raise AssertionError("ensure created an account for a refused credential")

    state = setup.Refused("YULON_AB12CD34", "stale", reason="rejected")

    after = setup.ensure(
        account="YULON_AB12CD34",
        password="stale",
        create=never_create,
        channel=_Answering("yes"),
        game="wow-wotlk",
        install_id="ab12cd34",
        host="127.0.0.1",
        port=7878,
        config_dir=tmp_path,
        state=state,
    )

    assert after is state
