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
from collections.abc import Callable, Mapping
from dataclasses import dataclass, field
from pathlib import Path

from yulon import soap
from yulon.catalog import composegen
from yulon.catalog.catalog import CatalogEntry
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


# -- the enable press --------------------------------------------------------


class EnableRefused(RuntimeError):
    """The press declined. The message is written for a person to act on."""


@dataclass(frozen=True)
class Enabled:
    """What the press did, and whether it had anything to do."""

    path: Path
    changed: bool


def enable(
    entry: CatalogEntry,
    server_dir: Path,
    *,
    templates_root: Path,
    world_running: bool,
    db_password: str | None = None,
) -> Enabled:
    """Write this install's channel on, and only while the world is down.

    **Requiring the world stopped is the whole design of this step**, and it
    replaced a probe. The version before it checked the port was free and then
    wrote: check-then-act, with a window between the probe and the bind, and a
    bind that fails for reasons a probe cannot see. On the CMaNGOS trees a
    failed SOAP bind is `exit(-1)` with no character saves.

    Declining to run while there is a world to lose removes the hazard rather
    than warning about it, and it is smaller than the guard it replaced: the
    configuration is written while the server is down, and the user's ordinary
    Start brings it up through `docker.start_staged()`, which Phase 7 proved.

    Only the override is rewritten. The SOAP environment lives in that block
    rather than in the install's own so that Phase 7.1's byte-identical
    compose fixtures keep asserting what they assert.
    """
    operations = entry.operations
    if operations is None:
        raise EnableRefused(
            f"{entry.id} has no measured command channel yet, so there is nothing to turn on"
        )
    if world_running:
        raise EnableRefused(
            "the server has to be stopped before the command channel can be turned on. Stop it, "
            "press this again, then start it as usual — the setting is read when the world "
            "starts, and writing it under a running world risks the world."
        )

    target = server_dir / composegen.OVERRIDE_FILE
    before = target.read_text(encoding="utf-8") if target.exists() else ""
    plan = composegen.render(
        entry,
        server_dir,
        templates_root=templates_root,
        world_env=_world_env(entry, operations.enable_env),
        db_password=db_password,
    )
    if plan.override == before:
        logger.info(f"{entry.id}'s command channel was already switched on in {target.name}")
        return Enabled(path=target, changed=False)
    target.write_text(plan.override, encoding="utf-8", newline="\n")
    logger.info(f"wrote {entry.id}'s command channel into {target}")
    return Enabled(path=target, changed=True)


def _world_env(entry: CatalogEntry, extra: Mapping[str, str]) -> dict[str, str]:
    """What the install already had, plus what the channel needs.

    Merged rather than replaced: the bot population lives in this same block,
    and an install that lost it would be a different server from the one the
    user built.
    """
    native = entry.install.native
    entry_env = native.azerothcore.world_env if native is not None and native.azerothcore else {}
    return {**composegen.DEFAULT_WORLD_ENV, **entry_env, **extra}
