"""The app's own SOAP account: mint it, prove it, then keep it (Phase 8.2a).

Pure and seam-injected, so the whole decision tree is testable with no server
and no database. What is impure — creating the row, writing the credential file,
enabling the listener — is layered on top and hands its answers back in here.

## Everything below was learned once already

`rust-main:crates/dml-wow/src/soap_autosetup.rs` has been through this, and its
three hard-won rules are kept with their reasons:

**A create that succeeded followed by a verify that failed must not reopen the
latch.** Otherwise the next poll creates a SECOND account and the one after that
a third — one row per tick into the user's auth database, forever. That is why
this is a state machine and not a function: `Pending` carries the credential
forward and says re-verify, never re-create. Design C's write ledger flagged the
same file as the hazard (`soap_autosetup.rs:116-120`) and named this as the
defence.

**Sixteen characters is a ceiling, not a preference.** AzerothCore validates
`{4,16}` before it writes anything, so a "stronger" 32-character password is
refused on every fresh install — at the one moment the user has nothing to
retype.

**Rejection sampling, not `byte % 70`.** 256 is not a multiple of the alphabet's
70 symbols (`256 = 3*70 + 46`), so plain modulo hands the first 46 symbols a
fourth chance the other 24 never get. The bias is invisible in any output a
person would look at, which is exactly why it belongs in code rather than in a
review.

## What this module adds to that

The account's **name is derived from the install id**, so two installs of one
game on one machine get different accounts and the same install always gets the
same one. That is what makes "create" idempotent: a second run finds the row it
made last time instead of writing another.

And only `Verified` can produce credentials. The architecture's rule for this
module — never persist before a round trip has answered — is expressed in the
types rather than in a comment: the earlier states have no such method, so a
caller cannot write a credential file it has not proved.
"""

from __future__ import annotations

import secrets
from collections.abc import Callable
from dataclasses import dataclass, field

from yulon import soap
from yulon.log import get_logger

logger = get_logger(__name__)

PASSWORD_LENGTH = 16
"""AzerothCore's own ceiling for an account password, not a preference here."""

PASSWORD_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_@#%+=!-"
"""Exactly the characters account creation accepts: 26 + 26 + 10 + 8 = 70."""

MAX_VERIFY_TRIES = 3
"""Attempts a created-but-unverified account gets before the user is asked.

Verify can fail after a successful create for one interesting reason — the
credential is well formed and the server rejects it — and one boring one: the
world went away between the two calls. Three tries tells them apart without
spinning.
"""

ACCOUNT_PREFIX = "YULON_"


def generate_password(fill: Callable[[int], bytes] = secrets.token_bytes, size: int = 64) -> str:
    """A fresh password, unbiased over `PASSWORD_ALPHABET`.

    `fill` is a seam so the discard rule can be proved with a scripted byte
    stream; production passes `secrets.token_bytes`.
    """
    alphabet = PASSWORD_ALPHABET
    n = len(alphabet)
    limit = (256 // n) * n  # 210: the largest multiple of n inside a byte
    out: list[str] = []
    while len(out) < PASSWORD_LENGTH:
        for byte in fill(size):
            if byte < limit:
                out.append(alphabet[byte % n])
                if len(out) == PASSWORD_LENGTH:
                    break
    return "".join(out)


def account_name(install_id: str) -> str:
    """This install's account name — stable for it, different for its neighbour."""
    return (ACCOUNT_PREFIX + install_id).upper()


@dataclass(frozen=True)
class Idle:
    """Nothing has been created yet."""

    def created(self, account: str, password: str) -> Pending:
        """Record a row that now exists, un-proved."""
        return Pending(account=account, password=password, tries=0)


@dataclass(frozen=True)
class Pending:
    """The row exists and the round trip has not answered yet.

    Deliberately has no `created()`: once a row exists, the only ways forward
    are to prove it or to give up. That is the latch.
    """

    account: str
    password: str = field(repr=False)
    tries: int = 0

    def verify_failed(self) -> Pending | GaveUp:
        """One more try, or hand over to the user — never another account."""
        tries = self.tries + 1
        if tries >= MAX_VERIFY_TRIES:
            return GaveUp(
                account=self.account,
                reason=(
                    f"the account {self.account} exists but three round trips did not prove it, "
                    "so nothing was saved and it is over to you"
                ),
            )
        return Pending(account=self.account, password=self.password, tries=tries)

    def verified(self) -> Verified:
        return Verified(account=self.account, password=self.password)


@dataclass(frozen=True)
class Verified:
    """The account exists AND a real round trip has answered through it."""

    account: str
    password: str = field(repr=False)

    def credentials(self, *, host: str, port: int) -> soap.Endpoint:
        """The endpoint to persist. Only reachable from here, by design."""
        return soap.Endpoint(host=host, port=port, account=self.account, password=self.password)


@dataclass(frozen=True)
class GaveUp:
    """This run will not try again; the reason is for a person to read."""

    account: str
    reason: str


State = Idle | Pending | Verified | GaveUp
