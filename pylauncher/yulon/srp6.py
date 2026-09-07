"""Is this account's stored credential the password we asked for?

The CMaNGOS trees report a successful `account set password` as a failure --
deliberately, "to avoid normal report for hide passwords" (`Level3.cpp:1178-1183`
on TBC, `:1139-1145` on Vanilla) -- so the reply cannot say what happened and the
row has to. **"The row changed" is not that answer.** Any other writer changes
the row too: a second window, an administrator at a console, a retry. A command
that was refused would then be reported as a success while the account holds
somebody else's password, and the person who asked would be locked out by the
reassurance. That was 8.3b's adversarial review, and it was right.

What answers is the verifier. SRP6 stores `g^H(s, H(USER:PASS)) mod N`, so
recomputing it from the row's own salt and the password that was asked for says
whether THAT password is the one this account now has -- whoever wrote it, and
whatever the server said.

**The recipe is measured, not read off a specification.** An account was created
through this app with a known password on the live Vanilla server (m910q,
2026-09-07), its `s` and `v` read back, and every candidate byte order computed
until one reproduced the stored verifier. Exactly one did, and the wrong password
reproduced none. A scheme that has not been through that is absent from the table
below, and this module answers `None` for it: a password change this app cannot
confirm is a smaller answer, not a wrong one.
"""

from __future__ import annotations

import hashlib

_N = int("894B645E89E1535BBDAD5B8B290650530801B18EBFBF5E8FAB3C82872A3E9BB7", 16)
"""The SRP6 modulus every WoW auth server has used since 2004."""

_G = 7

MEASURED_SCHEMES = ("mangos_srp6",)
"""Schemes whose stored credential this module can reproduce.

`azerothcore` is deliberately absent: that tree answers its own commands
properly, so nothing there needs a row to be believed, and adding a recipe
nobody has run against a live AzerothCore row would be a guess wearing a
measurement's clothes.
"""


def verifier(scheme: str, account: str, password: str, salt: str) -> str | None:
    """The verifier this account and password produce, or `None`.

    `None` where the scheme has not been measured, or the salt is not the hex
    the database is supposed to hold -- both are absences rather than failures,
    because the caller reads that salt off a live server and cannot promise
    what comes back.

    The name and the password are upper-cased before hashing, which is the
    server's own rule and the reason a lower-case account name still logs in.
    """
    if scheme not in MEASURED_SCHEMES:
        return None
    try:
        salt_bytes = bytes.fromhex(salt.strip())
    except ValueError:
        return None
    if not salt_bytes:
        return None
    identity = hashlib.sha1(f"{account.upper()}:{password.upper()}".encode()).digest()
    # The salt is stored big-endian text of a little-endian number, so it is
    # reversed before hashing and the digest is read back little-endian. Those
    # two are the measurement: three of the four other combinations reproduced
    # nothing, and the fourth reproduced it only for the wrong password.
    x = int.from_bytes(hashlib.sha1(salt_bytes[::-1] + identity).digest(), "little")
    return f"{pow(_G, x, _N):X}".rjust(2, "0")


def matches(scheme: str, account: str, password: str, salt: str, stored: str) -> bool:
    """Does this account's stored verifier belong to this password?

    A plain `==` on the text would be wrong twice over: the stored value is a
    big number's hex, whose width nobody promised, and the case is the writer's
    choice. Both sides are normalised. A scheme with no measured recipe answers
    `False` -- the question is "may this app tell somebody their password
    changed?", and the answer to that is no whenever it cannot be shown.
    """
    expected = verifier(scheme, account, password, salt)
    if expected is None or not stored.strip():
        return False
    return stored.strip().upper().lstrip("0") == expected.upper().lstrip("0")
